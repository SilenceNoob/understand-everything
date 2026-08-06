use std::collections::{HashMap, HashSet};

use super::Chunk;

/// Tokenize for BM25: ASCII alnum runs lowercase as one word, every CJK
/// (non-ASCII alphabetic) char as its own unigram, punctuation dropped.
/// No external tokenizer; good enough for keyword retrieval.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            word.push(c.to_ascii_lowercase());
        } else {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            if !c.is_ascii() && c.is_alphabetic() {
                out.push(c.to_string());
            }
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

const K1: f32 = 1.5;
const B: f32 = 0.75;

/// In-memory inverted index over the chunk list. Rebuilt per build() call;
/// the tokenize pass over a few thousand chunks is milliseconds, so there is
/// no persistent copy.
pub struct Bm25Index {
    n_docs: usize,
    avg_len: f32,
    doc_lens: Vec<usize>,
    postings: HashMap<String, Vec<(usize, usize)>>,
}

impl Bm25Index {
    pub fn build(chunks: &[Chunk]) -> Self {
        let mut postings: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let mut doc_lens = Vec::with_capacity(chunks.len());
        let mut total = 0usize;
        for (i, c) in chunks.iter().enumerate() {
            let mut tf: HashMap<String, usize> = HashMap::new();
            for t in tokenize(&c.text) {
                *tf.entry(t).or_insert(0) += 1;
            }
            let len: usize = tf.values().sum();
            doc_lens.push(len);
            total += len;
            for (t, n) in tf {
                postings.entry(t).or_default().push((i, n));
            }
        }
        let n = chunks.len();
        Self {
            n_docs: n,
            avg_len: total as f32 / n.max(1) as f32,
            doc_lens,
            postings,
        }
    }

    /// Top-k (chunk_idx, score) by BM25.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(usize, f32)> {
        let n = self.n_docs as f32;
        let mut scores: HashMap<usize, f32> = HashMap::new();
        let mut terms = HashSet::new();
        for t in tokenize(query) {
            if !terms.insert(t.clone()) {
                continue;
            }
            let Some(postings) = self.postings.get(&t) else {
                continue;
            };
            let df = postings.len() as f32;
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for &(di, tf) in postings {
                let dl = self.doc_lens[di] as f32;
                let denom = tf as f32 + K1 * (1.0 - B + B * dl / self.avg_len.max(1.0));
                *scores.entry(di).or_insert(0.0) += idf * tf as f32 * (K1 + 1.0) / denom;
            }
        }
        let mut v: Vec<(usize, f32)> = scores.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v.truncate(top_k);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(text: &str) -> Chunk {
        Chunk {
            id: 0,
            source: super::super::ChunkSource::Card(format!("c{}", rand_id())),
            heading: String::new(),
            text: text.to_string(),
            vector: Vec::new(),
        }
    }
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn rand_id() -> u64 {
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn tokenize_mixed_cjk_ascii() {
        assert_eq!(tokenize("Hello, 世界!"), vec!["hello", "世", "界"]);
        assert_eq!(tokenize("RAG 检索 & 排序"), vec!["rag", "检", "索", "排", "序"]);
        assert_eq!(tokenize(""), Vec::<String>::new());
        assert_eq!(tokenize("ABC-123"), vec!["abc", "123"]);
    }

    #[test]
    fn chinese_query_ranks_relevant_chunk_first() {
        let docs = [
            "机器学习是人工智能的分支，用数据训练模型",
            "烘焙蛋糕需要面粉、鸡蛋和糖，混合搅拌后烘烤",
            "神经网络由多层神经元组成，反向传播更新权重",
            "游泳是一项全身运动，锻炼心肺功能",
            "Redis 是内存数据库，常用作缓存",
            "Rust 语言以内存安全著称，适合系统编程",
            "思维导图帮助梳理知识结构，节点间用连线表示关系",
            "量子计算利用叠加态和纠缠态进行计算",
            "足球比赛的规则包括越位、角球和点球",
            "这本小说讲述了一个关于时间和记忆的故事",
        ];
        let chunks: Vec<Chunk> = docs.iter().map(|d| chunk(d)).collect();
        let bm25 = Bm25Index::build(&chunks);
        let hits = bm25.search("神经网络 权重 反向传播", 3);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].0, 2, "neural-net doc must rank first");
        assert_eq!(bm25.search("蛋糕 烘焙 面粉", 1)[0].0, 1);
        assert_eq!(bm25.search("足球 越位 角球", 1)[0].0, 8);
        assert_eq!(bm25.search("hello world", 1).len(), 0);
    }

    #[test]
    fn avg_len_and_len_zero_safe() {
        let bm25 = Bm25Index::build(&[]);
        assert!(bm25.search("x", 5).is_empty());
        let bm25 = Bm25Index::build(&[chunk("")]);
        assert!(bm25.search("x", 5).is_empty());
    }
}
