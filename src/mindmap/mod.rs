use makepad_widgets::*;
use makepad_widgets::makepad_platform::event::{ScrollEvent, ScrollPhase};
use crate::markdown_media::MarkdownMediaWidgetRefExt;
use crate::util::{apply_resize, app_base_dir};
use std::cell::Cell;

mod geometry;
mod minimap;
pub(crate) mod model;
mod nav;
use geometry::draw_edge;
pub use model::*;

// Below this zoom the body text is unreadable, so cards collapse to a
// centered title only (see CardTemplate's compact_title layer).
const COMPACT_ZOOM: f64 = 0.6;

// Ease-out speed for animated pan/zoom (higher = snappier), driven by a
// repeating 60Hz timer with frame-rate-independent dt.
const ZOOM_EASE_SPEED: f64 = 10.0;

// WASD pan speed in screen px/sec (same coordinate space as drag-panning,
// so it feels identical at any zoom). Q/E zoom as an exponential rate/sec.
const MOVE_SPEED: f64 = 1200.0;
const ZOOM_KEY_SPEED: f64 = 1.5;
// Shift+arrow resize speed in screen px/sec (world = /zoom); slower than
// MOVE_SPEED so the size can be dialed in precisely.
const RESIZE_SPEED: f64 = 600.0;
// Ctrl+arrow paging: one page = PAGE_TICKS small instant scrolls (is_mouse:
// false takes ScrollBar's non-smoothing branch), paced at 60Hz by
// `page_timer`. Constant speed, no pause between steps, refresh-rate
// independent — unlike the wheel path, which keeps its own smooth glide
// (smoothing: 0.05 on the card's ScrollYView).
const PAGE_TICKS: u32 = 60; // 60 × 1/60s = 1.0s per page

