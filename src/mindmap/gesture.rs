
use crate::mindmap::*;
use crate::util::apply_resize;


impl MindMap {
    /// WASD pan / QE zoom keys. Skipped while a card is being edited
    /// (TextInput owns the keys), or the file panel is naming a new
    /// map/dir inline.

    /// Toggle card edit mode on its edit/done buttons.
    pub(crate) fn handle_edit_buttons(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Actions(actions) = event {
            let clicked: Vec<usize> = self
                .cards
                .iter()
                .enumerate()
                .filter_map(|(i, card)| {
                    let card = card.as_ref()?;
                    (card.button(cx, ids!(edit_btn)).clicked(actions)
                        || card.button(cx, ids!(done_btn)).clicked(actions))
                    .then_some(i)
                })
                .collect();
            for i in clicked {
                if self.editing_card == Some(i) {
                    self.commit_edit(cx);
                } else {
                    self.enter_edit(cx, i);
                }
            }
        }
    }

    // ponytail: canvas buttons get no reliable FingerHoverOut — hover
    // tracking is one shared slot that our own area overwrites every
    // MouseMove, and the base hover.off animation only advances on
    // NextFrame (Paint-driven, stops when the mouse is still). Snap the
    // hover off ourselves whenever the pointer is outside a visible
    // button; animator_cut is instant and needs no frame ticks.
    pub(crate) fn reset_stale_hover(&mut self, cx: &mut Cx, local: &Event) {
        let reset_visible_buttons = |cx: &mut Cx, over: Option<DVec2>| {
            for i in 0..self.cards.len() {
                // Only on-screen cards can hold a stale hover: the
                // cursor must be in the viewport to hover a button.
                if !self.view_rect.intersects(self.card_rect(i)) {
                    continue;
                }
                let card = &self.cards[i];
                let Some(card) = card.as_ref() else {
                    continue;
                };
                for id in [ids!(edit_btn), ids!(done_btn)] {
                    let btn = card.button(cx, id);
                    if !btn.visible() {
                        continue;
                    }
                    if let Some(p) = over {
                        if btn.area().rect(cx).contains(p) {
                            continue;
                        }
                    }
                    btn.reset_hover(cx);
                }
            }
        };
        // Track the drawn color-button hover (world coords, remapped event);
        // MouseLeave clears it.
        match local {
            Event::MouseMove(e) => {
                reset_visible_buttons(cx, Some(e.abs));
                self.set_color_btn_hover(cx, self.hit_color_button(e.abs));
            }
            Event::MouseLeave(_) => {
                reset_visible_buttons(cx, None);
                self.set_color_btn_hover(cx, None);
            }
            _ => {}
        }
    }

    /// Track the hovered color button; redraws only on state change.
    pub(crate) fn set_color_btn_hover(&mut self, cx: &mut Cx, gi: Option<usize>) {
        if self.hover_color_btn != gi {
            self.hover_color_btn = gi;
            self.redraw(cx);
        }
    }

