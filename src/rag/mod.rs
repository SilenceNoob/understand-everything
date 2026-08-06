mod bm25;
mod chunk;
pub mod model;
pub mod service;

pub use chunk::{chunk_markdown, Chunk, ChunkSource};
pub use model::ModelStatus;
pub use service::RagService;
pub(crate) use bm25::Bm25Index;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::util::app_base_dir;

/// Cached index files bigger than this are discarded outright (a corrupt
/// length prefix would otherwise make bincode allocate absurdly).
const INDEX_MAX_BYTES: u64 = 200 * 1024 * 1024;

/// Per-map retrieval index, persisted as bincode in .rag_cache/.
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct RagIndex {
    pub chunks: Vec<Chunk>,
    /// Per-source fingerprint: doc = (mtime, len), card = content hash.
    fingerprints: HashMap<String, u64>,
}

impl RagIndex {
    /// "maps/foo.json" -> "<base>/.rag_cache/foo.bin" (mirrors refs_path).
    fn cache_path(map_rel: &str) -> std::path::PathBuf {
        let rel = map_rel.strip_prefix("maps/").unwrap_or(map_rel);
        app_base_dir().join(".rag_cache").join(format!("{rel}.bin"))
    }

    pub fn load(map_rel: &str) -> Option<RagIndex> {
        let path = Self::cache_path(map_rel);
        let len = std::fs::metadata(&path).ok()?.len();
        if len == 0 || len > INDEX_MAX_BYTES {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        bincode::deserialize(&bytes).ok()
    }

    pub fn save(&self, map_rel: &str) {
        let path = Self::cache_path(map_rel);
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(bytes) = bincode::serialize(self) {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Re-chunk every source whose file fingerprint (mtime ^ len) changed;
    /// drop sources no longer in the list. Others keep chunks and vectors.
    /// Card bodies live on disk (commit_edit writes them synchronously), so
    /// both kinds are read from `path`; the card title is the file stem,
    /// mirroring card_title(). Returns true when anything changed (callers
    /// skip persistence when false).
    pub fn sync_sources(&mut self, sources: &[(ChunkSource, std::path::PathBuf)]) -> bool {
        let mut seen = Vec::with_capacity(sources.len());
        let mut changed = false;
        for (source, path) in sources {
            let Some(meta) = std::fs::metadata(path).ok() else {
                continue;
            };
            let fp = (meta.modified().map(|m| {
                m.duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0)
            })
            .unwrap_or(0))
                ^ meta.len();
            let key = source.key();
            seen.push(key.clone());
            if self.fingerprints.get(&key) == Some(&fp) {
                continue;
            }
            let Some(text) = std::fs::read_to_string(path).ok() else {
                continue;
            };
            let heading_ctx = match source {
                ChunkSource::RefDoc(_) => String::new(),
                ChunkSource::Card(_) => path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            };
            let chunks = chunk_markdown(source.clone(), &heading_ctx, &text);
            self.replace_source(key, fp, chunks);
            changed = true;
        }
        changed |= self.prune(&seen);
        changed
    }

    fn replace_source(&mut self, key: String, fp: u64, chunks: Vec<Chunk>) {
        self.chunks.retain(|c| c.source.key() != key);
        self.chunks.extend(chunks);
        self.fingerprints.insert(key, fp);
    }

    /// Returns true when any chunk/fingerprint was dropped.
    fn prune(&mut self, kept: &[String]) -> bool {
        let before = self.chunks.len();
        self.chunks.retain(|c| kept.iter().any(|k| k == &c.source.key()));
        self.fingerprints.retain(|k, _| kept.iter().any(|k2| k2 == k));
        self.chunks.len() != before
    }
}

/// Frozen retrieval entry point (Phase 3 fills dense+rerank, same signature).
/// Phase 2: pure BM25.
#[derive(Clone, Debug, PartialEq)]
pub struct RetrievedChunk {
    pub source: ChunkSource,
    pub heading: String,
    pub text: String,
    pub score: f32,
}

/// Takes a prebuilt inverted index (callers cache it; building is O(all
/// tokens) per call).
pub fn retrieve(
    chunks: &[Chunk],
    bm25: &Bm25Index,
    query: &str,
    top_k: usize,
) -> Vec<RetrievedChunk> {
    bm25.search(query, top_k)
        .into_iter()
        .map(|(i, score)| {
            let c = &chunks[i];
            RetrievedChunk {
                source: c.source.clone(),
                heading: c.heading.clone(),
                text: c.text.clone(),
                score,
            }
        })
        .collect()
}

/// Embed every chunk that lacks a vector (doc side: plain text). Embedding
/// failures leave the chunk BM25-only; returns how many got embedded.
pub fn embed_chunks(models: &model::Models, chunks: &mut [Chunk]) -> usize {
    let mut n = 0;
    for c in chunks.iter_mut() {
        if c.vector.is_empty() {
            if let Ok(v) = models.embed(&c.text, false) {
                c.vector = v;
                n += 1;
            }
        }
    }
    n
}

/// Rerank candidate cap and per-doc token budget. Measured on CPU fp32:
/// rerank ≈ 3s at 512 tokens, ≈ 2s at 256. 5 × 2s + 0.8s embed ≈ 11s is the
/// interactive budget; raise only on faster hardware.
const RERANK_CANDIDATES: usize = 5;
const RERANK_DOC_TOKENS: usize = 256;

/// Full pipeline: dense cosine Top-5 ∪ BM25 Top-5, fused with RRF (rank-based,
/// so the two incomparable score scales never fight), rerank the top 5, return
/// top-k. Every step degrades: query embed failure skips dense, rerank
/// failure keeps the pre-rerank score, missing models → pure BM25.
pub fn retrieve_hybrid(
    chunks: &[Chunk],
    bm25: &Bm25Index,
    models: &model::Models,
    query: &str,
    top_k: usize,
) -> Vec<RetrievedChunk> {
    const RRF_K: f32 = 60.0;
    let mut rrf: HashMap<usize, f32> = HashMap::new();
    for (rank, (i, _)) in bm25.search(query, RERANK_CANDIDATES).into_iter().enumerate() {
        *rrf.entry(i).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    if let Ok(q) = models.embed(query, true) {
        let mut dense: Vec<(usize, f32)> = chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.vector.is_empty())
            .map(|(i, c)| (i, c.vector.iter().zip(&q).map(|(a, b)| a * b).sum()))
            .collect();
        dense.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (rank, (i, _)) in dense.into_iter().take(RERANK_CANDIDATES).enumerate() {
            *rrf.entry(i).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut cands: Vec<(usize, f32)> = rrf.into_iter().collect();
    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    cands.truncate(RERANK_CANDIDATES);
    for (i, s) in cands.iter_mut() {
        let doc = model::truncate_tokens(&chunks[*i].text, RERANK_DOC_TOKENS);
        if let Ok(r) = models.rerank(query, &doc) {
            *s = r;
        }
    }
    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    cands.truncate(top_k);
    cands
        .into_iter()
        .map(|(i, score)| {
            let c = &chunks[i];
            RetrievedChunk {
                source: c.source.clone(),
                heading: c.heading.clone(),
                text: c.text.clone(),
                score,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP_REL: &str = "maps/test_rag.json";

    fn tmp_doc(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("rag_test_docs");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn fingerprint_triggers_incremental_rechunk() {
        let p = tmp_doc("inc.md", "# A\n\n内容甲\n\n## B\n\n内容乙");
        let src = |p: std::path::PathBuf| vec![(ChunkSource::RefDoc(p.clone()), p)];
        let mut idx = RagIndex::default();
        idx.sync_sources(&src(p.clone()));
        let n = idx.chunks.len();
        assert!(n >= 2);
        let ids: Vec<u64> = idx.chunks.iter().map(|c| c.id).collect();

        // unchanged: no re-chunk, ids stable
        idx.sync_sources(&src(p.clone()));
        assert_eq!(idx.chunks.len(), n);
        assert!(idx.chunks.iter().map(|c| c.id).eq(ids.iter().copied()));

        // changed content: re-chunk
        std::fs::write(&p, "# A\n\n全新内容甲\n\n## B\n\n全新内容乙").unwrap();
        idx.sync_sources(&src(p.clone()));
        assert!(idx.chunks.iter().any(|c| c.text.contains("全新")));

        // removed doc: chunks gone
        idx.sync_sources(&[]);
        assert!(idx.chunks.is_empty());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn card_sync_keeps_unchanged_cards() {
        let dir = std::env::temp_dir().join("rag_test_cards");
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = dir.join("卡片甲.md");
        let p2 = dir.join("卡片乙.md");
        std::fs::write(&p1, "正文甲").unwrap();
        std::fs::write(&p2, "正文乙").unwrap();
        let sources = vec![
            (ChunkSource::Card("n1".into()), p1.clone()),
            (ChunkSource::Card("n2".into()), p2.clone()),
        ];
        let mut idx = RagIndex::default();
        idx.sync_sources(&sources);
        assert_eq!(idx.chunks.len(), 2);

        std::fs::write(&p2, "改了的乙").unwrap();
        idx.sync_sources(&sources);
        assert_eq!(idx.chunks.len(), 2);
        assert!(idx.chunks.iter().any(|c| c.text.contains("改了的乙")));
        assert_eq!(idx.chunks.iter().filter(|c| c.source.key() == "card:n1").count(), 1);
        // unchanged card keeps heading from the file stem
        let n1: Vec<&Chunk> = idx.chunks.iter().filter(|c| c.source.key() == "card:n1").collect();
        assert_eq!(n1[0].heading, "卡片甲");
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn retrieve_returns_ranked_chunks() {
        let dir = std::env::temp_dir().join("rag_test_retr");
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = dir.join("机器学习.md");
        let p2 = dir.join("烘焙.md");
        std::fs::write(&p1, "神经网络通过反向传播更新权重").unwrap();
        std::fs::write(&p2, "蛋糕需要面粉鸡蛋糖").unwrap();
        let sources = vec![
            (ChunkSource::Card("n1".into()), p1.clone()),
            (ChunkSource::Card("n2".into()), p2.clone()),
        ];
        let mut idx = RagIndex::default();
        idx.sync_sources(&sources);
        let chunks = idx.chunks.clone();
        let bm25 = Bm25Index::build(&chunks);
        let hits = retrieve(&chunks, &bm25, "神经网络 权重 蛋糕", 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].heading, "机器学习");
        assert!(hits[0].text.contains("神经网络"));
        assert_eq!(hits[0].source.key(), "card:n1");
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bincode_roundtrip() {
        let p = tmp_doc("rt.md", "# H\n\n正文内容\n\n## S\n\n更多内容");
        let sources = vec![(ChunkSource::RefDoc(p.clone()), p.clone())];
        let mut idx = RagIndex::load(MAP_REL).unwrap_or_default();
        idx.sync_sources(&sources);
        idx.save(MAP_REL);
        let loaded = RagIndex::load(MAP_REL).expect("reload");
        assert_eq!(loaded.chunks.len(), idx.chunks.len());
        assert_eq!(loaded.chunks, idx.chunks);
        let p = RagIndex::cache_path(MAP_REL);
        std::fs::remove_file(&p).ok();
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
        std::fs::remove_file(&sources[0].1).ok();
    }

    /// Phase 3 acceptance: real latency numbers + hybrid retrieval over
    /// docs/Canvas 规则.md. Needs models/ under the app base dir.
    #[test]
    #[ignore]
    fn bench_models_and_hybrid() {
        use std::time::Instant;
        let models = model::Models::new();
        models.ensure(&app_base_dir());
        println!("status: {:?}", *models.status.read().unwrap());
        assert!(models.embedding_ready());

        for label in ["query", "doc"] {
            let text = "什么是机器学习中的反向传播算法".repeat(3);
            let t = Instant::now();
            let v = models.embed(&text, label == "query").unwrap();
            println!("embed({label}) {} chars → {:.0}ms, dim={}", text.len(), t.elapsed().as_millis(), v.len());
        }
        let doc = "神经网络通过反向传播更新权重，这是深度学习训练的核心机制。".repeat(5);
        let t = Instant::now();
        let s = models.rerank("反向传播 神经网络", &doc).unwrap();
        println!("rerank(long) → {:.0}ms, score={:.3}", t.elapsed().as_millis(), s);
        let short: String = doc.chars().take(100).collect();
        let t = Instant::now();
        let s = models.rerank("反向传播 神经网络", &short).unwrap();
        println!("rerank(short) → {:.0}ms, score={:.3}", t.elapsed().as_millis(), s);

        let doc_path = app_base_dir().join("docs/Canvas 规则.md");
        let mut idx = RagIndex::default();
        idx.sync_sources(&[(ChunkSource::RefDoc(doc_path.clone()), doc_path.clone())]);
        // synthetic bigger doc so the rerank candidate cap is actually hit
        let big = std::env::temp_dir().join("rag_bench_big.md");
        let mut content = String::new();
        for i in 0..30 {
            content.push_str(&format!(
                "# 章节 {i}\n\n神经网络与反向传播的内容第 {i} 段，包含权重更新、损失函数与梯度下降的讨论。\n\n{}\n\n",
                "补充说明文字，用于凑足段落长度。".repeat(40)
            ));
        }
        std::fs::write(&big, &content).unwrap();
        idx.sync_sources(&[(ChunkSource::RefDoc(big.clone()), big.clone())]);
        let t = Instant::now();
        let n = embed_chunks(&models, &mut idx.chunks);
        println!("embedded {n} chunks in {:.1}s", t.elapsed().as_secs_f32());
        println!("total chunks: {}", idx.chunks.len());

        let query = "梯度下降 损失函数";
        let t = Instant::now();
        let chunks = idx.chunks.clone();
        let bm25 = Bm25Index::build(&chunks);
        let bm25_hits = retrieve(&chunks, &bm25, query, 5);
        println!("bm25({query}) {:.0}ms → {} hits", t.elapsed().as_millis(), bm25_hits.len());
        for h in &bm25_hits {
            println!("  [{:.3}] {}", h.score, h.heading);
        }
        let t = Instant::now();
        let hits = retrieve_hybrid(&chunks, &bm25, &models, query, 5);
        let dt = t.elapsed();
        println!("hybrid({query}) → {:.1}s", dt.as_secs_f32());
        for h in &hits {
            let snippet: String = h.text.chars().take(50).collect();
            println!("  [{:.3}] {} — {snippet}", h.score, h.heading);
        }
        assert!(!hits.is_empty());
        std::fs::remove_file(&big).ok();
    }
}