// Minimap panel, fixed to the bottom-left corner of the map view.
const MM_W: f64 = 240.0;
const MM_H: f64 = 150.0;
const MM_MARGIN: f64 = 12.0;
const MM_PAD: f64 = 8.0;

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawEdge {
    #[deref]
    draw_super: DrawQuad,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawHighlight {
    #[deref]
    draw_super: DrawQuad,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMarquee {
    #[deref]
    draw_super: DrawQuad,
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.DrawEdge = set_type_default() do #(DrawEdge::script_shader(vm)){
        ..mod.draw.DrawQuad

        line_color: uniform(#4a5266)
        line_width: uniform(2.0)
        p1: uniform(vec2(0.0, 0.0))
        p2: uniform(vec2(0.0, 0.0))
        p3: uniform(vec2(0.0, 0.0))
        p4: uniform(vec2(0.0, 0.0))

        // Cubic bezier from p1 (parent's right edge) to p4 (child's left
        // edge) with horizontal-tangent control points p2/p3, tessellated
        // into 24 segments in the shader so the connector is a smooth S-curve.
        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.move_to(self.p1.x * self.rect_size.x self.p1.y * self.rect_size.y)
            for i in 1..25 {
                let t = f32(i) * (1.0 / 24.0)
                let mt = 1.0 - t
                let x = mt*mt*mt*self.p1.x + 3.0*mt*mt*t*self.p2.x + 3.0*mt*t*t*self.p3.x + t*t*t*self.p4.x
                let y = mt*mt*mt*self.p1.y + 3.0*mt*mt*t*self.p2.y + 3.0*mt*t*t*self.p3.y + t*t*t*self.p4.y
                sdf.line_to(x * self.rect_size.x y * self.rect_size.y)
            }
            sdf.stroke(self.line_color self.line_width)
        }
    }

    // Feathered glow pad drawn behind the selected card; the card covers the
    // center, so the visible result is a soft halo hugging the card edge.
    // radius = card corner radius (6); width = halo falloff distance.
    mod.widgets.DrawHighlight = set_type_default() do #(DrawHighlight::script_shader(vm)){
        ..mod.draw.DrawQuad

        // explicit vec4 (not 8-digit hex) so the alpha reaches the shader
        // unambiguously on the uniform path
        color: uniform(vec4(0.49, 0.55, 0.83, 0.45))
        radius: uniform(6.0)
        width: uniform(4.0)

        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(self.width self.width self.rect_size.x - 2.0 * self.width self.rect_size.y - 2.0 * self.width self.radius)
            // shape = signed distance from the card edge (neg inside, pos outside)
            let f = clamp(1.0 - max(sdf.shape, 0.0) / self.width, 0.0, 1.0)
            return vec4(self.color.rgb * self.color.a * f, self.color.a * f)
        }
    }

    // Right-button marquee: faint fill + soft edge glow, same feathered-alpha
    // technique as the card glow. NOTE: box radius 0 degenerates the SDF to
    // dist=0 across the whole interior, which would paint glow/stroke over
    // everything; radius 2 keeps the interior distance negative (2px corners
    // are invisible). All colors are uniforms, tweakable in script.
    mod.widgets.DrawMarquee = set_type_default() do #(DrawMarquee::script_shader(vm)){
        ..mod.draw.DrawQuad

        color: uniform(vec4(0.49, 0.55, 0.83, 0.45))
        fill_alpha: uniform(0.08)
        width: uniform(4.0)

        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(0. 0. self.rect_size.x self.rect_size.y 2.0)
            // g: 1 inside the box, fading out over `width` px outside
            let g = clamp(1.0 - max(sdf.shape, 0.0) / self.width, 0.0, 1.0)
            // e: 1 on the border line, fading over `width` px on both sides
            let e = clamp(1.0 - abs(sdf.shape) / self.width, 0.0, 1.0)
            let a = self.fill_alpha * g + self.color.a * e
            return vec4(self.color.rgb * a, a)
        }
    }

    // Style type for the card edit/done buttons. The draw_bg overrides live
    // at TYPE level (like the theme colors) so card clones from
    // script_from_value get them on the first frame; instance-level uniform
    // overrides are lost until the first animator apply (the "wrong color
    // until hover" bug).
    let CardIconButton = mod.widgets.ButtonFlatIcon{
        padding: Inset{left: 3, right: 3, top: 3, bottom: 3}
        margin: 0
        draw_bg +: {
            color: #1d2129
            color_hover: #232834
            color_down: #232834
            color_focus: #1d2129
            border_size: uniform(0.0)
        }
        // ponytail: hover.off animates 0.1s via NextFrame events, which stop
        // firing when the mouse stops moving on X11 (Paint-driven), leaving
        // the button stuck at the hover color. Snap applies instantly.
        animator +: {
            hover: {
                off: {
                    from: {all: Snap}
                }
            }
        }
    }

    let CardTemplate = mod.widgets.RoundedView{
        width: 360
        height: 520
        flow: Down
        padding: 0
        show_bg: true
        draw_bg +: {
            color: #232834
            border_radius: 6.0
            border_size: 1.0
            border_color: #ffffff1a
        }
        header := mod.widgets.RoundedView{
            height: 44
            flow: Right
            align: Align{y: 0.5}
            spacing: 8
            padding: Inset{left: 14 right: 10}
            draw_bg +: {
                color: #1d2129

                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box_y(0. 0. self.rect_size.x self.rect_size.y 6.0 0.0)
                    sdf.fill(self.color)
                    return sdf.result
                }
            }
            title_box := mod.widgets.View{
                width: Fill
                height: Fit
                title := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 15.0
                    draw_text.color: #e6e9f0
                }
            }
            title_edit_box := mod.widgets.View{
                width: Fill
                height: Fit
                visible: false
                title_edit := mod.widgets.TextInput{
                    width: Fill
                    height: Fit
                    empty_text: ""
                }
            }
            edit_btn := CardIconButton{
                draw_icon +: {
                    svg: crate_resource("self:resources/pen.svg")
                    color: #e6e9f0
                }
                icon_walk: Walk{width: 9, height: 9}
            }
            done_btn := CardIconButton{
                visible: false
                draw_icon +: {
                    svg: crate_resource("self:resources/book.svg")
                    color: #e6e9f0
                }
                icon_walk: Walk{width: 9, height: 9}
            }
        }
        body := mod.widgets.ScrollYView{
            width: Fill
            height: Fill
            // ponytail: makepad clips only rectangularly; keep content 6px
            // off the bottom so code blocks/images never poke past the
            // 6px rounded corners (markdown adds 4px more).
            margin: Inset{bottom: 10}
            // Smooth glide for Ctrl+arrow paging (and wheel): ScrollBar's
            // smoothing routes wheel-style deltas through set_scroll_target,
            // which then animates itself frame-by-frame. ~0.05 → a page in
            // ~0.3s. The explicit mod.widgets.ScrollBars proto is required:
            // an anonymous {...} literal drops the type defaults
            // (show_scroll_x/y, axis) and kills all scrolling.
            scroll_bars: mod.widgets.ScrollBars {
                scroll_bar_x.drag_scrolling: true
                scroll_bar_y.drag_scrolling: true
                scroll_bar_y.smoothing: 0.05
            }
            read_view := mod.widgets.View{
                width: Fill
                height: Fit
                markdown := mod.widgets.MarkdownMedia{
                    width: Fill
                    height: Fit
                }
            }
            edit_view := mod.widgets.View{
                width: Fill
                height: Fill
                visible: false
                body_edit := mod.widgets.TextInput{
                    width: Fill
                    height: Fill
                    is_multiline: true
                    empty_text: ""
                }
            }
        }
        compact_title := mod.widgets.View{
            width: Fill
            height: Fill
            visible: false
            flow: Down
            align: Align{x: 0.5, y: 0.5}
            compact_label := mod.widgets.Label{
                text: ""
                draw_text.text_style.font_size: 24.0
                draw_text.color: #e6e9f0
            }
        }
    }

    let DetailTemplate = mod.widgets.View{
        width: Fill
        height: Fill
        flow: Overlay
        draw_bg +: {
            pixel: fn(){
                #000000cc
            }
        }
        panel := mod.widgets.RoundedView{
            width: Fill
            height: Fill
            margin: 80
            flow: Down
            padding: 20
            spacing: 12
            show_bg: true
            draw_bg +: {
                color: #1f2430
                border_radius: 8.0
                border_size: 1.0
                border_color: #ffffff14
            }
            header := mod.widgets.View{
                height: Fit
                flow: Right
                align: Align{y: 0.5}
                title := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 20.0
                    draw_text.color: #e6e9f0
                }
                close := mod.widgets.ButtonFlat{
                    text: "关闭"
                }
            }
            body := mod.widgets.ScrollYView{
                width: Fill
                height: Fill
                markdown := mod.widgets.MarkdownMedia{
                    width: Fill
                    height: Fit
                    font_size: 14.0
                }
            }
        }
    }

    mod.widgets.MindMapBase = #(MindMap::register_widget(vm))

    mod.widgets.MindMap = set_type_default() do mod.widgets.MindMapBase{
        width: Fill
        height: Fill
        flow: Flow.Overlay
        draw_bg +: {
            color: #14171d
        }
        draw_mm_bg +: {
            color: #1f2430dd
        }
        draw_mm_card +: {
            color: #39404f
        }
        draw_mm_sel +: {
            color: #7d8bd4
        }
        draw_mm_view +: {
            color: #ffffff30
        }
        draw_crosshair +: {
            color: #ffffff40
        }
        card := CardTemplate{}
        detail := DetailTemplate{}
    }
}

