pub use makepad_widgets;

use makepad_widgets::*;

use std::sync::atomic::Ordering;

use crate::ai::{AIConfig, SseParser};

app_main!(App);

mod ai;
mod chat_list;
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

    let NewChatBtn = mod.widgets.ButtonFlatIcon{
        padding: Inset{left: 3, right: 3, top: 3, bottom: 3}
        margin: 0
        draw_bg +: {
            color: #1f2430
            color_hover: #232834
            color_down: #232834
            color_focus: #1f2430
            border_size: uniform(0.0)
        }
    }

    // Chat send/stop button: padded to match the input's initial height (44px).
    let SendBtn = mod.widgets.ButtonFlatIcon{
        padding: Inset{left: 10, right: 10, top: 14, bottom: 14}
        margin: 0
        draw_bg +: {
            color: #1f2430
            color_hover: #232834
            color_down: #232834
            color_focus: #1f2430
            border_size: uniform(0.0)
        }
    }

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
            width: 420
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
            body_box := mod.widgets.View{
                width: Fill
                height: Fit
                flow: Down
                body := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 14.0
                    draw_text.color: #aab0bc
                }
            }
            // AI settings form (visible only for the Setting popup).
            settings_form := mod.widgets.View{
                width: Fill
                height: Fit
                flow: Down
                spacing: 10
                visible: false
                key_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Down
                    spacing: 4
                    key_label := mod.widgets.Label{
                        width: Fill
                        text: "API Key"
                        draw_text.text_style.font_size: 13.0
                        draw_text.color: #aab0bc
                    }
                    key_input := mod.widgets.TextInput{
                        width: Fill
                        height: Fit
                        is_password: true
                        empty_text: "sk-..."
                    }
                }
                url_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Down
                    spacing: 4
                    url_label := mod.widgets.Label{
                        width: Fill
                        text: "BaseURL"
                        draw_text.text_style.font_size: 13.0
                        draw_text.color: #aab0bc
                    }
                    url_input := mod.widgets.TextInput{
                        width: Fill
                        height: Fit
                        empty_text: "https://api.deepseek.com"
                    }
                }
                model_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Down
                    spacing: 4
                    model_label := mod.widgets.Label{
                        width: Fill
                        text: "Model"
                        draw_text.text_style.font_size: 13.0
                        draw_text.color: #aab0bc
                    }
                    model_input := mod.widgets.TextInput{
                        width: Fill
                        height: Fit
                        empty_text: "deepseek-v4-flash"
                    }
                }
                thinking_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Down
                    spacing: 4
                    thinking_label := mod.widgets.Label{
                        width: Fill
                        text: "Thinking"
                        draw_text.text_style.font_size: 13.0
                        draw_text.color: #aab0bc
                    }
                    thinking_input := mod.widgets.DropDown{
                        labels: ["low", "high", "xhigh", "max"]
                        selected_item: 3
                    }
                }
                btn_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Right
                    spacing: 8
                    save_btn := mod.widgets.ButtonFlat{
                        width: Fit
                        text: "保存"
                    }
                    test_btn := mod.widgets.ButtonFlat{
                        width: Fit
                        text: "测试连接"
                    }
                }
                status := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #aab0bc
                }
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
                    ai_btn := mod.widgets.ButtonFlat{
                        text: "AI"
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
                    ai_panel := mod.widgets.FloatPanel{
                        panel_size: vec2(520.0, 700.0)
                        pin_bottom_right: false
                        // Multi-turn streaming chat; bubbles are the
                        // msg_00..msg_31 slots below the greeting.
                        content := mod.widgets.RoundedView{
                            width: Fill
                            height: Fill
                            flow: Down
                            show_bg: true
                            clip_x: true
                            clip_y: true
                            draw_bg +: {
                                color: #1f2430f2
                                border_radius: 8.0
                                border_size: 1.0
                                border_color: #ffffff14
                            }
                            header := mod.widgets.View{
                                width: Fill
                                height: (36.0)
                                flow: Right
                                padding: Inset{left: 12, right: 12}
                                align: Align{y: 0.5}
                                title := mod.widgets.Label{
                                    width: Fill
                                    text: "AI 助手"
                                    draw_text.text_style.font_size: 14.0
                                    draw_text.color: #e6e9f0
                                }
                                new_chat_btn := NewChatBtn{
                                    draw_icon +: {
                                        svg: crate_resource("self:resources/plus.svg")
                                        color: #aab0bc
                                    }
                                    icon_walk: Walk{width: 14, height: 14}
                                }
                            }
                            chat_list := mod.widgets.ChatList{
                                width: Fill
                                height: Fill
                                list := mod.widgets.PortalList{
                                    width: Fill
                                    height: Fill
                                    flow: Down
                                    padding: Inset{bottom: 12}
                                    UserLine := mod.widgets.View{
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        bubble := mod.widgets.RoundedView{
                                            width: Fill
                                            height: Fit
                                            flow: Down
                                            margin: Inset{left: 20, right: 4, top: 2, bottom: 2}
                                            padding: Inset{left: 12, right: 12, top: 8, bottom: 8}
                                            show_bg: true
                                            draw_bg +: {
                                                color: #2b3240f2
                                                border_radius: 8.0
                                                border_size: 1.0
                                                border_color: #ffffff14
                                            }
                                            line_md := mod.widgets.MarkdownMedia{
                                                padding: 0
                                                font_size: 13.0
                                                font_color: #e6e9f0
                                                paragraph_spacing: 8
                                                pre_code_spacing: 4
                                                heading_base_scale: 1.2
                                                draw_text +: {
                                                    color: #e6e9f0
                                                }
                                                text_style_normal: theme.font_regular{
                                                    font_size: 13.0
                                                }
                                                text_style_italic: theme.font_italic{
                                                    font_size: 13.0
                                                }
                                                text_style_bold: theme.font_bold{
                                                    font_size: 13.0
                                                }
                                                text_style_bold_italic: theme.font_bold_italic{
                                                    font_size: 13.0
                                                }
                                                text_style_fixed: theme.font_code{
                                                    font_size: 13.0
                                                }
                                            }
                                        }
                                    }
                                    AssistantLine := mod.widgets.View{
                                        width: Fill
                                        height: Fit
                                        bubble := mod.widgets.RoundedView{
                                            width: Fill
                                            height: Fit
                                            flow: Down
                                            margin: Inset{left: 4, right: 20, top: 2, bottom: 2}
                                            padding: Inset{left: 12, right: 12, top: 8, bottom: 8}
                                            show_bg: true
                                            draw_bg +: {
                                                color: #232834f2
                                                border_radius: 8.0
                                                border_size: 1.0
                                                border_color: #ffffff14
                                            }
                                            thinking_row := mod.widgets.View{
                                                width: Fill
                                                height: Fit
                                                flow: Down
                                                visible: false
                                                spacing: 2
                                                thinking_btn := mod.widgets.ButtonFlat{
                                                    width: Fit
                                                    text: "思考过程 ↓"
                                                    padding: Inset{left: 0, right: 0, top: 2, bottom: 2}
                                                    draw_bg +: {
                                                        color: #0000
                                                        color_hover: #ffffff0a
                                                        color_down: #ffffff0a
                                                        color_focus: #0000
                                                        border_size: uniform(0.0)
                                                    }
                                                    draw_text +: {
                                                        text_style: theme.font_regular{
                                                            font_size: 11.0
                                                        }
                                                        color: #8a91a0
                                                    }
                                                }
                                                thinking_body := mod.widgets.View{
                                                    width: Fill
                                                    height: Fit
                                                    flow: Down
                                                    thinking_md := mod.widgets.MarkdownMedia{
                                                        width: Fill
                                                        padding: Inset{left: 0, right: 0, top: 2, bottom: 4}
                                                        font_size: 12.0
                                                        font_color: #8a91a0
                                                        paragraph_spacing: 6
                                                        pre_code_spacing: 3
                                                        heading_base_scale: 1.1
                                                        draw_text +: {
                                                            color: #8a91a0
                                                        }
                                                        text_style_normal: theme.font_regular{
                                                            font_size: 12.0
                                                        }
                                                        text_style_italic: theme.font_italic{
                                                            font_size: 12.0
                                                        }
                                                        text_style_bold: theme.font_bold{
                                                            font_size: 12.0
                                                        }
                                                        text_style_bold_italic: theme.font_bold_italic{
                                                            font_size: 12.0
                                                        }
                                                        text_style_fixed: theme.font_code{
                                                            font_size: 12.0
                                                        }
                                                    }
                                                }
                                            }
                                            line_md := mod.widgets.MarkdownMedia{
                                                padding: 0
                                                font_size: 13.0
                                                font_color: #aab0bc
                                                paragraph_spacing: 8
                                                pre_code_spacing: 4
                                                heading_base_scale: 1.2
                                                draw_text +: {
                                                    color: #aab0bc
                                                }
                                                text_style_normal: theme.font_regular{
                                                    font_size: 13.0
                                                }
                                                text_style_italic: theme.font_italic{
                                                    font_size: 13.0
                                                }
                                                text_style_bold: theme.font_bold{
                                                    font_size: 13.0
                                                }
                                                text_style_bold_italic: theme.font_bold_italic{
                                                    font_size: 13.0
                                                }
                                                text_style_fixed: theme.font_code{
                                                    font_size: 13.0
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                                input_row := mod.widgets.View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: 8
                                padding: Inset{left: 12, right: 12, bottom: 12}
                                align: Align{y: 1.0}
                                chat_input := mod.widgets.TextInput{
                                    width: Fill
                                    height: Fit{min: FitBound.Abs(44.0), max: FitBound.Abs(120.0)}
                                    is_multiline: true
                                    submit_on_enter: true
                                    empty_text: "输入消息…"
                                }
                                send_btn := SendBtn{
                                    draw_icon +: {
                                        svg: crate_resource("self:resources/send.svg")
                                        color: #aab0bc
                                    }
                                    icon_walk: Walk{width: 16, height: 16}
                                }
                                stop_btn := SendBtn{
                                    visible: false
                                    draw_icon +: {
                                        svg: crate_resource("self:resources/stop.svg")
                                        color: #aab0bc
                                    }
                                    icon_walk: Walk{width: 16, height: 16}
                                }
                            }
                                ctx_row := mod.widgets.View{
                                width: Fill
                                height: Fit
                                flow: Right
                                padding: Inset{left: 12, right: 12, bottom: 10}
                                ctx_label := mod.widgets.Label{
                                    width: Fit
                                    height: Fit
                                    text: "Context: 0 / 1M (0%)"
                                    draw_text.text_style.font_size: 10.0
                                    draw_text.color: #7a8192
                                }
                                model_label := mod.widgets.Label{
                                    width: Fit
                                    height: Fit
                                    text: "Model: -"
                                    draw_text.text_style.font_size: 10.0
                                    draw_text.color: #7a8192
                                }
                                thinking_label := mod.widgets.Label{
                                    width: Fit
                                    height: Fit
                                    text: ""
                                    draw_text.text_style.font_size: 10.0
                                    draw_text.color: #7a8192
                                }
                            }
                        }
                    }
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
    #[rust]
    ai_config: AIConfig,
    /// True while a "测试连接" request is in flight.
    #[rust]
    testing: bool,
    /// Request id of the in-flight test request.
    #[rust]
    test_id: LiveId,
    /// Chat history as messages, newest last. Bounded by the context window
    /// (see CONTEXT_WINDOW), not by a fixed message count.
    #[rust]
    chat_history: Vec<crate::chat_list::ChatMsg>,
    /// UI-only messages (warnings) shown under the history: they are not
    /// sent to the model and do not count towards context usage.
    #[rust]
    chat_extra: Vec<(String, String)>,
    /// True once the 80% context warning has been shown (reset under 80%).
    #[rust]
    ctx_warned: bool,
    /// True while a chat reply is streaming in.
    #[rust]
    chat_pending: bool,
    /// Request id of the in-flight chat request.
    #[rust]
    chat_id: LiveId,
    /// Assistant text accumulated so far for the in-flight reply.
    #[rust]
    chat_buf: String,
    /// Assistant thinking chain accumulated so far for the in-flight reply.
    #[rust]
    chat_think: String,
    /// Incremental SSE decoder for the in-flight reply.
    #[rust]
    chat_parser: SseParser,
    /// The ai_panel's ChatList widget, resolved via live child traversal
    /// (the widget-tree graph can't see deep into FloatPanel subtrees).
    #[rust]
    chat_list_ref: Option<WidgetRef>,
    /// The ai_panel's ctx_row View, resolved the same way.
    #[rust]
    ctx_row_ref: Option<WidgetRef>,
    /// The ai_panel's input_row View, resolved the same way.
    #[rust]
    input_row_ref: Option<WidgetRef>,
}

/// Format a token count compactly: 860, 2.4K, 1M.
fn fmt_tokens(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.0}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

impl App {
    /// Current config as typed in the settings form (empty base_url/model
    /// fall back to the DeepSeek defaults).
    fn form_config(&self, cx: &Cx) -> AIConfig {
        let p = self.ui.view(cx, ids!(setting_popup));
        let mut cfg = self.ai_config.clone();
        cfg.api_key = p.text_input(cx, ids!(key_input)).text();
        let base_url = p.text_input(cx, ids!(url_input)).text();
        let model = p.text_input(cx, ids!(model_input)).text();
        if !base_url.trim().is_empty() {
            cfg.base_url = base_url.trim().to_string();
        }
        if !model.trim().is_empty() {
            cfg.model = model.trim().to_string();
        }
        cfg.thinking = p.drop_down(cx, ids!(thinking_input)).selected_label();
        cfg
    }

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

    /// Append a (role, content) message to the history and re-render.
    fn push_chat_msg(&mut self, cx: &mut Cx, role: &str, content: &str) {
        self.chat_history.push(crate::chat_list::ChatMsg {
            role: role.to_string(),
            content: content.to_string(),
            thinking: String::new(),
            thinking_open: true,
        });
        self.render_msgs(cx);
    }

    /// Push an assistant message that carries a thinking chain.
    fn push_chat_msg_thinking(&mut self, cx: &mut Cx, content: &str, thinking: &str) {
        self.chat_history.push(crate::chat_list::ChatMsg {
            role: "assistant".to_string(),
            content: content.to_string(),
            thinking: thinking.to_string(),
            thinking_open: true,
        });
        self.render_msgs(cx);
    }

    /// The ai_panel's ChatList widget, found by walking live children from
    /// the panel content (avoids the widget-tree graph, which does not index
    /// deep into FloatPanel subtrees).
    fn chat_list(&mut self, cx: &Cx) -> WidgetRef {
        if self.chat_list_ref.is_none() {
            let content = self.ui.float_panel(cx, ids!(ai_panel)).content(cx);
            let found = self.child_by_name(&content, live_id!(chat_list));
            if !found.is_empty() {
                self.chat_list_ref = Some(found);
            }
        }
        self.chat_list_ref.clone().unwrap_or_default()
    }

    /// The ai_panel's ctx_row View (cached), via live children from the
    /// panel content.
    fn ctx_row(&mut self, cx: &Cx) -> WidgetRef {
        if self.ctx_row_ref.is_none() {
            let content = self.ui.float_panel(cx, ids!(ai_panel)).content(cx);
            let found = self.child_by_name(&content, live_id!(ctx_row));
            if !found.is_empty() {
                self.ctx_row_ref = Some(found);
            }
        }
        self.ctx_row_ref.clone().unwrap_or_default()
    }

    /// The ai_panel's header View, via live children from the panel content.
    fn panel_header(&mut self, cx: &Cx) -> WidgetRef {
        let content = self.ui.float_panel(cx, ids!(ai_panel)).content(cx);
        self.child_by_name(&content, live_id!(header))
    }

    /// The ai_panel's input_row View (cached), via live children from the
    /// panel content.
    fn panel_input_row(&mut self, cx: &Cx) -> WidgetRef {
        if self.input_row_ref.is_none() {
            let content = self.ui.float_panel(cx, ids!(ai_panel)).content(cx);
            let found = self.child_by_name(&content, live_id!(input_row));
            if !found.is_empty() {
                self.input_row_ref = Some(found);
            }
        }
        self.input_row_ref.clone().unwrap_or_default()
    }

    /// Child of `parent` by name, via live children (graph-independent).
    fn child_by_name(&self, parent: &WidgetRef, id: LiveId) -> WidgetRef {
        let mut found = WidgetRef::empty();
        parent.try_children(&mut |name, child| {
            if name == id {
                found = child;
            }
        });
        found
    }

    /// Estimated tokens of the whole history.
    fn context_tokens(&self) -> usize {
        self.chat_history
            .iter()
            .map(|m| ai::estimate_tokens(&m.content) + ai::estimate_tokens(&m.thinking))
            .sum()
    }

    /// Refresh the "Context: N / 1M (P%)" label (gray, plus a one-shot 80%
    /// warning bubble).
    fn update_ctx_label(&mut self, cx: &mut Cx) {
        let tokens = self.context_tokens();
        let pct = tokens as f64 / ai::CONTEXT_WINDOW as f64;
        let warned = pct >= ai::WARN_RATIO;
        let used = fmt_tokens(tokens);
        let full = fmt_tokens(ai::CONTEXT_WINDOW);
        let text = if pct >= 1.0 {
            format!("Context: {used} / {full} (100%) — full, start a new chat")
        } else {
            format!("Context: {used} / {full} ({:.1}%)", pct * 100.0)
        };
        let ctx_row = self.ctx_row(cx);
        let label = self.child_by_name(&ctx_row, live_id!(ctx_label)).as_label();
        label.set_text(cx, &text);
        label.set_text_color(cx, Vec4f::from_u32(0x7a8192ff));
        self.child_by_name(&ctx_row, live_id!(model_label))
            .as_label()
            .set_text(cx, &format!("Model: {}", self.ai_config.model));
        self.child_by_name(&ctx_row, live_id!(thinking_label))
            .as_label()
            .set_text(cx, &format!("· thinking: {}", self.ai_config.thinking));
        // One-shot bubble when crossing the warning threshold.
        if warned && !self.ctx_warned {
            self.ctx_warned = true;
            self.chat_extra.push((
                "assistant".to_string(),
                format!(
                    "Context usage reached {:.0}% — consider starting a new chat soon.",
                    pct * 100.0
                ),
            ));
            self.render_msgs(cx);
        } else if !warned {
            self.ctx_warned = false;
        }
    }

    /// Sync the ChatList widget: history + in-flight reply + UI-only extras.
    fn render_msgs(&mut self, cx: &mut Cx) {
        let mut display: Vec<crate::chat_list::ChatMsg> = self.chat_history.clone();
        if self.chat_pending {
            display.push(crate::chat_list::ChatMsg {
                role: "assistant".to_string(),
                content: format!("思考中…{}", self.chat_buf),
                thinking: self.chat_think.clone(),
                thinking_open: true,
            });
        }
        for (role, content) in &self.chat_extra {
            display.push(crate::chat_list::ChatMsg {
                role: role.clone(),
                content: content.clone(),
                thinking: String::new(),
                thinking_open: true,
            });
        }
        if let Some(mut list) = self.chat_list(cx).borrow_mut::<crate::chat_list::ChatList>() {
            list.set_msgs(cx, &display);
        }
        self.update_ctx_label(cx);
        self.sync_send_btn(cx);
    }

    /// Send the chat input text: append to history and stream a request.
    /// Refuses (with a hint) when the context window is full.
    fn send_chat(&mut self, cx: &mut Cx, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.chat_pending {
            return;
        }
        let would_use = self.context_tokens() + ai::estimate_tokens(text);
        if would_use >= ai::CONTEXT_WINDOW {
            if !self
                .chat_extra
                .iter()
                .any(|(_, c)| c.contains("Context is full"))
            {
                self.chat_extra.push((
                    "assistant".to_string(),
                    "Context is full — click + to start a new chat.".to_string(),
                ));
                self.render_msgs(cx);
            }
            return;
        }
        self.push_chat_msg(cx, "user", text);
        let row = self.panel_input_row(cx);
        self.child_by_name(&row, live_id!(chat_input))
            .as_text_input()
            .set_text(cx, "");
        if self.ai_config.api_key.trim().is_empty() {
            self.push_chat_msg(cx, "assistant", "请先在 Setting 中配置 API Key");
            return;
        }
        self.chat_pending = true;
        self.chat_buf.clear();
        self.chat_think.clear();
        self.chat_parser = ai::SseParser::new();
        self.chat_id = LiveId::unique();
        let messages: Vec<(String, String)> = self
            .chat_history
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        ai::chat_stream_request(cx, self.chat_id, &self.ai_config, &messages);
        self.render_msgs(cx);
    }

    /// Start a fresh conversation: drop all history and extras.
    fn new_chat(&mut self, cx: &mut Cx) {
        if self.chat_pending {
            cx.cancel_http_request(self.chat_id);
        }
        self.chat_history.clear();
        self.chat_extra.clear();
        self.chat_buf.clear();
        self.chat_think.clear();
        self.chat_pending = false;
        self.ctx_warned = false;
        self.render_msgs(cx);
    }

    /// Cancel the in-flight reply, keeping whatever text arrived so far.
    fn stop_chat(&mut self, cx: &mut Cx) {
        if !self.chat_pending {
            return;
        }
        self.chat_pending = false;
        cx.cancel_http_request(self.chat_id);
        if !self.chat_buf.is_empty() {
            let buf = std::mem::take(&mut self.chat_buf);
            let think = std::mem::take(&mut self.chat_think);
            self.push_chat_msg_thinking(cx, &buf, &think);
        } else {
            self.chat_think.clear();
        }
        self.render_msgs(cx);
    }

    /// Send/stop button: show the stop icon while a reply streams.
    fn sync_send_btn(&mut self, cx: &mut Cx) {
        let row = self.panel_input_row(cx);
        self.child_by_name(&row, live_id!(send_btn))
            .set_visible(cx, !self.chat_pending);
        self.child_by_name(&row, live_id!(stop_btn))
            .set_visible(cx, self.chat_pending);
    }
}

impl MatchEvent for App {
    fn handle_http_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        if request_id != self.test_id || !self.testing {
            return;
        }
        self.testing = false;
        let status = response.status_code;
        let msg = match status {
            200 => "连接成功".to_string(),
            401 => "认证失败：API Key 无效".to_string(),
            _ => {
                let detail = response
                    .get_string_body()
                    .and_then(|b| ai::body_error_message(&b))
                    .unwrap_or_default();
                format!("连接失败 ({})：{}", status, detail)
            }
        };
        self.ui
            .label(cx, ids!(setting_popup.status))
            .set_text(cx, &msg);
    }

    fn handle_http_request_error(&mut self, cx: &mut Cx, request_id: LiveId, err: &HttpError) {
        if request_id == self.test_id && self.testing {
            self.testing = false;
            self.ui
                .label(cx, ids!(setting_popup.status))
                .set_text(cx, &format!("连接失败：{}", err.message));
            return;
        }
        if request_id == self.chat_id && self.chat_pending {
            self.chat_pending = false;
            self.push_chat_msg(cx, "assistant", &format!("请求失败：{}", err.message));
        }
    }

    /// A chunk of the streaming reply; feed it to the SSE parser and refresh
    /// the "思考中…" bubble with the accumulated text.
    fn handle_http_stream(&mut self, cx: &mut Cx, request_id: LiveId, data: &HttpResponse) {
        if request_id != self.chat_id || !self.chat_pending {
            return;
        }
        if let Some(bytes) = data.body() {
            let (content, thinking) = self.chat_parser.feed(bytes);
            for delta in content {
                self.chat_buf.push_str(&delta);
            }
            for delta in thinking {
                self.chat_think.push_str(&delta);
            }
            self.render_msgs(cx);
        }
    }

    fn handle_http_stream_complete(&mut self, cx: &mut Cx, request_id: LiveId, data: &HttpResponse) {
        if request_id != self.chat_id || !self.chat_pending {
            return;
        }
        self.chat_pending = false;
        let content = if data.status_code == 200 {
            self.chat_buf.clone()
        } else {
            // Non-200 stream: the body was raw JSON (not SSE), recovered from
            // the parser's raw buffer.
            let raw = self.chat_parser.raw();
            let detail = ai::body_error_message(&raw)
                .unwrap_or_else(|| raw.chars().take(200).collect());
            format!("请求失败 ({}): {}", data.status_code, detail)
        };
        if data.status_code == 200 {
            let buf = std::mem::take(&mut self.chat_buf);
            let think = std::mem::take(&mut self.chat_think);
            self.push_chat_msg_thinking(cx, &buf, &think);
        } else {
            self.chat_think.clear();
            self.push_chat_msg(cx, "assistant", &content);
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        crate::markdown_media::script_mod(vm);
        crate::mindmap::script_mod(vm);
        crate::float_panel::script_mod(vm);
        crate::file_panel::script_mod(vm);
        crate::chat_list::script_mod(vm);
        self::script_mod(vm)
    }

    fn after_new_from_script(_vm: &mut ScriptVm, app: &mut Self) {
        app.ai_config = ai::load_config();
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
                    p.view(cx, ids!(body_box)).set_visible(cx, false);
                    p.view(cx, ids!(settings_form)).set_visible(cx, true);
                    p.text_input(cx, ids!(key_input))
                        .set_text(cx, &self.ai_config.api_key);
                    p.text_input(cx, ids!(url_input))
                        .set_text(cx, &self.ai_config.base_url);
                    p.text_input(cx, ids!(model_input))
                        .set_text(cx, &self.ai_config.model);
                    let thinking_idx = ai::THINKING_LEVELS
                        .iter()
                        .position(|l| *l == self.ai_config.thinking)
                        .unwrap_or(3);
                    p.drop_down(cx, ids!(thinking_input))
                        .set_selected_item(cx, thinking_idx);
                    p.label(cx, ids!(status)).set_text(cx, "");
                }
            }
            if self.ui.button(cx, ids!(about_btn)).clicked(actions) {
                let p = self.ui.view(cx, ids!(about_popup));
                let show = !p.visible();
                p.set_visible(cx, show);
                if show {
                    self.ui.view(cx, ids!(setting_popup)).set_visible(cx, false);
                    p.label(cx, ids!(title)).set_text(cx, "About");
                    p.view(cx, ids!(settings_form)).set_visible(cx, false);
                    p.view(cx, ids!(body_box)).set_visible(cx, true);
                    p.label(cx, ids!(body_box.body)).set_text(
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
            if self.ui.button(cx, ids!(ai_btn)).clicked(actions) {
                let panel = self.ui.float_panel(cx, ids!(ai_panel));
                if panel.opened() {
                    panel.hide(cx);
                } else {
                    panel.show(cx);
                    self.ui.view(cx, ids!(setting_popup)).set_visible(cx, false);
                    self.ui.view(cx, ids!(about_popup)).set_visible(cx, false);
                }
            }
            if self.ui.button(cx, ids!(save_btn)).clicked(actions) {
                self.ai_config = self.form_config(cx);
                ai::save_config(&self.ai_config);
                self.update_ctx_label(cx);
                self.ui
                    .label(cx, ids!(setting_popup.status))
                    .set_text(cx, "已保存");
            }
            if self.ui.button(cx, ids!(test_btn)).clicked(actions) {
                if !self.testing {
                    self.testing = true;
                    self.test_id = LiveId::unique();
                    let cfg = self.form_config(cx);
                    ai::test_request(cx, self.test_id, &cfg);
                    self.ui
                        .label(cx, ids!(setting_popup.status))
                        .set_text(cx, "测试中…");
                }
            }
            let row = self.panel_input_row(cx);
            if self
                .child_by_name(&row, live_id!(send_btn))
                .as_button()
                .clicked(actions)
            {
                let text = self
                    .child_by_name(&row, live_id!(chat_input))
                    .as_text_input()
                    .text();
                self.send_chat(cx, &text);
            }
            if self
                .child_by_name(&row, live_id!(stop_btn))
                .as_button()
                .clicked(actions)
            {
                self.stop_chat(cx);
            }
            let header = self.panel_header(cx);
            if self
                .child_by_name(&header, live_id!(new_chat_btn))
                .as_button()
                .clicked(actions)
            {
                self.new_chat(cx);
            }
            // While the AI chat input holds key focus, the mindmap must skip
            // its keyboard shortcuts (WASD/arrows/Space would otherwise fight
            // the typing).
            let chat_input = self
                .child_by_name(&row, live_id!(chat_input))
                .as_text_input();
            for action in
                actions.filter_widget_actions_cast::<TextInputAction>(chat_input.widget_uid())
            {
                match action {
                    TextInputAction::KeyFocus => {
                        crate::float_panel::CHAT_INPUT_ACTIVE.store(true, Ordering::Relaxed)
                    }
                    TextInputAction::KeyFocusLost => {
                        crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed)
                    }
                    TextInputAction::Returned(text, _) => self.send_chat(cx, &text),
                    _ => {}
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
            for id in [ids!(setting_btn), ids!(about_btn), ids!(debug_btn), ids!(ai_btn)] {
                let a = self.ui.button(cx, id).area();
                if a.is_valid(cx) && a.rect(cx).contains(dq.abs) {
                    dq.response.set(WindowDragQueryResponse::Client);
                    break;
                }
            }
        }
    }
}
