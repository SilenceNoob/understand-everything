use makepad_widgets::*;

use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::ai::{self};
use crate::app::{child_by_name, rag_bm25_context, RAG_CONTEXT_SLACK, RAG_RESYNC_SECS, RAG_RETRIEVE_TIMEOUT};
use crate::float_panel::FloatPanelWidgetRefExt;
use crate::mindmap::MindMapWidgetRefExt;
use crate::rag;
use crate::util::cached_widget;
use crate::App;

/// A send_chat deferred until the background RAG retrieval answers (or the
/// timeout falls back to the BM25 context computed at send time).
pub(crate) struct RagWait {
    query: String,
    retr: rag::service::RetrievalHandle,
    fallback: String,
    started: Instant,
}

/// Chat state + logic, extracted from App. `ui` mirrors App's widget handle;
/// `ai_config` is a copy the App syncs before forwarding so build_messages
/// and friends stay self-contained.
#[derive(Default)]
pub(crate) struct ChatController {
    pub(crate) ui: WidgetRef,
    pub(crate) chat_history: Vec<crate::chat_list::ChatMsg>,
    pub(crate) chat_extra: Vec<(String, String)>,
    pub(crate) ctx_warned: bool,
    pub(crate) chat_pending: bool,
    pub(crate) chat_id: LiveId,
    pub(crate) chat_buf: String,
    pub(crate) chat_think: String,
    pub(crate) chat_parser: crate::ai::SseParser,
    pub(crate) chat_list_ref: Option<WidgetRef>,
    pub(crate) ctx_row_ref: Option<WidgetRef>,
    pub(crate) input_row_ref: Option<WidgetRef>,
    pub(crate) tools_row_ref: Option<WidgetRef>,
    pub(crate) rag_wait: Option<RagWait>,
}
/// Lazy lookup of a child of the ai_panel content by name; a failed lookup
/// is never cached, so it retries.
fn cached_ai_child(cx: &Cx, ui: &WidgetRef, cache: &mut Option<WidgetRef>, id: LiveId) -> WidgetRef {
    cached_widget(cache, || {
        let content = ui.float_panel(cx, ids!(ai_panel)).content(cx);
        child_by_name(&content, id)
    })
    .unwrap_or_default()
}

/// Format a token count compactly: 860, 2.4K, 1M.
fn fmt_tokens(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.0}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
impl ChatController {
    /// Chat panel: send/stop/new-chat buttons and the input's focus/return
    /// actions (focus hands off the mindmap's keyboard shortcuts).
    pub(crate) fn handle_chat_actions(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
        rag: Option<&rag::service::RagService>,
        ai_config: &mut crate::ai::AIConfig,
    ) {
        let row = self.panel_input_row(cx);
        if child_by_name(&row, live_id!(send_btn))
            .as_button()
            .clicked(actions)
        {
            let text = child_by_name(&row, live_id!(chat_input))
                .as_text_input()
                .text();
            self.send_chat(cx, &text, rag, ai_config);
        }
        if child_by_name(&row, live_id!(stop_btn))
            .as_button()
            .clicked(actions)
        {
            self.stop_chat(cx, ai_config);
        }
        let tools = self.tools_row(cx);
        for (id, _) in ai::JIANGOU_SECTIONS {
            let base = LiveId::from_str(&format!("{id}_btn"));
            let on_id = LiveId::from_str(&format!("{id}_on_btn"));
            if child_by_name(&tools, base).as_button().clicked(actions)
                || child_by_name(&tools, on_id).as_button().clicked(actions)
            {
                let on = ai_config
                    .jiangou_sections
                    .iter()
                    .any(|s| s == id);
                if on {
                    ai_config.jiangou_sections.retain(|s| s != id);
                } else {
                    ai_config.jiangou_sections.push(id.to_string());
                }
                ai::save_config(&ai_config);
                self.sync_jiangou_btns(cx, ai_config);
                break;
            }
        }
        let header = self.panel_header(cx);
        if child_by_name(&header, live_id!(new_chat_btn))
            .as_button()
            .clicked(actions)
        {
            self.new_chat(cx, ai_config);
        }
        // While the AI chat input holds key focus, the mindmap must skip
        // its keyboard shortcuts (WASD/arrows/Space would otherwise fight
        // the typing).
        let chat_input = child_by_name(&row, live_id!(chat_input))
            .as_text_input();
        for action in actions.filter_widget_actions_cast::<TextInputAction>(chat_input.widget_uid())
        {
            match action {
                TextInputAction::KeyFocus => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(true, Ordering::Relaxed)
                }
                TextInputAction::KeyFocusLost => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed)
                }
                TextInputAction::Returned(text, _) => self.send_chat(cx, &text, rag, ai_config),
                _ => {}
            }
        }
    }
}
impl ChatController {
    /// Append a (role, content) message to the history and re-render.
    pub(crate) fn push_chat_msg(
        &mut self,
        cx: &mut Cx,
        role: &str,
        content: &str,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.chat_history.push(crate::chat_list::ChatMsg {
            role: role.to_string(),
            content: content.to_string(),
            thinking: String::new(),
            thinking_open: true,
        });
        self.render_msgs(cx, ai_config);
    }