/// Active Ctrl+arrow page burst: the card being paged, its scroll direction
/// (±1) and the 60Hz segments left.
#[derive(Clone, Copy)]
struct PageBurst {
    card: usize,
    dir: f64,
    left: u32,
}

/// In-progress marquee selection box, world coords.
#[derive(Clone, Copy)]
struct Marquee {
    start: DVec2,
    end: DVec2,
}

/// In-progress card resize drag (edge handle or Shift+arrow).
#[derive(Clone, Copy)]
struct ResizeDrag {
    card: usize,
    dir: u8,
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct MindMap {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[live]
    draw_bg: DrawColor,
    #[live]
    draw_edge: DrawEdge,
    #[live]
    draw_mm_bg: DrawColor,
    #[live]
    draw_mm_card: DrawColor,
    #[live]
    draw_mm_sel: DrawColor,
    #[live]
    draw_mm_view: DrawColor,
    #[live]
    draw_crosshair: DrawColor,
    #[rust]
    area: Area,

    #[rust]
    data: Option<MindMapData>,
    #[rust]
    loaded: bool,
    /// Map file (relative to the app base dir) this widget is showing and
    /// saving to; switched via MindMapRef::switch_map.
    #[rust("maps/map.json")]
    map_file: String,
    /// Lazily-created card widgets, keyed by node index (None = not yet
    /// created). Never `push` — entries must stay aligned with `data.nodes`
    /// or off-screen cards shift later indices.
    #[rust]
    cards: Vec<Option<WidgetRef>>,
    #[rust]
    edges: Vec<DrawEdge>,
    #[rust]
    highlight: Option<DrawHighlight>,
    #[rust]
    marquee_draw: Option<DrawMarquee>,
    #[rust]
    detail_ref: Option<WidgetRef>,

    #[rust]
    canvas: Option<DrawList2d>,

    #[rust]
    pan: DVec2,
    #[rust(1.0)]
    zoom: f64,
    #[rust(1.0)]
    zoom_target: f64,
    #[rust]
    pan_target: DVec2,
    #[rust]
    zoom_timer: Option<Timer>,
    #[rust]
    last_timer_time: f64,
    /// Ctrl+arrow paging: timer pacing the page burst and the burst state.
    #[rust]
    page_timer: Option<Timer>,
    #[rust]
    page_burst: Option<PageBurst>,
    /// Held navigation keys, bitmask: W=1 A=2 S=4 D=8 Q=16 E=32.
    #[rust]
    key_move: u8,
    /// Held arrow keys, bitmask: Up=1 Down=2 Left=4 Right=8. Moves the
    /// selected cards toward `rect_targets` (see set_arrow).
    #[rust]
    arrow_move: u8,
    /// Held Shift+arrow keys, same bit layout as `arrow_move`; resizes the
    /// selected cards toward `rect_targets` (top-left pinned, bottom-right
    /// handle: Right/Down grow, Left/Up shrink).
    #[rust]
    resize_arrows: u8,
    /// Interpolation targets for the selected cards' positions and sizes.
    #[rust]
    rect_targets: Vec<(usize, Rect)>,
    #[rust]
    panning: bool,
    #[rust]
    pan_last: DVec2,
    #[rust]
    selected: Vec<usize>,
    #[rust]
    marquee: Option<Marquee>,
    #[rust]
    detail_open: Option<usize>,
    #[rust]
    drag_card: Option<usize>,
    #[rust]
    drag_last: DVec2,
    #[rust]
    resize_card: Option<ResizeDrag>,
    #[rust]
    editing_card: Option<usize>,

    // Minimap: panel rect in window coords plus the world->minimap map,
    // cached on the last draw pass for event hit-testing and back-conversion.
    #[rust]
    minimap_rect: Rect,
    #[rust]
    mm_scale: f64,
    #[rust]
    mm_offset: DVec2,
    #[rust]
    mm_dragging: bool,

    /// World-space viewport rect, cached from the last draw pass (mirrors the
    /// card culling in draw_walk). Used by the event path to skip off-screen
    /// cards cheaply.
    #[rust]
    view_rect: Rect,

    #[rust]
    card_template: Option<ScriptObjectRef>,
    #[rust]
    detail_template: Option<ScriptObjectRef>,

}

impl WidgetNode for MindMap {
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
        // Redraw the whole widget incl. its draw-list children (the canvas),
        // otherwise the canvas would never re-issue after its first frame.
        cx.redraw_area_and_children(self.area);
    }

