
use crate::mindmap::*;


impl MindMap {
    /// Color picker popup: a panel of preset swatches anchored below the
    /// group's title bar, drawn in screen space (main turtle) so it stays
    /// readable at any zoom. `popup_rect` is cached for hit-testing.
    pub(crate) fn draw_color_popup(&mut self, cx: &mut Cx2d, view: Rect) {
        let Some(gi) = self.color_popup else { return };
        let t = self.group_title_rect(gi);
        let tl = t.pos * self.zoom + self.pan;
        let popup_w = POPUP_PAD * 2.0 + POPUP_COLS * POPUP_SWATCH + (POPUP_COLS - 1.0) * POPUP_GAP;
        let rows = (GROUP_PRESET_COLORS.len() as f64 / POPUP_COLS).ceil();
        let popup_h = POPUP_PAD * 2.0 + rows * POPUP_SWATCH + (rows - 1.0) * POPUP_GAP;
        let mut pos = dvec2(
            tl.x + t.size.x * self.zoom - popup_w,
            tl.y + crate::mindmap::geometry::GROUP_TITLE_H * self.zoom + 8.0,
        );
        // keep the panel inside the viewport
        pos.x = pos.x.clamp(view.pos.x, view.pos.x + view.size.x - popup_w);
        pos.y = pos.y.clamp(view.pos.y, view.pos.y + view.size.y - popup_h);
        let panel = Rect { pos, size: dvec2(popup_w, popup_h) };
        self.popup_rect = panel;
        // preset swatch rects, precomputed so no borrow of self crosses the
        // popup_draw borrow below
        let swatches: Vec<Rect> = (0..GROUP_PRESET_COLORS.len()).map(|i| self.popup_swatch_rect(i)).collect();
        let Some(d) = &mut self.popup_draw else { return };
        // panel: solid dark background + soft edge
        d.draw_vars.set_uniform(cx, id!(color), &[0.16, 0.18, 0.24, 1.0]);
        d.draw_vars.set_uniform(cx, id!(fill_alpha), &[0.97]);
        d.draw_vars.set_uniform(cx, id!(width), &[4.0]);
        d.draw_abs(cx, panel);
        // preset swatches
        for (i, hex) in GROUP_PRESET_COLORS.iter().enumerate() {
            let c = parse_hex_color(hex).unwrap_or([1.0, 1.0, 1.0, 0.45]);
            d.draw_vars.set_uniform(cx, id!(color), &[c[0], c[1], c[2], 1.0]);
            d.draw_vars.set_uniform(cx, id!(fill_alpha), &[1.0]);
            d.draw_vars.set_uniform(cx, id!(width), &[1.0]);
            d.draw_abs(cx, swatches[i]);
        }
    }

    /// Rect (window coords) of the popup swatch at preset index `i`.
    pub(crate) fn popup_swatch_rect(&self, i: usize) -> Rect {
        Rect {
            pos: self.popup_rect.pos
                + dvec2(
                    POPUP_PAD + (i as f64 % POPUP_COLS) * (POPUP_SWATCH + POPUP_GAP),
                    POPUP_PAD + (i as f64 / POPUP_COLS).floor() * (POPUP_SWATCH + POPUP_GAP),
                ),
            size: dvec2(POPUP_SWATCH, POPUP_SWATCH),
        }
    }

    /// Lazily create the title-bar widget for group `gi` (mirrors card_ref).
    pub(crate) fn group_ref(&mut self, cx: &mut Cx, gi: usize) -> WidgetRef {
        if let Some(Some(w)) = self.group_refs.get(gi) {
            return w.clone();
        }
        let Some(t) = &self.group_template else {
            return WidgetRef::empty();
        };
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        let group = self.data.as_ref().unwrap().groups[gi].clone();
        w.label(cx, ids!(title)).set_text(cx, &group.title);
        w.text_input(cx, ids!(title_edit)).set_text(cx, &group.title);
        if self.group_refs.len() <= gi {
            self.group_refs.resize(gi + 1, None);
        }
        self.group_refs[gi] = Some(w.clone());
        w
    }

    /// All card indices reachable from group `gi` (its cards + nested
    /// groups' cards).
    pub(crate) fn group_subtree_cards(&self, gi: usize) -> Vec<usize> {
        let Some(data) = &self.data else { return Vec::new() };
        let mut out = Vec::new();
        let mut visited = vec![false; data.groups.len()];
        let mut stack = vec![gi];
        while let Some(g) = stack.pop() {
            if g >= visited.len() || visited[g] {
                continue;
            }
            visited[g] = true;
            let (cards, grps) = { let g = &data.groups[g]; (g.cards.clone(), g.groups.clone()) };
            out.extend(cards);
            stack.extend(grps);
        }
        out
    }

    /// Translate group `gi` and everything nested inside it (forest, so no
    /// card is moved twice).
    pub(crate) fn move_group(&mut self, gi: usize, delta: DVec2) {
        let Some(data) = &mut self.data else { return };
        let mut visited = vec![false; data.groups.len()];
        let mut stack = vec![gi];
        while let Some(g) = stack.pop() {
            if g >= visited.len() || visited[g] {
                continue;
            }
            visited[g] = true;
            let (cards, grps) = { let g = &data.groups[g]; (g.cards.clone(), g.groups.clone()) };
            for &c in &cards {
                if let Some(n) = data.nodes.get_mut(c) {
                    n.pos += delta;
                }
            }
            stack.extend(grps);
        }
    }

