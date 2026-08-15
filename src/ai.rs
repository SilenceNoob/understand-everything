use makepad_widgets::*;

use serde::{Deserialize, Serialize};

use crate::util::data_dir;

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

/// The 渐构 concept-card sections, in display order: (id, tag line). The
/// ai_panel shows one pill button per section; enabled ids are persisted in
/// settings.json and the prompt is built from them. Ids match the card
/// generation sections, so both surfaces share the same tag-line format.
pub const JIANGOU_SECTIONS: [(&str, &str); 7] = [
    ("desc", "#d {总结标题}"),
    ("plain", "#t {总结标题}"),
    ("pos", "#e {例子名}(正例)"),
    ("neg", "#e {例子名}(负例)"),
    ("use", "#c 作用 {短标题}"),
    ("affect", "#c influence_to {短标题}"),
    ("affected", "#c influenced_by {短标题}"),
];

/// Instruction for one enabled section: (id, section body).
fn jiangou_section_instruction(id: &str) -> &'static str {
    match id {
        "desc" => "抽象描述：开头写「概念可以通过以下特征来定义：」，随后用 * 逐条罗列判别特征（特征名：说明）。这些特征是判断任意对象是否属于此概念的判别依据，全部满足才归为此概念；不要写成散文式定义。",
        "plain" => "通俗描述：用大白话和生活化比喻解释这个概念，让外行也能看懂。",
        "pos" => "1~3 个正例板块（视问题复杂度而定），每个板块一个例子：满足全部特征的具体现象。非代码类概念：先散文描述现象，再写「特征对比」，逐条指出该现象如何满足每个特征。代码相关概念（编程语言特性、API、设计模式等）：先给出例子代码片段，再简要解释这段代码在做什么，最后写「特征对比」，逐条指出该代码如何满足每个特征。",
        "neg" => "1~2 个负例板块，每个板块一个例子：与正例相似但缺失某个关键特征的具体现象，指出它违反了哪些特征。代码相关概念用反例代码呈现（通常无法编译或行为不符合预期）：先给出反例代码，再逐条指出它违反了哪些特征；非代码类概念用散文描述现象。",
        "use" => "作用：学会这个概念后有什么用处（能判别什么问题、指导什么实践）。",
        "affect" => "此概念会影响哪些事物，用 * 逐条罗列。",
        "affected" => "哪些事物会影响此概念，用 * 逐条罗列。",
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
/// the enabled sections as tag lines. Injected on every request while any
/// section is on (never stored in chat_history).
pub fn jiangou_format_prompt(enabled: &[String]) -> String {
    let mut out = String::from(
        "当用户的问题是在解释或学习某个概念时，请按以下板块回答。标签行格式如下（「总结标题」「例子名」「短标题」由你自拟，概括本节内容，不要照搬问题文字）：\n",
    );
    for (id, title) in JIANGOU_SECTIONS {
        if enabled.iter().any(|s| s == id) {
            out.push_str(&format!("{title}\n{}\n", jiangou_section_instruction(id)));
        }
    }
    out.push_str(
        "要求：每个标签行独占一行，内容可占多行，板块之间空一行；每个板块简短精炼；回答优先依据提供的参考资料和卡片内容，并在引用处标注 [编号]；参考资料不足时明确说明并基于自己的知识补充；若用户的问题不是概念解释类（例如编程任务、总结、闲聊），则直接正常回答，不要套用上述格式。",
    );
    out
}

/// Load settings.json; a missing or malformed file yields the defaults.
pub fn load_config() -> AIConfig {
    std::fs::read_to_string(data_dir().join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &AIConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(data_dir().join("settings.json"), json);
    }
}

/// Options for non-streaming requests that need a structured JSON answer.
#[derive(Clone, Copy)]
pub struct StructuredRequest<'a> {
    pub max_tokens: usize,
    /// Ask the OpenAI-compatible endpoint for JSON object output. Some
    /// compatible gateways reject `response_format`; callers fall back to
    /// a normal request on HTTP 400/422.
    pub json_mode: bool,
    /// Override the user's configured thinking strength for this request
    /// (structured extraction works better with less reasoning); None uses
    /// the configured value.
    pub thinking: Option<&'a str>,
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
    chat_completions_structured(
        cx,
        request_id,
        config,
        messages,
        StructuredRequest {
            max_tokens,
            json_mode: false,
            thinking: None,
        },
    );
}