    fn children(&self, visit: &mut dyn FnMut(LiveId, WidgetRef)) {
        for (i, card) in self.cards.iter().enumerate() {
            if let Some(card) = card {
                visit(LiveId(i as u64 + 1), card.clone());
            }
        }
    }

    fn find_widgets_from_point(&self, cx: &Cx, point: DVec2, found: &mut dyn FnMut(&WidgetRef)) {
        let local = self.screen_to_world(point);
        for card in self.cards.iter().flatten() {
            card.find_widgets_from_point(cx, local, found);
        }
    }
}

impl ScriptHook for MindMap {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        _scope: &mut Scope,
        value: ScriptValue,
    ) {
        if apply.is_eval() {
            return;
        }
        if let Some(obj) = value.as_object() {
            vm.vec_with(obj, |vm, vec| {
                for kv in vec {
                    if let Some(id) = kv.key.as_id() {
                        if let Some(template_obj) = kv.value.as_object() {
                            let template_ref = vm.bx.heap.new_object_ref(template_obj);
                            if id == live_id!(card) {
                                self.card_template = Some(template_ref);
                            } else if id == live_id!(detail) {
                                self.detail_template = Some(template_ref);
                            }
                        }
                    }
                }
            });
        }
    }
}

impl Widget for MindMap {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.loaded {
            self.ensure_loaded(cx);
        }
        self.draw_bg.begin(cx, walk, self.layout);
        self.draw_bg.end(cx);

        cx.begin_turtle(walk, self.layout);
        let view = cx.turtle().rect();

        let (detail_title, detail_body, detail_dir) = match self.detail_open {
            Some(di) => self
                .data
                .as_ref()
                .map(|d| {
                    let node = &d.nodes[di];
                    (
                        card_title(node),
                        node.body.clone(),
                        node.path.parent().map(|p| p.to_path_buf()),
                    )
                })
                .unwrap_or_default(),
            None => (String::new(), String::new(), None),
        };

        // Canvas content: always laid out at zoom = 1.0; pan/zoom are a pure
        // GPU view transform, so text layout caches never invalidate and
        // zooming costs the same as panning. It is drawn in a fresh Cx2d with
        // its own root turtle: the window rect clip (which the pass resolves
        // for the main turtle) would otherwise clamp the canvas geometry, so
        // an all-directional clip is the only draw_clip and the GPU viewport
        // does the real clipping.
        let z = self.zoom as f32;
        let mat = Mat4f {
            v: [
                z, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
                self.pan.x as f32, self.pan.y as f32, 0.0, 1.0,
            ],
        };
        if let Some(mut canvas) = self.canvas.take() {
            let dpi = cx.current_dpi_factor();
            let cx2d = &mut Cx2d::new(cx.cx);
            cx2d.set_current_pass_dpi_factor(dpi);
            cx2d.begin_root_turtle(dvec2(1e9, 1e9), Layout::flow_down());
            // begin (not begin_always): skip re-recording when neither this
            // canvas nor any ancestor is dirty, so a floating debug panel can
            // repaint every frame while the map stays cached. The Fixed walk
            // makes the rect check fire on window resize (peek needs a
            // turtle), which begin_always never cared about.
            let redrawing = canvas.begin(cx2d, Walk::fixed(view.size.x, view.size.y));
            if redrawing.is_ok() {
                canvas.set_view_transform(cx2d, &mat);
                // begin_root_turtle's clip starts at (0,0), which would clamp
                // left/up (negative world coords) content at the origin; pop it
                // and clip to this widget's own world rect instead, so canvas
                // content never renders over the window title bar.
                cx2d.pop_clip_rect();
                let local_view = Rect {
                    pos: self.screen_to_world(view.pos),
                    size: view.size / self.zoom,
                };
                self.view_rect = local_view;
                cx2d.push_clip_rect(local_view);

                self.draw_edges(cx2d, local_view);

                self.draw_cards(cx2d, scope, local_view);

                // right-button marquee, drawn on top of the cards
                if let Some(m) = self.marquee {
                    let rect = Rect {
                        pos: dvec2(m.start.x.min(m.end.x), m.start.y.min(m.end.y)),
                        size: dvec2((m.start.x - m.end.x).abs(), (m.start.y - m.end.y).abs()),
                    };
                    if local_view.intersects(rect) {
                        if let Some(md) = &mut self.marquee_draw {
                            md.draw_abs(cx2d, rect);
                        }
                    }
                }

                cx2d.end_pass_sized_turtle();
                canvas.end(cx2d);
            }
            self.canvas = Some(canvas);
        }

        // Minimap: whole-map overview pinned to the bottom-left of the view.
        // Drawn in the main turtle (screen coords), so it stays put while the
        // canvas pans/zooms. The pushed clip keeps the viewport indicator and
        // card rects inside the panel.
        self.draw_minimap(cx, view);

