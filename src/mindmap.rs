use makepad_widgets::*;
use crate::markdown_media::MarkdownMediaWidgetRefExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CARD_W: f64 = 360.0;
pub const CARD_H: f64 = 520.0;
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

#[derive(Deserialize, Serialize)]
struct MapFile {
    nodes: Vec<MapNodeFile>,
}

#[derive(Deserialize, Serialize)]
struct MapNodeFile {
    id: String,
    title: String,
    path: String,
    children: Option<Vec<String>>,
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
    pub root: usize,
    pub max_w: f64,
    pub max_h: f64,
}

impl MindMapData {
    pub fn load(base: &Path) -> Option<Self> {
        let map_path = base.join("map.json");
        let map: MapFile = serde_json::from_str(&std::fs::read_to_string(&map_path).ok()?).ok()?;
        let nodes_json = map.nodes;
        let mut nodes: Vec<Node> = nodes_json
            .iter()
            .map(|n| Node {
                id: n.id.clone(),
                title: n.title.clone(),
                path: base.join(&n.path),
                body: std::fs::read_to_string(base.join(&n.path)).unwrap_or_default(),
                parent: None,
                children: Vec::new(),
                pos: DVec2::default(),
                size: dvec2(CARD_W, CARD_H),
                subtree_h: 0.0,
            })
            .collect();
        let id_of = |nodes: &[Node], id: &str| nodes.iter().position(|n| n.id == id);
        let root = id_of(&nodes, "root")?;
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
            if nodes[i].body.is_empty() && nodes[i].title.is_empty() {
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
        };
        data.layout();
        Some(data)
    }

