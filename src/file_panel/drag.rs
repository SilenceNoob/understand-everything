use makepad_widgets::*;


use crate::file_panel::{FilePanel, FilePanelAction};
use crate::file_panel::list::*;
use crate::slide_panel::menu_item_index;


pub(crate) const TAB_W: f64 = 14.0;
pub(crate) const TAB_H: f64 = 48.0;
/// Splitter bar height and the grab margin around it.
pub(crate) const SPLITTER_BAR: f64 = 12.0;
pub(crate) const SPLITTER_MARGIN: f64 = 3.0;
/// Minimum height (px) each section keeps when dragging the divider.
pub(crate) const SPLIT_MIN: f64 = 60.0;
/// Panel width drag limits (px).
pub(crate) const PANEL_W_MIN: f64 = 140.0;
pub(crate) const PANEL_W_MAX: f64 = 520.0;
/// Width-grab strip on the panel's right edge: 8px inside the panel,
/// 4px straddling the edge (total 12px).
pub(crate) const EDGE_W: f64 = 12.0;
pub(crate) const EDGE_INSET: f64 = 8.0;
/// Fixed pane header height (px) — the DSL headers are exactly this tall, so
/// the row lists below them land on known rects.
pub(crate) const PANE_HEADER_H: f64 = 32.0;
/// Min pointer travel (px) before a row press becomes a drag.
pub(crate) const DRAG_THRESHOLD: f64 = 6.0;
impl FilePanel {
    /// Modal state (context menu / inline name edit) grabs every press
    /// first, so the lists/tab/divider below can't fire behind it. Enter
    /// confirms and Esc cancels an inline name edit.
    pub(crate) fn handle_modal_events(&mut self, cx: &mut Cx, event: &Event) {
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
        if self.editing.is_some() {
            if let Event::KeyDown(ke) = event {
                match ke.key_code {
                    KeyCode::ReturnKey => self.confirm_edit(cx),
                    KeyCode::Escape => self.cancel_edit(cx),
                    _ => {}
                }
            }
        }
    }

    /// Right-edge drag to resize the panel width. capture_overload (the
    /// mindmap canvas shadows plain hits, same as FloatPanel) on the edge
    /// strip; checked before the splitter so grabbing the corner of the
    /// strip resizes the width.
    pub(crate) fn handle_edge_drag(&mut self, cx: &mut Cx, event: &Event) {
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
    }

    /// Divider drag to resize the two panes.
    pub(crate) fn handle_splitter_drag(&mut self, cx: &mut Cx, event: &Event) {
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
    }

    /// Hover cursors over the edge/splitter and the context-menu highlight
    /// (tracked from the raw cursor, redraw only on change).
    pub(crate) fn handle_mouse_move(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::MouseMove(e) = event {
            if !self.panel_w_dragging && !self.split_dragging {
                if self.edge_rect.contains(e.abs) {
                    cx.set_cursor(MouseCursor::ColResize);
                } else if self.splitter_rect.contains(e.abs) {
                    cx.set_cursor(MouseCursor::RowResize);
                } else {
                    cx.set_cursor(MouseCursor::Default);
                }
            }
            if self.menu_open {
                let hover = menu_item_index(self.menu_rect, self.menu_items.len(), e.abs);
                if hover != self.menu_hover {
                    self.menu_hover = hover;
                    self.redraw(cx);
                }
            }
        }
    }

