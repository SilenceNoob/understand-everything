use makepad_widgets::*;

use crate::gen::NewCardType;

const PANEL_W: f64 = 420.0;
/// Fixed panel height (fits the status line when visible).
const PANEL_H: f64 = 300.0;

/// Action emitted by the CreateCardPopup to the App.
#[derive(Clone, Debug, Default)]
pub enum CreateCardAction {
    #[default]
    None,
    Close,
    /// The user confirmed: (chosen archetype, topic text, AI auto-attach).
    Submit(NewCardType, String, bool),
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let TypeBtn = mod.widgets.ButtonFlat{
        width: Fill
        height: (36.0)
        draw_bg +: {
            color: #ffffff0a
            color_hover: #ffffff14
            color_down: #ffffff1c
            color_focus: #0000
            border_radius: 4.0
            border_size: 1.0
            border_color: #ffffff26
        }
        draw_text +: {
            text_style: theme.font_regular{font_size: 13.0}
            color: #e6e9f0
        }
    }

    let TypeBtnOn = mod.widgets.ButtonFlat{
        width: Fill
        height: (36.0)
        draw_bg +: {
            color: #4c6ef518
            color_hover: #4c6ef526
            color_down: #5c7cfa30
            color_focus: #4c6ef520
            border_radius: 4.0
            border_size: 1.0
            border_color: #4c6ef5
        }
        draw_text +: {
            text_style: theme.font_bold{font_size: 13.0}
            color: #e6e9f0
        }
    }

    mod.widgets.CreateCardPopupBase = #(CreateCardPopup::register_widget(vm))

    mod.widgets.CreateCardPopup = set_type_default() do mod.widgets.CreateCardPopupBase{
        width: Fill
        height: Fill
        panel := mod.widgets.RoundedView{
            width: (420.0)
            height: (300.0)
            flow: Down
            padding: 16
            spacing: 10
            show_bg: true
            draw_bg +: {
                color: #2b3140
                border_radius: 8.0
                border_size: 1.0
                border_color: #ffffff3d
            }
            title := mod.widgets.Label{
                width: Fill
                text: "创建新卡片"
                draw_text +: {
                    text_style: theme.font_bold{font_size: 15.0}
                    color: #e6e9f0
                }
            }
            hint := mod.widgets.Label{
                width: Fill
                text: "选择卡片类型并输入主题，AI 将生成卡片的基础内容并自动挂到现有树状图中。"
                draw_text +: {
                    text_style: theme.font_regular{font_size: 11.5}
                    color: #9aa3b5
                }
            }
            type_row := mod.widgets.View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                concept_off := TypeBtn{ text: "判别模型卡（概念）" }
                concept_on := TypeBtnOn{ visible: false, text: "判别模型卡（概念）" }
                knowledge_off := TypeBtn{ text: "联结模型卡（知识）" }
                knowledge_on := TypeBtnOn{ visible: false, text: "联结模型卡（知识）" }
            }
            auto_row := mod.widgets.View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                auto_label := mod.widgets.Label{
                    width: Fill
                    text: "AI 自动判断连线关系（挂到相关卡片下）"
                    draw_text +: {
                        text_style: theme.font_regular{font_size: 12.0}
                        color: #9aa3b5
                    }
                }
                yes_off := TypeBtn{ width: Fit, padding: Inset{left: 12, right: 12}, text: "是" }
                yes_on := TypeBtnOn{ visible: false, width: Fit, padding: Inset{left: 12, right: 12}, text: "是" }
                no_off := TypeBtn{ width: Fit, padding: Inset{left: 12, right: 12}, text: "否" }
                no_on := TypeBtnOn{ visible: false, width: Fit, padding: Inset{left: 12, right: 12}, text: "否" }
            }
            topic_input := mod.widgets.TextInput{
                width: Fill
                height: (30.0)
                empty_text: "输入卡片主题…"
                draw_bg +: {
                    color: #ffffff0a
                    border_radius: 4.0
                    border_size: 1.0
                    border_color: #ffffff26
                }
            }
            status := mod.widgets.Label{
                width: Fill
                visible: false
                text: ""
                draw_text +: {
                    text_style: theme.font_regular{font_size: 12.0}
                    color: #f0a58a
                }
            }
            btn_row := mod.widgets.View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                cancel_btn := TypeBtn{ width: Fit, padding: Inset{left: 16, right: 16}, text: "取消" }
                submit_btn := TypeBtnOn{ width: Fit, padding: Inset{left: 16, right: 16}, text: "生成卡片" }
            }
        }
    }
}

