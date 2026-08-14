
use crate::mindmap::*;
use crate::util::data_dir;


impl MindMap {
    /// Open the in-canvas 序号 editor for card `i` (context menu 设置序号).
    pub(crate) fn start_order_edit(&mut self, cx: &mut Cx, i: usize) {
        if self.order_editing.is_some() {
            self.commit_order_edit(cx);
        }
        let Some(t) = &self.order_edit_template else { return };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let order = self
            .data
            .as_ref()
            .and_then(|d| d.nodes.get(i))
            .and_then(|n| n.order);
        w.text_input(cx, ids!(order_edit_input))
            .set_text(cx, &order.map(|n| n.to_string()).unwrap_or_default());
        self.order_edit_ref = Some(w);
        self.order_editing = Some(i);
        self.order_focus_pending = true;
        self.redraw(cx);
    }

    /// Apply the open order edit (Enter or any canvas press): a number sets
    /// the order, empty clears it, invalid text closes without changing.
    pub(crate) fn commit_order_edit(&mut self, cx: &mut Cx) {
        let Some(i) = self.order_editing.take() else { return };
        let Some(w) = self.order_edit_ref.take() else { return };
        self.order_focus_pending = false;
        let text = w.text_input(cx, ids!(order_edit_input)).text();
        let trimmed = text.trim();
        let new_order = if trimmed.is_empty() {
            None
        } else if let Ok(n) = trimmed.parse::<u32>() {
            Some(n)
        } else {
            self.redraw(cx);
            return;
        };
        let changed = self
            .data
            .as_ref()
            .is_some_and(|d| d.nodes[i].order != new_order);
        if changed {
            if let Some(data) = &mut self.data {
                data.nodes[i].order = new_order;
            }
            self.save_map();
            if let Some(Some(card)) = self.cards.get(i).cloned() {
                let title = card_title(&self.data.as_ref().unwrap().nodes[i]);
                set_card_texts(
                    cx,
                    &card,
                    &title,
                    new_order,
                    crate::gen::card_type(&self.data.as_ref().unwrap().nodes[i].body),
                );
            }
        }
        self.redraw(cx);
    }

    pub(crate) fn cancel_order_edit(&mut self, cx: &mut Cx) {
        if self.order_editing.take().is_some() || self.order_edit_ref.take().is_some() {
            self.order_focus_pending = false;
            self.redraw(cx);
        }
    }

    /// The 序号 editor popup, drawn at the card's top-left in world coords.
    /// Focuses its TextInput once the widget has a valid area.
    pub(crate) fn draw_order_edit(&mut self, cx2d: &mut Cx2d, scope: &mut Scope) {
        let Some(i) = self.order_editing else { return };
        let Some(w) = self.order_edit_ref.clone() else { return };
        let r = self.card_rect(i);
        let _ = w.draw_walk(
            cx2d,
            scope,
            Walk {
                abs_pos: Some(r.pos + dvec2(14.0, 5.0)),
                width: Size::Fit { min: None, max: None },
                height: Size::Fit { min: None, max: None },
                ..Walk::default()
            },
        );
        if self.order_focus_pending {
            let input = w.text_input(cx2d, ids!(order_edit_input));
            if input.area().is_valid(cx2d) {
                cx2d.set_key_focus(input.area());
                self.order_focus_pending = false;
            }
        }
    }

    /// the last drawn (highest index) wins — same z-order as `resize_hit`.

    pub(crate) fn card_ref(&mut self, cx: &mut Cx, i: usize) -> WidgetRef {
        if let Some(Some(c)) = self.cards.get(i) {
            return c.clone();
        }
        let Some(t) = &self.card_template else {
            return WidgetRef::empty();
        };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let node = self.data.as_ref().unwrap().nodes[i].clone();
        let name = card_title(&node);
        // A pending title indicator (set before this widget was created)
        // overrides the file-stem title until explicitly cleared.
        let title = self.pending_titles.get(&node.path).cloned().unwrap_or(name);
        set_card_texts(cx, &w, &title, node.order, crate::gen::card_type(&node.body));
        w.markdown_media(cx, ids!(markdown)).set_text(cx, &render_body(&node.body));
        if let Some(dir) = node.path.parent() {
            w.markdown_media(cx, ids!(markdown)).set_base_dir(dir.to_path_buf());
        }
        if self.cards.len() <= i {
            self.cards.resize(i + 1, None);
        }
        self.cards[i] = Some(w.clone());
        w
    }

    pub(crate) fn enter_edit(&mut self, cx: &mut Cx, i: usize) {
        if self.editing_card.is_some() && self.editing_card != Some(i) {
            self.commit_edit(cx);
        }
        if self.editing_card == Some(i) {
            return;
        }
        let Some(card) = self.cards.get(i).and_then(|c| c.clone()) else {
            return;
        };
        let node = self.data.as_ref().unwrap().nodes[i].clone();
        card.text_input(cx, ids!(title_edit)).set_text(cx, &card_title(&node));
        card.text_input(cx, ids!(body_edit)).set_text(cx, &node.body);
        card.button(cx, ids!(edit_btn)).reset_hover(cx);
        card.button(cx, ids!(done_btn)).reset_hover(cx);
        self.editing_card = Some(i);
        self.redraw(cx);
    }

