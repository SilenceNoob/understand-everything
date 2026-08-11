use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock, RwLock};
use std::thread;

use super::model::Models;
use super::{Bm25Index, Chunk, ChunkSource, RagIndex, RetrievedChunk, retrieve, retrieve_hybrid};
use crate::util::app_base_dir;

/// Max excerpts in the injected context (top-5 retrieval).
const CONTEXT_HITS: usize = 5;
/// Excerpt length in chars; keeps the injected system message small.
const EXCERPT_CHARS: usize = 300;

pub struct RetrieveResult {
    pub query: String,
    pub hits: Vec<RetrievedChunk>,
}

/// The published index plus its inverted index. Bm25Index is O(all tokens) to
/// build, so it is built once per index change and shared via Arc; readers
/// take both under one short read lock. `map_rel` distinguishes the current
/// map, so switching maps republishes even when nothing changed.
struct SharedState {
    map_rel: String,
    bm25: Arc<Bm25Index>,
    index: RagIndex,
}

/// RAG backend: two worker threads.
///
/// - Index worker: downloads/loads the models once, then serially rebuilds
///   the index from disk snapshots (fingerprint-diffed, embedding only
///   changed sources), persisting to .rag_cache/ and swapping the shared
///   index. Long embedding batches run here, never on the UI thread.
/// - Retrieval worker: budgeted dense+rerank per request, answering on a
///   per-request channel the UI polls from its timer.
///
/// Threads share the model bundles (refcounted tensor maps) and the index
/// (RwLock; readers clone chunks under a short lock).
pub struct RagService {
    slot: Arc<OnceLock<Arc<Models>>>,
    shared: Arc<RwLock<SharedState>>,
    indexing: Arc<AtomicBool>,
    index_tx: mpsc::Sender<String>,
    retr_tx: mpsc::Sender<(String, mpsc::Sender<RetrieveResult>)>,
}