        // Center crosshair while WASD/QE navigation keys are held, showing
        // where a Space press would select (same center as select_view_center).
        if self.key_move != 0 {
            let c = view.pos + view.size * 0.5;
            self.draw_crosshair
                .draw_abs(cx, Rect { pos: c + dvec2(-12.0, -1.25), size: dvec2(24.0, 2.5) });
            self.draw_crosshair
                .draw_abs(cx, Rect { pos: c + dvec2(-1.25, -12.0), size: dvec2(2.5, 24.0) });
        }

        // detail overlay (untransformed, on top)
        if self.detail_open.is_some() {
            if let Some(detail) = &self.detail_ref {
                let _ = detail.draw_walk(cx, scope, Walk::fill());
                detail.label(cx, ids!(title)).set_text(cx, &detail_title);
                let mut md = detail.markdown_media(cx, ids!(markdown));
                md.set_text(cx, &detail_body);
                if let Some(dir) = detail_dir {
                    md.set_base_dir(dir);
                }
            }
        }

        cx.end_turtle_with_area(&mut self.area);

        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.handle_zoom_anim(cx, event);
        self.handle_page_burst(cx, event, scope);
        self.handle_keys(cx, event, scope);
        self.handle_detail_events(cx, event, scope);
        self.handle_edit_buttons(cx, event);
        let on_mm = self.on_minimap(event);
        let local_event = self.remap_event(event);
        let card_event = local_event.as_ref().unwrap_or(event);
        // A press on the minimap must not reach the cards: the event's world
        // back-mapping could coincidentally land inside a card's (untransformed)
        // hit area and, e.g., focus a TextInput mid-edit.
        if !on_mm {
            for card in self.cards.iter().flatten() {
                card.handle_event(cx, card_event, scope);
            }
        }

        if let Some(local) = &local_event {
            if !cx.fingers.any_areas_captured() {
                self.reset_stale_hover(cx, local);
            }
        }

        // Snapshot before event.hits(self.area) below captures the digit to our own
        // area; at this point captures only exist if a card-internal widget
        // (scrollbar thumb, link) grabbed the press.
        let child_grabbed = cx.fingers.any_areas_captured();
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.handle_finger_down(cx, &fe, child_grabbed);
            }
            Hit::FingerDown(fe)
                if matches!(fe.device, DigitDevice::Mouse { button } if button.is_secondary()) =>
            {
                self.handle_finger_down_secondary(cx, &fe);
            }
            Hit::FingerMove(fe) => self.handle_finger_move(cx, &fe),
            Hit::FingerUp(_) => self.handle_finger_up(cx),
            Hit::FingerScroll(fe) => self.handle_finger_scroll(cx, &fe),
            _ => {}
        }
    }
}

impl MindMap {
    /// WASD pan / QE zoom keys. Skipped while a card is being edited
    /// (TextInput owns the keys), the detail panel is open (same rule as
    /// wheel zoom), or the file panel is naming a new map/dir inline.

    /// Detail overlay events: forward to the overlay and close it on its
    /// close button.
    fn handle_detail_events(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let mut close_clicked = false;
        if let Some(detail) = &self.detail_ref {
            detail.handle_event(cx, event, scope);
            if let Event::Actions(actions) = event {
                if detail.button(cx, ids!(close)).clicked(actions) {
                    close_clicked = true;
                }
            }
        }
        if close_clicked {
            self.detail_open = None;
            self.redraw(cx);
        }
    }

    /// Toggle card edit mode on its edit/done buttons.
    fn handle_edit_buttons(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Actions(actions) = event {
            let clicked: Vec<usize> = self
                .cards
                .iter()
                .enumerate()
                .filter_map(|(i, card)| {
                    let card = card.as_ref()?;
                    (card.button(cx, ids!(edit_btn)).clicked(actions)
                        || card.button(cx, ids!(done_btn)).clicked(actions))
                    .then_some(i)
                })
                .collect();
            for i in clicked {
                if self.editing_card == Some(i) {
                    self.commit_edit(cx);
                } else {
                    self.enter_edit(cx, i);
                }
            }
        }
    }

    // ponytail: canvas buttons get no reliable FingerHoverOut — hover
    // tracking is one shared slot that our own area overwrites every
    // MouseMove, and the base hover.off animation only advances on
    // NextFrame (Paint-driven, stops when the mouse is still). Snap the
    // hover off ourselves whenever the pointer is outside a visible
    // button; animator_cut is instant and needs no frame ticks.
    fn reset_stale_hover(&mut self, cx: &mut Cx, local: &Event) {
        let reset_visible_buttons = |cx: &mut Cx, over: Option<DVec2>| {
            for i in 0..self.cards.len() {
                // Only on-screen cards can hold a stale hover: the
                // cursor must be in the viewport to hover a button.
                if !self.view_rect.intersects(self.card_rect(i)) {
                    continue;
                }
                let card = &self.cards[i];
                let Some(card) = card.as_ref() else {
                    continue;
                };
                for id in [ids!(edit_btn), ids!(done_btn)] {
                    let btn = card.button(cx, id);
                    if !btn.visible() {
                        continue;
                    }
                    if let Some(p) = over {
                        if btn.area().rect(cx).contains(p) {
                            continue;
                        }
                    }
                    btn.reset_hover(cx);
                }
            }
        };
        match local {
            Event::MouseMove(e) => {
                reset_visible_buttons(cx, Some(e.abs));
            }
            Event::MouseLeave(_) => {
                reset_visible_buttons(cx, None);
            }
            _ => {}
        }
    }

