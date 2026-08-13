
use crate::mindmap::geometry::draw_edge;
use crate::mindmap::*;


impl MindMap {
    /// Draw all card connection edges, culled to the viewport.
    pub(crate) fn draw_edges(&mut self, cx2d: &mut Cx2d, local_view: Rect) {
        let edges: Vec<(usize, usize)> = self
            .data
            .as_ref()
            .map(|d| d.edges().collect())
            .unwrap_or_default();
        for (ei, (p, c)) in edges.into_iter().enumerate() {
            let [p1, p2, p3, p4] = self.edge_curve(p, c);
            let edge = &mut self.edges[ei];
            draw_edge(cx2d, edge, p1, p2, p3, p4, 2.0, Some(local_view));
        }
    }

    /// Draw the cards visible in the viewport: selection highlight, zoom
    /// collapse layers and the edit-mode swap. set_visible no-ops when the
    /// state is unchanged, so calling it every frame is free outside the
    /// zoom threshold; only View implements set_visible, so every toggle
    /// target below is wrapped in a View.
    pub(crate) fn draw_cards(&mut self, cx2d: &mut Cx2d, scope: &mut Scope, local_view: Rect) {
        let compact = self.zoom < COMPACT_ZOOM;
        let n = self.data.as_ref().map(|d| d.nodes.len()).unwrap_or(0);
        for i in 0..n {
            let r = self.card_rect(i);
            if !local_view.intersects(r) {
                continue;
            }
            // 已见/未见 glow, drawn first so a selected card's indigo halo
            // renders over it. Colors are set per draw below (the highlight
            // draw is shared with groups and the mastery glow).
            self.draw_mastery_glow(cx2d, i, r);
            if self.selected.contains(&i) {
                if let Some(hl) = &mut self.highlight {
                    hl.draw_vars.set_uniform(cx2d, id!(color), &[0.49, 0.55, 0.83, 0.45]);
                    hl.draw_abs(
                        cx2d,
                        Rect {
                            pos: r.pos - dvec2(4.0, 4.0),
                            size: r.size + dvec2(8.0, 8.0),
                        },
                    );
                }
            }
            let card = self.card_ref(cx2d, i);
            card.view(cx2d, ids!(header)).set_visible(cx2d, !compact);
            card.view(cx2d, ids!(body)).set_visible(cx2d, !compact);
            card.view(cx2d, ids!(compact_title)).set_visible(cx2d, compact);
            // edit mode swaps the read-only render for text inputs
            let editing = self.editing_card == Some(i);
            card.view(cx2d, ids!(title_box)).set_visible(cx2d, !editing);
            card.view(cx2d, ids!(title_edit_box)).set_visible(cx2d, editing);
            card.view(cx2d, ids!(read_view)).set_visible(cx2d, !editing);
            card.view(cx2d, ids!(edit_view)).set_visible(cx2d, editing);
            card.button(cx2d, ids!(edit_btn)).set_visible(cx2d, !editing);
            card.button(cx2d, ids!(done_btn)).set_visible(cx2d, editing);
            let _ = card.draw_walk(
                cx2d,
                scope,
                Walk {
                    abs_pos: Some(r.pos),
                    width: Size::Fixed(r.size.x),
                    height: Size::Fixed(r.size.y),
                    ..Walk::default()
                },
            );
        }
    }

    /// 已见/未见 glow around the card edge, same feathered halo as the
    /// selection highlight: grey = 未见 (never tested), red = tested below
    /// PASS_SCORE (判别/联结未过), green = 已见 (score >= PASS_SCORE,
    /// handleable by 经验预测). Directory nodes (no card file) get no glow.
    pub(crate) fn draw_mastery_glow(&mut self, cx2d: &mut Cx2d, i: usize, r: Rect) {
        let Some(data) = &self.data else { return };
        let Some(node) = data.nodes.get(i) else { return };
        // progress.json is keyed by rel path; Node.path is base-joined.
        let base = crate::util::data_dir();
        let Some(rel) = node.path.strip_prefix(&base).ok() else { return };
        if !node.path.is_file() {
            return;
        }
        let color: [f32; 4] = match self.progress.get(rel.to_str().unwrap_or("")) {
            Some(s) if *s >= crate::mindmap::model::PASS_SCORE => [0.29, 0.85, 0.5, 0.5],
            Some(_) => [0.97, 0.44, 0.44, 0.5],
            None => [0.42, 0.45, 0.52, 0.32],
        };
        if let Some(hl) = &mut self.highlight {
            hl.draw_vars.set_uniform(cx2d, id!(color), &color);
            hl.draw_abs(
                cx2d,
                Rect {
                    pos: r.pos - dvec2(4.0, 4.0),
                    size: r.size + dvec2(8.0, 8.0),
                },
            );
        }
    }

