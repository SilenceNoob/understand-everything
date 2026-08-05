use makepad_widgets::*;

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::mindmap::app_base_dir;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // One row in a pane's file list: the file name, or an inline name input
    // while a new entry is being created. FilePanel clones it per row.
    // Label/TextInput have no working set_visible (only View overrides it),
    // so each is wrapped in a box View that is toggled instead.
    let TreeRow = mod.widgets.View{
        width: Fill
        height: Fill
        flow: Right
        spacing: 8
        align: Align{y: 0.5}
        padding: Inset{left: 12, right: 8}
        // Type icon: folder (open/closed) for dirs, map/card doc for files.
        // Loaded per row at creation via Image::load_svg_from_shared_data.
        row_icon := mod.widgets.Image{
            width: (16.0)
            height: (16.0)
        }
        row_name_box := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Down
            row_name := mod.widgets.Label{
                width: Fill
                height: Fit
                text: ""
                draw_text.text_style.font_size: 13.0
                draw_text.color: #e6e9f0
            }
        }
        row_edit_box := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Down
            visible: false
            row_edit := mod.widgets.TextInput{
                width: Fill
                height: Fit
                empty_text: ""
            }
        }
    }

    mod.widgets.FilePanelBase = #(FilePanel::register_widget(vm))

    mod.widgets.FilePanel = set_type_default() do mod.widgets.FilePanelBase{
        width: Fit
        height: Fit
        clip_x: false
        clip_y: false

        // Row prototype; FilePanel clones it per row (never drawn here).
        tree_row := TreeRow{}

        // Chrome only: rounded bg + border behind the panes.
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
        // Divider line between the panes; FilePanel draws it with draw_abs
        // (the mindmap-crosshair pattern — DrawColor renders reliably, unlike
        // overriding the Splitter widget's custom shader).
        draw_divider +: {
            color: #ffffff30
        }
        // Highlight for the dir row a dragged file is hovering over.
        draw_drop_hl +: {
            color: #4c5c8c55
        }
        // Highlight for the context-menu item under the cursor.
        draw_menu_hl +: {
            color: #ffffff1a
        }
        // Highlight for the currently open map's row.
        draw_sel_hl +: {
            color: #7d8bd455
        }
        // Top pane: map files (v1: the single map.json).
        canvas_pane := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Down
            canvas_header := mod.widgets.View{
                width: Fill
                height: (32.0)
                flow: Down
                align: Align{y: 0.5}
                header_label := mod.widgets.Label{
                    width: Fill
                    height: Fit
                    padding: Inset{left: 12, right: 12}
                    text: "Map"
                    draw_text.text_style.font_size: 14.0
                    draw_text.color: #e6e9f0
                }
            }
        }
        // Bottom pane: the card tree of the current map. Rows are drawn by
        // FilePanel below the header (its own hit areas, see draw_rows).
        card_pane := mod.widgets.View{
            width: Fill
            height: Fill
            flow: Down
            card_header := mod.widgets.View{
                width: Fill
                height: (32.0)
                flow: Down
                align: Align{y: 0.5}
                header_label := mod.widgets.Label{
                    width: Fill
                    height: Fit
                    padding: Inset{left: 12, right: 12}
                    text: "Card"
                    draw_text.text_style.font_size: 14.0
                    draw_text.color: #e6e9f0
                }
            }
        }

        tab := mod.widgets.ButtonFlat{
            text: "▶"
            // Arrow sized to fit the 14px-wide tab; drop the theme's side
            // padding (8px each side would leave negative label space here).
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

        // Right-click context menu; FilePanel positions and draws it manually.
        // Item order matches MenuItem (NewMap, NewDir, Rename, Delete); hidden
        // items are skipped by the layout, so the visible order stays aligned
        // with the hit-test index math. RoundedView (not View): the plain
        // View's draw_bg shader ignores the color uniform.
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
            menu_new_map_box := mod.widgets.View{
                width: Fill
                height: (32.0)
                flow: Down
                align: Align{y: 0.5}
                menu_new_map := mod.widgets.Label{
                    max_lines: 1
                    text_overflow: TextOverflow.Ellipsis
                    width: Fill
                    height: Fit
                    padding: Inset{left: 10}
                    text: "新建 map"
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #e6e9f0
                }
            }
            menu_new_dir_box := mod.widgets.View{
                width: Fill
                height: (32.0)
                flow: Down
                align: Align{y: 0.5}
                menu_new_dir := mod.widgets.Label{
                    max_lines: 1
                    text_overflow: TextOverflow.Ellipsis
                    width: Fill
                    height: Fit
                    padding: Inset{left: 10}
                    text: "创建新目录"
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #e6e9f0
                }
            }
            menu_rename_box := mod.widgets.View{
                width: Fill
                height: (32.0)
                flow: Down
                align: Align{y: 0.5}
                menu_rename := mod.widgets.Label{
                    max_lines: 1
                    text_overflow: TextOverflow.Ellipsis
                    width: Fill
                    height: Fit
                    padding: Inset{left: 10}
                    text: "重命名"
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #e6e9f0
                }
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
/// Fixed pane header height (px) — the DSL headers are exactly this tall, so
/// the row lists below them land on known rects.
const PANE_HEADER_H: f64 = 32.0;
/// Row height (px).
const ROW_H: f64 = 30.0;
/// Directories backing the two lists: maps live in `maps/`, cards in `cards/`.
const MAPS_DIR: &str = "maps";
const CARDS_DIR: &str = "cards";
/// Context-menu geometry: fixed width, item height and padding.
const MENU_W: f64 = 220.0;
const MENU_ITEM_H: f64 = 32.0;
const MENU_PAD: f64 = 6.0;
/// Inline-edit list ids: 0 = map list, 1 = card list.
const LIST_MAP: u8 = 0;
const LIST_CARD: u8 = 1;
/// Min pointer travel (px) before a row press becomes a drag.
const DRAG_THRESHOLD: f64 = 6.0;
/// Row tree indent per depth and the arrow strip that toggles expansion.
const INDENT: f64 = 16.0;
const ARROW_W: f64 = 24.0;

/// Trim `raw`; None when empty. Inputs without an extension get `default_ext`
/// appended (maps `.json`, cards `.md`, dirs None).
pub(crate) fn normalize_name(raw: &str, default_ext: Option<&str>) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if !s.contains('.') {
        if let Some(ext) = default_ext {
            return Some(format!("{s}{ext}"));
        }
    }
    Some(s.to_string())
}

/// Context-menu items, in DSL order (NewMap, NewDir, Rename, Delete).
#[derive(Clone, Copy, PartialEq, Debug)]
enum MenuItem {
    NewMap,
    NewDir,
    Rename,
    Delete,
}

/// Context-menu items for a right-click: any row target adds 重命名; 删除
/// applies to map rows (files and dirs) and card dirs.
fn menu_items_for(target: Option<(u8, usize)>, target_is_dir: bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::NewMap, MenuItem::NewDir];
    if let Some((list, _)) = target {
        items.push(MenuItem::Rename);
        if list == LIST_MAP || target_is_dir {
            items.push(MenuItem::Delete);
        }
    }
    items
}

