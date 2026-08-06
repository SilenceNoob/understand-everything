use makepad_widgets::*;

use serde::{Deserialize, Serialize};

use crate::slide_panel::{menu_item_index, menu_rect, MENU_ITEM_H, MENU_PAD, SlideState};
use crate::util::{app_base_dir, cached_widget, set_panel_rect};

const TAB_W: f64 = 14.0;
const TAB_H: f64 = 48.0;
/// Default panel width and drag limits (px), same as the file panel.
const PANEL_W_DEFAULT: f64 = 520.0;
const PANEL_W_MIN: f64 = 140.0;
const PANEL_W_MAX: f64 = 520.0;
/// Width-grab strip on the panel's left edge: 8px inside the panel,
/// 4px straddling the edge (total 12px).
const EDGE_W: f64 = 12.0;
const EDGE_INSET: f64 = 8.0;
/// Fixed pane header height (px) — the DSL header is exactly this tall.
const PANE_HEADER_H: f64 = 32.0;
/// Bottom button bar height (px).
const BAR_H: f64 = 60.0;
/// Minimum row height (px) as a fallback when a row's laid-out height reads
/// as zero; rows are otherwise sized by their real rendered content.
const ROW_MIN: f64 = 40.0;
/// Excerpt length cap (chars) for file snippets.
const EXCERPT_MAX: usize = 200;
/// Shown (in red) as the excerpt of a document that failed to convert.
const FAILED_TEXT: &str = "文件解析失败";

/// One reference item: an absolute path to a local Markdown document, plus
/// its excerpt. `failed` marks a document whose conversion to Markdown
/// failed; its excerpt shows FAILED_TEXT in red.
#[derive(Clone, PartialEq, Debug)]
struct RefItem {
    value: String,
    desc: String,
    failed: bool,
}

impl RefItem {
    /// Display title: the file name.
    fn name(&self) -> String {
        std::path::Path::new(&self.value)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.value.clone())
    }
}

/// Per-map reference list, persisted as refs/<map rel>.json. File excerpts
/// are re-derived from disk on load (the file is the source).
#[derive(Serialize, Deserialize)]
struct RefsFile {
    files: Vec<FileRef>,
}

#[derive(Serialize, Deserialize)]
struct FileRef {
    path: String,
    desc: String,
    #[serde(default)]
    failed: bool,
}

/// Refs file for a map rel path ("maps/foo.json" -> "refs/foo.json", mirroring
/// subdirectories so maps in different dirs never collide).
fn refs_path(map_rel: &str) -> std::path::PathBuf {
    let rel = map_rel.strip_prefix("maps/").unwrap_or(map_rel);
    app_base_dir().join("refs").join(rel)
}

fn load_items(map_rel: &str) -> Vec<RefItem> {
    let Ok(json) = std::fs::read_to_string(refs_path(map_rel)) else {
        return Vec::new();
    };
    let Ok(data) = serde_json::from_str::<RefsFile>(&json) else {
        return Vec::new();
    };
    data.files
        .into_iter()
        .map(|f| RefItem {
            desc: if f.failed {
                FAILED_TEXT.to_string()
            } else {
                // fresh excerpt from disk beats the stored one
                file_excerpt(&f.path).unwrap_or(f.desc)
            },
            value: f.path,
            failed: f.failed,
        })
        .collect()
}

/// Document paths a map's refs list points at (failed conversions have no
/// retrievable content and are skipped).
pub(crate) fn ref_doc_paths(map_rel: &str) -> Vec<std::path::PathBuf> {
    load_items(map_rel)
        .into_iter()
        .filter(|i| !i.failed)
        .map(|i| std::path::PathBuf::from(i.value))
        .collect()
}

fn save_items(map_rel: &str, items: &[RefItem]) {
    let files = items
        .iter()
        .map(|it| FileRef {
            path: it.value.clone(),
            desc: String::new(),
            failed: it.failed,
        })
        .collect();
    let path = refs_path(map_rel);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(&RefsFile { files }) {
        let _ = std::fs::write(path, json);
    }
}

/// First snippet of a markdown file: strip heading/bullet markers and
/// collapse whitespace, cut at a word boundary.
fn file_excerpt(path: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(excerpt(&text, EXCERPT_MAX))
}

