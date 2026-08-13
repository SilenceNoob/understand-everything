use makepad_widgets::*;
use makepad_widgets::makepad_platform::event::{ScrollEvent, ScrollPhase};
use crate::gen::GenSection;
use crate::markdown_media::MarkdownMediaWidgetRefExt;
use crate::util::data_dir;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;

mod draw;
mod edit;
mod geometry;
mod gesture;
mod groups;
mod menu;
mod minimap;
pub(crate) mod model;
mod nav;
pub use model::*;

// Below this zoom the body text is unreadable, so cards collapse to a
// centered title only (see CardTemplate's compact_title layer).
pub(crate) const COMPACT_ZOOM: f64 = 0.6;

// Ease-out speed for animated pan/zoom (higher = snappier), driven by a
// repeating 60Hz timer with frame-rate-independent dt.
const ZOOM_EASE_SPEED: f64 = 10.0;

// WASD pan speed in screen px/sec (same coordinate space as drag-panning,
// so it feels identical at any zoom). Q/E zoom as an exponential rate/sec.
const MOVE_SPEED: f64 = 1200.0;
// Arrow-key card movement, deliberately slower than WASD pan (≈65% of
// MOVE_SPEED) so cards can be dialed in precisely.
const ARROW_MOVE_SPEED: f64 = 780.0;
const ZOOM_KEY_SPEED: f64 = 1.5;
// Shift+arrow resize speed in screen px/sec (world = /zoom); slower than
// MOVE_SPEED so the size can be dialed in precisely.
const RESIZE_SPEED: f64 = 600.0;
// Alt/Option+arrow paging: one page = PAGE_TICKS small instant scrolls (is_mouse:
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

// Group color picker: preset swatches (5x2 grid) in a popup anchored below
// the group's title bar. Colors tuned for the dark theme.
pub(crate) const GROUP_PRESET_COLORS: [&str; 10] = [
    "#7d8bd4", "#61afef", "#56b6c2", "#98c379", "#e5c07b",
    "#d19a66", "#e06c75", "#c678dd", "#abb2bf", "#e6e9f0",
];
pub(crate) const POPUP_COLS: f64 = 5.0;
pub(crate) const POPUP_SWATCH: f64 = 26.0;
pub(crate) const POPUP_GAP: f64 = 8.0;
pub(crate) const POPUP_PAD: f64 = 10.0;