/// Create-card dialog: two exclusive archetype buttons (判别模型/联结模型),
/// a topic input, and 生成/取消. Centered on screen; the App hosts it in a
/// PopupPanel. The AI request runs in GenController; the popup shows a busy
/// status until the response lands.
#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct CreateCardPopup {
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
    /// Window-wide capture area: a click outside the panel closes the popup.
    #[rust]
    backdrop_area: Area,
    /// Chosen archetype (default 判别模型).
    #[rust]
    ctype: NewCardType,
    /// Whether the AI should auto-attach the new card to a related card
    /// (default true); false = standalone card, no auto wiring.
    #[rust(true)]
    auto_attach: bool,
    /// True while an AI create request is in flight (submit/type clicks are
    /// ignored; the status label shows progress).
    #[rust]
    busy: bool,
    /// Status text ("" = hidden); errors keep the popup open.
    #[rust]
    status: String,
    /// The drawn panel rect in screen coords (used for click-outside tests).
    #[rust]
    panel_rect: Rect,
    /// Give the topic input key focus on the next draw after opening.
    #[rust]
    focus_pending: bool,
}

impl WidgetNode for CreateCardPopup {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }

    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        self.walk
    }

    fn area(&self) -> Area {
        self.area
    }

    fn redraw(&mut self, cx: &mut Cx) {
        cx.redraw_area_and_children(self.area);
    }
}

impl ScriptHook for CreateCardPopup {}

impl Widget for CreateCardPopup {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let window = Rect {
            pos: DVec2::default(),
            size: cx.current_pass_size(),
        };
        cx.add_aligned_rect_area(&mut self.backdrop_area, window);

        // Centered modal dialog (fixed size so click-outside hit tests and
        // the panel background always line up, status line included).
        let pos = dvec2(
            (window.size.x - PANEL_W) * 0.5,
            (window.size.y * 0.35).max(16.0),
        );
        if let Some(panel) = self.panel(cx) {
            let _ = panel.draw_walk(
                cx,
                scope,
                Walk {
                    abs_pos: Some(pos),
                    width: Size::Fixed(PANEL_W),
                    height: Size::Fixed(PANEL_H),
                    ..Walk::default()
                },
            );
        }
        self.panel_rect = Rect {
            pos,
            size: dvec2(PANEL_W, PANEL_H),
        };

        if self.focus_pending {
            self.focus_pending = false;
            if let Some(panel) = self.panel(cx) {
                let input = panel.widget(cx, ids!(topic_input));
                let a = input.area();
                if !a.is_empty() {
                    cx.set_key_focus(a);
                }
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(panel) = self.panel(cx) {
            panel.handle_event(cx, event, scope);
        }
        match event {
            Event::KeyDown(ke) if ke.key_code == KeyCode::Escape => {
                cx.widget_action(self.widget_uid(), CreateCardAction::Close);
            }
            Event::MouseDown(_) => {
                if let Hit::FingerDown(fe) = event.hits_with_capture_overload(cx, self.backdrop_area, true) {
                    if !self.panel_rect.contains(fe.abs) {
                        cx.widget_action(self.widget_uid(), CreateCardAction::Close);
                    }
                }
            }
            Event::Actions(actions) => {
                if let Some(panel) = self.panel(cx) {
                    let input = panel.widget(cx, ids!(topic_input)).as_text_input();
                    if input.escaped(actions) {
                        cx.widget_action(self.widget_uid(), CreateCardAction::Close);
                        return;
                    }
                    if input.returned(actions).is_some() {
                        self.try_submit(cx);
                        return;
                    }
                    // Cancel stays available while busy (the request just
                    // keeps running; its result still attaches/toasts).
                    if panel.widget(cx, ids!(cancel_btn)).as_button().clicked(actions) {
                        cx.widget_action(self.widget_uid(), CreateCardAction::Close);
                        return;
                    }
                    if self.busy {
                        return;
                    }
                    // Archetype toggle: clicking the unselected side of a type
                    // switches to it; the selected side is already visible.
                    let concept_clicked = panel.widget(cx, ids!(concept_off)).as_button().clicked(actions);
                    let knowledge_clicked = panel.widget(cx, ids!(knowledge_off)).as_button().clicked(actions);
                    if concept_clicked || knowledge_clicked {
                        let next = if concept_clicked {
                            NewCardType::Concept
                        } else {
                            NewCardType::Knowledge
                        };
                        if self.ctype != next {
                            self.ctype = next;
                            self.sync_type_buttons(cx);
                            self.redraw(cx);
                        }
                        return;
                    }
                    // Auto-attach toggle (是/否): clicking the unselected side
                    // flips the switch.
                    let yes_clicked = panel.widget(cx, ids!(yes_off)).as_button().clicked(actions);
                    let no_clicked = panel.widget(cx, ids!(no_off)).as_button().clicked(actions);
                    if yes_clicked || no_clicked {
                        let next = yes_clicked;
                        if self.auto_attach != next {
                            self.auto_attach = next;
                            self.sync_auto_buttons(cx);
                            self.redraw(cx);
                        }
                        return;
                    }
                    if panel.widget(cx, ids!(submit_btn)).as_button().clicked(actions) {
                        self.try_submit(cx);
                        return;
                    }
                }
            }
            _ => {}
        }
    }
}

impl CreateCardPopup {
    fn panel(&self, cx: &Cx) -> Option<WidgetRef> {
        let p = self.view.widget(cx, ids!(panel));
        if p.is_empty() {
            None
        } else {
            Some(p)
        }
    }

