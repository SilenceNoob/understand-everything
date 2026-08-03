use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.FloatPanelBase = #(FloatPanel::register_widget(vm))

    mod.widgets.FloatPanel = set_type_default() do mod.widgets.FloatPanelBase{
        width: Fit
        height: Fit
        clip_x: false
        clip_y: false

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
    grab: DVec2,
    #[rust]
    window_size: DVec2,
    // PerfGraph's default panel_width/panel_height (matches the script
    // defaults; the content pane is window-sized so the clamp never shrinks it).
    #[rust(dvec2(330.0, 150.0))]
    panel_size: DVec2,
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
}

impl Widget for FloatPanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        self.window_size = cx.current_pass_size();
        cx.begin_turtle(self.walk, self.layout);
        if self.opened {
            if let Some(content) = self.content_widget(cx) {
                // PerfGraph corner-pins its panel to the bottom-right of its
                // walk rect (pos + size - panel - margin), so give it a
                // window-sized walk shifted so the panel's top-left lands on
                // self.pos.
                let walk = Walk {
                    abs_pos: Some(
                        self.pos + self.panel_size + dvec2(10.0, 10.0) - self.window_size,
                    ),
                    width: Size::Fixed(self.window_size.x),
                    height: Size::Fixed(self.window_size.y),
                    ..Walk::default()
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
                if !self.inited {
                    self.inited = true;
                    let mut pos = self.window_size - self.panel_size - dvec2(10.0, 10.0);
                    pos.x = pos.x.max(0.0);
                    pos.y = pos.y.max(0.0);
                    self.pos = pos;
                }
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
                    self.dragging = true;
                    self.grab = fe.abs - self.pos;
                }
            }
            Hit::FingerMove(fe) => {
                if self.dragging {
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
            }
            _ => {}
        }
    }
}

impl FloatPanel {
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
}
