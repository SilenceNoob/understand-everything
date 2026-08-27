use makepad_widgets::*;


use crate::ai::{self};
use crate::gen::*;
use crate::mindmap::MindMapWidgetRefExt;
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::quiz_panel::{QuizPanelWidgetRefExt, QuizSubmission};
use crate::App;
use crate::app::{popup_child, popup_widget};

/// Quiz state + logic, extracted from App.
#[derive(Default)]
pub(crate) struct QuizController {
    pub(crate) ui: WidgetRef,
    pub(crate) quiz_id: LiveId,
    pub(crate) quiz_path: Option<String>,
    pub(crate) quiz_body: Option<String>,
    pub(crate) grade_id: LiveId,
    /// SSE accumulator for the in-flight quiz-generation stream (streaming
    /// keeps bytes flowing during long generations; the reply is finalized
    /// into an HttpResponse by the app's stream handlers).
    pub(crate) quiz_stream: ai::StructStream,
    /// SSE accumulator for the in-flight grading stream.
    pub(crate) grade_stream: ai::StructStream,
    /// One automatic format-repair retry per quiz generation.
    pub(crate) quiz_retried: bool,
    /// True once the endpoint rejected `response_format` (HTTP 400): fall
    /// back to prompt-only JSON requests from then on.
    pub(crate) quiz_json_retried: bool,
    /// Same fallback flag for the grading request.
    pub(crate) grade_json_retried: bool,
    /// One automatic format-repair retry per grading request.
    pub(crate) grade_retried: bool,
    /// The last grading submission, kept so a 400 fallback or a repair retry
    /// can re-fire the request without user interaction.
    pub(crate) grade_submission: Option<QuizSubmission>,
}

impl QuizController {
    pub(crate) fn quiz_title(&self) -> String {
        self.quiz_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Fire the quiz-generation request. `repair_hint`, when present, is
    /// appended to the user message after a parse failure so the retry tells
    /// the model what was wrong.
    fn send_quiz_request(
        &mut self,
        cx: &mut Cx,
        ai_config: &crate::ai::AIConfig,
        repair_hint: Option<&str>,
    ) {
        let Some(body) = self.quiz_body.clone() else { return };
        let (system, mut user) = quiz_generation_messages(&body, crate::gen::card_type(&body));
        if let Some(hint) = repair_hint {
            user.push_str(&format!("\n\n【格式修复要求】{hint}"));
        }
        self.quiz_id = LiveId::unique();
        self.quiz_stream = ai::StructStream::default();
        ai::chat_completions_structured_stream(
            cx,
            self.quiz_id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            ai::StructuredRequest {
                // A full quiz (3 single + 2 multi + 1 open) is a few KB at
                // most; the previous 358400 cap only invited huge outputs.
                max_tokens: 16384,
                json_mode: !self.quiz_json_retried,
                thinking: Some("low"),
            },
        );
    }

    pub(crate) fn start_quiz(
        &mut self,
        cx: &mut Cx,
        path: &str,
        ai_config: &crate::ai::AIConfig,
    ) {
        let full_path = crate::util::data_dir().join(path);
        let body = std::fs::read_to_string(&full_path).unwrap_or_default();
        let title = full_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if let Err(missing) = quiz_ready(&body) {
            quiz_panel.show_error(cx, &format!("卡片缺少以下板块：{}", missing.join("、")));
            return;
        }
        self.quiz_path = Some(path.to_string());
        self.quiz_body = Some(body.clone());
        self.quiz_retried = false;
        self.quiz_json_retried = false;
        self.grade_retried = false;
        self.grade_json_retried = false;
        self.grade_submission = None;
        quiz_panel.show_loading(cx, &title);
        self.send_quiz_request(cx, ai_config, None);
    }

    pub(crate) fn handle_quiz_response(
        &mut self,
        cx: &mut Cx,
        response: &HttpResponse,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.quiz_id = LiveId::empty();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            // `response_format` is rejected by some OpenAI-compatible
            // gateways with HTTP 400; retry once without it.
            if matches!(response.status_code, 400 | 422) && !self.quiz_json_retried {
                self.quiz_json_retried = true;
                quiz_panel.show_loading(cx, &self.quiz_title());
                self.send_quiz_request(cx, ai_config, None);
                return;
            }
            quiz_panel.show_error(cx, &format!("出题失败：{} {}", response.status_code, detail));
            return;
        }
        // Prefer content but fall back to the thinking chain when the model
        // left `content` empty.
        let content = ai::response_structured_text(response);
        match parse_quiz(&content) {
            Ok(q) => {
                self.quiz_retried = false;
                quiz_panel.set_quiz(cx, &self.quiz_title(), self.quiz_body.as_deref().unwrap_or(""), &q);
            }
            Err(e) => {
                if !self.quiz_retried {
                    self.quiz_retried = true;
                    let preview = ai::text_preview(&content, 300);
                    quiz_panel.show_loading(cx, &self.quiz_title());
                    self.send_quiz_request(
                        cx,
                        ai_config,
                        Some(&format!(
                            "你上一次的输出无法解析为 JSON（错误：{e}；输出预览：{preview}）。\
请严格只输出一个 JSON 对象，不要 markdown 代码块、不要任何解释或前后缀文字；题目若包含代码，\
代码必须作为 JSON 字符串的值，换行写为 \\n、双引号写为 \\\"，JSON 内部不要使用 ``` 围栏；\
单选题和多选题都必须恰好 4 个选项，answer/answers 只能是 A-D 字母。"
                        )),
                    );
                    return;
                }
                let debug = ai::response_debug_preview(response);
                quiz_panel.show_error(cx, &format!("题目解析失败：{e}（{debug}）"));
            }
        }
    }

