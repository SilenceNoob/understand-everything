
use crate::gen::GenSection;
use crate::slide_panel::{menu_item_index, menu_rect, MENU_ITEM_H, MENU_PAD};
use crate::mindmap::*;
use crate::util::data_dir;


impl MindMap {
    /// Open the card context menu at the right-click screen position.
    pub(crate) fn open_card_menu(&mut self, cx: &mut Cx, abs: DVec2, card: usize) {
        let Some(data) = &self.data else { return };
        let path = data.nodes[card]
            .path
            .strip_prefix(&data_dir())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let view = self.area.rect(cx);
        self.menu_card = Some(card);
        self.menu_card_path = path;
        // Snapshot the body selection now: the menu press itself won't touch
        // it (TextFlow only clears on primary press / focus loss), but the
        // snapshot keeps the 生成子卡片 row and its payload consistent.
        self.menu_card_selection = self
            .cards
            .get(card)
            .and_then(|c| c.clone())
            .map(|c| c.markdown_media(cx, ids!(markdown)).selected_text(cx))
            .unwrap_or_default();
        // 生成学习路线 only for the root goal card, and only while it has no
        // children yet (v1 plans once; a planned map gets no re-plan entry).
        // Hidden too while a route plan is in flight. 生成子卡片 only while
        // the card body has a selection. Both rows sit at the end of the menu;
        // item4 = plan row, item5 = subcard row.
        self.menu_plan_row = data.root == Some(card)
            && data.nodes[card].children.is_empty()
            && !self.route_planning;
        self.menu_subcard_row = !self.menu_card_selection.trim().is_empty();
        self.menu_items = 4 + usize::from(self.menu_plan_row) + usize::from(self.menu_subcard_row);
        self.menu_rect = menu_rect(view, abs, self.menu_items);
        self.menu_hover = menu_item_index(self.menu_rect, self.menu_items, abs);
        self.sub_open = false;
        self.sub_hover = None;
        self.compute_sub_rect(view);
        self.menu_open = true;
        self.update_menu_hover(cx, abs);
        self.redraw(cx);
    }

    pub(crate) fn close_menu(&mut self, cx: &mut Cx) {
        self.menu_open = false;
        self.sub_open = false;
        self.menu_hover = None;
        self.sub_hover = None;
        self.menu_card = None;
        self.menu_card_path.clear();
        self.menu_card_selection.clear();
        self.sec_press = None;
        self.redraw(cx);
    }

