use makepad_widgets::*;

use crate::gen::{GradeResult, Quiz};

/// A user's collected answers to a quiz, emitted by the quiz panel when the
/// submit button is pressed.
#[derive(Clone, Debug, Default)]
pub struct QuizSubmission {
    pub open: Vec<String>,
    pub open_questions: Vec<crate::gen::OpenQuestion>,
}

/// Action emitted by the quiz panel to the App.
#[derive(Clone, Debug, Default)]
pub enum QuizPanelAction {
    #[default]
    None,
    Close,
    Submit(QuizSubmission),
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let OptionBtn = mod.widgets.ButtonFlat{
        width: Fit
        height: Fit
        padding: Inset{left: 8, right: 8, top: 4, bottom: 4}
        margin: Inset{right: 4}
        draw_bg +: {
            color: #0000
            color_hover: #ffffff10
            color_down: #ffffff18
            color_focus: #0000
            border_size: uniform(1.0)
            border_color: #ffffff30
            border_radius: uniform(4.0)
        }
        draw_text +: {
            text_style: theme.font_regular{font_size: 12.0}
            color: #e6e9f0
        }
    }
    let OptionBtnOn = OptionBtn{
        draw_bg +: {
            color: #4c6ef520
            color_hover: #5c7cfa30
            color_down: #5c7cfa30
            color_focus: #4c6ef520
            border_color: #4c6ef5
        }
        draw_text +: {
            text_style: theme.font_bold{font_size: 12.0}
            color: #e6e9f0
        }
    }

    let QResult = mod.widgets.Label{
        width: Fill
        height: Fit
        visible: false
        text: ""
        draw_text.text_style.font_size: 12.0
        draw_text.color: #e6e9f0
    }

    let SingleQuestion = mod.widgets.View{
        width: Fill
        height: Fit
        flow: Down
        spacing: 6
        padding: Inset{left: 12, right: 12, top: 10, bottom: 10}
        q_label := mod.widgets.Label{
            width: Fill
            height: Fit
            text: ""
            draw_text.text_style.font_size: 13.0
            draw_text.color: #e6e9f0
        }
        options := mod.widgets.View{
            width: Fill
            height: Fit
            flow: Right
            spacing: 4
            opt0_off := OptionBtn{ text: "A" }
            opt0_on := OptionBtnOn{ visible: false, text: "A" }
            opt1_off := OptionBtn{ text: "B" }
            opt1_on := OptionBtnOn{ visible: false, text: "B" }
            opt2_off := OptionBtn{ text: "C" }
            opt2_on := OptionBtnOn{ visible: false, text: "C" }
            opt3_off := OptionBtn{ text: "D" }
            opt3_on := OptionBtnOn{ visible: false, text: "D" }
        }
        result := QResult{}
    }

    let MultiQuestion = mod.widgets.View{
        width: Fill
        height: Fit
        flow: Down
        spacing: 6
        padding: Inset{left: 12, right: 12, top: 10, bottom: 10}
        q_label := mod.widgets.Label{
            width: Fill
            height: Fit
            text: ""
            draw_text.text_style.font_size: 13.0
            draw_text.color: #e6e9f0
        }
        options := mod.widgets.View{
            width: Fill
            height: Fit
            flow: Right
            spacing: 4
            opt0_off := OptionBtn{ text: "A" }
            opt0_on := OptionBtnOn{ visible: false, text: "A" }
            opt1_off := OptionBtn{ text: "B" }
            opt1_on := OptionBtnOn{ visible: false, text: "B" }
            opt2_off := OptionBtn{ text: "C" }
            opt2_on := OptionBtnOn{ visible: false, text: "C" }
            opt3_off := OptionBtn{ text: "D" }
            opt3_on := OptionBtnOn{ visible: false, text: "D" }
        }
        result := QResult{}
    }

    let OpenQuestion = mod.widgets.View{
        width: Fill
        height: Fit
        flow: Down
        spacing: 6
        padding: Inset{left: 12, right: 12, top: 10, bottom: 10}
        q_label := mod.widgets.Label{
            width: Fill
            height: Fit
            text: ""
            draw_text.text_style.font_size: 13.0
            draw_text.color: #e6e9f0
        }
        answer := mod.widgets.TextInput{
            width: Fill
            height: Fit
            is_multiline: true
            empty_text: "用你自己的话回答…"
        }
        result := QResult{}
    }