    pub(crate) fn handle_quiz_panel_actions(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
        ai_config: &crate::ai::AIConfig,
    ) {
        let quiz_panel = self.quiz_panel();
        if quiz_panel.close_clicked(actions) {
            self.close_quiz_popup(cx);
        }
        if let Some(submission) = quiz_panel.submit_clicked(actions) {
            self.send_grade_request(cx, submission, ai_config);
        }
    }

    /// Fire the grading request for the stored submission. `repair_hint`, when
    /// present, is appended after a parse failure so the retry is not blind.
    fn send_grade_request_inner(
        &mut self,
        cx: &mut Cx,
        ai_config: &crate::ai::AIConfig,
        repair_hint: Option<&str>,
    ) {
        let Some(body) = self.quiz_body.clone() else { return };
        let Some(submission) = self.grade_submission.clone() else { return };
        if submission.open_questions.is_empty() || submission.open.is_empty() {
            return;
        }
        let (system, mut user) =
            quiz_grading_messages(&body, &submission.open_questions, &submission.open);
        if let Some(hint) = repair_hint {
            user.push_str(&format!("\n\n【格式修复要求】{hint}"));
        }
        self.grade_id = LiveId::unique();
        self.grade_stream = ai::StructStream::default();
        ai::chat_completions_structured_stream(
            cx,
            self.grade_id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            ai::StructuredRequest {
                max_tokens: 8192,
                json_mode: !self.grade_json_retried,
                thinking: Some("low"),
            },
        );
    }

    pub(crate) fn send_grade_request(
        &mut self,
        cx: &mut Cx,
        submission: QuizSubmission,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.grade_submission = Some(submission);
        self.grade_retried = false;
        self.grade_json_retried = false;
        self.send_grade_request_inner(cx, ai_config, None);
    }

