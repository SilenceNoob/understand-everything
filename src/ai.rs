use makepad_widgets::*;

use serde::{Deserialize, Serialize};

use crate::util::app_base_dir;

/// DeepSeek defaults (OpenAI-compatible API; base_url has no /v1 suffix).
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";

/// Model context window in tokens (1M is the current standard for LLMs);
/// the chat panel shows usage against this and warns near the limit.
pub const CONTEXT_WINDOW: usize = 1_000_000;
/// Fraction of the context window that triggers the "usage high" warning.
pub const WARN_RATIO: f64 = 0.8;

/// Thinking-strength levels (DeepSeek thinking mode): xhigh maps to high on
/// deepseek-v4-flash but is accepted as-is by the API.
pub const THINKING_LEVELS: [&str; 4] = ["low", "high", "xhigh", "max"];
/// Request parameter carrying the thinking strength (OpenAI format).
pub const THINKING_PARAM: &str = "reasoning_effort";

fn default_thinking() -> String {
    "max".to_string()
}

/// The 渐构 concept-card sections, in display order: (id, title). The
/// ai_panel shows one pill button per section; enabled ids are persisted in
/// settings.json and the prompt is built from them.
pub const JIANGOU_SECTIONS: [(&str, &str); 7] = [
    ("desc", "#d 标准描述"),
    ("plain", "#t 通俗描述"),
    ("pos", "#e 正面例子"),
    ("neg", "#e 反面例子"),
    ("affect", "#n 影响什么"),
    ("affected", "#n 被啥影响"),
    ("use", "#n 概念作用"),
];

/// Instruction for one enabled section: (id, section body).
fn jiangou_section_instruction(id: &str) -> &'static str {
    match id {
        "desc" => "用一两句话给出准确严谨的定义。",
        "plain" => "用生活化的比喻或大白话解释这个概念。",
        "pos" => "给出一个符合该概念的正面例子。",
        "neg" => "给出一个不符合该概念的负面例子，并说明为什么不符合。",
        "affect" => "说明这个概念会支持、影响或催生什么。",
        "affected" => "说明什么因素会影响、决定或催生这个概念。",
        "use" => "说明学会这个概念后有什么用，可以用在哪些场景。",
        _ => "",
    }
}

fn default_jiangou_sections() -> Vec<String> {
    JIANGOU_SECTIONS.iter().map(|(id, _)| id.to_string()).collect()
}

/// AI provider config, persisted as settings.json in the app base dir.
#[derive(Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_thinking")]
    pub thinking: String,
    /// Enabled 渐构 format sections (ids from JIANGOU_SECTIONS); empty =
    /// normal chat, non-empty = concept answers follow the enabled sections.
    #[serde(default = "default_jiangou_sections")]
    pub jiangou_sections: Vec<String>,
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            thinking: default_thinking(),
            jiangou_sections: default_jiangou_sections(),
        }
    }
}

/// System-prompt instruction for 渐构-style concept answers, listing only
/// the enabled sections. Injected on every request while any section is on
/// (never stored in chat_history).
pub fn jiangou_format_prompt(enabled: &[String]) -> String {
    let mut out = String::from(
        "当用户的问题是在解释或学习某个概念时，请按以下板块回答，板块标题用 markdown 标题：\n",
    );
    for (id, title) in JIANGOU_SECTIONS {
        if enabled.iter().any(|s| s == id) {
            out.push_str(&format!(
                "## {title}\n{}\n",
                jiangou_section_instruction(id)
            ));
        }
    }
    out.push_str(
        "要求：每个板块简短精炼；回答优先依据提供的参考资料和卡片内容，并在引用处标注 [编号]；参考资料不足时明确说明并基于自己的知识补充；若用户的问题不是概念解释类（例如编程任务、总结、闲聊），则直接正常回答，不要套用上述格式。",
    );
    out
}

/// Load settings.json; a missing or malformed file yields the defaults.
pub fn load_config() -> AIConfig {
    std::fs::read_to_string(app_base_dir().join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &AIConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(app_base_dir().join("settings.json"), json);
    }
}

