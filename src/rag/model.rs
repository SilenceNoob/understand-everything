use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::qwen3::{self, Model, ModelForCausalLM};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tokenizers::Tokenizer;

use crate::util::app_base_dir;

/// Model dirs under <app_base_dir>/models/, and their HF repos.
pub const MODEL_SPECS: &[(&str, &str)] = &[
    ("embedding", "Qwen/Qwen3-Embedding-0.6B"),
    ("reranker", "Qwen/Qwen3-Reranker-0.6B"),
];
const MODEL_FILES: &[&str] = &["config.json", "tokenizer.json", "model.safetensors"];
/// Default HF mirror for regions where huggingface.co is unreachable; users
/// can override by exporting HF_ENDPOINT themselves.
const HF_MIRROR: &str = "https://hf-mirror.com";

/// Query-side instruction prefix (Qwen3-Embedding convention); document side
/// is embedded as plain text.
pub const EMBED_QUERY_PREFIX: &str = "Instruct: Given a question, retrieve relevant passages\nQuery: ";
/// yes/no token ids from the reranker's vocab (verified by rag_smoke).
const YES_ID: u32 = 9693;
const NO_ID: u32 = 2152;
/// Embedding truncation cap; chunks are already <= this, only oversize query
/// prefixes or rerank docs hit it.
const MAX_TOKENS: usize = 512;

#[derive(Clone, Debug, PartialEq)]
pub enum ModelStatus {
    Downloading(String),
    Loading,
    Ready,
    Failed(String),
}

/// Reranker prompt, byte-identical to the one validated in rag_smoke (the
/// trailing thinking/response scaffolding is part of the official template).
pub fn rerank_prompt(query: &str, document: &str) -> String {
    format!(
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
    )
}

struct Bundle {
    config: qwen3::Config,
    tokenizer: Tokenizer,
    vb: VarBuilder<'static>,
}

/// Both models. `embed`/`rerank` construct a fresh Model per forward (candle
/// qwen3's KV cache cannot be reset between calls); the tensor map is
/// refcounted so this is cheap. Bundles are loaded into RwLocks so the
/// status is visible before loading completes; the embedding loads in
/// `ensure`, the reranker in `warm_reranker` (startup, ~2.3GB resident) with
/// the lazy path in `rerank` kept as a fallback.
pub struct Models {
    embedding: RwLock<Option<Bundle>>,
    reranker: RwLock<Option<Bundle>>,
    pub status: Arc<RwLock<ModelStatus>>,
}

impl Default for Models {
    fn default() -> Self {
        Self::new()
    }
}

impl Models {
    pub fn new() -> Models {
        Models {
            embedding: RwLock::new(None),
            reranker: RwLock::new(None),
            status: Arc::new(RwLock::new(ModelStatus::Loading)),
        }
    }

    /// Blocking: download any missing files (HF_ENDPOINT-aware), load the
    /// embedding model. Progress lands in `status` for the UI; call on a
    /// worker thread. The reranker is loaded by `warm_reranker` right after
    /// (see service.rs). Failures are per-model: status goes Failed but the
    /// other model still works.
    pub fn ensure(&self, app_dir: &Path) {
        let mut failures = Vec::new();
        for &(name, repo) in MODEL_SPECS {
            let dir = app_dir.join("models").join(name);
            if !bundle_files_present(&dir) {
                *self.status.write().unwrap() = ModelStatus::Downloading(format!("{name}"));
                if let Err(dl) = download_bundle(&dir, repo) {
                    failures.push(format!("{name} download: {dl}"));
                    continue;
                }
            }
            if name == "embedding" {
                *self.status.write().unwrap() = ModelStatus::Loading;
                match load_bundle(&dir) {
                    Ok(b) => *self.embedding.write().unwrap() = Some(b),
                    Err(e) => failures.push(format!("embedding: {e}")),
                }
            }
        }
        if failures.is_empty() {
            *self.status.write().unwrap() = ModelStatus::Ready;
        } else {
            *self.status.write().unwrap() = ModelStatus::Failed(failures.join("; "));
        }
    }

    pub fn embedding_ready(&self) -> bool {
        self.embedding.read().unwrap().is_some()
    }

    /// Load the reranker bundle without running a forward, moving the ~2.3GB
    /// load off the first interactive rerank. No-op when already loaded.
    pub fn warm_reranker(&self) {
        let mut guard = self.reranker.write().unwrap();
        if guard.is_none() {
            let dir = app_base_dir().join("models").join("reranker");
            *guard = load_bundle(&dir).ok();
        }
    }