    pub(crate) fn handle_grade_response(
        &mut self,
        cx: &mut Cx,
        response: &HttpResponse,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.grade_id = LiveId::empty();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            if matches!(response.status_code, 400 | 422) && !self.grade_json_retried {
                self.grade_json_retried = true;
                quiz_panel.grade_retrying(cx, "评分请求格式不受支持，正在重试…");
                self.send_grade_request_inner(cx, ai_config, None);
                return;
            }
            quiz_panel.grade_failed(cx, &format!("评分失败：{} {}", response.status_code, detail));
            return;
        }
        let content = ai::response_structured_text(response);
        let expected = self
            .grade_submission
            .as_ref()
            .map(|s| s.open_questions.len())
            .unwrap_or(0);
        let outcome = match parse_grades(&content) {
            Ok(g) if g.len() == expected => Ok(g),
            Ok(g) => Err(format!(
                "评分结果数量不匹配：应为 {expected} 条，收到 {} 条",
                g.len()
            )),
            Err(e) => Err(e),
        };
        match outcome {
            Ok(g) => {
                self.grade_retried = false;
                self.grade_submission = None;
                quiz_panel.set_grades(cx, &g);
                // Persist the mastery score (已见/未见) and refresh the canvas
                // badge. The card is keyed by rel path so progress follows it
                // across maps.
                if let Some(score) = quiz_panel.last_score() {
                    if let Some(path) = self.quiz_path.as_deref() {
                        let base = crate::util::data_dir();
                        let mut progress = crate::mindmap::model::load_progress(&base);
                        progress.insert(path.to_string(), score);
                        crate::mindmap::model::save_progress(&base, &progress);
                        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                        mind_map.reload_progress(cx);
                    }
                }
            }
            Err(e) => {
                if !self.grade_retried {
                    self.grade_retried = true;
                    let preview = ai::text_preview(&content, 300);
                    quiz_panel.grade_retrying(cx, "评分结果格式有误，正在重试…");
                    self.send_grade_request_inner(
                        cx,
                        ai_config,
                        Some(&format!(
                            "你上一次的输出无法解析为 JSON 数组（错误：{e}；输出预览：{preview}）。\
请严格只输出一个 JSON 数组，元素数量与题目数量一致，不要 markdown 代码块、不要任何解释或前后缀文字。"
                        )),
                    );
                    return;
                }
                self.grade_submission = None;
                let debug = ai::response_debug_preview(response);
                quiz_panel.grade_failed(cx, &format!("评分解析失败：{e}（{debug}）"));
            }
        }
    }

    pub(crate) fn quiz_panel(&self) -> crate::quiz_panel::QuizPanelRef {
        popup_child(&self.ui, live_id!(quiz_popup), &[live_id!(content)])
            .as_quiz_panel()
    }

    pub(crate) fn open_quiz_popup(&self, cx: &mut Cx) {
        popup_widget(&self.ui, live_id!(quiz_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(picker_popup),
            live_id!(confirm_popup),
        ] {
            popup_widget(&self.ui, id).as_popup_panel().hide(cx);
        }
    }

    pub(crate) fn close_quiz_popup(&self, cx: &mut Cx) {
        popup_widget(&self.ui, live_id!(quiz_popup)).as_popup_panel().hide(cx);
    }
}

impl App {
    /// Forwarding shims (state lives in QuizController).
    pub(crate) fn start_quiz(&mut self, cx: &mut Cx, path: &str) {
        self.quiz.start_quiz(cx, path, &self.ai_config);
    }

    pub(crate) fn handle_quiz_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.quiz.handle_quiz_response(cx, response, &self.ai_config);
    }

    pub(crate) fn handle_quiz_panel_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        self.quiz.handle_quiz_panel_actions(cx, actions, &self.ai_config);
    }

    pub(crate) fn handle_grade_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.quiz.handle_grade_response(cx, response, &self.ai_config);
    }

    pub(crate) fn quiz_panel(&self) -> crate::quiz_panel::QuizPanelRef {
        self.quiz.quiz_panel()
    }

    pub(crate) fn open_quiz_popup(&self, cx: &mut Cx) {
        self.quiz.open_quiz_popup(cx);
    }
}