/// Send a chat/completions request (non-streaming). `messages` is
/// (role, content) pairs; the response arrives via
/// `MatchEvent::handle_http_response` keyed by `request_id`.
pub fn chat_completions(
    cx: &mut Cx,
    request_id: LiveId,
    config: &AIConfig,
    messages: &[(String, String)],
    max_tokens: usize,
) {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": config.model,
        "messages": messages.iter().map(|(role, content)| serde_json::json!({"role": role, "content": content})).collect::<Vec<_>>(),
        "max_tokens": max_tokens,
        "stream": false,
        (THINKING_PARAM.to_string()): config.thinking,
    })
    .to_string();
    send_chat_request(cx, request_id, config, url, body, false);
}

/// Send a streaming chat/completions request. Raw SSE bytes arrive via
/// `MatchEvent::handle_http_stream`, the final status via
/// `handle_http_stream_complete` (both keyed by `request_id`).
pub fn chat_stream_request(cx: &mut Cx, request_id: LiveId, config: &AIConfig, messages: &[(String, String)]) {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": config.model,
        "messages": messages.iter().map(|(role, content)| serde_json::json!({"role": role, "content": content})).collect::<Vec<_>>(),
        "max_tokens": 4096,
        "stream": true,
        (THINKING_PARAM.to_string()): config.thinking,
    })
    .to_string();
    send_chat_request(cx, request_id, config, url, body, true);
}

fn send_chat_request(cx: &mut Cx, request_id: LiveId, config: &AIConfig, url: String, body: String, stream: bool) {
    let mut http = HttpRequest::new(url, HttpMethod::POST);
    http.set_header("Content-Type".to_string(), "application/json".to_string());
    http.set_header(
        "Authorization".to_string(),
        format!("Bearer {}", config.api_key),
    );
    http.set_string_body(body);
    if stream {
        http.set_is_streaming();
    }
    cx.http_request(request_id, http);
}

/// Send a minimal non-streaming chat request to verify key/base_url/model.
pub fn test_request(cx: &mut Cx, request_id: LiveId, config: &AIConfig) {
    chat_completions(
        cx,
        request_id,
        config,
        &[("user".to_string(), "ping".to_string())],
        4,
    );
}

/// Rough token estimate for context-usage accounting (no tokenizer on hand):
/// CJK chars ≈ 0.7 tokens, ASCII ≈ 0.3 tokens. Good enough for a gauge.
pub fn estimate_tokens(s: &str) -> usize {
    s.chars().map(|c| if c.is_ascii() { 3 } else { 7 }).sum::<usize>() / 10
}

/// Extract `{"error":{"message":...}}` from an error response body.
pub fn body_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("error")?
        .get("message")?
        .as_str()
        .map(|s| s.to_string())
}

/// Incremental SSE parser for OpenAI-compatible streaming responses.
/// Feed raw bytes in arbitrary chunk sizes; every call returns the
/// assistant text deltas extracted since the previous call, in order.
/// Also accumulates the raw text so error bodies stay recoverable.
#[derive(Default)]
pub struct SseParser {
    /// Byte tail not yet decoded (a UTF-8 char may be split across chunks).
    buf: Vec<u8>,
    /// Decoded text awaiting newline completion.
    line_buf: String,
    /// Every byte received, for error-body recovery on non-200.
    raw: Vec<u8>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raw text received so far (used to extract error JSON on non-200).
    pub fn raw(&self) -> String {
        String::from_utf8_lossy(&self.raw).into_owned()
    }

