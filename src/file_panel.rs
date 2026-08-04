use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.FilePanelBase = #(FilePanel::register_widget(vm))

    mod.widgets.FilePanel = set_type_default() do mod.widgets.FilePanelBase{
        width: Fit
        height: Fit
        clip_x: false
        clip_y: false

        // Chrome only: rounded bg + border behind the panes.
        content := mod.widgets.RoundedView{
            width: Fill
            height: Fill
            flow: Down
            show_bg: true
            draw_bg +: {
                color: #1f2430
                border_radius: 8.0
                border_size: 1.0
                border_color: #ffffff14
            }
        }
        // Divider line between the panes; FilePanel draws it with draw_abs
        // (the mindmap-crosshair pattern — DrawColor renders reliably, unlike
        // overriding the Splitter widget's custom shader).
        draw_divider +: {
            color: #ffffff30
        }
        // Top pane: future canvas file tree lands here (画布文件化).
        canvas_pane := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Down
            canvas_header := mod.widgets.Label{
                width: Fill
                height: Fit
                padding: Inset{left: 12, right: 12, top: 8, bottom: 8}
                text: "Map"
                draw_text.text_style.font_size: 14.0
                draw_text.color: #e6e9f0
            }
            canvas_list := mod.widgets.ScrollYView{
                width: Fill
                height: Fill
            }
        }
        // Bottom pane: future card file tree lands here (卡片文件化).
        card_pane := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Down
            card_header := mod.widgets.Label{
                width: Fill
                height: Fit
                padding: Inset{left: 12, right: 12, top: 8, bottom: 8}
                text: "Card"
                draw_text.text_style.font_size: 14.0
                draw_text.color: #e6e9f0
            }
            card_list := mod.widgets.ScrollYView{
                width: Fill
                height: Fill
            }
        }

        tab := mod.widgets.ButtonFlat{
            text: "◀"
            // Arrow sized to fit the 14px-wide tab; drop the theme's side
            // padding (8px each side would leave negative label space here).
            draw_text.text_style.font_size: 8.0
            padding: Inset{left: 0, right: 0}
            draw_bg +: {
                color: #1f2430
                color_hover: #232834
                color_down: #232834
                border_size: uniform(1.0)
                border_color: #ffffff14
            }
        }
    }
}

const TAB_W: f64 = 14.0;
const TAB_H: f64 = 48.0;
/// Exponential ease rate (1/s); settles in ~0.2s.
const SLIDE_EASE: f64 = 14.0;
/// Splitter bar height and the grab margin around it.
const SPLITTER_BAR: f64 = 12.0;
const SPLITTER_MARGIN: f64 = 3.0;
/// Minimum height (px) each section keeps when dragging the divider.
const SPLIT_MIN: f64 = 60.0;
/// Default panel width and drag limits (px).
const PANEL_W_DEFAULT: f64 = 260.0;
const PANEL_W_MIN: f64 = 140.0;
const PANEL_W_MAX: f64 = 520.0;
/// Width-grab strip on the panel's right edge: 8px inside the panel,
/// 4px straddling the edge (total 12px).
const EDGE_W: f64 = 12.0;
const EDGE_INSET: f64 = 8.0;

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct FilePanel {
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
    canvas_pane_ref: Option<WidgetRef>,
    #[rust]
    card_pane_ref: Option<WidgetRef>,
    #[rust]
    tab_ref: Option<WidgetRef>,
    #[live]
    draw_divider: DrawColor,

    #[rust(true)]
    opened: bool,
    /// 0 = collapsed off the left edge, 1 = fully open; eases toward the
    /// target on timer ticks.
    #[rust(1.0)]
    slide: f64,
    #[rust]
    slide_timer: Option<Timer>,
    #[rust]
    last_timer_time: f64,
    #[rust]
    window_size: DVec2,
    /// Panel body rect and its drawn hit area, in window coords.
    #[rust]
    panel_rect: Rect,
    #[rust]
    panel_area: Area,
    #[rust]
    tab_rect: Rect,
    #[rust]
    tab_area: Area,
    /// Divider position as a fraction of the panel height (0..1).
    #[rust(0.5)]
    split: f64,
    #[rust]
    split_dragging: bool,
    #[rust]
    splitter_rect: Rect,
    #[rust]
    splitter_area: Area,
    /// Panel width in px, adjustable by dragging the right edge.
    #[rust(260.0)]
    panel_w: f64,
    #[rust]
    panel_w_dragging: bool,
    #[rust]
    edge_rect: Rect,
    #[rust]
    edge_area: Area,
}