/// Target path for a converted document in the app's `docs/` dir: same stem
/// as the source, `.md` extension, unique-ified with a numeric suffix when
/// the name is taken (never overwrites). `docs/` is app-private: neither the
/// card pane (scans `cards/`) nor the map list (scans `maps/`) ever reads it.
fn converted_path(source: &std::path::Path) -> std::path::PathBuf {
    let dir = app_base_dir().join("docs");
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "doc".to_string());
    for n in 0.. {
        let name = if n == 0 {
            format!("{stem}.md")
        } else {
            format!("{stem}-{n}.md")
        };
        let p = dir.join(&name);
        if !p.exists() {
            return p;
        }
    }
    unreachable!()
}

/// First meaningful snippet of `text`: drop markdown line markers, collapse
/// whitespace, cut at a word boundary (never splitting CJK).
fn excerpt(text: &str, max: usize) -> String {
    let clean: String = text
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '*', '-', '>', '`']))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let collapsed: String = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max).collect();
    match cut.rfind(' ') {
        Some(i) if i > max / 2 => format!("{}…", &cut[..i]),
        _ => format!("{cut}…"),
    }
}

/// The row index under `abs` in a list rect, using the real per-row heights
/// cached at draw time (rows have variable heights).
fn row_index_at(heights: &[f64], list: Rect, abs: DVec2, scroll: f64) -> Option<usize> {
    if heights.is_empty() {
        return None;
    }
    let y = abs.y - list.pos.y + scroll;
    if y < 0.0 {
        return None;
    }
    let mut acc = 0.0;
    for (i, h) in heights.iter().enumerate() {
        acc += h;
        if y < acc {
            return Some(i);
        }
    }
    None
}

/// Clamp a list scroll offset by `dy` px; returns true when it moved. The
/// content height is the sum of the real per-row heights.
fn scroll_rows(heights: &[f64], list: Rect, dy: f64, scroll: &mut f64) -> bool {
    if heights.is_empty() || list.size.y <= 0.0 {
        return false;
    }
    let max = (heights.iter().sum::<f64>() - list.size.y).max(0.0);
    let old = *scroll;
    *scroll = (*scroll + dy).clamp(0.0, max);
    *scroll != old
}

