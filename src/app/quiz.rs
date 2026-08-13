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
}

impl QuizController {
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
        quiz_panel.show_loading(cx, &title);
        self.quiz_id = LiveId::unique();
        let (system, user) = quiz_generation_messages(&body, crate::gen::card_type(&body));
        ai::chat_completions(
            cx,
            self.quiz_id,
            &ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    pub(crate) fn handle_quiz_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.quiz_id = LiveId::empty();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            quiz_panel.show_error(cx, &format!("出题失败：{} {}", response.status_code, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match parse_quiz(&content) {
            Ok(q) => {
                let title = self
                    .quiz_path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let body = self.quiz_body.clone().unwrap_or_default();
                quiz_panel.set_quiz(cx, &title, &body, &q);
            }
            Err(e) => quiz_panel.show_error(cx, &format!("题目解析失败：{}", e)),
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

    pub(crate) fn send_grade_request(
        &mut self,
        cx: &mut Cx,
        submission: QuizSubmission,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(body) = self.quiz_body.as_deref() else { return };
        let questions = submission.open_questions;
        let answers = submission.open;
        if questions.is_empty() || answers.is_empty() {
            return;
        }
        let (system, user) = quiz_grading_messages(body, &questions, &answers);
        self.grade_id = LiveId::unique();
        ai::chat_completions(
            cx,
            self.grade_id,
            &ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    pub(crate) fn handle_grade_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.grade_id = LiveId::empty();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            quiz_panel.grade_failed(cx, &format!("评分失败：{} {}", response.status_code, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match parse_grades(&content) {
            Ok(g) => {
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
            Err(e) => quiz_panel.grade_failed(cx, &format!("评分解析失败：{}", e)),
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
        self.quiz.handle_quiz_response(cx, response);
    }

    pub(crate) fn handle_quiz_panel_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        self.quiz.handle_quiz_panel_actions(cx, actions, &self.ai_config);
    }

    pub(crate) fn handle_grade_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.quiz.handle_grade_response(cx, response);
    }

    pub(crate) fn quiz_panel(&self) -> crate::quiz_panel::QuizPanelRef {
        self.quiz.quiz_panel()
    }

    pub(crate) fn open_quiz_popup(&self, cx: &mut Cx) {
        self.quiz.open_quiz_popup(cx);
    }
}
