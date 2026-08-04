use makepad_widgets::*;
use makepad_widgets::makepad_platform::event::{ScrollEvent, ScrollPhase};
use crate::markdown_media::MarkdownMediaWidgetRefExt;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::path::{Path, PathBuf};

pub const CARD_W: f64 = 360.0;
pub const CARD_H: f64 = 520.0;
// Resize limits (mouse and Shift+arrow keyboard resizing share these).
const CARD_MIN_SIZE: f64 = 100.0;
const CARD_MAX_SIZE: f64 = 1000.0;
const GAP_X: f64 = 120.0;
const GAP_Y: f64 = 40.0;
const CANVAS_MARGIN: f64 = 60.0;

const RESIZE_LEFT: u8 = 1;
const RESIZE_RIGHT: u8 = 2;
const RESIZE_TOP: u8 = 4;
const RESIZE_BOTTOM: u8 = 8;

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

#[derive(Deserialize, Serialize)]
struct MapFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pan: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zoom: Option<f64>,
    nodes: Vec<MapNodeFile>,
}

#[derive(Deserialize, Serialize)]
struct MapNodeFile {
    id: String,
    title: String,
    path: String,
    children: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    h: Option<f64>,
}

#[derive(Clone)]
pub struct Node {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub body: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub pos: DVec2,
    pub size: DVec2,
    pub subtree_h: f64,
}

pub struct MindMapData {
    pub nodes: Vec<Node>,
    pub root: Option<usize>,
    pub max_w: f64,
    pub max_h: f64,
    /// View state (pan, zoom) restored from map.json, applied by the widget.
    pub saved_view: Option<(DVec2, f64)>,
}

impl MindMapData {
    /// Default map file, relative to the app base dir.
    pub const DEFAULT_MAP: &'static str = "maps/map.json";

    pub fn load(base: &Path) -> Option<Self> {
        Self::load_from(base, Self::DEFAULT_MAP)
    }

    /// Load the map JSON at `base/map_file`. Node body paths inside the JSON
    /// stay relative to `base` (not the map file's directory).
    pub fn load_from(base: &Path, map_file: &str) -> Option<Self> {
        let map_path = base.join(map_file);
        let map: MapFile = serde_json::from_str(&std::fs::read_to_string(&map_path).ok()?).ok()?;
        let saved_view = match (map.pan, map.zoom) {
            (Some(p), Some(z)) => Some((dvec2(p[0], p[1]), z.clamp(0.3, 2.5))),
            _ => None,
        };
        let nodes_json = map.nodes;
        let mut nodes: Vec<Node> = nodes_json
            .iter()
            .map(|n| Node {
                id: n.id.clone(),
                title: n.title.clone(),
                path: base.join(&n.path),
                body: std::fs::read_to_string(base.join(&n.path)).unwrap_or_else(|_| {
                    log!("mindmap: body file missing for node {}: {:?}", n.id, n.path);
                    String::new()
                }),
                parent: None,
                children: Vec::new(),
                pos: DVec2::default(),
                size: dvec2(CARD_W, CARD_H),
                subtree_h: 0.0,
            })
            .collect();
        let id_of = |nodes: &[Node], id: &str| nodes.iter().position(|n| n.id == id);
        let root = id_of(&nodes, "root");
        // Empty maps are valid (zero nodes); a non-empty map without a root
        // is malformed and fails to load, as before.
        if root.is_none() && !nodes.is_empty() {
            return None;
        }
        for i in 0..nodes_json.len() {
            if let Some(children) = &nodes_json[i].children {
                for cid in children {
                    let ci = id_of(&nodes, cid)?;
                    nodes[ci].parent = Some(i);
                    nodes[i].children.push(ci);
                }
            }
        }
        for i in 0..nodes.len() {
            if nodes[i].body.is_empty() && nodes[i].title.is_empty() && !nodes_json[i].path.is_empty() {
                if let Some(w) = nodes[i].path.file_stem() {
                    nodes[i].title = w.to_string_lossy().into_owned();
                }
            }
        }
        let mut data = MindMapData {
            nodes,
            root,
            max_w: 0.0,
            max_h: 0.0,
            saved_view,
        };
        data.layout();
        // Restore saved card geometry; nodes without it keep the auto layout.
        for (f, j) in nodes_json.iter().zip(&mut data.nodes) {
            if let (Some(x), Some(y)) = (f.x, f.y) {
                j.pos = dvec2(x, y);
            }
            if let (Some(w), Some(h)) = (f.w, f.h) {
                j.size = dvec2(w, h);
            }
        }
        let (mut max_w, mut max_h) = (0.0, 0.0);
        for n in &data.nodes {
            max_w = max_w.max(n.pos.x + n.size.x);
            max_h = max_h.max(n.pos.y + n.size.y);
        }
        data.max_w = max_w + CANVAS_MARGIN;
        data.max_h = max_h + CANVAS_MARGIN;
        Some(data)
    }

    fn layout(&mut self) {
        let Some(root) = self.root else {
            self.max_w = CANVAS_MARGIN;
            self.max_h = CANVAS_MARGIN;
            return;
        };
        self.calc_h(root);
        let mut cursor_y = 0.0;
        let mut max_w = 0.0;
        self.place(root, 0, &mut cursor_y, &mut max_w);
        self.max_h = (cursor_y - GAP_Y).max(CARD_H) + CANVAS_MARGIN;
        self.max_w = max_w + CANVAS_MARGIN;
    }

    fn calc_h(&mut self, i: usize) -> f64 {
        let children = self.nodes[i].children.clone();
        let sum: f64 = children
            .iter()
            .map(|&c| self.calc_h(c))
            .sum::<f64>()
            + (children.len() as f64 - 1.0).max(0.0) * GAP_Y;
        let h = sum.max(CARD_H);
        self.nodes[i].subtree_h = h;
        h
    }

    fn place(&mut self, i: usize, depth: usize, cursor_y: &mut f64, max_w: &mut f64) {
        let x = CANVAS_MARGIN + depth as f64 * (CARD_W + GAP_X);
        *max_w = (*max_w).max(x + CARD_W);
        let y_start = *cursor_y;
        let y = if self.nodes[i].children.is_empty() {
            let y = *cursor_y;
            *cursor_y += CARD_H + GAP_Y;
            y
        } else {
            for c in self.nodes[i].children.clone() {
                self.place(c, depth + 1, cursor_y, max_w);
            }
            (y_start + (*cursor_y - GAP_Y)) / 2.0
        };
        self.nodes[i].pos = dvec2(x, y);
    }