impl ScriptHook for FilePanel {}

impl WidgetNode for FilePanel {
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

impl Widget for FilePanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        self.window_size = cx.current_pass_size();
        let body_y = cx.turtle().rect().pos.y; // body top, window coords
        let geo = panel_geometry(self.slide, self.split, self.panel_w, self.window_size, body_y);
        self.panel_rect = geo.panel;
        self.tab_rect = geo.tab;
        self.splitter_rect = geo.splitter;
        self.edge_rect = geo.edge;

        cx.begin_turtle(self.walk, self.layout);
        if let Some(content) = self.content_widget(cx) {
            // Same clip-rect trick as FloatPanel: the root turtle's clip is
            // disabled (0-size walk), so push a real clip so draw_clip data
            // and hit-testing resolve to the panel rect.
            cx.push_clip_rect(self.panel_rect);
            let panel = self.panel_rect;
            let pane = |walk: Walk| Walk {
                abs_pos: Some(walk.abs_pos.unwrap_or(panel.pos)),
                width: Size::Fixed(panel.size.x),
                ..walk
            };
            let chrome = Walk {
                abs_pos: Some(panel.pos),
                width: Size::Fixed(panel.size.x),
                height: Size::Fixed(panel.size.y),
                ..Walk::default()
            };
            let _ = content.draw_walk(cx, scope, chrome);
            // Panes are adjacent; the 1px divider sits on the boundary and
            // the grab strip (18px) is centered on it.
            let a_h = (self.split * panel.size.y).clamp(0.0, panel.size.y);
            let b_h = (panel.size.y - a_h - 1.0).max(0.0);
            if let Some(pane_w) = self.canvas_pane_widget(cx) {
                let _ = pane_w.draw_walk(
                    cx,
                    scope,
                    pane(Walk {
                        abs_pos: Some(panel.pos),
                        height: Size::Fixed(a_h),
                        ..Walk::default()
                    }),
                );
            }
            self.draw_divider.draw_abs(
                cx,
                Rect {
                    pos: panel.pos + dvec2(0.0, a_h),
                    size: dvec2(panel.size.x, 1.0),
                },
            );
            if let Some(pane_w) = self.card_pane_widget(cx) {
                let _ = pane_w.draw_walk(
                    cx,
                    scope,
                    pane(Walk {
                        abs_pos: Some(panel.pos + dvec2(0.0, a_h + 1.0)),
                        height: Size::Fixed(b_h),
                        ..Walk::default()
                    }),
                );
            }
            cx.add_aligned_rect_area(&mut self.panel_area, self.panel_rect);
            cx.add_aligned_rect_area(&mut self.splitter_area, self.splitter_rect);
            cx.add_aligned_rect_area(&mut self.edge_area, self.edge_rect);
            cx.pop_clip_rect();
        }
        if let Some(tab) = self.tab_widget(cx) {
            cx.push_clip_rect(self.tab_rect);
            let walk = Walk {
                abs_pos: Some(self.tab_rect.pos),
                width: Size::Fixed(self.tab_rect.size.x),
                height: Size::Fixed(self.tab_rect.size.y),
                ..Walk::default()
            };
            let _ = tab.draw_walk(cx, scope, walk);
            cx.add_aligned_rect_area(&mut self.tab_area, self.tab_rect);
            cx.pop_clip_rect();
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.handle_slide_anim(cx, event);
        if let Some(content) = self.content_widget(cx) {
            content.handle_event(cx, event, scope);
        }
        // Forward to the panes so future file-tree items (scroll, clicks)
        // get events; empty ScrollYViews are unaffected.
        if let Some(pane_w) = self.canvas_pane_widget(cx) {
            pane_w.handle_event(cx, event, scope);
        }
        if let Some(pane_w) = self.card_pane_widget(cx) {
            pane_w.handle_event(cx, event, scope);
        }
        if let Some(tab) = self.tab_widget(cx) {
            // hover/press visuals on the tab button
            tab.handle_event(cx, event, scope);
        }
        // capture_overload: the mindmap canvas (earlier in tree order, covering
        // the whole body) hits first and marks t.handled, which makes plain
        // hits() skip our areas entirely (same trick as FloatPanel).
        match event.hits_with_capture_overload(cx, self.tab_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.toggle(cx);
            }
            _ => {}
        }
        // Divider drag: capture_overload (the mindmap canvas shadows plain
        // hits, same as FloatPanel) on the strip around the divider line.
        // Right-edge drag to resize the panel width. Checked before the
        // splitter so grabbing the corner of the strip resizes the width.
        match event.hits_with_capture_overload(cx, self.edge_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.panel_w_dragging = true;
                cx.set_cursor(MouseCursor::ColResize);
                self.apply_width(cx, fe.abs.x);
            }
            Hit::FingerMove(fe) => {
                if self.panel_w_dragging {
                    cx.set_cursor(MouseCursor::ColResize);
                    self.apply_width(cx, fe.abs.x);
                }
            }
            Hit::FingerUp(_) => {
                self.panel_w_dragging = false;
            }
            _ => {}
        }
        match event.hits_with_capture_overload(cx, self.splitter_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.split_dragging = true;
                cx.set_cursor(MouseCursor::RowResize);
                self.apply_split(cx, fe.abs.y);
            }
            Hit::FingerMove(fe) => {
                if self.split_dragging {
                    cx.set_cursor(MouseCursor::RowResize);
                    self.apply_split(cx, fe.abs.y);
                }
            }
            Hit::FingerUp(_) => {
                self.split_dragging = false;
            }
            _ => {}
        }
        if let Event::MouseMove(e) = event {
            if !self.panel_w_dragging && !self.split_dragging {
                if self.edge_rect.contains(e.abs) {
                    cx.set_cursor(MouseCursor::ColResize);
                } else if self.splitter_rect.contains(e.abs) {
                    cx.set_cursor(MouseCursor::RowResize);
                }
            }
        }
        // Claim the press over the panel body so it never reaches the canvas;
        // on FingerUp the tab button itself also fires a click action nobody
        // listens to (toggle already happened on FingerDown).
        let _ = event.hits_with_capture_overload(cx, self.panel_area, true);
    }
}