    /// Recreate the per-group draw/title state after a structural change
    /// (group count or indices changed). Any open color popup references a
    /// stale group index, so it closes too.
    pub(crate) fn rebuild_group_widgets(&mut self, cx: &mut Cx) {
        let n = self.data.as_ref().map(|d| d.groups.len()).unwrap_or(0);
        self.group_draws = (0..n)
            .map(|_| cx.with_vm(|vm| DrawMarquee::script_new_with_default(vm)))
            .collect();
        self.group_refs = vec![None; n];
        self.color_popup = None;
        self.hover_color_btn = None;
    }

    /// ⌘/Ctrl+G: wrap the selected cards and selected groups in a new group.
    /// Cards that already belong to a group stay there — their group is
    /// nested into the new one instead (fold_selection), so grouping over
    /// existing groups' cards wraps those groups rather than flattening them.
    /// Selected groups are re-parented under the new one. Titles auto-number
    /// as 组 N.
    pub(crate) fn group_selected(&mut self, cx: &mut Cx) {
        let Some(data) = &mut self.data else { return };
        let (cards, grps) = data.fold_selection(&self.selected, &self.selected_groups);
        let valid = cards.len() + grps.len() >= 2 || (cards.is_empty() && !grps.is_empty());
        if !valid {
            return;
        }
        // Selected groups leave their old parents (they nest under the new one).
        for &gi in &grps {
            if let Some(p) = data.group_parent(gi) {
                data.groups[p].groups.retain(|&x| x != gi);
            }
        }
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let max_n = data
            .groups
            .iter()
            .filter_map(|g| {
                let rest = g.title.strip_prefix("组 ")?;
                rest.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0);
        data.groups.push(Group {
            id: format!("g{ms}"),
            title: format!("组 {}", max_n + 1),
            cards: cards.clone(),
            groups: grps,
            color: None,
        });
        data.prune_empty_groups();
        let new_gi = self.data.as_ref().unwrap().groups.len() - 1;
        self.selected_groups = vec![new_gi];
        self.selected = self.group_subtree_cards(new_gi);
        self.rebuild_group_widgets(cx);
        self.reanchor_cards(cx);
        self.save_map();
        self.redraw(cx);
    }

    /// ⌘/Ctrl+Shift+G: dissolve every selected group and every group containing
    /// a selected card; their members (cards + nested groups) splice into the
    /// dissolved group's parent.
    pub(crate) fn ungroup_selected(&mut self, cx: &mut Cx) {
        let Some(data) = &mut self.data else { return };
        let mut doomed: Vec<usize> = self.selected_groups.clone();
        for &c in &self.selected {
            if let Some(gi) = data.group_of_card(c) {
                if !doomed.contains(&gi) {
                    doomed.push(gi);
                }
            }
        }
        if doomed.is_empty() {
            return;
        }
        // Children first (lower indices): dissolving a parent before its
        // children would splice members into a doomed group; children splice
        // up into the still-present parent.
        doomed.sort_unstable_by(|a, b| b.cmp(a));
        for &gi in &doomed {
            let (cards, grps) = { let g = &data.groups[gi]; (g.cards.clone(), g.groups.clone()) };
            if let Some(p) = data.group_parent(gi) {
                for c in cards {
                    if !data.groups[p].cards.contains(&c) {
                        data.groups[p].cards.push(c);
                    }
                }
                for g2 in grps {
                    if !data.groups[p].groups.contains(&g2) {
                        data.groups[p].groups.push(g2);
                    }
                }
            }
            data.groups.remove(gi);
            for g in &mut data.groups {
                for c in &mut g.groups {
                    if *c > gi {
                        *c -= 1;
                    }
                }
            }
        }
        self.selected_groups.clear();
        self.rebuild_group_widgets(cx);
        self.save_map();
        self.redraw(cx);
    }

    /// Double-click on a group title: show the rename input.
    pub(crate) fn enter_group_edit(&mut self, cx: &mut Cx, gi: usize) {
        if self.editing_group == Some(gi) {
            return;
        }
        if self.editing_card.is_some() {
            self.commit_edit(cx);
        }
        let Some(w) = self.group_refs.get(gi).and_then(|c| c.clone()) else {
            return;
        };
        let title = self.data.as_ref().unwrap().groups[gi].title.clone();
        w.text_input(cx, ids!(title_edit)).set_text(cx, &title);
        self.editing_group = Some(gi);
        self.redraw(cx);
    }

    /// Commit the open group rename (Enter or any canvas press).
    pub(crate) fn commit_group_edit(&mut self, cx: &mut Cx) {
        let Some(gi) = self.editing_group.take() else { return };
        let Some(w) = self.group_refs.get(gi).and_then(|c| c.clone()) else {
            return;
        };
        let new_title = w.text_input(cx, ids!(title_edit)).text();
        if let Some(data) = &mut self.data {
            let title = new_title.trim();
            if !title.is_empty() {
                data.groups[gi].title = title.to_string();
                w.label(cx, ids!(title)).set_text(cx, title);
            }
        }
        self.save_map();
        self.redraw(cx);
    }
}