    /// Push an assistant message that carries a thinking chain.
    pub(crate) fn push_chat_msg_thinking(
        &mut self,
        cx: &mut Cx,
        content: &str,
        thinking: &str,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.chat_history.push(crate::chat_list::ChatMsg {
            role: "assistant".to_string(),
            content: content.to_string(),
            thinking: thinking.to_string(),
            thinking_open: true,
        });
        self.render_msgs(cx, ai_config);
    }

    /// The ai_panel's ChatList widget, found by walking live children from
    /// the panel content (avoids the widget-tree graph, which does not index
    /// deep into FloatPanel subtrees).
    pub(crate) fn chat_list(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.chat_list_ref, live_id!(chat_list))
    }

    /// The ai_panel's ctx_row View (cached), via live children from the
    /// panel content.
    pub(crate) fn ctx_row(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.ctx_row_ref, live_id!(ctx_row))
    }

    /// The ai_panel's header View, via live children from the panel content.
    pub(crate) fn panel_header(&mut self, cx: &Cx) -> WidgetRef {
        let content = self.ui.float_panel(cx, ids!(ai_panel)).content(cx);
        child_by_name(&content, live_id!(header))
    }

    /// The ai_panel's input_row View (cached), via live children from the
    /// panel content.
    pub(crate) fn panel_input_row(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.input_row_ref, live_id!(input_row))
    }

    /// The ai_panel's tools_row View (cached), via live children from the
    /// panel content.
    pub(crate) fn tools_row(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.tools_row_ref, live_id!(tools_row))
    }
}
impl ChatController {
    /// Estimated tokens of the whole history.
    pub(crate) fn context_tokens(&self) -> usize {
        self.chat_history
            .iter()
            .map(|m| ai::estimate_tokens(&m.content) + ai::estimate_tokens(&m.thinking))
            .sum()
    }

    /// Refresh the "Context: N / 1M (P%)" label (gray, plus a one-shot 80%
    /// warning bubble).
    pub(crate) fn update_ctx_label(&mut self, cx: &mut Cx, ai_config: &crate::ai::AIConfig) {
        let tokens = self.context_tokens();
        let pct = tokens as f64 / ai::CONTEXT_WINDOW as f64;
        let warned = pct >= ai::WARN_RATIO;
        let used = fmt_tokens(tokens);
        let full = fmt_tokens(ai::CONTEXT_WINDOW);
        let text = if pct >= 1.0 {
            format!("Context: {used} / {full} (100%) — full, start a new chat")
        } else {
            format!("Context: {used} / {full} ({:.1}%)", pct * 100.0)
        };
        let ctx_row = self.ctx_row(cx);
        let label = child_by_name(&ctx_row, live_id!(ctx_label)).as_label();
        label.set_text(cx, &text);
        label.set_text_color(cx, Vec4f::from_u32(0x7a8192ff));
        child_by_name(&ctx_row, live_id!(model_label))
            .as_label()
            .set_text(cx, &format!("Model: {}", ai_config.model));
        child_by_name(&ctx_row, live_id!(thinking_label))
            .as_label()
            .set_text(cx, &format!("· thinking: {}", ai_config.thinking));
        // One-shot bubble when crossing the warning threshold.
        if warned && !self.ctx_warned {
            self.ctx_warned = true;
            self.chat_extra.push((
                "assistant".to_string(),
                format!(
                    "Context usage reached {:.0}% — consider starting a new chat soon.",
                    pct * 100.0
                ),
            ));
            self.render_msgs(cx, ai_config);
        } else if !warned {
            self.ctx_warned = false;
        }
    }