    /// Primary-button press on the canvas: minimap drag, card resize/drag,
    /// group title drag (or frame-gap select+drag), or background marquee
    /// (box select).
    pub(crate) fn handle_finger_down(&mut self, cx: &mut Cx, fe: &FingerDownEvent, child_grabbed: bool) {
        // Any canvas press commits an open group rename (a click inside the
        // rename TextInput is captured and skipped).
        if self.editing_group.is_some() && !child_grabbed {
            self.commit_group_edit(cx);
        }
        // Same for the 序号 editor.
        if self.order_editing.is_some() && !child_grabbed {
            self.commit_order_edit(cx);
        }
        // Color picker popup: a press on a swatch applies the color; any
        // other press closes it. Either way the press is consumed.
        if let Some(gi) = self.color_popup {
            if let Some(i) = (0..GROUP_PRESET_COLORS.len())
                .find(|&i| self.popup_swatch_rect(i).contains(fe.abs))
            {
                if let Some(data) = &mut self.data {
                    data.groups[gi].color = Some(GROUP_PRESET_COLORS[i].to_string());
                    self.save_map();
                }
            }
            self.color_popup = None;
            self.redraw(cx);
            return;
        }
        // Panels (file/refs/float/dock) own their presses; the canvas must
        // not start a marquee/drag under them.
        if !crate::util::over_any_panel(fe.abs) {
            if self.minimap_rect.contains(fe.abs) {
                self.mm_dragging = true;
                self.navigate_minimap(cx, fe.abs);
            } else {
                let world = self.screen_to_world(fe.abs);
                if let Some((i, dir)) = self.resize_hit(world) {
                    // layout ops are disabled while a card is being edited
                    if self.editing_card.is_none() {
                        self.resize_card = Some(ResizeDrag { card: i, dir });
                        self.redraw(cx);
                    }
                } else if let Some(i) = self.hit_card(world) {
                    // keep the group when re-pressing an already
                    // selected card, so dragging moves them all
                    if !self.selected.contains(&i) {
                        self.selected = vec![i];
                        self.selected_groups.clear();
                        self.reanchor_cards(cx);
                    }
                    // Card dragging starts from the header only: the body is
                    // selectable text (划选生成子卡片), so a press there must
                    // not compete with the TextFlow selection drag.
                    let r = self.card_rect(i);
                    let in_header = world.y >= r.pos.y && world.y <= r.pos.y + 44.0;
                    if !child_grabbed && self.editing_card.is_none() && in_header {
                        // no card-internal widget (scrollbar, link) grabbed the press
                        self.drag_card = Some(i);
                        self.drag_last = world;
                    }
                    self.redraw(cx);
                } else if let Some(gi) = self.hit_color_button(world) {
                    if self.editing_card.is_none() {
                        let cards = {
                            let g = &self.data.as_ref().unwrap().groups[gi];
                            g.cards.clone()
                        };
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                        self.reanchor_cards(cx);
                        self.hover_color_btn = None;
                        self.color_popup = Some(gi);
                    }
                    self.redraw(cx);
                } else if let Some(gi) = self.hit_group_title(world) {
                    if self.editing_card.is_none() {
                        let cards = {
                            let g = &self.data.as_ref().unwrap().groups[gi];
                            g.cards.clone()
                        };
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                        self.reanchor_cards(cx);
                        if fe.tap_count >= 2 {
                            self.enter_group_edit(cx, gi);
                        } else if !child_grabbed {
                            self.drag_group = Some(gi);
                            self.drag_last = world;
                        }
                    }
                    self.redraw(cx);
                } else if let Some(gi) = self.hit_group_frame(world) {
                    // Any gap inside the group frame selects the group and
                    // drags it (same as the title bar, minus rename).
                    if self.editing_card.is_none() {
                        let cards = {
                            let g = &self.data.as_ref().unwrap().groups[gi];
                            g.cards.clone()
                        };
                        self.selected = cards;
                        self.selected_groups = vec![gi];
                        self.reanchor_cards(cx);
                        if !child_grabbed {
                            self.drag_group = Some(gi);
                            self.drag_last = world;
                        }
                    }
                    self.redraw(cx);
                } else {
                    self.cancel_zoom_anim(cx);
                    self.marquee = Some(Marquee {
                        start: world,
                        end: world,
                    });
                    self.redraw(cx);
                }
            }
        }
    }

    /// Right-button press: prepare a context menu on a card, or fall back to a
    /// pan if the drag exceeds a small threshold. Click outside panels/minimap
    /// closes any open color popup.
    pub(crate) fn handle_finger_down_secondary(&mut self, cx: &mut Cx, fe: &FingerDownEvent) {
        if self.editing_group.is_some() {
            self.commit_group_edit(cx);
        }
        if self.color_popup.is_some() {
            self.color_popup = None;
            self.redraw(cx);
            return;
        }
        if self.editing_card.is_some()
            || self.minimap_rect.contains(fe.abs)
            || crate::util::over_any_panel(fe.abs)
        {
            return;
        }
        let world = self.screen_to_world(fe.abs);
        let card = self.hit_card(world);
        self.sec_press = Some((fe.abs, card));
        if let Some(i) = card {
            if !self.selected.contains(&i) {
                self.selected = vec![i];
                self.selected_groups.clear();
                self.reanchor_cards(cx);
            }
        }
        self.redraw(cx);
    }