    /// Feed a chunk; returns (content deltas, thinking deltas) extracted
    /// since the previous call.
    pub fn feed(&mut self, bytes: &[u8]) -> (Vec<String>, Vec<String>) {
        self.raw.extend_from_slice(bytes);
        self.buf.extend_from_slice(bytes);
        let mut content = Vec::new();
        let mut thinking = Vec::new();
        loop {
            // Decode the longest complete-UTF-8 prefix; an incomplete char at
            // the tail waits for the next chunk.
            let decoded = match std::str::from_utf8(&self.buf) {
                Ok(s) => {
                    let s = s.to_string();
                    self.buf.clear();
                    s
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0 {
                        let s = String::from_utf8_lossy(&self.buf[..valid]).into_owned();
                        self.buf.drain(..valid);
                        s
                    } else if e.error_len().is_some() {
                        let s = String::from_utf8_lossy(&self.buf[..1]).into_owned();
                        self.buf.drain(..1);
                        s
                    } else {
                        break;
                    }
                }
            };
            if decoded.is_empty() {
                break;
            }
            self.line_buf.push_str(&decoded);
            while let Some(pos) = self.line_buf.find('\n') {
                let line = self.line_buf[..pos].trim_end_matches('\r').to_string();
                self.line_buf.drain(..=pos);
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                if let Some(delta) = delta_content(data) {
                    content.push(delta);
                }
                if let Some(delta) = delta_reasoning(data) {
                    thinking.push(delta);
                }
            }
        }
        (content, thinking)
    }
}

/// Pull `choices[0].delta.content` out of one SSE data payload.
fn delta_content(data: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    v.get("choices")?
        .get(0)?
        .get("delta")?
        .get("content")?
        .as_str()
        .map(|s| s.to_string())
}

/// Pull `choices[0].delta.reasoning_content` (the thinking chain) out of
/// one SSE data payload.
fn delta_reasoning(data: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    v.get("choices")?
        .get(0)?
        .get("delta")?
        .get("reasoning_content")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_old_json_defaults_thinking() {
        // settings.json written before the thinking field existed.
        let cfg: AIConfig =
            serde_json::from_str(r#"{"api_key":"k","base_url":"https://api.deepseek.com","model":"deepseek-v4-flash"}"#)
                .unwrap();
        assert_eq!(cfg.thinking, "max");
        // Roundtrip preserves the chosen level.
        let cfg2: AIConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg2.thinking, "max");
        assert!(THINKING_LEVELS.contains(&cfg2.thinking.as_str()));
    }

    #[test]
    fn estimate_tokens_mixed_text() {
        // "你好" (2 CJK chars ≈ 1.4) + "hello world" (11 ASCII ≈ 3.3)
        let t = estimate_tokens("你好hello world");
        assert!(t >= 4 && t <= 5, "got {t}");
        assert_eq!(estimate_tokens(""), 0);
        // 100 CJK chars ≈ 70 tokens
        assert_eq!(estimate_tokens(&"汉".repeat(100)), 70);
        // 100 ASCII chars ≈ 30 tokens
        assert_eq!(estimate_tokens(&"a".repeat(100)), 30);
    }

    #[test]
    fn sse_parser_joins_fragmented_chunks() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"，世界\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"！\"}}]}\n\n\
                   data: [DONE]\n\n";
        let mut p = SseParser::new();
        let mut joined = String::new();
        // Feed the stream a byte at a time so every boundary is exercised.
        for i in 0..sse.len() {
            let (content, thinking) = p.feed(sse.as_bytes().get(i..=i).unwrap());
            for delta in content {
                joined.push_str(&delta);
            }
            assert!(thinking.is_empty());
        }
        assert_eq!(joined, "你好，世界！");
    }

    #[test]
    fn sse_parser_extracts_thinking_and_content_streams() {
        let sse = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"让我先思考\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"，再回答\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"答案是 42\"}}]}\n\n\
                   data: [DONE]\n\n";
        let mut p = SseParser::new();
        let (content, thinking) = p.feed(sse.as_bytes());
        assert_eq!(content, vec!["答案是 42"]);
        assert_eq!(thinking, vec!["让我先思考", "，再回答"]);
    }

    #[test]
    fn sse_parser_skips_non_data_lines_and_error_payloads() {
        let sse = "event: ping\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n\
                   data: {\"error\":{\"message\":\"boom\"}}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n";
        let mut p = SseParser::new();
        let mut joined = String::new();
        let (content, _) = p.feed(sse.as_bytes());
        for delta in content {
            joined.push_str(&delta);
        }
        assert_eq!(joined, "ab");
        // The raw buffer keeps the whole body for error extraction.
        assert!(p.raw().contains("\"boom\""));
    }
}