    pub(crate) fn commit_edit(&mut self, cx: &mut Cx) {
        let Some(i) = self.editing_card.take() else {
            return;
        };
        let Some(card) = self.cards.get(i).and_then(|c| c.clone()) else {
            return;
        };
        // The title input now edits the card's body file name (the header
        // shows the file stem), so committing may rename the .md file and
        // rewrite its path in every map.
        let new_name = card.text_input(cx, ids!(title_edit)).text();
        let new_body = card.text_input(cx, ids!(body_edit)).text();
        let mut renamed = false;
        if let Some(data) = &mut self.data {
            let node = &mut data.nodes[i];
            node.body = new_body;
            if let Err(e) = std::fs::write(&node.path, &node.body) {
                log!("mindmap: save {} failed: {e}", node.path.display());
            }
            if let Some(new_path) = rename_card_file(&data_dir(), &node.path, &new_name) {
                renamed = new_path != node.path;
                node.path = new_path;
            }
            let name = card_title(node);
            let body = node.body.clone();
            set_card_texts(cx, &card, &name, node.order, crate::gen::card_type(&node.body));
            card.markdown_media(cx, ids!(markdown)).set_text(cx, &render_body(&body));
        }
        if renamed {
            self.save_map();
            // The rename rewrote progress.json keys on disk; follow in memory.
            self.reload_progress(cx);
        }
        self.redraw(cx);
    }

    /// Update the in-memory body and any live card widget for the node whose
    /// path equals `full_path`. Used after external generation writes the file.
    pub(crate) fn update_card_body(&mut self, cx: &mut Cx, full_path: &std::path::Path, body: String) {
        let Some(i) = self.data.as_mut().and_then(|d| d.nodes.iter().position(|n| n.path == full_path)) else {
            return;
        };
        self.data.as_mut().unwrap().nodes[i].body = body.clone();
        if let Some(Some(card)) = self.cards.get(i).cloned() {
            card.markdown_media(cx, ids!(markdown)).set_text(cx, &render_body(&body));
        }
    }

    /// Set the visible title (and compact title) of the card at `full_path` to
    /// `indicator`, or restore it to the file-stem title when `indicator` is None.
    /// Card widgets are created lazily on draw, so an indicator set before the
    /// widget exists is recorded in `pending_titles` and applied by `card_ref`
    /// at creation; it is consumed only by an explicit None.
    pub(crate) fn set_card_title_indicator(&mut self, cx: &mut Cx, full_path: &std::path::Path, indicator: Option<&str>) {
        let Some(i) = self.data.as_mut().and_then(|d| d.nodes.iter().position(|n| n.path == full_path)) else {
            return;
        };
        let node_path = self.data.as_ref().unwrap().nodes[i].path.clone();
        match indicator {
            Some(s) => {
                self.pending_titles.insert(node_path, s.to_string());
            }
            None => {
                self.pending_titles.remove(&node_path);
            }
        }
        let title = indicator
            .map(|s| s.to_string())
            .unwrap_or_else(|| card_title(&self.data.as_ref().unwrap().nodes[i]));
        let order = self.data.as_ref().unwrap().nodes[i].order;
        let ctype = crate::gen::card_type(&self.data.as_ref().unwrap().nodes[i].body);
        if let Some(Some(card)) = self.cards.get(i).cloned() {
            set_card_texts(cx, &card, &title, order, ctype);
        }
    }

    /// Add the card file at `rel_path` (relative to the app base dir) to the
    /// canvas at the stored right-click world position, detached (no edge).
    /// No-op if the file is already on the map.
    pub(crate) fn add_card_at(&mut self, cx: &mut Cx, rel_path: &str) {
        let Some(data) = &mut self.data else { return };
        let path = data_dir().join(rel_path);
        if data.nodes.iter().any(|n| n.path == path) {
            return;
        }
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let i = data.add_detached(path, body, self.picker_world);
        self.save_map();
        self.selected = vec![i];
        self.selected_groups.clear();
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Add the card file at `child_rel` as a child of the card at
    /// `parent_rel` (划选生成子卡片): tree edge + a position to the parent's
    /// right, below any existing children. Saves and selects the new card.
    pub(crate) fn add_child_card(&mut self, cx: &mut Cx, parent_rel: &str, child_rel: &str) {
        let base = data_dir();
        let Some(data) = &mut self.data else { return };
        let Some(pi) = data.nodes.iter().position(|n| n.path == base.join(parent_rel)) else {
            return;
        };
        let path = base.join(child_rel);
        if data.nodes.iter().any(|n| n.path == path) {
            return;
        }
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let child_count = data.nodes[pi].children.len();
        let parent = &data.nodes[pi];
        let pos = parent.pos + dvec2(parent.size.x + 120.0, child_count as f64 * (CARD_H + 40.0));
        let i = data.add_detached(path, body, pos);
        data.nodes[i].parent = Some(pi);
        data.nodes[pi].children.push(i);
        // One new tree edge; card widget slots align lazily via card_ref.
        self.edges.push(cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)));
        self.save_map();
        self.selected = vec![i];
        self.selected_groups.clear();
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Reload 已见/未见 progress from progress.json and refresh the badges.
    pub(crate) fn reload_progress(&mut self, cx: &mut Cx) {
        self.progress = crate::mindmap::model::load_progress(&data_dir());
        self.redraw(cx);
    }

