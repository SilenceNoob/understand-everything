use makepad_widgets::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::mindmap::app_base_dir;

const TAB_W: f64 = 14.0;
const TAB_H: f64 = 48.0;
/// Exponential ease rate (1/s); settles in ~0.2s (mirrors FilePanel).
const SLIDE_EASE: f64 = 14.0;
/// Default panel width and drag limits (px), same as the file panel.
const PANEL_W_DEFAULT: f64 = 260.0;
const PANEL_W_MIN: f64 = 140.0;
const PANEL_W_MAX: f64 = 520.0;
/// Width-grab strip on the panel's left edge: 8px inside the panel,
/// 4px straddling the edge (total 12px).
const EDGE_W: f64 = 12.0;
const EDGE_INSET: f64 = 8.0;
/// Fixed pane header height (px) — the DSL header is exactly this tall.
const PANE_HEADER_H: f64 = 32.0;
/// Bottom button bar height (px).
const BAR_H: f64 = 44.0;
/// Minimum row height (px) as a fallback when a row's laid-out height reads
/// as zero; rows are otherwise sized by their real rendered content.
const ROW_MIN: f64 = 40.0;
/// Context-menu geometry.
const MENU_W: f64 = 220.0;
const MENU_ITEM_H: f64 = 32.0;
const MENU_PAD: f64 = 6.0;
/// Excerpt length cap (chars) for file snippets and link descriptions.
const EXCERPT_MAX: usize = 200;

/// Panel body rect in window coords, written every draw pass; the mindmap
/// reads it to keep wheel zoom and marquee selection off the panel.
pub(crate) static PANEL_RECT_RIGHT: Mutex<Option<Rect>> = Mutex::new(None);

/// True while the URL input row is active; the mindmap skips its keyboard
/// shortcuts so typing doesn't move the map.
static URL_EDITING: AtomicBool = AtomicBool::new(false);

pub fn is_url_editing() -> bool {
    URL_EDITING.load(Ordering::Relaxed)
}

/// What a reference item is: a local Markdown document (absolute path) or a
/// web link (URL).
#[derive(Clone, Copy, PartialEq, Debug)]
enum RefKind {
    File,
    Link,
}

/// One reference item. `value` is an absolute file path or a URL; `desc` is
/// the file excerpt or the parsed page description. `pending` marks a link
/// whose description is still being fetched (never persisted).
#[derive(Clone, PartialEq, Debug)]
struct RefItem {
    kind: RefKind,
    value: String,
    desc: String,
    pending: bool,
}

impl RefItem {
    /// Display title: file name for documents, the URL itself for links.
    fn name(&self) -> String {
        match self.kind {
            RefKind::File => std::path::Path::new(&self.value)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.value.clone()),
            RefKind::Link => self.value.clone(),
        }
    }

    fn new(kind: RefKind, value: String) -> Self {
        Self {
            kind,
            value,
            desc: String::new(),
            pending: false,
        }
    }
}

/// Per-map reference list, persisted as refs/<map rel>.json. File excerpts
/// are re-derived from disk on load (the file is the source); link
/// descriptions are fetched once and persisted here.
#[derive(Serialize, Deserialize)]
struct RefsFile {
    files: Vec<FileRef>,
    links: Vec<LinkRef>,
}

#[derive(Serialize, Deserialize)]
struct FileRef {
    path: String,
    desc: String,
}

#[derive(Serialize, Deserialize)]
struct LinkRef {
    url: String,
    desc: String,
}

/// The pre-card format ("files": ["path"]); kept readable so old refs files
/// still load (descriptions start empty).
#[derive(Deserialize)]
struct LegacyRefsFile {
    files: Vec<String>,
    links: Vec<String>,
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
    if let Ok(data) = serde_json::from_str::<RefsFile>(&json) {
        let mut items: Vec<RefItem> = data
            .files
            .into_iter()
            .map(|f| RefItem {
                // fresh excerpt from disk beats the stored one
                desc: file_excerpt(&f.path).unwrap_or(f.desc),
                ..RefItem::new(RefKind::File, f.path)
            })
            .collect();
        items.extend(data.links.into_iter().map(|l| RefItem {
            desc: l.desc,
            ..RefItem::new(RefKind::Link, l.url)
        }));
        return items;
    }
    // pre-card format: plain string lists
    if let Ok(old) = serde_json::from_str::<LegacyRefsFile>(&json) {
        let mut items: Vec<RefItem> = old
            .files
            .into_iter()
            .map(|p| RefItem {
                desc: file_excerpt(&p).unwrap_or_default(),
                ..RefItem::new(RefKind::File, p)
            })
            .collect();
        items.extend(
            old.links
                .into_iter()
                .map(|u| RefItem::new(RefKind::Link, u)),
        );
        return items;
    }
    Vec::new()
}

