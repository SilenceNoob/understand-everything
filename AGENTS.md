# AGENTS.md

## 设计理念

- 核心设计理念是「渐构学习观」（判别模型/联结模型、下上结构、明确输入输出）——**`docs/设计理念.md` 是本仓库的功能设计与决策依据**，任何功能改动前先读它，并回答其中第 3 节的准则问题。

## Commands

- Run app: `cargo run` (windowed Makepad app)
- Check: `cargo check` — fast; a pre-existing `naga v27.0.3` future-incompat warning from makepad is noise, ignore it
- Test: `cargo test` (unit tests live in-module: `markdown_media.rs`, `mindmap/model.rs`)
- RAG smoke test: `cargo run --example rag_smoke` — needs safetensors at `/tmp/rag_test/models/{embedding,reranker}/model.safetensors` (skips model tests if absent)
- RAG bench (real latency + hybrid retrieval): `cargo test rag::tests::bench -- --ignored --nocapture` — needs `models/` under the repo root (symlink to `/tmp/rag_test/models/*` works)

## Layout

- Single crate. `src/main.rs` = App struct + all widget wiring (script_mod DSL inline). Widgets: `src/{float_panel,file_panel,refs_panel,slide_panel,chat_list,markdown_media}.rs`. Mindmap: `src/mindmap/`. LLM HTTP: `src/ai.rs`. RAG: `src/rag/{chunk,bm25,model,service}.rs` (two worker threads started lazily on first draw; app polls via a 0.25s timer).
- `app_base_dir()` = `CARGO_MANIFEST_DIR`, so **runtime data lives in the repo root**: `cards/`, `maps/`, `refs/`, `docs/`, `models/`, `.rag_cache/`, `settings.json` (all gitignored except structure). `resources/` = fonts/icons.
- `vendor-pulldown-cmark/` is a patched local fork — do not switch back to crates.io.
- `.codegraph/` index exists — use `codegraph_explore` before grep/read for code questions.

## Pitfalls (verified, cost time to find)

- **bincode must stay at 1.x** — `bincode 3.0.0` on crates.io is a placeholder that fails with `compile_error!("https://xkcd.com/2347/")`.
- **candle on CPU: use `DType::F32`** — BF16 matmul is unsupported on the CPU backend (`unsupported dtype BF16 for op matmul`).
- **Qwen3 safetensors tensor naming differs by repo**: `Qwen3-Embedding-0.6B` ships bare names (`embed_tokens.weight`), `Qwen3-Reranker-0.6B` ships `model.`-prefixed names. candle's `qwen3::Model` expects the `model.` prefix — the loader must add it only when absent (see `load_and_rename_tensors` in `examples/rag_smoke.rs`).
- **candle qwen3 KV cache is mutable state** and `Model::clear_kv_cache` is crate-private. For independent forwards (embedding batches, rerank scoring), construct a fresh `Model` per call from a shared `VarBuilder::from_tensors` map (cheap — HashMap is refcounted) or outputs silently corrupt.
- **Reranker tokenizer**: `encode(text, false)` — with `add_special_tokens=true` an EOS token shifts the yes/no logit position and scores collapse. yes token id = 9693, no = 2152 (from `1_LogitScore/config.json`). Embedding is the opposite: `encode(text, true)` so the tokenizer's post-processor appends `<|endoftext|>` (the pooling target).
- **candle qwen3 `forward(input, offset)` takes no attention mask** — no padded batching; embedding/rerank run one forward per text.
- **Measured CPU latencies (fp32, 0.6B)**: embed ≈ 0.8s short / 2.5s @512 tokens; rerank ≈ 1.1s short / 3s @512. Hybrid retrieval is budgeted: 5 rerank candidates × 256 tokens ≈ 10s, run on the retrieval worker; send_chat defers up to 20s then falls back to BM25 (µs). The reranker is warmed at startup (~2.3GB resident) so the first interactive rerank never pays the load; empty indexes skip retrieval entirely (`has_chunks_for`). Route planning is streaming (progress bubble) and its retrieval is prefetched at diagnostic start (`route_prefetch`).
- **RAG index is disk-driven**: the index worker reads `maps/<map>.json` + `refs/<map>.json` + card body files every 5s (fingerprint-diffed, mtime^len; unchanged = free, no rewrite of the bincode cache). Card edits need no event hook — `commit_edit` writes the body file synchronously.
- `makepad-widgets` tracks the git `dev` branch — upstream API churn is expected; after updating the lockfile, check panel/widget code compiles before other work.

## Conventions

- LLM calls are remote OpenAI-compatible only (`settings.json` holds api_key/base_url/model). Local candle inference is for RAG embedding/rerank only.
- Background work must not block the UI thread; results return via Makepad's event/timer mechanisms (see existing `handle_http_stream` / timer polling in `main.rs`).