impl FilePanel {
    fn content_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.content_ref.is_none() {
            self.content_ref = Some(self.view.widget(cx, ids!(content)));
        }
        self.content_ref.clone()
    }

    fn canvas_pane_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.canvas_pane_ref.is_none() {
            self.canvas_pane_ref = Some(self.view.widget(cx, ids!(canvas_pane)));
        }
        self.canvas_pane_ref.clone()
    }

    fn card_pane_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.card_pane_ref.is_none() {
            self.card_pane_ref = Some(self.view.widget(cx, ids!(card_pane)));
        }
        self.card_pane_ref.clone()
    }

    fn tab_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.tab_ref.is_none() {
            self.tab_ref = Some(self.view.widget(cx, ids!(tab)));
        }
        self.tab_ref.clone()
    }

    /// Ease `slide` toward its target on each timer tick (mirrors the
    /// mindmap's zoom animation pattern).
    fn handle_slide_anim(&mut self, cx: &mut Cx, event: &Event) {
        let Some(timer) = self.slide_timer else { return };
        let Some(te) = timer.is_event(event) else { return };
        let now = te.time.unwrap_or(0.0);
        // first tick has no baseline; fall back to one 60Hz frame
        let dt = if self.last_timer_time == 0.0 {
            1.0 / 60.0
        } else {
            (now - self.last_timer_time).max(0.0)
        };
        self.last_timer_time = now;
        let target = if self.opened { 1.0 } else { 0.0 };
        self.slide += (target - self.slide) * (1.0 - (-dt * SLIDE_EASE).exp());
        if (target - self.slide).abs() < 1e-3 {
            self.slide = target;
            cx.stop_timer(timer);
            self.slide_timer = None;
        }
        self.redraw(cx);
    }

    fn toggle(&mut self, cx: &mut Cx) {
        self.opened = !self.opened;
        if let Some(tab) = self.tab_widget(cx) {
            tab.set_text(cx, if self.opened { "◀" } else { "▶" });
        }
        if self.slide_timer.is_none() {
            self.slide_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_timer_time = 0.0;
        }
        self.redraw(cx);
    }

    fn apply_split(&mut self, cx: &mut Cx, abs_y: f64) {
        self.split = split_from_y(abs_y, self.panel_rect, SPLIT_MIN);
        self.redraw(cx);
    }

    fn apply_width(&mut self, cx: &mut Cx, abs_x: f64) {
        self.panel_w = panel_w_from_x(abs_x, self.panel_rect, PANEL_W_MIN, PANEL_W_MAX);
        self.redraw(cx);
    }
}

