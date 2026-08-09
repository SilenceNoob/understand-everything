use makepad_widgets::*;

const PANEL_W: f64 = 360.0;
const SEARCH_H: f64 = 30.0;
const LIST_H: f64 = 320.0;
const PAD: f64 = 8.0;
const PANEL_H: f64 = PAD * 2.0 + SEARCH_H + PAD + LIST_H;

/// Choice resolved from the picker list, emitted to the App.
#[derive(Clone, Debug, Default)]
pub enum PickChoice {
    #[default]
    None,
    /// Add the existing card at the rel path (e.g. "cards/docs/foo.md").
    Card(String),
    /// Create a new card file with the given search text as its name.
    Create(String),
}

/// Action emitted by the CardPicker to the App.
#[derive(Clone, Debug, Default)]
pub enum CardPickerAction {
    #[default]
    None,
    Close,
    Pick(PickChoice),
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let PickerRow = mod.widgets.ButtonFlat{
        width: Fill
        height: (32.0)
        padding: Inset{left: 10, right: 10}
        draw_bg +: {
            color: #0000
            color_hover: #ffffff14
            color_down: #ffffff22
            color_focus: #0000
            border_radius: uniform(4.0)
        }
        draw_text +: {
            text_style: theme.font_regular{font_size: 13.0}
            color: #e6e9f0
        }
    }

    mod.widgets.CardPickerBase = #(CardPicker::register_widget(vm))

    mod.widgets.CardPicker = set_type_default() do mod.widgets.CardPickerBase{
        width: Fill
        height: Fill
        panel := mod.widgets.RoundedView{
            width: (360.0)
            height: (374.0)
            flow: Down
            padding: 8
            spacing: 8
            show_bg: true
            draw_bg +: {
                color: #2b3140
                border_radius: 6.0
                border_size: 1.0
                border_color: #ffffff3d
            }
            search := mod.widgets.TextInput{
                width: Fill
                height: (30.0)
                empty_text: "搜索卡片…"
                draw_bg +: {
                    color: #ffffff0a
                    border_radius: 4.0
                    border_size: 1.0
                    border_color: #ffffff26
                }
            }
            list := mod.widgets.PortalList{
                width: Fill
                height: (320.0)
                Row := PickerRow{}
            }
        }
    }
}

/// Candidates whose rel path contains `query` (case-insensitive). Empty query
/// returns all candidates, in their given order.
pub fn filter_cards(candidates: &[String], query: &str) -> Vec<String> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        candidates.to_vec()
    } else {
        candidates
            .iter()
            .filter(|p| p.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }
}

/// Row text for an existing card: rel path without the "cards/" prefix and
/// the ".md" extension ("cards/docs/foo.md" -> "docs/foo").
pub fn row_label(rel: &str) -> String {
    rel.strip_prefix("cards/")
        .unwrap_or(rel)
        .trim_end_matches(".md")
        .to_string()
}

/// Row text for the always-first "create new card" item.
pub fn create_label(query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        "＋ 创建新卡片".to_string()
    } else {
        format!("＋ 创建新卡片「{q}」")
    }
}

/// Wide searchable card picker: a search box over a virtualized list of card
/// files, with a "create new card" row pinned first. Drawn at the right-click
/// screen position as a popup content; the App hosts it in a PopupPanel.
#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct CardPicker {
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
    /// Window-wide capture area: a click outside the panel closes the picker.
    #[rust]
    backdrop_area: Area,
    /// Candidate card rel paths (already excluding cards on the map).
    #[rust]
    candidates: Vec<String>,
    #[rust]
    query: String,
    /// `candidates` filtered by `query` (row 0 is always "create").
    #[rust]
    filtered: Vec<String>,
    /// Raw right-click screen position; clamped into the window at draw.
    #[rust]
    panel_pos: DVec2,
    /// The drawn panel rect in screen coords (used for click-outside tests).
    #[rust]
    panel_rect: Rect,
    /// Give the search box key focus on the next draw after opening.
    #[rust]
    focus_pending: bool,
}

impl WidgetNode for CardPicker {
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

impl ScriptHook for CardPicker {}

impl Widget for CardPicker {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let window = Rect {
            pos: DVec2::default(),
            size: cx.current_pass_size(),
        };
        cx.add_aligned_rect_area(&mut self.backdrop_area, window);

        let panel_size = dvec2(PANEL_W, PANEL_H);
        let pos = dvec2(
            self.panel_pos.x.clamp(0.0, (window.size.x - panel_size.x).max(0.0)),
            self.panel_pos.y.clamp(0.0, (window.size.y - panel_size.y).max(0.0)),
        );
        if let Some(panel) = self.panel(cx) {
            let panel_walk = Walk {
                abs_pos: Some(pos),
                width: Size::Fixed(panel_size.x),
                height: Size::Fixed(panel_size.y),
                ..Walk::default()
            };
            while let Some(_item) = panel.draw_walk(cx, scope, panel_walk).step() {
                if let Some(mut list) = _item.borrow_mut::<PortalList>() {
                    let total = 1 + self.filtered.len();
                    list.set_item_range(cx, 0, total);
                    while let Some(idx) = list.next_visible_item(cx) {
                        if idx >= total {
                            continue;
                        }
                        let item = list.item(cx, idx, live_id!(Row));
                        if item.is_empty() {
                            continue;
                        }
                        let text = if idx == 0 {
                            create_label(&self.query)
                        } else {
                            row_label(&self.filtered[idx - 1])
                        };
                        item.as_button().set_text(cx, &text);
                        item.set_action_data(idx);
                        item.draw_all(cx, &mut Scope::empty());
                    }
                }
            }
        }
        self.panel_rect = Rect { pos, size: panel_size };