    /// Primary-button press on the canvas: minimap drag, card resize/drag,
    /// double-click to open the detail, or background pan.
    fn handle_finger_down(&mut self, cx: &mut Cx, fe: &FingerDownEvent, child_grabbed: bool) {
        // Panels (file/refs/float/dock) own their presses; the canvas must
        // not start a pan/drag under them.
        if self.detail_open.is_none() && !crate::util::over_any_panel(fe.abs) {
            if self.minimap_rect.contains(fe.abs) {
                self.mm_dragging = true;
                self.navigate_minimap(cx, fe.abs);
            } else {
                let world = self.screen_to_world(fe.abs);
                if let Some((i, dir)) = self.resize_hit(world) {
                    // layout ops are disabled while a card is being edited
                    if self.editing_card.is_none() {
                        self.resize_card = Some(ResizeDrag { card: i, dir });
                        self.redraw(cx);
                    }
                } else if let Some(i) = self.hit_card(world) {
                    // keep the group when re-pressing an already
                    // selected card, so dragging moves them all
                    if !self.selected.contains(&i) {
                        self.selected = vec![i];
                        self.reanchor_cards(cx);
                    }
                    if fe.tap_count >= 2 {
                        if self.editing_card.is_some() {
                            self.commit_edit(cx);
                        }
                        self.detail_open = Some(i);
                        self.ensure_detail(cx);
                    } else if !child_grabbed && self.editing_card.is_none() {
                        // no card-internal widget (scrollbar, link) grabbed the press
                        self.drag_card = Some(i);
                        self.drag_last = world;
                    }
                    self.redraw(cx);
                } else {
                    self.cancel_zoom_anim(cx);
                    self.panning = true;
                    self.pan_last = fe.abs;
                }
            }
        }
    }

    /// Right-button press: start a marquee selection, skipped over any panel
    /// (file/refs/float), which use right-clicks for themselves.
    fn handle_finger_down_secondary(&mut self, cx: &mut Cx, fe: &FingerDownEvent) {
        if self.detail_open.is_none()
            && self.editing_card.is_none()
            && !self.minimap_rect.contains(fe.abs)
            && !crate::util::over_any_panel(fe.abs)
        {
            let world = self.screen_to_world(fe.abs);
            self.marquee = Some(Marquee {
                start: world,
                end: world,
            });
            self.redraw(cx);
        }
    }

    /// Drag tracking: minimap nav, marquee growth, card resize/drag, pan.
    fn handle_finger_move(&mut self, cx: &mut Cx, fe: &FingerMoveEvent) {
        if self.mm_dragging {
            self.navigate_minimap(cx, fe.abs);
            return;
        }
        if let Some(m) = self.marquee {
            let world = self.screen_to_world(fe.abs);
            self.marquee = Some(Marquee { start: m.start, end: world });
            self.redraw(cx);
            return;
        }
        let world = self.screen_to_world(fe.abs);
        if let Some(r) = self.resize_card {
            if let Some(data) = &mut self.data {
                let node = &mut data.nodes[r.card];
                apply_resize(
                    &mut node.pos,
                    &mut node.size,
                    world,
                    r.dir,
                    dvec2(CARD_MIN_SIZE, CARD_MIN_SIZE),
                    dvec2(CARD_MAX_SIZE, CARD_MAX_SIZE),
                );
            }
            self.redraw(cx);
        } else if self.drag_card.is_some() {
            if let Some(data) = &mut self.data {
                let delta = world - self.drag_last;
                for &j in &self.selected {
                    data.nodes[j].pos += delta;
                }
                self.drag_last = world;
            }
            self.redraw(cx);
        } else if self.panning {
            self.pan += fe.abs - self.pan_last;
            self.pan_target = self.pan;
            self.pan_last = fe.abs;
            self.redraw(cx);
        }
    }

    /// End of a drag: clear transient state, save, and commit any marquee
    /// selection (every card whose rect touches the box; a tiny box, i.e. a
    /// mis-click, clears the selection).
    fn handle_finger_up(&mut self, cx: &mut Cx) {
        self.panning = false;
        self.drag_card = None;
        self.resize_card = None;
        self.mm_dragging = false;
        self.rebuild_targets();
        self.save_map();
        if let Some(m) = self.marquee.take() {
            let rect = Rect {
                pos: dvec2(m.start.x.min(m.end.x), m.start.y.min(m.end.y)),
                size: dvec2((m.start.x - m.end.x).abs(), (m.start.y - m.end.y).abs()),
            };
            if rect.size.x < 4.0 && rect.size.y < 4.0 {
                self.selected.clear();
            } else if let Some(data) = &self.data {
                self.selected = (0..data.nodes.len())
                    .filter(|&i| rect.intersects(self.card_rect(i)))
                    .collect();
            }
            self.reanchor_cards(cx);
            self.redraw(cx);
        }
    }