/// Lazily clone a row from the template, set its title/description and load
/// its doc icon.
fn row_ref(
    cx: &mut Cx,
    template: &ScriptObjectRef,
    i: usize,
    item: &RefItem,
    refs: &mut Vec<WidgetRef>,
) -> WidgetRef {
    if let Some(r) = refs.get(i) {
        return r.clone();
    }
    let value = template.as_object().into();
    let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
    w.label(cx, ids!(row_name)).set_text(cx, &item.name());
    let desc = w.label(cx, ids!(row_desc));
    desc.set_text(cx, &item.desc);
    if item.failed {
        desc.set_text_color(cx, Vec4f::from_u32(0xfca5a5ff));
    }
    let icon_path = app_base_dir().join("resources").join("card.svg");
    if let Ok(bytes) = std::fs::read(&icon_path) {
        let _ = w
            .image(cx, ids!(row_icon))
            .load_svg_from_shared_data(cx, bytes.into());
    }
    refs.push(w.clone());
    w
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // One card row in the reference list: type icon + title + excerpt (or
    // the URL input while a link is being added). RefsPanel clones it per
    // row. All heights are Fit: the row sizes itself to the real rendered
    // text (line heights vary with fonts/DPI and cannot be predicted in
    // advance), so the excerpt can never be vertically clipped.
    let RefRow = mod.widgets.View{
        width: Fill
        height: Fit
        flow: Right
        spacing: 10
        align: Align{y: 0.5}
        padding: Inset{left: 12, right: 10, top: 6, bottom: 6}
        row_icon := mod.widgets.Image{
            width: (18.0)
            height: (18.0)
        }
        row_text_box := mod.widgets.View{
            width: Fill
            height: Fit
            flow: Down
            row_name := mod.widgets.Label{
                max_lines: 1
                text_overflow: TextOverflow.Ellipsis
                width: Fill
                height: Fit
                text: ""
                draw_text.text_style.font_size: 13.0
                draw_text.color: #e6e9f0
            }
            row_desc := mod.widgets.Label{
                max_lines: 3
                text_overflow: TextOverflow.Ellipsis
                width: Fill
                // Fit: the label lays out at its true line height and caps at
                // 3 rows with "…" — never clipped mid-line.
                height: Fit
                text: ""
                draw_text.text_style.font_size: 11.0
                draw_text.color: #7a8192
            }
        }
        }

    mod.widgets.RefsPanelBase = #(RefsPanel::register_widget(vm))

    mod.widgets.RefsPanel = set_type_default() do mod.widgets.RefsPanelBase{
        width: Fit
        height: Fit
        clip_x: false
        clip_y: false

        // Row prototype; RefsPanel clones it per row (never drawn here).
        ref_row := RefRow{}

        // Chrome only: rounded bg + border behind the list.
        content := mod.widgets.RoundedView{
            width: Fill
            height: Fill
            flow: Down
            show_bg: true
            draw_bg +: {
                color: #1f2430f2
                border_radius: 8.0
                border_size: 1.0
                border_color: #ffffff14
            }
        }
        // Highlight for the context-menu item under the cursor.
        draw_menu_hl +: {
            color: #ffffff1a
        }
        header := mod.widgets.View{
            width: Fill
            height: (32.0)
            flow: Down
            align: Align{y: 0.5}
            header_label := mod.widgets.Label{
                width: Fill
                height: Fit
                padding: Inset{left: 12, right: 12}
                text: "参考资料"
                draw_text.text_style.font_size: 14.0
                draw_text.color: #e6e9f0
            }
        }
        // Bottom bar: add a document.
        bottom_bar := mod.widgets.View{
            width: Fill
            height: (60.0)
            flow: Right
            spacing: 8
            padding: Inset{left: 8, right: 8, top: 10, bottom: 10}
            add_doc_btn := mod.widgets.ButtonFlat{
                width: Fill
                height: Fill
                text: "添加文档"
                draw_text.text_style.font_size: 12.0
            }
        }
        // Tab on the panel's left edge (mirror of the file panel's tab).
        tab := mod.widgets.ButtonFlat{
            text: "◀"
            draw_text.text_style.font_size: 8.0
            padding: Inset{left: 0, right: 0}
            draw_bg +: {
                color: #1f2430f2
                color_hover: #232834f2
                color_down: #232834f2
                border_size: uniform(1.0)
                border_color: #ffffff14
            }
        }
        // Right-click context menu; RefsPanel positions and draws it manually.
        ctx_menu := mod.widgets.RoundedView{
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
            menu_delete_box := mod.widgets.View{
                width: Fill
                height: (32.0)
                flow: Down
                align: Align{y: 0.5}
                menu_delete := mod.widgets.Label{
                    max_lines: 1
                    text_overflow: TextOverflow.Ellipsis
                    width: Fill
                    height: Fit
                    padding: Inset{left: 10}
                    text: "删除"
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #e6e9f0
                }
            }
        }
    }
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct RefsPanel {
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
    tab_ref: Option<WidgetRef>,
    #[rust]
    ctx_menu_ref: Option<WidgetRef>,

    /// Slide-in/out animation state (shared with the file panel).
    #[rust]
    slide: SlideState,
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
    /// Panel width in px, adjustable by dragging the left edge.
    #[rust(PANEL_W_DEFAULT)]
    panel_w: f64,
    #[rust]
    panel_w_dragging: bool,
    #[rust]
    edge_rect: Rect,
    #[rust]
    edge_area: Area,

    #[live]
    draw_menu_hl: DrawColor,
    /// The currently open map (rel path, e.g. "maps/foo.json"); its
    /// reference list is loaded from refs/<rel>.json. Set by App::open_map.
    #[rust]
    current_map: Option<String>,
    /// Reference items of the current map, in display order.
    #[rust]
    items: Vec<RefItem>,
    /// Lazily-created row widgets, index-aligned with `items` (cleared on
    /// change so texts refresh).
    #[rust]
    row_refs: Vec<WidgetRef>,
    /// Real laid-out height of each row, rebuilt every draw (rows are
    /// Fit-sized, so heights follow the rendered text).
    #[rust]
    row_heights: Vec<f64>,
    #[rust]
    row_template: Option<ScriptObjectRef>,
    #[rust]
    scroll: f64,
    /// List viewport in window coords + hit area, cached at draw time.
    #[rust]
    list_rect: Rect,
    #[rust]
    list_area: Area,

    /// Right-click context menu state.
    #[rust]
    menu_open: bool,
    /// Row the menu was opened on.
    #[rust]
    menu_row: Option<usize>,
    #[rust]
    menu_rect: Rect,
    #[rust]
    menu_hover: Option<usize>,
    /// Window-wide area that captures all presses while the menu is open.
    #[rust]
    modal_area: Area,
}

impl ScriptHook for RefsPanel {
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
                            if id == live_id!(ref_row) {
                                self.row_template = Some(vm.bx.heap.new_object_ref(template_obj));
                            }
                        }
                    }
                }
            });
        }
    }
}