impl RagService {
    pub fn start() -> Self {
        let slot: Arc<OnceLock<Arc<Models>>> = Arc::new(OnceLock::new());
        let shared: Arc<RwLock<SharedState>> = Arc::new(RwLock::new(SharedState {
            map_rel: String::new(),
            bm25: Arc::new(Bm25Index::build(&[])),
            index: RagIndex::default(),
        }));
        let indexing = Arc::new(AtomicBool::new(false));
        let (index_tx, index_rx) = mpsc::channel::<String>();
        let (retr_tx, retr_rx) = mpsc::channel::<(String, mpsc::Sender<RetrieveResult>)>();

        {
            let slot = slot.clone();
            let shared = shared.clone();
            let indexing = indexing.clone();
            thread::spawn(move || {
                let base = app_base_dir();
                let models = Arc::new(Models::new());
                let _ = slot.set(models.clone());
                // Blocking download/load; status inside `models` is live, so
                // the UI can show 下载模型…/加载模型… while this runs.
                models.ensure(&base);
                // Move the reranker's ~2.3GB load off the first interactive
                // rerank (a route plan's 20s retrieval window would otherwise
                // fall back to BM25 while it loads).
                models.warm_reranker();
                while let Ok(map_rel) = index_rx.recv() {
                    // A panic (e.g. candle OOM) must not kill the worker or
                    // leave `indexing` stuck true; the index swap only
                    // happens after a fully-built local index, so a partial
                    // build never publishes.
                    indexing.store(true, Ordering::Relaxed);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let sources = snapshot_sources(&base, &map_rel);
                        let mut idx = RagIndex::load(&map_rel).unwrap_or_default();
                        let changed = idx.sync_sources(&sources);

                        // Text is ready: publish immediately (BM25 works) so
                        // the slow embedding batch never blocks retrieval of
                        // fresh content. Embedding then upgrades vectors.
                        let same_map = shared.read().unwrap().map_rel == map_rel;
                        let mut cur_bm25 = shared.read().unwrap().bm25.clone();
                        if changed || !same_map {
                            let bm25 = Arc::new(Bm25Index::build(&idx.chunks));
                            cur_bm25 = bm25.clone();
                            *shared.write().unwrap() = SharedState {
                                map_rel: map_rel.clone(),
                                bm25,
                                index: idx.clone(),
                            };
                        }
                        let embedded = if let Some(m) = slot.get() {
                            super::embed_chunks(m, &mut idx.chunks)
                        } else {
                            0
                        };
                        if changed || embedded > 0 {
                            idx.save(&map_rel);
                        }
                        if changed || embedded > 0 || !same_map {
                            *shared.write().unwrap() = SharedState {
                                map_rel: map_rel.clone(),
                                bm25: cur_bm25,
                                index: idx,
                            };
                        }
                    }));
                    indexing.store(false, Ordering::Relaxed);
                    if result.is_err() {
                        eprintln!("rag: index worker panic on {map_rel}");
                    }
                }
            });
        }
        {
            let slot = slot.clone();
            let shared = shared.clone();
            thread::spawn(move || {
                while let Ok((query, reply)) = retr_rx.recv() {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let Some(models) = slot.get() else {
                            return;
                        };
                        let (chunks, bm25) = read_state(&shared);
                        let hits = if models.embedding_ready() {
                            retrieve_hybrid(&chunks, &bm25, models, &query, CONTEXT_HITS)
                        } else {
                            retrieve(&chunks, &bm25, &query, CONTEXT_HITS)
                        };
                        let _ = reply.send(RetrieveResult { query, hits });
                    }));
                }
            });
        }
        Self {
            slot,
            shared,
            indexing,
            index_tx,
            retr_tx,
        }
    }

    /// Loaded models, once the index worker finished downloading/loading.
    pub fn models(&self) -> Option<Arc<Models>> {
        self.slot.get().cloned()
    }

    /// True while the index worker is re-chunking/embedding a snapshot.
    pub fn indexing(&self) -> bool {
        self.indexing.load(Ordering::Relaxed)
    }

    /// Enqueue a rebuild from the current on-disk map+refs snapshot.
    /// Fingerprint-diffed: unchanged sources are skipped, so calling this
    /// periodically is cheap.
    pub fn set_map(&self, map_rel: &str) {
        let _ = self.index_tx.send(map_rel.to_string());
    }

    /// Enqueue a retrieval; the UI polls the returned receiver (from its
    /// timer, never blocking the main thread).
    pub fn retrieve(&self, query: &str) -> mpsc::Receiver<RetrieveResult> {
        let (tx, rx) = mpsc::channel();
        let _ = self.retr_tx.send((query.to_string(), tx));
        rx
    }

    /// Synchronous BM25-only search against the current index (µs; reuses
    /// the cached inverted index).
    pub fn bm25_search(&self, query: &str, top_k: usize) -> Vec<RetrievedChunk> {
        let (chunks, bm25) = read_state(&self.shared);
        retrieve(&chunks, &bm25, query, top_k)
    }

    /// True when the published index belongs to `map_rel` and has chunks;
    /// hybrid retrieval over an empty index is guaranteed to return no hits,
    /// so callers can skip the query-embed/rerank round trip entirely.
    pub fn has_chunks_for(&self, map_rel: &str) -> bool {
        let s = self.shared.read().unwrap();
        s.map_rel == map_rel && !s.index.chunks.is_empty()
    }
}

fn read_state(shared: &RwLock<SharedState>) -> (Vec<Chunk>, Arc<Bm25Index>) {
    let s = shared.read().unwrap();
    (s.index.chunks.clone(), s.bm25.clone())
}

/// All indexable sources of a map, straight from disk: refs/<map>.json doc
/// paths plus every card body file referenced by maps/<map>.json.
fn snapshot_sources(base: &std::path::Path, map_rel: &str) -> Vec<(ChunkSource, PathBuf)> {
    let mut out = Vec::new();
    for p in crate::refs_panel::ref_doc_paths(map_rel) {
        out.push((ChunkSource::RefDoc(p.clone()), p));
    }
    if let Some(data) = crate::mindmap::model::MindMapData::load_from(base, map_rel) {
        for n in data.nodes {
            out.push((ChunkSource::Card(n.id), base.join(&n.path)));
        }
    }
    out
}

/// The system-message context injected before the user message. Empty when
/// there is nothing to cite.
pub fn format_context(hits: &[RetrievedChunk]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "以下是本思维导图的参考资料和卡片内容摘录，回答时优先依据它们，并在引用处标注 [编号]：\n",
    );
    for (i, h) in hits.iter().enumerate() {
        let src = match &h.source {
            ChunkSource::Card(_) => format!("卡片《{}》", h.heading),
            ChunkSource::RefDoc(p) => format!(
                "资料 {}",
                p.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
        };
        let excerpt: String = h.text.chars().take(EXCERPT_CHARS).collect();
        out.push_str(&format!("[{}] (来源: {src}) {excerpt}\n", i + 1));
    }
    out
}
