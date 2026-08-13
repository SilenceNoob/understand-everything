use makepad_widgets::*;

mod drag;
mod edit;
mod list;
mod menu;

#[allow(unused_imports)]
pub(crate) use crate::file_panel::drag::{
    PANE_HEADER_H, PanelGeo, panel_geometry, panel_w_from_x, split_from_y,
};
pub(crate) use crate::file_panel::edit::is_name_editing;
#[allow(unused_imports)]
pub(crate) use crate::file_panel::list::{
    all_card_files, all_map_files, display_name, flatten, moved_path, normalize_name, row_icon_svg,
    scan_dir, Row, LIST_CARD, LIST_MAP, ROW_H,
};
#[allow(unused_imports)]
pub(crate) use crate::file_panel::menu::{MenuItem, menu_items_for};


use std::collections::HashSet;
use std::time::SystemTime;

use crate::slide_panel::{MENU_ITEM_H, MENU_PAD, SlideState};
use crate::util::{cached_widget, set_panel_rect};

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



/// The menu item index under `abs`, or None outside the items (blank strip,
/// padding, out of the menu).


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

    /// Slide-in/out animation state (shared with the refs panel).
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
pub(crate) enum EditKind {
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
        let geo = panel_geometry(self.slide.progress, self.split, self.panel_w, self.window_size, body_y);
        self.panel_rect = geo.panel;
        self.tab_rect = geo.tab;
        self.splitter_rect = geo.splitter;
        self.edge_rect = geo.edge;
        set_panel_rect(self.uid.0, Some(self.panel_rect));

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
            self.draw_list(cx, scope, LIST_MAP, map_list);
            let card_list = Rect {
                pos: panel.pos + dvec2(0.0, a_h + 1.0 + PANE_HEADER_H),
                size: dvec2(panel.size.x, (b_h - PANE_HEADER_H).max(0.0)),
            };
            self.card_list_rect = card_list;
            self.draw_list(cx, scope, LIST_CARD, card_list);
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
                if let Some(w) = self.rows_refs(list).get(i) {
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
            if let Some(w) = self.rows_refs(list).get(i).cloned() {
                w.handle_event(cx, event, scope);
            }
        }
        self.handle_modal_events(cx, event);
        // capture_overload: the mindmap canvas (earlier in tree order, covering
        // the whole body) hits first and marks t.handled, which makes plain
        // hits() skip our areas entirely (same trick as FloatPanel).
        match event.hits_with_capture_overload(cx, self.tab_area, true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && self.editing.is_none() => {
                self.toggle(cx);
            }
            _ => {}
        }
        self.handle_edge_drag(cx, event);
        self.handle_splitter_drag(cx, event);
        self.handle_mouse_move(cx, event);
        self.handle_list_events(cx, event, LIST_MAP);
        self.handle_list_events(cx, event, LIST_CARD);
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
        cached_widget(&mut self.content_ref, || self.view.widget(cx, ids!(content)))
    }

    fn canvas_pane_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.canvas_pane_ref, || self.view.widget(cx, ids!(canvas_pane)))
    }

    fn card_pane_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.card_pane_ref, || self.view.widget(cx, ids!(card_pane)))
    }

    fn tab_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.tab_ref, || self.view.widget(cx, ids!(tab)))
    }

    fn ctx_menu_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        cached_widget(&mut self.ctx_menu_ref, || self.view.widget(cx, ids!(ctx_menu)))
    }

    /// The list being edited: (rect, scroll) for the given list id.
    fn edit_geometry(&self, list: u8) -> (Rect, f64) {
        (self.list_rect(list), self.list_scroll(list))
    }

    /// The row index being edited in `list`, if any.
    fn edit_index(&self, list: u8) -> Option<usize> {
        match self.editing {
            Some((l, i, _)) if l == list => Some(i),
            _ => None,
        }
    }

    fn toggle(&mut self, cx: &mut Cx) {
        self.slide.toggle(cx);
        self.menu_open = false;
        if self.editing.is_some() {
            self.cancel_edit(cx);
        }
        if let Some(tab) = self.tab_widget(cx) {
            tab.set_text(cx, if self.slide.opened { "◀" } else { "▶" });
        }
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
    /// A card row was dragged and released; carries (card rel path, screen
    /// position of the release). The App hit-tests the canvas and adds it.
    DropCard(String, DVec2),
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

    /// A card row drag that ended without a dir drop target: (card rel path,
    /// release position). The App decides whether it landed on the canvas.
    pub fn card_dropped(&self, actions: &Actions) -> Option<(String, DVec2)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let FilePanelAction::DropCard(rel, pos) = item.cast() {
                return Some((rel, pos));
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::slide_panel::menu_item_index;

    #[test]
    fn geometry_open_collapsed_and_clamped_tab() {
        let window = dvec2(1440.0, 900.0);
        let h = 866.0 * crate::util::SIDE_PANEL_H_FRAC;
        let y = 34.0 + (866.0 - h) * 0.5;
        // open: panel is 95% of the body height, centered, 8px off the left
        // edge; the tab straddles the panel's right edge (centering keeps it
        // at the body center)
        let geo = panel_geometry(1.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.panel, Rect { pos: dvec2(8.0, y), size: dvec2(260.0, h) });
        assert_eq!(geo.tab, Rect { pos: dvec2(268.0, 443.0), size: dvec2(14.0, 48.0) });
        // collapsed: panel fully off-screen (right edge at x=0), tab parked
        // 8px off the edge
        let geo = panel_geometry(0.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.panel, Rect { pos: dvec2(-260.0, y), size: dvec2(260.0, h) });
        assert_eq!(geo.tab, Rect { pos: dvec2(8.0, 443.0), size: dvec2(14.0, 48.0) });
        // half-open: tab tracks the panel edge
        let geo = panel_geometry(0.5, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.panel.pos.x, -126.0);
        // window resize shrinks the panel height
        let geo = panel_geometry(1.0, 0.5, 260.0, dvec2(800.0, 600.0), 34.0);
        assert_eq!(geo.panel.size.y, 566.0 * crate::util::SIDE_PANEL_H_FRAC);
        // custom width moves the right edge and the tab with it
        let geo = panel_geometry(1.0, 0.5, 360.0, window, 34.0);
        assert_eq!(geo.panel.size.x, 360.0);
        assert_eq!(geo.tab.pos.x, 368.0);
    }

    #[test]
    fn splitter_strip_tracks_split_and_drag_clamps() {
        let window = dvec2(1440.0, 900.0);
        let geo = panel_geometry(1.0, 0.5, 260.0, window, 34.0);
        let panel = geo.panel;
        // strip (12px grab + 3px margins) centered on the divider line
        assert_eq!(geo.splitter, Rect { pos: dvec2(8.0, 458.0), size: dvec2(260.0, 18.0) });
        // dragging the strip center keeps the ratio
        let center = geo.splitter.pos.y + geo.splitter.size.y * 0.5;
        assert!((split_from_y(center, panel, 60.0) - 0.5).abs() < 1e-9);
        // extremes clamp so both sections keep >= 60px
        assert_eq!(split_from_y(panel.pos.y + 6.0, panel, 60.0), 60.0 / panel.size.y);
        assert_eq!(
            split_from_y(panel.pos.y + panel.size.y, panel, 60.0),
            1.0 - 60.0 / panel.size.y
        );
        // collapsed panel: strip slides off-screen with it
        let geo = panel_geometry(0.0, 0.5, 260.0, window, 34.0);
        assert_eq!(geo.splitter.pos.x, -260.0);
    }

    #[test]
    fn edge_strip_and_width_clamp() {
        let window = dvec2(1440.0, 900.0);
        let h = 866.0 * crate::util::SIDE_PANEL_H_FRAC;
        // edge strip hugs the panel's right edge (8px inside, 4px overhang)
        let geo = panel_geometry(1.0, 0.5, 260.0, window, 34.0);
        assert_eq!(
            geo.edge,
            Rect { pos: dvec2(260.0, 34.0 + (866.0 - h) * 0.5), size: dvec2(12.0, h) }
        );
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
        std::fs::write(dir.join("cards/docs/a.md"), "x").unwrap();
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
        // recursive card find walks subdirs and ignores non-md files
        assert_eq!(
            all_card_files(&dir),
            vec!["cards/a.md", "cards/b.md", "cards/docs/a.md"]
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
    fn normalize_name_replaces_path_separators() {
        assert_eq!(
            normalize_name("实体与组件的关联（附加/移除）", Some(".md")),
            Some("实体与组件的关联（附加／移除）.md".to_string())
        );
        assert_eq!(
            normalize_name("a\\b", Some(".md")),
            Some("a／b.md".to_string())
        );
        let name = normalize_name("实体与组件的关联（附加/移除）", Some(".md")).unwrap();
        // A separator-free name joins into a single file, never a nested dir.
        let p = std::path::Path::new(&name);
        assert_eq!(p.file_name(), Some(std::ffi::OsStr::new("实体与组件的关联（附加／移除）.md")));
        assert_eq!(p.parent(), Some(std::path::Path::new("")));
    }

    #[test]
    fn menu_items_follow_target_row() {
        assert_eq!(menu_items_for(None, false), vec![MenuItem::NewMap, MenuItem::NewDir]);
        // card file: rename + delete
        assert_eq!(
            menu_items_for(Some((LIST_CARD, 0)), false),
            vec![
                MenuItem::NewMap,
                MenuItem::NewDir,
                MenuItem::Rename,
                MenuItem::Delete
            ]
        );
        // card dir: rename + delete
        assert_eq!(
            menu_items_for(Some((LIST_CARD, 0)), true),
            vec![
                MenuItem::NewMap,
                MenuItem::NewDir,
                MenuItem::Rename,
                MenuItem::Delete
            ]
        );
        // map row (file or dir): rename + delete
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