impl WidgetNode for RefsPanel {
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

impl Widget for RefsPanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        let panel = self.compute_rects(cx);

        // Default to the default map until App::open_map sets one (mirrors the
        // mindmap, which loads maps/map.json on its own).
        if self.current_map.is_none() {
            self.current_map = Some(crate::mindmap::MindMapData::DEFAULT_MAP.to_string());
            self.items = load_items(crate::mindmap::MindMapData::DEFAULT_MAP);
        }

        cx.begin_turtle(self.walk, self.layout);
        if let Some(content) = self.content_widget(cx) {
            self.draw_chrome(cx, scope, content, panel);
        }
        self.draw_tab(cx, scope);
        self.draw_menu(cx, scope);
        // While the menu is open, a window-wide modal area captures every
        // press.
        if self.menu_open {
            cx.add_aligned_rect_area(
                &mut self.modal_area,
                Rect {
                    pos: DVec2::default(),
                    size: self.window_size,
                },
            );
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.handle_slide_anim(cx, event);
        if let Some(content) = self.content_widget(cx) {
            content.handle_event(cx, event, scope);
        }
        let bar = self.view.widget(cx, ids!(bottom_bar));
        bar.handle_event(cx, event, scope);
        if let Some(tab) = self.tab_widget(cx) {
            tab.handle_event(cx, event, scope);
        }
        self.handle_modal_events(cx, event);
        // Button actions fire from the bottom bar's own event handling.
        if let Event::Actions(actions) = event {
            let bar = self.view.widget(cx, ids!(bottom_bar));
            if bar.button(cx, ids!(add_doc_btn)).clicked(actions) {
                self.pick_doc(cx);
            }
        }
        // capture_overload: widgets above us (float panels) may mark the
        // press handled first; plain hits would then skip our areas.
        match event.hits_with_capture_overload(cx, self.tab_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.toggle(cx);
            }
            _ => {}
        }
        self.handle_edge_drag(cx, event);
        self.handle_mouse_move(cx, event);
        self.handle_list_events(cx, event);
        // Right-click anywhere on the panel opens (or repositions) the menu.
        match event.hits_with_capture_overload(cx, self.panel_area, true) {
            Hit::FingerDown(fe)
                if matches!(fe.device, DigitDevice::Mouse { button } if button.is_secondary())
                    && !self.menu_open =>
            {
                self.open_menu(cx, fe.abs);
            }
            _ => {}
        }
        // Claim the press over the panel body so it never reaches the canvas.
        let _ = event.hits_with_capture_overload(cx, self.panel_area, true);
    }
}

/// Stage `src` into the app's `docs/` dir and return the path a ref item
/// should record, plus whether parsing failed. Markdown files are copied
/// as-is; other formats are converted with anydoc (failure keeps the source
/// path so the row still shows which file was rejected). Sources already
/// inside `docs/` are referenced in place.
fn stage_document(src: &std::path::Path) -> (String, bool) {
    if src.starts_with(app_base_dir().join("docs")) {
        return (src.to_string_lossy().into_owned(), false);
    }
    let is_md = src.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown")
    });
    let out = converted_path(src);
    if is_md {
        return match std::fs::copy(src, &out) {
            Ok(_) => (out.to_string_lossy().into_owned(), false),
            Err(e) => {
                eprintln!("failed to copy {} to {}: {}", src.display(), out.display(), e);
                (src.to_string_lossy().into_owned(), false)
            }
        };
    }
    match anydoc::to_markdown(src) {
        Ok(md) => {
            let ok = std::fs::create_dir_all(out.parent().unwrap()).is_ok()
                && std::fs::write(&out, md).is_ok();
            if ok {
                (out.to_string_lossy().into_owned(), false)
            } else {
                eprintln!("failed to write converted markdown {}", out.display());
                (src.to_string_lossy().into_owned(), true)
            }
        }
        Err(e) => {
            eprintln!("anydoc conversion failed for {}: {}", src.display(), e);
            (src.to_string_lossy().into_owned(), true)
        }
    }
}