    fn sync_type_buttons(&mut self, cx: &mut Cx) {
        let Some(panel) = self.panel(cx) else { return };
        let concept_on = self.ctype == NewCardType::Concept;
        panel.widget(cx, ids!(concept_off)).set_visible(cx, !concept_on);
        panel.widget(cx, ids!(concept_on)).set_visible(cx, concept_on);
        panel.widget(cx, ids!(knowledge_off)).set_visible(cx, concept_on);
        panel.widget(cx, ids!(knowledge_on)).set_visible(cx, !concept_on);
    }

    fn sync_auto_buttons(&mut self, cx: &mut Cx) {
        let Some(panel) = self.panel(cx) else { return };
        panel.widget(cx, ids!(yes_off)).set_visible(cx, !self.auto_attach);
        panel.widget(cx, ids!(yes_on)).set_visible(cx, self.auto_attach);
        panel.widget(cx, ids!(no_off)).set_visible(cx, self.auto_attach);
        panel.widget(cx, ids!(no_on)).set_visible(cx, !self.auto_attach);
    }

    fn sync_status(&mut self, cx: &mut Cx) {
        if let Some(panel) = self.panel(cx) {
            let label = panel.widget(cx, ids!(status));
            label.set_text(cx, &self.status);
            label.set_visible(cx, !self.status.is_empty());
        }
    }

    fn try_submit(&mut self, cx: &mut Cx) {
        if self.busy {
            return;
        }
        let Some(panel) = self.panel(cx) else { return };
        let topic = panel.widget(cx, ids!(topic_input)).as_text_input().text();
        let topic = topic.trim().to_string();
        if topic.is_empty() {
            self.status = "请输入卡片主题".to_string();
            self.sync_status(cx);
            self.redraw(cx);
            return;
        }
        self.status = "正在生成…".to_string();
        self.sync_status(cx);
        cx.widget_action(
            self.widget_uid(),
            CreateCardAction::Submit(self.ctype, topic, self.auto_attach),
        );
    }
}

impl CreateCardPopupRef {
    /// Open with `topic` prefilled into the input; resets the archetype to
    /// 判别模型, the auto-attach switch to on, clears status/busy, and
    /// focuses the topic input.
    pub fn open(&self, cx: &mut Cx, topic: &str) {
        if let Some(mut w) = self.borrow_mut() {
            w.ctype = NewCardType::Concept;
            w.auto_attach = true;
            w.busy = false;
            w.status = String::new();
            w.focus_pending = true;
            w.sync_type_buttons(cx);
            w.sync_auto_buttons(cx);
            w.sync_status(cx);
            if let Some(panel) = w.panel(cx) {
                panel.widget(cx, ids!(topic_input)).as_text_input().set_text(cx, topic);
            }
            w.redraw(cx);
        }
    }

    /// Update the status label ("正在生成…"/errors); `busy` locks the form
    /// while an AI request is in flight. Empty text hides the label.
    pub fn set_status(&self, cx: &mut Cx, text: &str, busy: bool) {
        if let Some(mut w) = self.borrow_mut() {
            w.status = text.to_string();
            w.busy = busy;
            w.sync_status(cx);
            w.redraw(cx);
        }
    }

    /// True while an AI create request is in flight.
    pub fn busy(&self) -> bool {
        self.borrow().map(|w| w.busy).unwrap_or(false)
    }

    /// True when the user closed the popup (Esc / click outside / 取消).
    pub fn close_clicked(&self, actions: &Actions) -> bool {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let CreateCardAction::Close = item.cast() {
                return true;
            }
        }
        false
    }

    /// The submitted (archetype, topic, auto_attach), if any was made this
    /// action cycle.
    pub fn submitted(&self, actions: &Actions) -> Option<(NewCardType, String, bool)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let CreateCardAction::Submit(ctype, topic, auto_attach) = item.cast() {
                return Some((ctype, topic, auto_attach));
            }
        }
        None
    }
}

/// The create-card popup's PopupPanel host (via the body's live children).
pub(crate) fn create_popup(ui: &WidgetRef) -> WidgetRef {
    crate::app::popup_widget(ui, live_id!(create_card_popup))
}

/// The CreateCardPopup content widget inside the popup.
pub(crate) fn create_content(ui: &WidgetRef) -> WidgetRef {
    crate::app::popup_child(ui, live_id!(create_card_popup), &[live_id!(content)])
}
