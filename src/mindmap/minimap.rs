use super::*;

use super::geometry::draw_edge;

impl MindMap {
    pub(super) fn draw_minimap(&mut self, cx: &mut Cx2d, view: Rect) {
        let mm_rect = Rect {
            pos: view.pos
                + dvec2(view.size.x - MM_W - MM_MARGIN, view.size.y - MM_H - MM_MARGIN),
            size: dvec2(MM_W, MM_H),
        };
        self.minimap_rect = mm_rect;
        cx.push_clip_rect(mm_rect);
        self.draw_mm_bg.draw_abs(cx, mm_rect);

        let (scale, offset) = self
            .data
            .as_ref()
            .map(|data| {
                let scale = ((MM_W - 2.0 * MM_PAD) / data.max_w)
                    .min((MM_H - 2.0 * MM_PAD) / data.max_h);
                let offset = mm_rect.pos
                    + (mm_rect.size - dvec2(data.max_w * scale, data.max_h * scale)) * 0.5;
                self.mm_scale = scale;
                self.mm_offset = offset;
                (scale, offset)
            })
            .unwrap_or((0.0, DVec2::default()));

        // Connectors first, so the cards draw on top. Same bezier points as
        // the canvas, just mapped into minimap space (identical S-curves).
        if scale > 0.0 {
            let to_mm = |p: DVec2| p * scale + offset;
            let edges: Vec<(usize, usize)> = self
                .data
                .as_ref()
                .map(|d| d.edges().collect())
                .unwrap_or_default();
            for (ei, (p, c)) in edges.into_iter().enumerate() {
                let [p1, p2, p3, p4] = self.edge_curve(p, c).map(to_mm);
                let edge = &mut self.edges[ei];
                // Thinner than the canvas (2.0) so it fits the small panel.
                draw_edge(cx, edge, p1, p2, p3, p4, 1.0, None);
            }
        }

        let card_rects: Vec<Rect> = self
            .data
            .as_ref()
            .map(|data| {
                let to_mm = |p: DVec2| p * scale + offset;
                data.nodes
                    .iter()
                    .map(|n| Rect {
                        pos: to_mm(n.pos),
                        size: n.size * scale,
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (i, r) in card_rects.iter().enumerate() {
            let draw = if self.selected.contains(&i) {
                &mut self.draw_mm_sel
            } else {
                &mut self.draw_mm_card
            };
            draw.draw_abs(cx, *r);
        }

        // Viewport indicator: the current pan/zoom view in world coords.
        if self.mm_scale > 0.0 {
            let world_view = Rect {
                pos: self.screen_to_world(view.pos),
                size: view.size / self.zoom,
            };
            let to_mm = |p: DVec2| p * self.mm_scale + self.mm_offset;
            self.draw_mm_view.draw_abs(
                cx,
                Rect {
                    pos: to_mm(world_view.pos),
                    size: world_view.size * self.mm_scale,
                },
            );
        }
        cx.pop_clip_rect();
    }

        // Jump the viewport so the minimap point under `abs` becomes the view
        // center; used for click-to-jump and drag-to-navigate. Animates toward
        // the target; during drag the target is re-aimed every move so the
        // view smoothly chases the cursor.
    pub(super) fn navigate_minimap(&mut self, cx: &mut Cx, abs: DVec2) {
            if self.mm_scale <= 0.0 {
                return;
            }
            let world = (abs - self.mm_offset) / self.mm_scale;
            let view_center = (self.view_rect.pos + self.view_rect.size * 0.5) * self.zoom + self.pan;
            self.pan_target = view_center - world * self.zoom;
            self.zoom_target = self.zoom;
            self.start_zoom_anim(cx);
            self.redraw(cx);
        }

    pub(super) fn on_minimap(&self, event: &Event) -> bool {
            let hit = |p: &DVec2| self.minimap_rect.contains(*p);
            match event {
                Event::MouseDown(e) => hit(&e.abs),
                Event::MouseMove(e) => hit(&e.abs),
                Event::MouseUp(e) => hit(&e.abs),
                Event::Scroll(e) => hit(&e.abs),
                Event::LongPress(e) => hit(&e.abs),
                Event::TouchUpdate(e) => e.touches.iter().any(|t| hit(&t.abs)),
                _ => false,
            }
        }
    }
