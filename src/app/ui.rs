use makepad_widgets::*;


use crate::ai::{self};
use crate::app::{popup_child, popup_widget, toast_widget};
use crate::bottom_bar::BottomBarWidgetRefExt;
use crate::float_panel::FloatPanelWidgetRefExt;
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::App;

impl App {
    /// The popup widget (setting/about), walked through live children from
    /// the root — the widget-tree graph does not index widgets inside
    /// custom-widget content (BottomBar, FloatPanel, PopupPanel…), while
    /// live navigation always reflects the real tree.
    pub(crate) fn popup_widget(&self, id: LiveId) -> WidgetRef {
        popup_widget(&self.ui, id)
    }

    /// Descendant of a popup by live-child path (content → panel → …).
    pub(crate) fn popup_child(&self, popup_id: LiveId, path: &[LiveId]) -> WidgetRef {
        popup_child(&self.ui, popup_id, path)
    }

    /// The corner toast widget (feature-task notifications), via the body's
    /// live children like the popups.
    pub(crate) fn toast_widget(&self) -> WidgetRef {
        toast_widget(&self.ui)
    }

    /// Setting/About popup close buttons.
    pub(crate) fn handle_popup_closes(&mut self, cx: &mut Cx, actions: &Actions) {
        for id in [live_id!(setting_popup), live_id!(about_popup)] {
            if self
                .popup_child(id, &[live_id!(content), live_id!(panel), live_id!(close)])
                .as_button()
                .clicked(actions)
            {
                self.popup_widget(id).as_popup_panel().hide(cx);
            }
        }
    }

    /// Bottom-dock slot taps (0=Setting, 1=About, 2=Debug, 3=AI) — the
    /// dock hit-tests its own slots and reports taps here.
    pub(crate) fn handle_dock_clicks(&mut self, cx: &mut Cx) {
        let col = self.ui.bottom_bar(cx, ids!(bottom_bar)).take_clicked();
        let Some(col) = col else {
            return;
        };
        match col {
            0 => self.toggle_popup(cx, live_id!(setting_popup)),
            1 => self.toggle_popup(cx, live_id!(about_popup)),
            2 => self.toggle_debug_panel(cx),
            3 => self.toggle_ai_panel(cx),
            _ => {}
        }
    }

    /// Toggle a Setting/About popup, filling its body when shown.
    pub(crate) fn toggle_popup(&mut self, cx: &mut Cx, id: LiveId) {
        let p = self.popup_widget(id).as_popup_panel();
        let show = !p.opened();
        if show {
            self.popup_widget(live_id!(setting_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(about_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(startup_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(picker_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(confirm_popup)).as_popup_panel().hide(cx);
            p.show(cx);
            if id == live_id!(setting_popup) {
                let child = |path: &[LiveId]| self.popup_child(live_id!(setting_popup), path);
                child(&[live_id!(content), live_id!(panel), live_id!(title)])
                    .set_text(cx, "Setting");
                child(&[live_id!(content), live_id!(panel), live_id!(body_box)])
                    .set_visible(cx, false);
                child(&[live_id!(content), live_id!(panel), live_id!(settings_form)])
                    .set_visible(cx, true);
                child(&[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(key_row),
                    live_id!(key_input),
                ])
                .set_text(cx, &self.ai_config.api_key);
                child(&[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(url_row),
                    live_id!(url_input),
                ])
                .set_text(cx, &self.ai_config.base_url);
                child(&[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(model_row),
                    live_id!(model_input),
                ])
                .set_text(cx, &self.ai_config.model);
                let thinking_idx = ai::THINKING_LEVELS
                    .iter()
                    .position(|l| *l == self.ai_config.thinking)
                    .unwrap_or(3);
                child(&[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(thinking_row),
                    live_id!(thinking_input),
                ])
                .as_drop_down()
                .set_selected_item(cx, thinking_idx);
                child(&[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(status),
                ])
                .set_text(cx, "");
            } else {
                self.popup_child(
                    live_id!(about_popup),
                    &[live_id!(content), live_id!(panel), live_id!(title)],
                )
                .set_text(cx, "About");
                self.popup_child(
                    live_id!(about_popup),
                    &[live_id!(content), live_id!(panel), live_id!(settings_form)],
                )
                .set_visible(cx, false);
                self.popup_child(
                    live_id!(about_popup),
                    &[live_id!(content), live_id!(panel), live_id!(body_box)],
                )
                .set_visible(cx, true);
                self.popup_child(
                    live_id!(about_popup),
                    &[
                        live_id!(content),
                        live_id!(panel),
                        live_id!(body_box),
                        live_id!(body),
                    ],
                )
                .set_text(
                    cx,
                    &format!(
                        "Understand Everything v{}\n基于「渐构」学习观（参考 modevol.com《渐构：世界模型》）设计的桌面学习工具：把知识建构为判别模型（概念卡）与联结模型（知识卡），在可缩放思维导图上展开——路线规划先判别后联结、按模型类型出题测验、明确输入输出空间、已见/未见掌握状态标注。Rust + Makepad。",
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
        } else {
            p.hide(cx);
        }
    }

    /// Perf/chat float panel show-hide toggles.
    pub(crate) fn toggle_debug_panel(&mut self, cx: &mut Cx) {
        let panel = self.ui.float_panel(cx, ids!(float_panel));
        if panel.opened() {
            panel.hide(cx);
        } else {
            panel.show(cx);
            self.popup_widget(live_id!(setting_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(about_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(startup_popup)).as_popup_panel().hide(cx);
        }
    }

    pub(crate) fn toggle_ai_panel(&mut self, cx: &mut Cx) {
        let panel = self.ui.float_panel(cx, ids!(ai_panel));
        if panel.opened() {
            panel.hide(cx);
        } else {
            panel.show(cx);
            self.sync_jiangou_btns(cx);
            self.popup_widget(live_id!(setting_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(about_popup)).as_popup_panel().hide(cx);
            self.popup_widget(live_id!(startup_popup)).as_popup_panel().hide(cx);
        }
    }

    /// Settings form: save and connection test.
    pub(crate) fn handle_settings_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let status = self.popup_child(
            live_id!(setting_popup),
            &[
                live_id!(content),
                live_id!(panel),
                live_id!(settings_form),
                live_id!(status),
            ],
        );
        if self
            .popup_child(
                live_id!(setting_popup),
                &[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(btn_row),
                    live_id!(save_btn),
                ],
            )
            .as_button()
            .clicked(actions)
        {
            self.ai_config = self.form_config(cx);
            ai::save_config(&self.ai_config);
            self.update_ctx_label(cx);
            status.set_text(cx, "已保存");
        }
        if self
            .popup_child(
                live_id!(setting_popup),
                &[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(btn_row),
                    live_id!(test_btn),
                ],
            )
            .as_button()
            .clicked(actions)
        {
            if !self.testing {
                self.testing = true;
                self.test_id = LiveId::unique();
                let cfg = self.form_config(cx);
                ai::test_request(cx, self.test_id, &cfg);
                status.set_text(cx, "测试中…");
            }
        }
    }
}