impl FilePanelRef {}

/// Panel geometry in window coords: body, tab, divider strip and width-grab
/// edge. Pure so it is unit-testable.
struct PanelGeo {
    panel: Rect,
    tab: Rect,
    splitter: Rect,
    edge: Rect,
}

/// Panel body, tab, divider-strip and edge rects for a given slide progress
/// (0 = collapsed off the left edge, 1 = fully open), split fraction and
/// panel width.
fn panel_geometry(slide: f64, split: f64, panel_w: f64, window: DVec2, body_y: f64) -> PanelGeo {
    let body_h = (window.y - body_y).max(0.0);
    let offset_x = -panel_w * (1.0 - slide);
    let panel = Rect {
        pos: dvec2(offset_x, body_y),
        size: dvec2(panel_w, body_h),
    };
    // Tab protrudes fully outside the panel, flush against its right edge;
    // when collapsed it pins to the left edge (x = 0).
    let tab_x = (panel.pos.x + panel.size.x).max(0.0);
    let tab = Rect {
        pos: dvec2(tab_x, body_y + body_h * 0.5 - TAB_H * 0.5),
        size: dvec2(TAB_W, TAB_H),
    };
    // Grab strip centered on the divider line (line at panel.y + split*h).
    let splitter = Rect {
        pos: dvec2(
            panel.pos.x,
            panel.pos.y + split * panel.size.y - SPLITTER_BAR * 0.5 - SPLITTER_MARGIN,
        ),
        size: dvec2(panel.size.x, SPLITTER_BAR + 2.0 * SPLITTER_MARGIN),
    };
    let edge = Rect {
        pos: dvec2(panel.pos.x + panel.size.x - EDGE_INSET, panel.pos.y),
        size: dvec2(EDGE_W, panel.size.y),
    };
    PanelGeo {
        panel,
        tab,
        splitter,
        edge,
    }
}

/// Panel width in px from a window-absolute x (the right edge follows the
/// cursor), clamped to [min, max].
fn panel_w_from_x(abs_x: f64, panel: Rect, min: f64, max: f64) -> f64 {
    (abs_x - panel.pos.x).clamp(min, max)
}