    /// Wheel zoom, swallowed over the minimap, the detail overlay and any
    /// panel (file/refs/float — their content scrolls instead). Compact
    /// cards have no scrollable body, so wheel over them zooms like canvas.
    fn handle_finger_scroll(&mut self, cx: &mut Cx, fe: &FingerScrollEvent) {
        if !self.minimap_rect.contains(fe.abs)
            && self.detail_open.is_none()
            && fe.scroll.y != 0.0
            && !crate::util::over_any_panel(fe.abs)
        {
            let world = self.screen_to_world(fe.abs);
            if self.zoom < COMPACT_ZOOM || self.hit_card(world).is_none() {
                let factor = (1.0 + fe.scroll.y * 0.002).clamp(0.8, 1.25);
                let new_zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
                if (new_zoom - self.zoom).abs() > f64::EPSILON {
                    let w = self.screen_to_world(fe.abs);
                    self.pan_target = fe.abs - w * new_zoom;
                    self.zoom_target = new_zoom;
                    self.start_zoom_anim(cx);
                    self.redraw(cx);
                }
            }
        }
    }

    /// Ease `pan`/`zoom` one step toward their targets on each timer tick.

    fn ensure_loaded(&mut self, cx: &mut Cx) {
        self.loaded = true;
        self.load_map(cx);
    }