    /// Group frames: colored translucent border (DrawMarquee shader) + title
    /// bar, drawn under the cards. The title strip lives in the padding above
    /// the member bbox, so it never covers a member card.
    pub(crate) fn draw_groups(&mut self, cx2d: &mut Cx2d, scope: &mut Scope, local_view: Rect) {
        let n = self.data.as_ref().map(|d| d.groups.len()).unwrap_or(0);
        for gi in 0..n {
            let r = self.group_rect(gi);
            if !local_view.intersects(r) {
                continue;
            }
            let color = self.group_color(gi);
            if self.selected_groups.contains(&gi) {
                // same glow treatment as selected cards, tinted to the group
                if let Some(hl) = &mut self.highlight {
                    hl.draw_vars.set_uniform(cx2d, id!(color), &color);
                    hl.draw_abs(
                        cx2d,
                        Rect {
                            pos: r.pos - dvec2(4.0, 4.0),
                            size: r.size + dvec2(8.0, 8.0),
                        },
                    );
                }
            }
            // the frame itself: translucent fill + colored border
            if let Some(d) = self.group_draws.get_mut(gi) {
                d.draw_vars.set_uniform(cx2d, id!(color), &color);
                d.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[0.08]);
                d.draw_vars.set_uniform(cx2d, id!(width), &[4.0]);
                d.draw_abs(cx2d, r);
            }
            let t = self.group_title_rect(gi);
            // colored title bar behind the transparent title widget
            if let Some(d) = self.group_draws.get_mut(gi) {
                d.draw_vars.set_uniform(cx2d, id!(color), &color);
                d.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[1.0]);
                d.draw_vars.set_uniform(cx2d, id!(width), &[1.5]);
                d.draw_abs(cx2d, t);
            }
            let w = self.group_ref(cx2d, gi);
            let editing = self.editing_group == Some(gi);
            w.view(cx2d, ids!(title_box)).set_visible(cx2d, !editing);
            w.view(cx2d, ids!(title_edit_box)).set_visible(cx2d, editing);
            let _ = w.draw_walk(
                cx2d,
                scope,
                Walk {
                    abs_pos: Some(t.pos),
                    width: Size::Fixed(t.size.x),
                    height: Size::Fixed(t.size.y),
                    ..Walk::default()
                },
            );
            // color button: soft hover highlight behind a tinted palette icon
            let btn = self.color_button_rect(gi);
            if self.hover_color_btn == Some(gi) {
                if let Some(d) = self.group_draws.get_mut(gi) {
                    let hr = Rect { pos: btn.pos - dvec2(2.5, 2.5), size: btn.size + dvec2(5.0, 5.0) };
                    d.draw_vars.set_uniform(cx2d, id!(color), &color);
                    d.draw_vars.set_uniform(cx2d, id!(fill_alpha), &[0.28]);
                    d.draw_vars.set_uniform(cx2d, id!(width), &[2.0]);
                    d.draw_abs(cx2d, hr);
                }
            }
            // fixed white icon (SVG strokes are white; the color uniform
            // stays at its -1 default so the SVG's own colors render)
            self.draw_grp_icon.draw_abs(cx2d, btn);
        }
    }

    /// The group's frame color as an RGBA uniform (alpha = shader stroke
    /// alpha); falls back to the script default indigo when unset.
    pub(crate) fn group_color(&self, gi: usize) -> [f32; 4] {
        let Some(data) = &self.data else { return [0.49, 0.55, 0.83, 0.45] };
        data.groups
            .get(gi)
            .and_then(|g| g.color.as_deref())
            .and_then(parse_hex_color)
            .unwrap_or([0.49, 0.55, 0.83, 0.45])
    }
}