impl RefsPanel {
    /// Panel/tab/edge rects from the current slide progress, in window
    /// coords; registers the panel rect for the mindmap's pointer guards.
    fn compute_rects(&mut self, cx: &Cx2d) -> Rect {
        self.window_size = cx.current_pass_size();
        let body_y = cx.turtle().rect().pos.y; // body top, window coords
        let body_h = (self.window_size.y - body_y).max(0.0);
        // 85% of the body height, centered vertically.
        let panel_h = body_h * crate::util::SIDE_PANEL_H_FRAC;
        let y_off = (body_h - panel_h) * 0.5;
        let panel = Rect {
            pos: dvec2(self.window_size.x - self.panel_w * self.slide.progress, body_y + y_off),
            size: dvec2(self.panel_w, panel_h),
        };
        self.panel_rect = panel;
        self.tab_rect = Rect {
            pos: dvec2(panel.pos.x - TAB_W, panel.pos.y + panel.size.y * 0.5 - TAB_H * 0.5),
            size: dvec2(TAB_W, TAB_H),
        };
        self.edge_rect = Rect {
            pos: dvec2(panel.pos.x - (EDGE_W - EDGE_INSET), panel.pos.y),
            size: dvec2(EDGE_W, panel.size.y),
        };
        set_panel_rect(self.uid.0, Some(panel));
        panel
    }

    /// Draw the panel content chrome: content view, header and bottom bar
    /// (clipped to the panel rect — same trick as FilePanel/FloatPanel: the
    /// root turtle's clip is disabled, so push a real clip so draw_clip data
    /// and hit-testing resolve to the panel rect).
    fn draw_chrome(&mut self, cx: &mut Cx2d, scope: &mut Scope, content: WidgetRef, panel: Rect) {
        cx.push_clip_rect(panel);
        let chrome = Walk {
            abs_pos: Some(panel.pos),
            width: Size::Fixed(panel.size.x),
            height: Size::Fixed(panel.size.y),
            ..Walk::default()
        };
        let _ = content.draw_walk(cx, scope, chrome);
        let header = self.view.widget(cx, ids!(header));
        let _ = header.draw_walk(
            cx,
            scope,
            Walk {
                abs_pos: Some(panel.pos),
                width: Size::Fixed(panel.size.x),
                height: Size::Fixed(PANE_HEADER_H),
                ..Walk::default()
            },
        );
        let bar = self.view.widget(cx, ids!(bottom_bar));
        let _ = bar.draw_walk(
            cx,
            scope,
            Walk {
                abs_pos: Some(panel.pos + dvec2(0.0, panel.size.y - BAR_H)),
                width: Size::Fixed(panel.size.x),
                height: Size::Fixed(BAR_H),
                ..Walk::default()
            },
        );
        // Row list between the header and the bottom bar.
        let list = Rect {
            pos: panel.pos + dvec2(0.0, PANE_HEADER_H),
            size: dvec2(panel.size.x, (panel.size.y - PANE_HEADER_H - BAR_H).max(0.0)),
        };
        self.list_rect = list;
        if let Some(t) = &self.row_template {
            let t = t.clone();
            self.draw_rows(cx, scope, &t);
        }
        cx.add_aligned_rect_area(&mut self.panel_area, panel);
        cx.add_aligned_rect_area(&mut self.edge_area, self.edge_rect);
        cx.pop_clip_rect();
    }

    /// The slide-in tab button.
    fn draw_tab(&mut self, cx: &mut Cx2d, scope: &mut Scope) {
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
    }

