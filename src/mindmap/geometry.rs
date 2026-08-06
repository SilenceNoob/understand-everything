use super::*;

impl MindMap {
    pub(super) fn card_rect(&self, i: usize) -> Rect {
        let node = &self.data.as_ref().unwrap().nodes[i];
        Rect {
            pos: node.pos,
            size: node.size,
        }
    }

    /// Screen coords → world coords (inverse of the pan/zoom transform).
    pub(super) fn screen_to_world(&self, p: DVec2) -> DVec2 {
        (p - self.pan) / self.zoom
    }

    /// World-space bezier points for the connector between parent `p` and
    /// child `c`: start/end at the card edge midpoints, horizontal-tangent
    /// control points (clamped so short links don't get a sharp kink).
    pub(super) fn edge_curve(&self, p: usize, c: usize) -> [DVec2; 4] {
        let p_rect = self.card_rect(p);
        let c_rect = self.card_rect(c);
        let p1 = p_rect.pos + dvec2(p_rect.size.x, p_rect.size.y * 0.5);
        let p4 = c_rect.pos + dvec2(0.0, c_rect.size.y * 0.5);
        let reach = ((p4.x - p1.x).abs() * 0.5).clamp(60.0, 220.0);
        [p1, p1 + dvec2(reach, 0.0), p4 - dvec2(reach, 0.0), p4]
    }

    pub(super) fn hit_card(&self, world: DVec2) -> Option<usize> {
        let data = self.data.as_ref()?;
        for i in (0..data.nodes.len()).rev() {
            let n = &data.nodes[i];
            if world.x >= n.pos.x
                && world.x <= n.pos.x + n.size.x
                && world.y >= n.pos.y
                && world.y <= n.pos.y + n.size.y
            {
                return Some(i);
            }
        }
        None
    }

    pub(super) fn resize_hit(&self, p: DVec2) -> Option<(usize, u8)> {
        let data = self.data.as_ref()?;
        let t = 6.0 / self.zoom;
        for i in (0..data.nodes.len()).rev() {
            let r = self.card_rect(i);
            let dir = crate::util::resize_dir(r, p, t);
            if dir != 0 {
                return Some((i, dir));
            }
        }
        None
    }

    // Cards are laid out in world coords but hit-testing compares raw event
    // abs (window coords) against the untransformed area rects, so map events
    // into the canvas-local space before dispatching to cards.
    pub(super) fn remap_event(&self, event: &Event) -> Option<Event> {
        let map = |p: DVec2| self.screen_to_world(p);
        match event {
            Event::MouseDown(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseDown(e))
            }
            Event::MouseMove(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseMove(e))
            }
            Event::MouseUp(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseUp(e))
            }
            Event::MouseLeave(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::MouseLeave(e))
            }
            Event::LongPress(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::LongPress(e))
            }
            Event::Scroll(e) => {
                let mut e = e.clone();
                e.abs = map(e.abs);
                Some(Event::Scroll(e))
            }
            Event::TouchUpdate(e) => {
                let mut e = e.clone();
                for t in &mut e.touches {
                    t.abs = map(t.abs);
                }
                Some(Event::TouchUpdate(e))
            }
            _ => None,
        }
    }
}

/// Draw one bezier edge through its four control points. The curve stays
/// inside the control points' convex hull, so the bbox over all four points
/// always covers it; the draw rect is that bbox +4px. Culls to `view` when
/// given.
pub(super) fn draw_edge(
    cx2d: &mut Cx2d,
    edge: &mut DrawEdge,
    p1: DVec2,
    p2: DVec2,
    p3: DVec2,
    p4: DVec2,
    line_width: f32,
    view: Option<Rect>,
) {
    let min_x = p1.x.min(p2.x).min(p3.x).min(p4.x) - 4.0;
    let max_x = p1.x.max(p2.x).max(p3.x).max(p4.x) + 4.0;
    let min_y = p1.y.min(p2.y).min(p3.y).min(p4.y) - 4.0;
    let max_y = p1.y.max(p2.y).max(p3.y).max(p4.y) + 4.0;
    let rect = Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2(max_x - min_x, max_y - min_y),
    };
    if let Some(view) = view {
        if !view.intersects(rect) {
            return;
        }
    }
    let to_local = |p: DVec2| {
        vec2(
            ((p.x - rect.pos.x) / rect.size.x) as f32,
            ((p.y - rect.pos.y) / rect.size.y) as f32,
        )
    };
    edge.draw_vars.set_uniform(cx2d, id!(p1), &[to_local(p1).x, to_local(p1).y]);
    edge.draw_vars.set_uniform(cx2d, id!(p2), &[to_local(p2).x, to_local(p2).y]);
    edge.draw_vars.set_uniform(cx2d, id!(p3), &[to_local(p3).x, to_local(p3).y]);
    edge.draw_vars.set_uniform(cx2d, id!(p4), &[to_local(p4).x, to_local(p4).y]);
    edge.draw_vars.set_uniform(cx2d, id!(line_width), &[line_width]);
    edge.draw_abs(cx2d, rect);
}

