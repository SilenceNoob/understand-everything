pub use makepad_widgets;

use makepad_widgets::*;

app_main!(App);

mod file_panel;
mod float_panel;
mod markdown_media;
mod mindmap;

use crate::file_panel::FilePanelWidgetRefExt;
use crate::float_panel::FloatPanelWidgetRefExt;
use crate::mindmap::MindMapWidgetRefExt;

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
    #[rust]
    map_opened: bool,
}

impl App {
    fn open_map(&mut self, cx: &mut Cx, map_file: &str) {
        self.ui.mind_map(cx, ids!(mindmap)).switch_map(cx, map_file);
        self.ui
            .file_panel(cx, ids!(file_panel))
            .set_current_map(cx, Some(map_file));
        self.map_opened = true;
        self.sync_title(cx);
    }

    fn sync_title(&mut self, cx: &mut Cx) {
        let title = if self.map_opened {
            self.ui
                .mind_map(cx, ids!(mindmap))
                .current_map_file()
                .map(|f| file_panel::display_name(&f))
                .unwrap_or_else(|| "Understand Everything".to_string())
        } else {
            "Understand Everything".to_string()
        };
        self.ui.label(cx, ids!(caption_label.label)).set_text(cx, &title);
    }
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
            // File panel tree: clicking a map switches the mindmap to it.
            if let Some(map_file) = self.ui.file_panel(cx, ids!(file_panel)).map_clicked(actions) {
                self.open_map(cx, &map_file);
            }
            // Context menu: create map / dir, delete map, rename.
            let base = crate::mindmap::app_base_dir();
            if let Some(map_file) = self.ui.file_panel(cx, ids!(file_panel)).create_map(actions) {
                std::fs::write(base.join(&map_file), crate::mindmap::new_map_json()).ok();
                self.open_map(cx, &map_file);
            }
            if let Some(dir) = self.ui.file_panel(cx, ids!(file_panel)).create_dir(actions) {
                std::fs::create_dir(base.join(&dir)).ok();
            }
            if let Some(rel) = self.ui.file_panel(cx, ids!(file_panel)).delete_entry(actions) {
                let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                if rel.ends_with('/') {
                    // Directory: maps/ dirs are deletable outright; a cards/
                    // dir also drops the referencing nodes from every map.
                    std::fs::remove_dir_all(base.join(&rel)).ok();
                    if rel.starts_with("cards/") {
                        crate::mindmap::remove_dir_nodes(&base, &rel);
                        // Drop ghost cards from the in-memory map so a later
                        // save can't resurrect the references.
                        mind_map.reload_map(cx);
                    }
                    // The current map may live inside the deleted dir.
                    if mind_map
                        .current_map_file()
                        .is_some_and(|c| c == rel || c.starts_with(&rel))
                    {
                        let next = file_panel::all_map_files(&base)
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| mindmap::MindMapData::DEFAULT_MAP.to_string());
                        self.open_map(cx, &next);
                    }
                } else {
                    std::fs::remove_file(base.join(&rel)).ok();
                    if mind_map.current_map_file().as_deref() == Some(rel.as_str()) {
                        // Switch to the first remaining map; none left → the
                        // default, whose failed load empties the canvas.
                        let next = file_panel::all_map_files(&base)
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| mindmap::MindMapData::DEFAULT_MAP.to_string());
                        self.open_map(cx, &next);
                    }
                }
            }
            if let Some((from, to)) = self.ui.file_panel(cx, ids!(file_panel)).rename_file(actions) {
                if std::fs::rename(base.join(&from), base.join(&to)).is_ok() {
                    // Renaming a card/dir breaks map references; rewrite them.
                    if from.starts_with("cards/") {
                        crate::mindmap::rewrite_node_paths(&base, &from, &to);
                    }
                    // Renaming the current map: keep showing it under the new
                    // name (content is unchanged, the saved view survives).
                    let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                    if mind_map.current_map_file().as_deref() == Some(from.as_str()) {
                        self.open_map(cx, &to);
                    }
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
