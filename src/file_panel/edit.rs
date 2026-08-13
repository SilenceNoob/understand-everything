use makepad_widgets::*;


use crate::file_panel::{EditKind, FilePanel, FilePanelAction};
use crate::file_panel::list::*;
use crate::util::data_dir;


pub(crate) static NAME_EDITING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_name_editing() -> bool {
    NAME_EDITING.load(std::sync::atomic::Ordering::Relaxed)
}

impl FilePanel {
    /// Start an inline name edit: NewMap appends a placeholder to the map
    /// list, NewDir heads the target list, Rename edits the right-clicked row
    /// in place (the input seeds with its current display name).
    pub(crate) fn start_edit(&mut self, cx: &mut Cx, list: u8, kind: EditKind) {
        let index = match kind {
            EditKind::NewMap => self.map_rows.len(),
            EditKind::NewDir => 0,
            EditKind::Rename => self.menu_row.map(|(_, i)| i).unwrap_or(0),
        };
        self.edit_snapshot = self.rows(list).to_vec();
        if kind != EditKind::Rename {
            self.rows_mut(list).insert(
                index,
                Row {
                    value: String::new(),
                    depth: 0,
                },
            );
        }
        self.editing = Some((list, index, kind));
        self.edit_focus_pending = true;
        self.rows_refs_mut(list).clear();
        NAME_EDITING.store(true, std::sync::atomic::Ordering::Relaxed);
        self.redraw(cx);
    }

    /// Restore the pre-edit list and stop editing.
    pub(crate) fn cancel_edit(&mut self, cx: &mut Cx) {
        let Some((list, _, _)) = self.editing else {
            return;
        };
        self.editing = None;
        NAME_EDITING.store(false, std::sync::atomic::Ordering::Relaxed);
        self.drop_edit_focus(cx, list);
        *self.rows_mut(list) = std::mem::take(&mut self.edit_snapshot);
        self.rows_refs_mut(list).clear();
        self.redraw(cx);
    }

    /// Release key focus from the edit input so the map gets keys back.
    pub(crate) fn drop_edit_focus(&self, cx: &mut Cx, list: u8) {
        let Some(i) = self.edit_index(list) else {
            return;
        };
        if let Some(w) = self.rows_refs(list).get(i) {
            let input = w.text_input(cx, ids!(row_edit));
            if input.area() == cx.key_focus() {
                cx.set_key_focus(Area::Empty);
            }
        }
    }

    /// Read the inline name input; on success fire CreateMap/CreateDir/
    /// RenameFile and stop editing. Empty input and existing targets keep
    /// the edit going.
    pub(crate) fn confirm_edit(&mut self, cx: &mut Cx) {
        let Some((list, i, kind)) = self.editing else {
            return;
        };
        let Some(w) = self.rows_refs(list).get(i).cloned() else {
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
        let base = data_dir();
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
        let depth = self.rows(list).get(i).map(|r| r.depth).unwrap_or(0);
        self.rows_mut(list)[i] = Row {
            value: to.clone(),
            depth,
        };
        self.rows_refs_mut(list).clear();
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
}