    /// (Re)load `self.map_file` and rebuild all per-map state. Used both for
    /// the initial load and for switching maps; the previous map is already
    /// saved on every interaction, so nothing is flushed here. On failure the
    /// canvas is emptied (so a deleted map can't be resurrected by save_map).
    fn load_map(&mut self, cx: &mut Cx) {
        let base = app_base_dir();
        let map_file = self.map_file.clone();
        let Some(data) = MindMapData::load_from(&base, &map_file) else {
            log!("mindmap: failed to load {} in {:?}", map_file, base);
            self.data = None;
            self.cards.clear();
            self.edges.clear();
            self.selected.clear();
            self.marquee = None;
            self.editing_card = None;
            self.detail_open = None;
            self.mm_dragging = false;
            self.cancel_zoom_anim(cx);
            self.cancel_page_burst(cx);
            self.redraw(cx);
            return;
        };
        let mut edges = Vec::new();
        for _ in data.edges() {
            edges.push(cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)));
        }
        let n = data.nodes.len();
        let saved_view = data.saved_view;
        self.data = Some(data);
        self.edges = edges;
        self.highlight = Some(cx.with_vm(|vm| DrawHighlight::script_new_with_default(vm)));
        self.marquee_draw = Some(cx.with_vm(|vm| DrawMarquee::script_new_with_default(vm)));
        self.cards = Vec::with_capacity(n);
        self.canvas = Some(DrawList2d::new(cx));
        // Per-map transient state must not leak across switches.
        self.selected.clear();
        self.marquee = None;
        self.editing_card = None;
        self.detail_open = None;
        self.drag_card = None;
        self.resize_card = None;
        self.mm_dragging = false;
        self.arrow_move = 0;
        self.resize_arrows = 0;
        self.key_move = 0;
        self.cancel_page_burst(cx);
        self.cancel_zoom_anim(cx);
        self.pan = dvec2(120.0, 60.0);
        self.zoom = 1.0;
        if let Some((p, z)) = saved_view {
            self.pan = p;
            self.zoom = z;
        }
        self.pan_target = self.pan;
        self.zoom_target = self.zoom;
        log!(
            "mindmap ready: {} nodes, {} edges, card_template={}, detail_template={}",
            n,
            self.edges.len(),
            self.card_template.is_some(),
            self.detail_template.is_some()
        );
    }

    fn ensure_detail(&mut self, cx: &mut Cx) {
        if self.detail_ref.is_none() {
            if let Some(t) = &self.detail_template {
                let value = t.as_object().into();
                let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
                self.detail_ref = Some(w);
            }
        }
    }

    /// Draw all card connection edges, culled to the viewport.
    fn draw_edges(&mut self, cx2d: &mut Cx2d, local_view: Rect) {
        let edges: Vec<(usize, usize)> = self
            .data
            .as_ref()
            .map(|d| d.edges().collect())
            .unwrap_or_default();
        for (ei, (p, c)) in edges.into_iter().enumerate() {
            let [p1, p2, p3, p4] = self.edge_curve(p, c);
            let edge = &mut self.edges[ei];
            draw_edge(cx2d, edge, p1, p2, p3, p4, 2.0, Some(local_view));
        }
    }

    /// Draw the cards visible in the viewport: selection highlight, zoom
    /// collapse layers and the edit-mode swap. set_visible no-ops when the
    /// state is unchanged, so calling it every frame is free outside the
    /// zoom threshold; only View implements set_visible, so every toggle
    /// target below is wrapped in a View.
    fn draw_cards(&mut self, cx2d: &mut Cx2d, scope: &mut Scope, local_view: Rect) {
        let compact = self.zoom < COMPACT_ZOOM;
        let n = self.data.as_ref().map(|d| d.nodes.len()).unwrap_or(0);
        for i in 0..n {
            let r = self.card_rect(i);
            if !local_view.intersects(r) {
                continue;
            }
            if self.selected.contains(&i) {
                if let Some(hl) = &mut self.highlight {
                    hl.draw_abs(
                        cx2d,
                        Rect {
                            pos: r.pos - dvec2(4.0, 4.0),
                            size: r.size + dvec2(8.0, 8.0),
                        },
                    );
                }
            }
            let card = self.card_ref(cx2d, i);
            card.view(cx2d, ids!(header)).set_visible(cx2d, !compact);
            card.view(cx2d, ids!(body)).set_visible(cx2d, !compact);
            card.view(cx2d, ids!(compact_title)).set_visible(cx2d, compact);
            // edit mode swaps the read-only render for text inputs
            let editing = self.editing_card == Some(i);
            card.view(cx2d, ids!(title_box)).set_visible(cx2d, !editing);
            card.view(cx2d, ids!(title_edit_box)).set_visible(cx2d, editing);
            card.view(cx2d, ids!(read_view)).set_visible(cx2d, !editing);
            card.view(cx2d, ids!(edit_view)).set_visible(cx2d, editing);
            card.button(cx2d, ids!(edit_btn)).set_visible(cx2d, !editing);
            card.button(cx2d, ids!(done_btn)).set_visible(cx2d, editing);
            let _ = card.draw_walk(
                cx2d,
                scope,
                Walk {
                    abs_pos: Some(r.pos),
                    width: Size::Fixed(r.size.x),
                    height: Size::Fixed(r.size.y),
                    ..Walk::default()
                },
            );
        }
    }

    /// the last drawn (highest index) wins — same z-order as `resize_hit`.

    fn card_ref(&mut self, cx: &mut Cx, i: usize) -> WidgetRef {
        if let Some(Some(c)) = self.cards.get(i) {
            return c.clone();
        }
        let Some(t) = &self.card_template else {
            return WidgetRef::empty();
        };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let node = self.data.as_ref().unwrap().nodes[i].clone();
        let name = card_title(&node);
        w.label(cx, ids!(title)).set_text(cx, &name);
        w.label(cx, ids!(compact_label)).set_text(cx, &name);
        w.markdown_media(cx, ids!(markdown)).set_text(cx, &node.body);
        if let Some(dir) = node.path.parent() {
            w.markdown_media(cx, ids!(markdown)).set_base_dir(dir.to_path_buf());
        }
        if self.cards.len() <= i {
            self.cards.resize(i + 1, None);
        }
        self.cards[i] = Some(w.clone());
        w
    }

    fn enter_edit(&mut self, cx: &mut Cx, i: usize) {
        if self.editing_card.is_some() && self.editing_card != Some(i) {
            self.commit_edit(cx);
        }
        if self.editing_card == Some(i) {
            return;
        }
        let Some(card) = self.cards.get(i).and_then(|c| c.clone()) else {
            return;
        };
        let node = self.data.as_ref().unwrap().nodes[i].clone();
        card.text_input(cx, ids!(title_edit)).set_text(cx, &card_title(&node));
        card.text_input(cx, ids!(body_edit)).set_text(cx, &node.body);
        card.button(cx, ids!(edit_btn)).reset_hover(cx);
        card.button(cx, ids!(done_btn)).reset_hover(cx);
        self.editing_card = Some(i);
        self.redraw(cx);
    }

    fn commit_edit(&mut self, cx: &mut Cx) {
        let Some(i) = self.editing_card.take() else {
            return;
        };
        let Some(card) = self.cards.get(i).and_then(|c| c.clone()) else {
            return;
        };
        // The title input now edits the card's body file name (the header
        // shows the file stem), so committing may rename the .md file and
        // rewrite its path in every map.
        let new_name = card.text_input(cx, ids!(title_edit)).text();
        let new_body = card.text_input(cx, ids!(body_edit)).text();
        let mut renamed = false;
        if let Some(data) = &mut self.data {
            let node = &mut data.nodes[i];
            node.body = new_body;
            if let Err(e) = std::fs::write(&node.path, &node.body) {
                log!("mindmap: save {} failed: {e}", node.path.display());
            }
            if let Some(new_path) = rename_card_file(&app_base_dir(), &node.path, &new_name) {
                renamed = new_path != node.path;
                node.path = new_path;
            }
            let name = card_title(node);
            let body = node.body.clone();
            card.label(cx, ids!(title)).set_text(cx, &name);
            card.label(cx, ids!(compact_label)).set_text(cx, &name);
            card.markdown_media(cx, ids!(markdown)).set_text(cx, &body);
        }
        if renamed {
            self.save_map();
        }
        self.redraw(cx);
    }

    fn save_map(&self) {
        let Some(data) = &self.data else {
            return;
        };
        write_map(&app_base_dir(), data, self.pan_target, self.zoom_target, &self.map_file);
    }
}

impl MindMapRef {
    /// Map file (relative to the app base dir) this widget is showing.
    pub fn current_map_file(&self) -> Option<String> {
        self.borrow().map(|w| w.map_file.clone())
    }

    /// Reload the current map from disk (no same-path early return); used
    /// after external edits like card-dir deletion, so ghost cards don't get
    /// written back on the next save.
    pub fn reload_map(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.load_map(cx);
            inner.redraw(cx);
        }
    }

    /// Switch the map this widget shows and edits; `map_file` is relative to
    /// the app base dir (e.g. "maps/foo.json"). The previous map is already
    /// saved on every interaction, so it is not flushed here.
    pub fn switch_map(&self, cx: &mut Cx, map_file: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            if inner.map_file == map_file {
                return;
            }
            inner.map_file = map_file.to_string();
            inner.load_map(cx);
            inner.redraw(cx);
        }
    }
}

