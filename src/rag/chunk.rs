use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ai::estimate_tokens;

/// Chunk text budget: aligned with the embedding truncation cap so the dense
/// vector never silently drops chunk tail.
pub const CHUNK_MAX_TOKENS: usize = 512;
/// Chunks below this are merged into a neighbor (when the result fits).
pub const CHUNK_MIN_TOKENS: usize = 300;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChunkSource {
    RefDoc(PathBuf),
    Card(String),
}

impl ChunkSource {
    pub fn key(&self) -> String {
        match self {
            ChunkSource::RefDoc(p) => format!("doc:{}", p.display()),
            ChunkSource::Card(id) => format!("card:{id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// Stable within a build of one source: hash(source key + block seq).
    pub id: u64,
    pub source: ChunkSource,
    /// Heading path, e.g. "章节A > 1.2 小节"; the card title for cards.
    pub heading: String,
    /// Heading path prefixed to the body, so retrieval carries provenance.
    pub text: String,
    /// L2-normalized dense vector; empty until Phase 3 embedding runs.
    pub vector: Vec<f32>,
}

/// Split markdown into chunks: ATX headings delimit sections; oversized
/// sections are packed into <=512-token chunks (paragraph granularity, then
/// sentence), and neighboring small chunks merge.
pub fn chunk_markdown(source: ChunkSource, heading_ctx: &str, text: &str) -> Vec<Chunk> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut flush = |stack: &mut Vec<&str>, cur: &mut Vec<&str>| {
        let body = cur.join("\n").trim().to_string();
        if body.is_empty() {
            return;
        }
        let mut path = if heading_ctx.is_empty() {
            String::new()
        } else {
            heading_ctx.to_string()
        };
        if !stack.is_empty() {
            let sub = stack.join(" > ");
            if path.is_empty() {
                path = sub;
            } else {
                path = format!("{path} > {sub}");
            }
        }
        sections.push((path, body));
        cur.clear();
    };
    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim();
        // Fenced code blocks: their `#` lines are code, not headings.
        if trimmed.starts_with("```") {
            in_code = !in_code;
            cur.push(line);
            continue;
        }
        if !in_code {
            if let Some(rest) = trimmed.strip_prefix('#') {
                let level = rest.len() - rest.trim_start_matches('#').len();
                let name = rest.trim_start_matches('#').trim();
                if !name.is_empty() {
                    flush(&mut stack, &mut cur);
                    stack.truncate(level.min(stack.len() + 1));
                    stack.push(name);
                }
                continue;
            }
        }
        cur.push(line);
    }
    flush(&mut stack, &mut cur);

    let mut chunks = Vec::new();
    let mut seq = 0u64;
    for (heading, body) in sections {
        for text in pack_section(heading.as_str(), &body) {
            chunks.push(Chunk {
                id: hash(&format!("{}#{}", source.key(), seq)),
                source: source.clone(),
                heading: heading.clone(),
                text,
                vector: Vec::new(),
            });
            seq += 1;
        }
    }
    chunks
}

/// Pack one section's body into chunk texts, each <= CHUNK_MAX_TOKENS
/// including the heading prefix. Paragraph granularity, sentence fallback.
/// A chunk left below CHUNK_MIN_TOKENS mid-section is carried into the next
/// chunk's text instead of being emitted alone.
fn pack_section(heading: &str, body: &str) -> Vec<String> {
    let prefix = if heading.is_empty() {
        String::new()
    } else {
        format!("{heading}\n\n")
    };
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut carry: Option<String> = None;
    for para in body.split("\n\n").map(str::trim) {
        if para.is_empty() {
            continue;
        }
        let attempt = match (&carry, cur.is_empty()) {
            (Some(c), _) => format!("{c}\n\n{prefix}{para}"),
            (None, true) => format!("{prefix}{para}"),
            (None, false) => format!("{cur}\n\n{para}"),
        };
        if estimate_tokens(&attempt) <= CHUNK_MAX_TOKENS {
            cur = attempt;
            carry = None;
            continue;
        }
        // Over budget: the carried chunk can't merge here, emit it alone.
        if let Some(c) = carry.take() {
            chunks.push(c);
        }
        let alone = format!("{prefix}{para}");
        if cur.is_empty() {
            if estimate_tokens(&alone) <= CHUNK_MAX_TOKENS {
                cur = alone;
            } else {
                chunks.extend(split_oversized(&prefix, para));
            }
        } else if estimate_tokens(&cur) < CHUNK_MIN_TOKENS {
            // Small chunk: merge it into this paragraph's chunk(s).
            if estimate_tokens(&format!("{cur}\n\n{alone}")) <= CHUNK_MAX_TOKENS {
                cur = format!("{cur}\n\n{alone}");
            } else {
                let mut pieces = split_oversized(&prefix, para);
                let first = pieces.remove(0);
                let merged = format!("{cur}\n\n{first}");
                if estimate_tokens(&merged) <= CHUNK_MAX_TOKENS {
                    chunks.push(merged);
                } else {
                    chunks.push(cur.clone());
                    chunks.push(first);
                }
                chunks.extend(pieces);
                cur.clear();
            }
        } else {
            // Full chunk: flush it, start fresh with this paragraph.
            chunks.push(cur.clone());
            cur = if estimate_tokens(&alone) <= CHUNK_MAX_TOKENS {
                alone
            } else {
                chunks.extend(split_oversized(&prefix, para));
                String::new()
            };
        }
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    // Only a small trailing chunk can still exist; merge it into its
    // predecessor when the result fits.
    merge_small(chunks)
}

/// Split one paragraph that alone exceeds the budget: greedy sentence packs,
/// hard character cut for sentences that are still too long.
fn split_oversized(prefix: &str, para: &str) -> Vec<String> {
    let sentences = para
        .split_inclusive(['。', '！', '？', '.', '!', '?', '…', '；', ';'])
        .collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut cur = prefix.to_string();
    for s in sentences {
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        let candidate = if cur == prefix {
            format!("{prefix}{s}")
        } else {
            format!("{cur}{s}")
        };
        if estimate_tokens(&candidate) <= CHUNK_MAX_TOKENS {
            cur = candidate;
        } else {
            if cur != prefix {
                chunks.push(cur);
                cur = prefix.to_string();
            }
            if estimate_tokens(&format!("{prefix}{s}")) <= CHUNK_MAX_TOKENS {
                cur = format!("{prefix}{s}");
            } else {
                let mut piece = prefix.to_string();
                for c in s.chars() {
                    let with = format!("{piece}{c}");
                    if estimate_tokens(&with) > CHUNK_MAX_TOKENS && piece != prefix {
                        chunks.push(piece);
                        piece = prefix.to_string();
                    }
                    piece.push(c);
                }
                if piece != prefix {
                    chunks.push(piece);
                }
            }
        }
    }
    if cur != prefix {
        chunks.push(cur);
    }
    chunks
}

/// Merge a chunk below CHUNK_MIN_TOKENS into the previous one when the
/// result still fits the budget (adjacent smalls cascade through last()).
fn merge_small(chunks: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in chunks {
        if let Some(last) = out.last() {
            if estimate_tokens(&c) < CHUNK_MIN_TOKENS {
                let merged = format!("{last}\n\n{c}");
                if estimate_tokens(&merged) <= CHUNK_MAX_TOKENS {
                    let n = out.len();
                    out[n - 1] = merged;
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Deterministic id hashing (DefaultHasher keys are fixed across runs).
fn hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens_of(chunks: &[Chunk]) -> usize {
        chunks.iter().map(|c| estimate_tokens(&c.text)).sum()
    }

    #[test]
    fn heading_path_accumulates() {
        let md = "# 第一章\n正文A\n\n## 1.1 小节\n正文B\n\n### 1.1.1\n正文C";
        let chunks = chunk_markdown(ChunkSource::Card("n1".into()), "", md);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].heading, "第一章");
        assert_eq!(chunks[1].heading, "第一章 > 1.1 小节");
        assert_eq!(chunks[2].heading, "第一章 > 1.1 小节 > 1.1.1");
        assert!(chunks[0].text.starts_with("第一章\n\n正文A"));
        assert!(chunks[2].text.starts_with("第一章 > 1.1 小节 > 1.1.1\n\n正文C"));
    }

    #[test]
    fn heading_ctx_prefixes_card_chunks() {
        let chunks = chunk_markdown(ChunkSource::Card("n1".into()), "卡片标题", "## 小节\n内容");
        assert_eq!(chunks[0].heading, "卡片标题 > 小节");
    }

    #[test]
    fn oversized_section_splits_within_budget() {
        // 1500 CJK chars ≈ 1050 tokens > 512 → several chunks, all in budget
        let md = format!("# H\n\n{}", "汉".repeat(1500));
        let chunks = chunk_markdown(ChunkSource::Card("n1".into()), "", &md);
        assert!(chunks.len() >= 2, "got {}", chunks.len());
        for c in &chunks {
            assert!(
                estimate_tokens(&c.text) <= CHUNK_MAX_TOKENS,
                "chunk {} tokens",
                estimate_tokens(&c.text)
            );
            assert!(c.text.starts_with("H\n\n"));
        }
        assert_eq!(tokens_of(&chunks), estimate_tokens(&md));
    }

    #[test]
    fn small_neighbors_merge() {
        // two 200-char paragraphs: 140 tokens each, combined 280 < 512
        let md = format!("# H\n\n{}\n\n{}", "甲".repeat(200), "乙".repeat(200));
        let chunks = chunk_markdown(ChunkSource::Card("n1".into()), "", &md);
        assert_eq!(chunks.len(), 1, "small paragraphs should merge into one chunk");
        assert!(chunks[0].text.contains("甲") && chunks[0].text.contains("乙"));
    }

    #[test]
    fn split_oversized_sentence_packing() {
        // one paragraph of long sentences, >512 tokens total
        let s = format!("句子内容{}。", "的".repeat(400));
        let para = format!("{s}{s}{s}");
        let pieces = split_oversized("", &para);
        assert!(pieces.len() >= 2);
        for p in &pieces {
            assert!(estimate_tokens(p) <= CHUNK_MAX_TOKENS);
        }
    }

    #[test]
    fn code_block_hashes_are_not_headings() {
        let md = "# 标题\n\n```rust\n# comment line\nfn main() {}\n```\n\n正文继续";
        let chunks = chunk_markdown(ChunkSource::Card("n1".into()), "", md);
        assert_eq!(chunks.len(), 1, "code block must not split the section");
        assert_eq!(chunks[0].heading, "标题");
        assert!(chunks[0].text.contains("# comment line"));
        assert!(chunks[0].text.contains("正文继续"));
    }

    #[test]
    fn small_mid_chunk_carries_into_next() {
        // para1 ≈ 200 tokens (small), para2 ≈ 500 tokens (oversized): the
        // small chunk must merge into para2's first piece, not stand alone.
        let para1 = "甲".repeat(280); // ~196 tokens
        let para2 = "乙".repeat(715); // ~500 tokens
        let md = format!("# H\n\n{para1}\n\n{para2}");
        let chunks = chunk_markdown(ChunkSource::Card("n1".into()), "", &md);
        assert!(chunks.len() >= 2, "got {}", chunks.len());
        for c in &chunks {
            assert!(
                estimate_tokens(&c.text) >= CHUNK_MIN_TOKENS || c.text.contains("甲"),
                "small standalone chunk left behind: {} tokens",
                estimate_tokens(&c.text)
            );
        }
    }

    #[test]
    fn card_chunks_carry_title_and_content_hash() {
        let c = chunk_markdown(ChunkSource::Card("n1".into()), "标题", "正文内容");
        assert_eq!(c[0].heading, "标题");
        assert_eq!(c[0].source.key(), "card:n1");
    }
}
