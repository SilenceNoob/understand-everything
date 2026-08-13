use makepad_widgets::*;


use crate::file_panel::{EditKind, FilePanel, FilePanelAction};
use crate::file_panel::list::*;
use crate::slide_panel::{menu_item_index, menu_rect};


/// Context-menu items, in DSL order (NewMap, NewDir, Rename, Delete).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum MenuItem {
    NewMap,
    NewDir,
    Rename,
    Delete,
}

/// Context-menu items for a right-click: any row target adds 重命名 and 删除
/// (card files included); blank area keeps just 新建 map / 创建新目录.
pub(crate) fn menu_items_for(target: Option<(u8, usize)>, _target_is_dir: bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::NewMap, MenuItem::NewDir];
    if let Some((_, _)) = target {
        items.push(MenuItem::Rename);
        items.push(MenuItem::Delete);
    }
    items
}
impl FilePanel {
    /// Open the context menu at `abs`, clamped inside the panel. The target
    /// row (map or card list) enables 重命名; map rows and card dirs get 删除.
    /// The 创建新目录 item targets the pane the right-click happened in.
    pub(crate) fn open_menu(&mut self, cx: &mut Cx, abs: DVec2) {
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
        self.menu_rect = menu_rect(self.panel_rect, abs, self.menu_items.len());
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
            // 删除 exists for every row target (maps, card files and dirs).
            menu.view(cx, ids!(menu_delete_box))
                .set_visible(cx, self.menu_items.contains(&MenuItem::Delete));
        }
        // The item under the right-click lands pre-highlighted.
        self.menu_hover = menu_item_index(self.menu_rect, self.menu_items.len(), abs);
        self.menu_open = true;
        self.redraw(cx);
    }
}
impl FilePanel {
    /// Map a press inside the menu to its item (menu_items, DSL order).
    pub(crate) fn on_menu_press(&mut self, cx: &mut Cx, abs: DVec2) {
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
}