    /// Query/文档 embedding: instruction prefix for queries, last-token
    /// hidden state, L2-normalized. Encode with special tokens so the
    /// tokenizer's post-processor appends <|endoftext|> (the EOS position is
    /// the pooling target).
    pub fn embed(&self, text: &str, is_query: bool) -> Result<Vec<f32>, String> {
        let guard = self.embedding.read().unwrap();
        let b = guard.as_ref().ok_or("embedding model not ready")?;
        let s = if is_query {
            format!("{EMBED_QUERY_PREFIX}{text}")
        } else {
            text.to_string()
        };
        let enc = b.tokenizer.encode(s.as_str(), true).map_err(|e| e.to_string())?;
        let ids: Vec<u32> = enc.get_ids().iter().take(MAX_TOKENS).copied().collect();
        if ids.is_empty() {
            return Err("empty input".into());
        }
        let input = Tensor::from_slice(&ids, (1, ids.len()), &Device::Cpu).map_err(|e| e.to_string())?;
        let mut model = Model::new(&b.config, b.vb.clone()).map_err(|e| e.to_string())?;
        let hidden = model.forward(&input, 0).map_err(|e| e.to_string())?;
        let seq = hidden.dims()[1];
        let last = hidden
            .narrow(1, seq - 1, 1)
            .map_err(|e| e.to_string())?
            .squeeze(1)
            .map_err(|e| e.to_string())?
            .squeeze(0)
            .map_err(|e| e.to_string())?;
        let vec: Vec<f32> = last.to_vec1().map_err(|e| e.to_string())?;
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm == 0.0 {
            return Err("zero norm".into());
        }
        Ok(vec.into_iter().map(|x| x / norm).collect())
    }

    /// Rerank score p(yes) via the official template; encode WITHOUT special
    /// tokens (they are literal in the prompt; adding EOS shifts the logit
    /// position and collapses scores). Lazy-loads the reranker on first call.
    pub fn rerank(&self, query: &str, doc: &str) -> Result<f32, String> {
        let mut guard = self.reranker.write().unwrap();
        if guard.is_none() {
            let dir = app_base_dir().join("models").join("reranker");
            *guard = load_bundle(&dir).ok();
        }
        let b = guard.as_ref().ok_or("reranker not ready")?;
        // Truncate the document, not the prompt: the trailing template text
        // (thinking/response scaffolding) is the yes/no scoring position, so
        // cutting the tail would make the logits meaningless.
        let doc = truncate_tokens(doc, RERANK_DOC_MAX_TOKENS);
        let prompt = rerank_prompt(query, &doc);
        let enc = b.tokenizer.encode(prompt.as_str(), false).map_err(|e| e.to_string())?;
        let ids: Vec<u32> = enc.get_ids().iter().copied().collect();
        if ids.is_empty() {
            return Err("empty input".into());
        }
        let input = Tensor::from_slice(&ids, (1, ids.len()), &Device::Cpu).map_err(|e| e.to_string())?;
        let mut model = ModelForCausalLM::new(&b.config, b.vb.clone()).map_err(|e| e.to_string())?;
        let logits = model.forward(&input, 0).map_err(|e| e.to_string())?;
        let logits = logits.squeeze(1).map_err(|e| e.to_string())?;
        let yes = logits
            .narrow(1, YES_ID as usize, 1)
            .map_err(|e| e.to_string())?
            .squeeze(1)
            .map_err(|e| e.to_string())?
            .squeeze(0)
            .map_err(|e| e.to_string())?
            .to_scalar::<f32>()
            .map_err(|e| e.to_string())?;
        let no = logits
            .narrow(1, NO_ID as usize, 1)
            .map_err(|e| e.to_string())?
            .squeeze(1)
            .map_err(|e| e.to_string())?
            .squeeze(0)
            .map_err(|e| e.to_string())?
            .to_scalar::<f32>()
            .map_err(|e| e.to_string())?;
        Ok(1.0 / (1.0 + (no - yes).exp()))
    }
}

fn load_bundle(dir: &Path) -> Result<Bundle, String> {
    let config_path = dir.join("config.json");
    let tokenizer_path = dir.join("tokenizer.json");
    let model_path = dir.join("model.safetensors");
    if !config_path.exists() || !tokenizer_path.exists() || !model_path.exists() {
        return Err("model files missing".into());
    }
    let config: qwen3::Config = serde_json::from_str(
        &std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| e.to_string())?;
    let tensor_map = load_and_rename_tensors(&model_path)?;
    let vb = VarBuilder::from_tensors(tensor_map, DType::F32, &Device::Cpu);
    Ok(Bundle {
        config,
        tokenizer,
        vb,
    })
}