/// Divider fraction from a window-absolute y, clamped so both sections keep
/// at least `min_px`. The line follows the cursor, so no bar-half offset.
fn split_from_y(abs_y: f64, panel: Rect, min_px: f64) -> f64 {
    let h = panel.size.y;
    if h <= 0.0 {
        return 0.5;
    }
    let frac = (abs_y - panel.pos.y) / h;
    let min = (min_px / h).clamp(0.0, 0.5);
    frac.clamp(min, 1.0 - min)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn geometry_open_collapsed_and_clamped_tab() {
        let window = dvec2(1440.0, 900.0);
        // open: panel hugs the body, tab straddles the panel's right edge
        let geo = panel_geometry(1.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.panel, Rect { pos: dvec2(0.0, 34.0), size: dvec2(260.0, 866.0) });
        assert_eq!(geo.tab, Rect { pos: dvec2(260.0, 443.0), size: dvec2(14.0, 48.0) });
        // collapsed: panel fully off-screen left, tab pinned to the left edge
        let geo = panel_geometry(0.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.panel, Rect { pos: dvec2(-260.0, 34.0), size: dvec2(260.0, 866.0) });
        assert_eq!(geo.tab, Rect { pos: dvec2(0.0, 443.0), size: dvec2(14.0, 48.0) });
        // half-open: tab tracks the panel edge
        let geo = panel_geometry(0.5, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.panel.pos.x, -130.0);
        // window resize shrinks the panel height
        let geo = panel_geometry(1.0, 0.5, 260.0, dvec2(800.0, 600.0), 34.0);
        assert_eq!(geo.panel.size.y, 566.0);
        // custom width moves the right edge and the tab with it
        let geo = panel_geometry(1.0, 0.5, 360.0, window, 34.0);
        assert_eq!(geo.panel.size.x, 360.0);
        assert_eq!(geo.tab.pos.x, 360.0);
    }

    #[test]
    fn splitter_strip_tracks_split_and_drag_clamps() {
        let window = dvec2(1440.0, 900.0);
        let geo = panel_geometry(1.0, 0.5, 260.0, window, 34.0);
        let panel = geo.panel;
        // strip (12px grab + 3px margins) centered on the divider line
        assert_eq!(geo.splitter, Rect { pos: dvec2(0.0, 458.0), size: dvec2(260.0, 18.0) });
        // dragging the strip center keeps the ratio
        let center = geo.splitter.pos.y + geo.splitter.size.y * 0.5;
        assert!((split_from_y(center, panel, 60.0) - 0.5).abs() < 1e-9);
        // extremes clamp so both sections keep >= 60px
        assert_eq!(split_from_y(panel.pos.y + 6.0, panel, 60.0), 60.0 / 866.0);
        assert_eq!(
            split_from_y(panel.pos.y + 866.0, panel, 60.0),
            1.0 - 60.0 / 866.0
        );
        // collapsed panel: strip slides off-screen with it
        let geo = panel_geometry(0.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.splitter.pos.x, -260.0);
    }

    #[test]
    fn edge_strip_and_width_clamp() {
        let window = dvec2(1440.0, 900.0);
        // edge strip hugs the panel's right edge (8px inside, 4px overhang)
        let geo = panel_geometry(1.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.edge, Rect { pos: dvec2(252.0, 34.0), size: dvec2(12.0, 866.0) });
        // width follows the cursor (right edge at abs x), clamped to 140..520
        let panel = geo.panel;
        assert_eq!(panel_w_from_x(panel.pos.x + 300.0, panel, 140.0, 520.0), 300.0);
        assert_eq!(panel_w_from_x(panel.pos.x + 50.0, panel, 140.0, 520.0), 140.0);
        assert_eq!(panel_w_from_x(panel.pos.x + 700.0, panel, 140.0, 520.0), 520.0);
        // collapsed panel: edge slides off-screen with it
        let geo = panel_geometry(0.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.edge.pos.x, -8.0);
    }
}
