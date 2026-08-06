use makepad_widgets::*;

use crate::slide_panel::SlideState;

/// Bottom-edge hot zone (px) that reveals the dock when the cursor enters.
const HOT_ZONE: f64 = 60.0;
/// Slide travel for a fully hidden dock. Must stay >= dock height + bottom
/// margin + a few px so progress 0 is fully off-screen; bump if the dock
/// DSL grows taller.
const HIDDEN_Y: f64 = 72.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.BottomBarBase = #(BottomBar::register_widget(vm))

    // Auto-hiding bottom dock: the app's content slot (the 25/50/25
    // button row) is drawn at a window-sized walk shifted down by
    // (1 - progress) * HIDDEN_Y, so progress 0 sits fully below the
    // window bottom and slides up on hover.
    mod.widgets.BottomBar = set_type_default() do mod.widgets.BottomBarBase{
        width: Fill
        height: Fill
        clip_x: false
        clip_y: false

        content := mod.widgets.View{}
    }
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct BottomBar {
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
    window_size: DVec2,
    /// 0 = hidden below the window bottom, 1 = fully shown.
    #[rust]
    slide: SlideState,
}

impl ScriptHook for BottomBar {}

impl WidgetNode for BottomBar {
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

impl Widget for BottomBar {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.slide.handle_event(cx, event) {
            self.redraw(cx);
        }
        // Reveal while the cursor is in the bottom-edge hot zone; hide once
        // it leaves. The area rect (full window) is refreshed every draw.
        if let Event::MouseMove(e) = event {
            let rect = self.area.rect(cx);
            let want = e.abs.y > rect.pos.y + rect.size.y - HOT_ZONE;
            if want != self.slide.opened {
                self.slide.set(cx, want);
            }
        }
        if let Some(content) = self.content_widget(cx) {
            content.handle_event(cx, event, scope);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        self.window_size = cx.current_pass_size();
        cx.begin_turtle(self.walk, self.layout);
        if let Some(content) = self.content_widget(cx) {
            // Same clip trick as FloatPanel: this turtle's clip is disabled,
            // so push the window rect — draw_clip data and hit-testing then
            // resolve correctly, and the dock is clipped at the bottom edge
            // while sliding.
            let window_rect = Rect {
                pos: DVec2::default(),
                size: self.window_size,
            };
            cx.push_clip_rect(window_rect);
            let walk = Walk {
                abs_pos: Some(dvec2(0.0, (1.0 - self.slide.progress) * HIDDEN_Y)),
                width: Size::Fixed(self.window_size.x),
                height: Size::Fixed(self.window_size.y),
                ..Walk::default()
            };
            let _ = content.draw_walk(cx, scope, walk);
            cx.pop_clip_rect();
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl BottomBar {
    fn content_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.content_ref.is_none() {
            self.content_ref = Some(self.view.widget(cx, ids!(content)));
        }
        self.content_ref.clone()
    }
}

impl BottomBarRef {
    /// The dock's content widget (resolved and cached by the dock itself).
    /// Direct navigation — root tree lookups don't index widgets nested
    /// inside custom-widget content.
    pub fn content(&self, cx: &Cx) -> WidgetRef {
        self.borrow_mut()
            .and_then(|mut w| w.content_widget(cx))
            .unwrap_or_default()
    }
}
