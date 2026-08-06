// Phase 0: candle + Qwen3 pipeline smoke test.
// Validates the qwen3 model can be loaded, hidden states extracted for
// embedding, and logits extracted for reranker scoring.

use candle_core::{Device, DType, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::qwen3::{Config, Model, ModelForCausalLM};
use std::collections::HashMap;
use std::path::PathBuf;
use tokenizers::Tokenizer;

fn main() {
    let model_dir = PathBuf::from("/tmp/rag_test/models/embedding");
    let reranker_dir = PathBuf::from("/tmp/rag_test/models/reranker");

    println!("=== Phase 0: RAG Pipeline Smoke Test ===\n");

    // ── 1. Config and tokenizer ──
    println!("[1] Loading config & tokenizer...");
    let config: Config = serde_json::from_str(
        &std::fs::read_to_string(model_dir.join("config.json")).unwrap(),
    )
    .unwrap();
    println!("    Config: {} layers, {} heads, hidden={}, vocab={}",
        config.num_hidden_layers, config.num_attention_heads,
        config.hidden_size, config.vocab_size);

    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).unwrap();
    let tokens = tokenizer.encode("Hello world", true).unwrap();
    assert!(tokens.len() > 0);
    println!("    Tokenizer: \"Hello world\" → {} tokens\n", tokens.len());

    // ── 2. Model availability check ──
    let model_path = model_dir.join("model.safetensors");
    let reranker_path = reranker_dir.join("model.safetensors");

    if !model_path.exists() {
        println!("[2] ⚠ Embedding model not found at {:?}", model_path);
        println!("    Download:\n      wget -O {:?} \\\n        https://huggingface.co/Qwen/Qwen3-Embedding-0.6B/resolve/main/model.safetensors",
            model_path);
        return;
    }
    println!("[2] Embedding model found\n");

    // ── 3. Load Model (hidden states for embedding) ──
    println!("[3] Loading Model (hidden states)...");
    let device = Device::Cpu;
    let dtype = DType::F32;

    // Build VarBuilder: rename tensors to match candle's expected naming
    let tensor_map = load_and_rename_tensors(&model_path, dtype, &device);
    let vb = VarBuilder::from_tensors(tensor_map, dtype, &device);
    println!("    ✓ Model construction works\n");

    // ── Embedding test ──
    // Create a FRESH Model per forward (Model's clear_kv_cache is crate-private,
    // so we can't reset the attention KV cache between calls). The HashMap
    // VarBuilder is just a reference-counted wrapper, so constructing is cheap.
    let make_model = || Model::new(&config, vb.clone()).unwrap();
    let texts: [&str; 3] = [
        "What is machine learning?",
        "Explain ML algorithms and models",
        "How to bake a chocolate cake",
    ];

    let embeds: Vec<Vec<f32>> = texts.iter()
        .map(|t| get_embedding(&mut make_model(), &tokenizer, t))
        .collect();

    for (i, (t, e)) in texts.iter().zip(&embeds).enumerate() {
        println!("    embed[{}] \"{}\" → dim={}, peek=[{:.4}, {:.4}, {:.4}, {:.4}]",
            i, t, e.len(), e[0], e[1], e[2], e[3]);
    }
    assert_eq!(embeds[0].len(), config.hidden_size as usize);
    assert_eq!(embeds[1].len(), config.hidden_size as usize);
    assert_eq!(embeds[2].len(), config.hidden_size as usize);

    let cos_12: f32 = embeds[0].iter().zip(&embeds[1]).map(|(a, b)| a * b).sum();
    let cos_13: f32 = embeds[0].iter().zip(&embeds[2]).map(|(a, b)| a * b).sum();
    println!("    cos(ML, ML-related) = {:.4}", cos_12);
    println!("    cos(ML, unrelated)  = {:.4}", cos_13);
    assert!(cos_12 > cos_13,
        "similar docs should have higher cosine (got {} vs {})", cos_12, cos_13);
    println!("    ✓ Embedding: last-token + L2 normalize works\n");

    // ── 4. ModelForCausalLM (logits → reranker) ──
    println!("[4] Loading ModelForCausalLM (logits)...");
    let mut causal = ModelForCausalLM::new(&config, vb).unwrap();
    println!("    ✓ ModelForCausalLM constructed\n");

    let query = "machine learning basics";
    let doc_rel = "Machine learning is a subfield of artificial intelligence where computers learn from data...";
    let doc_irr = "This cake recipe requires flour, eggs, and sugar mixed in a bowl...";

    let score_rel = rerank_score(&mut causal, &tokenizer, query, doc_rel);
    let score_irr = rerank_score(&mut causal, &tokenizer, query, doc_irr);
    println!("    rerank(relevant)   = {:.4}", score_rel);
    println!("    rerank(irrelevant) = {:.4}", score_irr);
    assert!(score_rel.is_finite() && score_irr.is_finite());
    assert!((0.0..=1.0).contains(&score_rel));
    assert!((0.0..=1.0).contains(&score_irr));
    println!("    ✓ Reranker: yes/no logit extraction works\n");

    // ── 5. Reranker model (dedicated weights if available) ──
    if reranker_path.exists() {
        let tensor_map_r = load_and_rename_tensors(&reranker_path, dtype, &device);
        let vb_r = VarBuilder::from_tensors(tensor_map_r, dtype, &device);
        let make_reranker = || ModelForCausalLM::new(&config, vb_r.clone()).unwrap();

        // Fresh model per call to avoid KV-cache carry-over
        let score_rel_r = rerank_score(&mut make_reranker(), &tokenizer, query, doc_rel);
        let score_irr_r = rerank_score(&mut make_reranker(), &tokenizer, query, doc_irr);
        println!("[5] Reranker fine-tuned:\n    rerank(relevant)   = {:.4}\n    rerank(irrelevant) = {:.4}",
            score_rel_r, score_irr_r);
        assert!(score_rel_r > score_irr_r,
            "reranker: relevant > irrelevant (got {} vs {})", score_rel_r, score_irr_r);
        println!("    ✓ Reranker fine-tuned ordering correct\n");
    } else {
        println!("[5] ⚠ Reranker weights not found (optional)\n");
    }

    println!("=== All Phase 0 checks passed! ===");
}