// Primary-modifier drag grid snap (⌘ on macOS, Ctrl elsewhere): card/group
// positions and resize edges round to GRID_SIZE world px while the key is
// held. The grid lines fade in/out at GRID_EASE_SPEED (≈0.2s) via a 60Hz
// timer, mirroring the zoom anim.
const GRID_SIZE: f64 = 24.0;
const GRID_EASE_SPEED: f64 = 14.0;

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

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawGrid {
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

    // Marquee: faint fill + soft edge glow, same feathered-alpha
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

    // Grid-snap guide: 1px hairlines at GRID_SIZE spacing, drawn in world
    // coords over the viewport so lines pan/zoom with the canvas. The whole
    // grid is one draw call — the shader derives the lines from each
    // fragment's world position (modulo spacing). Alpha driven by the
    // grid_alpha fade (primary modifier held).
    mod.widgets.DrawGrid = set_type_default() do #(DrawGrid::script_shader(vm)){
        ..mod.draw.DrawQuad

        color: uniform(vec4(0.62, 0.68, 0.85, 0.14))
        spacing: uniform(24.0)

        pixel: fn() {
            let p = self.rect_pos + self.pos * self.rect_size
            let g = self.spacing
            let dx = p.x - floor(p.x / g) * g
            let dy = p.y - floor(p.y / g) * g
            let d = min(min(dx, g - dx), min(dy, g - dy))
            let a = self.color.a * (1.0 - min(d, 1.0))
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
            // Learning-order badge ("03"), set from the node's per-map order.
            order_badge := mod.widgets.Label{
                width: Fit
                height: Fit
                visible: false
                text: ""
                margin: Inset{right: 2}
                draw_text.text_style: theme.font_bold{font_size: 11.0}
                draw_text.color: #8a93a6
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
            // Card archetype badge ("联结模型"/"判别模型"), right-aligned
            // next to the edit button; set from the body's `#c 知识类型`
            // marker (hidden from the rendered body by render_body).
            type_badge := mod.widgets.Label{
                width: Fit
                height: Fit
                visible: false
                text: ""
                margin: Inset{right: 2}
                draw_text.text_style: theme.font_bold{font_size: 10.0}
                draw_text.color: #8a93a6
            }
            edit_btn := CardIconButton{
                draw_icon +: {
                    svg: file_resource(#(crate::util::resource_path("pen.svg")))
                    color: #e6e9f0
                }
                icon_walk: Walk{width: 9, height: 9}
            }
            done_btn := CardIconButton{
                visible: false
                draw_icon +: {
                    svg: file_resource(#(crate::util::resource_path("book.svg")))
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
            // Smooth glide for Alt/Option+arrow paging (and wheel): ScrollBar's
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
                    // Body text is selectable (划选生成子卡片); card dragging
                    // therefore starts from the header only (handle_finger_down).
                    selectable: true
                    // Symmetric left/right breathing room; top/bottom keep
                    // the type-default 3px.
                    padding: Inset{left: 12, right: 12, top: 3, bottom: 3}
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

    // Title-bar widget for group frames. Transparent: the colored bar
    // behind it is drawn manually (per-group color), so no instance-level
    // widget uniform state is involved. The color button is a plain tinted
    // icon drawn manually too (hit-tested via color_button_rect).
    let GroupTemplate = mod.widgets.RoundedView{
        height: 24
        flow: Right
        align: Align{y: 0.5}
        spacing: 6
        padding: Inset{left: 10 right: 8}
        title_box := mod.widgets.View{
            width: Fill
            height: Fit
            title := mod.widgets.Label{
                width: Fill
                height: Fit
                text: ""
                draw_text.text_style.font_size: 12.0
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
    }

    // Card right-click context menu: lightweight template cloned for each item.
    let MenuItem = mod.widgets.View{
        width: Fill
        height: (32.0)
        flow: Down
        align: Align{y: 0.5}
        label := mod.widgets.Label{
            max_lines: 1
            text_overflow: TextOverflow.Ellipsis
            width: Fill
            height: Fit
            padding: Inset{left: 10}
            text: ""
            draw_text.text_style.font_size: 13.0
            draw_text.color: #e6e9f0
        }
    }

    let MenuTemplate = mod.widgets.RoundedView{
        width: (220.0)
        height: Fit
        flow: Down
        padding: 6
        show_bg: true
        draw_bg +: {
            color: #2b3140
            border_radius: 3.0
            border_size: 1.0
            border_color: #ffffff3d
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
        draw_grp_icon +: {
            svg: file_resource(#(crate::util::resource_path("palette.svg")))
        }
        draw_menu_hl +: {
            color: #ffffff1a
        }
        ctx_menu := MenuTemplate{
            item0 := MenuItem{ label.text: "从 map 中移除" }
            item1 := MenuItem{ label.text: "生成 ▶" }
            item2 := MenuItem{ label.text: "测试" }
            item3 := MenuItem{ label.text: "设置序号" }
            item4 := MenuItem{ label.text: "生成学习路线" }
            item5 := MenuItem{ label.text: "生成子卡片" }
        }
        // In-canvas 序号 editor, opened by the context menu item; drawn at
        // the card's top-left in world coords (group-rename style).
        order_edit_pop := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Right
            spacing: 6
            padding: Inset{left: 10, right: 10, top: 6, bottom: 6}
            show_bg: true
            draw_bg +: {
                color: #2b3140
                border_radius: 4.0
                border_size: 1.0
                border_color: #ffffff3d
            }
            order_edit_label := mod.widgets.Label{
                width: Fit
                height: Fit
                text: "序号"
                draw_text.text_style.font_size: 12.0
                draw_text.color: #aab0bc
            }
            order_edit_input := mod.widgets.TextInput{
                width: 64
                height: Fit
                is_multiline: false
                empty_text: "留空清除"
                draw_text.text_style.font_size: 12.0
                draw_text.color: #e6e9f0
            }
        }
        sub_menu := mod.widgets.RoundedView{
            width: (180.0)
            height: Fit
            flow: Down
            padding: 6
            show_bg: true
            draw_bg +: {
                color: #2b3140
                border_radius: 3.0
                border_size: 1.0
                border_color: #ffffff3d
            }
            item0 := MenuItem{ label.text: "所有" }
            item1 := MenuItem{ label.text: "抽象描述" }
            item2 := MenuItem{ label.text: "通俗描述" }
            item3 := MenuItem{ label.text: "正例" }
            item4 := MenuItem{ label.text: "负例" }
            item5 := MenuItem{ label.text: "作用" }
            item6 := MenuItem{ label.text: "影响什么" }
            item7 := MenuItem{ label.text: "被什么影响" }
        }
        // File-panel card drag ghost: translucent card preview drawn at the
        // pointer while dragging a card over the canvas.
        drag_ghost := mod.widgets.RoundedView{
            width: (360.0)
            height: (520.0)
            flow: Down
            show_bg: true
            draw_bg +: {
                color: #4c6ef580
                border_radius: 6.0
                border_size: 1.0
                border_color: #7d8bd4cc
            }
            ghost_title := mod.widgets.Label{
                width: Fill
                height: Fit
                padding: Inset{left: 12, right: 12, top: 12, bottom: 12}
                text: ""
                draw_text.text_style.font_size: 18.0
                draw_text.color: #ffffffcc
            }
        }
        card := CardTemplate{}
        group := GroupTemplate{}
    }
}

/// Active Alt/Option+arrow page burst: the card being paged, its scroll direction
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
    /// Palette icon for the per-group color button; the SVG's own white
    /// strokes render as-is (the color uniform keeps its -1 default).
    #[live]
    draw_grp_icon: DrawSvg,
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
    /// Per-card mastery (已见/未见): rel card path -> latest quiz score,
    /// loaded from progress.json at map load; refreshed by reload_progress.
    #[rust]
    progress: HashMap<String, f64>,
    #[rust]
    edges: Vec<DrawEdge>,
    #[rust]
    highlight: Option<DrawHighlight>,
    #[rust]
    marquee_draw: Option<DrawMarquee>,
    #[rust]
    grid_draw: Option<DrawGrid>,

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
    /// Alt/Option+arrow paging: timer pacing the page burst and the burst state.
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
    /// Held Alt/Option+arrow keys, same bit layout as `arrow_move`; resizes
    /// the selected cards toward `rect_targets` (top-left pinned, bottom-right
    /// handle: Right/Down grow, Left/Up shrink). Sizes snap to the grid while
    /// the primary modifier is also held (⌘/Ctrl+Alt+arrow).
    #[rust]
    resize_arrows: u8,
    /// Interpolation targets for the selected cards' positions and sizes.
    #[rust]
    rect_targets: Vec<(usize, Rect)>,
    #[rust]
    panning: bool,
    #[rust]
    pan_last: DVec2,
    /// Primary modifier held (⌘ on macOS, Ctrl elsewhere): grid-snap drags
    /// and the grid lines fade in. Tracked from key events (KeyUp may be
    /// lost on focus change; the map-switch resets below clear the stale
    /// state).
    #[rust]
    ctrl_down: bool,
    /// Grid line alpha, eased toward `grid_alpha_target` by a 60Hz timer.
    #[rust]
    grid_alpha: f64,
    #[rust]
    grid_alpha_target: f64,
    #[rust]
    grid_timer: Option<Timer>,
    #[rust]
    last_grid_time: f64,
    /// Fractional cell motion accumulated for grid-snap arrow movement/resize
    /// (a whole cell is applied only once GRID_SIZE is crossed — round-to-
    /// nearest per tick collapses when the per-tick delta is under half a
    /// cell).
    #[rust]
    grid_accum: DVec2,
    #[rust]
    selected: Vec<usize>,
    #[rust]
    marquee: Option<Marquee>,
    #[rust]
    drag_card: Option<usize>,
    #[rust]
    drag_last: DVec2,
    #[rust]
    resize_card: Option<ResizeDrag>,
    #[rust]
    editing_card: Option<usize>,
    /// In-progress group drag: translating the group moves its member cards
    /// and every nested group's cards.
    #[rust]
    drag_group: Option<usize>,
    /// Groups selected via their title bar (for ⌘/Ctrl+G nesting and the
    /// selection highlight); `selected` holds their member cards.
    #[rust]
    selected_groups: Vec<usize>,
    /// Group whose title is being renamed (TextInput shown on the title bar).
    #[rust]
    editing_group: Option<usize>,
    /// Lazily-created group title widgets, keyed by group index; entries must
    /// stay aligned with `data.groups` (rebuilt on any structural change).
    #[rust]
    group_refs: Vec<Option<WidgetRef>>,
    /// Per-group frame draws (the DrawMarquee border shader), one per group.
    #[rust]
    group_draws: Vec<DrawMarquee>,
    /// Group whose color picker popup is open; None = closed. The popup is
    /// drawn in screen space (main turtle) next to the group's title bar.
    #[rust]
    color_popup: Option<usize>,
    /// Group whose color button the pointer hovers (manual hover tracking
    /// for the drawn icon button, redrawn on change).
    #[rust]
    hover_color_btn: Option<usize>,
    /// Popup panel rect (window coords), cached on the last draw pass for
    /// swatch hit-testing.
    #[rust]
    popup_rect: Rect,
    /// Shared draw for the popup panel and its swatches.
    #[rust]
    popup_draw: Option<DrawMarquee>,

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

    #[live]
    draw_menu_hl: DrawColor,
    /// Right-click context menu templates and cached widget instances.
    #[rust]
    ctx_menu_template: Option<ScriptObjectRef>,
    #[rust]
    sub_menu_template: Option<ScriptObjectRef>,
    #[rust]
    drag_ghost_template: Option<ScriptObjectRef>,
    #[rust]
    ctx_menu_ref: Option<WidgetRef>,
    #[rust]
    sub_menu_ref: Option<WidgetRef>,
    /// Cached drag-ghost widget (file-panel card drag preview).
    #[rust]
    drag_ghost_ref: Option<WidgetRef>,
    /// Right-button press state: screen position + optional hit card index.
    /// A small drag (no menu) falls back to pan; a release without drag opens
    /// the card context menu.
    #[rust]
    sec_press: Option<(DVec2, Option<usize>)>,
    /// Open card context menu and its hit-test rect in screen coords.
    #[rust]
    menu_open: bool,
    #[rust]
    menu_card: Option<usize>,
    #[rust]
    menu_card_path: String,
    /// Selected body text of the menu card at open time ("" = none); drives
    /// the 生成子卡片 row and the action payload.
    #[rust]
    menu_card_selection: String,
    /// Whether the 生成学习路线 row is shown (root goal card without
    /// children) / whether the 生成子卡片 row is shown (non-empty selection).
    #[rust]
    menu_plan_row: bool,
    #[rust]
    menu_subcard_row: bool,
    /// True while the App is planning a learning route: hides the
    /// 生成学习路线 row to prevent re-entry mid-plan.
    #[rust]
    route_planning: bool,
    /// Visible menu row count: 5 on the root goal card (生成学习路线),
    /// 4 everywhere else. Drives geometry and the item4 visibility.
    #[rust]
    menu_items: usize,
    /// Title indicators (e.g. "规划中…") set before a card widget existed
    /// (lazily created on draw); applied by `card_ref`, cleared explicitly.
    #[rust]
    pending_titles: HashMap<PathBuf, String>,
    #[rust]
    menu_rect: Rect,
    #[rust]
    menu_hover: Option<usize>,
    #[rust]
    sub_open: bool,
    #[rust]
    sub_rect: Rect,
    #[rust]
    sub_hover: Option<usize>,
    /// Window-wide capture area while the menu is open.
    #[rust]
    menu_modal_area: Area,
    /// World position for the next canvas-added card (set on canvas
    /// right-click, consumed by App via add_card_at).
    #[rust]
    picker_world: DVec2,

    #[rust]
    card_template: Option<ScriptObjectRef>,
    #[rust]
    group_template: Option<ScriptObjectRef>,
    /// 序号 editor: the card index being edited, its lazily-created widget,
    /// and the DSL template the widget is cloned from.
    #[rust]
    order_editing: Option<usize>,
    #[rust]
    order_edit_ref: Option<WidgetRef>,
    #[rust]
    order_edit_template: Option<ScriptObjectRef>,
    /// Keyboard focus for the order input once its widget has been drawn.
    #[rust]
    order_focus_pending: bool,
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
        let base = self.cards.len() as u64 + 1;
        for (i, g) in self.group_refs.iter().enumerate() {
            if let Some(g) = g {
                visit(LiveId(base + i as u64 + 1), g.clone());
            }
        }
    }

    fn find_widgets_from_point(&self, cx: &Cx, point: DVec2, found: &mut dyn FnMut(&WidgetRef)) {
        let local = self.screen_to_world(point);
        for card in self.cards.iter().flatten() {
            card.find_widgets_from_point(cx, local, found);
        }
        for g in self.group_refs.iter().flatten() {
            g.find_widgets_from_point(cx, local, found);
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
                            } else if id == live_id!(group) {
                                self.group_template = Some(template_ref);
                            } else if id == live_id!(ctx_menu) {
                                self.ctx_menu_template = Some(template_ref);
                            } else if id == live_id!(sub_menu) {
                                self.sub_menu_template = Some(template_ref);
                            } else if id == live_id!(drag_ghost) {
                                self.drag_ghost_template = Some(template_ref);
                            } else if id == live_id!(order_edit_pop) {
                                self.order_edit_template = Some(template_ref);
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

                // Grid-snap guide, behind everything else; one draw call for
                // the whole viewport (lines derived in the shader).
                if self.grid_alpha > 0.003 {
                    if let Some(gd) = &mut self.grid_draw {
                        let a = (self.grid_alpha * 0.14) as f32;
                        gd.draw_vars.set_uniform(cx2d, id!(color), &[0.62, 0.68, 0.85, a]);
                        gd.draw_vars
                            .set_uniform(cx2d, id!(spacing), &[GRID_SIZE as f32]);
                        gd.draw_abs(cx2d, local_view);
                    }
                }

                self.draw_edges(cx2d, local_view);

                self.draw_groups(cx2d, scope, local_view);

                self.draw_cards(cx2d, scope, local_view);

                // marquee, drawn on top of the cards
                if let Some(m) = self.marquee {
                    let rect = Rect {
                        pos: dvec2(m.start.x.min(m.end.x), m.start.y.min(m.end.y)),
                        size: dvec2((m.start.x - m.end.x).abs(), (m.start.y - m.end.y).abs()),
                    };
                    if local_view.intersects(rect) {
                        if let Some(md) = &mut self.marquee_draw {
                            md.draw_vars.set_uniform(cx2d, id!(color), &[0.49, 0.55, 0.83, 0.45]);
                            md.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[0.08]);
                            md.draw_vars.set_uniform(cx2d, id!(width), &[4.0]);
                            md.draw_abs(cx2d, rect);
                        }
                    }
                }

                // 序号 editor on top of the cards.
                if self.order_editing.is_some() {
                    self.draw_order_edit(cx2d, scope);
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

        // Group color picker popup, anchored next to the title bar.
        if self.color_popup.is_some() {
            self.draw_color_popup(cx, view);
        }

        // Card right-click context menu, drawn in screen coords on top of canvas.
        self.draw_card_menu(cx, scope, view);

        // File-panel card drag preview (ghost card at the pointer).
        self.draw_drag_ghost(cx, scope, view);

        // Center crosshair while WASD/QE navigation keys are held, showing
        // where a Space press would select (same center as select_view_center).
        if self.key_move != 0 {
            let c = view.pos + view.size * 0.5;
            self.draw_crosshair
                .draw_abs(cx, Rect { pos: c + dvec2(-12.0, -1.25), size: dvec2(24.0, 2.5) });
            self.draw_crosshair
                .draw_abs(cx, Rect { pos: c + dvec2(-1.25, -12.0), size: dvec2(2.5, 24.0) });
        }

        cx.end_turtle_with_area(&mut self.area);

        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Reset the cursor left behind by panel edge hovers (ColResize etc.);
        // the canvas gets no raw MouseMove events of its own, so this is the
        // only place that can restore the default when the pointer returns.
        // Panels later in the tree (file/refs) re-apply their own cursor.
        cx.set_cursor(MouseCursor::Default);
        if self.menu_open {
            self.handle_card_menu_events(cx, event, scope);
            return;
        }
        // 序号 editor keyboard: Enter commits, Esc cancels.
        if self.order_editing.is_some() {
            if let Event::Actions(actions) = event {
                if let Some(w) = &self.order_edit_ref {
                    let input = w.text_input(cx, ids!(order_edit_input));
                    if input.escaped(actions) {
                        self.cancel_order_edit(cx);
                        return;
                    }
                    if input.returned(actions).is_some() {
                        self.commit_order_edit(cx);
                        return;
                    }
                }
            }
        }
        self.handle_zoom_anim(cx, event);
        self.handle_grid_anim(cx, event);
        self.handle_page_burst(cx, event, scope);
        self.handle_keys(cx, event, scope);
        self.handle_grid_key(cx, event);
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
            for g in self.group_refs.iter().flatten() {
                g.handle_event(cx, card_event, scope);
            }
            if let Some(w) = &self.order_edit_ref {
                w.handle_event(cx, card_event, scope);
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
        let base = data_dir();
        let map_file = self.map_file.clone();
        let Some(data) = MindMapData::load_from(&base, &map_file) else {
            log!("mindmap: failed to load {} in {:?}", map_file, base);
            self.data = None;
            self.cards.clear();
            self.edges.clear();
            self.progress.clear();
            self.group_refs.clear();
            self.group_draws.clear();
            self.color_popup = None;
            self.hover_color_btn = None;
            self.selected.clear();
            self.selected_groups.clear();
            self.marquee = None;
            self.editing_card = None;
            self.editing_group = None;
            self.order_editing = None;
            self.order_edit_ref = None;
            self.order_focus_pending = false;
            self.menu_open = false;
            self.sub_open = false;
            self.menu_card = None;
            self.menu_card_path.clear();
            self.menu_hover = None;
            self.sub_hover = None;
            self.sec_press = None;
            self.mm_dragging = false;
            self.cancel_zoom_anim(cx);
            self.cancel_page_burst(cx);
            self.reset_grid_state(cx);
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
        self.progress = crate::mindmap::model::load_progress(&base);
        self.highlight = Some(cx.with_vm(|vm| DrawHighlight::script_new_with_default(vm)));
        self.marquee_draw = Some(cx.with_vm(|vm| DrawMarquee::script_new_with_default(vm)));
        self.grid_draw = Some(cx.with_vm(|vm| DrawGrid::script_new_with_default(vm)));
        self.cards = Vec::with_capacity(n);
        self.canvas = Some(DrawList2d::new(cx));
        self.rebuild_group_widgets(cx);
        self.popup_draw = Some(cx.with_vm(|vm| DrawMarquee::script_new_with_default(vm)));
        // Per-map transient state must not leak across switches.
        self.selected.clear();
        self.selected_groups.clear();
        self.marquee = None;
        self.editing_card = None;
        self.editing_group = None;
        self.order_editing = None;
        self.order_edit_ref = None;
        self.order_focus_pending = false;
        self.drag_card = None;
        self.drag_group = None;
        self.resize_card = None;
        self.mm_dragging = false;
        self.arrow_move = 0;
        self.resize_arrows = 0;
        self.key_move = 0;
        self.menu_open = false;
        self.sub_open = false;
        self.menu_card = None;
        self.menu_card_path.clear();
        self.menu_hover = None;
        self.sub_hover = None;
        self.sec_press = None;
        self.cancel_page_burst(cx);
        self.cancel_zoom_anim(cx);
        self.reset_grid_state(cx);
        self.pan = dvec2(120.0, 60.0);
        self.zoom = 1.0;
        if let Some((p, z)) = saved_view {
            self.pan = p;
            self.zoom = z;
        }
        self.pan_target = self.pan;
        self.zoom_target = self.zoom;
        log!(
            "mindmap ready: {} nodes, {} edges, card_template={}",
            n,
            self.edges.len(),
            self.card_template.is_some()
        );
    }

}

/// Action emitted by the MindMap card context menu for the App to handle.
#[derive(Clone, Debug, Default)]
pub enum MindMapAction {
    #[default]
    None,
    Generate(String, GenSection),
    Quiz(String),
    /// Root goal card: plan the learning route under it.
    PlanRoute(String),
    /// 划选生成子卡片: (parent card rel path, selected body text).
    GenSubCard(String, String),
    /// Canvas right-click at the given screen position: open the card picker.
    CanvasMenu(DVec2),
}

impl MindMapRef {
    /// Map file (relative to the app base dir) this widget is showing.
    pub fn current_map_file(&self) -> Option<String> {
        self.borrow().map(|w| w.map_file.clone())
    }

    /// Poll the action list for a "生成" menu click; returns the card path and
    /// the requested section.
    pub fn generate_clicked(&self, actions: &Actions) -> Option<(String, GenSection)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let MindMapAction::Generate(p, s) = item.cast() {
                return Some((p, s));
            }
        }
        None
    }

    /// Poll the action list for a "测试" menu click; returns the card path.
    pub fn quiz_clicked(&self, actions: &Actions) -> Option<String> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let MindMapAction::Quiz(p) = item.cast() {
                return Some(p);
            }
        }
        None
    }

    /// Poll the action list for a "生成学习路线" menu click; returns the
    /// root goal card's path.
    pub fn route_clicked(&self, actions: &Actions) -> Option<String> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let MindMapAction::PlanRoute(p) = item.cast() {
                return Some(p);
            }
        }
        None
    }

    /// Poll the action list for a "生成子卡片" menu click; returns (parent
    /// card rel path, selected body text).
    pub fn subcard_clicked(&self, actions: &Actions) -> Option<(String, String)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let MindMapAction::GenSubCard(p, s) = item.cast() {
                return Some((p, s));
            }
        }
        None
    }

    /// Add `child_rel` as a child of the card at `parent_rel` on the canvas.
    pub fn add_child_card(&self, cx: &mut Cx, parent_rel: &str, child_rel: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.add_child_card(cx, parent_rel, child_rel);
        }
    }

    /// Update the in-memory card body and any live widget for the card at `full_path`.
    pub fn update_card_body(&self, cx: &mut Cx, full_path: &std::path::Path, body: String) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.update_card_body(cx, full_path, body);
        }
    }

    /// Set or restore the visible title indicator of the card at `full_path`.
    pub fn set_card_title_indicator(&self, cx: &mut Cx, full_path: &std::path::Path, indicator: Option<&str>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_card_title_indicator(cx, full_path, indicator);
        }
    }

    /// Mark whether a learning-route plan is in flight; while true the card
    /// context menu hides the 生成学习路线 row (set by the App at plan start
    /// and cleared on completion/abort).
    pub fn set_route_planning(&self, cx: &mut Cx, on: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            if inner.route_planning != on {
                inner.route_planning = on;
                inner.redraw(cx);
            }
        }
    }

    /// Poll the action list for a canvas right-click; returns the screen pos.
    pub fn canvas_menu_clicked(&self, actions: &Actions) -> Option<DVec2> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let MindMapAction::CanvasMenu(p) = item.cast() {
                return Some(p);
            }
        }
        None
    }

    /// Add the card file at `rel_path` to the canvas at the last right-click
    /// world position (see `add_card_at`).
    pub fn add_card_at(&self, cx: &mut Cx, rel_path: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.add_card_at(cx, rel_path);
        }
    }

    /// Drop the card file at `rel_path` onto the canvas at screen pos `abs`
    /// (no-op unless the pointer is over the canvas).
    pub fn drop_card_at(&self, cx: &mut Cx, rel_path: &str, abs: DVec2) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.drop_card_at(cx, rel_path, abs);
        }
    }

    /// Redraw the canvas (used by the App to keep the drag ghost current).
    pub fn redraw(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.redraw(cx);
        }
    }

    /// Rel paths of every card currently on the map.
    pub fn card_rel_paths(&self) -> Vec<String> {
        self.borrow().map(|w| w.card_rel_paths()).unwrap_or_default()
    }

    /// Whether the current map has a live root card (node present, body file
    /// intact). A map whose root card body was deleted is treated as rootless.
    pub fn has_root(&self) -> bool {
        self.borrow()
            .map(|w| {
                w.data.as_ref().is_some_and(|d| {
                    d.root.is_some_and(|i| d.nodes.get(i).is_some_and(|n| n.path.exists()))
                })
            })
            .unwrap_or(false)
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

    /// Refresh the 已见/未见 badges after a quiz was graded.
    pub fn reload_progress(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.reload_progress(cx);
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

/// Set a card's header title, compact title and order badge from its display
/// title and learning-order number. The compact label carries the number as a
/// "03·" prefix; the badge is hidden when there's no order.
/// The body shown in the card: the `#c 知识类型` archetype marker is
/// metadata (now surfaced as the header badge), so those lines are skipped.
/// The file itself keeps the marker — `card_type` and generation logic read
/// it from `body`.
pub(crate) fn render_body(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.lines() {
        if line.trim_start().starts_with("#c 知识类型") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    while out.ends_with('\n') {
        out.pop();
    }
    // The marker usually sits at the top of the body; dropping it leaves a
    // leading blank line that would push the content down — trim that too.
    while out.starts_with('\n') {
        out.drain(..1);
    }
    out
}

/// Set a card's header texts: title, compact label, learning-order badge,
/// and the archetype badge (联结模型/判别模型) derived from `ctype`.
pub(crate) fn set_card_texts(cx: &mut Cx, card: &WidgetRef, title: &str, order: Option<u32>, ctype: crate::gen::CardType) {
    card.label(cx, ids!(title)).set_text(cx, title);
    let compact = match order {
        Some(n) => format!("{n:02}·{title}"),
        None => title.to_string(),
    };
    card.label(cx, ids!(compact_label)).set_text(cx, &compact);
    let badge = card.label(cx, ids!(order_badge));
    match order {
        Some(n) => {
            badge.set_text(cx, &format!("{n:02}"));
            badge.set_visible(cx, true);
        }
        None => badge.set_visible(cx, false),
    }
    let tbadge = card.label(cx, ids!(type_badge));
    let (text, color) = match ctype {
        crate::gen::CardType::Knowledge => ("联结模型", 0x7aa2f7ff),
        crate::gen::CardType::Concept => ("判别模型", 0x8a93a6ff),
    };
    tbadge.set_text(cx, text);
    tbadge.set_text_color(cx, Vec4f::from_u32(color));
    tbadge.set_visible(cx, true);
}

#[cfg(test)]
mod tests {
    use super::render_body;

    #[test]
    fn render_body_skips_ctype_marker_only() {
        let body = "#c 知识类型 联结模型\n\n#d 学习目标\n内容\n#c 作用 用途\n";
        let out = render_body(body);
        assert_eq!(out, "#d 学习目标\n内容\n#c 作用 用途");
        // 概念 marker, 行首带空格, 出现在中间：同样跳过。
        let body2 = "先导内容\n  #c 知识类型 概念\n后续内容\n";
        assert_eq!(render_body(body2), "先导内容\n后续内容");
    }

    #[test]
    fn render_body_keeps_body_without_marker() {
        let body = "#d 抽象描述\n特征\n";
        assert_eq!(render_body(body), "#d 抽象描述\n特征");
        assert_eq!(render_body(""), "");
    }
}

