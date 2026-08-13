use makepad_widgets::*;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::SystemTime;

use crate::file_panel::FilePanel;
use crate::util::data_dir;


/// Row height (px).
pub(crate) const ROW_H: f64 = 30.0;
/// Directories backing the two lists: maps live in `maps/`, cards in `cards/`.
pub(crate) const MAPS_DIR: &str = "maps";
pub(crate) const CARDS_DIR: &str = "cards";
/// Inline-edit list ids: 0 = map list, 1 = card list.
pub(crate) const LIST_MAP: u8 = 0;
pub(crate) const LIST_CARD: u8 = 1;
/// Row tree indent per depth and the arrow strip that toggles expansion.
pub(crate) const INDENT: f64 = 16.0;
pub(crate) const ARROW_W: f64 = 24.0;
/// Trim `raw`; None when empty. Inputs without an extension get `default_ext`
/// appended (maps `.json`, cards `.md`, dirs None). Path separators (`/`,
/// `\`) are replaced with the full-width `／` so a name like
/// "附加/移除" can never create a nested directory (LLM card titles did).
pub(crate) fn normalize_name(raw: &str, default_ext: Option<&str>) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let s = s.replace(['/', '\\'], "／");
    if !s.contains('.') {
        if let Some(ext) = default_ext {
            return Some(format!("{s}{ext}"));
        }
    }
    Some(s)
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
pub(crate) fn row_icon_svg(list: u8, row: &Row, expanded: &HashSet<String>) -> &'static str {
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

/// True while an inline name edit (新建 map / 创建新目录) is active; the
/// mindmap skips its keyboard shortcuts so typing doesn't move the map.
/// Sort list rows: directories (trailing "/") first, then files, each group
/// by name.
pub(crate) fn sort_rows(names: &mut Vec<String>) {
    names.sort_by(|a, b| {
        let a_dir = a.ends_with('/');
        let b_dir = b.ends_with('/');
        b_dir.cmp(&a_dir).then_with(|| a.cmp(b))
    });
}

/// One visible row in a pane's tree: a rel path with its tree depth.
/// Dirs carry a trailing "/" (is_dir = value.ends_with('/')).
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Row {
    pub(crate) value: String,
    pub(crate) depth: usize,
}

impl Row {
    pub(crate) fn is_dir(&self) -> bool {
        self.value.ends_with('/')
    }
}