fn load_and_rename_tensors(
    path: &std::path::Path,
    _dtype: DType,
    device: &Device,
) -> HashMap<String, Tensor> {
    use candle_core::safetensors;
    let tensors = safetensors::load(path, device).unwrap();
    let mut map = HashMap::new();
    for (name, tensor) in tensors {
        // Qwen3-Embedding safetensors: bare names (embed_tokens.weight)
        // Qwen3-Reranker safetensors: already prefixed (model.embed_tokens.weight)
        // candle Qwen3 model code looks up model.embed_tokens.weight etc.
        let new_name = if name.starts_with("model.") || name == "lm_head.weight" {
            name
        } else {
            format!("model.{}", name)
        };
        map.insert(new_name, tensor);
    }
    map
}

fn get_embedding(
    model: &mut Model,
    tokenizer: &Tokenizer,
    text: &str,
) -> Vec<f32> {
    let tokens = tokenizer.encode(text, true).unwrap();
    let input = Tensor::from_slice(tokens.get_ids(), (1, tokens.len()), &Device::Cpu).unwrap();
    let hidden = model.forward(&input, 0).unwrap();
    let seq_len = hidden.dims()[1];
    let last = hidden.narrow(1, seq_len - 1, 1).unwrap()   // (1, 1, 1024)
        .squeeze(1).unwrap()                                // (1, 1024)
        .squeeze(0).unwrap();                                // (1024,)
    // L2 normalize: divide by sqrt of sum of squares
    let vec_1d: Vec<f32> = last.to_vec1().unwrap();
    let sum_sq: f32 = vec_1d.iter().map(|x| x * x).sum();
    let norm = sum_sq.sqrt();
    vec_1d.into_iter().map(|x| x / norm).collect()
}

fn rerank_score(
    model: &mut ModelForCausalLM,
    tokenizer: &Tokenizer,
    query: &str,
    document: &str,
) -> f32 {
    // Use the default instruction from Qwen3-Reranker chat_template.jinja
    let prompt = format!(
        "<|im_start|>system\n\
         Judge whether the Document meets the requirements based on the \
         Query and the Instruct provided. Note that the answer can only \
         be \"yes\" or \"no\".<|im_end|>\n\
         <|im_start|>user\n\
         <Instruct>: Given a web search query, retrieve relevant passages that answer the query\n\
         <Query>: {query}\n\
         <Document>: {document}<|im_end|>\n\
         <|im_start|>assistant\n\
          thinking\n\
         \n\
          response\n\
         \n"
    );
    // Note: the prompt ends right before the yes/no token prediction.
    // The model's next-token logits at the last position encode the relevance decision.
    let tokens = tokenizer.encode(prompt.as_str(), false).unwrap();
    let input = Tensor::from_slice(tokens.get_ids(), (1, tokens.len()), &Device::Cpu).unwrap();
    let logits = model.forward(&input, 0).unwrap();                    // (1, 1, 151669)
    let logits = logits.squeeze(1).unwrap();                            // (1, 151669)
    let yes = logits.narrow(1, 9693, 1).unwrap()
        .squeeze(1).unwrap()
        .squeeze(0).unwrap()
        .to_scalar::<f32>().unwrap();
    let no  = logits.narrow(1, 2152, 1).unwrap()
        .squeeze(1).unwrap()
        .squeeze(0).unwrap()
        .to_scalar::<f32>().unwrap();
    // softmax: p(yes) = 1 / (1 + exp(no - yes))
    1.0 / (1.0 + (no - yes).exp())
}