    /// Drop the card file at `rel_path` from the file panel onto the canvas
    /// at the screen position `abs`. No-op when the pointer is not over the
    /// canvas (a panel covers it), and when the card is already on the map.
    pub(crate) fn drop_card_at(&mut self, cx: &mut Cx, rel_path: &str, abs: DVec2) {
        if !self.area.rect(cx).contains(abs) || crate::util::over_any_panel(abs) {
            return;
        }
        // Center the card on the pointer, matching the drag ghost preview
        // (which is also centered on the cursor).
        self.picker_world = self.screen_to_world(abs) - dvec2(CARD_W, CARD_H) * 0.5;
        self.add_card_at(cx, rel_path);
    }

    /// Rel paths of every card currently on the map (for excluding them from
    /// the canvas picker's candidate list).
    pub(crate) fn card_rel_paths(&self) -> Vec<String> {
        let Some(data) = &self.data else { return Vec::new() };
        let base = data_dir();
        data.nodes
            .iter()
            .filter_map(|n| n.path.strip_prefix(&base).ok().map(|p| p.to_string_lossy().into_owned()))
            .collect()
    }

    /// Display title (file stem) + archetype of every card on the map, in
    /// node order. The parent-selection context for AI card creation.
    pub(crate) fn card_infos(&self) -> Vec<(String, crate::gen::CardType)> {
        let Some(data) = &self.data else { return Vec::new() };
        data.nodes
            .iter()
            .map(|n| (card_title(n), crate::gen::card_type(&n.body)))
            .collect()
    }

    /// Rel path of the first card whose display title equals `title`.
    pub(crate) fn rel_path_by_title(&self, title: &str) -> Option<String> {
        let Some(data) = &self.data else { return None };
        let base = data_dir();
        data.nodes
            .iter()
            .find(|n| card_title(n) == title)
            .and_then(|n| {
                n.path
                    .strip_prefix(&base)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            })
    }

    /// Number of child cards of the node whose rel path is `rel_path`
    /// (None when the card is not on the map).
    pub(crate) fn card_child_count(&self, rel_path: &str) -> Option<usize> {
        let Some(data) = &self.data else { return None };
        let base = data_dir();
        let i = data.nodes.iter().position(|n| n.path == base.join(rel_path))?;
        Some(data.nodes[i].children.len())
    }

    /// Attach a generated learning route (plan cards in learning order) under
    /// the root card at `root_rel`: add the nodes, wire parents (unknown
    /// parents fall back to the root), position the new subtree to the right
    /// of the root, and save. Other trees/groups/pan-zoom are untouched.
    pub(crate) fn attach_route(
        &mut self,
        cx: &mut Cx,
        root_rel: &str,
        cards: &[(String, String, String, Option<String>, Option<u32>)],
    ) {
        let base = data_dir();
        let Some(data) = &mut self.data else { return };
        let Some(ri) = data.nodes.iter().position(|n| n.path == base.join(root_rel)) else {
            return;
        };
        data.attach_route_nodes(ri, &base, cards);
        // Position: children stack to the right of the root (recursive).
        self.position_route_subtree(ri);
        // One edge per parent link; card widget slots align lazily via card_ref.
        self.edges = (0..self.data.as_ref().unwrap().edges().count())
            .map(|_| cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)))
            .collect();
        self.save_map();
        self.selected = vec![ri];
        self.selected_groups.clear();
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Place the children of `pi` to the card's right (recursive), stacking
    /// them by index (route children arrive in learning order).
    fn position_route_subtree(&mut self, pi: usize) {
        let parent_pos = self.data.as_ref().map(|d| d.nodes[pi].pos);
        let children = self.data.as_ref().map(|d| d.nodes[pi].children.clone());
        let (Some(parent_pos), Some(children)) = (parent_pos, children) else {
            return;
        };
        for (k, &ci) in children.iter().enumerate() {
            let pos = parent_pos + dvec2(CARD_W + 120.0, k as f64 * (CARD_H + 40.0));
            if let Some(data) = &mut self.data {
                data.nodes[ci].pos = pos;
            }
            self.position_route_subtree(ci);
        }
    }

    pub(crate) fn save_map(&self) {
        let Some(data) = &self.data else {
            return;
        };
        write_map(&data_dir(), data, self.pan_target, self.zoom_target, &self.map_file);
    }
}