/// One-level listing of `rel` ("" for the pane root, "maps/docs" for a
/// subdir): children rel paths, dirs first, then files, by name. Files must
/// match `ext` when given (maps "json", cards "md").
pub(crate) fn scan_dir(base: &std::path::Path, rel: &str, ext: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(base.join(rel))
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| {
                    !e.file_name().to_string_lossy().starts_with('.')
                        && (e.path().is_dir()
                            || ext.is_none_or(|x| e.path().extension().is_some_and(|e| e == x)))
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
pub(crate) fn flatten(
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
                if e.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
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

/// All card body files (rel paths ending ".md") under cards/, recursively.
pub(crate) fn all_card_files(base: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![String::from("cards")];
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
                } else if rel.ends_with(".md") {
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
pub(crate) fn moved_path(dir: &str, from: &str) -> Option<String> {
    let name = from.rsplit('/').next().filter(|s| !s.is_empty())?;
    Some(format!("{dir}{name}"))
}

/// Row index under `abs` in a list of `rows` rows, or None when outside.
pub(crate) fn row_index_at(rows: usize, list: Rect, abs: DVec2, scroll: f64) -> Option<usize> {
    let row_i = ((abs.y - list.pos.y + scroll) / ROW_H).floor() as isize;
    if row_i < 0 || row_i as usize >= rows {
        return None;
    }
    Some(row_i as usize)
}

/// Clamp a list scroll offset by `dy` px; returns true when it moved.
pub(crate) fn scroll_rows(n: usize, list: Rect, dy: f64, scroll: &mut f64) -> bool {
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
pub(crate) fn draw_rows(
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
/// Svg bytes for a row icon (compile-time embedded; 4 icons).
pub(crate) fn icon_bytes(name: &'static str) -> Option<Arc<[u8]>> {
    let bytes: &'static [u8] = match name {
        "folder-open.svg" => include_bytes!("../../resources/folder-open.svg"),
        "folder.svg" => include_bytes!("../../resources/folder.svg"),
        "map.svg" => include_bytes!("../../resources/map.svg"),
        "card.svg" => include_bytes!("../../resources/card.svg"),
        _ => return None,
    };
    Some(Arc::from(bytes))
}

pub(crate) fn row_ref(
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
    if let Some(bytes) = icon_bytes(icon) {
        let _ = w
            .image(cx, ids!(row_icon))
            .load_svg_from_shared_data(cx, bytes);
    }
    refs.push(w.clone());
    w
}
impl FilePanel {
    /// The row list for a list id.
    pub(crate) fn rows(&self, list: u8) -> &[Row] {
        if list == LIST_MAP {
            &self.map_rows
        } else {
            &self.card_rows
        }
    }

    pub(crate) fn rows_mut(&mut self, list: u8) -> &mut Vec<Row> {
        if list == LIST_MAP {
            &mut self.map_rows
        } else {
            &mut self.card_rows
        }
    }

    /// The lazy row-widget refs for a list id.
    pub(crate) fn rows_refs(&self, list: u8) -> &[WidgetRef] {
        if list == LIST_MAP {
            &self.map_row_refs
        } else {
            &self.card_row_refs
        }
    }

    pub(crate) fn rows_refs_mut(&mut self, list: u8) -> &mut Vec<WidgetRef> {
        if list == LIST_MAP {
            &mut self.map_row_refs
        } else {
            &mut self.card_row_refs
        }
    }

    /// The list's viewport rect.
    pub(crate) fn list_rect(&self, list: u8) -> Rect {
        if list == LIST_MAP {
            self.map_list_rect
        } else {
            self.card_list_rect
        }
    }

    /// The list's scroll offset.
    pub(crate) fn list_scroll(&self, list: u8) -> f64 {
        if list == LIST_MAP {
            self.map_scroll
        } else {
            self.card_scroll
        }
    }

    pub(crate) fn list_scroll_mut(&mut self, list: u8) -> &mut f64 {
        if list == LIST_MAP {
            &mut self.map_scroll
        } else {
            &mut self.card_scroll
        }
    }

    /// The list's hit area.
    pub(crate) fn list_area(&self, list: u8) -> Area {
        if list == LIST_MAP {
            self.map_list_area
        } else {
            self.card_list_area
        }
    }

    /// Draw one list's rows (lazily creating row widgets), with the map list
    /// highlighting the current map.
    pub(crate) fn draw_list(&mut self, cx: &mut Cx2d, scope: &mut Scope, list: u8, rect: Rect) {
        let Some(t) = &self.row_template else {
            return;
        };
        let edit = self.edit_index(list);
        let (rows, refs, scroll, area, selected) = if list == LIST_MAP {
            (
                &self.map_rows[..],
                &mut self.map_row_refs,
                self.map_scroll,
                &mut self.map_list_area,
                self.current_map.as_deref(),
            )
        } else {
            (
                &self.card_rows[..],
                &mut self.card_row_refs,
                self.card_scroll,
                &mut self.card_list_area,
                None,
            )
        };
        draw_rows(
            cx,
            scope,
            list,
            rows,
            t,
            refs,
            rect,
            scroll,
            area,
            edit,
            &self.expanded,
            selected,
            &mut self.draw_sel_hl,
        );
    }
}
impl FilePanel {
    /// Rebuild the row lists when the pane roots or any expanded subdir
    /// change (cheap metadata stats per draw pass; scanning only on change).
    pub(crate) fn rebuild_rows(&mut self) {
        let base = data_dir();
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
    pub(crate) fn rebuild_now(&mut self) {
        let base = data_dir();
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
    pub(crate) fn toggle_expand(&mut self, cx: &mut Cx, list: u8, i: usize) {
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
}
impl FilePanel {
    /// The row value at (list, index), if in bounds.
    pub(crate) fn row_value(&self, list: u8, i: usize) -> Option<String> {
        self.rows(list).get(i).map(|r| r.value.clone())
    }

    /// (rows, viewport rect, scroll) for a list id.
    pub(crate) fn list_geometry(&self, list: u8) -> (&[Row], Rect, f64) {
        (self.rows(list), self.list_rect(list), self.list_scroll(list))
    }
}