    /// The open context menu on top of everything; hover highlight under
    /// the cursor.
    fn draw_menu(&mut self, cx: &mut Cx2d, scope: &mut Scope) {
        if self.menu_open {
            if let Some(menu) = self.ctx_menu_widget(cx) {
                cx.push_clip_rect(self.menu_rect);
                let walk = Walk {
                    abs_pos: Some(self.menu_rect.pos),
                    width: Size::Fixed(self.menu_rect.size.x),
                    height: Size::Fixed(self.menu_rect.size.y),
                    ..Walk::default()
                };
                let _ = menu.draw_walk(cx, scope, walk);
                if let Some(i) = self.menu_hover {
                    self.draw_menu_hl.draw_abs(
                        cx,
                        Rect {
                            pos: self.menu_rect.pos
                                + dvec2(MENU_PAD, MENU_PAD + i as f64 * MENU_ITEM_H),
                            size: dvec2(self.menu_rect.size.x - 2.0 * MENU_PAD, MENU_ITEM_H),
                        },
                    );
                }
                cx.pop_clip_rect();
            }
        }
    }

    /// Modal state (context menu) grabs every press first, so the
    /// list/tab/edge below can't fire behind it.
    fn handle_modal_events(&mut self, cx: &mut Cx, event: &Event) {
        if self.menu_open {
            match event.hits_with_capture_overload(cx, self.modal_area, true) {
                Hit::FingerDown(fe) if fe.is_primary_hit() => {
                    if self.menu_rect.contains(fe.abs) {
                        self.on_menu_press(cx, fe.abs);
                    } else {
                        self.menu_open = false;
                        self.redraw(cx);
                    }
                }
                _ => {}
            }
        }
    }

    /// Left-edge drag to resize the panel width.
    fn handle_edge_drag(&mut self, cx: &mut Cx, event: &Event) {
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
    }

    /// Edge hover cursor and the context-menu highlight (tracked from the
    /// raw cursor, redraw only on change).
    fn handle_mouse_move(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::MouseMove(e) = event {
            if !self.panel_w_dragging && self.edge_rect.contains(e.abs) {
                cx.set_cursor(MouseCursor::ColResize);
            }
            if self.menu_open {
                let hover = menu_item_index(self.menu_rect, 1, e.abs);
                if hover != self.menu_hover {
                    self.menu_hover = hover;
                    self.redraw(cx);
                }
            }
        }
    }

    /// The row list: press marks the row for the context menu, wheel
    /// scrolls (Scroll bypasses the handled flag).
    fn handle_list_events(&mut self, cx: &mut Cx, event: &Event) {
        match event.hits_with_capture_overload(cx, self.list_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && !self.menu_open => {
                if let Some(i) = row_index_at(&self.row_heights, self.list_rect, fe.abs, self.scroll)
                {
                    self.menu_row = Some(i);
                }
            }
            Hit::FingerScroll(fe) => {
                if scroll_rows(&self.row_heights, self.list_rect, fe.scroll.y, &mut self.scroll) {
                    self.redraw(cx);
                }
            }
            _ => {}
        }
    }

