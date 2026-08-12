use makepad_widgets::*;
use makepad_widgets::makepad_platform::event::{ScrollEvent, ScrollPhase};
use crate::gen::GenSection;
use crate::markdown_media::MarkdownMediaWidgetRefExt;
use crate::slide_panel::{menu_item_index, menu_rect, MENU_ITEM_H, MENU_PAD};
use crate::util::{apply_resize, app_base_dir};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;

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
const GROUP_PRESET_COLORS: [&str; 10] = [
    "#7d8bd4", "#61afef", "#56b6c2", "#98c379", "#e5c07b",
    "#d19a66", "#e06c75", "#c678dd", "#abb2bf", "#e6e9f0",
];
const POPUP_COLS: f64 = 5.0;
const POPUP_SWATCH: f64 = 26.0;
const POPUP_GAP: f64 = 8.0;
const POPUP_PAD: f64 = 10.0;

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
            svg: crate_resource("self:resources/palette.svg")
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
    /// WASD pan / QE zoom keys. Skipped while a card is being edited
    /// (TextInput owns the keys), or the file panel is naming a new
    /// map/dir inline.

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
        // Track the drawn color-button hover (world coords, remapped event);
        // MouseLeave clears it.
        match local {
            Event::MouseMove(e) => {
                reset_visible_buttons(cx, Some(e.abs));
                self.set_color_btn_hover(cx, self.hit_color_button(e.abs));
            }
            Event::MouseLeave(_) => {
                reset_visible_buttons(cx, None);
                self.set_color_btn_hover(cx, None);
            }
            _ => {}
        }
    }

    /// Track the hovered color button; redraws only on state change.
    fn set_color_btn_hover(&mut self, cx: &mut Cx, gi: Option<usize>) {
        if self.hover_color_btn != gi {
            self.hover_color_btn = gi;
            self.redraw(cx);
        }
    }

    /// Primary-button press on the canvas: minimap drag, card resize/drag,
    /// group title drag (or frame-gap select+drag), or background marquee
    /// (box select).
    fn handle_finger_down(&mut self, cx: &mut Cx, fe: &FingerDownEvent, child_grabbed: bool) {
        // Any canvas press commits an open group rename (a click inside the
        // rename TextInput is captured and skipped).
        if self.editing_group.is_some() && !child_grabbed {
            self.commit_group_edit(cx);
        }
        // Same for the 序号 editor.
        if self.order_editing.is_some() && !child_grabbed {
            self.commit_order_edit(cx);
        }
        // Color picker popup: a press on a swatch applies the color; any
        // other press closes it. Either way the press is consumed.
        if let Some(gi) = self.color_popup {
            if let Some(i) = (0..GROUP_PRESET_COLORS.len())
                .find(|&i| self.popup_swatch_rect(i).contains(fe.abs))
            {
                if let Some(data) = &mut self.data {
                    data.groups[gi].color = Some(GROUP_PRESET_COLORS[i].to_string());
                    self.save_map();
                }
            }
            self.color_popup = None;
            self.redraw(cx);
            return;
        }
        // Panels (file/refs/float/dock) own their presses; the canvas must
        // not start a marquee/drag under them.
        if !crate::util::over_any_panel(fe.abs) {
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
                        self.selected_groups.clear();
                        self.reanchor_cards(cx);
                    }
                    // Card dragging starts from the header only: the body is
                    // selectable text (划选生成子卡片), so a press there must
                    // not compete with the TextFlow selection drag.
                    let r = self.card_rect(i);
                    let in_header = world.y >= r.pos.y && world.y <= r.pos.y + 44.0;
                    if !child_grabbed && self.editing_card.is_none() && in_header {
                        // no card-internal widget (scrollbar, link) grabbed the press
                        self.drag_card = Some(i);
                        self.drag_last = world;
                    }
                    self.redraw(cx);
                } else if let Some(gi) = self.hit_color_button(world) {
                    if self.editing_card.is_none() {
                        let cards = {
                            let g = &self.data.as_ref().unwrap().groups[gi];
                            g.cards.clone()
                        };
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                        self.reanchor_cards(cx);
                        self.hover_color_btn = None;
                        self.color_popup = Some(gi);
                    }
                    self.redraw(cx);
                } else if let Some(gi) = self.hit_group_title(world) {
                    if self.editing_card.is_none() {
                        let cards = {
                            let g = &self.data.as_ref().unwrap().groups[gi];
                            g.cards.clone()
                        };
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                        self.reanchor_cards(cx);
                        if fe.tap_count >= 2 {
                            self.enter_group_edit(cx, gi);
                        } else if !child_grabbed {
                            self.drag_group = Some(gi);
                            self.drag_last = world;
                        }
                    }
                    self.redraw(cx);
                } else if let Some(gi) = self.hit_group_frame(world) {
                    // Any gap inside the group frame selects the group and
                    // drags it (same as the title bar, minus rename).
                    if self.editing_card.is_none() {
                        let cards = {
                            let g = &self.data.as_ref().unwrap().groups[gi];
                            g.cards.clone()
                        };
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                        self.reanchor_cards(cx);
                        if !child_grabbed {
                            self.drag_group = Some(gi);
                            self.drag_last = world;
                        }
                    }
                    self.redraw(cx);
                } else {
                    self.cancel_zoom_anim(cx);
                    self.marquee = Some(Marquee {
                        start: world,
                        end: world,
                    });
                    self.redraw(cx);
                }
            }
        }
    }

    /// Right-button press: prepare a context menu on a card, or fall back to a
    /// pan if the drag exceeds a small threshold. Click outside panels/minimap
    /// closes any open color popup.
    fn handle_finger_down_secondary(&mut self, cx: &mut Cx, fe: &FingerDownEvent) {
        if self.editing_group.is_some() {
            self.commit_group_edit(cx);
        }
        if self.color_popup.is_some() {
            self.color_popup = None;
            self.redraw(cx);
            return;
        }
        if self.editing_card.is_some()
            || self.minimap_rect.contains(fe.abs)
            || crate::util::over_any_panel(fe.abs)
        {
            return;
        }
        let world = self.screen_to_world(fe.abs);
        let card = self.hit_card(world);
        self.sec_press = Some((fe.abs, card));
        if let Some(i) = card {
            if !self.selected.contains(&i) {
                self.selected = vec![i];
                self.selected_groups.clear();
                self.reanchor_cards(cx);
            }
        }
        self.redraw(cx);
    }

    /// Drag tracking: minimap nav, marquee growth, card resize/drag, pan, or
    /// converting a held right-button press into a pan once it moves enough.
    fn handle_finger_move(&mut self, cx: &mut Cx, fe: &FingerMoveEvent) {
        if let Some((start, _card)) = self.sec_press {
            if (fe.abs - start).length() >= 4.0 {
                self.sec_press = None;
                self.cancel_zoom_anim(cx);
                self.panning = true;
                self.pan_last = fe.abs;
            }
            return;
        }
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
        // ⌘/Ctrl: the dragged edge/corner snaps to the grid (anchor edge stays).
        let w = if self.ctrl_down { Self::snap_grid(world) } else { world };
        if let Some(r) = self.resize_card {
            if let Some(data) = &mut self.data {
                let node = &mut data.nodes[r.card];
                apply_resize(
                    &mut node.pos,
                    &mut node.size,
                    w,
                    r.dir,
                    dvec2(CARD_MIN_SIZE, CARD_MIN_SIZE),
                    dvec2(CARD_MAX_SIZE, CARD_MAX_SIZE),
                );
            }
            self.redraw(cx);
        } else if self.drag_card.is_some() {
            if let Some(data) = &mut self.data {
                let delta = w - self.drag_last;
                for &j in &self.selected {
                    data.nodes[j].pos += delta;
                    if self.ctrl_down {
                        data.nodes[j].pos = Self::snap_grid(data.nodes[j].pos);
                    }
                }
                self.drag_last = w;
            }
            self.redraw(cx);
        } else if let Some(gi) = self.drag_group {
            let delta = w - self.drag_last;
            self.move_group(gi, delta);
            if self.ctrl_down {
                let cards = self.group_subtree_cards(gi);
                if let Some(data) = &mut self.data {
                    for c in cards {
                        data.nodes[c].pos = Self::snap_grid(data.nodes[c].pos);
                    }
                }
            }
            self.drag_last = w;
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
    /// mis-click, clears the selection). A right-button release without drag
    /// opens the card context menu (on a card) or the canvas card picker.
    fn handle_finger_up(&mut self, cx: &mut Cx) {
        if let Some((abs, card)) = self.sec_press.take() {
            if let Some(i) = card {
                self.open_card_menu(cx, abs, i);
            } else {
                self.picker_world = self.screen_to_world(abs);
                cx.widget_action(self.widget_uid(), MindMapAction::CanvasMenu(abs));
            }
            return;
        }
        self.panning = false;
        self.drag_card = None;
        self.drag_group = None;
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
                self.selected_groups.clear();
            } else if let Some(data) = &self.data {
                self.selected = (0..data.nodes.len())
                    .filter(|&i| rect.intersects(self.card_rect(i)))
                    .collect();
                self.selected_groups.clear();
            }
            self.reanchor_cards(cx);
            self.redraw(cx);
        }
    }

    /// Wheel zoom, swallowed over the minimap and any panel (file/refs/float
    /// — their content scrolls instead). Compact cards have no scrollable
    /// body, so wheel over them zooms like canvas.
    fn handle_finger_scroll(&mut self, cx: &mut Cx, fe: &FingerScrollEvent) {
        if !self.minimap_rect.contains(fe.abs)
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
            // 已见/未见 glow, drawn first so a selected card's indigo halo
            // renders over it. Colors are set per draw below (the highlight
            // draw is shared with groups and the mastery glow).
            self.draw_mastery_glow(cx2d, i, r);
            if self.selected.contains(&i) {
                if let Some(hl) = &mut self.highlight {
                    hl.draw_vars.set_uniform(cx2d, id!(color), &[0.49, 0.55, 0.83, 0.45]);
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

    /// 已见/未见 glow around the card edge, same feathered halo as the
    /// selection highlight: grey = 未见 (never tested), red = tested below
    /// PASS_SCORE (判别/联结未过), green = 已见 (score >= PASS_SCORE,
    /// handleable by 经验预测). Directory nodes (no card file) get no glow.
    fn draw_mastery_glow(&mut self, cx2d: &mut Cx2d, i: usize, r: Rect) {
        let Some(data) = &self.data else { return };
        let Some(node) = data.nodes.get(i) else { return };
        // progress.json is keyed by rel path; Node.path is base-joined.
        let base = crate::util::app_base_dir();
        let Some(rel) = node.path.strip_prefix(&base).ok() else { return };
        if !node.path.is_file() {
            return;
        }
        let color: [f32; 4] = match self.progress.get(rel.to_str().unwrap_or("")) {
            Some(s) if *s >= crate::mindmap::model::PASS_SCORE => [0.29, 0.85, 0.5, 0.5],
            Some(_) => [0.97, 0.44, 0.44, 0.5],
            None => [0.42, 0.45, 0.52, 0.32],
        };
        if let Some(hl) = &mut self.highlight {
            hl.draw_vars.set_uniform(cx2d, id!(color), &color);
            hl.draw_abs(
                cx2d,
                Rect {
                    pos: r.pos - dvec2(4.0, 4.0),
                    size: r.size + dvec2(8.0, 8.0),
                },
            );
        }
    }

    /// Group frames: colored translucent border (DrawMarquee shader) + title
    /// bar, drawn under the cards. The title strip lives in the padding above
    /// the member bbox, so it never covers a member card.
    fn draw_groups(&mut self, cx2d: &mut Cx2d, scope: &mut Scope, local_view: Rect) {
        let n = self.data.as_ref().map(|d| d.groups.len()).unwrap_or(0);
        for gi in 0..n {
            let r = self.group_rect(gi);
            if !local_view.intersects(r) {
                continue;
            }
            let color = self.group_color(gi);
            if self.selected_groups.contains(&gi) {
                // same glow treatment as selected cards, tinted to the group
                if let Some(hl) = &mut self.highlight {
                    hl.draw_vars.set_uniform(cx2d, id!(color), &color);
                    hl.draw_abs(
                        cx2d,
                        Rect {
                            pos: r.pos - dvec2(4.0, 4.0),
                            size: r.size + dvec2(8.0, 8.0),
                        },
                    );
                }
            }
            // the frame itself: translucent fill + colored border
            if let Some(d) = self.group_draws.get_mut(gi) {
                d.draw_vars.set_uniform(cx2d, id!(color), &color);
                d.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[0.08]);
                d.draw_vars.set_uniform(cx2d, id!(width), &[4.0]);
                d.draw_abs(cx2d, r);
            }
            let t = self.group_title_rect(gi);
            // colored title bar behind the transparent title widget
            if let Some(d) = self.group_draws.get_mut(gi) {
                d.draw_vars.set_uniform(cx2d, id!(color), &color);
                d.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[1.0]);
                d.draw_vars.set_uniform(cx2d, id!(width), &[1.5]);
                d.draw_abs(cx2d, t);
            }
            let w = self.group_ref(cx2d, gi);
            let editing = self.editing_group == Some(gi);
            w.view(cx2d, ids!(title_box)).set_visible(cx2d, !editing);
            w.view(cx2d, ids!(title_edit_box)).set_visible(cx2d, editing);
            let _ = w.draw_walk(
                cx2d,
                scope,
                Walk {
                    abs_pos: Some(t.pos),
                    width: Size::Fixed(t.size.x),
                    height: Size::Fixed(t.size.y),
                    ..Walk::default()
                },
            );
            // color button: soft hover highlight behind a tinted palette icon
            let btn = self.color_button_rect(gi);
            if self.hover_color_btn == Some(gi) {
                if let Some(d) = self.group_draws.get_mut(gi) {
                    let hr = Rect { pos: btn.pos - dvec2(2.5, 2.5), size: btn.size + dvec2(5.0, 5.0) };
                    d.draw_vars.set_uniform(cx2d, id!(color), &color);
                    d.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[0.28]);
                    d.draw_vars.set_uniform(cx2d, id!(width), &[2.0]);
                    d.draw_abs(cx2d, hr);
                }
            }
            // fixed white icon (SVG strokes are white; the color uniform
            // stays at its -1 default so the SVG's own colors render)
            self.draw_grp_icon.draw_abs(cx2d, btn);
        }
    }

    /// The group's frame color as an RGBA uniform (alpha = shader stroke
    /// alpha); falls back to the script default indigo when unset.
    fn group_color(&self, gi: usize) -> [f32; 4] {
        let Some(data) = &self.data else { return [0.49, 0.55, 0.83, 0.45] };
        data.groups
            .get(gi)
            .and_then(|g| g.color.as_deref())
            .and_then(parse_hex_color)
            .unwrap_or([0.49, 0.55, 0.83, 0.45])
    }

    /// Color picker popup: a panel of preset swatches anchored below the
    /// group's title bar, drawn in screen space (main turtle) so it stays
    /// readable at any zoom. `popup_rect` is cached for hit-testing.
    fn draw_color_popup(&mut self, cx: &mut Cx2d, view: Rect) {
        let Some(gi) = self.color_popup else { return };
        let t = self.group_title_rect(gi);
        let tl = t.pos * self.zoom + self.pan;
        let popup_w = POPUP_PAD * 2.0 + POPUP_COLS * POPUP_SWATCH + (POPUP_COLS - 1.0) * POPUP_GAP;
        let rows = (GROUP_PRESET_COLORS.len() as f64 / POPUP_COLS).ceil();
        let popup_h = POPUP_PAD * 2.0 + rows * POPUP_SWATCH + (rows - 1.0) * POPUP_GAP;
        let mut pos = dvec2(
            tl.x + t.size.x * self.zoom - popup_w,
            tl.y + geometry::GROUP_TITLE_H * self.zoom + 8.0,
        );
        // keep the panel inside the viewport
        pos.x = pos.x.clamp(view.pos.x, view.pos.x + view.size.x - popup_w);
        pos.y = pos.y.clamp(view.pos.y, view.pos.y + view.size.y - popup_h);
        let panel = Rect { pos, size: dvec2(popup_w, popup_h) };
        self.popup_rect = panel;
        // preset swatch rects, precomputed so no borrow of self crosses the
        // popup_draw borrow below
        let swatches: Vec<Rect> = (0..GROUP_PRESET_COLORS.len()).map(|i| self.popup_swatch_rect(i)).collect();
        let Some(d) = &mut self.popup_draw else { return };
        // panel: solid dark background + soft edge
        d.draw_vars.set_uniform(cx, id!(color), &[0.16, 0.18, 0.24, 1.0]);
        d.draw_vars.set_uniform(cx, id!(fill_alpha), &[0.97]);
        d.draw_vars.set_uniform(cx, id!(width), &[4.0]);
        d.draw_abs(cx, panel);
        // preset swatches
        for (i, hex) in GROUP_PRESET_COLORS.iter().enumerate() {
            let c = parse_hex_color(hex).unwrap_or([1.0, 1.0, 1.0, 0.45]);
            d.draw_vars.set_uniform(cx, id!(color), &[c[0], c[1], c[2], 1.0]);
            d.draw_vars.set_uniform(cx, id!(fill_alpha), &[1.0]);
            d.draw_vars.set_uniform(cx, id!(width), &[1.0]);
            d.draw_abs(cx, swatches[i]);
        }
    }

    /// Rect (window coords) of the popup swatch at preset index `i`.
    fn popup_swatch_rect(&self, i: usize) -> Rect {
        Rect {
            pos: self.popup_rect.pos
                + dvec2(
                    POPUP_PAD + (i as f64 % POPUP_COLS) * (POPUP_SWATCH + POPUP_GAP),
                    POPUP_PAD + (i as f64 / POPUP_COLS).floor() * (POPUP_SWATCH + POPUP_GAP),
                ),
            size: dvec2(POPUP_SWATCH, POPUP_SWATCH),
        }
    }

    /// Lazily create the title-bar widget for group `gi` (mirrors card_ref).
    fn group_ref(&mut self, cx: &mut Cx, gi: usize) -> WidgetRef {
        if let Some(Some(w)) = self.group_refs.get(gi) {
            return w.clone();
        }
        let Some(t) = &self.group_template else {
            return WidgetRef::empty();
        };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let group = self.data.as_ref().unwrap().groups[gi].clone();
        w.label(cx, ids!(title)).set_text(cx, &group.title);
        w.text_input(cx, ids!(title_edit)).set_text(cx, &group.title);
        if self.group_refs.len() <= gi {
            self.group_refs.resize(gi + 1, None);
        }
        self.group_refs[gi] = Some(w.clone());
        w
    }

    /// All card indices reachable from group `gi` (its cards + nested
    /// groups' cards).
    fn group_subtree_cards(&self, gi: usize) -> Vec<usize> {
        let Some(data) = &self.data else { return Vec::new() };
        let mut out = Vec::new();
        let mut visited = vec![false; data.groups.len()];
        let mut stack = vec![gi];
        while let Some(g) = stack.pop() {
            if g >= visited.len() || visited[g] {
                continue;
            }
            visited[g] = true;
            let (cards, grps) = { let g = &data.groups[g]; (g.cards.clone(), g.groups.clone()) };
            out.extend(cards);
            stack.extend(grps);
        }
        out
    }

    /// Translate group `gi` and everything nested inside it (forest, so no
    /// card is moved twice).
    fn move_group(&mut self, gi: usize, delta: DVec2) {
        let Some(data) = &mut self.data else { return };
        let mut visited = vec![false; data.groups.len()];
        let mut stack = vec![gi];
        while let Some(g) = stack.pop() {
            if g >= visited.len() || visited[g] {
                continue;
            }
            visited[g] = true;
            let (cards, grps) = { let g = &data.groups[g]; (g.cards.clone(), g.groups.clone()) };
            for &c in &cards {
                if let Some(n) = data.nodes.get_mut(c) {
                    n.pos += delta;
                }
            }
            stack.extend(grps);
        }
    }

    /// Recreate the per-group draw/title state after a structural change
    /// (group count or indices changed). Any open color popup references a
    /// stale group index, so it closes too.
    fn rebuild_group_widgets(&mut self, cx: &mut Cx) {
        let n = self.data.as_ref().map(|d| d.groups.len()).unwrap_or(0);
        self.group_draws = (0..n)
            .map(|_| cx.with_vm(|vm| DrawMarquee::script_new_with_default(vm)))
            .collect();
        self.group_refs = vec![None; n];
        self.color_popup = None;
        self.hover_color_btn = None;
    }

    /// ⌘/Ctrl+G: wrap the selected cards and selected groups in a new group.
    /// Cards that already belong to a group stay there — their group is
    /// nested into the new one instead (fold_selection), so grouping over
    /// existing groups' cards wraps those groups rather than flattening them.
    /// Selected groups are re-parented under the new one. Titles auto-number
    /// as 组 N.
    fn group_selected(&mut self, cx: &mut Cx) {
        let Some(data) = &mut self.data else { return };
        let (cards, grps) = data.fold_selection(&self.selected, &self.selected_groups);
        let valid = cards.len() + grps.len() >= 2 || (cards.is_empty() && !grps.is_empty());
        if !valid {
            return;
        }
        // Selected groups leave their old parents (they nest under the new one).
        for &gi in &grps {
            if let Some(p) = data.group_parent(gi) {
                data.groups[p].groups.retain(|&x| x != gi);
            }
        }
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let max_n = data
            .groups
            .iter()
            .filter_map(|g| {
                let rest = g.title.strip_prefix("组 ")?;
                rest.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0);
        data.groups.push(Group {
            id: format!("g{ms}"),
            title: format!("组 {}", max_n + 1),
            cards: cards.clone(),
            groups: grps,
            color: None,
        });
        data.prune_empty_groups();
        let new_gi = self.data.as_ref().unwrap().groups.len() - 1;
        self.selected_groups = vec![new_gi];
        self.selected = self.group_subtree_cards(new_gi);
        self.rebuild_group_widgets(cx);
        self.reanchor_cards(cx);
        self.save_map();
        self.redraw(cx);
    }

    /// ⌘/Ctrl+Shift+G: dissolve every selected group and every group containing
    /// a selected card; their members (cards + nested groups) splice into the
    /// dissolved group's parent.
    fn ungroup_selected(&mut self, cx: &mut Cx) {
        let Some(data) = &mut self.data else { return };
        let mut doomed: Vec<usize> = self.selected_groups.clone();
        for &c in &self.selected {
            if let Some(gi) = data.group_of_card(c) {
                if !doomed.contains(&gi) {
                    doomed.push(gi);
                }
            }
        }
        if doomed.is_empty() {
            return;
        }
        // Children first (lower indices): dissolving a parent before its
        // children would splice members into a doomed group; children splice
        // up into the still-present parent.
        doomed.sort_unstable_by(|a, b| b.cmp(a));
        for &gi in &doomed {
            let (cards, grps) = { let g = &data.groups[gi]; (g.cards.clone(), g.groups.clone()) };
            if let Some(p) = data.group_parent(gi) {
                for c in cards {
                    if !data.groups[p].cards.contains(&c) {
                        data.groups[p].cards.push(c);
                    }
                }
                for g2 in grps {
                    if !data.groups[p].groups.contains(&g2) {
                        data.groups[p].groups.push(g2);
                    }
                }
            }
            data.groups.remove(gi);
            for g in &mut data.groups {
                for c in &mut g.groups {
                    if *c > gi {
                        *c -= 1;
                    }
                }
            }
        }
        self.selected_groups.clear();
        self.rebuild_group_widgets(cx);
        self.save_map();
        self.redraw(cx);
    }

    /// Double-click on a group title: show the rename input.
    fn enter_group_edit(&mut self, cx: &mut Cx, gi: usize) {
        if self.editing_group == Some(gi) {
            return;
        }
        if self.editing_card.is_some() {
            self.commit_edit(cx);
        }
        let Some(w) = self.group_refs.get(gi).and_then(|c| c.clone()) else {
            return;
        };
        let title = self.data.as_ref().unwrap().groups[gi].title.clone();
        w.text_input(cx, ids!(title_edit)).set_text(cx, &title);
        self.editing_group = Some(gi);
        self.redraw(cx);
    }

    /// Commit the open group rename (Enter or any canvas press).
    fn commit_group_edit(&mut self, cx: &mut Cx) {
        let Some(gi) = self.editing_group.take() else { return };
        let Some(w) = self.group_refs.get(gi).and_then(|c| c.clone()) else {
            return;
        };
        let new_title = w.text_input(cx, ids!(title_edit)).text();
        if let Some(data) = &mut self.data {
            let title = new_title.trim();
            if !title.is_empty() {
                data.groups[gi].title = title.to_string();
                w.label(cx, ids!(title)).set_text(cx, title);
            }
        }
        self.save_map();
        self.redraw(cx);
    }

    /// Open the in-canvas 序号 editor for card `i` (context menu 设置序号).
    fn start_order_edit(&mut self, cx: &mut Cx, i: usize) {
        if self.order_editing.is_some() {
            self.commit_order_edit(cx);
        }
        let Some(t) = &self.order_edit_template else { return };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let order = self
            .data
            .as_ref()
            .and_then(|d| d.nodes.get(i))
            .and_then(|n| n.order);
        w.text_input(cx, ids!(order_edit_input))
            .set_text(cx, &order.map(|n| n.to_string()).unwrap_or_default());
        self.order_edit_ref = Some(w);
        self.order_editing = Some(i);
        self.order_focus_pending = true;
        self.redraw(cx);
    }

    /// Apply the open order edit (Enter or any canvas press): a number sets
    /// the order, empty clears it, invalid text closes without changing.
    fn commit_order_edit(&mut self, cx: &mut Cx) {
        let Some(i) = self.order_editing.take() else { return };
        let Some(w) = self.order_edit_ref.take() else { return };
        self.order_focus_pending = false;
        let text = w.text_input(cx, ids!(order_edit_input)).text();
        let trimmed = text.trim();
        let new_order = if trimmed.is_empty() {
            None
        } else if let Ok(n) = trimmed.parse::<u32>() {
            Some(n)
        } else {
            self.redraw(cx);
            return;
        };
        let changed = self
            .data
            .as_ref()
            .is_some_and(|d| d.nodes[i].order != new_order);
        if changed {
            if let Some(data) = &mut self.data {
                data.nodes[i].order = new_order;
            }
            self.save_map();
            if let Some(Some(card)) = self.cards.get(i).cloned() {
                let title = card_title(&self.data.as_ref().unwrap().nodes[i]);
                set_card_texts(cx, &card, &title, new_order);
            }
        }
        self.redraw(cx);
    }

    fn cancel_order_edit(&mut self, cx: &mut Cx) {
        if self.order_editing.take().is_some() || self.order_edit_ref.take().is_some() {
            self.order_focus_pending = false;
            self.redraw(cx);
        }
    }

    /// The 序号 editor popup, drawn at the card's top-left in world coords.
    /// Focuses its TextInput once the widget has a valid area.
    fn draw_order_edit(&mut self, cx2d: &mut Cx2d, scope: &mut Scope) {
        let Some(i) = self.order_editing else { return };
        let Some(w) = self.order_edit_ref.clone() else { return };
        let r = self.card_rect(i);
        let _ = w.draw_walk(
            cx2d,
            scope,
            Walk {
                abs_pos: Some(r.pos + dvec2(14.0, 5.0)),
                width: Size::Fit { min: None, max: None },
                height: Size::Fit { min: None, max: None },
                ..Walk::default()
            },
        );
        if self.order_focus_pending {
            let input = w.text_input(cx2d, ids!(order_edit_input));
            if input.area().is_valid(cx2d) {
                cx2d.set_key_focus(input.area());
                self.order_focus_pending = false;
            }
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
        // A pending title indicator (set before this widget was created)
        // overrides the file-stem title until explicitly cleared.
        let title = self.pending_titles.get(&node.path).cloned().unwrap_or(name);
        set_card_texts(cx, &w, &title, node.order);
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
            set_card_texts(cx, &card, &name, node.order);
            card.markdown_media(cx, ids!(markdown)).set_text(cx, &body);
        }
        if renamed {
            self.save_map();
        }
        self.redraw(cx);
    }

    /// Update the in-memory body and any live card widget for the node whose
    /// path equals `full_path`. Used after external generation writes the file.
    pub fn update_card_body(&mut self, cx: &mut Cx, full_path: &std::path::Path, body: String) {
        let Some(i) = self.data.as_mut().and_then(|d| d.nodes.iter().position(|n| n.path == full_path)) else {
            return;
        };
        self.data.as_mut().unwrap().nodes[i].body = body.clone();
        if let Some(Some(card)) = self.cards.get(i).cloned() {
            card.markdown_media(cx, ids!(markdown)).set_text(cx, &body);
        }
    }

    /// Set the visible title (and compact title) of the card at `full_path` to
    /// `indicator`, or restore it to the file-stem title when `indicator` is None.
    /// Card widgets are created lazily on draw, so an indicator set before the
    /// widget exists is recorded in `pending_titles` and applied by `card_ref`
    /// at creation; it is consumed only by an explicit None.
    pub fn set_card_title_indicator(&mut self, cx: &mut Cx, full_path: &std::path::Path, indicator: Option<&str>) {
        let Some(i) = self.data.as_mut().and_then(|d| d.nodes.iter().position(|n| n.path == full_path)) else {
            return;
        };
        let node_path = self.data.as_ref().unwrap().nodes[i].path.clone();
        match indicator {
            Some(s) => {
                self.pending_titles.insert(node_path, s.to_string());
            }
            None => {
                self.pending_titles.remove(&node_path);
            }
        }
        let title = indicator
            .map(|s| s.to_string())
            .unwrap_or_else(|| card_title(&self.data.as_ref().unwrap().nodes[i]));
        let order = self.data.as_ref().unwrap().nodes[i].order;
        if let Some(Some(card)) = self.cards.get(i).cloned() {
            set_card_texts(cx, &card, &title, order);
        }
    }

    /// Add the card file at `rel_path` (relative to the app base dir) to the
    /// canvas at the stored right-click world position, detached (no edge).
    /// No-op if the file is already on the map.
    pub fn add_card_at(&mut self, cx: &mut Cx, rel_path: &str) {
        let Some(data) = &mut self.data else { return };
        let path = app_base_dir().join(rel_path);
        if data.nodes.iter().any(|n| n.path == path) {
            return;
        }
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let i = data.add_detached(path, body, self.picker_world);
        self.save_map();
        self.selected = vec![i];
        self.selected_groups.clear();
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Add the card file at `child_rel` as a child of the card at
    /// `parent_rel` (划选生成子卡片): tree edge + a position to the parent's
    /// right, below any existing children. Saves and selects the new card.
    pub fn add_child_card(&mut self, cx: &mut Cx, parent_rel: &str, child_rel: &str) {
        let base = app_base_dir();
        let Some(data) = &mut self.data else { return };
        let Some(pi) = data.nodes.iter().position(|n| n.path == base.join(parent_rel)) else {
            return;
        };
        let path = base.join(child_rel);
        if data.nodes.iter().any(|n| n.path == path) {
            return;
        }
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let child_count = data.nodes[pi].children.len();
        let parent = &data.nodes[pi];
        let pos = parent.pos + dvec2(parent.size.x + 120.0, child_count as f64 * (CARD_H + 40.0));
        let i = data.add_detached(path, body, pos);
        data.nodes[i].parent = Some(pi);
        data.nodes[pi].children.push(i);
        // One new tree edge; card widget slots align lazily via card_ref.
        self.edges.push(cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)));
        self.save_map();
        self.selected = vec![i];
        self.selected_groups.clear();
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Reload 已见/未见 progress from progress.json and refresh the badges.
    pub fn reload_progress(&mut self, cx: &mut Cx) {
        self.progress = crate::mindmap::model::load_progress(&app_base_dir());
        self.redraw(cx);
    }

    /// Drop the card file at `rel_path` from the file panel onto the canvas
    /// at the screen position `abs`. No-op when the pointer is not over the
    /// canvas (a panel covers it), and when the card is already on the map.
    pub fn drop_card_at(&mut self, cx: &mut Cx, rel_path: &str, abs: DVec2) {
        if !self.area.rect(cx).contains(abs) || crate::util::over_any_panel(abs) {
            return;
        }
        // Center the card on the pointer, matching the drag ghost preview
        // (which is also centered on the cursor).
        self.picker_world = self.screen_to_world(abs) - dvec2(CARD_W, CARD_H) * 0.5;
        self.add_card_at(cx, rel_path);
    }

    /// Rel paths of every card currently on the map (for excluding them from
    /// the canvas picker's candidate list).
    pub fn card_rel_paths(&self) -> Vec<String> {
        let Some(data) = &self.data else { return Vec::new() };
        let base = app_base_dir();
        data.nodes
            .iter()
            .filter_map(|n| n.path.strip_prefix(&base).ok().map(|p| p.to_string_lossy().into_owned()))
            .collect()
    }

    fn save_map(&self) {
        let Some(data) = &self.data else {
            return;
        };
        write_map(&app_base_dir(), data, self.pan_target, self.zoom_target, &self.map_file);
    }

    /// Open the card context menu at the right-click screen position.
    fn open_card_menu(&mut self, cx: &mut Cx, abs: DVec2, card: usize) {
        let Some(data) = &self.data else { return };
        let path = data.nodes[card]
            .path
            .strip_prefix(&app_base_dir())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let view = self.area.rect(cx);
        self.menu_card = Some(card);
        self.menu_card_path = path;
        // Snapshot the body selection now: the menu press itself won't touch
        // it (TextFlow only clears on primary press / focus loss), but the
        // snapshot keeps the 生成子卡片 row and its payload consistent.
        self.menu_card_selection = self
            .cards
            .get(card)
            .and_then(|c| c.clone())
            .map(|c| c.markdown_media(cx, ids!(markdown)).selected_text(cx))
            .unwrap_or_default();
        // 生成学习路线 only for the root goal card, and only while it has no
        // children yet (v1 plans once; a planned map gets no re-plan entry).
        // 生成子卡片 only while the card body has a selection. Both rows sit
        // at the end of the menu; item4 = plan row, item5 = subcard row.
        self.menu_plan_row = data.root == Some(card) && data.nodes[card].children.is_empty();
        self.menu_subcard_row = !self.menu_card_selection.trim().is_empty();
        self.menu_items = 4 + usize::from(self.menu_plan_row) + usize::from(self.menu_subcard_row);
        self.menu_rect = menu_rect(view, abs, self.menu_items);
        self.menu_hover = menu_item_index(self.menu_rect, self.menu_items, abs);
        self.sub_open = false;
        self.sub_hover = None;
        self.compute_sub_rect(view);
        self.menu_open = true;
        self.update_menu_hover(cx, abs);
        self.redraw(cx);
    }

    fn close_menu(&mut self, cx: &mut Cx) {
        self.menu_open = false;
        self.sub_open = false;
        self.menu_hover = None;
        self.sub_hover = None;
        self.menu_card = None;
        self.menu_card_path.clear();
        self.menu_card_selection.clear();
        self.sec_press = None;
        self.redraw(cx);
    }

    /// Remove the card from the current map: re-attach children to the parent,
    /// drop the node, rebuild index-dependent state, and save.
    fn remove_card(&mut self, cx: &mut Cx, i: usize) {
        let Some(data) = &mut self.data else { return };
        if data.root == Some(i) || !data.remove_node(i) {
            return;
        }
        self.cards.clear();
        self.edges = (0..data.edges().count())
            .map(|_| cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)))
            .collect();
        self.rebuild_group_widgets(cx);
        self.selected.clear();
        self.selected_groups.clear();
        self.rect_targets.clear();
        self.editing_card = None;
        self.order_editing = None;
        self.order_edit_ref = None;
        self.order_focus_pending = false;
        self.drag_card = None;
        self.drag_group = None;
        self.resize_card = None;
        self.marquee = None;
        self.save_map();
        self.redraw(cx);
    }

    /// Compute the submenu rect anchored to the right of the "生成" row.
    fn compute_sub_rect(&mut self, view: Rect) {
        let sub_w = 180.0;
        let sub_h = MENU_PAD * 2.0 + 8.0 * MENU_ITEM_H;
        let mut x = self.menu_rect.pos.x + self.menu_rect.size.x;
        let mut y = self.menu_rect.pos.y + MENU_PAD + 1.0 * MENU_ITEM_H;
        if x + sub_w > view.pos.x + view.size.x {
            x = (self.menu_rect.pos.x - sub_w).max(view.pos.x);
        }
        if y + sub_h > view.pos.y + view.size.y {
            y = (view.pos.y + view.size.y - sub_h).max(view.pos.y);
        }
        self.sub_rect = Rect {
            pos: dvec2(x, y),
            size: dvec2(sub_w, sub_h),
        };
    }

    fn update_menu_hover(&mut self, cx: &mut Cx, abs: DVec2) {
        let main = menu_item_index(self.menu_rect, self.menu_items, abs);
        let in_sub = self.sub_open && self.sub_rect.contains(abs);
        let sub_hover = if in_sub { menu_item_index(self.sub_rect, 8, abs) } else { None };
        let want_sub = main == Some(1) || in_sub;
        if self.menu_hover != main || self.sub_open != want_sub || self.sub_hover != sub_hover {
            self.menu_hover = main;
            self.sub_open = want_sub;
            self.sub_hover = sub_hover;
            self.redraw(cx);
        }
    }

    fn ctx_menu_widget(&mut self, cx: &mut Cx2d) -> Option<WidgetRef> {
        if self.ctx_menu_ref.is_some() {
            return self.ctx_menu_ref.clone();
        }
        let t = self.ctx_menu_template.as_ref()?;
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        self.ctx_menu_ref = Some(w.clone());
        Some(w)
    }

    fn sub_menu_widget(&mut self, cx: &mut Cx2d) -> Option<WidgetRef> {
        if self.sub_menu_ref.is_some() {
            return self.sub_menu_ref.clone();
        }
        let t = self.sub_menu_template.as_ref()?;
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        self.sub_menu_ref = Some(w.clone());
        Some(w)
    }

    fn drag_ghost_widget(&mut self, cx: &mut Cx2d) -> Option<WidgetRef> {
        if self.drag_ghost_ref.is_some() {
            return self.drag_ghost_ref.clone();
        }
        let t = self.drag_ghost_template.as_ref()?;
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        self.drag_ghost_ref = Some(w.clone());
        Some(w)
    }

    /// Draw the file-panel card drag preview: a translucent card at the
    /// pointer, sized like a real card at the current zoom. Only drawn while
    /// the pointer is over the canvas (panels show no ghost).
    fn draw_drag_ghost(&mut self, cx: &mut Cx2d, scope: &mut Scope, view: Rect) {
        let Some(drag) = crate::util::card_drag() else {
            return;
        };
        // `view` (the turtle's current viewport) not `self.area`: the latter
        // is only refreshed by end_turtle_with_area at the end of draw_walk,
        // so it is stale (or Empty on the very first frame) here.
        if !view.contains(drag.pos) || crate::util::over_any_panel(drag.pos) {
            return;
        }
        let Some(w) = self.drag_ghost_widget(cx) else {
            return;
        };
        let size = dvec2(CARD_W, CARD_H) * self.zoom;
        w.label(cx, ids!(ghost_title)).set_text(cx, &drag.title);
        // draw_walk_all: steps the DrawStep to completion, so the panel bg
        // and title label are actually emitted (a bare draw_walk() without
        // stepping issues no draw calls for child widgets).
        w.draw_walk_all(
            cx,
            scope,
            Walk {
                abs_pos: Some(drag.pos - size * 0.5),
                width: Size::Fixed(size.x),
                height: Size::Fixed(size.y),
                ..Walk::default()
            },
        );
    }

    /// Draw the card context menu and its submenu in screen coords, then
    /// register a window-wide modal area to capture all events while open.
    fn draw_card_menu(&mut self, cx: &mut Cx2d, scope: &mut Scope, _view: Rect) {
        if !self.menu_open {
            return;
        }
        if let Some(w) = self.ctx_menu_widget(cx) {
            // Conditional rows at the end of the menu: 生成学习路线 on the
            // root goal card, 生成子卡片 while the body has a selection.
            w.view(cx, ids!(item4)).set_visible(cx, self.menu_plan_row);
            w.view(cx, ids!(item5)).set_visible(cx, self.menu_subcard_row);
            let _ = w.draw_walk(
                cx,
                scope,
                Walk {
                    abs_pos: Some(self.menu_rect.pos),
                    width: Size::Fixed(self.menu_rect.size.x),
                    height: Size::Fixed(self.menu_rect.size.y),
                    ..Walk::default()
                },
            );
            if let Some(hover) = self.menu_hover {
                self.draw_menu_hl.draw_abs(
                    cx,
                    Rect {
                        pos: self.menu_rect.pos + dvec2(MENU_PAD, MENU_PAD + hover as f64 * MENU_ITEM_H),
                        size: dvec2(self.menu_rect.size.x - 2.0 * MENU_PAD, MENU_ITEM_H),
                    },
                );
            }
        }
        if self.sub_open {
            if let Some(w) = self.sub_menu_widget(cx) {
                let _ = w.draw_walk(
                    cx,
                    scope,
                    Walk {
                        abs_pos: Some(self.sub_rect.pos),
                        width: Size::Fixed(self.sub_rect.size.x),
                        height: Size::Fixed(self.sub_rect.size.y),
                        ..Walk::default()
                    },
                );
                if let Some(hover) = self.sub_hover {
                    self.draw_menu_hl.draw_abs(
                        cx,
                        Rect {
                            pos: self.sub_rect.pos + dvec2(MENU_PAD, MENU_PAD + hover as f64 * MENU_ITEM_H),
                            size: dvec2(self.sub_rect.size.x - 2.0 * MENU_PAD, MENU_ITEM_H),
                        },
                    );
                }
            }
        }
        let window = Rect {
            pos: DVec2::default(),
            size: cx.current_pass_size(),
        };
        cx.add_aligned_rect_area(&mut self.menu_modal_area, window);
    }

    /// Handle Esc / hover / menu item clicks while the card context menu is open.
    fn handle_card_menu_events(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        match event {
            Event::KeyDown(ke) if ke.key_code == KeyCode::Escape => {
                self.close_menu(cx);
                return;
            }
            Event::MouseMove(e) => {
                self.update_menu_hover(cx, e.abs);
                return;
            }
            _ => {}
        }
        match event.hits_with_capture_overload(cx, self.menu_modal_area, true) {
            Hit::FingerDown(fe) => self.on_menu_click(cx, fe.abs),
            _ => {}
        }
    }

    fn on_menu_click(&mut self, cx: &mut Cx, abs: DVec2) {
        if let Some(idx) = menu_item_index(self.menu_rect, self.menu_items, abs) {
            if idx == 0 {
                if let Some(i) = self.menu_card {
                    let root = self.data.as_ref().and_then(|d| d.root);
                    if root != Some(i) {
                        self.remove_card(cx, i);
                    }
                }
                self.close_menu(cx);
                return;
            }
            if idx == 1 {
                if !self.sub_open {
                    self.sub_open = true;
                    self.redraw(cx);
                }
                return;
            }
            if idx == 2 {
                if !self.menu_card_path.is_empty() {
                    cx.widget_action(self.widget_uid(), MindMapAction::Quiz(self.menu_card_path.clone()));
                }
                self.close_menu(cx);
                return;
            }
            if idx == 3 {
                // 设置序号: open the in-canvas order editor for the card.
                if let Some(i) = self.menu_card {
                    self.close_menu(cx);
                    self.start_order_edit(cx, i);
                }
                return;
            }
            // Rows 4+: 生成学习路线 (plan row) then 生成子卡片 (subcard row),
            // each only present when its flag is set.
            if idx >= 4 {
                let mut row = 4usize;
                if self.menu_plan_row {
                    if idx == row {
                        if !self.menu_card_path.is_empty() {
                            cx.widget_action(
                                self.widget_uid(),
                                MindMapAction::PlanRoute(self.menu_card_path.clone()),
                            );
                        }
                        self.close_menu(cx);
                        return;
                    }
                    row += 1;
                }
                if self.menu_subcard_row && idx == row {
                    if !self.menu_card_path.is_empty() {
                        cx.widget_action(
                            self.widget_uid(),
                            MindMapAction::GenSubCard(
                                self.menu_card_path.clone(),
                                self.menu_card_selection.clone(),
                            ),
                        );
                    }
                    self.close_menu(cx);
                    return;
                }
            }
        }
        if self.sub_open {
            if let Some(idx) = menu_item_index(self.sub_rect, 8, abs) {
                let section = match idx {
                    0 => GenSection::All,
                    1 => GenSection::Desc,
                    2 => GenSection::Plain,
                    3 => GenSection::PosExample,
                    4 => GenSection::NegExample,
                    5 => GenSection::Purpose,
                    6 => GenSection::Affect,
                    7 => GenSection::Affected,
                    _ => GenSection::All,
                };
                if !self.menu_card_path.is_empty() {
                    cx.widget_action(
                        self.widget_uid(),
                        MindMapAction::Generate(self.menu_card_path.clone(), section),
                    );
                }
                self.close_menu(cx);
                return;
            }
        }
        // Click outside the menu or submenu closes it.
        self.close_menu(cx);
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
fn set_card_texts(cx: &mut Cx, card: &WidgetRef, title: &str, order: Option<u32>) {
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
}

