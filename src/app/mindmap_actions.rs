use makepad_widgets::*;


use crate::card_picker::{CardPickerWidgetRefExt, PickChoice};
use crate::mindmap::MindMapWidgetRefExt;
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::App;

impl App {
    /// Card context menu: generate a section, start a quiz, or open the
    /// canvas card picker.
    pub(crate) fn handle_mindmap_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        if let Some((path, section)) = mind_map.generate_clicked(actions) {
            self.start_generation(cx, &path, section);
        }
        if let Some((parent, selected)) = mind_map.subcard_clicked(actions) {
            self.start_subcard_gen(cx, &parent, &selected);
        }
        if let Some(path) = mind_map.quiz_clicked(actions) {
            self.start_quiz(cx, &path);
        }
        if let Some(path) = mind_map.route_clicked(actions) {
            // The menu only offers planning on the root goal card; the goal
            // text is the card's file stem (minus a numeric order prefix).
            // Same as the startup path: run the diagnostic interview first.
            let goal = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .map(|s| crate::gen::strip_order_prefix(&s).to_string())
                .unwrap_or_default();
            if !goal.is_empty() {
                self.begin_diag(cx, &goal);
            }
        }
        if let Some(pos) = mind_map.canvas_menu_clicked(actions) {
            self.open_card_picker(cx, pos);
        }
    }

    /// Open the canvas card picker at `pos` (screen coords): scan cards/,
    /// exclude cards already on the map, and show the popup.
    pub(crate) fn open_card_picker(&mut self, cx: &mut Cx, pos: DVec2) {
        let base = crate::util::data_dir();
        let on_map = self.ui.mind_map(cx, ids!(mindmap)).card_rel_paths();
        let candidates: Vec<String> = crate::file_panel::all_card_files(&base)
            .into_iter()
            .filter(|p| !on_map.contains(p))
            .collect();
        self.open_picker_popup(cx);
        self.picker().open(cx, pos, &candidates);
    }

    pub(crate) fn picker(&self) -> crate::card_picker::CardPickerRef {
        self.popup_child(live_id!(picker_popup), &[live_id!(content)])
            .as_card_picker()
    }

    pub(crate) fn open_picker_popup(&self, cx: &mut Cx) {
        self.popup_widget(live_id!(picker_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(quiz_popup),
            live_id!(confirm_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
    }

    /// CardPicker popup: apply the choice (add existing / create new card).
    pub(crate) fn handle_picker_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let picker = self.picker();
        if picker.close_clicked(actions) {
            self.popup_widget(live_id!(picker_popup)).as_popup_panel().hide(cx);
            return;
        }
        let Some(choice) = picker.picked(actions) else {
            return;
        };
        self.popup_widget(live_id!(picker_popup)).as_popup_panel().hide(cx);
        let rel = match choice {
            PickChoice::Card(rel) => Some(rel),
            PickChoice::Create(name) => self.create_card_file(&name),
            PickChoice::None => None,
        };
        if let Some(rel) = rel {
            self.ui.mind_map(cx, ids!(mindmap)).add_card_at(cx, &rel);
        }
    }

    /// Create an empty card file in the default `cards/` dir from the search
    /// text (default "未命名" when empty); unique-ified with a numeric suffix
    /// when the name is taken. Returns the rel path.
    pub(crate) fn create_card_file(&self, name: &str) -> Option<String> {
        let stem = crate::file_panel::normalize_name(name, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let stem = stem.strip_suffix(".md").unwrap_or(&stem).to_string();
        let base = crate::util::data_dir();
        for n in 0.. {
            let fname = if n == 0 {
                format!("{stem}.md")
            } else {
                format!("{stem}-{n}.md")
            };
            let p = base.join("cards").join(&fname);
            if !p.exists() {
                std::fs::write(&p, "").ok()?;
                return Some(format!("cards/{fname}"));
            }
        }
        None
    }
}