    fn content_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.content_ref, || self.view.widget(cx, ids!(content)))
    }

    fn tab_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.tab_ref, || self.view.widget(cx, ids!(tab)))
    }

    fn ctx_menu_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.ctx_menu_ref, || self.view.widget(cx, ids!(ctx_menu)))
    }

    /// Draw the reference rows (clipped to the list rect), lazily creating
    /// row widgets and registering the list hit area. Rows are Fit-sized:
    /// each row's real laid-out height is read back and cached in
    /// `row_heights`, so hit-testing and scrolling follow the rendered text
    /// (and the excerpt can never be clipped mid-line).
    fn draw_rows(&mut self, cx: &mut Cx2d, scope: &mut Scope, template: &ScriptObjectRef) {
        let list = self.list_rect;
        if self.items.is_empty() || list.size.y <= 0.0 {
            self.row_heights.clear();
            return;
        }
        // Clamp scroll to the previous content height (rows may have changed).
        let max_scroll = (self.row_heights.iter().sum::<f64>() - list.size.y).max(0.0);
        self.scroll = self.scroll.min(max_scroll);
        cx.push_clip_rect(list);
        self.row_heights.clear();
        let mut y = -self.scroll;
        for (i, item) in self.items.iter().enumerate() {
            let w = row_ref(cx, template, i, item, &mut self.row_refs);
            let _ = w.draw_walk(
                cx,
                scope,
                Walk {
                    abs_pos: Some(dvec2(list.pos.x, list.pos.y + y)),
                    width: Size::Fixed(list.size.x),
                    height: Size::fit(),
                    ..Walk::default()
                },
            );
            let h = w.area().rect(cx).size.y.max(ROW_MIN);
            self.row_heights.push(h);
            y += h;
        }
        cx.add_aligned_rect_area(&mut self.list_area, list);
        cx.pop_clip_rect();
    }

    /// Ease the slide animation on its timer tick.
    fn handle_slide_anim(&mut self, cx: &mut Cx, event: &Event) {
        if self.slide.handle_event(cx, event) {
            self.redraw(cx);
        }
    }

    fn toggle(&mut self, cx: &mut Cx) {
        self.slide.toggle(cx);
        self.menu_open = false;
        if let Some(tab) = self.tab_widget(cx) {
            tab.set_text(cx, if self.slide.opened { "▶" } else { "◀" });
        }
        self.redraw(cx);
    }

    /// Panel width from the cursor x (the left edge follows the cursor),
    /// clamped to [min, max].
    fn apply_width(&mut self, cx: &mut Cx, abs_x: f64) {
        let w = (self.window_size.x - abs_x).clamp(PANEL_W_MIN, PANEL_W_MAX);
        if (w - self.panel_w).abs() > f64::EPSILON {
            self.panel_w = w;
            self.redraw(cx);
        }
    }

    /// Native file dialog for the 添加文档 button; appends the picked file
    /// to the current map's list. Markdown files are copied into the app's
    /// `docs/` dir, other formats are converted to Markdown via anydoc; a
    /// document that failed to parse is still listed, with FAILED_TEXT in
    /// red as its excerpt.
    fn pick_doc(&mut self, cx: &mut Cx) {
        let path = rfd::FileDialog::new()
            .set_directory(app_base_dir())
            .add_filter("Markdown", &["md", "markdown"])
            .add_filter(
                "文档",
                &[
                    "doc", "docx", "docm", "ppt", "pptx", "xls", "xlsx", "odt", "odp", "ods",
                    "rtf", "epub", "pdf", "csv",
                ],
            )
            .add_filter("所有文件", &["*"])
            .pick_file();
        let Some(path) = path else {
            return;
        };
        let (value, failed) = stage_document(&path);
        let mut item = RefItem {
            value,
            desc: String::new(),
            failed,
        };
        item.desc = if failed {
            FAILED_TEXT.to_string()
        } else {
            file_excerpt(&item.value).unwrap_or_default()
        };
        self.items.push(item);
        self.row_refs.clear();
        self.save();
        self.redraw(cx);
    }

    /// Open the context menu at `abs`, clamped inside the panel.
    fn open_menu(&mut self, cx: &mut Cx, abs: DVec2) {
        self.menu_row = row_index_at(&self.row_heights, self.list_rect, abs, self.scroll);
        self.menu_rect = menu_rect(self.panel_rect, abs, 1);
        self.menu_hover = menu_item_index(self.menu_rect, 1, abs);
        self.menu_open = true;
        self.redraw(cx);
    }

    fn on_menu_press(&mut self, cx: &mut Cx, abs: DVec2) {
        let idx = menu_item_index(self.menu_rect, 1, abs);
        self.menu_open = false;
        self.menu_hover = None;
        if idx == Some(0) {
            if let Some(i) = self.menu_row {
                if i < self.items.len() {
                    self.items.remove(i);
                    self.row_refs.clear();
                    self.save();
                }
            }
        }
        self.redraw(cx);
    }

    /// Persist the current list to refs/<map>.json.
    fn save(&self) {
        if let Some(map) = &self.current_map {
            save_items(map, &self.items);
        }
    }
}