        if self.focus_pending {
            self.focus_pending = false;
            if let Some(panel) = self.panel(cx) {
                let search = panel.widget(cx, ids!(search));
                let a = search.area();
                if !a.is_empty() {
                    cx.set_key_focus(a);
                }
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(panel) = self.panel(cx) {
            panel.handle_event(cx, event, scope);
        }
        match event {
            Event::KeyDown(ke) if ke.key_code == KeyCode::Escape => {
                cx.widget_action(self.widget_uid(), CardPickerAction::Close);
            }
            Event::MouseDown(_) => {
                if let Hit::FingerDown(fe) = event.hits_with_capture_overload(cx, self.backdrop_area, true) {
                    if !self.panel_rect.contains(fe.abs) {
                        cx.widget_action(self.widget_uid(), CardPickerAction::Close);
                    }
                }
            }
            Event::Actions(actions) => {
                if let Some(panel) = self.panel(cx) {
                    let input = panel.widget(cx, ids!(search)).as_text_input();
                    if input.escaped(actions) {
                        cx.widget_action(self.widget_uid(), CardPickerAction::Close);
                        return;
                    }
                    if let Some((text, _)) = input.returned(actions) {
                        let choice = if self.filtered.is_empty() {
                            PickChoice::Create(text)
                        } else {
                            PickChoice::Card(self.filtered[0].clone())
                        };
                        cx.widget_action(self.widget_uid(), CardPickerAction::Pick(choice));
                        return;
                    }
                    if let Some(text) = input.changed(actions) {
                        self.query = text;
                        self.refilter();
                        self.redraw(cx);
                        return;
                    }
                }
                for action in actions.iter() {
                    let Some(wa) = action.downcast_ref::<WidgetAction>() else {
                        continue;
                    };
                    if !matches!(wa.action.downcast_ref::<ButtonAction>(), Some(ButtonAction::Clicked(_))) {
                        continue;
                    }
                    let Some(idx) = wa.data.as_ref().and_then(|d| d.downcast_ref::<usize>().copied()) else {
                        continue;
                    };
                    self.emit_pick(cx, idx);
                    return;
                }
            }
            _ => {}
        }
    }
}

impl CardPicker {
    fn panel(&self, cx: &Cx) -> Option<WidgetRef> {
        let p = self.view.widget(cx, ids!(panel));
        if p.is_empty() {
            None
        } else {
            Some(p)
        }
    }

    fn refilter(&mut self) {
        self.filtered = filter_cards(&self.candidates, &self.query);
    }

    fn emit_pick(&mut self, cx: &mut Cx, idx: usize) {
        let choice = if idx == 0 {
            PickChoice::Create(self.query.clone())
        } else if let Some(rel) = self.filtered.get(idx - 1) {
            PickChoice::Card(rel.clone())
        } else {
            PickChoice::None
        };
        if !matches!(choice, PickChoice::None) {
            cx.widget_action(self.widget_uid(), CardPickerAction::Pick(choice));
        }
    }
}

impl CardPickerRef {
    /// Open with the candidate card rel paths; the search box starts empty
    /// and the panel is anchored at `pos` (screen coords).
    pub fn open(&self, cx: &mut Cx, pos: DVec2, candidates: &[String]) {
        if let Some(mut w) = self.borrow_mut() {
            w.candidates = candidates.to_vec();
            w.query = String::new();
            w.refilter();
            w.panel_pos = pos;
            w.focus_pending = true;
            w.redraw(cx);
        }
    }

    /// True when the user closed the picker (Esc, click outside).
    pub fn close_clicked(&self, actions: &Actions) -> bool {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let CardPickerAction::Close = item.cast() {
                return true;
            }
        }
        false
    }

    /// The picker's choice, if any was made this action cycle.
    pub fn picked(&self, actions: &Actions) -> Option<PickChoice> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let CardPickerAction::Pick(c) = item.cast() {
                return Some(c);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_empty_query_returns_all() {
        let all = vec!["cards/a.md".to_string(), "cards/docs/b.md".to_string()];
        assert_eq!(filter_cards(&all, ""), all);
        assert_eq!(filter_cards(&all, "   "), all);
    }

    #[test]
    fn filter_matches_case_insensitive_substring() {
        let all = vec![
            "cards/Rust.md".to_string(),
            "cards/docs/神经网络.md".to_string(),
            "cards/rusty.md".to_string(),
        ];
        assert_eq!(
            filter_cards(&all, "rust"),
            vec!["cards/Rust.md".to_string(), "cards/rusty.md".to_string()]
        );
        assert_eq!(
            filter_cards(&all, "神经"),
            vec!["cards/docs/神经网络.md".to_string()]
        );
        assert_eq!(filter_cards(&all, "zzz"), Vec::<String>::new());
    }

    #[test]
    fn row_label_strips_prefix_and_extension() {
        assert_eq!(row_label("cards/docs/foo.md"), "docs/foo");
        assert_eq!(row_label("cards/bar.md"), "bar");
    }

    #[test]
    fn create_label_shows_query() {
        assert_eq!(create_label(""), "＋ 创建新卡片");
        assert_eq!(create_label("  深度  "), "＋ 创建新卡片「深度」");
    }
}