    /// Sync the ChatList widget: history + in-flight reply + UI-only extras.
    pub(crate) fn render_msgs(&mut self, cx: &mut Cx, ai_config: &crate::ai::AIConfig) {
        let mut display: Vec<crate::chat_list::ChatMsg> = self.chat_history.clone();
        if self.chat_pending {
            display.push(crate::chat_list::ChatMsg {
                role: "assistant".to_string(),
                content: format!("思考中…{}", self.chat_buf),
                thinking: self.chat_think.clone(),
                thinking_open: true,
            });
        }
        for (role, content) in &self.chat_extra {
            display.push(crate::chat_list::ChatMsg {
                role: role.clone(),
                content: content.clone(),
                thinking: String::new(),
                thinking_open: true,
            });
        }
        if let Some(mut list) = self.chat_list(cx).borrow_mut::<crate::chat_list::ChatList>() {
            list.set_msgs(cx, &display);
        }
        self.update_ctx_label(cx, ai_config);
        self.sync_send_btn(cx);
        self.sync_jiangou_btns(cx, ai_config);
    }

    /// Send the chat input text: append to history and stream a request.
    /// Refuses (with a hint) when the context window is full.
    pub(crate) fn send_chat(
        &mut self,
        cx: &mut Cx,
        text: &str,
        rag: Option<&rag::service::RagService>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let text = text.trim();
        if text.is_empty() || self.chat_pending {
            return;
        }
        // BM25 context first (µs): the gauge must count what will actually
        // be injected, plus slack for the async hybrid upgrade.
        let ctx = rag_bm25_context(rag, text);
        let upgradeable = rag.is_some_and(|r| r.models().is_some_and(|m| m.embedding_ready()));
        let rag_tokens = ai::estimate_tokens(&ctx)
            + if upgradeable { RAG_CONTEXT_SLACK } else { 0 };
        let would_use = self.context_tokens() + ai::estimate_tokens(text) + rag_tokens;
        if would_use >= ai::CONTEXT_WINDOW {
            if !self
                .chat_extra
                .iter()
                .any(|(_, c)| c.contains("Context is full"))
            {
                self.chat_extra.push((
                    "assistant".to_string(),
                    "Context is full — click + to start a new chat.".to_string(),
                ));
                self.render_msgs(cx, ai_config);
            }
            return;
        }
        self.push_chat_msg(cx, "user", text, ai_config);
        let row = self.panel_input_row(cx);
        child_by_name(&row, live_id!(chat_input))
            .as_text_input()
            .set_text(cx, "");
        if ai_config.api_key.trim().is_empty() {
            self.push_chat_msg(cx, "assistant", "请先在 Setting 中配置 API Key", ai_config);
            return;
        }
        self.chat_pending = true;
        self.chat_buf.clear();
        self.chat_think.clear();
        self.chat_parser = ai::SseParser::new();
        // RAG context: sync BM25 always (fast path); when the models are
        // ready, defer the request until the background hybrid retrieval
        // answers (or times out), so citations reflect reranked results.
        let mut defer = false;
        if let Some(rag) = rag {
            if upgradeable {
                let retr = rag.retrieve(text);
                self.rag_wait = Some(RagWait {
                    query: text.to_string(),
                    retr,
                    fallback: ctx.clone(),
                    started: Instant::now(),
                });
                defer = true;
            }
        }
        if !defer {
            self.fire_chat(cx, self.build_messages(&ctx, ai_config), ai_config);
        }
    }

