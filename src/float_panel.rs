use makepad_widgets::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const RESIZE_LEFT: u8 = 1;
const RESIZE_RIGHT: u8 = 2;
const RESIZE_TOP: u8 = 4;
const RESIZE_BOTTOM: u8 = 8;
/// Edge band (px) where a press starts a resize instead of a drag.
const RESIZE_T: f64 = 6.0;
/// Smallest panel size (header + input row fit inside).
const RESIZE_MIN: DVec2 = dvec2(240.0, 160.0);

/// True while a chat input inside a FloatPanel holds key focus; the mindmap
/// skips its keyboard shortcuts so typing doesn't move the map.
pub(crate) static CHAT_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn is_chat_input_active() -> bool {
    CHAT_INPUT_ACTIVE.load(Ordering::Relaxed)
}

/// Rect of every open FloatPanel in window coords, keyed by widget uid; the
/// mindmap skips wheel-zoom over them (their content scrolls instead).
pub(crate) static FLOAT_PANEL_RECTS: Mutex<Vec<(WidgetUid, Rect)>> = Mutex::new(Vec::new());

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.FloatPanelBase = #(FloatPanel::register_widget(vm))

    mod.widgets.FloatPanel = set_type_default() do mod.widgets.FloatPanelBase{
        width: Fit
        height: Fit
        clip_x: false
        clip_y: false

        // PerfGraph's default panel size (its draw_walk matches this).
        panel_size: vec2(330.0, 150.0)

        content := mod.widgets.PerfGraph{
            // PerfGraph's template leaves draw_text without a text_style, so
            // its labels would shape with an empty font family ("WARNING:
            // encountered empty font family" and invisible text). draw_walk
            // only overrides font_size, the family survives.
            draw_text +: {
                text_style: theme.font_regular{}
            }
        }
    }
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct FloatPanel {
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
    /// Top-left corner of the PerfGraph panel, relative to the window.
    #[rust(dvec2(1100.0, 740.0))]
    pos: DVec2,
    #[rust]
    dragging: bool,
    #[rust]
    resizing: u8,
    #[rust]
    grab: DVec2,
    #[rust]
    window_size: DVec2,
    // Panel rect size; per-instance overridable from script.
    #[live(dvec2(330.0, 150.0))]
    panel_size: DVec2,
    /// Content anchors its panel to the bottom-right of its walk rect
    /// (PerfGraph does this natively); false walks the content in an exact
    /// rect at self.pos so a script View draws from its top-left.
    #[live(true)]
    pin_bottom_right: bool,
    #[rust]
    panel_area: Area,
    #[rust]
    inited: bool,
}

impl ScriptHook for FloatPanel {}

