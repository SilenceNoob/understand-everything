pub use makepad_widgets;

use makepad_widgets::*;

app_main!(App);

mod file_panel;
mod float_panel;
mod markdown_media;
mod mindmap;

use crate::float_panel::FloatPanelWidgetRefExt;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let PopupTemplate = mod.widgets.View{
        width: Fill
        height: Fill
        flow: Overlay
        align: Align{x: 0.5, y: 0.5}
        visible: false
        draw_bg +: {
            pixel: fn(){
                #000000cc
            }
        }
        panel := mod.widgets.RoundedView{
            width: 360
            height: Fit
            flow: Down
            padding: 20
            spacing: 12
            show_bg: true
            draw_bg +: {
                color: #1f2430
                border_radius: 8.0
                border_size: 1.0
                border_color: #ffffff14
            }
            title := mod.widgets.Label{
                width: Fill
                text: ""
                draw_text.text_style.font_size: 20.0
                draw_text.color: #e6e9f0
            }
            body := mod.widgets.Label{
                width: Fill
                text: ""
                draw_text.text_style.font_size: 14.0
                draw_text.color: #aab0bc
            }
            close := mod.widgets.ButtonFlat{
                width: Fit
                text: "关闭"
            }
        }
    }

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(1440, 900)
                window.title: "Understand Everything"
                window.caption_bar_height_override: 34.0
                pass +: {
                    clear_color: #14171d
                }
                caption_bar := mod.widgets.View{
                    visible: false
                    flow: Right
                    height: 34
                    setting_btn := mod.widgets.ButtonFlat{
                        text: "Setting"
                        draw_bg +: {
                            color: #14171d
                            color_hover: #232834
                            color_down: #232834
                            color_focus: #232834
                            border_size: uniform(0.0)
                        }
                    }
                    about_btn := mod.widgets.ButtonFlat{
                        text: "About"
                        draw_bg +: {
                            color: #14171d
                            color_hover: #232834
                            color_down: #232834
                            color_focus: #232834
                            border_size: uniform(0.0)
                        }
                    }
                    debug_btn := mod.widgets.ButtonFlat{
                        text: "Debug"
                        draw_bg +: {
                            color: #14171d
                            color_hover: #232834
                            color_down: #232834
                            color_focus: #232834
                            border_size: uniform(0.0)
                        }
                    }
                    caption_label := mod.widgets.View{
                        width: Fill
                        height: Fill
                        align: Center
                        // Balance the padding.left that sync_caption_centering
                        // applies (windows_buttons width = 3x46) so the title
                        // stays truly centered with the left menu buttons.
                        padding: Inset{right: 138}
                        label := mod.widgets.Label{
                            text: ""
                        }
                    }
                    windows_buttons := mod.widgets.View{
                        width: Fit
                        height: Fit
                        min := mod.widgets.DesktopButton{
                            draw_bg.button_type: DesktopButtonType.WindowsMin
                            width: 46 height: 29
                            draw_bg +: {
                                color: #000, color_hover: #000, color_down: #000
                                bg_color_hover: #E9E9E9, bg_color_down: #CCCCCC
                            }
                        }
                        max := mod.widgets.DesktopButton{
                            draw_bg.button_type: DesktopButtonType.WindowsMax
                            width: 46 height: 29
                            draw_bg +: {
                                color: #000, color_hover: #000, color_down: #000
                                bg_color_hover: #E9E9E9, bg_color_down: #CCCCCC
                            }
                        }
                        close := mod.widgets.DesktopButton{
                            draw_bg.button_type: DesktopButtonType.WindowsClose
                            width: 46 height: 29
                            draw_bg +: {
                                color: #000, color_hover: #FFF, color_down: #FFF
                                bg_color_hover: #E81123, bg_color_down: #F1707A
                            }
                        }
                    }
                }
                body +: {
                    flow: Flow.Overlay
                    mindmap := mod.widgets.MindMap{}
                    setting_popup := PopupTemplate{}
                    about_popup := PopupTemplate{}
                    float_panel := mod.widgets.FloatPanel{}
                    file_panel := mod.widgets.FilePanel{}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
}

impl MatchEvent for App {}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        crate::markdown_media::script_mod(vm);
        crate::mindmap::script_mod(vm);
        crate::float_panel::script_mod(vm);
        crate::file_panel::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Draw(de) = event {
            // PerfGraph reads per-frame samples from Cx::perf_monitor, but the
            // platform only feeds frame_boundary on macOS; without this the
            // graph on Linux would be an empty shell. One line of wiring.
            cx.perf_monitor.frame_boundary(de.time);
        }
        self.match_event(cx, event);
        if let Event::Actions(actions) = event {
            if self.ui.button(cx, ids!(setting_btn)).clicked(actions) {
                let p = self.ui.view(cx, ids!(setting_popup));
                let show = !p.visible();
                p.set_visible(cx, show);
                if show {
                    self.ui.view(cx, ids!(about_popup)).set_visible(cx, false);
                    p.label(cx, ids!(title)).set_text(cx, "Setting");
                    p.label(cx, ids!(body)).set_text(cx, "设置（开发中）");
                }
            }
            if self.ui.button(cx, ids!(about_btn)).clicked(actions) {
                let p = self.ui.view(cx, ids!(about_popup));
                let show = !p.visible();
                p.set_visible(cx, show);
                if show {
                    self.ui.view(cx, ids!(setting_popup)).set_visible(cx, false);
                    p.label(cx, ids!(title)).set_text(cx, "About");
                    p.label(cx, ids!(body)).set_text(
                        cx,
                        &format!(
                            "Understand Everything v{}\n把知识库渲染成可缩放的思维导图。",
                            env!("CARGO_PKG_VERSION")
                        ),
                    );
                }
            }
            if self
                .ui
                .view(cx, ids!(setting_popup))
                .button(cx, ids!(close))
                .clicked(actions)
            {
                self.ui.view(cx, ids!(setting_popup)).set_visible(cx, false);
            }
            if self
                .ui
                .view(cx, ids!(about_popup))
                .button(cx, ids!(close))
                .clicked(actions)
            {
                self.ui.view(cx, ids!(about_popup)).set_visible(cx, false);
            }
            if self.ui.button(cx, ids!(debug_btn)).clicked(actions) {
                let panel = self.ui.float_panel(cx, ids!(float_panel));
                if panel.opened() {
                    panel.hide(cx);
                } else {
                    panel.show(cx);
                    self.ui.view(cx, ids!(setting_popup)).set_visible(cx, false);
                    self.ui.view(cx, ids!(about_popup)).set_visible(cx, false);
                }
            }
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
        // The Window widget answers Caption for the whole caption bar (a
        // window-drag zone) BEFORE children see the event; this runs last
        // (last write wins, read by the platform after handle_event), so the
        // menu buttons inside the title bar stay clickable.
        if let Event::WindowDragQuery(dq) = event {
            for id in [ids!(setting_btn), ids!(about_btn), ids!(debug_btn)] {
                let a = self.ui.button(cx, id).area();
                if a.is_valid(cx) && a.rect(cx).contains(dq.abs) {
                    dq.response.set(WindowDragQueryResponse::Client);
                    break;
                }
            }
        }
    }
}
