
use crate::mindmap::*;
use crate::util::data_dir;


impl MindMap {
    /// Rel path of node `i` ("" when missing).
    fn rel_path(&self, i: usize) -> Option<String> {
        let data = self.data.as_ref()?;
        let base = data_dir();
        data.nodes
            .get(i)?
            .path
            .strip_prefix(&base)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// Rebuild `edges` (one DrawEdge per parent link) after any tree change.
    pub(crate) fn rebuild_edges(&mut self, cx: &mut Cx) {
        self.edges = (0..self.data.as_ref().map(|d| d.edges().count()).unwrap_or(0))
            .map(|_| cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)))
            .collect();
    }

    /// Enter 连线到… mode: the source card shows a "连线到…" title indicator
    /// and the next canvas click picks the target card (source attaches under
    /// it). Esc / right-click / blank click cancel.
    pub(crate) fn enter_connect_mode(&mut self, cx: &mut Cx, i: usize) {
        self.connect_from = Some(i);
        self.connect_hover = None;
        self.selected = vec![i];
        self.selected_groups.clear();
        if let Some(rel) = self.rel_path(i) {
            self.set_card_title_indicator(cx, &data_dir().join(rel), Some("连线到…"));
        }
        self.redraw(cx);
    }

    /// Leave connect mode and restore the source card's title indicator.
    pub(crate) fn cancel_connect_mode(&mut self, cx: &mut Cx) {
        if let Some(i) = self.connect_from.take() {
            if let Some(rel) = self.rel_path(i) {
                self.set_card_title_indicator(cx, &data_dir().join(rel), None);
            }
        }
        self.connect_hover = None;
        self.redraw(cx);
    }

    /// Left click while in connect mode: `Some(target)` connects the source
    /// under the target (cycle/duplicate failures keep the mode open and are
    /// surfaced to the App as a toast); `None` or the source itself cancels.
    pub(crate) fn connect_click(&mut self, cx: &mut Cx, target: Option<usize>) {
        let Some(from) = self.connect_from else {
            return;
        };
        match target {
            None => self.cancel_connect_mode(cx),
            Some(t) if t == from => self.cancel_connect_mode(cx),
            Some(t) => {
                let result = self.data.as_mut().map(|d| d.connect(t, from));
                let from_rel = self.rel_path(from).unwrap_or_default();
                let to_rel = self.rel_path(t).unwrap_or_default();
                match result {
                    Some(Ok(())) => {
                        self.cancel_connect_mode(cx);
                        self.rebuild_edges(cx);
                        self.save_map();
                        cx.widget_action(
                            self.widget_uid(),
                            MindMapAction::Connect(from_rel, to_rel),
                        );
                    }
                    Some(Err(e)) => {
                        cx.widget_action(
                            self.widget_uid(),
                            MindMapAction::ConnectRejected(e.to_string()),
                        );
                    }
                    None => self.cancel_connect_mode(cx),
                }
            }
        }
    }

    /// 断开与父卡片的连线: the card's subtree becomes an independent root
    /// card (order badge cleared). The App toasts via the emitted action.
    pub(crate) fn disconnect_card(&mut self, cx: &mut Cx, i: usize) {
        let rel = self.rel_path(i).unwrap_or_default();
        if let Some(data) = &mut self.data {
            data.disconnect(i);
        }
        self.rebuild_edges(cx);
        self.save_map();
        self.redraw(cx);
        cx.widget_action(self.widget_uid(), MindMapAction::Disconnect(rel));
    }

    /// Display titles + parent titles + archetypes of the subtree under the
    /// root card at `root_rel` (the root itself excluded; entries are
    /// pre-order). The input context for AI learning-order estimation.
    pub(crate) fn subtree_entries(&self, root_rel: &str) -> Vec<(String, String, crate::gen::CardType)> {
        let Some(data) = &self.data else { return Vec::new() };
        let base = data_dir();
        let Some(ri) = data.nodes.iter().position(|n| n.path == base.join(root_rel)) else {
            return Vec::new();
        };
        let root_title = card_title(&data.nodes[ri]);
        let mut out = Vec::new();
        let mut stack: Vec<(usize, String)> = data.nodes[ri]
            .children
            .iter()
            .map(|&c| (c, root_title.clone()))
            .collect();
        while let Some((x, ptitle)) = stack.pop() {
            let t = card_title(&data.nodes[x]);
            out.push((t.clone(), ptitle, crate::gen::card_type(&data.nodes[x].body)));
            stack.extend(data.nodes[x].children.iter().map(|&c| (c, t.clone())));
        }
        out
    }

    /// Apply AI-estimated learning orders (title → number) to the cards of
    /// the subtree rooted at `root_rel`; the root card itself stays
    /// unnumbered. Titles are matched within that subtree only. Saves and
    /// refreshes the live badges.
    pub(crate) fn apply_orders(&mut self, cx: &mut Cx, root_rel: &str, orders: &[(String, u32)]) {
        let base = data_dir();
        let mut updates: Vec<(usize, String, u32, crate::gen::CardType)> = Vec::new();
        if let Some(data) = &mut self.data {
            let Some(ri) = data.nodes.iter().position(|n| n.path == base.join(root_rel)) else {
                return;
            };
            let mut members: Vec<usize> = Vec::new();
            let mut stack = data.nodes[ri].children.clone();
            while let Some(x) = stack.pop() {
                members.push(x);
                stack.extend(data.nodes[x].children.iter().copied());
            }
            let mut used: Vec<usize> = Vec::new();
            for (title, n) in orders {
                if let Some(&i) = members
                    .iter()
                    .find(|&&m| !used.contains(&m) && card_title(&data.nodes[m]) == *title)
                {
                    data.nodes[i].order = Some(*n);
                    used.push(i);
                }
            }
            for i in used {
                updates.push((
                    i,
                    card_title(&data.nodes[i]),
                    data.nodes[i].order.unwrap_or(0),
                    crate::gen::card_type(&data.nodes[i].body),
                ));
            }
        }
        for (i, title, order, ctype) in updates {
            if let Some(Some(card)) = self.cards.get(i).cloned() {
                set_card_texts(cx, &card, &title, Some(order), ctype);
            }
        }
        self.save_map();
        self.redraw(cx);
    }

    /// Remove the parent-less node at `rel` and its whole subtree from the
    /// map (confirmed by the App). Card files stay on disk.
    pub(crate) fn remove_root_subtree(&mut self, cx: &mut Cx, rel: &str) {
        let base = data_dir();
        let Some(i) = self
            .data
            .as_ref()
            .and_then(|d| d.nodes.iter().position(|n| n.path == base.join(rel)))
        else {
            return;
        };
        if let Some(data) = &mut self.data {
            data.remove_subtree(i);
        }
        self.cards.clear();
        self.rebuild_edges(cx);
        self.rebuild_group_widgets(cx);
        self.selected.clear();
        self.selected_groups.clear();
        self.rect_targets.clear();
        self.editing_card = None;
        self.order_editing = None;
        self.order_edit_ref = None;
        self.order_focus_pending = false;
        self.drag_card = None;
        self.drag_group = None;
        self.resize_card = None;
        self.marquee = None;
        self.save_map();
        self.redraw(cx);
    }
}