fn save_items(map_rel: &str, items: &[RefItem]) {
    let mut files = Vec::new();
    let mut links = Vec::new();
    for it in items {
        match it.kind {
            RefKind::File => files.push(FileRef {
                path: it.value.clone(),
                desc: String::new(),
            }),
            RefKind::Link => links.push(LinkRef {
                url: it.value.clone(),
                desc: it.desc.clone(),
            }),
        }
    }
    let path = refs_path(map_rel);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(&RefsFile { files, links }) {
        let _ = std::fs::write(path, json);
    }
}

/// First snippet of a markdown file: strip heading/bullet markers and
/// collapse whitespace, cut at a word boundary.
fn file_excerpt(path: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(excerpt(&text, EXCERPT_MAX))
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

/// Decode the few HTML entities that actually show up in descriptions.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Remove `<...>` tags (attribute values containing '>' are rare enough to
/// ignore here).
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// The value of a `name="value"` attribute (either quote style).
fn attr_value(tag: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(pos) = tag.find(&needle) {
            let rest = &tag[pos + needle.len()..];
            let end = rest.find(quote)?;
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// The first <p> paragraph's text, if any.
fn first_paragraph(html: &str) -> Option<String> {
    let start = html.find("<p")?;
    let rest = &html[start..];
    let end = rest.find("</p>")?;
    Some(strip_tags(&rest[..end]))
}

/// Parse a page description out of HTML: og:description or meta description,
/// falling back to the first paragraph. None when nothing usable is found.
fn extract_meta_description(html: &str) -> Option<String> {
    for (i, _) in html.match_indices("<meta").take(200) {
        let tail = &html[i..];
        let end = tail.find('>').map(|e| i + e).unwrap_or(html.len().min(i + 512));
        let tag = &html[i..end];
        let lower = tag.to_lowercase();
        let is_desc = lower.contains("og:description")
            || lower.contains("name=\"description\"")
            || lower.contains("name='description'");
        if !is_desc {
            continue;
        }
        if let Some(content) = attr_value(tag, "content") {
            let text = excerpt(&decode_entities(&strip_tags(&content)), EXCERPT_MAX);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    first_paragraph(html).map(|t| excerpt(&decode_entities(&t), EXCERPT_MAX))
}

/// Resolve a redirect `Location` header against the previous URL: absolute
/// URLs pass through, "//host/path" keeps the scheme, everything else is
/// resolved against the base's host. None for empty/unresolvable targets.
fn resolve_url(base: &str, location: &str) -> Option<String> {
    let loc = location.trim();
    if loc.is_empty() {
        return None;
    }
    if loc.starts_with("http://") || loc.starts_with("https://") {
        return Some(loc.to_string());
    }
    let (proto, rest) = base.split_once("://")?;
    let host = rest.split('/').next().unwrap_or(rest);
    if let Some(l) = loc.strip_prefix("//") {
        return Some(format!("{proto}://{l}"));
    }
    let path = loc.trim_start_matches('/');
    Some(format!("{proto}://{host}/{path}"))
}

/// Normalize a URL for storage: trim, and prepend https:// when no scheme is
/// given. Returns None for empty input.
fn normalize_url(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if !s.contains("://") {
        return Some(format!("https://{s}"));
    }
    Some(s.to_string())
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
/// its type icon (link for URLs, doc for files).
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
    let desc = if item.pending {
        "获取中…".to_string()
    } else {
        item.desc.clone()
    };
    w.label(cx, ids!(row_desc)).set_text(cx, &desc);
    let icon = match item.kind {
        RefKind::File => "card.svg",
        RefKind::Link => "link.svg",
    };
    let icon_path = app_base_dir().join("resources").join(icon);
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
                color: #1f2430
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
        // Bottom bar: add a document or a link.
        bottom_bar := mod.widgets.View{
            width: Fill
            height: (44.0)
            flow: Right
            spacing: 8
            padding: Inset{left: 8, right: 8, top: 8, bottom: 8}
            add_doc_btn := mod.widgets.ButtonFlat{
                width: Fill
                height: Fill
                text: "添加文档"
                draw_text.text_style.font_size: 12.0
            }
            add_link_btn := mod.widgets.ButtonFlat{
                width: Fill
                height: Fill
                text: "添加链接"
                draw_text.text_style.font_size: 12.0
            }
        }
        // Tab on the panel's left edge (mirror of the file panel's tab).
        tab := mod.widgets.ButtonFlat{
            text: "◀"
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

    #[rust(false)]
    opened: bool,
    /// 0 = collapsed off the right edge, 1 = fully open; eases toward the
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
    /// Window-wide area that captures all presses while the menu is open or
    /// the URL input row is active.
    #[rust]
    modal_area: Area,

    /// While Some, row 0 shows the URL input (a placeholder was inserted).
    #[rust]
    url_edit: bool,
    /// The items before the URL placeholder was inserted (for cancel).
    #[rust]
    edit_snapshot: Vec<RefItem>,
    #[rust]
    edit_focus_pending: bool,
    /// In-flight link fetches: request id → (item index, redirect hops so
    /// far). Responses arrive via RefsPanelRef::apply_link_fetch, forwarded
    /// by App; 3xx redirects are followed manually (makepad's HTTP client
    /// does not).
    #[rust]
    pending_links: HashMap<LiveId, (usize, u8)>,
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
        self.window_size = cx.current_pass_size();
        let body_y = cx.turtle().rect().pos.y; // body top, window coords
        let body_h = (self.window_size.y - body_y).max(0.0);
        let panel = Rect {
            pos: dvec2(self.window_size.x - self.panel_w * self.slide, body_y),
            size: dvec2(self.panel_w, body_h),
        };
        self.panel_rect = panel;
        self.tab_rect = Rect {
            pos: dvec2(panel.pos.x - TAB_W, body_y + body_h * 0.5 - TAB_H * 0.5),
            size: dvec2(TAB_W, TAB_H),
        };
        self.edge_rect = Rect {
            pos: dvec2(panel.pos.x - (EDGE_W - EDGE_INSET), panel.pos.y),
            size: dvec2(EDGE_W, panel.size.y),
        };
        PANEL_RECT_RIGHT.lock().unwrap().replace(panel);

        // Default to the default map until App::open_map sets one (mirrors the
        // mindmap, which loads maps/map.json on its own).
        if self.current_map.is_none() {
            self.current_map = Some(crate::mindmap::MindMapData::DEFAULT_MAP.to_string());
            self.items = load_items(crate::mindmap::MindMapData::DEFAULT_MAP);
            self.refetch_empty_links(cx.cx);
        }

        cx.begin_turtle(self.walk, self.layout);
        if let Some(content) = self.content_widget(cx) {
            // Same clip-rect trick as FilePanel/FloatPanel: the root turtle's
            // clip is disabled (0-size walk), so push a real clip so draw_clip
            // data and hit-testing resolve to the panel rect.
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
        // Context menu on top of everything; hover highlight under the cursor.
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
        // Focus the URL input once its row has been drawn.
        if self.edit_focus_pending {
            if let Some(w) = self.row_refs.get(0) {
                let input = w.text_input(cx, ids!(row_edit));
                if input.area().is_valid(cx) {
                    cx.set_key_focus(input.area());
                    self.edit_focus_pending = false;
                }
            }
        }
        // While the menu is open or the URL input row is active, a
        // window-wide modal area captures every press.
        if self.menu_open || self.url_edit {
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
        // The URL input must see events itself to process keystrokes (IME);
        // row widgets are not forwarded otherwise.
        if self.url_edit {
            if let Some(w) = self.row_refs.get(0).cloned() {
                w.handle_event(cx, event, scope);
            }
        }
        // Modal state (context menu / URL input) grabs every press first, so
        // the list/tab/edge below can't fire behind it.
        if self.menu_open || self.url_edit {
            match event.hits_with_capture_overload(cx, self.modal_area, true) {
                Hit::FingerDown(fe) if fe.is_primary_hit() => {
                    if self.menu_open {
                        if self.menu_rect.contains(fe.abs) {
                            self.on_menu_press(cx, fe.abs);
                        } else {
                            self.menu_open = false;
                            self.redraw(cx);
                        }
                    } else if self.url_edit {
                        let row_h = self.row_heights.first().copied().unwrap_or(ROW_MIN);
                        let row_rect = Rect {
                            pos: self.list_rect.pos - dvec2(0.0, self.scroll),
                            size: dvec2(self.list_rect.size.x, row_h),
                        };
                        if row_rect.contains(fe.abs) {
                            self.edit_focus_pending = true;
                            self.redraw(cx);
                        } else {
                            self.cancel_url_edit(cx);
                        }
                    }
                }
                _ => {}
            }
        }
        // URL edit: Enter confirms, Esc cancels.
        if self.url_edit {
            if let Event::KeyDown(ke) = event {
                match ke.key_code {
                    KeyCode::ReturnKey => self.confirm_url_edit(cx),
                    KeyCode::Escape => self.cancel_url_edit(cx),
                    _ => {}
                }
            }
        }
        // Button actions fire from the bottom bar's own event handling.
        if let Event::Actions(actions) = event {
            let bar = self.view.widget(cx, ids!(bottom_bar));
            if bar.button(cx, ids!(add_doc_btn)).clicked(actions) {
                self.pick_doc(cx);
            }
            if bar.button(cx, ids!(add_link_btn)).clicked(actions) {
                self.start_url_edit(cx);
            }
        }
        // capture_overload: widgets above us (float panels) may mark the
        // press handled first; plain hits would then skip our areas.
        match event.hits_with_capture_overload(cx, self.tab_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && !self.url_edit => {
                self.toggle(cx);
            }
            _ => {}
        }
        // Left-edge drag to resize the panel width.
        match event.hits_with_capture_overload(cx, self.edge_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && !self.url_edit => {
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
        if let Event::MouseMove(e) = event {
            if !self.panel_w_dragging && self.edge_rect.contains(e.abs) {
                cx.set_cursor(MouseCursor::ColResize);
            }
            if self.menu_open {
                let hover = self.menu_item_index(e.abs);
                if hover != self.menu_hover {
                    self.menu_hover = hover;
                    self.redraw(cx);
                }
            }
        }
        match event.hits_with_capture_overload(cx, self.list_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && !self.url_edit && !self.menu_open => {
                if let Some(i) = row_index_at(&self.row_heights, self.list_rect, fe.abs, self.scroll)
                {
                    self.menu_row = Some(i);
                }
            }
            Hit::FingerScroll(fe) => {
                if !self.url_edit
                    && scroll_rows(&self.row_heights, self.list_rect, fe.scroll.y, &mut self.scroll)
                {
                    self.redraw(cx);
                }
            }
            _ => {}
        }
        // Right-click anywhere on the panel opens (or repositions) the menu.
        match event.hits_with_capture_overload(cx, self.panel_area, true) {
            Hit::FingerDown(fe)
                if matches!(fe.device, DigitDevice::Mouse { button } if button.is_secondary())
                    && !self.url_edit && !self.menu_open =>
            {
                self.open_menu(cx, fe.abs);
            }
            _ => {}
        }
        // Claim the press over the panel body so it never reaches the canvas.
        let _ = event.hits_with_capture_overload(cx, self.panel_area, true);
    }
}

impl RefsPanel {
    fn content_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.content_ref.is_none() {
            self.content_ref = Some(self.view.widget(cx, ids!(content)));
        }
        self.content_ref.clone()
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
            let fresh = self.row_refs.get(i).is_none();
            let w = row_ref(cx, template, i, item, &mut self.row_refs);
            if self.url_edit && i == 0 {
                w.view(cx, ids!(row_text_box)).set_visible(cx, false);
                w.view(cx, ids!(row_edit_box)).set_visible(cx, true);
                if fresh {
                    w.text_input(cx, ids!(row_edit)).set_text(cx, "");
                }
            } else {
                w.view(cx, ids!(row_text_box)).set_visible(cx, true);
                w.view(cx, ids!(row_edit_box)).set_visible(cx, false);
            }
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

    /// Ease `slide` toward its target on each timer tick (mirrors the
    /// mindmap's zoom animation pattern).
    fn handle_slide_anim(&mut self, cx: &mut Cx, event: &Event) {
        let Some(timer) = self.slide_timer else { return };
        let Some(te) = timer.is_event(event) else { return };
        let now = te.time.unwrap_or(0.0);
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
        self.menu_open = false;
        if self.url_edit {
            self.cancel_url_edit(cx);
        }
        if let Some(tab) = self.tab_widget(cx) {
            tab.set_text(cx, if self.opened { "▶" } else { "◀" });
        }
        if self.slide_timer.is_none() {
            self.slide_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_timer_time = 0.0;
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
    /// (absolute path) to the current map's list.
    fn pick_doc(&mut self, cx: &mut Cx) {
        let path = rfd::FileDialog::new()
            .set_directory(app_base_dir())
            .add_filter("Markdown", &["md", "markdown"])
            .add_filter("所有文件", &["*"])
            .pick_file();
        let Some(path) = path else {
            return;
        };
        let path_str = path.to_string_lossy().into_owned();
        let mut item = RefItem::new(RefKind::File, path_str);
        item.desc = file_excerpt(&item.value).unwrap_or_default();
        self.items.push(item);
        self.row_refs.clear();
        self.save();
        self.redraw(cx);
    }

    /// Insert the URL input placeholder row at the top of the list.
    fn start_url_edit(&mut self, cx: &mut Cx) {
        if self.url_edit {
            return;
        }
        self.edit_snapshot = self.items.clone();
        self.items.insert(0, RefItem::new(RefKind::Link, String::new()));
        self.url_edit = true;
        self.edit_focus_pending = true;
        self.row_refs.clear();
        // The placeholder sits at the top; make sure it is on screen.
        self.scroll = 0.0;
        URL_EDITING.store(true, Ordering::Relaxed);
        self.redraw(cx);
    }

    /// Cancel the URL edit, restoring the pre-edit list.
    fn cancel_url_edit(&mut self, cx: &mut Cx) {
        if !self.url_edit {
            return;
        }
        self.url_edit = false;
        URL_EDITING.store(false, Ordering::Relaxed);
        self.drop_edit_focus(cx);
        self.items = std::mem::take(&mut self.edit_snapshot);
        self.row_refs.clear();
        self.redraw(cx);
    }

    /// Read the URL input; on success append the link and stop editing.
    fn confirm_url_edit(&mut self, cx: &mut Cx) {
        if !self.url_edit {
            return;
        }
        let Some(w) = self.row_refs.get(0).cloned() else {
            return;
        };
        let raw = w.text_input(cx, ids!(row_edit)).text();
        let Some(url) = normalize_url(&raw) else {
            // keep editing on empty input (mirrors FilePanel::confirm_edit)
            return;
        };
        self.items[0] = RefItem {
            desc: String::new(),
            pending: true,
            ..RefItem::new(RefKind::Link, url)
        };
        self.url_edit = false;
        URL_EDITING.store(false, Ordering::Relaxed);
        self.drop_edit_focus(cx);
        self.row_refs.clear();
        self.save();
        self.fetch_link_desc(cx, 0);
        self.redraw(cx);
    }

    /// Start fetching the description of the link at index `i` (async; the
    /// response arrives via App::handle_http_response → apply_link_fetch).
    fn fetch_link_desc(&mut self, cx: &mut Cx, i: usize) {
        let Some(item) = self.items.get(i) else {
            return;
        };
        if item.kind != RefKind::Link || item.value.is_empty() {
            return;
        }
        let request_id = LiveId::unique();
        self.pending_links.insert(request_id, (i, 0));
        let mut http = HttpRequest::new(item.value.clone(), HttpMethod::GET);
        http.set_header(
            "User-Agent".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)".to_string(),
        );
        http.set_header("Accept".to_string(), "text/html,application/xhtml+xml".to_string());
        cx.http_request(request_id, http);
    }

    /// Kick off fetches for every link whose description is empty (fresh
    /// loads, legacy-format items, earlier failed attempts). No-op while
    /// other fetches are still in flight. This also heals refs files written
    /// by older versions: the next save rewrites them in the current format.
    fn refetch_empty_links(&mut self, cx: &mut Cx) {
        if !self.pending_links.is_empty() {
            return;
        }
        let idxs: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.kind == RefKind::Link && it.desc.is_empty())
            .map(|(i, _)| i)
            .collect();
        if idxs.is_empty() {
            return;
        }
        for i in &idxs {
            self.items[*i].pending = true;
            self.fetch_link_desc(cx, *i);
        }
        self.row_refs.clear();
        self.redraw(cx);
    }

    fn drop_edit_focus(&self, cx: &mut Cx) {
        if let Some(w) = self.row_refs.get(0) {
            let input = w.text_input(cx, ids!(row_edit));
            if input.area() == cx.key_focus() {
                cx.set_key_focus(Area::Empty);
            }
        }
    }

    /// Open the context menu at `abs`, clamped inside the panel.
    fn open_menu(&mut self, cx: &mut Cx, abs: DVec2) {
        if self.url_edit {
            return;
        }
        self.menu_row = row_index_at(&self.row_heights, self.list_rect, abs, self.scroll);
        let panel = self.panel_rect;
        let h = MENU_PAD * 2.0 + MENU_ITEM_H;
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
        self.menu_hover = self.menu_item_index(abs);
        self.menu_open = true;
        self.redraw(cx);
    }

    fn on_menu_press(&mut self, cx: &mut Cx, abs: DVec2) {
        let idx = self.menu_item_index(abs);
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

    /// The single menu item's index under `abs` (None outside the item).
    fn menu_item_index(&self, abs: DVec2) -> Option<usize> {
        if !self.menu_rect.contains(abs) {
            return None;
        }
        let idx = ((abs.y - self.menu_rect.pos.y - MENU_PAD) / MENU_ITEM_H).floor() as isize;
        if idx == 0 {
            Some(0)
        } else {
            None
        }
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
                // stale requests from the previous map are dropped
                w.pending_links.clear();
                w.row_refs.clear();
                w.refetch_empty_links(cx);
                w.redraw(cx);
            }
        }
    }

    /// Apply a fetched link page (forwarded by App::handle_http_response).
    /// 3xx redirects are followed (up to 5 hops), 2xx bodies are parsed;
    /// anything else leaves the description empty.
    pub fn apply_link_fetch(&self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        let Some(mut w) = self.borrow_mut() else {
            return;
        };
        let Some(&(i, hops)) = w.pending_links.get(&request_id) else {
            return;
        };
        let abort = |w: &mut RefsPanel, cx: &mut Cx, rid: LiveId| {
            w.pending_links.remove(&rid);
            for item in &mut w.items {
                item.pending = false;
            }
            w.row_refs.clear();
            w.redraw(cx);
        };
        if (300..400).contains(&response.status_code) {
            let location = response
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("location"))
                .and_then(|(_, v)| v.first())
                .map(|s| s.to_string());
            let Some(location) = location else {
                abort(&mut w, cx, request_id);
                return;
            };
            if hops >= 5 {
                abort(&mut w, cx, request_id);
                return;
            }
            let Some(item) = w.items.get(i).cloned() else {
                return;
            };
            let Some(url) = resolve_url(&item.value, &location) else {
                abort(&mut w, cx, request_id);
                return;
            };
            let new_id = LiveId::unique();
            w.pending_links.remove(&request_id);
            w.pending_links.insert(new_id, (i, hops + 1));
            let mut http = HttpRequest::new(url, HttpMethod::GET);
            http.set_header(
                "User-Agent".to_string(),
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)"
                    .to_string(),
            );
            http.set_header(
                "Accept".to_string(),
                "text/html,application/xhtml+xml".to_string(),
            );
            cx.http_request(new_id, http);
            return;
        }
        w.pending_links.remove(&request_id);
        if let Some(item) = w.items.get_mut(i) {
            item.pending = false;
            if (200..300).contains(&response.status_code) {
                if let Some(body) = response.body() {
                    // lossy: pages with stray non-UTF-8 bytes (or GBK) must
                    // not drop the whole description
                    let html = String::from_utf8_lossy(body);
                    if let Some(desc) = extract_meta_description(&html) {
                        item.desc = desc;
                    }
                }
            }
        }
        w.row_refs.clear();
        w.save();
        w.redraw(cx);
    }

    /// The link fetch failed (forwarded by App::handle_http_request_error);
    /// the row just drops the "获取中…" placeholder.
    pub fn link_fetch_error(&self, cx: &mut Cx, request_id: LiveId) {
        if let Some(mut w) = self.borrow_mut() {
            if !w.pending_links.contains_key(&request_id) {
                return;
            }
            w.pending_links.remove(&request_id);
            for item in &mut w.items {
                item.pending = false;
            }
            w.row_refs.clear();
            w.redraw(cx);
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
    fn normalize_url_adds_scheme() {
        assert_eq!(normalize_url("example.com"), Some("https://example.com".into()));
        assert_eq!(
            normalize_url("http://example.com/a"),
            Some("http://example.com/a".into())
        );
        assert_eq!(normalize_url("  "), None);
    }

    #[test]
    fn resolve_redirect_location() {
        // absolute
        assert_eq!(
            resolve_url("https://a.com/x", "http://b.com/y"),
            Some("http://b.com/y".into())
        );
        // scheme-relative
        assert_eq!(
            resolve_url("https://a.com/x", "//b.com/y"),
            Some("https://b.com/y".into())
        );
        // path-relative
        assert_eq!(
            resolve_url("https://a.com/dir/page", "/wiki"),
            Some("https://a.com/wiki".into())
        );
        assert_eq!(
            resolve_url("https://a.com/x", "wiki"),
            Some("https://a.com/wiki".into())
        );
        // unusable
        assert_eq!(resolve_url("https://a.com/x", ""), None);
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
    fn extracts_og_description() {
        let html = r#"<html><head><meta property="og:description" content="  An og &amp; nice description  "><meta name="description" content="fallback"></head></html>"#;
        assert_eq!(
            extract_meta_description(html).as_deref(),
            Some("An og & nice description")
        );
    }

    #[test]
    fn meta_attr_order_does_not_matter() {
        let html = r#"<meta content="c-desc" name="description">"#;
        assert_eq!(extract_meta_description(html).as_deref(), Some("c-desc"));
    }

    #[test]
    fn falls_back_to_first_paragraph() {
        let html = r#"<html><body><p>Hello <b>world</b>, this is the first paragraph.</p><p>second</p></body></html>"#;
        assert_eq!(
            extract_meta_description(html).as_deref(),
            Some("Hello world, this is the first paragraph.")
        );
    }

    #[test]
    fn legacy_refs_json_still_parses() {
        let old = r#"{"files":["/a/b.md"],"links":["https://x.com"]}"#;
        let data: LegacyRefsFile = serde_json::from_str(old).unwrap();
        assert_eq!(data.files, vec!["/a/b.md"]);
        assert_eq!(data.links, vec!["https://x.com"]);
    }

    #[test]
    fn refs_json_roundtrip_keeps_link_desc() {
        let items = vec![
            RefItem {
                desc: "excerpt".into(),
                ..RefItem::new(RefKind::File, "/a/b.md".to_string())
            },
            RefItem {
                desc: "page desc".into(),
                ..RefItem::new(RefKind::Link, "https://x.com".to_string())
            },
        ];
        let json = serde_json::to_string(&RefsFile {
            files: items
                .iter()
                .filter(|i| i.kind == RefKind::File)
                .map(|i| FileRef { path: i.value.clone(), desc: String::new() })
                .collect(),
            links: items
                .iter()
                .filter(|i| i.kind == RefKind::Link)
                .map(|i| LinkRef { url: i.value.clone(), desc: i.desc.clone() })
                .collect(),
        })
        .unwrap();
        let data: RefsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(data.files[0].path, "/a/b.md");
        assert_eq!(data.links[0].url, "https://x.com");
        assert_eq!(data.links[0].desc, "page desc");
    }
}