/// The menu item index under `abs`, or None outside the items (blank strip,
/// padding, out of the menu).
fn menu_item_index(menu_rect: Rect, items: usize, abs: DVec2) -> Option<usize> {
    if !menu_rect.contains(abs) {
        return None;
    }
    let idx = ((abs.y - menu_rect.pos.y - MENU_PAD) / MENU_ITEM_H).floor() as isize;
    if idx < 0 || idx as usize >= items {
        return None;
    }
    Some(idx as usize)
}

/// Display name for a list row value (rel path, dirs carry a trailing "/"):
/// just the last path segment — files show their stem (no extension), dirs
/// their name without the trailing "/".
pub(crate) fn display_name(rel: &str) -> String {
    let name = rel
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(rel);
    std::path::Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

/// The row's type icon: open/closed folder for dirs, a map or card document
/// for files (by list).
fn row_icon_svg(list: u8, row: &Row, expanded: &HashSet<String>) -> &'static str {
    if row.is_dir() {
        if expanded.contains(&row.value) {
            "folder-open.svg"
        } else {
            "folder.svg"
        }
    } else if list == LIST_MAP {
        "map.svg"
    } else {
        "card.svg"
    }
}

/// Panel body rect in window coords, written every draw pass; the mindmap
/// reads it to keep wheel zoom off the panel.
pub(crate) static PANEL_RECT: Mutex<Option<Rect>> = Mutex::new(None);

/// True while an inline name edit (新建 map / 创建新目录) is active; the
/// mindmap skips its keyboard shortcuts so typing doesn't move the map.
pub(crate) static NAME_EDITING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_name_editing() -> bool {
    NAME_EDITING.load(std::sync::atomic::Ordering::Relaxed)
}

/// Sort list rows: directories (trailing "/") first, then files, each group
/// by name.
fn sort_rows(names: &mut Vec<String>) {
    names.sort_by(|a, b| {
        let a_dir = a.ends_with('/');
        let b_dir = b.ends_with('/');
        b_dir.cmp(&a_dir).then_with(|| a.cmp(b))
    });
}

/// One visible row in a pane's tree: a rel path with its tree depth.
/// Dirs carry a trailing "/" (is_dir = value.ends_with('/')).
#[derive(Clone, PartialEq, Debug)]
struct Row {
    value: String,
    depth: usize,
}

impl Row {
    fn is_dir(&self) -> bool {
        self.value.ends_with('/')
    }
}