/// Non-streaming request with structured-output options (`json_mode`,
/// thinking override, explicit token cap).
pub fn chat_completions_structured(
    cx: &mut Cx,
    request_id: LiveId,
    config: &AIConfig,
    messages: &[(String, String)],
    options: StructuredRequest<'_>,
) {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut payload = serde_json::json!({
        "model": config.model,
        "messages": messages.iter().map(|(role, content)| serde_json::json!({"role": role, "content": content})).collect::<Vec<_>>(),
        "max_tokens": options.max_tokens,
        "stream": false,
        (THINKING_PARAM.to_string()): options.thinking.unwrap_or(config.thinking.as_str()),
    });
    if options.json_mode {
        payload["response_format"] = serde_json::json!({"type": "json_object"});
    }
    send_chat_request(cx, request_id, config, url, payload.to_string(), false);
}

/// Send a streaming chat/completions request. Raw SSE bytes arrive via
/// `MatchEvent::handle_http_stream`, the final status via
/// `handle_http_stream_complete` (both keyed by `request_id`).
pub fn chat_stream_request(cx: &mut Cx, request_id: LiveId, config: &AIConfig, messages: &[(String, String)]) {
    chat_stream_request_max(cx, request_id, config, messages, 4096);
}

/// Streaming variant with an explicit max_tokens cap (route planning
/// outputs more than the chat default).
pub fn chat_stream_request_max(
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

/// Extract the assistant `(content, reasoning_content)` pair from a
/// non-streaming chat/completions response.
pub fn response_message_parts(response: &HttpResponse) -> Option<(String, String)> {
    if response.status_code != 200 {
        return None;
    }
    let body = response.get_string_body()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let msg = v.get("choices")?.get(0)?.get("message")?;
    let content = msg
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let reasoning = msg
        .get("reasoning_content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    Some((content, reasoning))
}

/// Extract the assistant content from a non-streaming chat/completions response.
pub fn response_content(response: &HttpResponse) -> Option<String> {
    response_message_parts(response).map(|(content, _)| content)
}

/// Text for structured JSON responses: the normal content, or the reasoning
/// chain when the model left `content` empty (a thinking model can emit the
/// final JSON inside `reasoning_content` on structured prompts).
pub fn response_structured_text(response: &HttpResponse) -> String {
    let (content, reasoning) = response_message_parts(response).unwrap_or_default();
    if content.trim().is_empty() {
        reasoning
    } else {
        content
    }
}

/// A short printable preview of arbitrary model text (for repair prompts).
pub fn text_preview(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.is_empty() {
        return "<空>".to_string();
    }
    if text.chars().count() > max_chars {
        text.chars().take(max_chars).collect::<String>() + "…"
    } else {
        text.to_string()
    }
}

/// Diagnostics for a failed/unparseable generation: finish_reason plus a
/// ~200-char preview of the content (or the reasoning content when the
/// content itself is empty), so a truncation is distinguishable from a
/// format miss. When the body isn't JSON at all (SSE despite stream:false,
/// an error page, …), falls back to a raw body prefix.
pub fn response_debug_preview(response: &HttpResponse) -> String {
    let body = response.get_string_body().unwrap_or_default();
    let v: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            let raw = body.trim();
            let raw = if raw.chars().count() > 200 {
                raw.chars().take(200).collect::<String>() + "…"
            } else {
                raw.to_string()
            };
            return format!("body 非 JSON，原始内容：{raw}");
        }
    };
    let choice = v.get("choices").and_then(|c| c.get(0));
    let finish = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .unwrap_or("?");
    let msg = choice.and_then(|c| c.get("message"));
    let content = msg.and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("");
    let preview = if content.trim().is_empty() {
        msg.and_then(|m| m.get("reasoning_content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
    } else {
        content
    };
    let preview = preview.trim();
    let preview = if preview.is_empty() {
        "<空>".to_string()
    } else if preview.chars().count() > 200 {
        preview.chars().take(200).collect::<String>() + "…"
    } else {
        preview.to_string()
    };
    format!("finish_reason={finish}, 内容预览：{preview}")
}
/// Incremental decoder for `Transfer-Encoding: chunked` HTTP bodies.
/// makepad's Linux streaming backend forwards raw socket bytes, chunk
/// framing included (only the non-streaming path decodes chunked — see
/// platform/network/src/backend/linux/http.rs). Auto-detects: the first
/// complete line decides — a pure-hex chunk size (with optional `;ext`)
/// means chunked mode; a bare `\n` or any non-hex first line means
/// passthrough forever (plain SSE, or backends that already decoded).
#[derive(Default)]
struct ChunkDecoder {
    /// None = undecided, Some(false) = passthrough, Some(true) = chunked.
    mode: Option<bool>,
    /// Partial chunk-size line held across feeds.
    size_buf: Vec<u8>,
    /// Payload bytes of the current chunk still to emit.
    remaining: usize,
    /// CRLF after a chunk payload still to swallow (0..=2).
    crlf: u8,
    /// 0-size chunk seen: swallow everything after it (trailers).
    done: bool,
    out: Vec<u8>,
}

/// Parse a chunk-size line (`<hex>` with optional `;ext`); None when invalid.
fn parse_chunk_size(line: &[u8]) -> Option<usize> {
    let hex = line.split(|&b| b == b';').next()?;
    if hex.is_empty() || hex.len() > 16 {
        return None;
    }
    let mut size = 0usize;
    for &b in hex {
        size = size.checked_mul(16)?;
        size = size.checked_add(match b {
            b'0'..=b'9' => (b - b'0') as usize,
            b'a'..=b'f' => (b - b'a' + 10) as usize,
            b'A'..=b'F' => (b - b'A' + 10) as usize,
            _ => return None,
        })?;
    }
    Some(size)
}

fn find_crlf(hay: &[u8], from: usize) -> Option<usize> {
    hay[from..].windows(2).position(|w| w == b"\r\n").map(|p| p + from)
}

impl ChunkDecoder {
    fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.out.clear();
        if self.done {
            return std::mem::take(&mut self.out);
        }
        if self.mode == Some(false) {
            self.out.extend_from_slice(bytes);
            return std::mem::take(&mut self.out);
        }
        if self.mode.is_none() {
            // Hold bytes until the first complete line decides the framing.
            self.size_buf.extend_from_slice(bytes);
            // A bare \n (not \r\n) cannot be chunk framing: passthrough.
            if let Some(pos) = self.size_buf.iter().position(|&b| b == b'\n') {
                if pos == 0 || self.size_buf[pos - 1] != b'\r' {
                    self.mode = Some(false);
                    self.out = std::mem::take(&mut self.size_buf);
                    return std::mem::take(&mut self.out);
                }
            }
            match find_crlf(&self.size_buf, 0) {
                Some(pos) => match parse_chunk_size(&self.size_buf[..pos]) {
                    Some(size) => {
                        self.mode = Some(true);
                        self.remaining = size;
                        let rest = self.size_buf[pos + 2..].to_vec();
                        self.size_buf.clear();
                        self.process_chunked(&rest);
                    }
                    None => {
                        self.mode = Some(false);
                        self.out = std::mem::take(&mut self.size_buf);
                    }
                },
                None => {
                    // A chunk-size line is at most ~16 hex chars; anything
                    // longer without a CRLF cannot be chunked framing.
                    if self.size_buf.len() > 64 {
                        self.mode = Some(false);
                        self.out = std::mem::take(&mut self.size_buf);
                    }
                }
            }
            return std::mem::take(&mut self.out);
        }
        self.process_chunked(bytes);
        std::mem::take(&mut self.out)
    }

    /// Consume `bytes` as chunked framing, appending payload to `self.out`.
    fn process_chunked(&mut self, bytes: &[u8]) {
        let mut i = 0usize;
        while i < bytes.len() {
            if self.done {
                return;
            }
            if self.crlf > 0 {
                let n = (self.crlf as usize).min(bytes.len() - i);
                self.crlf -= n as u8;
                i += n;
                continue;
            }
            if self.remaining > 0 {
                let n = self.remaining.min(bytes.len() - i);
                self.out.extend_from_slice(&bytes[i..i + n]);
                self.remaining -= n;
                i += n;
                if self.remaining == 0 {
                    self.crlf = 2;
                }
                continue;
            }
            if self.size_buf.is_empty() {
                match find_crlf(bytes, i) {
                    Some(pos) => match parse_chunk_size(&bytes[i..pos]) {
                        Some(0) => {
                            self.done = true;
                            return;
                        }
                        Some(size) => {
                            self.remaining = size;
                            i = pos + 2;
                        }
                        None => {
                            // Corrupt framing: stop decoding and pass the
                            // rest through verbatim.
                            self.mode = Some(false);
                            self.out.extend_from_slice(&bytes[i..]);
                            return;
                        }
                    },
                    None => {
                        self.size_buf.extend_from_slice(&bytes[i..]);
                        return;
                    }
                }
            } else {
                self.size_buf.extend_from_slice(&bytes[i..]);
                match find_crlf(&self.size_buf, 0) {
                    Some(pos) => match parse_chunk_size(&self.size_buf[..pos]) {
                        Some(0) => {
                            self.done = true;
                            return;
                        }
                        Some(size) => {
                            self.remaining = size;
                            let rest = self.size_buf[pos + 2..].to_vec();
                            self.size_buf.clear();
                            self.process_chunked(&rest);
                            return;
                        }
                        None => {
                            self.mode = Some(false);
                            self.out.extend_from_slice(&self.size_buf);
                            self.size_buf.clear();
                            return;
                        }
                    },
                    None => return,
                }
            }
        }
    }
}

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
    /// Deframes chunked transfer encoding (Linux streaming backend).
    decoder: ChunkDecoder,
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
        let bytes = self.decoder.feed(bytes);
        self.raw.extend_from_slice(&bytes);
        self.buf.extend_from_slice(&bytes);
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

    #[test]
    fn chunk_decoder_deframes_at_every_split() {
        let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\ndata: [DONE]\n\n";
        let mut framed = format!("{:x}\r\n{}\r\n", payload.len(), payload).into_bytes();
        framed.extend_from_slice(b"0\r\n\r\n");
        // One shot.
        let mut d = ChunkDecoder::default();
        assert_eq!(d.feed(&framed), payload.as_bytes());
        // Split at every byte position across two feeds.
        for split in 0..framed.len() {
            let mut d = ChunkDecoder::default();
            let mut out = Vec::new();
            out.extend_from_slice(&d.feed(&framed[..split]));
            out.extend_from_slice(&d.feed(&framed[split..]));
            assert_eq!(out, payload.as_bytes(), "split at {split}");
        }
    }

    #[test]
    fn chunk_decoder_multiple_frames_and_trailers_swallowed() {
        let a = b"hello".to_vec();
        let b = b", world".to_vec();
        let mut framed = format!("{:x}\r\n", a.len()).into_bytes();
        framed.extend_from_slice(&a);
        framed.extend_from_slice(b"\r\n");
        framed.extend_from_slice(&format!("{:x}\r\n", b.len()).into_bytes());
        framed.extend_from_slice(&b);
        framed.extend_from_slice(b"\r\n0\r\n\r\ntrailer: x\r\n\r\n");
        let mut d = ChunkDecoder::default();
        let mut expect = a;
        expect.extend_from_slice(&b);
        assert_eq!(d.feed(&framed), expect);
        assert!(d.feed(b"after-end").is_empty());
    }

    #[test]
    fn chunk_decoder_passthrough_plain_stream() {
        // First feed ends mid-line: undecided, bytes are held.
        let sse = b"data: {\"x\":1}\n\ndata: [DONE]\n\n";
        let mut d = ChunkDecoder::default();
        let first = d.feed(&sse[..7]);
        assert!(first.is_empty());
        let mut all = first;
        all.extend_from_slice(&d.feed(&sse[7..]));
        assert_eq!(all, sse);
        // A tiny plain stream (under any hold threshold) still passes through.
        let mut d = ChunkDecoder::default();
        assert_eq!(d.feed(b"data: x\n\n"), b"data: x\n\n");
    }

    #[test]
    fn chunk_decoder_hex_extension_sizes() {
        let payload = b"data: x";
        let mut framed = format!("{:x};foo=1\r\n", payload.len()).into_bytes();
        framed.extend_from_slice(payload);
        framed.extend_from_slice(b"\r\n0\r\n\r\n");
        let mut d = ChunkDecoder::default();
        assert_eq!(d.feed(&framed), payload);
    }

    #[test]
    fn sse_parser_deframes_chunked_stream() {
        // The exact failure mode seen in the wild: DeepSeek's SSE arrives
        // chunked, and the Linux backend forwards the framing bytes inline.
        // A chunk boundary mid-`data:` line used to drop a delta entirely.
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"goal_input\\\":\\\"...\\\"\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"}\"}}]}\n\n\
                   data: [DONE]\n\n";
        let framed = format!("{:x}\r\n{}\r\n0\r\n\r\n", sse.len(), sse);
        let mut p = SseParser::new();
        let mut joined = String::new();
        for i in 0..framed.len() {
            let (content, _) = p.feed(framed.as_bytes().get(i..=i).unwrap());
            for delta in content {
                joined.push_str(&delta);
            }
        }
        assert_eq!(joined, "{\"goal_input\":\"...\"}");
    }
}