    /// Remove the card from the current map: re-attach children to the parent,
    /// drop the node, rebuild index-dependent state, and save.
    pub(crate) fn remove_card(&mut self, cx: &mut Cx, i: usize) {
        let Some(data) = &mut self.data else { return };
        if data.root == Some(i) || !data.remove_node(i) {
            return;
        }
        self.cards.clear();
        self.edges = (0..data.edges().count())
            .map(|_| cx.with_vm(|vm| DrawEdge::script_new_with_default(vm)))
            .collect();
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

    /// Compute the submenu rect anchored to the right of the "生成" row.
    pub(crate) fn compute_sub_rect(&mut self, view: Rect) {
        let sub_w = 180.0;
        let sub_h = MENU_PAD * 2.0 + 8.0 * MENU_ITEM_H;
        let mut x = self.menu_rect.pos.x + self.menu_rect.size.x;
        let mut y = self.menu_rect.pos.y + MENU_PAD + 1.0 * MENU_ITEM_H;
        if x + sub_w > view.pos.x + view.size.x {
            x = (self.menu_rect.pos.x - sub_w).max(view.pos.x);
        }
        if y + sub_h > view.pos.y + view.size.y {
            y = (view.pos.y + view.size.y - sub_h).max(view.pos.y);
        }
        self.sub_rect = Rect {
            pos: dvec2(x, y),
            size: dvec2(sub_w, sub_h),
        };
    }

    pub(crate) fn update_menu_hover(&mut self, cx: &mut Cx, abs: DVec2) {
        let main = menu_item_index(self.menu_rect, self.menu_items, abs);
        let in_sub = self.sub_open && self.sub_rect.contains(abs);
        let sub_hover = if in_sub { menu_item_index(self.sub_rect, 8, abs) } else { None };
        let want_sub = main == Some(1) || in_sub;
        if self.menu_hover != main || self.sub_open != want_sub || self.sub_hover != sub_hover {
            self.menu_hover = main;
            self.sub_open = want_sub;
            self.sub_hover = sub_hover;
            self.redraw(cx);
        }
    }

    pub(crate) fn ctx_menu_widget(&mut self, cx: &mut Cx2d) -> Option<WidgetRef> {
        if self.ctx_menu_ref.is_some() {
            return self.ctx_menu_ref.clone();
        }
        let t = self.ctx_menu_template.as_ref()?;
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        self.ctx_menu_ref = Some(w.clone());
        Some(w)
    }

    pub(crate) fn sub_menu_widget(&mut self, cx: &mut Cx2d) -> Option<WidgetRef> {
        if self.sub_menu_ref.is_some() {
            return self.sub_menu_ref.clone();
        }
        let t = self.sub_menu_template.as_ref()?;
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        self.sub_menu_ref = Some(w.clone());
        Some(w)
    }

    pub(crate) fn drag_ghost_widget(&mut self, cx: &mut Cx2d) -> Option<WidgetRef> {
        if self.drag_ghost_ref.is_some() {
            return self.drag_ghost_ref.clone();
        }
        let t = self.drag_ghost_template.as_ref()?;
        let value = t.as_object().into();
        let w = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
        self.drag_ghost_ref = Some(w.clone());
        Some(w)
    }

    /// Draw the file-panel card drag preview: a translucent card at the
    /// pointer, sized like a real card at the current zoom. Only drawn while
    /// the pointer is over the canvas (panels show no ghost).
    pub(crate) fn draw_drag_ghost(&mut self, cx: &mut Cx2d, scope: &mut Scope, view: Rect) {
        let Some(drag) = crate::util::card_drag() else {
            return;
        };
        // `view` (the turtle's current viewport) not `self.area`: the latter
        // is only refreshed by end_turtle_with_area at the end of draw_walk,
        // so it is stale (or Empty on the very first frame) here.
        if !view.contains(drag.pos) || crate::util::over_any_panel(drag.pos) {
            return;
        }
        let Some(w) = self.drag_ghost_widget(cx) else {
            return;
        };
        let size = dvec2(CARD_W, CARD_H) * self.zoom;
        w.label(cx, ids!(ghost_title)).set_text(cx, &drag.title);
        // draw_walk_all: steps the DrawStep to completion, so the panel bg
        // and title label are actually emitted (a bare draw_walk() without
        // stepping issues no draw calls for child widgets).
        w.draw_walk_all(
            cx,
            scope,
            Walk {
                abs_pos: Some(drag.pos - size * 0.5),
                width: Size::Fixed(size.x),
                height: Size::Fixed(size.y),
                ..Walk::default()
            },
        );
    }

    /// Draw the card context menu and its submenu in screen coords, then
    /// register a window-wide modal area to capture all events while open.
    pub(crate) fn draw_card_menu(&mut self, cx: &mut Cx2d, scope: &mut Scope, _view: Rect) {
        if !self.menu_open {
            return;
        }
        if let Some(w) = self.ctx_menu_widget(cx) {
            // Conditional rows at the end of the menu: 生成学习路线 on the
            // root goal card, 生成子卡片 while the body has a selection.
            w.view(cx, ids!(item4)).set_visible(cx, self.menu_plan_row);
            w.view(cx, ids!(item5)).set_visible(cx, self.menu_subcard_row);
            let _ = w.draw_walk(
                cx,
                scope,
                Walk {
                    abs_pos: Some(self.menu_rect.pos),
                    width: Size::Fixed(self.menu_rect.size.x),
                    height: Size::Fixed(self.menu_rect.size.y),
                    ..Walk::default()
                },
            );
            if let Some(hover) = self.menu_hover {
                self.draw_menu_hl.draw_abs(
                    cx,
                    Rect {
                        pos: self.menu_rect.pos + dvec2(MENU_PAD, MENU_PAD + hover as f64 * MENU_ITEM_H),
                        size: dvec2(self.menu_rect.size.x - 2.0 * MENU_PAD, MENU_ITEM_H),
                    },
                );
            }
        }
        if self.sub_open {
            if let Some(w) = self.sub_menu_widget(cx) {
                let _ = w.draw_walk(
                    cx,
                    scope,
                    Walk {
                        abs_pos: Some(self.sub_rect.pos),
                        width: Size::Fixed(self.sub_rect.size.x),
                        height: Size::Fixed(self.sub_rect.size.y),
                        ..Walk::default()
                    },
                );
                if let Some(hover) = self.sub_hover {
                    self.draw_menu_hl.draw_abs(
                        cx,
                        Rect {
                            pos: self.sub_rect.pos + dvec2(MENU_PAD, MENU_PAD + hover as f64 * MENU_ITEM_H),
                            size: dvec2(self.sub_rect.size.x - 2.0 * MENU_PAD, MENU_ITEM_H),
                        },
                    );
                }
            }
        }
        let window = Rect {
            pos: DVec2::default(),
            size: cx.current_pass_size(),
        };
        cx.add_aligned_rect_area(&mut self.menu_modal_area, window);
    }

    /// Handle Esc / hover / menu item clicks while the card context menu is open.
    pub(crate) fn handle_card_menu_events(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        match event {
            Event::KeyDown(ke) if ke.key_code == KeyCode::Escape => {
                self.close_menu(cx);
                return;
            }
            Event::MouseMove(e) => {
                self.update_menu_hover(cx, e.abs);
                return;
            }
            _ => {}
        }
        match event.hits_with_capture_overload(cx, self.menu_modal_area, true) {
            Hit::FingerDown(fe) => self.on_menu_click(cx, fe.abs),
            _ => {}
        }
    }

    pub(crate) fn on_menu_click(&mut self, cx: &mut Cx, abs: DVec2) {
        if let Some(idx) = menu_item_index(self.menu_rect, self.menu_items, abs) {
            if idx == 0 {
                if let Some(i) = self.menu_card {
                    let root = self.data.as_ref().and_then(|d| d.root);
                    if root != Some(i) {
                        self.remove_card(cx, i);
                    }
                }
                self.close_menu(cx);
                return;
            }
            if idx == 1 {
                if !self.sub_open {
                    self.sub_open = true;
                    self.redraw(cx);
                }
                return;
            }
            if idx == 2 {
                if !self.menu_card_path.is_empty() {
                    cx.widget_action(self.widget_uid(), MindMapAction::Quiz(self.menu_card_path.clone()));
                }
                self.close_menu(cx);
                return;
            }
            if idx == 3 {
                // 设置序号: open the in-canvas order editor for the card.
                if let Some(i) = self.menu_card {
                    self.close_menu(cx);
                    self.start_order_edit(cx, i);
                }
                return;
            }
            // Rows 4+: 生成学习路线 (plan row) then 生成子卡片 (subcard row),
            // each only present when its flag is set.
            if idx >= 4 {
                let mut row = 4usize;
                if self.menu_plan_row {
                    if idx == row {
                        if !self.menu_card_path.is_empty() {
                            cx.widget_action(
                                self.widget_uid(),
                                MindMapAction::PlanRoute(self.menu_card_path.clone()),
                            );
                        }
                        self.close_menu(cx);
                        return;
                    }
                    row += 1;
                }
                if self.menu_subcard_row && idx == row {
                    if !self.menu_card_path.is_empty() {
                        cx.widget_action(
                            self.widget_uid(),
                            MindMapAction::GenSubCard(
                                self.menu_card_path.clone(),
                                self.menu_card_selection.clone(),
                            ),
                        );
                    }
                    self.close_menu(cx);
                    return;
                }
            }
        }
        if self.sub_open {
            if let Some(idx) = menu_item_index(self.sub_rect, 8, abs) {
                let section = match idx {
                    0 => GenSection::All,
                    1 => GenSection::Desc,
                    2 => GenSection::Plain,
                    3 => GenSection::PosExample,
                    4 => GenSection::NegExample,
                    5 => GenSection::Purpose,
                    6 => GenSection::Affect,
                    7 => GenSection::Affected,
                    _ => GenSection::All,
                };
                if !self.menu_card_path.is_empty() {
                    cx.widget_action(
                        self.widget_uid(),
                        MindMapAction::Generate(self.menu_card_path.clone(), section),
                    );
                }
                self.close_menu(cx);
                return;
            }
        }
        // Click outside the menu or submenu closes it.
        self.close_menu(cx);
    }
}