fn bundle_files_present(dir: &Path) -> bool {
    MODEL_FILES.iter().all(|f| dir.join(f).exists())
}

/// Document budget inside the rerank prompt. The template + query take ~100
/// tokens; keeping the doc under this leaves the scoring tail intact and
/// bounds the forward to ~500 tokens.
const RERANK_DOC_MAX_TOKENS: usize = 400;

/// First `max_tokens` of `s` (estimate_tokens-based char scan).
pub(crate) fn truncate_tokens(s: &str, max_tokens: usize) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let t = crate::ai::estimate_tokens(&out) + crate::ai::estimate_tokens(&c.to_string());
        if t > max_tokens {
            break;
        }
        out.push(c);
    }
    out
}

/// Download the repo's files into `dir` via hf-hub (blocking). Uses
/// HF_ENDPOINT when the user set it, else the mirror.
fn download_bundle(dir: &Path, repo: &str) -> Result<(), String> {
    std::env::var("HF_ENDPOINT").unwrap_or_else(|_| {
        std::env::set_var("HF_ENDPOINT", HF_MIRROR);
        HF_MIRROR.to_string()
    });
    let client = hf_hub::HFClientSync::new().map_err(|e| e.to_string())?;
    let (owner, name) = repo.split_once('/').ok_or("bad repo")?;
    let repo = client.model(owner, name);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for f in MODEL_FILES {
        if dir.join(f).exists() {
            continue;
        }
        let fname = f.to_string();
        repo.download_file()
            .filename(fname)
            .local_dir(dir.to_path_buf())
            .send()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Qwen3-Embedding ships bare tensor names, Qwen3-Reranker ships model.-
/// prefixed ones; candle expects the prefix. Add it only when absent.
fn load_and_rename_tensors(path: &Path) -> Result<HashMap<String, Tensor>, String> {
    let tensors = candle_core::safetensors::load(path, &Device::Cpu).map_err(|e| e.to_string())?;
    let mut map = HashMap::with_capacity(tensors.len());
    for (name, tensor) in tensors {
        let new_name = if name.starts_with("model.") || name == "lm_head.weight" {
            name
        } else {
            format!("model.{name}")
        };
        map.insert(new_name, tensor);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rerank_prompt_shape() {
        let p = rerank_prompt("q", "d");
        assert!(p.contains("<|im_start|>system\nJudge whether the Document meets"));
        assert!(p.contains("<Query>: q"));
        assert!(p.contains("<Document>: d"));
        assert!(p.contains("<|im_start|>assistant"));
        assert!(p.contains("response"));
    }

    #[test]
    fn query_prefix_applied() {
        assert_eq!(
            format!("{EMBED_QUERY_PREFIX}{}", "x"),
            "Instruct: Given a question, retrieve relevant passages\nQuery: x"
        );
    }

    #[test]
    fn rename_adds_model_prefix_once() {
        use candle_core::Tensor;
        let dir = std::env::temp_dir().join("rag_rename_test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.safetensors");
        let tensors: HashMap<String, Tensor> = [
            ("embed_tokens.weight".to_string(), Tensor::zeros(&[2, 2], DType::F32, &Device::Cpu).unwrap()),
            ("model.layers.0.weight".to_string(), Tensor::zeros(&[2, 2], DType::F32, &Device::Cpu).unwrap()),
            ("lm_head.weight".to_string(), Tensor::zeros(&[2, 2], DType::F32, &Device::Cpu).unwrap()),
        ]
        .into_iter()
        .collect();
        candle_core::safetensors::save(&tensors, &file).unwrap();
        let map = load_and_rename_tensors(&file).unwrap();
        assert!(map.contains_key("model.embed_tokens.weight"));
        assert!(map.contains_key("model.layers.0.weight"));
        assert!(map.contains_key("lm_head.weight"));
        assert!(!map.contains_key("embed_tokens.weight"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_model_errors() {
        let b = load_bundle(Path::new("/nonexistent"));
        assert!(b.is_err());
    }

    #[test]
    fn embed_errors_without_model() {
        let m = Models::new();
        assert!(m.embed("x", true).is_err());
        // rerank may lazy-load from the app dir; with no app models it errs.
        let status = m.status.read().unwrap().clone();
        let _ = status;
    }

    #[test]
    fn truncate_tokens_budget() {
        assert_eq!(truncate_tokens("你好世界", 1), "你好世");
        let long = "汉".repeat(1000);
        let t = truncate_tokens(&long, 300);
        // floor-based estimator can overshoot by a few chars (~4 tokens)
        let est = crate::ai::estimate_tokens(&t);
        assert!(est <= 305, "got {est}");
        assert!(est >= 295, "got {est}");
    }
}
