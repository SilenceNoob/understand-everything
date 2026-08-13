use makepad_widgets::*;

use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::ai::{self};
use crate::mindmap::MindMapWidgetRefExt;
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::App;
use crate::app::files::is_temp_map;
use crate::app::route::{RouteController, RoutePrefetch};
use crate::app::{popup_child, popup_widget, rag_bm25_context, show_toast};

/// Startup popup phase: goal input vs the adaptive diagnostic interview.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupPhase {
    Goal,
    Diag,
}

/// Upper bound for the interview: after this many answered rounds the
/// route is planned from the transcript alone (no further questions).
const MAX_DIAG_ROUNDS: usize = 6;

/// Diagnostic-interview state + logic, extracted from App.
#[derive(Default)]
pub(crate) struct DiagController {
    pub(crate) ui: WidgetRef,
    pub(crate) diag_id: LiveId,
    pub(crate) diag_goal: String,
    pub(crate) diag_history: Vec<(crate::gen::DiagQuestion, String)>,
    pub(crate) diag_current: Option<crate::gen::DiagQuestion>,
    pub(crate) diag_single: Option<usize>,
    pub(crate) diag_multi: [bool; 4],
    pub(crate) diag_retried: bool,
    /// In-flight map-naming request (startup goal input; the temp map is
    /// renamed to the AI's answer when it lands).
    pub(crate) map_name_id: LiveId,
    /// The goal the pending naming request was fired for (fallback name).
    pub(crate) map_name_goal: String,
}
impl DiagController {
    pub(crate) fn handle_startup_actions(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let popup = popup_widget(&self.ui, live_id!(startup_popup));
        if !popup.as_popup_panel().opened() {
            return;
        }
        let input = popup_child(
            &self.ui,
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(goal_view), live_id!(input_row), live_id!(start_input)],
            )
            .as_text_input();
        for action in actions.filter_widget_actions_cast::<TextInputAction>(input.widget_uid()) {
            match action {
                TextInputAction::KeyFocus => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(true, Ordering::Relaxed)
                }
                TextInputAction::KeyFocusLost => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed)
                }
                TextInputAction::Returned(text, _) => self.submit_concept(cx, &text, route, rag, toast_until, ai_config),
                _ => {}
            }
        }
        if popup_child(
            &self.ui,
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(goal_view), live_id!(input_row), live_id!(start_send_btn)],
            )
            .as_button()
            .clicked(actions)
        {
            self.submit_concept(cx, &input.text(), route, rag, toast_until, ai_config);
        }
        // Diagnostic interview: answer input focus, option toggles, submit.
        let diag_input = popup_child(
            &self.ui,
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box), live_id!(diag_input)],
            )
            .as_text_input();
        for action in actions.filter_widget_actions_cast::<TextInputAction>(diag_input.widget_uid()) {
            match action {
                TextInputAction::KeyFocus => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(true, Ordering::Relaxed)
                }
                TextInputAction::KeyFocusLost => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed)
                }
                TextInputAction::Returned(_, _) => self.submit_diag_answer(cx, route, rag, toast_until, ai_config),
                _ => {}
            }
        }
        for i in 0..4 {
            let off = popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button();
            if off.clicked(actions) {
                match self.diag_current.as_ref().map(|q| q.kind.as_str()) {
                    Some("single") => self.diag_single = Some(i),
                    Some("multi") => self.diag_multi[i] = true,
                    _ => {}
                }
                self.sync_diag_options(cx);
            }
            let on = popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button();
            if on.clicked(actions) {
                match self.diag_current.as_ref().map(|q| q.kind.as_str()) {
                    // clicking the already-selected option in single-choice keeps it
                    Some("multi") => {
                        self.diag_multi[i] = false;
                        self.sync_diag_options(cx);
                    }
                    _ => {}
                }
            }
        }
        if popup_child(
            &self.ui,
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row), live_id!(diag_submit_btn)],
            )
            .as_button()
            .clicked(actions)
        {
            self.submit_diag_answer(cx, route, rag, toast_until, ai_config);
        }
        if popup_child(
            &self.ui,
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row), live_id!(diag_unknown_btn)],
            )
            .as_button()
            .clicked(actions)
        {
            self.submit_diag_unknown(cx, route, rag, toast_until, ai_config);
        }
    }

    /// Start the adaptive diagnostic interview for `goal` (startup popup
    /// stays open, switches to the diag phase).
    pub(crate) fn submit_concept(
        &mut self,
        cx: &mut Cx,
        text: &str,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.begin_diag(cx, &text, route, rag, toast_until, ai_config);
    }

    /// Enter the diagnostic phase for `goal`: reset the session, switch the
    /// popup to the interview view, and fire the first question.
    pub(crate) fn begin_diag(
        &mut self,
        cx: &mut Cx,
        goal: &str,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        if self.diag_id != LiveId::empty() || goal.trim().is_empty() {
            return;
        }
        if ai_config.api_key.trim().is_empty() {
            route.close_startup(cx, self);
            show_toast(&self.ui, toast_until, cx, "请先在 Setting 中配置 API Key 再生成学习路线");
            return;
        }
        self.reset_diag();
        self.diag_goal = goal.trim().to_string();
        // The user has committed to a goal: the launch temp map stops being
        // temporary. Ask the model for a name; the temp file is renamed when
        // the reply lands (goal text as fallback).
        if is_temp_map(&self.ui.mind_map(cx, ids!(mindmap)).current_map_file().unwrap_or_default()) {
            self.map_name_goal = self.diag_goal.clone();
            self.map_name_id = LiveId::unique();
            let (system, user) = crate::gen::map_name_messages(&self.diag_goal);
            ai::chat_completions(cx, self.map_name_id, ai_config, &[("system".to_string(), system), ("user".to_string(), user)], 30);
        }
        // Prefetch the route-plan retrieval now: the query is just the goal,
        // so by the time the interview ends the result is ready and the
        // route request fires immediately. Only when the current map's index
        // actually has chunks (empty index would return no hits anyway).
        route.route_prefetch = None;
        let map_file = self.ui.mind_map(cx, ids!(mindmap)).current_map_file();
        if let (Some(rag), Some(map)) = (rag, map_file) {
            if rag.models().is_some_and(|m| m.embedding_ready()) && rag.has_chunks_for(&map) {
                let rx = rag.retrieve(&self.diag_goal);
                route.route_prefetch = Some(RoutePrefetch {
                    goal: self.diag_goal.clone(),
                    rx,
                    started: Instant::now(),
                });
            }
        }
        self.set_startup_phase(cx, StartupPhase::Diag);
        popup_widget(&self.ui, live_id!(startup_popup)).as_popup_panel().show(cx);
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_goal_label)],
        )
        .as_label()
        .set_text(cx, &format!("学习目标：{}", self.diag_goal));
        self.send_diag_request(cx, rag, ai_config);
    }

    /// Fire the next diagnostic question request (BM25 context only — the
    /// interview must stay snappy; the final route request uses hybrid RAG).
    pub(crate) fn send_diag_request(
        &mut self,
        cx: &mut Cx,
        rag: Option<&crate::rag::service::RagService>,
        ai_config: &crate::ai::AIConfig,
    ) {
        if self.diag_id != LiveId::empty() || self.diag_goal.is_empty() {
            return;
        }
        let goal = self.diag_goal.clone();
        let ctx = rag_bm25_context(rag, &goal);
        let (system, user) = crate::gen::diagnostic_messages(&goal, &ctx, &self.diag_history);
        self.diag_id = LiveId::unique();
        self.set_diag_status(cx, "正在出题…");
        // Clear the answered question so the popup shows a clean waiting
        // state until the next question arrives.
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_question)],
        )
        .as_label()
        .set_text(cx, "");
        for i in 0..4 {
            popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button()
                .set_visible(cx, false);
            popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button()
                .set_visible(cx, false);
        }
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box)],
        )
        .set_visible(cx, false);
        // The submit row stays hidden while 出题中 (clicking it with no
        // question loaded was a silent no-op); render_diag_question re-shows
        // it, and the failure paths re-show it as a retry entry.
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row)],
        )
        .set_visible(cx, false);
        ai::chat_completions(
            cx,
            self.diag_id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    pub(crate) fn handle_diag_response(
        &mut self,
        cx: &mut Cx,
        response: &HttpResponse,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.diag_id = LiveId::empty();
        // Popup closed mid-interview (user bailed): drop the session.
        if !popup_widget(&self.ui, live_id!(startup_popup)).as_popup_panel().opened() {
            self.reset_diag();
            return;
        }
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            self.set_diag_status(cx, &format!("出题失败 ({}): {}", response.status_code, detail));
            self.diag_unknown_btn(cx).set_visible(cx, false);
            self.diag_btn_row_visible(cx, true);
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match crate::gen::parse_diag_step(&content) {
            Ok(crate::gen::DiagStep::Question(q)) => {
                self.diag_current = Some(q);
                self.diag_single = None;
                self.diag_multi = [false; 4];
                self.render_diag_question(cx);
            }
            Ok(crate::gen::DiagStep::Done(summary)) => self.finish_diag(cx, &summary, route, rag, toast_until, ai_config),
            Err(e) => {
                // Empty/unparseable content is often a transient thinking-model
                // response; retry once before surfacing the failure.
                if !self.diag_retried {
                    self.diag_retried = true;
                    self.send_diag_request(cx, rag, ai_config);
                    return;
                }
                let debug = ai::response_debug_preview(response);
                self.set_diag_status(cx, &format!("出题解析失败：{e}（{debug}）。可点「提交」重试"));
                self.diag_unknown_btn(cx).set_visible(cx, false);
                self.diag_btn_row_visible(cx, true);
            }
        }
    }

    /// Render the current question in the popup: status, question text,
    /// option buttons (or the open-answer input).
    pub(crate) fn render_diag_question(&mut self, cx: &mut Cx) {
        let Some(q) = &self.diag_current else { return };
        let n = self.diag_history.len() + 1;
        let status = if q.target.is_empty() {
            format!("第 {n} 题")
        } else {
            format!("第 {n} 题 · 探测：{}", q.target)
        };
        self.set_diag_status(cx, &status);
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_question)],
        )
        .as_label()
        .set_text(cx, &q.question);
        let is_open = q.kind == "open";
        for i in 0..4 {
            let off = popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button();
            off.set_visible(cx, !is_open && i < q.options.len());
            popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button()
                .set_visible(cx, false);
        }
        // Toggle the container, not the TextInput (which ignores `visible`).
        let input_box = popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box)],
        );
        input_box.set_visible(cx, is_open);
        if is_open {
            popup_child(
            &self.ui,
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box), live_id!(diag_input)],
            )
            .as_text_input()
            .set_text(cx, "");
        }
        self.diag_btn_row_visible(cx, true);
        // 我不知道 is a choice-question escape hatch; hidden for open answers.
        self.diag_unknown_btn(cx).set_visible(cx, !is_open);
        self.sync_diag_options(cx);
    }

    /// Refresh the option buttons: text on both twins, selection shown by
    /// swapping to the highlighted on-variant (●/○ prefix is gone).
    pub(crate) fn sync_diag_options(&mut self, cx: &mut Cx) {
        let Some(q) = &self.diag_current else { return };
        for i in 0..q.options.len() {
            let selected = match q.kind.as_str() {
                "single" => self.diag_single == Some(i),
                "multi" => self.diag_multi[i],
                _ => false,
            };
            let off = popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button();
            let on = popup_child(&self.ui, live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button();
            off.set_text(cx, &q.options[i]);
            on.set_text(cx, &q.options[i]);
            off.set_visible(cx, !selected);
            on.set_visible(cx, selected);
        }
    }

    pub(crate) fn set_diag_status(&self, cx: &mut Cx, text: &str) {
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_status)],
        )
        .as_label()
        .set_text(cx, text);
    }

    pub(crate) fn diag_btn_row_visible(&self, cx: &mut Cx, visible: bool) {
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row)],
        )
        .set_visible(cx, visible);
    }

    /// The 我不知道 button inside the submit row (choice questions only).
    pub(crate) fn diag_unknown_btn(&self, _cx: &Cx) -> WidgetRef {
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row), live_id!(diag_unknown_btn)],
        )
    }

    pub(crate) fn opt_id(i: usize, on: bool) -> LiveId {
        match (i, on) {
            (0, false) => live_id!(opt0_off),
            (0, true) => live_id!(opt0_on),
            (1, false) => live_id!(opt1_off),
            (1, true) => live_id!(opt1_on),
            (2, false) => live_id!(opt2_off),
            (2, true) => live_id!(opt2_on),
            (3, false) => live_id!(opt3_off),
            _ => live_id!(opt3_on),
        }
    }

    /// The row view hosting option `i` (0/1 in row ab, 2/3 in row cd).
    pub(crate) fn opt_row_id(i: usize) -> LiveId {
        if i < 2 {
            live_id!(diag_opt_ab)
        } else {
            live_id!(diag_opt_cd)
        }
    }

    /// Full lookup path for an option button: every segment is a direct
    /// child of the previous one (startup-popup lookups are only reliable
    /// along direct-child chains).
    pub(crate) fn opt_path(i: usize, on: bool) -> [LiveId; 5] {
        [
            live_id!(content),
            live_id!(panel),
            live_id!(diag_view),
            Self::opt_row_id(i),
            Self::opt_id(i, on),
        ]
    }

    pub(crate) fn option_letter(i: usize) -> String {
        char::from(b'A' + i as u8).to_string()
    }

    /// Collect the user's answer for the current question (None when they
    /// haven't answered yet).
    pub(crate) fn collect_diag_answer(&mut self) -> Option<String> {
        let q = self.diag_current.as_ref()?;
        match q.kind.as_str() {
            "single" => self.diag_single.map(Self::option_letter),
            "multi" => {
                let sel: Vec<usize> = (0..4)
                    .filter(|&i| self.diag_multi[i] && i < q.options.len())
                    .collect();
                if sel.is_empty() {
                    None
                } else {
                    Some(
                        sel.iter()
                            .map(|&i| Self::option_letter(i))
                            .collect::<Vec<_>>()
                            .join(","),
                    )
                }
            }
            "open" => {
                let text = popup_child(
                    &self.ui,
                        live_id!(startup_popup),
                        &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box), live_id!(diag_input)],
                    )
                    .as_text_input()
                    .text();
                let t = text.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            _ => None,
        }
    }

    /// Append the current answer to the transcript and fire the next round;
    /// at the cap, plan the route from the transcript alone.
    pub(crate) fn submit_diag_answer(
        &mut self,
        cx: &mut Cx,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(q) = self.diag_current.clone() else {
            // No question loaded: the button acts as 重试出题 after a
            // question-request failure.
            if self.diag_id == LiveId::empty() {
                self.diag_retried = false;
                self.send_diag_request(cx, rag, ai_config);
            }
            return;
        };
        let Some(ans) = self.collect_diag_answer() else {
            self.set_diag_status(
                cx,
                if q.kind == "open" {
                    "请输入你的回答"
                } else {
                    "请先选择答案"
                },
            );
            return;
        };
        self.record_diag_answer(cx, q, ans, route, rag, toast_until, ai_config);
    }

    /// The 我不知道 escape hatch: records the round as unknown (never 答对),
    /// available only on choice questions with a loaded question.
    pub(crate) fn submit_diag_unknown(
        &mut self,
        cx: &mut Cx,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(q) = self.diag_current.clone() else { return };
        if !matches!(q.kind.as_str(), "single" | "multi") {
            return;
        }
        self.record_diag_answer(cx, q, crate::gen::DIAG_UNKNOWN.to_string(), route, rag, toast_until, ai_config);
    }

    /// Record the (question, answer) round and advance: finish at the round
    /// cap, otherwise request the next question.
    pub(crate) fn record_diag_answer(
        &mut self,
        cx: &mut Cx,
        q: crate::gen::DiagQuestion,
        ans: String,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.diag_history.push((q, ans));
        self.diag_current = None;
        if self.diag_history.len() >= MAX_DIAG_ROUNDS {
            self.finish_diag(cx, "", route, rag, toast_until, ai_config);
        } else {
            self.send_diag_request(cx, rag, ai_config);
        }
    }

    /// Close the popup and plan the route with the interview transcript
    /// (plus the model's summary when it stopped early).
    pub(crate) fn finish_diag(
        &mut self,
        cx: &mut Cx,
        summary: &str,
        route: &mut RouteController,
        rag: Option<&crate::rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let goal = self.diag_goal.clone();
        let mut diag = crate::gen::format_diag_history(&self.diag_history);
        if !summary.is_empty() {
            if !diag.is_empty() {
                diag.push('\n');
            }
            diag.push_str(&format!("诊断摘要：{summary}"));
        }
        self.reset_diag();
        route.close_startup(cx, self);
        route.start_route_plan(cx, &goal, &diag, rag, toast_until, ai_config);
    }

    pub(crate) fn reset_diag(&mut self) {
        self.diag_id = LiveId::empty();
        self.diag_goal.clear();
        self.diag_history.clear();
        self.diag_current = None;
        self.diag_single = None;
        self.diag_multi = [false; 4];
        self.diag_retried = false;
    }

    /// Switch the startup popup between the goal-input and diag phases.
    pub(crate) fn set_startup_phase(&mut self, cx: &mut Cx, phase: StartupPhase) {
        let goal_view = popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(goal_view)],
        );
        goal_view.set_visible(cx, phase == StartupPhase::Goal);
        let diag_view = popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view)],
        );
        diag_view.set_visible(cx, phase == StartupPhase::Diag);
    }
}

impl App {
    /// Forwarding shims (state lives in DiagController).
    pub(crate) fn handle_startup_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        self.diag.handle_startup_actions(
            cx,
            actions,
            &mut self.route,
            self.rag.as_ref(),
            &mut self.toast_until,
            &self.ai_config,
        );
    }

    pub(crate) fn begin_diag(&mut self, cx: &mut Cx, goal: &str) {
        self.diag.begin_diag(
            cx,
            goal,
            &mut self.route,
            self.rag.as_ref(),
            &mut self.toast_until,
            &self.ai_config,
        );
    }

    pub(crate) fn handle_diag_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.diag.handle_diag_response(
            cx,
            response,
            &mut self.route,
            self.rag.as_ref(),
            &mut self.toast_until,
            &self.ai_config,
        );
    }

    pub(crate) fn set_diag_status(&self, cx: &mut Cx, text: &str) {
        self.diag.set_diag_status(cx, text);
    }
}