    /// The messages for the next request: chat history plus (when enabled)
    /// the 渐构 format instruction and the RAG context, both as system
    /// messages (injected per request, never stored in chat_history).
    pub(crate) fn build_messages(
        &self,
        ctx: &str,
        ai_config: &crate::ai::AIConfig,
    ) -> Vec<(String, String)> {
        let mut messages: Vec<(String, String)> = self
            .chat_history
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        if !ai_config.jiangou_sections.is_empty() {
            messages.insert(
                0,
                (
                    "system".to_string(),
                    ai::jiangou_format_prompt(&ai_config.jiangou_sections),
                ),
            );
        }
        if !ctx.is_empty() {
            messages.insert(0, ("system".to_string(), ctx.to_string()));
        }
        messages
    }

    /// Fire the chat request with the given (system-prefixed) messages.
    pub(crate) fn fire_chat(
        &mut self,
        cx: &mut Cx,
        messages: Vec<(String, String)>,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.chat_id = LiveId::unique();
        ai::chat_stream_request(cx, self.chat_id, &ai_config, &messages);
        self.render_msgs(cx, ai_config);
    }

    /// Synchronous BM25-only context from the shared index (µs; the
    /// retrieval worker's hybrid upgrade replaces it when available).
    /// rag_wait polling (called from App::handle_rag_tick): fire the deferred
    /// chat once its background retrieval answers or times out.
    pub(crate) fn poll_rag_wait(&mut self, cx: &mut Cx, ai_config: &crate::ai::AIConfig) {
        let now = Instant::now();
        let Some(wait) = &mut self.rag_wait else {
            return;
        };
        let hits = match wait.retr.rx.try_recv() {
            Ok(r) if r.query == wait.query => Some(r.hits),
            Ok(_) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if now.duration_since(wait.started) > RAG_RETRIEVE_TIMEOUT {
                    // Chat fires on the BM25 fallback; stop the worker from
                    // finishing a result nobody reads.
                    wait.retr.cancel();
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Err(_) => Some(Vec::new()),
        };
        let Some(hits) = hits else {
            return;
        };
        let fallback = wait.fallback.clone();
        self.rag_wait = None;
        let ctx = if hits.is_empty() {
            fallback
        } else {
            rag::service::format_context(&hits)
        };
        self.fire_chat(cx, self.build_messages(&ctx, ai_config), ai_config);
    }

    /// Start a fresh conversation: drop all history and extras.
    pub(crate) fn new_chat(&mut self, cx: &mut Cx, ai_config: &crate::ai::AIConfig) {
        if self.chat_pending {
            cx.cancel_http_request(self.chat_id);
        }
        if let Some(wait) = &self.rag_wait {
            wait.retr.cancel();
        }
        self.rag_wait = None;
        self.chat_history.clear();
        self.chat_extra.clear();
        self.chat_buf.clear();
        self.chat_think.clear();
        self.chat_pending = false;
        self.ctx_warned = false;
        self.render_msgs(cx, ai_config);
    }

    /// Cancel the in-flight reply, keeping whatever text arrived so far.
    pub(crate) fn stop_chat(&mut self, cx: &mut Cx, ai_config: &crate::ai::AIConfig) {
        if !self.chat_pending {
            return;
        }
        self.chat_pending = false;
        if let Some(wait) = &self.rag_wait {
            wait.retr.cancel();
        }
        self.rag_wait = None;
        cx.cancel_http_request(self.chat_id);
        if !self.chat_buf.is_empty() {
            let buf = std::mem::take(&mut self.chat_buf);
            let think = std::mem::take(&mut self.chat_think);
            self.push_chat_msg_thinking(cx, &buf, &think, ai_config);
        } else {
            self.chat_think.clear();
        }
        self.render_msgs(cx, ai_config);
    }

    /// Send/stop button: show the stop icon while a reply streams.
    pub(crate) fn sync_send_btn(&mut self, cx: &mut Cx) {
        let row = self.panel_input_row(cx);
        child_by_name(&row, live_id!(send_btn))
            .set_visible(cx, !self.chat_pending);
        child_by_name(&row, live_id!(stop_btn))
            .set_visible(cx, self.chat_pending);
    }

    /// 渐构 section pills: swap each gray/blue pair by its enabled state.
    pub(crate) fn sync_jiangou_btns(&mut self, cx: &mut Cx, ai_config: &crate::ai::AIConfig) {
        let tools = self.tools_row(cx);
        for (id, _) in ai::JIANGOU_SECTIONS {
            let on = ai_config
                .jiangou_sections
                .iter()
                .any(|s| s == id);
            let base = LiveId::from_str(&format!("{id}_btn"));
            let on_id = LiveId::from_str(&format!("{id}_on_btn"));
            child_by_name(&tools, base).set_visible(cx, !on);
            child_by_name(&tools, on_id).set_visible(cx, on);
        }
    }
}

impl App {
    /// Thin forwarding shims: the App surface used by http.rs / ui.rs /
    /// files.rs / mindmap_actions.rs stays unchanged; chat state lives in
    /// the ChatController.
    pub(crate) fn push_chat_msg(&mut self, cx: &mut Cx, role: &str, content: &str) {
        self.chat.push_chat_msg(cx, role, content, &self.ai_config);
    }