    /// One list's events: press a file row to drag it into a maps/ dir, or
    /// click to switch the map (fired on FingerUp so a drag never switches);
    /// dir rows toggle expansion on their arrow strip; wheel scrolls the
    /// list (Scroll bypasses the handled flag).
    pub(crate) fn handle_list_events(&mut self, cx: &mut Cx, event: &Event, list: u8) {
        let rows_len = self.rows(list).len();
        let (list_rect, scroll) = self.edit_geometry(list);
        let list_area = self.list_area(list);
        match event.hits_with_capture_overload(cx, list_area, true) {
            Hit::FingerDown(fe)
                if fe.is_primary_hit() && self.editing.is_none() && !self.menu_open =>
            {
                if let Some(i) = row_index_at(rows_len, list_rect, fe.abs, scroll) {
                    if !self.rows(list)[i].is_dir() {
                        // file rows start a drag (or a click on FingerUp)
                        self.drag_press = Some((list, i, fe.abs));
                    } else {
                        // dir rows: the arrow strip toggles expansion
                        let indent_x = list_rect.pos.x + self.rows(list)[i].depth as f64 * INDENT;
                        if fe.abs.x < indent_x + ARROW_W {
                            self.toggle_expand(cx, list, i);
                        }
                    }
                }
            }
            Hit::FingerMove(fe) => {
                self.track_drag(cx, fe.abs, list);
            }
            Hit::FingerUp(fe) => {
                self.finish_drag(cx, list, fe.abs);
            }
            Hit::FingerScroll(fe) => {
                if scroll_rows(rows_len, list_rect, fe.scroll.y, self.list_scroll_mut(list)) {
                    self.redraw(cx);
                }
            }
            _ => {}
        }
    }
}
impl FilePanel {
    /// Ease the slide animation on its timer tick.
    pub(crate) fn handle_slide_anim(&mut self, cx: &mut Cx, event: &Event) {
        if self.slide.handle_event(cx, event) {
            self.redraw(cx);
        }
    }
}
impl FilePanel {
    /// Update the drag for a press in `list`: activate past the threshold and
    /// track the hovered dir row (drop targets are dirs of the same list, so
    /// map files can never land in card dirs and vice versa). While dragging
    /// a card, publish the drop-ghost state for the canvas to render.
    pub(crate) fn track_drag(&mut self, cx: &mut Cx, abs: DVec2, list: u8) {
        let Some((l, i, start)) = self.drag_press else {
            return;
        };
        if l != list {
            return;
        }
        if !self.drag_active && (abs - start).length() >= DRAG_THRESHOLD {
            self.drag_active = true;
            if list == LIST_CARD {
                let title = self
                    .row_value(list, i)
                    .as_deref()
                    .map(display_name)
                    .unwrap_or_default();
                crate::util::set_card_drag(Some(crate::util::CardDrag { title, pos: abs }));
            }
        } else if self.drag_active && list == LIST_CARD {
            // Keep the drop ghost glued to the pointer.
            if let Some(mut drag) = crate::util::card_drag() {
                drag.pos = abs;
                crate::util::set_card_drag(Some(drag));
            }
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
    /// RenameFile), release a dragged card on the canvas (DropCard), or,
    /// without drag, treat it as a click (map rows switch).
    pub(crate) fn finish_drag(&mut self, cx: &mut Cx, list: u8, up_abs: DVec2) {
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
        crate::util::set_card_drag(None);
        if let Some(to) = to {
            if let Some(from) = from {
                cx.widget_action(self.widget_uid(), FilePanelAction::RenameFile(from, to));
            }
        } else if dragged && list == LIST_CARD {
            if let Some(from) = from {
                cx.widget_action(self.widget_uid(), FilePanelAction::DropCard(from, up_abs));
            }
        } else if !dragged && list == LIST_MAP {
            if let Some(from) = from {
                cx.widget_action(self.widget_uid(), FilePanelAction::MapClicked(from));
            }
        }
        self.redraw(cx);
    }
}
impl FilePanel {
    pub(crate) fn apply_split(&mut self, cx: &mut Cx, abs_y: f64) {
        self.split = split_from_y(abs_y, self.panel_rect, SPLIT_MIN);
        self.redraw(cx);
    }

    pub(crate) fn apply_width(&mut self, cx: &mut Cx, abs_x: f64) {
        self.panel_w = panel_w_from_x(abs_x, self.panel_rect, PANEL_W_MIN, PANEL_W_MAX);
        self.redraw(cx);
    }
}
/// Panel geometry in window coords: body, tab, divider strip and width-grab
/// edge. Pure so it is unit-testable.
pub(crate) struct PanelGeo {
    pub(crate) panel: Rect,
    pub(crate) tab: Rect,
    pub(crate) splitter: Rect,
    pub(crate) edge: Rect,
}

/// Panel body, tab, divider-strip and edge rects for a given slide progress
/// (0 = collapsed off the left edge, 1 = fully open), split fraction and
/// panel width.
pub(crate) fn panel_geometry(slide: f64, split: f64, panel_w: f64, window: DVec2) -> PanelGeo {
    // 95% of the window height, centered vertically.
    let panel_h = window.y * crate::util::SIDE_PANEL_H_FRAC;
    let y_off = (window.y - panel_h) * 0.5;
    // GAP keeps the open panel off the window edge; the interpolation
    // (GAP+panel_w)*slide - panel_w puts the collapsed panel fully outside
    // (right edge flush with x=0) so no sliver shows while the tab stays
    // parked at the gap.
    let offset_x = (crate::util::SIDE_PANEL_GAP + panel_w) * slide - panel_w;
    let panel = Rect {
        pos: dvec2(offset_x, y_off),
        size: dvec2(panel_w, panel_h),
    };
    // Tab protrudes fully outside the panel, flush against its right edge;
    // when collapsed it pins to the gap from the left edge (x = GAP).
    let tab_x = (panel.pos.x + panel.size.x).max(crate::util::SIDE_PANEL_GAP);
    let tab = Rect {
        pos: dvec2(tab_x, panel.pos.y + panel.size.y * 0.5 - TAB_H * 0.5),
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
pub(crate) fn panel_w_from_x(abs_x: f64, panel: Rect, min: f64, max: f64) -> f64 {
    (abs_x - panel.pos.x).clamp(min, max)
}

/// Divider fraction from a window-absolute y, clamped so both sections keep
/// at least `min_px`. The line follows the cursor, so no bar-half offset.
pub(crate) fn split_from_y(abs_y: f64, panel: Rect, min_px: f64) -> f64 {
    let h = panel.size.y;
    if h <= 0.0 {
        return 0.5;
    }
    let frac = (abs_y - panel.pos.y) / h;
    let min = (min_px / h).clamp(0.0, 0.5);
    frac.clamp(min, 1.0 - min)
}

