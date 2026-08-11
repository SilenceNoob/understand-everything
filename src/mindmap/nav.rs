use super::*;

/// Held-key bitmasks: WASD/QE pan-zoom keys; arrows (move or Shift+resize)
/// share a second layout.
const KEY_W: u8 = 1;
const KEY_A: u8 = 2;
const KEY_S: u8 = 4;
const KEY_D: u8 = 8;
const KEY_Q: u8 = 16;
const KEY_E: u8 = 32;
const ARROW_UP: u8 = 1;
const ARROW_DOWN: u8 = 2;
const ARROW_LEFT: u8 = 4;
const ARROW_RIGHT: u8 = 8;

/// Axis delta from a held-key bitmask: +1 with `pos` held, -1 with `neg`
/// held, else 0.
fn axis(bits: u8, pos: u8, neg: u8) -> f64 {
    ((bits & pos) != 0) as i8 as f64 - ((bits & neg) != 0) as i8 as f64
}

/// The primary modifier key: Command (⌘) on macOS, Control elsewhere
/// (mirrors KeyModifiers::is_primary's cfg split).
fn mod_key() -> KeyCode {
    #[cfg(target_vendor = "apple")]
    {
        KeyCode::Logo
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        KeyCode::Control
    }
}

impl MindMap {
    pub(super) fn handle_keys(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Group rename: Enter commits, Esc cancels; the TextInput owns the
        // letters, so the nav handling below is skipped while renaming.
        if self.editing_group.is_some() {
            if let Event::KeyDown(ke) = event {
                match ke.key_code {
                    KeyCode::ReturnKey | KeyCode::NumpadEnter => self.commit_group_edit(cx),
                    KeyCode::Escape => {
                        self.editing_group = None;
                        self.redraw(cx);
                    }
                    _ => {}
                }
            }
        }
        // Esc also closes an open color picker popup.
        if self.color_popup.is_some() {
            if let Event::KeyDown(ke) = event {
                if ke.key_code == KeyCode::Escape {
                    self.color_popup = None;
                    self.redraw(cx);
                }
            }
        }
        if self.editing_card.is_none()
            && self.editing_group.is_none()
            && self.order_editing.is_none()
            && !crate::file_panel::is_name_editing()
            && !crate::float_panel::is_chat_input_active()
        {
            match event {
                Event::KeyDown(ke) => {
                    if ke.modifiers.is_primary() && ke.key_code == KeyCode::KeyG && !ke.is_repeat {
                        if ke.modifiers.shift {
                            self.ungroup_selected(cx);
                        } else {
                            self.group_selected(cx);
                        }
                    } else if ke.key_code == KeyCode::Space && !ke.is_repeat {
                        self.select_view_center(cx, ke.modifiers.shift);
                    } else if ke.modifiers.shift && arrow_mask(ke.key_code).is_some() {
                        // Shift+arrow pages the selected card's markdown body;
                        // repeats keep paging. Left/Right are deliberately
                        // absorbed so they never fall through to card movement.
                        self.page_card(ke.key_code, cx, scope);
                    } else if ke.modifiers.alt && arrow_mask(ke.key_code).is_some() {
                        // Alt/Option+arrow resizes the selected cards
                        // (bottom-right handle); holding the primary
                        // modifier (⌘/Ctrl+Alt+arrow) snaps to the grid.
                        self.set_arrow(ke.key_code, true, true, cx);
                    } else {
                        self.set_key_move(ke.key_code, true, cx);
                        // Plain arrows move without snapping; holding the
                        // primary modifier (⌘/Ctrl) snaps to the grid.
                        self.set_arrow(ke.key_code, true, false, cx);
                    }
                }
                Event::KeyUp(ke) => {
                    self.set_key_move(ke.key_code, false, cx);
                    self.set_arrow(ke.key_code, false, false, cx);
                    self.set_arrow(ke.key_code, false, true, cx);
                    if arrow_mask(ke.key_code).is_some() {
                        self.cancel_page_burst(cx);
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn handle_zoom_anim(&mut self, cx: &mut Cx, event: &Event) {
        let Some(timer) = self.zoom_timer else { return };
        let Some(te) = timer.is_event(event) else { return };
        let now = te.time.unwrap_or(0.0);
        // first tick has no baseline; fall back to one 60Hz frame
        let dt = if self.last_timer_time == 0.0 {
            1.0 / 60.0
        } else {
            (now - self.last_timer_time).max(0.0)
        };
        self.last_timer_time = now;
        // Held-key velocity: WASD moves the pan target (skipped while the
        // mouse is drag-panning so they don't fight), QE zoom center-anchored.
        if self.key_move != 0 && !self.panning {
            let bits = self.key_move;
            let dir = dvec2(axis(bits, KEY_A, KEY_D), axis(bits, KEY_W, KEY_S));
            self.pan_target += dir * (MOVE_SPEED * dt);
            let rate = axis(bits, KEY_E, KEY_Q) * ZOOM_KEY_SPEED;
            if rate != 0.0 {
                // view_rect center is already in world coords; keep it at the
                // same screen position: screen = wc*zoom + pan, solve for pan.
                let wc = self.view_rect.pos + self.view_rect.size * 0.5;
                self.zoom_target = (self.zoom_target * (rate * dt).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
                self.pan_target = self.pan + wc * (self.zoom - self.zoom_target);
            }
        }
        // Arrow keys: advance the selected cards' position targets
        // (screen-constant speed, like WASD), then ease toward the targets.
        if self.arrow_move != 0 {
            let dir = dvec2(
                axis(self.arrow_move, ARROW_RIGHT, ARROW_LEFT),
                axis(self.arrow_move, ARROW_DOWN, ARROW_UP),
            );
            let delta = dir * (ARROW_MOVE_SPEED / self.zoom) * dt;
            if self.ctrl_down {
                // Grid mode: accumulate the per-tick delta and apply whole
                // cells only (round-to-nearest per tick collapses when the
                // delta is under half a cell, and can jitter backward near a
                // boundary). Stepping always follows the motion direction;
                // the pin keeps targets on the grid and aligns an off-grid
                // start. Average speed matches the unsnapped branch.
                self.grid_accum += delta;
                let mut step = dvec2(0.0, 0.0);
                while self.grid_accum.x >= GRID_SIZE {
                    self.grid_accum.x -= GRID_SIZE;
                    step.x += GRID_SIZE;
                }
                while self.grid_accum.x <= -GRID_SIZE {
                    self.grid_accum.x += GRID_SIZE;
                    step.x -= GRID_SIZE;
                }
                while self.grid_accum.y >= GRID_SIZE {
                    self.grid_accum.y -= GRID_SIZE;
                    step.y += GRID_SIZE;
                }
                while self.grid_accum.y <= -GRID_SIZE {
                    self.grid_accum.y += GRID_SIZE;
                    step.y -= GRID_SIZE;
                }
                if step != dvec2(0.0, 0.0) {
                    for (_, t) in &mut self.rect_targets {
                        t.pos += step;
                    }
                }
                for (_, t) in &mut self.rect_targets {
                    t.pos = Self::snap_grid(t.pos);
                }
            } else {
                self.grid_accum = dvec2(0.0, 0.0);
                for (_, t) in &mut self.rect_targets {
                    t.pos += delta;
                }
            }
        }
        // Alt/Option+arrow: bottom-right handle mode — the top-left corner is
        // pinned, Right/Down grow and Left/Up shrink. With the primary
        // modifier (⌘/Ctrl+Alt+arrow) sizes step whole grid cells via the
        // same accumulator (snap-then-clamp so CARD_MIN_SIZE holds).
        if self.resize_arrows != 0 {
            let rx = axis(self.resize_arrows, ARROW_RIGHT, ARROW_LEFT);
            let ry = axis(self.resize_arrows, ARROW_DOWN, ARROW_UP);
            let s = (RESIZE_SPEED / self.zoom) * dt;
            if self.ctrl_down {
                self.grid_accum += dvec2(rx * s, ry * s);
                let mut step = dvec2(0.0, 0.0);
                while self.grid_accum.x >= GRID_SIZE {
                    self.grid_accum.x -= GRID_SIZE;
                    step.x += GRID_SIZE;
                }
                while self.grid_accum.x <= -GRID_SIZE {
                    self.grid_accum.x += GRID_SIZE;
                    step.x -= GRID_SIZE;
                }
                while self.grid_accum.y >= GRID_SIZE {
                    self.grid_accum.y -= GRID_SIZE;
                    step.y += GRID_SIZE;
                }
                while self.grid_accum.y <= -GRID_SIZE {
                    self.grid_accum.y += GRID_SIZE;
                    step.y -= GRID_SIZE;
                }
                if step != dvec2(0.0, 0.0) {
                    for (_, t) in &mut self.rect_targets {
                        t.size += step;
                    }
                }
                for (_, t) in &mut self.rect_targets {
                    t.size.x = (t.size.x / GRID_SIZE).round() * GRID_SIZE;
                    t.size.y = (t.size.y / GRID_SIZE).round() * GRID_SIZE;
                }
            } else {
                self.grid_accum = dvec2(0.0, 0.0);
                for (_, t) in &mut self.rect_targets {
                    t.size.x += rx * s;
                    t.size.y += ry * s;
                }
            }
            for (_, t) in &mut self.rect_targets {
                t.size.x = t.size.x.clamp(CARD_MIN_SIZE, CARD_MAX_SIZE);
                t.size.y = t.size.y.clamp(CARD_MIN_SIZE, CARD_MAX_SIZE);
            }
        }
        let k = 1.0 - (-dt * ZOOM_EASE_SPEED).exp();
        self.zoom += (self.zoom_target - self.zoom) * k;
        self.pan += (self.pan_target - self.pan) * k;
        let mut cards_done = true;
        // Drag/resize own the card geometry; skip the ease so they don't fight.
        if self.drag_card.is_none() && self.resize_card.is_none() {
            if let Some(data) = &mut self.data {
                for &(i, t) in &self.rect_targets {
                    let n = &mut data.nodes[i];
                    n.pos += (t.pos - n.pos) * k;
                    n.size += (t.size - n.size) * k;
                    if (n.pos - t.pos).length() >= 0.5 || (n.size - t.size).length() >= 0.5 {
                        cards_done = false;
                    }
                }
            }
        }
        if (self.zoom_target - self.zoom).abs() < 5e-4
            && (self.pan_target - self.pan).length() < 0.5
            && self.arrow_move == 0
            && self.resize_arrows == 0
            && cards_done
        {
            self.zoom = self.zoom_target;
            self.pan = self.pan_target;
            if let Some(data) = &mut self.data {
                for &(i, t) in &self.rect_targets {
                    data.nodes[i].pos = t.pos;
                    data.nodes[i].size = t.size;
                }
            }
            self.save_map();
            cx.stop_timer(timer);
            self.zoom_timer = None;
        }
        self.redraw(cx);
    }

    /// Ensure the repeating zoom timer is running (idempotent).
    pub(super) fn start_zoom_anim(&mut self, cx: &mut Cx) {
        if self.zoom_timer.is_none() {
            self.zoom_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_timer_time = 0.0;
        }
    }

    /// Stop animating and pin the targets to the current view, so direct
    /// panning isn't fought by a stale in-flight target.
    pub(super) fn cancel_zoom_anim(&mut self, cx: &mut Cx) {
        if let Some(t) = self.zoom_timer.take() {
            cx.stop_timer(t);
        }
        self.zoom_target = self.zoom;
        self.pan_target = self.pan;
    }

    /// Primary modifier held (⌘ on macOS, Ctrl elsewhere): start/stop the
    /// grid fade (and record the key state for drag snapping). KeyUp can be
    /// lost on focus loss, so the state is also cleared on map switches
    /// (reset_grid_state).
    pub(super) fn handle_grid_key(&mut self, cx: &mut Cx, event: &Event) {
        match event {
            Event::KeyDown(ke) if ke.key_code == mod_key() && !ke.is_repeat => {
                self.set_grid_fade(cx, 1.0);
                self.ctrl_down = true;
            }
            Event::KeyUp(ke) if ke.key_code == mod_key() => {
                self.set_grid_fade(cx, 0.0);
                self.ctrl_down = false;
            }
            _ => {}
        }
    }

    /// Ease `grid_alpha` toward its target on the 60Hz fade timer, stopping
    /// once settled. Mirrors handle_zoom_anim.
    pub(super) fn handle_grid_anim(&mut self, cx: &mut Cx, event: &Event) {
        let Some(timer) = self.grid_timer else { return };
        let Some(te) = timer.is_event(event) else { return };
        let now = te.time.unwrap_or(0.0);
        // first tick has no baseline; fall back to one 60Hz frame
        let dt = if self.last_grid_time == 0.0 {
            1.0 / 60.0
        } else {
            (now - self.last_grid_time).max(0.0)
        };
        self.last_grid_time = now;
        let k = 1.0 - (-dt * GRID_EASE_SPEED).exp();
        self.grid_alpha += (self.grid_alpha_target - self.grid_alpha) * k;
        if (self.grid_alpha_target - self.grid_alpha).abs() < 0.005 {
            self.grid_alpha = self.grid_alpha_target;
            cx.stop_timer(timer);
            self.grid_timer = None;
        }
        self.redraw(cx);
    }

    /// Stop the fade timer and reset grid state (map switch).
    pub(super) fn reset_grid_state(&mut self, cx: &mut Cx) {
        if let Some(t) = self.grid_timer.take() {
            cx.stop_timer(t);
        }
        self.ctrl_down = false;
        self.grid_alpha = 0.0;
        self.grid_alpha_target = 0.0;
        self.grid_accum = dvec2(0.0, 0.0);
    }

    /// Round a world-space point to the grid (primary-modifier drag snapping).
    pub(super) fn snap_grid(p: DVec2) -> DVec2 {
        dvec2(
            (p.x / GRID_SIZE).round() * GRID_SIZE,
            (p.y / GRID_SIZE).round() * GRID_SIZE,
        )
    }

    /// Target the grid fade (1.0 shown, 0.0 hidden), starting the 60Hz timer.
    fn set_grid_fade(&mut self, cx: &mut Cx, target: f64) {
        if self.grid_timer.is_none() {
            self.grid_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_grid_time = 0.0;
        }
        self.grid_alpha_target = target;
    }

    /// Select the card under the view center, or the group whose frame
    /// contains it (card first); with `add` (Shift+Space) the hit is added
    /// to the selection instead of replacing it.
    pub(super) fn select_view_center(&mut self, cx: &mut Cx, add: bool) {
        // view_rect is the world-space viewport rect, so its center is the
        // hit point directly (no screen->world conversion).
        let world = self.view_rect.pos + self.view_rect.size * 0.5;
        match self.hit_card(world) {
            Some(i) => {
                if add {
                    if !self.selected.contains(&i) {
                        self.selected.push(i);
                    }
                } else {
                    self.selected = vec![i];
                    self.selected_groups.clear();
                }
            }
            None => {
                if let Some(gi) = self.hit_group_frame(world) {
                    let cards = {
                        let g = &self.data.as_ref().unwrap().groups[gi];
                        g.cards.clone()
                    };
                    if add {
                        for c in cards {
                            if !self.selected.contains(&c) {
                                self.selected.push(c);
                            }
                        }
                        if !self.selected_groups.contains(&gi) {
                            self.selected_groups.push(gi);
                        }
                    } else {
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                    }
                } else if !add {
                    self.selected.clear();
                    self.selected_groups.clear();
                }
            }
        }
        self.reanchor_cards(cx);
        self.redraw(cx);
    }

    /// Alt/Option+arrow: page the first selected card's markdown body by one
    /// viewport. One page = PAGE_TICKS small instant scrolls (is_mouse: false
    /// skips the bar's smoothing glide) paced at 60Hz by `page_timer`, so the
    /// motion is constant-speed and refresh-rate independent. Synthesizes
    /// wheel-like Scroll events at the body's center and dispatches them
    /// through the card's own event path, so makepad's scrollbar state
    /// (clamp, position) stays the single source of truth. Key repeats
    /// extend the burst.
    pub(super) fn page_card(&mut self, code: KeyCode, cx: &mut Cx, scope: &mut Scope) {
        let Some(&i) = self.selected.first() else {
            return;
        };
        let dir = match code {
            KeyCode::ArrowDown => 1.0,
            KeyCode::ArrowUp => -1.0,
            _ => return,
        };
        if let Some(burst) = &mut self.page_burst {
            burst.left += PAGE_TICKS;
        } else {
            self.page_burst = Some(PageBurst {
                card: i,
                dir,
                left: PAGE_TICKS,
            });
            self.page_timer = Some(cx.start_interval(1.0 / 60.0));
        }
        self.dispatch_page_tick(cx, scope);
    }

    /// Dispatch one 60Hz tick of the current page burst (a viewport/PAGE_TICKS
    /// instant scroll).
    pub(super) fn dispatch_page_tick(&mut self, cx: &mut Cx, scope: &mut Scope) {
        let Some(burst) = self.page_burst else {
            return;
        };
        let Some(card) = self.cards.get(burst.card).and_then(|c| c.clone()) else {
            return;
        };
        // Cards live in world coords, so the body rect (from the last pass)
        // is already world-space; stale/empty rects (compact mode, not yet
        // drawn) fail the contains() check and safely no-op.
        let body_rect = card.view(cx, ids!(body)).area().rect(cx);
        if body_rect.size.y <= 0.0 {
            return;
        }
        let e = ScrollEvent {
            // The scroll handling only reads abs/scroll, so any id works.
            window_id: WindowId(0, 0),
            scroll: dvec2(0.0, burst.dir * body_rect.size.y / PAGE_TICKS as f64),
            abs: body_rect.pos + body_rect.size * 0.5,
            modifiers: KeyModifiers::default(),
            handled_x: Cell::new(false),
            handled_y: Cell::new(false),
            // False → instant set_scroll_pos branch: our timer paces the
            // motion, the bar just applies each small step immediately.
            is_mouse: false,
            time: 0.0,
            phase: ScrollPhase::None,
        };
        card.handle_event(cx, &Event::Scroll(e), scope);
    }

    /// Advance the page burst on each timer tick; stops when the held page
    /// count is exhausted.
    pub(super) fn handle_page_burst(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(timer) = self.page_timer else {
            return;
        };
        if timer.is_event(event).is_none() {
            return;
        }
        let done = if let Some(burst) = &mut self.page_burst {
            burst.left -= 1;
            burst.left == 0
        } else {
            true
        };
        self.dispatch_page_tick(cx, scope);
        if done {
            self.cancel_page_burst(cx);
        }
    }

    /// Stop the page burst; the scroll position is already where the burst
    /// left it (each tick applies instantly).
    pub(super) fn cancel_page_burst(&mut self, cx: &mut Cx) {
        if let Some(t) = self.page_timer.take() {
            cx.stop_timer(t);
        }
        self.page_burst = None;
    }

    /// Track held WASD/QE keys in the `key_move` bitmask; the first key press
    /// starts the animation timer, which drives the motion until all keys up.
    pub(super) fn set_key_move(&mut self, code: KeyCode, down: bool, cx: &mut Cx) {
        let mask = match code {
            KeyCode::KeyW => KEY_W,
            KeyCode::KeyA => KEY_A,
            KeyCode::KeyS => KEY_S,
            KeyCode::KeyD => KEY_D,
            KeyCode::KeyQ => KEY_Q,
            KeyCode::KeyE => KEY_E,
            _ => return,
        };
        let keys = if down {
            self.key_move | mask
        } else {
            self.key_move & !mask
        };
        if keys != self.key_move {
            self.key_move = keys;
            if keys != 0 {
                self.start_zoom_anim(cx);
            }
        }
    }

    /// Track held arrow keys in the `arrow_move` (move) or `resize_arrows`
    /// (Alt/Option+arrow resize) bitmask; the first press re-anchors the
    /// selected cards' targets and starts the animation timer that eases
    /// toward them.
    pub(super) fn set_arrow(&mut self, code: KeyCode, down: bool, resize: bool, cx: &mut Cx) {
        let Some(mask) = arrow_mask(code) else {
            return;
        };
        let field = if resize {
            &mut self.resize_arrows
        } else {
            &mut self.arrow_move
        };
        let bits = if down {
            *field | mask
        } else {
            *field & !mask
        };
        if bits != *field {
            *field = bits;
            if down {
                self.rebuild_targets();
                self.start_zoom_anim(cx);
            }
        }
    }

    /// Re-anchor rect_targets to the selected cards' current geometry, so a
    /// stale in-flight target can't yank a card after a selection change or
    /// a drag.
    pub(super) fn rebuild_targets(&mut self) {
        self.rect_targets = self
            .data
            .as_ref()
            .map(|_| self.selected.iter().map(|&i| (i, self.card_rect(i))).collect())
            .unwrap_or_default();
    }

    /// Re-anchor after a selection change; restart the timer if arrow keys
    /// are still held so they keep driving the new selection.
    pub(super) fn reanchor_cards(&mut self, cx: &mut Cx) {
        self.rebuild_targets();
        if self.arrow_move != 0 || self.resize_arrows != 0 {
            self.start_zoom_anim(cx);
        }
    }
}

fn arrow_mask(code: KeyCode) -> Option<u8> {
    Some(match code {
        KeyCode::ArrowUp => ARROW_UP,
        KeyCode::ArrowDown => ARROW_DOWN,
        KeyCode::ArrowLeft => ARROW_LEFT,
        KeyCode::ArrowRight => ARROW_RIGHT,
        _ => return None,
    })
}