impl WidgetNode for FloatPanel {
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

impl Widget for FloatPanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        self.window_size = cx.current_pass_size();
        cx.begin_turtle(self.walk, self.layout);
        // Register/remove this panel's rect for the mindmap's wheel-zoom skip.
        FLOAT_PANEL_RECTS
            .lock()
            .unwrap()
            .retain(|(u, _)| *u != self.uid);
        if self.opened {
            // Initialize the panel position before the first draw: the first
            // frame must already be drawn at the final pos, or the visible
            // position (old pos) and the logical self.pos diverge and any
            // drag grab computed against self.pos jumps the panel to the
            // bottom-right on the first FingerMove.
            if !self.inited {
                self.inited = true;
                let mut pos = self.window_size - self.panel_size - dvec2(10.0, 10.0);
                pos.x = pos.x.max(0.0);
                pos.y = pos.y.max(0.0);
                self.pos = pos;
            }
            if let Some(content) = self.content_widget(cx) {
                // PerfGraph corner-pins its panel to the bottom-right of its
                // walk rect (pos + size - panel - margin), so give it a
                // window-sized walk shifted so the panel's top-left lands on
                // self.pos. Top-left anchored content (pin_bottom_right:
                // false) gets an exact rect at self.pos instead.
                let walk = if self.pin_bottom_right {
                    Walk {
                        abs_pos: Some(
                            self.pos + self.panel_size + dvec2(10.0, 10.0) - self.window_size,
                        ),
                        width: Size::Fixed(self.window_size.x),
                        height: Size::Fixed(self.window_size.y),
                        ..Walk::default()
                    }
                } else {
                    Walk {
                        abs_pos: Some(self.pos),
                        width: Size::Fixed(self.panel_size.x),
                        height: Size::Fixed(self.panel_size.y),
                        ..Walk::default()
                    }
                };
                // This turtle's clip is disabled (clip_x/y: false, required
                // for visibility at a 0-size walk), so the content's instance
                // draw_clip would fall back to garbage/NaN and hit-testing via
                // clipped_rect would only ever match a stray corner. Pushing a
                // real clip rect over the panel area fixes the draw_clip data
                // (same trick the mindmap canvas uses for its cards).
                let panel_rect = Rect {
                    pos: self.pos,
                    size: self.panel_size,
                };
                FLOAT_PANEL_RECTS.lock().unwrap().push((self.uid, panel_rect));
                cx.push_clip_rect(panel_rect);
                let _ = content.draw_walk(cx, scope, walk);
                // Own rect area over the panel: hit-testing against
                // content.area() is wrong (PerfGraph's derive area() resolves
                // to its draw_bg DrawVars area, which every draw_abs
                // overwrites — the last one is the bottom-left 6x6 legend
                // swatch). The rect area's clip is set from the pushed clip
                // at pass end, so clipped_rect == panel_rect.
                cx.add_aligned_rect_area(&mut self.panel_area, panel_rect);
                cx.pop_clip_rect();
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.opened {
            return;
        }
        let Some(content) = self.content_widget(cx) else {
            return;
        };
        content.handle_event(cx, event, scope);
        // Snapshot before event.hits below captures the digit to our own area
        // (mirrors MindMap: a child widget that grabbed the press must not
        // start a panel drag).
        let child_grabbed = cx.fingers.any_areas_captured();
        // capture_overload: the mindmap's canvas (earlier in tree order,
        // covering the whole body) hits first and marks t.handled, which
        // makes plain hits() skip our area entirely. Overriding the capture
        // lets the panel win the digit on its own rect.
        match event.hits_with_capture_overload(cx, self.panel_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if !child_grabbed {
                    let dir = self.resize_hit(fe.abs);
                    if dir != 0 {
                        self.resizing = dir;
                    } else {
                        self.dragging = true;
                        self.grab = fe.abs - self.pos;
                    }
                }
            }
            Hit::FingerMove(fe) => {
                if self.resizing != 0 {
                    let dir = self.resizing;
                    let mut pos = self.pos;
                    let mut size = self.panel_size;
                    let max = self.window_size;
                    if dir & RESIZE_LEFT != 0 {
                        let w = (size.x + pos.x - fe.abs.x).clamp(RESIZE_MIN.x, max.x);
                        pos.x += size.x - w;
                        size.x = w;
                    }
                    if dir & RESIZE_RIGHT != 0 {
                        size.x = (fe.abs.x - pos.x).clamp(RESIZE_MIN.x, max.x);
                    }
                    if dir & RESIZE_TOP != 0 {
                        let h = (size.y + pos.y - fe.abs.y).clamp(RESIZE_MIN.y, max.y);
                        pos.y += size.y - h;
                        size.y = h;
                    }
                    if dir & RESIZE_BOTTOM != 0 {
                        size.y = (fe.abs.y - pos.y).clamp(RESIZE_MIN.y, max.y);
                    }
                    self.pos = pos;
                    self.panel_size = size;
                    self.redraw(cx);
                } else if self.dragging {
                    let mut pos = fe.abs - self.grab;
                    let max_x = (self.window_size.x - self.panel_size.x - 20.0).max(0.0);
                    let max_y = (self.window_size.y - self.panel_size.y - 20.0).max(0.0);
                    pos.x = pos.x.clamp(0.0, max_x);
                    pos.y = pos.y.clamp(0.0, max_y);
                    self.pos = pos;
                    self.redraw(cx);
                }
            }
            Hit::FingerUp(_) => {
                self.dragging = false;
                self.resizing = 0;
            }
            _ => {}
        }
        if let Event::MouseMove(e) = event {
            if self.resizing == 0 && !self.dragging {
                let dir = self.resize_hit(e.abs);
                let cursor = match (dir & (RESIZE_LEFT | RESIZE_RIGHT) != 0, dir & (RESIZE_TOP | RESIZE_BOTTOM) != 0) {
                    (true, true) => {
                        let ne_sw = dir & RESIZE_TOP != 0 && dir & RESIZE_RIGHT != 0
                            || dir & RESIZE_BOTTOM != 0 && dir & RESIZE_LEFT != 0;
                        if ne_sw {
                            MouseCursor::NeswResize
                        } else {
                            MouseCursor::NwseResize
                        }
                    }
                    (true, false) => MouseCursor::EwResize,
                    (false, true) => MouseCursor::NsResize,
                    (false, false) => MouseCursor::Default,
                };
                cx.set_cursor(cursor);
            }
        }
    }
}

impl FloatPanel {
    /// Edge band hit (window coords) as a direction bitmask, 0 when not on
    /// any edge. Mirrors the mindmap's card resize_hit.
    fn resize_hit(&self, p: DVec2) -> u8 {
        let t = RESIZE_T;
        let r = Rect {
            pos: self.pos,
            size: self.panel_size,
        };
        let on_l = (p.x - r.pos.x).abs() <= t;
        let on_r = (p.x - (r.pos.x + r.size.x)).abs() <= t;
        let on_t = (p.y - r.pos.y).abs() <= t;
        let on_b = (p.y - (r.pos.y + r.size.y)).abs() <= t;
        let in_x = p.x >= r.pos.x - t && p.x <= r.pos.x + r.size.x + t;
        let in_y = p.y >= r.pos.y - t && p.y <= r.pos.y + r.size.y + t;
        let mut dir = 0;
        if (on_l || on_r) && in_y {
            dir |= if on_l { RESIZE_LEFT } else { RESIZE_RIGHT };
        }
        if (on_t || on_b) && in_x {
            dir |= if on_t { RESIZE_TOP } else { RESIZE_BOTTOM };
        }
        dir
    }

    fn content_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.content_ref.is_none() {
            self.content_ref = Some(self.view.widget(cx, ids!(content)));
        }
        self.content_ref.clone()
    }

    pub fn show(&mut self, cx: &mut Cx) {
        self.opened = true;
        self.redraw(cx);
    }

    pub fn hide(&mut self, cx: &mut Cx) {
        self.opened = false;
        self.dragging = false;
        self.resizing = 0;
        // The input can't emit KeyFocusLost once the panel is gone.
        CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed);
        self.redraw(cx);
    }
}

impl FloatPanelRef {
    pub fn opened(&self) -> bool {
        self.borrow().map(|w| w.opened).unwrap_or(false)
    }

    pub fn show(&self, cx: &mut Cx) {
        if let Some(mut w) = self.borrow_mut() {
            w.show(cx);
        }
    }

    pub fn hide(&self, cx: &mut Cx) {
        if let Some(mut w) = self.borrow_mut() {
            w.hide(cx);
        }
    }

    /// The panel's content widget (resolved and cached by the panel itself).
    pub fn content(&self, cx: &Cx) -> WidgetRef {
        self.borrow_mut()
            .and_then(|mut w| w.content_widget(cx))
            .unwrap_or_default()
    }
}