    fn layout(&mut self) {
        self.calc_h(self.root);
        let mut cursor_y = 0.0;
        let mut max_w = 0.0;
        self.place(self.root, 0, &mut cursor_y, &mut max_w);
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

        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.move_to(self.p1.x * self.rect_size.x self.p1.y * self.rect_size.y)
            sdf.line_to(self.p2.x * self.rect_size.x self.p2.y * self.rect_size.y)
            sdf.line_to(self.p3.x * self.rect_size.x self.p3.y * self.rect_size.y)
            sdf.line_to(self.p4.x * self.rect_size.x self.p4.y * self.rect_size.y)
            sdf.stroke(self.line_color self.line_width)
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
            color: #232834
            color_hover: #2a3140
            color_down: #2a3140
            color_focus: #232834
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
            border_radius: 10.0
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
                    sdf.box_y(0. 0. self.rect_size.x self.rect_size.y 10.0 0.0)
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
            // ponytail: makepad clips only rectangularly; keep content 8px
            // off the bottom so code blocks/images never poke past the
            // 6px rounded corners (markdown adds 4px more).
            margin: Inset{bottom: 12}
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
    draw_highlight: DrawColor,
    #[rust]
    area: Area,

    #[rust]
    data: Option<MindMapData>,
    #[rust]
    loaded: bool,
    #[rust]
    cards: Vec<WidgetRef>,
    #[rust]
    edges: Vec<DrawEdge>,
    #[rust]
    detail_ref: Option<WidgetRef>,

    #[rust]
    canvas: Option<DrawList2d>,

    #[rust]
    pan: DVec2,
    #[rust(1.0)]
    zoom: f64,
    #[rust]
    panning: bool,
    #[rust]
    pan_last: DVec2,
    #[rust]
    selected: Option<usize>,
    #[rust]
    detail_open: Option<usize>,
    #[rust]
    drag_card: Option<usize>,
    #[rust]
    drag_grab: DVec2,
    #[rust]
    resize_card: Option<(usize, u8)>,
    #[rust]
    editing_card: Option<usize>,

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
            canvas.begin_always(cx2d);
            canvas.set_view_transform(cx2d, &mat);
            cx2d.begin_root_turtle(dvec2(1e9, 1e9), Layout::flow_down());
            // begin_root_turtle's clip starts at (0,0), which would clamp
            // left/up (negative world coords) content at the origin; pop it
            // and clip to this widget's own world rect instead, so canvas
            // content never renders over the window title bar.
            cx2d.pop_clip_rect();
            let local_view = Rect {
                pos: (view.pos - self.pan) / self.zoom,
                size: view.size / self.zoom,
            };
            cx2d.push_clip_rect(local_view);

            let edges: Vec<(usize, usize)> = self
                .data
                .as_ref()
                .map(|d| d.edges().collect())
                .unwrap_or_default();
            let n = self.data.as_ref().map(|d| d.nodes.len()).unwrap_or(0);

            for (ei, (p, c)) in edges.into_iter().enumerate() {
                let p_rect = self.card_rect(p);
                let c_rect = self.card_rect(c);
                let p1 = p_rect.pos + dvec2(p_rect.size.x, p_rect.size.y * 0.5);
                let p4 = c_rect.pos + dvec2(0.0, c_rect.size.y * 0.5);
                let mid_x = (p1.x + p4.x) * 0.5;
                let p2 = dvec2(mid_x, p1.y);
                let p3 = dvec2(mid_x, p4.y);
                let min_x = p1.x.min(p4.x).min(mid_x) - 4.0;
                let max_x = p1.x.max(p4.x).max(mid_x) + 4.0;
                let min_y = p1.y.min(p4.y).min(p2.y).min(p3.y) - 4.0;
                let max_y = p1.y.max(p4.y).max(p2.y).max(p3.y) + 4.0;
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
                if self.selected == Some(i) {
                    self.draw_highlight.draw_abs(
                        cx2d,
                        Rect {
                            pos: r.pos - dvec2(3.0, 3.0),
                            size: r.size + dvec2(6.0, 6.0),
                        },
                    );
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

            cx2d.end_pass_sized_turtle();
            canvas.end(cx2d);
            self.canvas = Some(canvas);
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
        let local_event = self.remap_event(event);
        let card_event = local_event.as_ref().unwrap_or(event);
        for card in &self.cards {
            card.handle_event(cx, card_event, scope);
        }

        // ponytail: canvas buttons get no reliable FingerHoverOut — hover
        // tracking is one shared slot that our own area overwrites every
        // MouseMove, and the base hover.off animation only advances on
        // NextFrame (Paint-driven, stops when the mouse is still). Snap the
        // hover off ourselves whenever the pointer is outside a visible
        // button; animator_cut is instant and needs no frame ticks.
        if let Some(local) = &local_event {
            if !cx.fingers.any_areas_captured() {
                let mut reset_visible_buttons = |cx: &mut Cx, over: Option<DVec2>| {
                    for card in &self.cards {
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
                    let world = (fe.abs - self.pan) / self.zoom;
                    if let Some((i, dir)) = self.resize_hit(world) {
                        // layout ops are disabled while a card is being edited
                        if self.editing_card.is_none() {
                            self.selected = Some(i);
                            self.resize_card = Some((i, dir));
                            self.redraw(cx);
                        }
                    } else if let Some(i) = self.hit_card(world) {
                        self.selected = Some(i);
                        if fe.tap_count >= 2 {
                            if self.editing_card.is_some() {
                                self.commit_edit(cx);
                            }
                            self.detail_open = Some(i);
                            self.ensure_detail(cx);
                        } else if !child_grabbed && self.editing_card.is_none() {
                            // no card-internal widget (scrollbar, link) grabbed the press
                            self.drag_card = Some(i);
                            self.drag_grab = world - self.data.as_ref().unwrap().nodes[i].pos;
                        }
                        self.redraw(cx);
                    } else {
                        self.panning = true;
                        self.pan_last = fe.abs;
                    }
                }
            }
            Hit::FingerMove(fe) => {
                let world = (fe.abs - self.pan) / self.zoom;
                if let Some((i, dir)) = self.resize_card {
                    if let Some(data) = &mut self.data {
                        let node = &mut data.nodes[i];
                        let min = dvec2(100.0, 100.0);
                        if dir & RESIZE_LEFT != 0 {
                            let w = (node.size.x + node.pos.x - world.x).max(min.x);
                            node.pos.x += node.size.x - w;
                            node.size.x = w;
                        }
                        if dir & RESIZE_RIGHT != 0 {
                            node.size.x = (world.x - node.pos.x).max(min.x);
                        }
                        if dir & RESIZE_TOP != 0 {
                            let h = (node.size.y + node.pos.y - world.y).max(min.y);
                            node.pos.y += node.size.y - h;
                            node.size.y = h;
                        }
                        if dir & RESIZE_BOTTOM != 0 {
                            node.size.y = (world.y - node.pos.y).max(min.y);
                        }
                    }
                    self.redraw(cx);
                } else if let Some(i) = self.drag_card {
                    if let Some(data) = &mut self.data {
                        data.nodes[i].pos = world - self.drag_grab;
                    }
                    self.redraw(cx);
                } else if self.panning {
                    self.pan += fe.abs - self.pan_last;
                    self.pan_last = fe.abs;
                    self.redraw(cx);
                }
            }
            Hit::FingerUp(_) => {
                self.panning = false;
                self.drag_card = None;
                self.resize_card = None;
            }
            Hit::FingerScroll(fe) => {
                if self.detail_open.is_none() && fe.scroll.y != 0.0 {
                    let world = (fe.abs - self.pan) / self.zoom;
                    // Compact cards have no scrollable body, so treat them
                    // like canvas: wheel always zooms.
                    if self.zoom < COMPACT_ZOOM || self.hit_card(world).is_none() {
                        let factor = (1.0 + fe.scroll.y * 0.002).clamp(0.8, 1.25);
                        let new_zoom = (self.zoom * factor).clamp(0.3, 2.5);
                        if (new_zoom - self.zoom).abs() > f64::EPSILON {
                            let w = (fe.abs - self.pan) / self.zoom;
                            self.pan = fe.abs - w * new_zoom;
                            self.zoom = new_zoom;
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
    fn ensure_loaded(&mut self, cx: &mut Cx) {
        self.loaded = true;
        let base = app_base_dir();
        let Some(data) = MindMapData::load(&base) else {
            log!("mindmap: failed to load map.json in {:?}", base);
            return;
        };
        let mut edges = Vec::new();
        for _ in data.edges() {
            edges.push(cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)));
        }
        let n = data.nodes.len();
        self.data = Some(data);
        self.edges = edges;
        self.cards = Vec::with_capacity(n);
        self.canvas = Some(DrawList2d::new(cx));
        self.pan = dvec2(120.0, 60.0);
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
        write_map(&app_base_dir(), data);
    }
}

fn write_map(base: &Path, data: &MindMapData) {
    let nodes = data
        .nodes
        .iter()
        .map(|n| MapNodeFile {
            id: n.id.clone(),
            title: n.title.clone(),
            path: n
                .path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default(),
            children: if n.children.is_empty() {
                None
            } else {
                Some(n.children.iter().map(|&c| data.nodes[c].id.clone()).collect())
            },
        })
        .collect();
    let map = MapFile { nodes };
    if let Ok(json) = serde_json::to_string_pretty(&map) {
        if let Err(e) = std::fs::write(base.join("map.json"), json) {
            log!("mindmap: save map.json failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_write_reload_preserves_title_and_children() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("map.json"),
            r#"{"nodes":[{"id":"root","title":"Rust","path":"a.md","children":["child"]},{"id":"child","title":"","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load(&dir).unwrap();
        assert_eq!(data.nodes[0].title, "Rust");
        data.nodes[0].title = "Rust2".into();
        write_map(&dir, &data);
        let again = MindMapData::load(&dir).unwrap();
        assert_eq!(again.nodes[0].title, "Rust2");
        assert_eq!(again.nodes[0].children, vec![1]);
        assert_eq!(again.nodes[1].children, Vec::<usize>::new());
        assert_eq!(again.nodes[0].body, "hello");
        std::fs::remove_dir_all(&dir).ok();
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