/// One-level listing of `rel` ("" for the pane root, "maps/docs" for a
/// subdir): children rel paths, dirs first, then files, by name. Files must
/// match `ext` when given (maps "json", cards "md").
fn scan_dir(base: &std::path::Path, rel: &str, ext: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(base.join(rel))
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().is_dir()
                        || ext.is_none_or(|x| e.path().extension().is_some_and(|e| e == x))
                })
                .map(|e| {
                    let child = e.path();
                    let rel = child
                        .strip_prefix(base)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if child.is_dir() {
                        format!("{rel}/")
                    } else {
                        rel
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    sort_rows(&mut names);
    names
}

/// Flatten a directory listing into visible rows, recursing into dirs that
/// are in `expanded` (rel paths with trailing "/").
fn flatten(
    base: &std::path::Path,
    entries: &[String],
    expanded: &HashSet<String>,
    depth: usize,
    ext: Option<&str>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for value in entries {
        let is_dir = value.ends_with('/');
        rows.push(Row {
            value: value.clone(),
            depth,
        });
        if is_dir && expanded.contains(value) {
            let children = scan_dir(base, value.trim_end_matches('/'), ext);
            rows.extend(flatten(base, &children, expanded, depth + 1, ext));
        }
    }
    rows
}

/// All map files (rel paths ending ".json") under maps/, recursively.
pub(crate) fn all_map_files(base: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![String::from("maps")];
    while let Some(dir) = stack.pop() {
        if let Ok(it) = std::fs::read_dir(base.join(&dir)) {
            for e in it.flatten() {
                let rel = e
                    .path()
                    .strip_prefix(base)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if e.path().is_dir() {
                    stack.push(rel);
                } else if rel.ends_with(".json") {
                    out.push(rel);
                }
            }
        }
    }
    out.sort();
    out
}

/// Destination rel path for moving `from` into the dir row `dir` (which
/// carries a trailing "/"): "cards/docs/" + "cards/a.md" → "cards/docs/a.md".
fn moved_path(dir: &str, from: &str) -> Option<String> {
    let name = from.rsplit('/').next().filter(|s| !s.is_empty())?;
    Some(format!("{dir}{name}"))
}

/// Row index under `abs` in a list of `rows` rows, or None when outside.
fn row_index_at(rows: usize, list: Rect, abs: DVec2, scroll: f64) -> Option<usize> {
    let row_i = ((abs.y - list.pos.y + scroll) / ROW_H).floor() as isize;
    if row_i < 0 || row_i as usize >= rows {
        return None;
    }
    Some(row_i as usize)
}

/// Clamp a list scroll offset by `dy` px; returns true when it moved.
fn scroll_rows(n: usize, list: Rect, dy: f64, scroll: &mut f64) -> bool {
    if n == 0 || list.size.y <= 0.0 {
        return false;
    }
    let max = (n as f64 * ROW_H - list.size.y).max(0.0);
    let old = *scroll;
    *scroll = (*scroll + dy).clamp(0.0, max);
    *scroll != old
}

/// Draw one pane's row list (clipped to the list rect), lazily creating row
/// widgets and registering the list hit area. `edit` marks the row showing
/// the inline name input instead of the name label. Map rows whose value
/// matches `selected` are highlighted (the currently open map).
fn draw_rows(
    cx: &mut Cx2d,
    scope: &mut Scope,
    list: u8,
    rows: &[Row],
    template: &ScriptObjectRef,
    refs: &mut Vec<WidgetRef>,
    list_rect: Rect,
    scroll: f64,
    area: &mut Area,
    edit: Option<usize>,
    expanded: &HashSet<String>,
    selected: Option<&str>,
    sel_hl: &mut DrawColor,
) {
    if rows.is_empty() || list_rect.size.y <= 0.0 {
        return;
    }
    cx.push_clip_rect(list_rect);
    for (i, row) in rows.iter().enumerate() {
        let r = Rect {
            pos: dvec2(list_rect.pos.x, list_rect.pos.y + i as f64 * ROW_H - scroll),
            size: dvec2(list_rect.size.x, ROW_H),
        };
        if !list_rect.intersects(r) {
            continue;
        }
        if list == LIST_MAP && selected == Some(row.value.as_str()) {
            sel_hl.draw_abs(cx, r);
        }
        let fresh = refs.get(i).is_none();
        let w = row_ref(cx, template, refs, i, list, row, expanded);
        if Some(i) == edit {
            w.view(cx, ids!(row_name_box)).set_visible(cx, false);
            let box_view = w.view(cx, ids!(row_edit_box));
            box_view.set_visible(cx, true);
            // Seed the input only on creation; later passes must not clobber
            // what the user typed (the row value stays "" while editing).
            // Rename seeds with the current display name.
            if fresh {
                box_view
                    .text_input(cx, ids!(row_edit))
                    .set_text(cx, &display_name(&row.value));
            }
        } else {
            w.view(cx, ids!(row_name_box)).set_visible(cx, true);
            w.view(cx, ids!(row_edit_box)).set_visible(cx, false);
        }
        let _ = w.draw_walk(
            cx,
            scope,
            Walk {
                abs_pos: Some(r.pos),
                width: Size::Fixed(r.size.x),
                height: Size::Fixed(ROW_H),
                margin: Inset {
                    left: row.depth as f64 * INDENT,
                    ..Inset::default()
                },
                ..Walk::default()
            },
        );
    }
    cx.add_aligned_rect_area(area, list_rect);
    cx.pop_clip_rect();
}

/// Lazily clone a row from the template, set its texts and load its type
/// icon (folders by expansion state, files by list).
fn row_ref(
    cx: &mut Cx,
    template: &ScriptObjectRef,
    refs: &mut Vec<WidgetRef>,
    i: usize,
    list: u8,
    row: &Row,
    expanded: &HashSet<String>,
) -> WidgetRef {
    if let Some(r) = refs.get(i) {
        return r.clone();
    }
    let value = template.as_object().into();
    let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
    w.label(cx, ids!(row_name)).set_text(cx, &display_name(&row.value));
    let icon = row_icon_svg(list, row, expanded);
    let icon_path = app_base_dir().join("resources").join(icon);
    if let Ok(bytes) = std::fs::read(&icon_path) {
        let _ = w
            .image(cx, ids!(row_icon))
            .load_svg_from_shared_data(cx, bytes.into());
    }
    refs.push(w.clone());
    w
}

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

    #[rust(false)]
    opened: bool,
    /// 0 = collapsed off the left edge, 1 = fully open; eases toward the
    /// target on timer ticks.
    #[rust]
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

    #[rust]
    row_template: Option<ScriptObjectRef>,
    /// Visible tree rows (map files/dirs, card files/dirs), depth-indented.
    #[rust]
    map_rows: Vec<Row>,
    #[rust]
    card_rows: Vec<Row>,
    /// Lazily-created row widgets, index-aligned with the rows (cleared on
    /// rebuild so texts refresh).
    #[rust]
    map_row_refs: Vec<WidgetRef>,
    #[rust]
    card_row_refs: Vec<WidgetRef>,
    #[rust]
    map_scroll: f64,
    #[rust]
    card_scroll: f64,
    /// `maps/` and `cards/` dir mtimes when the lists were last built.
    /// Dir mtimes only change on add/remove/rename — exactly what the
    /// name-only lists show.
    #[rust]
    maps_mtime: Option<SystemTime>,
    #[rust]
    cards_mtime: Option<SystemTime>,
    /// mtimes of the expanded subdirs (rel paths with "/") — changes inside
    /// them don't touch the pane root's mtime.
    #[rust]
    expanded_mtimes: Vec<(String, SystemTime)>,
    /// Expanded dirs (rel paths with trailing "/"), session-only.
    #[rust]
    expanded: HashSet<String>,
    /// List viewports in window coords + hit areas, cached at draw time.
    #[rust]
    map_list_rect: Rect,
    #[rust]
    map_list_area: Area,
    #[rust]
    card_list_rect: Rect,
    #[rust]
    card_list_area: Area,

    /// Right-click context menu state.
    #[rust]
    menu_open: bool,
    /// Row the menu was opened on: (list id, row index). None = blank panel.
    #[rust]
    menu_row: Option<(u8, usize)>,
    /// List the 创建新目录 item will target (from the right-click position).
    #[rust]
    menu_target: u8,
    /// Visible menu items, in DSL order.
    #[rust]
    menu_items: Vec<MenuItem>,
    #[rust]
    menu_rect: Rect,
    /// Context-menu item under the cursor (for the hover highlight).
    #[rust]
    menu_hover: Option<usize>,
    #[rust]
    ctx_menu_ref: Option<WidgetRef>,
    /// Window-wide area that captures all presses while menu is open or a
    /// row is being renamed inline.
    #[rust]
    modal_area: Area,

    /// Inline edit state: (list id, row index, kind). The row shows the name
    /// input; NewMap/NewDir rows are placeholders, Rename edits in place.
    #[rust]
    editing: Option<(u8, usize, EditKind)>,
    /// The list's rows before the placeholder was inserted (for cancel).
    #[rust]
    edit_snapshot: Vec<Row>,
    #[rust]
    edit_focus_pending: bool,
    /// Drag state: (list, row, start abs) of a pressed file row.
    #[rust]
    drag_press: Option<(u8, usize, DVec2)>,
    /// True once the press moved past DRAG_THRESHOLD.
    #[rust]
    drag_active: bool,
    /// Hovered drop-target dir row while dragging: (list, row).
    #[rust]
    drag_target: Option<(u8, usize)>,
    #[live]
    draw_drop_hl: DrawColor,
    #[live]
    draw_menu_hl: DrawColor,
    #[live]
    draw_sel_hl: DrawColor,
    /// The currently open map (rel path, e.g. "maps/foo.json"); its row gets
    /// the selection highlight. Set by App::open_map alongside switch_map.
    #[rust]
    current_map: Option<String>,
}

/// What an inline edit will create / modify on confirm.
#[derive(Clone, Copy, PartialEq, Debug)]
enum EditKind {
    NewMap,
    NewDir,
    Rename,
}

impl ScriptHook for FilePanel {
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
                            if id == live_id!(tree_row) {
                                self.row_template = Some(vm.bx.heap.new_object_ref(template_obj));
                            }
                        }
                    }
                }
            });
        }
    }
}

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
        PANEL_RECT.lock().unwrap().replace(self.panel_rect);

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
            // File lists: below each pane's fixed 32px header.
            self.rebuild_rows();
            let map_list = Rect {
                pos: panel.pos + dvec2(0.0, PANE_HEADER_H),
                size: dvec2(panel.size.x, (a_h - PANE_HEADER_H).max(0.0)),
            };
            self.map_list_rect = map_list;
            if let Some(t) = &self.row_template {
                let edit = self.edit_index(LIST_MAP);
                let expanded = &self.expanded;
                draw_rows(
                    cx,
                    scope,
                    LIST_MAP,
                    &self.map_rows,
                    t,
                    &mut self.map_row_refs,
                    map_list,
                    self.map_scroll,
                    &mut self.map_list_area,
                    edit,
                    expanded,
                    self.current_map.as_deref(),
                    &mut self.draw_sel_hl,
                );
            }
            let card_list = Rect {
                pos: panel.pos + dvec2(0.0, a_h + 1.0 + PANE_HEADER_H),
                size: dvec2(panel.size.x, (b_h - PANE_HEADER_H).max(0.0)),
            };
            self.card_list_rect = card_list;
            if let Some(t) = &self.row_template {
                let edit = self.edit_index(LIST_CARD);
                let expanded = &self.expanded;
                draw_rows(
                    cx,
                    scope,
                    LIST_CARD,
                    &self.card_rows,
                    t,
                    &mut self.card_row_refs,
                    card_list,
                    self.card_scroll,
                    &mut self.card_list_area,
                    edit,
                    expanded,
                    None,
                    &mut self.draw_sel_hl,
                );
            }
            // Drop-target highlight while dragging a file over a dir row.
            if let Some((list, i)) = self.drag_target {
                let (rows, rect, scroll) = self.list_geometry(list);
                if rows.get(i).is_some() {
                    let r = Rect {
                        pos: rect.pos + dvec2(0.0, i as f64 * ROW_H - scroll),
                        size: dvec2(rect.size.x, ROW_H),
                    };
                    self.draw_drop_hl.draw_abs(cx, r);
                }
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
        // Context menu on top of everything; hover highlight under the cursor
        // (tracked manually — makepad's hover slot is stolen by the canvas).
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
                            // +MENU_PAD on x: the items sit inside the view's
                            // 6px padding, so the bar aligns with them.
                            pos: self.menu_rect.pos
                                + dvec2(MENU_PAD, MENU_PAD + i as f64 * MENU_ITEM_H),
                            size: dvec2(self.menu_rect.size.x - 2.0 * MENU_PAD, MENU_ITEM_H),
                        },
                    );
                }
                cx.pop_clip_rect();
            }
        }
        // Focus the inline name input once its row has been drawn.
        if self.edit_focus_pending {
            if let Some((list, i, _)) = self.editing {
                let refs = if list == LIST_MAP {
                    &self.map_row_refs
                } else {
                    &self.card_row_refs
                };
                if let Some(w) = refs.get(i) {
                    let input = w.text_input(cx, ids!(row_edit));
                    if input.area().is_valid(cx) {
                        cx.set_key_focus(input.area());
                        self.edit_focus_pending = false;
                    }
                }
            }
        }
        // While the menu is open or a row is being named, a window-wide
        // modal area captures every press.
        if self.menu_open || self.editing.is_some() {
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
        if let Some(tab) = self.tab_widget(cx) {
            // hover/press visuals on the tab button
            tab.handle_event(cx, event, scope);
        }
        // The inline edit input must see events itself to process keystrokes
        // (typing/IME); row widgets are not forwarded otherwise.
        if let Some((list, i, _)) = self.editing {
            let refs = if list == LIST_MAP {
                &self.map_row_refs
            } else {
                &self.card_row_refs
            };
            if let Some(w) = refs.get(i).cloned() {
                w.handle_event(cx, event, scope);
            }
        }
        // Modal state (context menu / inline name edit) grabs every press
        // first, so the lists/tab/divider below can't fire behind it.
        if self.menu_open || self.editing.is_some() {
            match event.hits_with_capture_overload(cx, self.modal_area, true) {
                Hit::FingerDown(fe) if fe.is_primary_hit() => {
                    if self.menu_open {
                        if self.menu_rect.contains(fe.abs) {
                            self.on_menu_press(cx, fe.abs);
                        } else {
                            self.menu_open = false;
                            self.redraw(cx);
                        }
                    } else if let Some((list, i, _)) = self.editing {
                        let (list_rect, scroll) = self.edit_geometry(list);
                        let row_rect = Rect {
                            pos: list_rect.pos + dvec2(0.0, i as f64 * ROW_H - scroll),
                            size: dvec2(list_rect.size.x, ROW_H),
                        };
                        if row_rect.contains(fe.abs) {
                            // keep editing; re-focus the input
                            self.edit_focus_pending = true;
                            self.redraw(cx);
                        } else {
                            self.cancel_edit(cx);
                        }
                    }
                }
                _ => {}
            }
        }
        // Inline name edit: Enter confirms, Esc cancels.
        if self.editing.is_some() {
            if let Event::KeyDown(ke) = event {
                match ke.key_code {
                    KeyCode::ReturnKey => self.confirm_edit(cx),
                    KeyCode::Escape => self.cancel_edit(cx),
                    _ => {}
                }
            }
        }
        // capture_overload: the mindmap canvas (earlier in tree order, covering
        // the whole body) hits first and marks t.handled, which makes plain
        // hits() skip our areas entirely (same trick as FloatPanel).
        match event.hits_with_capture_overload(cx, self.tab_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && self.editing.is_none() => {
                self.toggle(cx);
            }
            _ => {}
        }
        // Divider drag: capture_overload (the mindmap canvas shadows plain
        // hits, same as FloatPanel) on the strip around the divider line.
        // Right-edge drag to resize the panel width. Checked before the
        // splitter so grabbing the corner of the strip resizes the width.
        match event.hits_with_capture_overload(cx, self.edge_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && self.editing.is_none() => {
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
            Hit::FingerDown(fe) if fe.is_primary_hit() && self.editing.is_none() => {
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
            // Menu hover: tracked from the raw cursor, redraw only on change.
            if self.menu_open {
                let hover = menu_item_index(self.menu_rect, self.menu_items.len(), e.abs);
                if hover != self.menu_hover {
                    self.menu_hover = hover;
                    self.redraw(cx);
                }
            }
        }
        // Map list: press a file row to drag it into a maps/ dir, or click to
        // switch the map (fired on FingerUp so a drag never switches). Wheel
        // scrolls both lists (Scroll bypasses the handled flag).
        match event.hits_with_capture_overload(cx, self.map_list_area, true) {
            Hit::FingerDown(fe)
                if fe.is_primary_hit() && self.editing.is_none() && !self.menu_open =>
            {
                if let Some(i) = row_index_at(
                    self.map_rows.len(),
                    self.map_list_rect,
                    fe.abs,
                    self.map_scroll,
                ) {
                    if !self.map_rows[i].is_dir() {
                        // file rows start a drag (or a click on FingerUp)
                        self.drag_press = Some((LIST_MAP, i, fe.abs));
                    } else {
                        // dir rows: the arrow strip toggles expansion
                        let indent_x =
                            self.map_list_rect.pos.x + self.map_rows[i].depth as f64 * INDENT;
                        if fe.abs.x < indent_x + ARROW_W {
                            self.toggle_expand(cx, LIST_MAP, i);
                        }
                    }
                }
            }
            Hit::FingerMove(fe) => {
                self.track_drag(cx, fe.abs, LIST_MAP);
            }
            Hit::FingerUp(_) => {
                self.finish_drag(cx, LIST_MAP);
            }
            Hit::FingerScroll(fe) => {
                if scroll_rows(self.map_rows.len(), self.map_list_rect, fe.scroll.y, &mut self.map_scroll) {
                    self.redraw(cx);
                }
            }
            _ => {}
        }
        match event.hits_with_capture_overload(cx, self.card_list_area, true) {
            Hit::FingerDown(fe)
                if fe.is_primary_hit() && self.editing.is_none() && !self.menu_open =>
            {
                if let Some(i) = row_index_at(
                    self.card_rows.len(),
                    self.card_list_rect,
                    fe.abs,
                    self.card_scroll,
                ) {
                    if !self.card_rows[i].is_dir() {
                        self.drag_press = Some((LIST_CARD, i, fe.abs));
                    } else {
                        let indent_x =
                            self.card_list_rect.pos.x + self.card_rows[i].depth as f64 * INDENT;
                        if fe.abs.x < indent_x + ARROW_W {
                            self.toggle_expand(cx, LIST_CARD, i);
                        }
                    }
                }
            }
            Hit::FingerMove(fe) => {
                self.track_drag(cx, fe.abs, LIST_CARD);
            }
            Hit::FingerUp(_) => {
                self.finish_drag(cx, LIST_CARD);
            }
            Hit::FingerScroll(fe) => {
                if scroll_rows(self.card_rows.len(), self.card_list_rect, fe.scroll.y, &mut self.card_scroll) {
                    self.redraw(cx);
                }
            }
            _ => {}
        }
        // Right-click anywhere on the panel opens (or repositions) the context
        // menu; right-clicking a map row enables the "删除" item.
        match event.hits_with_capture_overload(cx, self.panel_area, true) {
            Hit::FingerDown(fe)
                if matches!(fe.device, DigitDevice::Mouse { button } if button.is_secondary()) =>
            {
                self.open_menu(cx, fe.abs);
            }
            _ => {}
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

    fn ctx_menu_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.ctx_menu_ref.is_none() {
            self.ctx_menu_ref = Some(self.view.widget(cx, ids!(ctx_menu)));
        }
        self.ctx_menu_ref.clone()
    }

    /// The list being edited: (rect, scroll) for the given list id.
    fn edit_geometry(&self, list: u8) -> (Rect, f64) {
        if list == LIST_MAP {
            (self.map_list_rect, self.map_scroll)
        } else {
            (self.card_list_rect, self.card_scroll)
        }
    }

    /// The row index being edited in `list`, if any.
    fn edit_index(&self, list: u8) -> Option<usize> {
        match self.editing {
            Some((l, i, _)) if l == list => Some(i),
            _ => None,
        }
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

    /// Rebuild the row lists when the pane roots or any expanded subdir
    /// change (cheap metadata stats per draw pass; scanning only on change).
    fn rebuild_rows(&mut self) {
        let base = app_base_dir();
        let maps_mtime = std::fs::metadata(base.join(MAPS_DIR))
            .and_then(|m| m.modified())
            .ok();
        let cards_mtime = std::fs::metadata(base.join(CARDS_DIR))
            .and_then(|m| m.modified())
            .ok();
        // Expanded subdirs: an edit inside them never touches the root mtime.
        let mut expanded_mtimes = Vec::with_capacity(self.expanded.len());
        let mut dirty = maps_mtime != self.maps_mtime || cards_mtime != self.cards_mtime;
        for dir in &self.expanded {
            let m = std::fs::metadata(base.join(dir.trim_end_matches('/')))
                .and_then(|m| m.modified())
                .ok();
            let changed = match self
                .expanded_mtimes
                .iter()
                .find(|(d, _)| d == dir)
            {
                Some((_, old)) => Some(*old) != m,
                None => true,
            };
            expanded_mtimes.push((dir.clone(), m.unwrap_or_else(SystemTime::now)));
            dirty |= changed;
        }
        self.expanded_mtimes = expanded_mtimes;
        if dirty {
            self.maps_mtime = maps_mtime;
            self.cards_mtime = cards_mtime;
            self.rebuild_now();
        }
    }

    /// Rebuild both row lists unconditionally (expansion toggles and mtime
    /// changes share this).
    fn rebuild_now(&mut self) {
        let base = app_base_dir();
        self.map_rows = flatten(
            &base,
            &scan_dir(&base, MAPS_DIR, Some("json")),
            &self.expanded,
            0,
            Some("json"),
        );
        self.card_rows = flatten(
            &base,
            &scan_dir(&base, CARDS_DIR, Some("md")),
            &self.expanded,
            0,
            Some("md"),
        );
        self.map_row_refs.clear();
        self.card_row_refs.clear();
        self.map_scroll = 0.0;
        self.card_scroll = 0.0;
    }

    /// Toggle expansion of the dir at visible row `i` in `list`.
    fn toggle_expand(&mut self, cx: &mut Cx, list: u8, i: usize) {
        let Some(dir) = self.row_value(list, i) else {
            return;
        };
        if !dir.ends_with('/') {
            return;
        }
        if !self.expanded.remove(&dir) {
            self.expanded.insert(dir);
        }
        self.rebuild_now();
        self.redraw(cx);
    }

    fn toggle(&mut self, cx: &mut Cx) {
        self.opened = !self.opened;
        self.menu_open = false;
        if self.editing.is_some() {
            self.cancel_edit(cx);
        }
        if let Some(tab) = self.tab_widget(cx) {
            tab.set_text(cx, if self.opened { "◀" } else { "▶" });
        }
        if self.slide_timer.is_none() {
            self.slide_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_timer_time = 0.0;
        }
        self.redraw(cx);
    }

    /// Open the context menu at `abs`, clamped inside the panel. The target
    /// row (map or card list) enables 重命名; map rows and card dirs get 删除.
    /// The 创建新目录 item targets the pane the right-click happened in.
    fn open_menu(&mut self, cx: &mut Cx, abs: DVec2) {
        if self.editing.is_some() {
            self.cancel_edit(cx);
        }
        self.menu_row = row_index_at(self.map_rows.len(), self.map_list_rect, abs, self.map_scroll)
            .map(|i| (LIST_MAP, i))
            .or_else(|| {
                row_index_at(self.card_rows.len(), self.card_list_rect, abs, self.card_scroll)
                    .map(|i| (LIST_CARD, i))
            });
        let target_is_dir = self
            .menu_row
            .and_then(|(l, i)| self.row_value(l, i))
            .is_some_and(|v| v.ends_with('/'));
        let a_h = (self.split * self.panel_rect.size.y).clamp(0.0, self.panel_rect.size.y);
        self.menu_target = if abs.y < self.panel_rect.pos.y + a_h {
            LIST_MAP
        } else {
            LIST_CARD
        };
        self.menu_items = menu_items_for(self.menu_row, target_is_dir);
        let h = MENU_PAD * 2.0 + self.menu_items.len() as f64 * MENU_ITEM_H;
        let panel = self.panel_rect;
        // clamp(min > max) panics; keep the menu inside the panel, or at its
        // edge when the panel is narrower than the menu.
        let max_x = (panel.pos.x + panel.size.x - MENU_W).max(panel.pos.x);
        let max_y = (panel.pos.y + panel.size.y - h).max(panel.pos.y);
        let pos = dvec2(
            abs.x.clamp(panel.pos.x, max_x),
            abs.y.clamp(panel.pos.y, max_y),
        );
        self.menu_rect = Rect {
            pos,
            size: dvec2(MENU_W, h),
        };
        if let Some(menu) = self.ctx_menu_widget(cx) {
            let target_display = self
                .menu_row
                .and_then(|(list, i)| self.row_value(list, i))
                .as_deref()
                .map(display_name);
            menu.label(cx, ids!(menu_rename)).set_text(
                cx,
                &target_display
                    .as_deref()
                    .map(|n| format!("重命名 {n}"))
                    .unwrap_or_default(),
            );
            menu.view(cx, ids!(menu_rename_box))
                .set_visible(cx, self.menu_row.is_some());
            menu.label(cx, ids!(menu_delete)).set_text(
                cx,
                &target_display
                    .as_deref()
                    .map(|n| format!("删除 {n}"))
                    .unwrap_or_default(),
            );
            // 删除 exists for map rows and card dirs (matches menu_items_for).
            menu.view(cx, ids!(menu_delete_box))
                .set_visible(cx, self.menu_items.contains(&MenuItem::Delete));
        }
        // The item under the right-click lands pre-highlighted.
        self.menu_hover = menu_item_index(self.menu_rect, self.menu_items.len(), abs);
        self.menu_open = true;
        self.redraw(cx);
    }

    /// The row value at (list, index), if in bounds.
    fn row_value(&self, list: u8, i: usize) -> Option<String> {
        let rows = if list == LIST_MAP {
            &self.map_rows
        } else {
            &self.card_rows
        };
        rows.get(i).map(|r| r.value.clone())
    }

    /// (rows, viewport rect, scroll) for a list id.
    fn list_geometry(&self, list: u8) -> (&[Row], Rect, f64) {
        if list == LIST_MAP {
            (&self.map_rows, self.map_list_rect, self.map_scroll)
        } else {
            (&self.card_rows, self.card_list_rect, self.card_scroll)
        }
    }

    /// Update the drag for a press in `list`: activate past the threshold and
    /// track the hovered dir row (drop targets are dirs of the same list, so
    /// map files can never land in card dirs and vice versa).
    fn track_drag(&mut self, cx: &mut Cx, abs: DVec2, list: u8) {
        let Some((l, _, start)) = self.drag_press else {
            return;
        };
        if l != list {
            return;
        }
        if !self.drag_active && (abs - start).length() >= DRAG_THRESHOLD {
            self.drag_active = true;
        }
        let (rows, rect, scroll) = self.list_geometry(list);
        let target = if self.drag_active {
            row_index_at(rows.len(), rect, abs, scroll).filter(|&i| rows[i].is_dir())
        } else {
            None
        };
        if target != self.drag_target.map(|(_, i)| i) {
            self.drag_target = target.map(|i| (list, i));
            self.redraw(cx);
        }
    }

    /// End the drag for a press in `list`: drop onto a dir (move via
    /// RenameFile) or, without drag, treat it as a click (map rows switch).
    fn finish_drag(&mut self, cx: &mut Cx, list: u8) {
        let Some((l, i, _)) = self.drag_press.take() else {
            return;
        };
        if l != list {
            return;
        }
        let from = self.row_value(list, i);
        let dragged = self.drag_active;
        let to = if dragged {
            self.drag_target
                .filter(|&(tl, _)| tl == list)
                .and_then(|(_, ti)| self.row_value(list, ti))
                .zip(from.as_deref())
                .and_then(|(dir, from)| moved_path(&dir, from))
        } else {
            None
        };
        self.drag_active = false;
        self.drag_target = None;
        if let Some(to) = to {
            if let Some(from) = from {
                cx.widget_action(self.widget_uid(), FilePanelAction::RenameFile(from, to));
            }
        } else if !dragged && list == LIST_MAP {
            if let Some(from) = from {
                cx.widget_action(self.widget_uid(), FilePanelAction::MapClicked(from));
            }
        }
        self.redraw(cx);
    }

    /// Map a press inside the menu to its item (menu_items, DSL order).
    fn on_menu_press(&mut self, cx: &mut Cx, abs: DVec2) {
        let idx = menu_item_index(self.menu_rect, self.menu_items.len(), abs);
        self.menu_open = false;
        self.menu_hover = None;
        let Some(idx) = idx else {
            self.redraw(cx);
            return;
        };
        match self.menu_items[idx] {
            MenuItem::NewMap => self.start_edit(cx, LIST_MAP, EditKind::NewMap),
            MenuItem::NewDir => self.start_edit(cx, self.menu_target, EditKind::NewDir),
            MenuItem::Rename => {
                if let Some((list, _)) = self.menu_row {
                    self.start_edit(cx, list, EditKind::Rename);
                }
            }
            MenuItem::Delete => {
                if let Some((list, i)) = self.menu_row {
                    if let Some(value) = self.row_value(list, i) {
                        cx.widget_action(
                            self.widget_uid(),
                            FilePanelAction::DeleteEntry(value),
                        );
                    }
                }
            }
        }
        self.redraw(cx);
    }

    /// Start an inline name edit: NewMap appends a placeholder to the map
    /// list, NewDir heads the target list, Rename edits the right-clicked row
    /// in place (the input seeds with its current display name).
    fn start_edit(&mut self, cx: &mut Cx, list: u8, kind: EditKind) {
        let index = match kind {
            EditKind::NewMap => self.map_rows.len(),
            EditKind::NewDir => 0,
            EditKind::Rename => self.menu_row.map(|(_, i)| i).unwrap_or(0),
        };
        let rows = if list == LIST_MAP {
            &mut self.map_rows
        } else {
            &mut self.card_rows
        };
        self.edit_snapshot = rows.clone();
        if kind != EditKind::Rename {
            rows.insert(
                index,
                Row {
                    value: String::new(),
                    depth: 0,
                },
            );
        }
        self.editing = Some((list, index, kind));
        self.edit_focus_pending = true;
        if list == LIST_MAP {
            self.map_row_refs.clear();
        } else {
            self.card_row_refs.clear();
        }
        NAME_EDITING.store(true, std::sync::atomic::Ordering::Relaxed);
        self.redraw(cx);
    }

    /// Restore the pre-edit list and stop editing.
    fn cancel_edit(&mut self, cx: &mut Cx) {
        let Some((list, _, _)) = self.editing else {
            return;
        };
        self.editing = None;
        NAME_EDITING.store(false, std::sync::atomic::Ordering::Relaxed);
        self.drop_edit_focus(cx, list);
        if list == LIST_MAP {
            self.map_rows = std::mem::take(&mut self.edit_snapshot);
            self.map_row_refs.clear();
        } else {
            self.card_rows = std::mem::take(&mut self.edit_snapshot);
            self.card_row_refs.clear();
        }
        self.redraw(cx);
    }

    /// Release key focus from the edit input so the map gets keys back.
    fn drop_edit_focus(&self, cx: &mut Cx, list: u8) {
        let refs = if list == LIST_MAP {
            &self.map_row_refs
        } else {
            &self.card_row_refs
        };
        let Some(i) = self.edit_index(list) else {
            return;
        };
        if let Some(w) = refs.get(i) {
            let input = w.text_input(cx, ids!(row_edit));
            if input.area() == cx.key_focus() {
                cx.set_key_focus(Area::Empty);
            }
        }
    }

    /// Read the inline name input; on success fire CreateMap/CreateDir/
    /// RenameFile and stop editing. Empty input and existing targets keep
    /// the edit going.
    fn confirm_edit(&mut self, cx: &mut Cx) {
        let Some((list, i, kind)) = self.editing else {
            return;
        };
        let refs = if list == LIST_MAP {
            &self.map_row_refs
        } else {
            &self.card_row_refs
        };
        let Some(w) = refs.get(i).cloned() else {
            return;
        };
        let raw = w.text_input(cx, ids!(row_edit)).text();
        let (default_ext, dir) = match kind {
            EditKind::NewMap => (Some(".json"), false),
            EditKind::NewDir => (None, true),
            EditKind::Rename => {
                let is_dir = self
                    .row_value(list, i)
                    .is_some_and(|v| v.ends_with('/'));
                let ext = if is_dir {
                    None
                } else if list == LIST_MAP {
                    Some(".json")
                } else {
                    Some(".md")
                };
                (ext, is_dir)
            }
        };
        let Some(name) = normalize_name(&raw, default_ext) else {
            return;
        };
        let base = app_base_dir();
        let dir_name = if dir { format!("{name}/") } else { name.clone() };
        let from = self.row_value(list, i).unwrap_or_default();
        let to = if list == LIST_MAP {
            format!("{MAPS_DIR}/{dir_name}")
        } else {
            format!("{CARDS_DIR}/{dir_name}")
        };
        // Same name or an existing target keeps the edit going.
        if to == from {
            return;
        }
        if base.join(&to).exists() {
            return;
        }
        // Replace the placeholder with the typed name; the dir mtime rebuild
        // will swap in the real scan result once the entry exists. Refs are
        // cleared so the row label (set at creation) shows the new name.
        let depth = if list == LIST_MAP {
            self.map_rows.get(i).map(|r| r.depth).unwrap_or(0)
        } else {
            self.card_rows.get(i).map(|r| r.depth).unwrap_or(0)
        };
        if list == LIST_MAP {
            self.map_rows[i] = Row {
                value: to.clone(),
                depth,
            };
            self.map_row_refs.clear();
        } else {
            self.card_rows[i] = Row {
                value: to.clone(),
                depth,
            };
            self.card_row_refs.clear();
        }
        self.editing = None;
        NAME_EDITING.store(false, std::sync::atomic::Ordering::Relaxed);
        self.drop_edit_focus(cx, list);
        let action = match kind {
            EditKind::NewMap => FilePanelAction::CreateMap(to),
            EditKind::NewDir => FilePanelAction::CreateDir(to),
            EditKind::Rename => FilePanelAction::RenameFile(from, to),
        };
        cx.widget_action(self.widget_uid(), action);
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

#[derive(Clone, Debug, Default)]
pub enum FilePanelAction {
    #[default]
    None,
    /// A map row was pressed; carries the map rel path (e.g. "maps/map.json").
    MapClicked(String),
    /// Context menu "删除": delete this map (rel path in `maps/`).
    DeleteEntry(String),
    /// Inline edit confirmed for 新建 map; carries the new map rel path.
    CreateMap(String),
    /// Inline edit confirmed for 创建新目录; carries the dir rel path
    /// (e.g. "maps/docs" or "cards/docs").
    CreateDir(String),
    /// Inline edit confirmed for 重命名; carries (old rel path, new rel path).
    RenameFile(String, String),
}

macro_rules! action_string_getter {
    ($name:ident, $variant:ident) => {
        pub fn $name(&self, actions: &Actions) -> Option<String> {
            if let Some(item) = actions.find_widget_action(self.widget_uid()) {
                if let FilePanelAction::$variant(s) = item.cast() {
                    return Some(s);
                }
            }
            None
        }
    };
}

impl FilePanelRef {
    action_string_getter!(map_clicked, MapClicked);
    action_string_getter!(delete_entry, DeleteEntry);
    action_string_getter!(create_map, CreateMap);
    action_string_getter!(create_dir, CreateDir);

    /// The (old rel path, new rel path) of a confirmed rename.
    pub fn rename_file(&self, actions: &Actions) -> Option<(String, String)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let FilePanelAction::RenameFile(from, to) = item.cast() {
                return Some((from, to));
            }
        }
        None
    }

    /// Highlight the row of the currently open map (`None` clears it).
    pub fn set_current_map(&self, cx: &mut Cx, map_file: Option<&str>) {
        if let Some(mut w) = self.borrow_mut() {
            if w.current_map.as_deref() != map_file {
                w.current_map = map_file.map(|s| s.to_string());
                w.redraw(cx);
            }
        }
    }
}

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

    #[test]
    fn scan_dir_lists_dirs_first_and_all_map_files_recurses() {
        let dir = std::env::temp_dir().join(format!("ue-filetree-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps/backup/sub")).unwrap();
        std::fs::create_dir_all(dir.join("cards/docs")).unwrap();
        for f in ["b.md", "a.md", "img.png", "tiger.svg"] {
            std::fs::write(dir.join("cards").join(f), "x").unwrap();
        }
        for f in ["b.json", "a.json", "notes.txt", "map.json"] {
            std::fs::write(dir.join("maps").join(f), "{}").unwrap();
        }
        std::fs::write(dir.join("maps/backup/old.json"), "{}").unwrap();
        std::fs::write(dir.join("maps/backup/sub/deep.json"), "{}").unwrap();
        // one level, dirs first, then files, each by name
        assert_eq!(
            scan_dir(&dir, "maps", Some("json")),
            vec![
                "maps/backup/",
                "maps/a.json",
                "maps/b.json",
                "maps/map.json"
            ]
        );
        assert_eq!(
            scan_dir(&dir, "cards", Some("md")),
            vec!["cards/docs/", "cards/a.md", "cards/b.md"]
        );
        // recursive map find walks subdirs
        assert_eq!(
            all_map_files(&dir),
            vec![
                "maps/a.json",
                "maps/b.json",
                "maps/backup/old.json",
                "maps/backup/sub/deep.json",
                "maps/map.json"
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn flatten_expands_and_indents_dirs() {
        let dir = std::env::temp_dir().join(format!("ue-filetree-test2-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cards/docs/nested")).unwrap();
        std::fs::write(dir.join("cards/a.md"), "x").unwrap();
        std::fs::write(dir.join("cards/docs/b.md"), "x").unwrap();
        std::fs::write(dir.join("cards/docs/nested/c.md"), "x").unwrap();
        let entries = scan_dir(&dir, "cards", Some("md"));
        // collapsed: only the top level
        let rows = flatten(&dir, &entries, &HashSet::new(), 0, Some("md"));
        assert_eq!(
            rows.iter()
                .map(|r| (r.value.as_str(), r.depth))
                .collect::<Vec<_>>(),
            vec![("cards/docs/", 0), ("cards/a.md", 0)]
        );
        // expanded: nested rows with increasing depth
        let mut expanded = HashSet::new();
        expanded.insert("cards/docs/".to_string());
        let rows = flatten(&dir, &entries, &expanded, 0, Some("md"));
        assert_eq!(
            rows.iter()
                .map(|r| (r.value.as_str(), r.depth))
                .collect::<Vec<_>>(),
            vec![
                ("cards/docs/", 0),
                ("cards/docs/nested/", 1),
                ("cards/docs/b.md", 1),
                ("cards/a.md", 0)
            ]
        );
        // double expansion reaches depth 2
        expanded.insert("cards/docs/nested/".to_string());
        let rows = flatten(&dir, &entries, &expanded, 0, Some("md"));
        assert_eq!(
            rows.iter()
                .map(|r| (r.value.as_str(), r.depth))
                .collect::<Vec<_>>(),
            vec![
                ("cards/docs/", 0),
                ("cards/docs/nested/", 1),
                ("cards/docs/nested/c.md", 2),
                ("cards/docs/b.md", 1),
                ("cards/a.md", 0)
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn normalize_name_appends_default_ext_and_rejects_empty() {
        assert_eq!(
            normalize_name("  my map ", Some(".json")),
            Some("my map.json".to_string())
        );
        assert_eq!(normalize_name("a.b", Some(".json")), Some("a.b".to_string()));
        assert_eq!(normalize_name("  ", Some(".json")), None);
        assert_eq!(normalize_name("docs", None), Some("docs".to_string()));
        assert_eq!(normalize_name(" card ", Some(".md")), Some("card.md".to_string()));
        assert_eq!(normalize_name("", None), None);
    }

    #[test]
    fn menu_items_follow_target_row() {
        assert_eq!(menu_items_for(None, false), vec![MenuItem::NewMap, MenuItem::NewDir]);
        // card file: no delete
        assert_eq!(
            menu_items_for(Some((LIST_CARD, 0)), false),
            vec![MenuItem::NewMap, MenuItem::NewDir, MenuItem::Rename]
        );
        // card dir: delete
        assert_eq!(
            menu_items_for(Some((LIST_CARD, 0)), true),
            vec![
                MenuItem::NewMap,
                MenuItem::NewDir,
                MenuItem::Rename,
                MenuItem::Delete
            ]
        );
        // map row (file or dir): delete
        assert_eq!(
            menu_items_for(Some((LIST_MAP, 2)), false),
            vec![
                MenuItem::NewMap,
                MenuItem::NewDir,
                MenuItem::Rename,
                MenuItem::Delete
            ]
        );
    }

    #[test]
    fn display_name_strips_extensions_and_shows_dir_names() {
        assert_eq!(display_name("cards/a.md"), "a");
        assert_eq!(display_name("maps/map.json"), "map");
        assert_eq!(display_name("cards/docs/"), "docs");
        assert_eq!(display_name("cards/docs/a.md"), "a");
        assert_eq!(display_name("cards/docs/nested/"), "nested");
        assert_eq!(display_name("maps/backup/old.json"), "old");
        assert_eq!(display_name("cards/.gitignore"), ".gitignore");
        assert_eq!(display_name(""), "");
    }

    #[test]
    fn row_icon_follows_type_and_expansion() {
        let file_map = Row {
            value: "maps/map.json".to_string(),
            depth: 0,
        };
        let file_card = Row {
            value: "cards/a.md".to_string(),
            depth: 0,
        };
        let dir = Row {
            value: "cards/docs/".to_string(),
            depth: 0,
        };
        let mut expanded = HashSet::new();
        assert_eq!(row_icon_svg(LIST_MAP, &file_map, &expanded), "map.svg");
        assert_eq!(row_icon_svg(LIST_CARD, &file_card, &expanded), "card.svg");
        assert_eq!(row_icon_svg(LIST_CARD, &dir, &expanded), "folder.svg");
        expanded.insert("cards/docs/".to_string());
        assert_eq!(row_icon_svg(LIST_CARD, &dir, &expanded), "folder-open.svg");
    }

    #[test]
    fn menu_item_index_hits_items_and_rejects_blank() {
        let menu = Rect {
            pos: dvec2(100.0, 200.0),
            size: dvec2(180.0, MENU_PAD * 2.0 + 3.0 * MENU_ITEM_H),
        };
        // 3 items of 32px inside 6px padding
        assert_eq!(menu_item_index(menu, 3, dvec2(150.0, 200.0 + MENU_PAD)), Some(0));
        assert_eq!(menu_item_index(menu, 3, dvec2(150.0, 200.0 + MENU_PAD + 32.0)), Some(1));
        assert_eq!(
            menu_item_index(menu, 3, dvec2(150.0, 200.0 + MENU_PAD + 64.0 + 1.0)),
            Some(2)
        );
        // padding strip between items
        assert_eq!(
            menu_item_index(menu, 3, dvec2(150.0, 200.0 + MENU_PAD + 32.0 - 0.5)),
            Some(0)
        );
        // below the last item, and outside the menu
        assert_eq!(
            menu_item_index(menu, 3, dvec2(150.0, 200.0 + MENU_PAD + 3.0 * 32.0 + 1.0)),
            None
        );
        assert_eq!(menu_item_index(menu, 3, dvec2(300.0, 220.0)), None);
        assert_eq!(menu_item_index(menu, 2, dvec2(150.0, 200.0 + MENU_PAD + 64.0)), None);
    }

    #[test]
    fn moved_path_appends_file_into_dir() {
        assert_eq!(
            moved_path("cards/docs/", "cards/a.md"),
            Some("cards/docs/a.md".to_string())
        );
        assert_eq!(
            moved_path("maps/backup/", "maps/map.json"),
            Some("maps/backup/map.json".to_string())
        );
        assert_eq!(moved_path("cards/docs/", ""), None);
    }
}
