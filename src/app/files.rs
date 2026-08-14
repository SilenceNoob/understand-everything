use makepad_widgets::*;


use crate::ai::{self, AIConfig};
use crate::app::show_toast;
use crate::file_panel::{self, FilePanelWidgetRefExt};
use crate::mindmap::{self, MindMapWidgetRefExt};
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::refs_panel::RefsPanelWidgetRefExt;
use crate::App;


/// True for the launch placeholder map (maps/.startup-*.json).
pub(crate) fn is_temp_map(map_file: &str) -> bool {
    map_file.starts_with("maps/.startup-")
}
impl App {
    /// File panel tree: map click, context-menu create/delete/rename.
    pub(crate) fn handle_file_panel_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        // File panel tree: clicking a map switches the mindmap to it.
        if let Some(map_file) = self.ui.file_panel(cx, ids!(file_panel)).map_clicked(actions) {
            self.open_map(cx, &map_file);
        }
        // Context menu: create map / dir, delete map, rename.
        let base = crate::util::data_dir();
        if let Some(map_file) = self.ui.file_panel(cx, ids!(file_panel)).create_map(actions) {
            std::fs::write(base.join(&map_file), crate::mindmap::new_map_json()).ok();
            self.open_map(cx, &map_file);
        }
        if let Some(dir) = self.ui.file_panel(cx, ids!(file_panel)).create_dir(actions) {
            std::fs::create_dir(base.join(&dir)).ok();
        }
        if let Some(rel) = self.ui.file_panel(cx, ids!(file_panel)).delete_entry(actions) {
            let mind_map = self.ui.mind_map(cx, ids!(mindmap));
            if rel.ends_with('/') {
                // Directory: maps/ dirs are deletable outright; a cards/
                // dir also drops the referencing nodes from every map.
                std::fs::remove_dir_all(base.join(&rel)).ok();
                if rel.starts_with("cards/") {
                    crate::mindmap::remove_dir_nodes(&base, &rel);
                    // Drop ghost cards from the in-memory map so a later
                    // save can't resurrect the references.
                    mind_map.reload_map(cx);
                    self.sync_startup(cx);
                }
                // The current map may live inside the deleted dir.
                if mind_map
                    .current_map_file()
                    .is_some_and(|c| c == rel || c.starts_with(&rel))
                {
                    self.open_map(cx, &self.next_map(&base));
                }
            } else if rel.starts_with("cards/") {
                // Card file: confirm first (the dialog lists using maps).
                self.open_card_delete_confirm(cx, &rel);
            } else {
                std::fs::remove_file(base.join(&rel)).ok();
                if mind_map.current_map_file().as_deref() == Some(rel.as_str()) {
                    // Switch to the first remaining map; none left → the
                    // default, whose failed load empties the canvas.
                    self.open_map(cx, &self.next_map(&base));
                }
            }
        }
        if let Some((from, to)) = self.ui.file_panel(cx, ids!(file_panel)).rename_file(actions) {
            if std::fs::rename(base.join(&from), base.join(&to)).is_ok() {
                let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                // Renaming a card/dir breaks map references and progress
                // keys; rewrite both and reload the current map so its
                // in-memory node paths follow (else the next save_map()
                // writes the stale paths back).
                if from.starts_with("cards/") {
                    crate::mindmap::rewrite_node_paths(&base, &from, &to);
                    crate::mindmap::rewrite_progress_paths(&base, &from, &to);
                    mind_map.reload_map(cx);
                    mind_map.reload_progress(cx);
                }
                // Renaming the current map: keep showing it under the new
                // name (content is unchanged, the saved view survives).
                if mind_map.current_map_file().as_deref() == Some(from.as_str()) {
                    self.open_map(cx, &to);
                }
            }
        }
    }

    /// The first remaining map under maps/ (the default when none is left;
    /// a failed load of it empties the canvas).
    pub(crate) fn next_map(&self, base: &std::path::Path) -> String {
        file_panel::all_map_files(base)
            .into_iter()
            .next()
            .unwrap_or_else(|| mindmap::MindMapData::DEFAULT_MAP.to_string())
    }

    /// Open the delete-confirm popup for the card at `rel`, listing every
    /// map that references it.
    pub(crate) fn open_card_delete_confirm(&mut self, cx: &mut Cx, rel: &str) {
        self.pending_delete_card = Some(rel.to_string());
        self.pending_remove_root = None;
        let base = crate::util::data_dir();
        let name = std::path::Path::new(rel)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| rel.to_string());
        let maps = crate::mindmap::maps_using_card(&base, rel);
        let usage = if maps.is_empty() {
            "该卡片没有被任何 map 使用。".to_string()
        } else {
            let list = maps
                .iter()
                .map(|m| format!("• {}", file_panel::display_name(m)))
                .collect::<Vec<_>>()
                .join("\n");
            format!("该卡片被以下 {} 个 map 使用：\n{list}", maps.len())
        };
        let child = |path: &[LiveId]| self.popup_child(live_id!(confirm_popup), path);
        child(&[live_id!(content), live_id!(panel), live_id!(title)]).set_text(cx, "删除卡片");
        child(&[live_id!(content), live_id!(panel), live_id!(card_name)]).set_text(cx, &name);
        child(&[live_id!(content), live_id!(panel), live_id!(usage)]).set_text(cx, &usage);
        child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(delete_btn)])
            .set_text(cx, "删除");
        self.popup_widget(live_id!(confirm_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(quiz_popup),
            live_id!(picker_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
    }

    /// Open the confirm popup for removing a root card and its whole subtree
    /// from the current map (card files stay on disk).
    pub(crate) fn open_remove_root_confirm(&mut self, cx: &mut Cx, rel: &str, title: &str, children: usize) {
        self.pending_delete_card = None;
        self.pending_remove_root = Some((rel.to_string(), title.to_string(), children));
        let child = |path: &[LiveId]| self.popup_child(live_id!(confirm_popup), path);
        child(&[live_id!(content), live_id!(panel), live_id!(title)]).set_text(cx, "移除根卡片");
        child(&[live_id!(content), live_id!(panel), live_id!(card_name)]).set_text(cx, title);
        child(&[live_id!(content), live_id!(panel), live_id!(usage)]).set_text(
            cx,
            &format!(
                "该根卡片下还有 {children} 张卡片（整条学习路线）。\n移除后它们将一起从当前 map 中移除，卡片文件保留在 cards/ 中。"
            ),
        );
        child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(delete_btn)])
            .set_text(cx, "移除");
        self.popup_widget(live_id!(confirm_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(quiz_popup),
            live_id!(picker_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
    }

    /// Card delete-confirm popup buttons (file deletion + root-subtree removal
    /// share the popup; the pending state picks the action).
    pub(crate) fn handle_card_delete_confirm(&mut self, cx: &mut Cx, actions: &Actions) {
        let popup = live_id!(confirm_popup);
        // Field-based closure (only borrows self.ui) so the pending-state
        // mutations below can coexist with it.
        let child = |path: &[LiveId]| crate::app::popup_child(&self.ui, popup, path);
        if child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(cancel_btn)])
            .as_button()
            .clicked(actions)
        {
            self.pending_delete_card = None;
            self.pending_remove_root = None;
            self.popup_widget(popup).as_popup_panel().hide(cx);
            return;
        }
        if child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(delete_btn)])
            .as_button()
            .clicked(actions)
        {
            // Root card + subtree removal (manual-wiring relaxation).
            if let Some((rel, title, count)) = self.pending_remove_root.take() {
                self.pending_delete_card = None;
                self.popup_widget(popup).as_popup_panel().hide(cx);
                child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(delete_btn)])
                    .set_text(cx, "删除");
                self.ui.mind_map(cx, ids!(mindmap)).remove_root_subtree(cx, &rel);
                show_toast(
                    &self.ui,
                    &mut self.toast_until,
                    cx,
                    &format!("已从 map 中移除根卡片「{title}」及其 {count} 张卡片。"),
                );
                self.sync_startup(cx);
                return;
            }
            let rel = self.pending_delete_card.take();
            self.popup_widget(popup).as_popup_panel().hide(cx);
            let Some(rel) = rel else { return };
            let base = crate::util::data_dir();
            crate::mindmap::remove_card_node(&base, &rel);
            std::fs::remove_file(base.join(&rel)).ok();
            // Drop the ghost node from the in-memory map (if present) so a
            // later save can't resurrect it; RAG and the file panel follow
            // via their own mtime/fingerprint watchers.
            self.ui.mind_map(cx, ids!(mindmap)).reload_map(cx);
            self.sync_startup(cx);
        }
    }

    /// Drop a card dragged from the file panel onto the canvas.
    pub(crate) fn handle_card_drop(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some((rel, abs)) = self.ui.file_panel(cx, ids!(file_panel)).card_dropped(actions) {
            self.ui.mind_map(cx, ids!(mindmap)).drop_card_at(cx, &rel, abs);
        }
    }

    /// Current config as typed in the settings form (empty base_url/model
    /// fall back to the DeepSeek defaults).
    pub(crate) fn form_config(&self, _cx: &Cx) -> AIConfig {
        let child = |path: &[LiveId]| self.popup_child(live_id!(setting_popup), path);
        let mut cfg = self.ai_config.clone();
        cfg.api_key = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(key_row),
            live_id!(key_input),
        ])
        .as_text_input()
        .text();
        let base_url = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(url_row),
            live_id!(url_input),
        ])
        .as_text_input()
        .text();
        let model = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(model_row),
            live_id!(model_input),
        ])
        .as_text_input()
        .text();
        if !base_url.trim().is_empty() {
            cfg.base_url = base_url.trim().to_string();
        }
        if !model.trim().is_empty() {
            cfg.model = model.trim().to_string();
        }
        cfg.thinking = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(thinking_row),
            live_id!(thinking_input),
        ])
        .as_drop_down()
        .selected_label();
        cfg
    }

    /// The naming request landed: rename the launch temp map to the model's
    /// name (goal text on failure/error — the map must become permanent the
    /// moment the user committed to a goal).
    pub(crate) fn handle_map_name_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        let ai_name = if response.status_code == 200 {
            ai::response_content(response).map(|s| s.trim().to_string())
        } else {
            None
        };
        self.apply_map_name(cx, ai_name);
    }

    pub(crate) fn handle_map_name_error(&mut self, cx: &mut Cx) {
        self.apply_map_name(cx, None);
    }

    /// Rename the current temp map to `ai_name` (or a goal-derived fallback),
    /// unless the user already switched away (the temp file is then gone).
    pub(crate) fn apply_map_name(&mut self, cx: &mut Cx, ai_name: Option<String>) {
        self.diag.map_name_id = LiveId::empty();
        let goal = std::mem::take(&mut self.diag.map_name_goal);
        let raw = match ai_name.filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => goal.chars().take(20).collect::<String>(),
        };
        let safe = crate::file_panel::normalize_name(&raw, None)
            .map(|s| s.split('.').next().unwrap_or("").trim().to_string())
            .unwrap_or_default();
        if safe.is_empty() {
            return;
        }
        let Some(cur) = self.ui.mind_map(cx, ids!(mindmap)).current_map_file() else {
            return;
        };
        if !is_temp_map(&cur) {
            return;
        }
        let base = crate::util::data_dir();
        for n in 0.. {
            let target = if n == 0 {
                format!("maps/{safe}.json")
            } else {
                format!("maps/{safe}-{n}.json")
            };
            if !base.join(&target).exists() {
                if std::fs::rename(base.join(&cur), base.join(&target)).is_ok() {
                    self.switch_map_state(cx, &target);
                    self.sync_title(cx);
                }
                return;
            }
        }
    }

    pub(crate) fn open_map(&mut self, cx: &mut Cx, map_file: &str) {
        // The launch temp map dies the moment the user switches away; the
        // startup goal input renames it into a permanent map instead.
        let current = self.ui.mind_map(cx, ids!(mindmap)).current_map_file();
        if let Some(cur) = current {
            if is_temp_map(&cur) && cur != map_file {
                std::fs::remove_file(crate::util::data_dir().join(&cur)).ok();
            }
        }
        self.switch_map_state(cx, map_file);
        self.map_opened = true;
        self.sync_title(cx);
        self.sync_startup(cx);
    }

    /// Point the mindmap, file/refs panels and RAG at `map_file` without
    /// open_map's side effects (the rename path must not re-sync the startup
    /// popup, which would reset an in-flight diagnostic back to the goal).
    pub(crate) fn switch_map_state(&mut self, cx: &mut Cx, map_file: &str) {
        self.ui.mind_map(cx, ids!(mindmap)).switch_map(cx, map_file);
        self.ui
            .file_panel(cx, ids!(file_panel))
            .set_current_map(cx, Some(map_file));
        self.ui
            .refs_panel(cx, ids!(refs_panel))
            .set_current_map(cx, Some(map_file));
        if let Some(rag) = &self.rag {
            rag.set_map(map_file);
        }
    }

    /// Show the startup page iff the current map has no root card; close it
    /// otherwise.
    pub(crate) fn sync_startup(&mut self, cx: &mut Cx) {
        if self.ui.mind_map(cx, ids!(mindmap)).has_root() {
            self.close_startup(cx);
        } else {
            self.show_startup(cx);
        }
    }

    pub(crate) fn sync_title(&mut self, cx: &mut Cx) {
        let title = if self.map_opened {
            self.ui
                .mind_map(cx, ids!(mindmap))
                .current_map_file()
                .map(|f| {
                    if is_temp_map(&f) {
                        String::new()
                    } else {
                        file_panel::display_name(&f)
                    }
                })
                .unwrap_or_else(|| "Understand Everything".to_string())
        } else {
            "Understand Everything".to_string()
        };
        self.ui.label(cx, ids!(caption_label.label)).set_text(cx, &title);
    }
}