    pub(crate) fn push_chat_msg_thinking(&mut self, cx: &mut Cx, content: &str, thinking: &str) {
        self.chat.push_chat_msg_thinking(cx, content, thinking, &self.ai_config);
    }

    pub(crate) fn update_ctx_label(&mut self, cx: &mut Cx) {
        self.chat.update_ctx_label(cx, &self.ai_config);
    }

    pub(crate) fn render_msgs(&mut self, cx: &mut Cx) {
        self.chat.render_msgs(cx, &self.ai_config);
    }

    pub(crate) fn sync_jiangou_btns(&mut self, cx: &mut Cx) {
        self.chat.sync_jiangou_btns(cx, &self.ai_config);
    }

    pub(crate) fn handle_chat_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        self.chat
            .handle_chat_actions(cx, actions, self.rag.as_ref(), &mut self.ai_config);
    }

    /// Status text for the ai_panel header label; "" when idle and ready.
    pub(crate) fn rag_status_text(&self) -> String {
        if self.chat.rag_wait.is_some() {
            return "检索中…".to_string();
        }
        let Some(rag) = &self.rag else {
            return String::new();
        };
        let Some(models) = rag.models() else {
            return "模型准备中…".to_string();
        };
        let st = models.status.read().unwrap();
        match &*st {
            rag::ModelStatus::Downloading(f) => format!("下载模型 {f}…"),
            rag::ModelStatus::Loading => "加载模型…".to_string(),
            rag::ModelStatus::Failed(e) => format!("RAG 不可用: {e}"),
            rag::ModelStatus::Ready => {
                if rag.indexing() {
                    "索引中…".to_string()
                } else {
                    String::new()
                }
            }
        }
    }

    /// Periodic rag timer: re-sync the index from disk, refresh the status
    /// label, poll the deferred retrieval waits.
    pub(crate) fn handle_rag_tick(&mut self, cx: &mut Cx) {
        let now = Instant::now();
        let due = self
            .last_resync
            .map(|t| now.duration_since(t).as_secs() >= RAG_RESYNC_SECS)
            .unwrap_or(false);
        if self.map_opened && due {
            self.last_resync = Some(now);
            if let Some(rag) = &self.rag {
                if let Some(map) = self.ui.mind_map(cx, ids!(mindmap)).current_map_file() {
                    rag.set_map(&map);
                }
            }
        }
        let text = self.rag_status_text();
        if text != self.last_rag_label {
            self.last_rag_label = text.clone();
            self.ui.label(cx, ids!(rag_status)).set_text(cx, &text);
        }
        self.handle_gen_rag_tick(cx);
        self.handle_route_rag_tick(cx);
        self.handle_route_progress_toast(cx);
        self.chat.poll_rag_wait(cx, &self.ai_config);
    }
}