    /// Drag tracking: minimap nav, marquee growth, card resize/drag, pan, or
    /// converting a held right-button press into a pan once it moves enough.
    pub(crate) fn handle_finger_move(&mut self, cx: &mut Cx, fe: &FingerMoveEvent) {
        if let Some((start, _card)) = self.sec_press {
            if (fe.abs - start).length() >= 4.0 {
                self.sec_press = None;
                self.cancel_zoom_anim(cx);
                self.panning = true;
                self.pan_last = fe.abs;
            }
            return;
        }
        if self.mm_dragging {
            self.navigate_minimap(cx, fe.abs);
            return;
        }
        if let Some(m) = self.marquee {
            let world = self.screen_to_world(fe.abs);
            self.marquee = Some(Marquee { start: m.start, end: world });
            self.redraw(cx);
            return;
        }
        let world = self.screen_to_world(fe.abs);
        // ⌘/Ctrl: the dragged edge/corner snaps to the grid (anchor edge stays).
        let w = if self.ctrl_down { Self::snap_grid(world) } else { world };
        if let Some(r) = self.resize_card {
            if let Some(data) = &mut self.data {
                let node = &mut data.nodes[r.card];
                apply_resize(
                    &mut node.pos,
                    &mut node.size,
                    w,
                    r.dir,
                    dvec2(CARD_MIN_SIZE, CARD_MIN_SIZE),
                    dvec2(CARD_MAX_SIZE, CARD_MAX_SIZE),
                );
            }
            self.redraw(cx);
        } else if self.drag_card.is_some() {
            if let Some(data) = &mut self.data {
                let delta = w - self.drag_last;
                for &j in &self.selected {
                    data.nodes[j].pos += delta;
                    if self.ctrl_down {
                        data.nodes[j].pos = Self::snap_grid(data.nodes[j].pos);
                    }
                }
                self.drag_last = w;
            }
            self.redraw(cx);
        } else if let Some(gi) = self.drag_group {
            let delta = w - self.drag_last;
            self.move_group(gi, delta);
            if self.ctrl_down {
                let cards = self.group_subtree_cards(gi);
                if let Some(data) = &mut self.data {
                    for c in cards {
                        data.nodes[c].pos = Self::snap_grid(data.nodes[c].pos);
                    }
                }
            }
            self.drag_last = w;
            self.redraw(cx);
        } else if self.panning {
            self.pan += fe.abs - self.pan_last;
            self.pan_target = self.pan;
            self.pan_last = fe.abs;
            self.redraw(cx);
        }
    }

    /// End of a drag: clear transient state, save, and commit any marquee
    /// selection (every card whose rect touches the box; a tiny box, i.e. a
    /// mis-click, clears the selection). A right-button release without drag
    /// opens the card context menu (on a card) or the canvas card picker.
    pub(crate) fn handle_finger_up(&mut self, cx: &mut Cx) {
        if let Some((abs, card)) = self.sec_press.take() {
            if let Some(i) = card {
                self.open_card_menu(cx, abs, i);
            } else {
                self.picker_world = self.screen_to_world(abs);
                cx.widget_action(self.widget_uid(), MindMapAction::CanvasMenu(abs));
            }
            return;
        }
        self.panning = false;
        self.drag_card = None;
        self.drag_group = None;
        self.resize_card = None;
        self.mm_dragging = false;
        self.rebuild_targets();
        self.save_map();
        if let Some(m) = self.marquee.take() {
            let rect = Rect {
                pos: dvec2(m.start.x.min(m.end.x), m.start.y.min(m.end.y)),
                size: dvec2((m.start.x - m.end.x).abs(), (m.start.y - m.end.y).abs()),
            };
            if rect.size.x < 4.0 && rect.size.y < 4.0 {
                self.selected.clear();
                self.selected_groups.clear();
            } else if let Some(data) = &self.data {
                self.selected = (0..data.nodes.len())
                    .filter(|&i| rect.intersects(self.card_rect(i)))
                    .collect();
                self.selected_groups.clear();
            }
            self.reanchor_cards(cx);
            self.redraw(cx);
        }
    }

    /// Wheel zoom, swallowed over the minimap and any panel (file/refs/float
    /// — their content scrolls instead). Compact cards have no scrollable
    /// body, so wheel over them zooms like canvas.
    pub(crate) fn handle_finger_scroll(&mut self, cx: &mut Cx, fe: &FingerScrollEvent) {
        if !self.minimap_rect.contains(fe.abs)
            && fe.scroll.y != 0.0
            && !crate::util::over_any_panel(fe.abs)
        {
            let world = self.screen_to_world(fe.abs);
            if self.zoom < COMPACT_ZOOM || self.hit_card(world).is_none() {
                // Makepad's scroll convention is positive y = scroll
                // down/backward. Invert it so wheel up/forward (negative y)
                // zooms in and wheel down/backward zooms out.
                let factor = (1.0 - fe.scroll.y * 0.002).clamp(0.8, 1.25);
                let new_zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
                if (new_zoom - self.zoom).abs() > f64::EPSILON {
                    let w = self.screen_to_world(fe.abs);
                    self.pan_target = fe.abs - w * new_zoom;
                    self.zoom_target = new_zoom;
                    self.start_zoom_anim(cx);
                    self.redraw(cx);
                }
            }
        }
    }
}
