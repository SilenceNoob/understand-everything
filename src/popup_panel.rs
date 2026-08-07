use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.PopupPanelBase = #(PopupPanel::register_widget(vm))

    // Always-drawn modal popup (setting/about): the widget itself is in the
    // draw pass every frame, so its area stays valid and show/hide redraws
    // work — unlike a visible:false View, which never draws and whose
    // set_visible redraw path is unreliable. Content is drawn only while
    // opened; the walk is Fit so a closed popup has a 0-size area that
    // doesn't block the canvas.
    mod.widgets.PopupPanel = set_type_default() do mod.widgets.PopupPanelBase{
        width: Fit
        height: Fit
        clip_x: false
        clip_y: false
    }
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct PopupPanel {
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
    content_ref: Option<WidgetRef>,
    #[rust]
    opened: bool,
}

impl ScriptHook for PopupPanel {}

impl WidgetNode for PopupPanel {
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

    fn children(&self, visit: &mut dyn FnMut(LiveId, WidgetRef)) {
        self.view.children(visit);
    }
}

impl Widget for PopupPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.opened {
            return;
        }
        if let Some(content) = self.content_widget(cx) {
            content.handle_event(cx, event, scope);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        cx.begin_turtle(self.walk, self.layout);
        if self.opened {
            if let Some(content) = self.content_widget(cx) {
                // Window-sized walk so the content's own layout (centered
                // panel, full-window backdrop) resolves against the window;
                // the widget's own Fit turtle stays 0-size.
                let window_size = cx.current_pass_size();
                let _ = content.draw_walk(
                    cx,
                    scope,
                    Walk {
                        abs_pos: Some(dvec2(0.0, 0.0)),
                        width: Size::Fixed(window_size.x),
                        height: Size::Fixed(window_size.y),
                        ..Walk::default()
                    },
                );
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl PopupPanel {
    /// The popup's content widget, found via live children (graph-
    /// independent, like the app's popup_widget/popup_child helpers).
    fn content_widget(&mut self, _cx: &Cx) -> Option<WidgetRef> {
        if self.content_ref.is_none() {
            let mut found = None;
            for (name, child) in &self.view.children {
                if *name == live_id!(content) {
                    found = Some(child.clone());
                }
            }
            self.content_ref = found;
        }
        self.content_ref.clone()
    }
}

impl PopupPanelRef {
    pub fn opened(&self) -> bool {
        self.borrow().map(|w| w.opened).unwrap_or(false)
    }

    pub fn show(&self, cx: &mut Cx) {
        if let Some(mut w) = self.borrow_mut() {
            w.opened = true;
            w.redraw(cx);
        }
    }

    pub fn hide(&self, cx: &mut Cx) {
        if let Some(mut w) = self.borrow_mut() {
            w.opened = false;
            w.redraw(cx);
        }
    }
}