impl RefsPanelRef {
    /// Switch the panel to another map (rel path), loading its reference
    /// list. `None` falls back to the default map.
    pub fn set_current_map(&self, cx: &mut Cx, map_file: Option<&str>) {
        if let Some(mut w) = self.borrow_mut() {
            if w.current_map.as_deref() != map_file {
                w.current_map = map_file.map(|s| s.to_string());
                w.items = map_file.map(load_items).unwrap_or_default();
                w.row_refs.clear();
                w.redraw(cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_path_mirrors_map_dirs() {
        assert_eq!(
            refs_path("maps/map.json"),
            app_base_dir().join("refs/map.json")
        );
        assert_eq!(
            refs_path("maps/backup/old.json"),
            app_base_dir().join("refs/backup/old.json")
        );
    }

    #[test]
    fn converted_path_unique_ifies_collisions() {
        let dir = app_base_dir().join("docs");
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..3 {
            let name = if n == 0 {
                "report.md".to_string()
            } else {
                format!("report-{n}.md")
            };
            std::fs::remove_file(dir.join(name)).ok();
        }
        let source = std::path::Path::new("/somewhere/report.docx");
        let first = converted_path(source);
        assert_eq!(first, dir.join("report.md"));
        std::fs::write(&first, "x").unwrap();
        let second = converted_path(source);
        assert_eq!(second, dir.join("report-1.md"));
        std::fs::write(&second, "x").unwrap();
        let third = converted_path(source);
        assert_eq!(third, dir.join("report-2.md"));
        std::fs::write(&third, "x").unwrap();
        std::fs::remove_file(&first).ok();
        std::fs::remove_file(&second).ok();
        std::fs::remove_file(&third).ok();
    }

    #[test]
    fn excerpt_strips_markdown_and_collapses() {
        let e = excerpt("# 标题\n\n一些**内容** 和\n多行文字", 100);
        assert_eq!(e, "标题 一些**内容** 和 多行文字");
    }

    #[test]
    fn excerpt_truncates_at_word_boundary() {
        let e = excerpt("a b c d e f g h i j k l m n o p", 10);
        assert_eq!(e, "a b c d e…");
        // CJK text never splits mid-word
        assert_eq!(excerpt("一二三四五六七八九十甲乙丙丁", 8), "一二三四五六七八…");
    }

    #[test]
    fn refs_json_roundtrip() {
        let items = vec![RefItem {
            desc: "excerpt".into(),
            value: "/a/b.md".to_string(),
            failed: true,
        }];
        let json = serde_json::to_string(&RefsFile {
            files: items
                .iter()
                .map(|i| FileRef {
                    path: i.value.clone(),
                    desc: String::new(),
                    failed: i.failed,
                })
                .collect(),
        })
        .unwrap();
        let data: RefsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(data.files[0].path, "/a/b.md");
        assert!(data.files[0].failed);
    }

    #[test]
    fn files_without_failed_key_default_to_false() {
        let old = r#"{"files":[{"path":"/a/b.md","desc":""}]}"#;
        let data: RefsFile = serde_json::from_str(old).unwrap();
        assert!(!data.files[0].failed);
    }

    #[test]
    fn stage_document_copies_markdown_into_docs() {
        let stem = format!("ue-md-{}", std::process::id());
        let src = std::env::temp_dir().join(format!("{stem}.md"));
        std::fs::write(&src, "# 标题\n内容").unwrap();
        let out = app_base_dir().join("docs").join(format!("{stem}.md"));
        std::fs::remove_file(&out).ok();
        let (value, failed) = stage_document(&src);
        assert_eq!(value, out.to_string_lossy());
        assert!(!failed);
        assert!(out.exists());
        std::fs::remove_file(&out).ok();
        std::fs::remove_file(&src).ok();
    }

    #[test]
    fn stage_document_converts_csv_into_docs() {
        let stem = format!("ue-csv-{}", std::process::id());
        let src = std::env::temp_dir().join(format!("{stem}.csv"));
        std::fs::write(&src, "name,age\nAlice,30\n").unwrap();
        let out = app_base_dir().join("docs").join(format!("{stem}.md"));
        std::fs::remove_file(&out).ok();
        let (value, failed) = stage_document(&src);
        assert_eq!(value, out.to_string_lossy());
        assert!(!failed);
        let md = std::fs::read_to_string(&out).unwrap();
        assert!(md.contains("Alice"));
        std::fs::remove_file(&out).ok();
        std::fs::remove_file(&src).ok();
    }

    #[test]
    fn stage_document_flags_unparseable_source() {
        let stem = format!("ue-bad-{}", std::process::id());
        let src = std::env::temp_dir().join(format!("{stem}.docx"));
        std::fs::write(&src, "definitely not a docx").unwrap();
        let (value, failed) = stage_document(&src);
        assert_eq!(value, src.to_string_lossy());
        assert!(failed);
        std::fs::remove_file(&src).ok();
    }
}