    pub fn edges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.parent.map(|p| (p, i)))
    }
}

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
    #[rust]
    cards: Vec<WidgetRef>,
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
    /// Ctrl+arrow paging: timer pacing the page burst and the burst state
    /// (card index, direction ±1, segments left).
    #[rust]
    page_timer: Option<Timer>,
    #[rust]
    page_burst: Option<(usize, f64, u32)>,
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
    marquee: Option<(DVec2, DVec2)>,
    #[rust]
    detail_open: Option<usize>,
    #[rust]
    drag_card: Option<usize>,
    #[rust]
    drag_last: DVec2,
    #[rust]
    resize_card: Option<(usize, u8)>,
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
            visit(LiveId(i as u64 + 1), card.clone());
        }
    }

    fn find_widgets_from_point(&self, cx: &Cx, point: DVec2, found: &mut dyn FnMut(&WidgetRef)) {
        let local = (point - self.pan) / self.zoom;
        for card in &self.cards {
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
                        node.title.clone(),
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
                pos: (view.pos - self.pan) / self.zoom,
                size: view.size / self.zoom,
                };
                self.view_rect = local_view;
                cx2d.push_clip_rect(local_view);

                let edges: Vec<(usize, usize)> = self
                .data
                .as_ref()
                .map(|d| d.edges().collect())
                .unwrap_or_default();
                let n = self.data.as_ref().map(|d| d.nodes.len()).unwrap_or(0);

                for (ei, (p, c)) in edges.into_iter().enumerate() {
                let [p1, p2, p3, p4] = self.edge_curve(p, c);
                // The curve stays inside the control points' convex hull, so
                // the bbox over all four points always covers it.
                let min_x = p1.x.min(p2.x).min(p3.x).min(p4.x) - 4.0;
                let max_x = p1.x.max(p2.x).max(p3.x).max(p4.x) + 4.0;
                let min_y = p1.y.min(p2.y).min(p3.y).min(p4.y) - 4.0;
                let max_y = p1.y.max(p2.y).max(p3.y).max(p4.y) + 4.0;
                let rect = Rect {
                    pos: dvec2(min_x, min_y),
                    size: dvec2(max_x - min_x, max_y - min_y),
                };
                if !local_view.intersects(rect) {
                    continue;
                    }
                    let edge = &mut self.edges[ei];
                    let to_local = |p: DVec2| {
                        vec2(
                            ((p.x - rect.pos.x) / rect.size.x) as f32,
                            ((p.y - rect.pos.y) / rect.size.y) as f32,
                        )
                    };
                    edge.draw_vars.set_uniform(cx2d, id!(p1), &[to_local(p1).x, to_local(p1).y]);
                    edge.draw_vars.set_uniform(cx2d, id!(p2), &[to_local(p2).x, to_local(p2).y]);
                    edge.draw_vars.set_uniform(cx2d, id!(p3), &[to_local(p3).x, to_local(p3).y]);
                    edge.draw_vars.set_uniform(cx2d, id!(p4), &[to_local(p4).x, to_local(p4).y]);
                    edge.draw_vars.set_uniform(cx2d, id!(line_width), &[2.0]);
                    edge.draw_abs(cx2d, rect);
                }

                let compact = self.zoom < COMPACT_ZOOM;
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
                    // set_visible no-ops when the state is unchanged, so calling
                    // it every frame is free outside the zoom threshold. Only
                    // View implements set_visible, so every toggle target below
                    // is wrapped in a View.
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

                // right-button marquee, drawn on top of the cards
                if let Some((s, e)) = self.marquee {
                    let rect = Rect {
                        pos: dvec2(s.x.min(e.x), s.y.min(e.y)),
                        size: dvec2((s.x - e.x).abs(), (s.y - e.y).abs()),
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
        // WASD pan / QE zoom. Skipped while a card is being edited (TextInput
        // owns the keys), the detail panel is open (same rule as wheel zoom),
        // or the file panel is naming a new map/dir inline.
        if self.editing_card.is_none()
            && self.detail_open.is_none()
            && !crate::file_panel::is_name_editing()
        {
            match event {
                Event::KeyDown(ke) => {
                    if ke.key_code == KeyCode::Space && !ke.is_repeat {
                        self.select_view_center(cx, ke.modifiers.shift);
                    } else if ke.modifiers.control && arrow_mask(ke.key_code).is_some() {
                        // Ctrl+arrow pages the selected card's markdown body;
                        // repeats keep paging. Left/Right are deliberately
                        // absorbed so they never fall through to card movement.
                        self.page_card(ke.key_code, cx, scope);
                    } else {
                        self.set_key_move(ke.key_code, true, cx);
                        // Shift+arrow resizes the selected cards instead of
                        // moving them; also switches a still-held key's mode.
                        let resize = ke.modifiers.shift;
                        self.set_arrow(ke.key_code, false, !resize, cx);
                        self.set_arrow(ke.key_code, true, resize, cx);
                    }
                }
                Event::KeyUp(ke) => {
                    self.set_key_move(ke.key_code, false, cx);
                    self.set_arrow(ke.key_code, false, false, cx);
                    self.set_arrow(ke.key_code, false, true, cx);
                    if arrow_mask(ke.key_code).is_some() {
                        self.cancel_page_burst(cx);
                    }
                }
                _ => {}
            }
        }
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
        if let Event::Actions(actions) = event {
            let clicked: Vec<usize> = self
                .cards
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    card.button(cx, ids!(edit_btn)).clicked(actions)
                        || card.button(cx, ids!(done_btn)).clicked(actions)
                })
                .map(|(i, _)| i)
                .collect();
            for i in clicked {
                if self.editing_card == Some(i) {
                    self.commit_edit(cx);
                } else {
                    self.enter_edit(cx, i);
                }
            }
        }
        let on_mm = self.on_minimap(event);
        let local_event = self.remap_event(event);
        let card_event = local_event.as_ref().unwrap_or(event);
        // A press on the minimap must not reach the cards: the event's world
        // back-mapping could coincidentally land inside a card's (untransformed)
        // hit area and, e.g., focus a TextInput mid-edit.
        if !on_mm {
            for card in &self.cards {
                card.handle_event(cx, card_event, scope);
            }
        }

        // ponytail: canvas buttons get no reliable FingerHoverOut — hover
        // tracking is one shared slot that our own area overwrites every
        // MouseMove, and the base hover.off animation only advances on
        // NextFrame (Paint-driven, stops when the mouse is still). Snap the
        // hover off ourselves whenever the pointer is outside a visible
        // button; animator_cut is instant and needs no frame ticks.
        if let Some(local) = &local_event {
            if !cx.fingers.any_areas_captured() {
                let reset_visible_buttons = |cx: &mut Cx, over: Option<DVec2>| {
                    for i in 0..self.cards.len() {
                        // Only on-screen cards can hold a stale hover: the
                        // cursor must be in the viewport to hover a button.
                        if !self.view_rect.intersects(self.card_rect(i)) {
                            continue;
                        }
                        let card = &self.cards[i];
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
        }

        // Snapshot before event.hits(self.area) below captures the digit to our own
        // area; at this point captures only exist if a card-internal widget
        // (scrollbar thumb, link) grabbed the press.
        let child_grabbed = cx.fingers.any_areas_captured();
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if self.detail_open.is_none() {
                    if self.minimap_rect.contains(fe.abs) {
                        self.mm_dragging = true;
                        self.navigate_minimap(cx, fe.abs);
                    } else {
                        let world = (fe.abs - self.pan) / self.zoom;
                        if let Some((i, dir)) = self.resize_hit(world) {
                            // layout ops are disabled while a card is being edited
                            if self.editing_card.is_none() {
                                self.resize_card = Some((i, dir));
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
            Hit::FingerDown(fe)
                if matches!(fe.device, DigitDevice::Mouse { button } if button.is_secondary()) =>
            {
                // right-button marquee selection; skipped over the file panel,
                // which uses right-clicks for its context menu
                if self.detail_open.is_none()
                    && self.editing_card.is_none()
                    && !self.minimap_rect.contains(fe.abs)
                    && !crate::file_panel::PANEL_RECT
                        .lock()
                        .unwrap()
                        .is_some_and(|r| r.contains(fe.abs))
                {
                    let world = (fe.abs - self.pan) / self.zoom;
                    self.marquee = Some((world, world));
                    self.redraw(cx);
                }
            }
            Hit::FingerMove(fe) => {
                if self.mm_dragging {
                    self.navigate_minimap(cx, fe.abs);
                    return;
                }
                if let Some((s, _)) = self.marquee {
                    let world = (fe.abs - self.pan) / self.zoom;
                    self.marquee = Some((s, world));
                    self.redraw(cx);
                    return;
                }
                let world = (fe.abs - self.pan) / self.zoom;
                if let Some((i, dir)) = self.resize_card {
                    if let Some(data) = &mut self.data {
                        let node = &mut data.nodes[i];
                        let min = dvec2(CARD_MIN_SIZE, CARD_MIN_SIZE);
                        let max = dvec2(CARD_MAX_SIZE, CARD_MAX_SIZE);
                        if dir & RESIZE_LEFT != 0 {
                            let w = (node.size.x + node.pos.x - world.x).clamp(min.x, max.x);
                            node.pos.x += node.size.x - w;
                            node.size.x = w;
                        }
                        if dir & RESIZE_RIGHT != 0 {
                            node.size.x = (world.x - node.pos.x).clamp(min.x, max.x);
                        }
                        if dir & RESIZE_TOP != 0 {
                            let h = (node.size.y + node.pos.y - world.y).clamp(min.y, max.y);
                            node.pos.y += node.size.y - h;
                            node.size.y = h;
                        }
                        if dir & RESIZE_BOTTOM != 0 {
                            node.size.y = (world.y - node.pos.y).clamp(min.y, max.y);
                        }
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
            Hit::FingerUp(_) => {
                self.panning = false;
                self.drag_card = None;
                self.resize_card = None;
                self.mm_dragging = false;
                self.rebuild_targets();
                self.save_map();
                if let Some((s, e)) = self.marquee.take() {
                    // commit the selection: every card whose rect touches the
                    // marquee; a tiny box (mis-click) clears the selection
                    let rect = Rect {
                        pos: dvec2(s.x.min(e.x), s.y.min(e.y)),
                        size: dvec2((s.x - e.x).abs(), (s.y - e.y).abs()),
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
            Hit::FingerScroll(fe) => {
                // Wheel over the minimap is swallowed so it never zooms the map;
                // same for the file panel, whose lists scroll instead.
                if !self.minimap_rect.contains(fe.abs)
                    && self.detail_open.is_none()
                    && fe.scroll.y != 0.0
                    && !crate::file_panel::PANEL_RECT
                        .lock()
                        .unwrap()
                        .is_some_and(|r| r.contains(fe.abs))
                {
                    let world = (fe.abs - self.pan) / self.zoom;
                    // Compact cards have no scrollable body, so treat them
                    // like canvas: wheel always zooms.
                    if self.zoom < COMPACT_ZOOM || self.hit_card(world).is_none() {
                        let factor = (1.0 + fe.scroll.y * 0.002).clamp(0.8, 1.25);
                        let new_zoom = (self.zoom * factor).clamp(0.3, 2.5);
                        if (new_zoom - self.zoom).abs() > f64::EPSILON {
                            let w = (fe.abs - self.pan) / self.zoom;
                            self.pan_target = fe.abs - w * new_zoom;
                            self.zoom_target = new_zoom;
                            self.start_zoom_anim(cx);
                            self.redraw(cx);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl MindMap {
    /// Ease `pan`/`zoom` one step toward their targets on each timer tick.
    fn handle_zoom_anim(&mut self, cx: &mut Cx, event: &Event) {
        let Some(timer) = self.zoom_timer else { return };
        let Some(te) = timer.is_event(event) else { return };
        let now = te.time.unwrap_or(0.0);
        // first tick has no baseline; fall back to one 60Hz frame
        let dt = if self.last_timer_time == 0.0 {
            1.0 / 60.0
        } else {
            (now - self.last_timer_time).max(0.0)
        };
        self.last_timer_time = now;
        // Held-key velocity: WASD moves the pan target (skipped while the
        // mouse is drag-panning so they don't fight), QE zoom center-anchored.
        if self.key_move != 0 && !self.panning {
            let bits = self.key_move;
            let dir = dvec2(
                ((bits >> 1) & 1) as f64 - ((bits >> 3) & 1) as f64, // A - D
                (bits & 1) as f64 - ((bits >> 2) & 1) as f64,        // W - S
            );
            self.pan_target += dir * (MOVE_SPEED * dt);
            let rate = (((bits >> 5) & 1) as f64 - ((bits >> 4) & 1) as f64) * ZOOM_KEY_SPEED;
            if rate != 0.0 {
                // view_rect center is already in world coords; keep it at the
                // same screen position: screen = wc*zoom + pan, solve for pan.
                let wc = self.view_rect.pos + self.view_rect.size * 0.5;
                self.zoom_target = (self.zoom_target * (rate * dt).exp()).clamp(0.3, 2.5);
                self.pan_target = self.pan + wc * (self.zoom - self.zoom_target);
            }
        }
        // Arrow keys: advance the selected cards' position targets
        // (screen-constant speed, like WASD), then ease toward the targets.
        if self.arrow_move != 0 {
            let dir = dvec2(
                ((self.arrow_move >> 3) & 1) as f64 - ((self.arrow_move >> 2) & 1) as f64, // Right - Left
                ((self.arrow_move >> 1) & 1) as f64 - (self.arrow_move & 1) as f64, // Down - Up
            );
            let delta = dir * (MOVE_SPEED / self.zoom) * dt;
            for (_, t) in &mut self.rect_targets {
                t.pos += delta;
            }
        }
        // Shift+arrow: bottom-right handle mode — the top-left corner is
        // pinned, Right/Down grow and Left/Up shrink (100px floor).
        if self.resize_arrows != 0 {
            let rx = ((self.resize_arrows >> 3) & 1) as f64 - ((self.resize_arrows >> 2) & 1) as f64; // Right - Left
            let ry = ((self.resize_arrows >> 1) & 1) as f64 - (self.resize_arrows & 1) as f64; // Down - Up
            let s = (RESIZE_SPEED / self.zoom) * dt;
            for (_, t) in &mut self.rect_targets {
                t.size.x = (t.size.x + rx * s).clamp(CARD_MIN_SIZE, CARD_MAX_SIZE);
                t.size.y = (t.size.y + ry * s).clamp(CARD_MIN_SIZE, CARD_MAX_SIZE);
            }
        }
        let k = 1.0 - (-dt * ZOOM_EASE_SPEED).exp();
        self.zoom += (self.zoom_target - self.zoom) * k;
        self.pan += (self.pan_target - self.pan) * k;
        let mut cards_done = true;
        // Drag/resize own the card geometry; skip the ease so they don't fight.
        if self.drag_card.is_none() && self.resize_card.is_none() {
            if let Some(data) = &mut self.data {
                for &(i, t) in &self.rect_targets {
                    let n = &mut data.nodes[i];
                    n.pos += (t.pos - n.pos) * k;
                    n.size += (t.size - n.size) * k;
                    if (n.pos - t.pos).length() >= 0.5 || (n.size - t.size).length() >= 0.5 {
                        cards_done = false;
                    }
                }
            }
        }
        if (self.zoom_target - self.zoom).abs() < 5e-4
            && (self.pan_target - self.pan).length() < 0.5
            && self.arrow_move == 0
            && self.resize_arrows == 0
            && cards_done
        {
            self.zoom = self.zoom_target;
            self.pan = self.pan_target;
            if let Some(data) = &mut self.data {
                for &(i, t) in &self.rect_targets {
                    data.nodes[i].pos = t.pos;
                    data.nodes[i].size = t.size;
                }
            }
            self.save_map();
            cx.stop_timer(timer);
            self.zoom_timer = None;
        }
        self.redraw(cx);
    }

    /// Ensure the repeating zoom timer is running (idempotent).
    fn start_zoom_anim(&mut self, cx: &mut Cx) {
        if self.zoom_timer.is_none() {
            self.zoom_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_timer_time = 0.0;
        }
    }

    /// Stop animating and pin the targets to the current view, so direct
    /// panning isn't fought by a stale in-flight target.
    fn cancel_zoom_anim(&mut self, cx: &mut Cx) {
        if let Some(t) = self.zoom_timer.take() {
            cx.stop_timer(t);
        }
        self.zoom_target = self.zoom;
        self.pan_target = self.pan;
    }

    /// Select the card under the view center; with `add` (Shift+Space) the
    /// card is added to the selection instead of replacing it.
    fn select_view_center(&mut self, cx: &mut Cx, add: bool) {
        // view_rect is the world-space viewport rect, so its center is the
        // hit point directly (no screen->world conversion).
        let world = self.view_rect.pos + self.view_rect.size * 0.5;
        match self.hit_card(world) {
            Some(i) => {
                if add {
                    if !self.selected.contains(&i) {
                        self.selected.push(i);
                    }
                } else {
                    self.selected = vec![i];
                }
            }
            None => {
                if !add {
                    self.selected.clear();
                }
            }
        }
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Ctrl+arrow: page the first selected card's markdown body by one
    /// viewport. One page = PAGE_TICKS small instant scrolls (is_mouse: false
    /// skips the bar's smoothing glide) paced at 60Hz by `page_timer`, so the
    /// motion is constant-speed and refresh-rate independent. Synthesizes
    /// wheel-like Scroll events at the body's center and dispatches them
    /// through the card's own event path, so makepad's scrollbar state
    /// (clamp, position) stays the single source of truth. Key repeats
    /// extend the burst.
    fn page_card(&mut self, code: KeyCode, cx: &mut Cx, scope: &mut Scope) {
        let Some(&i) = self.selected.first() else {
            return;
        };
        let dir = match code {
            KeyCode::ArrowDown => 1.0,
            KeyCode::ArrowUp => -1.0,
            _ => return,
        };
        if let Some((_, _, left)) = &mut self.page_burst {
            *left += PAGE_TICKS;
        } else {
            self.page_burst = Some((i, dir, PAGE_TICKS));
            self.page_timer = Some(cx.start_interval(1.0 / 60.0));
        }
        self.dispatch_page_tick(cx, scope);
    }

    /// Dispatch one 60Hz tick of the current page burst (a viewport/PAGE_TICKS
    /// instant scroll).
    fn dispatch_page_tick(&mut self, cx: &mut Cx, scope: &mut Scope) {
        let Some((i, dir, _)) = self.page_burst else {
            return;
        };
        let Some(card) = self.cards.get(i) else {
            return;
        };
        // Cards live in world coords, so the body rect (from the last pass)
        // is already world-space; stale/empty rects (compact mode, not yet
        // drawn) fail the contains() check and safely no-op.
        let body_rect = card.view(cx, ids!(body)).area().rect(cx);
        if body_rect.size.y <= 0.0 {
            return;
        }
        let mut e = ScrollEvent {
            // The scroll handling only reads abs/scroll, so any id works.
            window_id: WindowId(0, 0),
            scroll: dvec2(0.0, dir * body_rect.size.y / PAGE_TICKS as f64),
            abs: body_rect.pos + body_rect.size * 0.5,
            modifiers: KeyModifiers::default(),
            handled_x: Cell::new(false),
            handled_y: Cell::new(false),
            // False → instant set_scroll_pos branch: our timer paces the
            // motion, the bar just applies each small step immediately.
            is_mouse: false,
            time: 0.0,
            phase: ScrollPhase::None,
        };
        card.handle_event(cx, &Event::Scroll(e), scope);
    }

    /// Advance the page burst on each timer tick; stops when the held page
    /// count is exhausted.
    fn handle_page_burst(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(timer) = self.page_timer else {
            return;
        };
        if timer.is_event(event).is_none() {
            return;
        }
        let done = if let Some((_, _, left)) = &mut self.page_burst {
            *left -= 1;
            *left == 0
        } else {
            true
        };
        self.dispatch_page_tick(cx, scope);
        if done {
            self.cancel_page_burst(cx);
        }
    }

    /// Stop the page burst; the scroll position is already where the burst
    /// left it (each tick applies instantly).
    fn cancel_page_burst(&mut self, cx: &mut Cx) {
        if let Some(t) = self.page_timer.take() {
            cx.stop_timer(t);
        }
        self.page_burst = None;
    }

    /// Track held WASD/QE keys in the `key_move` bitmask; the first key press
    /// starts the animation timer, which drives the motion until all keys up.
    fn set_key_move(&mut self, code: KeyCode, down: bool, cx: &mut Cx) {
        let mask = match code {
            KeyCode::KeyW => 1,
            KeyCode::KeyA => 2,
            KeyCode::KeyS => 4,
            KeyCode::KeyD => 8,
            KeyCode::KeyQ => 16,
            KeyCode::KeyE => 32,
            _ => return,
        };
        let keys = if down {
            self.key_move | mask
        } else {
            self.key_move & !mask
        };
        if keys != self.key_move {
            self.key_move = keys;
            if keys != 0 {
                self.start_zoom_anim(cx);
            }
        }
    }

    /// Track held arrow keys in the `arrow_move` (move) or `resize_arrows`
    /// (Shift+arrow resize) bitmask; the first press re-anchors the selected
    /// cards' targets and starts the animation timer that eases toward them.
    fn set_arrow(&mut self, code: KeyCode, down: bool, resize: bool, cx: &mut Cx) {
        let Some(mask) = arrow_mask(code) else {
            return;
        };
        let field = if resize {
            &mut self.resize_arrows
        } else {
            &mut self.arrow_move
        };
        let bits = if down {
            *field | mask
        } else {
            *field & !mask
        };
        if bits != *field {
            *field = bits;
            if down {
                self.rebuild_targets();
                self.start_zoom_anim(cx);
            }
        }
    }

    /// Re-anchor rect_targets to the selected cards' current geometry, so a
    /// stale in-flight target can't yank a card after a selection change or
    /// a drag.
    fn rebuild_targets(&mut self) {
        self.rect_targets = self
            .data
            .as_ref()
            .map(|_| self.selected.iter().map(|&i| (i, self.card_rect(i))).collect())
            .unwrap_or_default();
    }

    /// Re-anchor after a selection change; restart the timer if arrow keys
    /// are still held so they keep driving the new selection.
    fn reanchor_cards(&mut self, cx: &mut Cx) {
        self.rebuild_targets();
        if self.arrow_move != 0 || self.resize_arrows != 0 {
            self.start_zoom_anim(cx);
        }
    }

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
            self.cancel_zoom_anim(cx);
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
        self.arrow_move = 0;
        self.resize_arrows = 0;
        self.key_move = 0;
        self.page_burst = None;
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

    fn card_rect(&self, i: usize) -> Rect {
        let node = &self.data.as_ref().unwrap().nodes[i];
        Rect {
            pos: node.pos,
            size: node.size,
        }
    }

    /// World-space bezier points for the connector between parent `p` and
    /// child `c`: start/end at the card edge midpoints, horizontal-tangent
    /// control points (clamped so short links don't get a sharp kink).
    fn edge_curve(&self, p: usize, c: usize) -> [DVec2; 4] {
        let p_rect = self.card_rect(p);
        let c_rect = self.card_rect(c);
        let p1 = p_rect.pos + dvec2(p_rect.size.x, p_rect.size.y * 0.5);
        let p4 = c_rect.pos + dvec2(0.0, c_rect.size.y * 0.5);
        let reach = ((p4.x - p1.x).abs() * 0.5).clamp(60.0, 220.0);
        [p1, p1 + dvec2(reach, 0.0), p4 - dvec2(reach, 0.0), p4]
    }

    fn draw_minimap(&mut self, cx: &mut Cx2d, view: Rect) {
        let mm_rect = Rect {
            pos: view.pos
                + dvec2(view.size.x - MM_W - MM_MARGIN, view.size.y - MM_H - MM_MARGIN),
            size: dvec2(MM_W, MM_H),
        };
        self.minimap_rect = mm_rect;
        cx.push_clip_rect(mm_rect);
        self.draw_mm_bg.draw_abs(cx, mm_rect);

        let (scale, offset) = self
            .data
            .as_ref()
            .map(|data| {
                let scale = ((MM_W - 2.0 * MM_PAD) / data.max_w)
                    .min((MM_H - 2.0 * MM_PAD) / data.max_h);
                let offset = mm_rect.pos
                    + (mm_rect.size - dvec2(data.max_w * scale, data.max_h * scale)) * 0.5;
                self.mm_scale = scale;
                self.mm_offset = offset;
                (scale, offset)
            })
            .unwrap_or((0.0, DVec2::default()));

        // Connectors first, so the cards draw on top. Same bezier points as
        // the canvas, just mapped into minimap space (identical S-curves).
        if scale > 0.0 {
            let to_mm = |p: DVec2| p * scale + offset;
            let edges: Vec<(usize, usize)> = self
                .data
                .as_ref()
                .map(|d| d.edges().collect())
                .unwrap_or_default();
            for (ei, (p, c)) in edges.into_iter().enumerate() {
                let [p1, p2, p3, p4] = self.edge_curve(p, c).map(to_mm);
                let min_x = p1.x.min(p2.x).min(p3.x).min(p4.x) - 4.0;
                let max_x = p1.x.max(p2.x).max(p3.x).max(p4.x) + 4.0;
                let min_y = p1.y.min(p2.y).min(p3.y).min(p4.y) - 4.0;
                let max_y = p1.y.max(p2.y).max(p3.y).max(p4.y) + 4.0;
                let rect = Rect {
                    pos: dvec2(min_x, min_y),
                    size: dvec2(max_x - min_x, max_y - min_y),
                };
                let edge = &mut self.edges[ei];
                let to_local = |p: DVec2| {
                    vec2(
                        ((p.x - rect.pos.x) / rect.size.x) as f32,
                        ((p.y - rect.pos.y) / rect.size.y) as f32,
                    )
                };
                edge.draw_vars.set_uniform(cx, id!(p1), &[to_local(p1).x, to_local(p1).y]);
                edge.draw_vars.set_uniform(cx, id!(p2), &[to_local(p2).x, to_local(p2).y]);
                edge.draw_vars.set_uniform(cx, id!(p3), &[to_local(p3).x, to_local(p3).y]);
                edge.draw_vars.set_uniform(cx, id!(p4), &[to_local(p4).x, to_local(p4).y]);
                // Thinner than the canvas (2.0) so it fits the small panel.
                edge.draw_vars.set_uniform(cx, id!(line_width), &[1.0]);
                edge.draw_abs(cx, rect);
            }
        }

        let card_rects: Vec<Rect> = self
            .data
            .as_ref()
            .map(|data| {
                let to_mm = |p: DVec2| p * scale + offset;
                data.nodes
                    .iter()
                    .map(|n| Rect {
                        pos: to_mm(n.pos),
                        size: n.size * scale,
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (i, r) in card_rects.iter().enumerate() {
            let draw = if self.selected.contains(&i) {
                &mut self.draw_mm_sel
            } else {
                &mut self.draw_mm_card
            };
            draw.draw_abs(cx, *r);
        }

        // Viewport indicator: the current pan/zoom view in world coords.
        if self.mm_scale > 0.0 {
            let world_view = Rect {
                pos: (view.pos - self.pan) / self.zoom,
                size: view.size / self.zoom,
            };
            let to_mm = |p: DVec2| p * self.mm_scale + self.mm_offset;
            self.draw_mm_view.draw_abs(
                cx,
                Rect {
                    pos: to_mm(world_view.pos),
                    size: world_view.size * self.mm_scale,
                },
            );
        }
        cx.pop_clip_rect();
    }

        // Jump the viewport so the minimap point under `abs` becomes the view
        // center; used for click-to-jump and drag-to-navigate. Animates toward
        // the target; during drag the target is re-aimed every move so the
        // view smoothly chases the cursor.
        fn navigate_minimap(&mut self, cx: &mut Cx, abs: DVec2) {
            if self.mm_scale <= 0.0 {
                return;
            }
            let world = (abs - self.mm_offset) / self.mm_scale;
            let view_center = (self.view_rect.pos + self.view_rect.size * 0.5) * self.zoom + self.pan;
            self.pan_target = view_center - world * self.zoom;
            self.zoom_target = self.zoom;
            self.start_zoom_anim(cx);
            self.redraw(cx);
        }

        fn on_minimap(&self, event: &Event) -> bool {
            let hit = |p: &DVec2| self.minimap_rect.contains(*p);
            match event {
                Event::MouseDown(e) => hit(&e.abs),
                Event::MouseMove(e) => hit(&e.abs),
                Event::MouseUp(e) => hit(&e.abs),
                Event::Scroll(e) => hit(&e.abs),
                Event::LongPress(e) => hit(&e.abs),
                Event::TouchUpdate(e) => e.touches.iter().any(|t| hit(&t.abs)),
                _ => false,
            }
        }

    fn hit_card(&self, world: DVec2) -> Option<usize> {
        let data = self.data.as_ref()?;
        for i in 0..data.nodes.len() {
            let n = &data.nodes[i];
            if world.x >= n.pos.x
                && world.x <= n.pos.x + n.size.x
                && world.y >= n.pos.y
                && world.y <= n.pos.y + n.size.y
            {
                return Some(i);
            }
        }
        None
    }

    fn resize_hit(&self, p: DVec2) -> Option<(usize, u8)> {
        let data = self.data.as_ref()?;
        let t = 6.0 / self.zoom;
        for i in (0..data.nodes.len()).rev() {
            let r = self.card_rect(i);
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
            if dir != 0 {
                return Some((i, dir));
            }
        }
        None
    }

    // Cards are laid out in world coords but hit-testing compares raw event
    // abs (window coords) against the untransformed area rects, so map events
    // into the canvas-local space before dispatching to cards.
    fn remap_event(&self, event: &Event) -> Option<Event> {
        let map = |p: DVec2| (p - self.pan) / self.zoom;
        match event {
            Event::MouseDown(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseDown(e))
            }
            Event::MouseMove(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseMove(e))
            }
            Event::MouseUp(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseUp(e))
            }
            Event::MouseLeave(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseLeave(e))
            }
            Event::LongPress(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::LongPress(e))
            }
            Event::Scroll(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::Scroll(e))
            }
            Event::TouchUpdate(e) => {
                let mut e = e.clone();
                for t in &mut e.touches {
                    t.abs = map(t.abs);
                }
                Some(Event::TouchUpdate(e))
            }
            _ => None,
        }
    }

    fn card_ref(&mut self, cx: &mut Cx, i: usize) -> WidgetRef {
        if let Some(c) = self.cards.get(i) {
            return c.clone();
        }
        let Some(t) = &self.card_template else {
            return WidgetRef::empty();
        };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let node = self.data.as_ref().unwrap().nodes[i].clone();
        w.label(cx, ids!(title)).set_text(cx, &node.title);
        w.label(cx, ids!(compact_label)).set_text(cx, &node.title);
        w.markdown_media(cx, ids!(markdown)).set_text(cx, &node.body);
        if let Some(dir) = node.path.parent() {
            w.markdown_media(cx, ids!(markdown)).set_base_dir(dir.to_path_buf());
        }
        self.cards.push(w.clone());
        w
    }

    fn enter_edit(&mut self, cx: &mut Cx, i: usize) {
        if self.editing_card.is_some() && self.editing_card != Some(i) {
            self.commit_edit(cx);
        }
        if self.editing_card == Some(i) {
            return;
        }
        let Some(card) = self.cards.get(i).cloned() else {
            return;
        };
        let node = self.data.as_ref().unwrap().nodes[i].clone();
        card.text_input(cx, ids!(title_edit)).set_text(cx, &node.title);
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
        let Some(card) = self.cards.get(i).cloned() else {
            return;
        };
        let new_title = card.text_input(cx, ids!(title_edit)).text();
        let new_body = card.text_input(cx, ids!(body_edit)).text();
        let mut title_changed = false;
        if let Some(data) = &mut self.data {
            let node = &mut data.nodes[i];
            title_changed = new_title != node.title;
            node.title = new_title;
            node.body = new_body;
            if let Err(e) = std::fs::write(&node.path, &node.body) {
                log!("mindmap: save {} failed: {e}", node.path.display());
            }
            let title = node.title.clone();
            let body = node.body.clone();
            card.label(cx, ids!(title)).set_text(cx, &title);
            card.label(cx, ids!(compact_label)).set_text(cx, &title);
            card.markdown_media(cx, ids!(markdown)).set_text(cx, &body);
        }
        if title_changed {
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

/// Arrow-key bit for a key code (shared by `arrow_move` and `resize_arrows`).
fn arrow_mask(code: KeyCode) -> Option<u8> {
    Some(match code {
        KeyCode::ArrowUp => 1,
        KeyCode::ArrowDown => 2,
        KeyCode::ArrowLeft => 4,
        KeyCode::ArrowRight => 8,
        _ => return None,
    })
}

/// Remove every node whose card path lives under the deleted dir `dir_rel`
/// ("cards/docs/") from all maps under maps/. The root is never removed (its
/// card body just goes missing), and surviving children of removed nodes are
/// re-attached to their nearest surviving ancestor so no dangling children
/// references remain. Only touched maps are written back.
pub(crate) fn remove_dir_nodes(base: &Path, dir_rel: &str) {
    let Some(entries) = std::fs::read_dir(base.join("maps")).ok() else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(json) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(mut map) = serde_json::from_str::<MapFile>(&json) else {
            continue;
        };
        // Owned parent map: id -> parent id (children arrays may be missing).
        let mut parent_of: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for n in &map.nodes {
            if let Some(children) = &n.children {
                for c in children {
                    parent_of.insert(c.clone(), n.id.clone());
                }
            }
        }
        // The removed set, excluding the root (a map must keep a loadable root).
        let root_id = map.nodes.iter().find(|n| n.id == "root").map(|n| n.id.clone());
        let removed: Vec<String> = map
            .nodes
            .iter()
            .filter(|n| Some(&n.id) != root_id.as_ref() && n.path.starts_with(dir_rel))
            .map(|n| n.id.clone())
            .collect();
        if removed.is_empty() {
            continue;
        }
        let is_removed = |id: &str| removed.iter().any(|r| r == id);
        // Strip removed ids from every children list (including the doomed
        // nodes' own lists, so only survivors remain there for re-attach).
        for n in &mut map.nodes {
            if let Some(children) = n.children.as_mut() {
                children.retain(|c| !is_removed(c));
            }
        }
        // Re-attach each removed node's surviving children to its nearest
        // surviving ancestor (walk past other removed nodes).
        for n in &removed {
            let mut ancestor = parent_of.get(n).cloned();
            while let Some(a) = &ancestor {
                if is_removed(a) {
                    ancestor = parent_of.get(a).cloned();
                } else {
                    break;
                }
            }
            let Some(ancestor) = ancestor else {
                continue;
            };
            let survivors: Vec<String> = map
                .nodes
                .iter()
                .find(|node| &node.id == n)
                .and_then(|node| node.children.as_deref())
                .unwrap_or(&[])
                .to_vec();
            if survivors.is_empty() {
                continue;
            }
            if let Some(a) = map.nodes.iter_mut().find(|node| node.id == ancestor) {
                let list = a.children.get_or_insert_with(Vec::new);
                for s in survivors {
                    if !list.contains(&s) {
                        list.push(s);
                    }
                }
            }
        }
        map.nodes.retain(|n| !is_removed(&n.id));
        if let Ok(out) = serde_json::to_string_pretty(&map) {
            std::fs::write(&p, out).ok();
        }
    }
}

/// Minimal map file content for a brand-new map: zero nodes (a truly empty
/// map — the user starts from a blank canvas).
pub fn new_map_json() -> String {
    serde_json::json!({"nodes":[]}).to_string()
}

/// Rewrite node `path` references in every map under maps/ so a renamed
/// card/dir keeps its content wired up. Files match exactly; dirs (trailing
/// "/") match by prefix. Only touched maps are written back.
pub(crate) fn rewrite_node_paths(base: &Path, from_rel: &str, to_rel: &str) {
    let Some(entries) = std::fs::read_dir(base.join("maps")).ok() else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(json) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(mut map) = serde_json::from_str::<MapFile>(&json) else {
            continue;
        };
        let mut changed = false;
        for n in &mut map.nodes {
            if from_rel.ends_with('/') {
                if let Some(rest) = n.path.strip_prefix(from_rel) {
                    n.path = format!("{to_rel}{rest}");
                    changed = true;
                }
            } else if n.path == from_rel {
                n.path = to_rel.to_string();
                changed = true;
            }
        }
        if changed {
            if let Ok(out) = serde_json::to_string_pretty(&map) {
                std::fs::write(&p, out).ok();
            }
        }
    }
}

fn write_map(base: &Path, data: &MindMapData, pan: DVec2, zoom: f64, map_file: &str) {
    let nodes = data
        .nodes
        .iter()
        .map(|n| MapNodeFile {
            id: n.id.clone(),
            title: n.title.clone(),
            path: n
                .path
                .strip_prefix(base)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            children: if n.children.is_empty() {
                None
            } else {
                Some(n.children.iter().map(|&c| data.nodes[c].id.clone()).collect())
            },
            x: Some(n.pos.x),
            y: Some(n.pos.y),
            w: Some(n.size.x),
            h: Some(n.size.y),
        })
        .collect();
    let map = MapFile {
        pan: Some([pan.x, pan.y]),
        zoom: Some(zoom),
        nodes,
    };
    if let Ok(json) = serde_json::to_string_pretty(&map) {
        if let Err(e) = std::fs::write(base.join(map_file), json) {
            log!("mindmap: save {map_file} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_write_reload_preserves_title_and_children() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"Rust","path":"a.md","children":["child"]},{"id":"child","title":"","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load(&dir).unwrap();
        assert_eq!(data.nodes[0].title, "Rust");
        assert_eq!(data.saved_view, None);
        data.nodes[0].title = "Rust2".into();
        data.nodes[0].pos = dvec2(11.0, 22.0);
        data.nodes[0].size = dvec2(300.0, 400.0);
        write_map(&dir, &data, dvec2(5.0, 6.0), 1.5, MindMapData::DEFAULT_MAP);
        let again = MindMapData::load(&dir).unwrap();
        assert_eq!(again.nodes[0].title, "Rust2");
        assert_eq!(again.nodes[0].children, vec![1]);
        assert_eq!(again.nodes[1].children, Vec::<usize>::new());
        assert_eq!(again.nodes[0].body, "hello");
        assert_eq!(again.nodes[0].pos, dvec2(11.0, 22.0));
        assert_eq!(again.nodes[0].size, dvec2(300.0, 400.0));
        assert_eq!(again.saved_view, Some((dvec2(5.0, 6.0), 1.5)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_map_preserves_subdir_path() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test2-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("content")).unwrap();
        std::fs::write(dir.join("content/a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"Rust","path":"content/a.md","children":null}]}"#,
        )
        .unwrap();
        let data = MindMapData::load(&dir).unwrap();
        assert_eq!(data.nodes[0].body, "hello");
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, MindMapData::DEFAULT_MAP);
        let json = std::fs::read_to_string(dir.join("maps/map.json")).unwrap();
        assert!(json.contains("\"path\": \"content/a.md\""), "{json}");
        let again = MindMapData::load(&dir).unwrap();
        assert_eq!(again.nodes[0].body, "hello");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_switches_map_file() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test3-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"One","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("maps/other.json"),
            r#"{"nodes":[{"id":"root","title":"Two","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        let one = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let two = MindMapData::load_from(&dir, "maps/other.json").unwrap();
        assert_eq!(one.nodes[one.root.unwrap()].title, "One");
        assert_eq!(two.nodes[two.root.unwrap()].title, "Two");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn new_map_json_loads_empty() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test4-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("maps/x.json"), new_map_json()).unwrap();
        let data = MindMapData::load_from(&dir, "maps/x.json").unwrap();
        assert!(data.nodes.is_empty());
        assert_eq!(data.root, None);
        // An empty map survives save/reload (write_map iterates all nodes,
        // zero of them, and produces a loadable file again).
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, "maps/x.json");
        let again = MindMapData::load_from(&dir, "maps/x.json").unwrap();
        assert!(again.nodes.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rewrite_node_paths_updates_all_referencing_maps() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test5-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("content/docs")).unwrap();
        let map = |root, path| {
            serde_json::json!({
                "nodes": [
                    {"id": "root", "title": root, "path": path, "children": ["kid"]},
                    {"id": "kid", "title": "", "path": path, "children": null}
                ]
            })
            .to_string()
        };
        std::fs::write(
            dir.join("maps/map.json"),
            map("One", "content/docs/a.md"),
        )
        .unwrap();
        std::fs::write(
            dir.join("maps/other.json"),
            map("Two", "content/docs/a.md"),
        )
        .unwrap();
        std::fs::write(dir.join("maps/untouched.json"), map("Three", "content/b.md")).unwrap();
        // file rename rewrites exact matches in every map
        rewrite_node_paths(&dir, "content/docs/a.md", "content/docs/b.md");
        for f in ["maps/map.json", "maps/other.json"] {
            let json = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(json.contains("content/docs/b.md"), "{f}: {json}");
            assert!(!json.contains("content/docs/a.md"), "{f}: {json}");
        }
        // dir rename rewrites by prefix
        rewrite_node_paths(&dir, "content/docs/", "content/renamed/");
        for f in ["maps/map.json", "maps/other.json"] {
            let json = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(json.contains("content/renamed/b.md"), "{f}: {json}");
        }
        // non-referencing map is untouched
        let json = std::fs::read_to_string(dir.join("maps/untouched.json")).unwrap();
        assert!(json.contains("content/b.md"), "{json}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_dir_nodes_drops_cards_and_reparents_survivors() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test6-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        let json = r#"{"nodes":[
            {"id":"root","title":"R","path":"cards/r.md","children":["A"]},
            {"id":"A","title":"A","path":"cards/docs/a.md","children":["B","C"]},
            {"id":"B","title":"B","path":"cards/b.md","children":null},
            {"id":"C","title":"C","path":"cards/docs/c.md","children":["D"]},
            {"id":"D","title":"D","path":"cards/d.md","children":null}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        // untouched: no refs under the doomed dir
        std::fs::write(
            dir.join("maps/other.json"),
            r#"{"nodes":[{"id":"root","title":"O","path":"cards/x.md","children":null}]}"#,
        )
        .unwrap();
        // root lives inside the doomed dir: it must survive anyway
        std::fs::write(
            dir.join("maps/rootdir.json"),
            r#"{"nodes":[{"id":"root","title":"RD","path":"cards/docs/root.md","children":null}]}"#,
        )
        .unwrap();
        remove_dir_nodes(&dir, "cards/docs/");
        // A and C removed; B and D re-parented to the root
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(data.nodes.len(), 3, "root + B + D");
        assert_eq!(data.nodes[data.root.unwrap()].children, vec![1, 2]);
        assert_eq!(data.nodes[1].title, "B");
        assert_eq!(data.nodes[2].title, "D");
        // root in the doomed dir is kept
        let data = MindMapData::load_from(&dir, "maps/rootdir.json").unwrap();
        assert_eq!(data.nodes.len(), 1);
        // untouched map unchanged
        let json = std::fs::read_to_string(dir.join("maps/other.json")).unwrap();
        assert!(json.contains("cards/x.md"), "{json}");
        std::fs::remove_dir_all(&dir).ok();
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

pub fn app_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::current_dir() {
        return dir;
    }
    PathBuf::from(".")
}