    mod.widgets.QuizPanelBase = #(QuizPanel::register_widget(vm))

    mod.widgets.QuizPanel = set_type_default() do mod.widgets.QuizPanelBase{
        width: Fill
        height: Fill
        content := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Overlay
            align: Align{x: 0.5, y: 0.5}
            draw_bg +: {
                color: #000000cc
            }
            panel := mod.widgets.RoundedView{
                width: 560
                height: (640.0)
                flow: Down
                show_bg: true
                draw_bg +: {
                    color: #1f2430
                    border_radius: 8.0
                    border_size: 1.0
                    border_color: #ffffff14
                }
                header := mod.widgets.View{
                    width: Fill
                    height: (44.0)
                    flow: Right
                    align: Align{y: 0.5}
                    padding: Inset{left: 16, right: 12}
                    title := mod.widgets.Label{
                        width: Fill
                        height: Fit
                        text: "测验"
                        draw_text.text_style.font_size: 16.0
                        draw_text.color: #e6e9f0
                    }
                    close_btn := mod.widgets.ButtonFlat{
                        width: Fit
                        height: Fit
                        text: "✕"
                        draw_text.text_style.font_size: 14.0
                        draw_text.color: #e6e9f0
                    }
                }
                body := mod.widgets.ScrollYView{
                    width: Fill
                    height: Fill
                    questions := mod.widgets.View{
                        width: Fill
                        height: Fit
                        flow: Down
                        single0 := SingleQuestion{}
                        single1 := SingleQuestion{}
                        single2 := SingleQuestion{}
                        multi0 := MultiQuestion{}
                        multi1 := MultiQuestion{}
                        open0 := OpenQuestion{}
                    }
                }
                status := mod.widgets.Label{
                    width: Fill
                    height: Fit
                    padding: Inset{left: 16, right: 16, top: 8, bottom: 8}
                    text: ""
                    draw_text.text_style.font_size: 12.0
                    draw_text.color: #aab0bc
                }
                footer := mod.widgets.View{
                    width: Fill
                    height: (52.0)
                    flow: Right
                    align: Align{x: 1.0, y: 0.5}
                    padding: Inset{left: 16, right: 16, bottom: 12}
                    submit_btn := mod.widgets.ButtonFlat{
                        width: Fit
                        height: Fit
                        text: "提交"
                        padding: Inset{left: 14, right: 14, top: 6, bottom: 6}
                        draw_bg +: {
                            color: #4c6ef5
                            color_hover: #5c7cfa
                            color_down: #5c7cfa
                            color_focus: #4c6ef5
                            border_radius: uniform(4.0)
                        }
                        draw_text +: {
                            text_style: theme.font_bold{font_size: 13.0}
                            color: #ffffff
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct QuizPanel {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,

    #[rust]
    area: Area,
    #[rust]
    quiz: Option<Quiz>,
    #[rust]
    graded: bool,
    #[rust]
    loading: bool,
    #[rust]
    error: Option<String>,
    #[rust]
    card_title: String,
    #[rust]
    card_body: String,
    #[rust]
    single_selected: [Option<usize>; 3],
    #[rust]
    multi_selected: [Vec<bool>; 2],
}

impl WidgetNode for QuizPanel {
    fn widget_uid(&self) -> WidgetUid { self.uid }
    fn walk(&mut self, _cx: &mut Cx) -> Walk { self.walk }
    fn area(&self) -> Area { self.area }
    fn redraw(&mut self, cx: &mut Cx) { cx.redraw_area_and_children(self.area); }
}

impl ScriptHook for QuizPanel {}

impl Widget for QuizPanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let _ = self.view.draw_walk(cx, scope, walk);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else { return };
        let content = self.content(cx);
        let close_btn = content.widget(cx, ids!(panel)).widget(cx, ids!(header)).widget(cx, ids!(close_btn));
        if close_btn.as_button().clicked(actions) {
            cx.widget_action(self.widget_uid(), QuizPanelAction::Close);
        }
        let submit_btn = content.widget(cx, ids!(panel)).widget(cx, ids!(footer)).widget(cx, ids!(submit_btn));
        if submit_btn.as_button().clicked(actions) && self.quiz.is_some() && !self.graded && !self.loading {
            let submission = self.collect_submission(cx);
            cx.widget_action(self.widget_uid(), QuizPanelAction::Submit(submission));
        }
        self.handle_option_clicks(cx, actions);
    }
}

impl QuizPanel {
    fn content(&self, cx: &Cx) -> WidgetRef {
        self.view.widget(cx, ids!(content))
    }

    fn panel(&self, cx: &Cx) -> WidgetRef {
        self.content(cx).widget(cx, ids!(panel))
    }

    fn status(&self, cx: &Cx) -> WidgetRef {
        self.panel(cx).widget(cx, ids!(status))
    }

    fn submit_btn(&self, cx: &Cx) -> WidgetRef {
        self.panel(cx).widget(cx, ids!(footer)).widget(cx, ids!(submit_btn))
    }

    fn question_view(&self, cx: &Cx) -> WidgetRef {
        self.panel(cx).widget(cx, ids!(body)).widget(cx, ids!(questions))
    }

    fn single_slot(&self, cx: &Cx, i: usize) -> WidgetRef {
        self.question_view(cx).widget(cx, match i {
            0 => ids!(single0),
            1 => ids!(single1),
            _ => ids!(single2),
        })
    }

    fn multi_slot(&self, cx: &Cx, i: usize) -> WidgetRef {
        self.question_view(cx).widget(cx, match i {
            0 => ids!(multi0),
            _ => ids!(multi1),
        })
    }

    fn open_slot(&self, cx: &Cx) -> WidgetRef {
        self.question_view(cx).widget(cx, ids!(open0))
    }

    fn option_btn(&self, cx: &Cx, slot: &WidgetRef, i: usize, on: bool) -> WidgetRef {
        if i == 0 && !on {
            slot.widget(cx, ids!(opt0_off))
        } else if i == 0 {
            slot.widget(cx, ids!(opt0_on))
        } else if i == 1 && !on {
            slot.widget(cx, ids!(opt1_off))
        } else if i == 1 {
            slot.widget(cx, ids!(opt1_on))
        } else if i == 2 && !on {
            slot.widget(cx, ids!(opt2_off))
        } else if i == 2 {
            slot.widget(cx, ids!(opt2_on))
        } else if i == 3 && !on {
            slot.widget(cx, ids!(opt3_off))
        } else if i == 3 {
            slot.widget(cx, ids!(opt3_on))
        } else {
            WidgetRef::empty()
        }
    }

    fn handle_option_clicks(&mut self, cx: &mut Cx, actions: &Actions) {
        for qi in 0..3 {
            let slot = self.single_slot(cx, qi);
            for oi in 0..4 {
                let off = self.option_btn(cx, &slot, oi, false);
                if off.as_button().clicked(actions) {
                    self.single_selected[qi] = Some(oi);
                    self.sync_single_options(cx, qi);
                }
                let on = self.option_btn(cx, &slot, oi, true);
                if on.as_button().clicked(actions) {
                    // clicking the already-selected option in a single-choice keeps it selected
                }
            }
        }
        for qi in 0..2 {
            let slot = self.multi_slot(cx, qi);
            for oi in 0..4 {
                let off = self.option_btn(cx, &slot, oi, false);
                if off.as_button().clicked(actions) {
                    if self.multi_selected[qi].get(oi) != Some(&true) {
                        if oi >= self.multi_selected[qi].len() {
                            self.multi_selected[qi].resize(oi + 1, false);
                        }
                        self.multi_selected[qi][oi] = true;
                    }
                    self.sync_multi_options(cx, qi);
                }
                let on = self.option_btn(cx, &slot, oi, true);
                if on.as_button().clicked(actions) {
                    if oi < self.multi_selected[qi].len() {
                        self.multi_selected[qi][oi] = false;
                    }
                    self.sync_multi_options(cx, qi);
                }
            }
        }
    }

    fn sync_single_options(&self, cx: &mut Cx, qi: usize) {
        let slot = self.single_slot(cx, qi);
        for oi in 0..4 {
            let selected = self.single_selected[qi] == Some(oi);
            self.option_btn(cx, &slot, oi, false).set_visible(cx, !selected);
            self.option_btn(cx, &slot, oi, true).set_visible(cx, selected);
        }
    }

    fn sync_multi_options(&self, cx: &mut Cx, qi: usize) {
        let slot = self.multi_slot(cx, qi);
        for oi in 0..4 {
            let selected = self.multi_selected[qi].get(oi) == Some(&true);
            self.option_btn(cx, &slot, oi, false).set_visible(cx, !selected);
            self.option_btn(cx, &slot, oi, true).set_visible(cx, selected);
        }
    }

    fn collect_submission(&self, cx: &Cx) -> QuizSubmission {
        let open_text = self.open_slot(cx).text_input(cx, ids!(answer)).text();
        let open_questions = self.quiz.as_ref().map(|q| q.open.clone()).unwrap_or_default();
        QuizSubmission {
            open: vec![open_text],
            open_questions,
        }
    }

    fn set_status(&self, cx: &mut Cx, text: &str) {
        self.status(cx).as_label().set_text(cx, text);
    }

    fn sync_all_options(&self, cx: &mut Cx) {
        for qi in 0..3 {
            self.sync_single_options(cx, qi);
        }
        for qi in 0..2 {
            self.sync_multi_options(cx, qi);
        }
    }

    fn set_option_texts(&self, cx: &mut Cx, slot: &WidgetRef, options: &[String]) {
        for (oi, opt) in options.iter().take(4).enumerate() {
            self.option_btn(cx, slot, oi, false).as_button().set_text(cx, opt);
            self.option_btn(cx, slot, oi, true).as_button().set_text(cx, opt);
        }
    }

    fn hide_results(&self, cx: &mut Cx) {
        for qi in 0..3 {
            let slot = self.single_slot(cx, qi);
            slot.widget(cx, ids!(result)).set_visible(cx, false);
        }
        for qi in 0..2 {
            let slot = self.multi_slot(cx, qi);
            slot.widget(cx, ids!(result)).set_visible(cx, false);
        }
        let slot = self.open_slot(cx);
        slot.widget(cx, ids!(result)).set_visible(cx, false);
    }

    fn show_empty(&self, cx: &mut Cx) {
        for qi in 0..3 {
            self.single_slot(cx, qi).set_visible(cx, false);
        }
        for qi in 0..2 {
            self.multi_slot(cx, qi).set_visible(cx, false);
        }
        self.open_slot(cx).set_visible(cx, false);
        self.submit_btn(cx).set_visible(cx, false);
    }
}

impl QuizPanelRef {
    pub fn show_loading(&self, cx: &mut Cx, title: &str) {
        if let Some(mut w) = self.borrow_mut() {
            w.quiz = None;
            w.graded = false;
            w.loading = true;
            w.error = None;
            w.card_title = title.to_string();
            w.show_empty(cx);
            w.set_status(cx, &format!("{title}: 出题中…"));
            w.redraw(cx);
        }
    }

    pub fn show_error(&self, cx: &mut Cx, msg: &str) {
        if let Some(mut w) = self.borrow_mut() {
            w.quiz = None;
            w.graded = false;
            w.loading = false;
            w.error = Some(msg.to_string());
            w.show_empty(cx);
            w.set_status(cx, msg);
            w.redraw(cx);
        }
    }

    pub fn set_status_text(&self, cx: &mut Cx, msg: &str) {
        if let Some(w) = self.borrow() {
            w.set_status(cx, msg);
        }
    }

    pub fn set_quiz(&self, cx: &mut Cx, title: &str, body: &str, quiz: &Quiz) {
        if let Some(mut w) = self.borrow_mut() {
            w.quiz = Some(quiz.clone());
            w.graded = false;
            w.loading = false;
            w.error = None;
            w.card_title = title.to_string();
            w.card_body = body.to_string();
            w.single_selected = [None, None, None];
            w.multi_selected = [Vec::new(), Vec::new()];
            w.set_status(cx, "选择/填写后点击提交");
            w.submit_btn(cx).set_visible(cx, true);

            let panel = w.panel(cx);
            panel.widget(cx, ids!(header)).widget(cx, ids!(title)).as_label().set_text(cx, &format!("测验 · {title}"));

            for qi in 0..3 {
                let slot = w.single_slot(cx, qi);
                if let Some(q) = quiz.single.get(qi) {
                    slot.set_visible(cx, true);
                    slot.widget(cx, ids!(q_label)).as_label().set_text(cx, &q.question);
                    w.set_option_texts(cx, &slot, &q.options);
                } else {
                    slot.set_visible(cx, false);
                }
            }
            for qi in 0..2 {
                let slot = w.multi_slot(cx, qi);
                if let Some(q) = quiz.multi.get(qi) {
                    slot.set_visible(cx, true);
                    slot.widget(cx, ids!(q_label)).as_label().set_text(cx, &q.question);
                    w.set_option_texts(cx, &slot, &q.options);
                } else {
                    slot.set_visible(cx, false);
                }
            }
            let open_slot = w.open_slot(cx);
            if let Some(q) = quiz.open.first() {
                open_slot.set_visible(cx, true);
                open_slot.widget(cx, ids!(q_label)).as_label().set_text(cx, &q.question);
                open_slot.text_input(cx, ids!(answer)).set_text(cx, "");
            } else {
                open_slot.set_visible(cx, false);
            }
            w.hide_results(cx);
            w.sync_all_options(cx);
            w.redraw(cx);
        }
    }

    pub fn set_grades(&self, cx: &mut Cx, grades: &[GradeResult]) {
        if let Some(mut w) = self.borrow_mut() {
            let Some(quiz) = w.quiz.clone() else { return };
            w.graded = true;
            w.set_status(cx, "评分完成");

            for qi in 0..3 {
                let slot = w.single_slot(cx, qi);
                if let Some(q) = quiz.single.get(qi) {
                    let result = slot.widget(cx, ids!(result));
                    let correct = w.single_selected[qi] == Some(letter_to_index(&q.answer));
                    let text = if correct {
                        "✓ 正确".to_string()
                    } else {
                        format!("✗ 正确答案：{}", q.answer)
                    };
                    let color = if correct { 0x4ade80ff } else { 0xfca5a5ff };
                    result.as_label().set_text(cx, &text);
                    result.as_label().set_text_color(cx, Vec4f::from_u32(color));
                    result.set_visible(cx, true);
                }
            }
            for qi in 0..2 {
                let slot = w.multi_slot(cx, qi);
                if let Some(q) = quiz.multi.get(qi) {
                    let result = slot.widget(cx, ids!(result));
                    let selected: std::collections::HashSet<usize> = w.multi_selected[qi].iter().enumerate().filter(|(_, v)| **v).map(|(i, _)| i).collect();
                    let correct: std::collections::HashSet<usize> = q.answers.iter().filter_map(|a| letter_to_index_opt(a)).collect();
                    let correct = selected == correct && !selected.is_empty();
                    let text = if correct {
                        "✓ 正确".to_string()
                    } else {
                        let ans = q.answers.join(", ");
                        format!("✗ 正确答案：{ans}")
                    };
                    let color = if correct { 0x4ade80ff } else { 0xfca5a5ff };
                    result.as_label().set_text(cx, &text);
                    result.as_label().set_text_color(cx, Vec4f::from_u32(color));
                    result.set_visible(cx, true);
                }
            }
            let open_slot = w.open_slot(cx);
            if let (Some(q), Some(g)) = (quiz.open.first(), grades.first()) {
                let result = open_slot.widget(cx, ids!(result));
                let text = format!(
                    "得分：{} / 10\n{}\n\n标准解答：{}",
                    g.score.clamp(0, 10),
                    g.feedback,
                    q.reference_answer
                );
                result.as_label().set_text(cx, &text);
                result.as_label().set_text_color(cx, Vec4f::from_u32(0xe6e9f0ff));
                result.set_visible(cx, true);
            }
            w.redraw(cx);
        }
    }

    pub fn close_clicked(&self, actions: &Actions) -> bool {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let QuizPanelAction::Close = item.cast() {
                return true;
            }
        }
        false
    }

    pub fn submit_clicked(&self, actions: &Actions) -> Option<QuizSubmission> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let QuizPanelAction::Submit(s) = item.cast() {
                return Some(s);
            }
        }
        None
    }
}

fn letter_to_index(s: &str) -> usize {
    letter_to_index_opt(s).unwrap_or(0)
}

fn letter_to_index_opt(s: &str) -> Option<usize> {
    let mut chars = s.trim().chars();
    let c = chars.next()?;
    if !c.is_ascii_alphabetic() {
        return None;
    }
    Some((c.to_ascii_uppercase() as u8 - b'A') as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letter_to_index_works() {
        assert_eq!(letter_to_index("A"), 0);
        assert_eq!(letter_to_index("c"), 2);
        assert_eq!(letter_to_index_opt("Z"), Some(25));
        assert!(letter_to_index_opt("").is_none());
    }
}
