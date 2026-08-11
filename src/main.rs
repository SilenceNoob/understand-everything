pub use makepad_widgets;

use makepad_widgets::*;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::ai::{AIConfig, SseParser};
use crate::bottom_bar::BottomBarWidgetRefExt;
use crate::card_picker::{CardPickerWidgetRefExt, PickChoice};
use crate::gen::{
    generation_messages, parse_generation_output, parse_grades, parse_quiz, quiz_generation_messages,
    quiz_grading_messages, quiz_ready, upsert_sections, GenSection,
};
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::quiz_panel::{QuizPanelWidgetRefExt, QuizSubmission};
use crate::util::cached_widget;

app_main!(App);

mod ai;
mod bottom_bar;
mod card_picker;
mod chat_list;
mod file_panel;
mod float_panel;
mod gen;
mod markdown_media;
mod mindmap;
mod popup_panel;
mod quiz_panel;
mod rag;
mod refs_panel;
mod slide_panel;
mod util;

use crate::file_panel::FilePanelWidgetRefExt;
use crate::float_panel::FloatPanelWidgetRefExt;
use crate::mindmap::MindMapWidgetRefExt;
use crate::refs_panel::RefsPanelWidgetRefExt;

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

    // 渐构 section pills: gray (off) and blue (on) variants, swapped by
    // visibility like the send/stop buttons.
    let JiangouSectionBtn = mod.widgets.ButtonFlat{
        width: Fit
        margin: 0
        padding: Inset{left: 8, right: 8, top: 3, bottom: 3}
        draw_bg +: {
            color: #1f2430
            color_hover: #232834
            color_down: #232834
            color_focus: #1f2430
            border_size: uniform(0.0)
            border_radius: uniform(2.0)
        }
        draw_text +: {
            text_style: theme.font_regular{
                font_size: 8.0
            }
            color: #7a8192
        }
    }
    let JiangouSectionBtnOn = mod.widgets.ButtonFlat{
        width: Fit
        margin: 0
        padding: Inset{left: 8, right: 8, top: 3, bottom: 3}
        draw_bg +: {
            color: #4c6ef5
            color_hover: #5c7cfa
            color_down: #5c7cfa
            color_focus: #4c6ef5
            border_size: uniform(0.0)
            border_radius: uniform(2.0)
        }
        draw_text +: {
            text_style: theme.font_bold{
                font_size: 8.0
            }
            color: #ffffff
        }
    }

    // Diagnostic interview choice options: same look as the quiz panel's
    // OptionBtn — wrap text to the button width (Right wrap flow + Fill
    // label walk) and highlight the selection with a blue-bordered twin.
    let DiagOptionBtn = mod.widgets.ButtonFlat{
        width: Fill
        height: Fit
        flow: Flow.Right{wrap: true}
        padding: Inset{left: 8, right: 8, top: 4, bottom: 4}
        margin: Inset{right: 4}
        draw_bg +: {
            color: #0000
            color_hover: #ffffff10
            color_down: #ffffff18
            color_focus: #0000
            border_size: uniform(1.0)
            border_color: #ffffff30
            border_radius: uniform(4.0)
        }
        draw_text +: {
            text_style: theme.font_regular{font_size: 12.0}
            color: #e6e9f0
        }
    }
    let DiagOptionBtnOn = DiagOptionBtn{
        draw_bg +: {
            color: #4c6ef520
            color_hover: #5c7cfa30
            color_down: #5c7cfa30
            color_focus: #4c6ef520
            border_color: #4c6ef5
        }
        draw_text +: {
            text_style: theme.font_bold{font_size: 12.0}
            color: #e6e9f0
        }
    }

    // Popup content shared by the Setting/About PopupPanel instances; the
    // panel widget draws it window-sized only while opened (the old
    // visible:false View never drew, so its area stayed empty and
    // set_visible couldn't repaint it).
    let PopupPanelContent = mod.widgets.View{
        width: Fill
        height: Fill
        flow: Overlay
        align: Align{x: 0.5, y: 0.5}
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

    // Startup welcome page: asks what concept to learn, then hands off to
    // the AI panel. Shown once per launch (the PopupPanel starts closed;
    // App::handle_event opens it on the first draw).
    let StartupPageContent = mod.widgets.View{
        width: Fill
        height: Fill
        flow: Overlay
        align: Align{x: 0.5, y: 0.5}
        draw_bg +: {
            pixel: fn(){
                #000000cc
            }
        }
        panel := mod.widgets.RoundedView{
            width: 720
            height: Fit
            flow: Down
            padding: 28
            spacing: 14
            show_bg: true
            draw_bg +: {
                color: #1f2430
                border_radius: 8.0
                border_size: 1.0
                border_color: #ffffff14
            }
            goal_view := mod.widgets.View{
                width: Fill
                height: Fit
                flow: Down
                spacing: 14
                title := mod.widgets.Label{
                    width: Fill
                    text: "想学习什么？"
                    draw_text.text_style.font_size: 22.0
                    draw_text.color: #e6e9f0
                }
                hint := mod.widgets.Label{
                    width: Fill
                    text: "输入你想学会的内容，AI 会先诊断你的知识水平，再规划学习路线"
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #aab0bc
                }
                hint2 := mod.widgets.Label{
                    width: Fill
                    text: "提示：可先在右侧「参考资料」中添加文档，规划会更贴合你的资料"
                    draw_text.text_style.font_size: 12.0
                    draw_text.color: #7a8192
                }
                input_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Right
                    spacing: 8
                    start_input := mod.widgets.TextInput{
                        width: Fill
                        height: (44.0)
                        submit_on_enter: true
                        empty_text: "提出学习目标，如「学会浮力定律」…"
                    }
                    start_send_btn := SendBtn{
                        draw_icon +: {
                            svg: crate_resource("self:resources/send.svg")
                            color: #aab0bc
                        }
                        icon_walk: Walk{width: 16, height: 16}
                    }
                }
            }
            // Diagnostic interview phase: one adaptive question at a time.
            diag_view := mod.widgets.View{
                visible: false
                width: Fill
                height: Fit
                flow: Down
                spacing: 10
                diag_goal_label := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #7a8192
                }
                diag_status := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 13.0
                    draw_text.color: #aab0bc
                }
                diag_question := mod.widgets.Label{
                    width: Fill
                    text: ""
                    draw_text.text_style.font_size: 15.0
                    draw_text.color: #e6e9f0
                }
                diag_opt_ab := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Right
                    spacing: 4
                    opt0_off := DiagOptionBtn{ visible: false, text: "" }
                    opt0_on := DiagOptionBtnOn{ visible: false, text: "" }
                    opt1_off := DiagOptionBtn{ visible: false, text: "" }
                    opt1_on := DiagOptionBtnOn{ visible: false, text: "" }
                }
                diag_opt_cd := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Right
                    spacing: 4
                    opt2_off := DiagOptionBtn{ visible: false, text: "" }
                    opt2_on := DiagOptionBtnOn{ visible: false, text: "" }
                    opt3_off := DiagOptionBtn{ visible: false, text: "" }
                    opt3_on := DiagOptionBtnOn{ visible: false, text: "" }
                }
                // TextInput ignores `visible` (its draw_walk never checks
                // it), so the open-answer box toggles via this container.
                diag_input_box := mod.widgets.View{
                    visible: false
                    width: Fill
                    height: Fit
                    diag_input := mod.widgets.TextInput{
                        width: Fill
                        height: (44.0)
                        submit_on_enter: true
                        empty_text: "输入你的回答…"
                    }
                }
                diag_btn_row := mod.widgets.View{
                    width: Fill
                    height: Fit
                    flow: Right
                    spacing: 8
                    diag_unknown_btn := mod.widgets.ButtonFlat{
                        visible: false
                        width: Fit
                        text: "我不知道"
                    }
                    diag_submit_btn := mod.widgets.ButtonFlat{
                        width: Fit
                        text: "提交，下一题"
                    }
                }
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
                    caption_label := mod.widgets.View{
                        width: Fill
                        height: Fill
                        align: Center
                        // Balance the caption's right padding against the
                        // windows buttons (width = 3x46) so the title stays
                        // centered on Windows; on macOS the buttons are
                        // hidden, so the title sits slightly left of center.
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
                    setting_popup := mod.widgets.PopupPanel{
                        content := PopupPanelContent{}
                    }
                    about_popup := mod.widgets.PopupPanel{
                        content := PopupPanelContent{}
                    }
                    startup_popup := mod.widgets.PopupPanel{
                        content := StartupPageContent{}
                    }
                    quiz_popup := mod.widgets.PopupPanel{
                        content := mod.widgets.QuizPanel{}
                    }
                    picker_popup := mod.widgets.PopupPanel{
                        content := mod.widgets.CardPicker{}
                    }
                    confirm_popup := mod.widgets.PopupPanel{
                        content := mod.widgets.View{
                            width: Fill
                            height: Fill
                            flow: Overlay
                            align: Align{x: 0.5, y: 0.5}
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
                                    text: "删除卡片"
                                    draw_text.text_style.font_size: 18.0
                                    draw_text.color: #e6e9f0
                                }
                                card_name := mod.widgets.Label{
                                    width: Fill
                                    text: ""
                                    draw_text.text_style.font_size: 14.0
                                    draw_text.color: #e6e9f0
                                }
                                usage := mod.widgets.Label{
                                    width: Fill
                                    text: ""
                                    draw_text.text_style.font_size: 13.0
                                    draw_text.color: #aab0bc
                                }
                                btn_row := mod.widgets.View{
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    align: Align{x: 1.0, y: 0.5}
                                    spacing: 8
                                    delete_btn := mod.widgets.ButtonFlat{
                                        width: Fit
                                        text: "删除"
                                        padding: Inset{left: 14, right: 14, top: 6, bottom: 6}
                                        draw_bg +: {
                                            color: #e5484d
                                            color_hover: #f2555a
                                            color_down: #f2555a
                                            color_focus: #e5484d
                                            border_radius: uniform(4.0)
                                        }
                                        draw_text +: {
                                            text_style: theme.font_bold{font_size: 13.0}
                                            color: #ffffff
                                        }
                                    }
                                    cancel_btn := mod.widgets.ButtonFlat{
                                        width: Fit
                                        text: "取消"
                                        padding: Inset{left: 14, right: 14, top: 6, bottom: 6}
                                    }
                                }
                            }
                        }
                    }
                    float_panel := mod.widgets.FloatPanel{}
                    ai_panel := mod.widgets.FloatPanel{
                        panel_size: vec2(512.0, 800.0)
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
                                rag_status := mod.widgets.Label{
                                    width: Fit
                                    text: ""
                                    draw_text.text_style.font_size: 12.0
                                    draw_text.color: #8a91a0
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
                                        flow: Down
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
                                        copy_btn := mod.widgets.ButtonFlat{
                                            width: Fit
                                            text: ""
                                            icon_walk: Walk{width: 13, height: 13}
                                            padding: Inset{left: 6, right: 6, top: 2, bottom: 2}
                                            margin: Inset{left: 20, top: 4, bottom: 6}
                                            draw_bg +: {
                                                color: #0000
                                                color_hover: #ffffff0a
                                                color_down: #ffffff0a
                                                color_focus: #0000
                                                border_size: uniform(0.0)
                                            }
                                            draw_icon +: {
                                                svg: crate_resource("self:resources/copy.svg")
                                                color: #8a91a0
                                            }
                                        }
                                        copy_on_btn := mod.widgets.ButtonFlat{
                                            width: Fit
                                            visible: false
                                            text: ""
                                            icon_walk: Walk{width: 13, height: 13}
                                            padding: Inset{left: 6, right: 6, top: 2, bottom: 2}
                                            margin: Inset{left: 20, top: 4, bottom: 6}
                                            draw_bg +: {
                                                color: #0000
                                                color_hover: #ffffff0a
                                                color_down: #ffffff0a
                                                color_focus: #0000
                                                border_size: uniform(0.0)
                                            }
                                            draw_icon +: {
                                                svg: crate_resource("self:resources/check.svg")
                                                color: #4ade80
                                            }
                                        }
                                    }
                                    AssistantLine := mod.widgets.View{
                                        width: Fill
                                        height: Fit
                                        flow: Down
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
                                        copy_btn := mod.widgets.ButtonFlat{
                                            width: Fit
                                            text: ""
                                            icon_walk: Walk{width: 13, height: 13}
                                            padding: Inset{left: 6, right: 6, top: 2, bottom: 2}
                                            margin: Inset{left: 8, top: 4, bottom: 6}
                                            draw_bg +: {
                                                color: #0000
                                                color_hover: #ffffff0a
                                                color_down: #ffffff0a
                                                color_focus: #0000
                                                border_size: uniform(0.0)
                                            }
                                            draw_icon +: {
                                                svg: crate_resource("self:resources/copy.svg")
                                                color: #8a91a0
                                            }
                                        }
                                        copy_on_btn := mod.widgets.ButtonFlat{
                                            width: Fit
                                            visible: false
                                            text: ""
                                            icon_walk: Walk{width: 13, height: 13}
                                            padding: Inset{left: 6, right: 6, top: 2, bottom: 2}
                                            margin: Inset{left: 8, top: 4, bottom: 6}
                                            draw_bg +: {
                                                color: #0000
                                                color_hover: #ffffff0a
                                                color_down: #ffffff0a
                                                color_focus: #0000
                                                border_size: uniform(0.0)
                                            }
                                            draw_icon +: {
                                                svg: crate_resource("self:resources/check.svg")
                                                color: #4ade80
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
                                    empty_text: "提出要解释的概念…"
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
                            tools_row := mod.widgets.View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: 0
                                padding: Inset{left: 12, right: 12, bottom: 8}
                                desc_btn := JiangouSectionBtn{ text: "抽象描述" }
                                desc_on_btn := JiangouSectionBtnOn{ visible: false, text: "抽象描述" }
                                spacer1 := mod.widgets.View{ width: Fill, height: Fit }
                                plain_btn := JiangouSectionBtn{ text: "通俗描述" }
                                plain_on_btn := JiangouSectionBtnOn{ visible: false, text: "通俗描述" }
                                spacer2 := mod.widgets.View{ width: Fill, height: Fit }
                                pos_btn := JiangouSectionBtn{ text: "正例" }
                                pos_on_btn := JiangouSectionBtnOn{ visible: false, text: "正例" }
                                spacer3 := mod.widgets.View{ width: Fill, height: Fit }
                                neg_btn := JiangouSectionBtn{ text: "负例" }
                                neg_on_btn := JiangouSectionBtnOn{ visible: false, text: "负例" }
                                spacer4 := mod.widgets.View{ width: Fill, height: Fit }
                                use_btn := JiangouSectionBtn{ text: "作用" }
                                use_on_btn := JiangouSectionBtnOn{ visible: false, text: "作用" }
                                spacer5 := mod.widgets.View{ width: Fill, height: Fit }
                                affect_btn := JiangouSectionBtn{ text: "影响什么" }
                                affect_on_btn := JiangouSectionBtnOn{ visible: false, text: "影响什么" }
                                spacer6 := mod.widgets.View{ width: Fill, height: Fit }
                                affected_btn := JiangouSectionBtn{ text: "被什么影响" }
                                affected_on_btn := JiangouSectionBtnOn{ visible: false, text: "被什么影响" }
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
                    // Setting/About/Debug/AI buttons as an auto-hiding dock,
                    // bottom-centered at half the window width (25/50/25
                    // Fill weights keep it proportional on resize); slides up
                    // while the cursor is in the bottom-edge hot zone.
                    bottom_bar := mod.widgets.BottomBar{
                        content := mod.widgets.View{
                            width: Fill
                            height: Fill
                            flow: Right
                            align: Align{y: 1.0}
                            pad_l := mod.widgets.View{
                                width: Fill{weight: 25}
                                height: Fit
                            }
                            bar := mod.widgets.RoundedView{
                            // Width = 4×34 slots + 3×12 gaps + 2×6 padding,
                            // keep in sync with layout_dock's BASE_W/GAP.
                            width: (184.0)
                            height: (59.0)
                            // Bottom gap lives on the bar (the content
                            // margin is dropped by the manual walk).
                            margin: Inset{bottom: 14}
                            flow: Overlay
                            align: Align{y: 0.5}
                            spacing: 4
                            padding: Inset{left: 6, right: 6, top: 4, bottom: 4}
                            show_bg: true
                            draw_bg +: {
                                color: #1f2430f2
                                border_radius: 8.0
                                border_size: 1.0
                                border_color: #ffffff14
                            }
                        }
                        pad_r := mod.widgets.View{
                            width: Fill{weight: 25}
                            height: Fit
                        }
                        }
                    }
                    file_panel := mod.widgets.FilePanel{}
                    refs_panel := mod.widgets.RefsPanel{}
                }
            }
        }
    }
}

/// Child of `parent` by name, via live children (graph-independent).
fn child_by_name(parent: &WidgetRef, id: LiveId) -> WidgetRef {
    let mut found = WidgetRef::empty();
    parent.try_children(&mut |name, child| {
        if name == id {
            found = child;
        }
    });
    found
}

/// A deferred generation request waiting for hybrid RAG retrieval to finish.
struct GenWait {
    path: String,
    /// The sections to generate, in order (7 items for "所有").
    sections: Vec<GenSection>,
    title: String,
    rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    fallback: String,
    started: Instant,
}

/// A send_chat deferred until the background RAG retrieval answers (or the
/// timeout falls back to the BM25 context computed at send time).
struct RagWait {
    query: String,
    rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    fallback: String,
    started: Instant,
}

/// A learning-route plan request deferred until the hybrid RAG retrieval
/// answers (or times out to the BM25 fallback).
struct RouteWait {
    goal: String,
    /// Diagnostic transcript passed through to the route planner.
    diag: String,
    rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    fallback: String,
    started: Instant,
}

/// Route-plan retrieval fired when the diagnostic interview starts (the
/// query is just the goal, known minutes before the interview ends); adopted
/// by `start_route_plan` when the goal matches, dropped otherwise.
struct RoutePrefetch {
    goal: String,
    rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    started: Instant,
}

/// Startup popup phase: goal input vs the adaptive diagnostic interview.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartupPhase {
    Goal,
    Diag,
}

/// Upper bound for the interview: after this many answered rounds the
/// route is planned from the transcript alone (no further questions).
const MAX_DIAG_ROUNDS: usize = 6;

/// Upper bound for the retrieval pre-roll; on timeout the BM25 fallback
/// context fires the request. Measured hybrid ≈ 11s (5 × ~2s rerank + embed),
/// plus first-call reranker lazy load (~4s) and CPU contention from a
/// concurrent index build, so the budget carries headroom.
const RAG_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(20);
/// How often the app re-syncs the index from disk (catches card edits and
/// refs changes; fingerprint-diffed so unchanged snapshots are free).
const RAG_RESYNC_SECS: u64 = 5;
/// Token slack for the async hybrid context: 5 excerpts × 300 chars ≈ 1050
/// CJK-weighted tokens, not yet known at the context gauge.
const RAG_CONTEXT_SLACK: usize = 1100;

/// Lazy lookup of a child of the ai_panel content by name; a failed lookup
/// is never cached, so it retries.
fn cached_ai_child(cx: &Cx, ui: &WidgetRef, cache: &mut Option<WidgetRef>, id: LiveId) -> WidgetRef {
    cached_widget(cache, || {
        let content = ui.float_panel(cx, ids!(ai_panel)).content(cx);
        child_by_name(&content, id)
    })
    .unwrap_or_default()
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
    /// Route-plan stream state (mirrors the chat trio; parsed at stream end).
    #[rust]
    route_buf: String,
    #[rust]
    route_think: String,
    #[rust]
    route_parser: SseParser,
    /// True after one automatic retry of an unparseable route plan response
    /// (thinking models occasionally emit malformed JSON).
    #[rust]
    route_retried: bool,
    /// RAG context of the in-flight route request, reused by the retry.
    #[rust]
    route_context: String,
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
    /// The ai_panel's tools_row View (渐构 toggle), resolved the same way.
    #[rust]
    tools_row_ref: Option<WidgetRef>,
    /// RAG backend (two worker threads), created on first draw.
    #[rust]
    rag: Option<rag::RagService>,
    /// Drives status label refresh, periodic re-sync and retrieval polling.
    #[rust]
    rag_timer: Option<Timer>,
    /// Deferred chat send waiting on the background retrieval.
    #[rust]
    rag_wait: Option<RagWait>,
    /// Last periodic index re-sync time.
    #[rust]
    last_resync: Option<Instant>,
    /// Last text shown in the rag_status label (set_text only on change, to
    /// avoid a redraw every 250ms tick).
    #[rust]
    last_rag_label: String,
    /// Deferred card generation waiting on hybrid RAG retrieval.
    #[rust]
    gen_wait: Option<GenWait>,
    /// In-flight card generation request id and target card path.
    #[rust]
    gen_id: LiveId,
    #[rust]
    gen_path: String,
    /// Remaining sections of the current generation queue (empty for
    /// single-section jobs whose request is already in flight).
    #[rust]
    gen_sections: Vec<GenSection>,
    /// Total queue length, for the "生成中… (2/7)" progress indicator.
    #[rust]
    gen_total: usize,
    /// Context/title reused across the queue's sequential requests.
    #[rust]
    gen_context: String,
    #[rust]
    gen_title: String,
    /// In-flight 划选生成子卡片 request.
    #[rust]
    subcard_id: LiveId,
    /// Rel path of the parent card the subcard is generated under.
    #[rust]
    subcard_parent: String,
    /// In-flight quiz generation request.
    #[rust]
    quiz_id: LiveId,
    #[rust]
    quiz_path: Option<String>,
    #[rust]
    quiz_body: Option<String>,
    /// Deferred route planning waiting on hybrid RAG retrieval.
    #[rust]
    route_wait: Option<RouteWait>,
    /// Route-plan retrieval fired at diagnostic start; adopted by
    /// start_route_plan when the goal matches.
    #[rust]
    route_prefetch: Option<RoutePrefetch>,
    /// In-flight learning-route plan request.
    #[rust]
    route_id: LiveId,
    /// The goal being planned (shown on the root card).
    #[rust]
    route_goal: String,
    /// Rel path of the root goal card (set when the route starts).
    #[rust]
    route_root: String,
    /// Diagnostic transcript, recorded on the root card and reused when the
    /// route is re-planned later in the session.
    #[rust]
    route_diag: String,
    /// In-flight diagnostic question request (startup popup interview).
    #[rust]
    diag_id: LiveId,
    /// The goal being diagnosed.
    #[rust]
    diag_goal: String,
    /// Interview transcript (question + user answer), oldest first.
    #[rust]
    diag_history: Vec<(crate::gen::DiagQuestion, String)>,
    /// The question currently shown in the popup.
    #[rust]
    diag_current: Option<crate::gen::DiagQuestion>,
    /// Single-choice selection (option index).
    #[rust]
    diag_single: Option<usize>,
    /// Multi-choice selections.
    #[rust]
    diag_multi: [bool; 4],
    /// True after one automatic retry of an empty/unparseable question
    /// response (thinking models occasionally return empty content).
    #[rust]
    diag_retried: bool,
    /// In-flight quiz grading request.
    #[rust]
    grade_id: LiveId,
    /// Card awaiting delete confirmation (rel path), set while the confirm
    /// popup is open.
    #[rust]
    pending_delete_card: Option<String>,
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

/// Seed body for a route card: the archetype marker (drives 生成/测试 prompt
/// selection), the card's own input/output, and why it's in the route.
fn route_card_seed_body(rc: &crate::gen::RouteCard) -> String {
    let ctype = if rc.card_type == "knowledge" {
        "联结模型"
    } else {
        "概念"
    };
    let mut body = format!("#c 知识类型 {ctype}\n");
    if !rc.input.is_empty() || !rc.output.is_empty() {
        body.push_str(&format!("\n#c 输入输出\n输入：{}\n输出：{}\n", rc.input, rc.output));
    }
    if !rc.reason.is_empty() {
        body.push_str(&format!("\n#c 为何学\n{}\n", rc.reason));
    }
    body
}

impl App {
    /// The popup widget (setting/about), walked through live children from
    /// the root — the widget-tree graph does not index widgets inside
    /// custom-widget content (BottomBar, FloatPanel, PopupPanel…), while
    /// live navigation always reflects the real tree.
    fn popup_widget(&self, id: LiveId) -> WidgetRef {
        let main_window = child_by_name(&self.ui, live_id!(main_window));
        let body = child_by_name(&main_window, live_id!(body));
        child_by_name(&body, id)
    }

    /// Descendant of a popup by live-child path (content → panel → …).
    fn popup_child(&self, popup_id: LiveId, path: &[LiveId]) -> WidgetRef {
        let mut cur = self.popup_widget(popup_id);
        for &seg in path {
            cur = child_by_name(&cur, seg);
            if cur.is_empty() {
                break;
            }
        }
        cur
    }

    /// Setting/About popup close buttons.
    fn handle_popup_closes(&mut self, cx: &mut Cx, actions: &Actions) {
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
    fn handle_dock_clicks(&mut self, cx: &mut Cx) {
        let Some(col) = self.ui.bottom_bar(cx, ids!(bottom_bar)).take_clicked() else {
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
    fn toggle_popup(&mut self, cx: &mut Cx, id: LiveId) {
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
                        "Understand Everything v{}\n把知识库渲染成可缩放的思维导图。",
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
        } else {
            p.hide(cx);
        }
    }

    /// Perf/chat float panel show-hide toggles.
    fn toggle_debug_panel(&mut self, cx: &mut Cx) {
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

    fn toggle_ai_panel(&mut self, cx: &mut Cx) {
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
    fn handle_settings_actions(&mut self, cx: &mut Cx, actions: &Actions) {
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

    /// Chat panel: send/stop/new-chat buttons and the input's focus/return
    /// actions (focus hands off the mindmap's keyboard shortcuts).
    fn handle_chat_actions(&mut self, cx: &mut Cx, actions: &Actions) {
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
        let tools = self.tools_row(cx);
        for (id, _) in ai::JIANGOU_SECTIONS {
            let base = LiveId::from_str(&format!("{id}_btn"));
            let on_id = LiveId::from_str(&format!("{id}_on_btn"));
            if self.child_by_name(&tools, base).as_button().clicked(actions)
                || self.child_by_name(&tools, on_id).as_button().clicked(actions)
            {
                let on = self
                    .ai_config
                    .jiangou_sections
                    .iter()
                    .any(|s| s == id);
                if on {
                    self.ai_config.jiangou_sections.retain(|s| s != id);
                } else {
                    self.ai_config.jiangou_sections.push(id.to_string());
                }
                ai::save_config(&self.ai_config);
                self.sync_jiangou_btns(cx);
                break;
            }
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
        for action in actions.filter_widget_actions_cast::<TextInputAction>(chat_input.widget_uid())
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
    }

    /// Startup page: submit the concept (dismisses the page, opens the AI
    /// panel and sends it as the first message) or skip to the main UI.
    fn handle_startup_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let popup = self.popup_widget(live_id!(startup_popup));
        if !popup.as_popup_panel().opened() {
            return;
        }
        let input = self
            .popup_child(
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(goal_view), live_id!(input_row), live_id!(start_input)],
            )
            .as_text_input();
        for action in actions.filter_widget_actions_cast::<TextInputAction>(input.widget_uid()) {
            match action {
                TextInputAction::KeyFocus => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(true, Ordering::Relaxed)
                }
                TextInputAction::KeyFocusLost => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed)
                }
                TextInputAction::Returned(text, _) => self.submit_concept(cx, &text),
                _ => {}
            }
        }
        if self
            .popup_child(
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(goal_view), live_id!(input_row), live_id!(start_send_btn)],
            )
            .as_button()
            .clicked(actions)
        {
            self.submit_concept(cx, &input.text());
        }
        // Diagnostic interview: answer input focus, option toggles, submit.
        let diag_input = self
            .popup_child(
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box), live_id!(diag_input)],
            )
            .as_text_input();
        for action in actions.filter_widget_actions_cast::<TextInputAction>(diag_input.widget_uid()) {
            match action {
                TextInputAction::KeyFocus => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(true, Ordering::Relaxed)
                }
                TextInputAction::KeyFocusLost => {
                    crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed)
                }
                TextInputAction::Returned(_, _) => self.submit_diag_answer(cx),
                _ => {}
            }
        }
        for i in 0..4 {
            let off = self
                .popup_child(live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button();
            if off.clicked(actions) {
                match self.diag_current.as_ref().map(|q| q.kind.as_str()) {
                    Some("single") => self.diag_single = Some(i),
                    Some("multi") => self.diag_multi[i] = true,
                    _ => {}
                }
                self.sync_diag_options(cx);
            }
            let on = self
                .popup_child(live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button();
            if on.clicked(actions) {
                match self.diag_current.as_ref().map(|q| q.kind.as_str()) {
                    // clicking the already-selected option in single-choice keeps it
                    Some("multi") => {
                        self.diag_multi[i] = false;
                        self.sync_diag_options(cx);
                    }
                    _ => {}
                }
            }
        }
        if self
            .popup_child(
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row), live_id!(diag_submit_btn)],
            )
            .as_button()
            .clicked(actions)
        {
            self.submit_diag_answer(cx);
        }
        if self
            .popup_child(
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row), live_id!(diag_unknown_btn)],
            )
            .as_button()
            .clicked(actions)
        {
            self.submit_diag_unknown(cx);
        }
    }

    /// Start the adaptive diagnostic interview for `goal` (startup popup
    /// stays open, switches to the diag phase).
    fn submit_concept(&mut self, cx: &mut Cx, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.begin_diag(cx, &text);
    }

    /// Enter the diagnostic phase for `goal`: reset the session, switch the
    /// popup to the interview view, and fire the first question.
    fn begin_diag(&mut self, cx: &mut Cx, goal: &str) {
        if self.diag_id != LiveId::empty() || goal.trim().is_empty() {
            return;
        }
        if self.ai_config.api_key.trim().is_empty() {
            self.close_startup(cx);
            self.push_chat_msg(cx, "assistant", "请先在 Setting 中配置 API Key 再生成学习路线");
            self.ensure_ai_panel_open(cx);
            return;
        }
        self.reset_diag();
        self.diag_goal = goal.trim().to_string();
        // Prefetch the route-plan retrieval now: the query is just the goal,
        // so by the time the interview ends the result is ready and the
        // route request fires immediately. Only when the current map's index
        // actually has chunks (empty index would return no hits anyway).
        self.route_prefetch = None;
        let map_file = self.ui.mind_map(cx, ids!(mindmap)).current_map_file();
        if let (Some(rag), Some(map)) = (&self.rag, map_file) {
            if rag.models().is_some_and(|m| m.embedding_ready()) && rag.has_chunks_for(&map) {
                let rx = rag.retrieve(&self.diag_goal);
                self.route_prefetch = Some(RoutePrefetch {
                    goal: self.diag_goal.clone(),
                    rx,
                    started: Instant::now(),
                });
            }
        }
        self.set_startup_phase(cx, StartupPhase::Diag);
        self.popup_widget(live_id!(startup_popup)).as_popup_panel().show(cx);
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_goal_label)],
        )
        .as_label()
        .set_text(cx, &format!("学习目标：{}", self.diag_goal));
        self.send_diag_request(cx);
    }

    /// Fire the next diagnostic question request (BM25 context only — the
    /// interview must stay snappy; the final route request uses hybrid RAG).
    fn send_diag_request(&mut self, cx: &mut Cx) {
        if self.diag_id != LiveId::empty() || self.diag_goal.is_empty() {
            return;
        }
        let goal = self.diag_goal.clone();
        let ctx = self.rag_bm25_context(&goal);
        let (system, user) = crate::gen::diagnostic_messages(&goal, &ctx, &self.diag_history);
        self.diag_id = LiveId::unique();
        self.set_diag_status(cx, "正在出题…");
        // Clear the answered question so the popup shows a clean waiting
        // state until the next question arrives.
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_question)],
        )
        .as_label()
        .set_text(cx, "");
        for i in 0..4 {
            self.popup_child(live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button()
                .set_visible(cx, false);
            self.popup_child(live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button()
                .set_visible(cx, false);
        }
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box)],
        )
        .set_visible(cx, false);
        // The submit row stays hidden while 出题中 (clicking it with no
        // question loaded was a silent no-op); render_diag_question re-shows
        // it, and the failure paths re-show it as a retry entry.
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row)],
        )
        .set_visible(cx, false);
        ai::chat_completions(
            cx,
            self.diag_id,
            &self.ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    fn handle_diag_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.diag_id = LiveId::empty();
        // Popup closed mid-interview (user bailed): drop the session.
        if !self.popup_widget(live_id!(startup_popup)).as_popup_panel().opened() {
            self.reset_diag();
            return;
        }
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            self.set_diag_status(cx, &format!("出题失败 ({}): {}", response.status_code, detail));
            self.diag_unknown_btn(cx).set_visible(cx, false);
            self.diag_btn_row_visible(cx, true);
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match crate::gen::parse_diag_step(&content) {
            Ok(crate::gen::DiagStep::Question(q)) => {
                self.diag_current = Some(q);
                self.diag_single = None;
                self.diag_multi = [false; 4];
                self.render_diag_question(cx);
            }
            Ok(crate::gen::DiagStep::Done(summary)) => self.finish_diag(cx, &summary),
            Err(e) => {
                // Empty/unparseable content is often a transient thinking-model
                // response; retry once before surfacing the failure.
                if !self.diag_retried {
                    self.diag_retried = true;
                    self.send_diag_request(cx);
                    return;
                }
                let debug = ai::response_debug_preview(response);
                self.set_diag_status(cx, &format!("出题解析失败：{e}（{debug}）。可点「提交」重试"));
                self.diag_unknown_btn(cx).set_visible(cx, false);
                self.diag_btn_row_visible(cx, true);
            }
        }
    }

    /// Render the current question in the popup: status, question text,
    /// option buttons (or the open-answer input).
    fn render_diag_question(&mut self, cx: &mut Cx) {
        let Some(q) = &self.diag_current else { return };
        let n = self.diag_history.len() + 1;
        let status = if q.target.is_empty() {
            format!("第 {n} 题")
        } else {
            format!("第 {n} 题 · 探测：{}", q.target)
        };
        self.set_diag_status(cx, &status);
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_question)],
        )
        .as_label()
        .set_text(cx, &q.question);
        let is_open = q.kind == "open";
        for i in 0..4 {
            let off = self
                .popup_child(live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button();
            off.set_visible(cx, !is_open && i < q.options.len());
            self.popup_child(live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button()
                .set_visible(cx, false);
        }
        // Toggle the container, not the TextInput (which ignores `visible`).
        let input_box = self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box)],
        );
        input_box.set_visible(cx, is_open);
        if is_open {
            self.popup_child(
                live_id!(startup_popup),
                &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box), live_id!(diag_input)],
            )
            .as_text_input()
            .set_text(cx, "");
        }
        self.diag_btn_row_visible(cx, true);
        // 我不知道 is a choice-question escape hatch; hidden for open answers.
        self.diag_unknown_btn(cx).set_visible(cx, !is_open);
        self.sync_diag_options(cx);
    }

    /// Refresh the option buttons: text on both twins, selection shown by
    /// swapping to the highlighted on-variant (●/○ prefix is gone).
    fn sync_diag_options(&mut self, cx: &mut Cx) {
        let Some(q) = &self.diag_current else { return };
        for i in 0..q.options.len() {
            let selected = match q.kind.as_str() {
                "single" => self.diag_single == Some(i),
                "multi" => self.diag_multi[i],
                _ => false,
            };
            let off = self
                .popup_child(live_id!(startup_popup), &Self::opt_path(i, false))
                .as_button();
            let on = self
                .popup_child(live_id!(startup_popup), &Self::opt_path(i, true))
                .as_button();
            off.set_text(cx, &q.options[i]);
            on.set_text(cx, &q.options[i]);
            off.set_visible(cx, !selected);
            on.set_visible(cx, selected);
        }
    }

    fn set_diag_status(&self, cx: &mut Cx, text: &str) {
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_status)],
        )
        .as_label()
        .set_text(cx, text);
    }

    fn diag_btn_row_visible(&self, cx: &mut Cx, visible: bool) {
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row)],
        )
        .set_visible(cx, visible);
    }

    /// The 我不知道 button inside the submit row (choice questions only).
    fn diag_unknown_btn(&self, _cx: &Cx) -> WidgetRef {
        self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_btn_row), live_id!(diag_unknown_btn)],
        )
    }

    fn opt_id(i: usize, on: bool) -> LiveId {
        match (i, on) {
            (0, false) => live_id!(opt0_off),
            (0, true) => live_id!(opt0_on),
            (1, false) => live_id!(opt1_off),
            (1, true) => live_id!(opt1_on),
            (2, false) => live_id!(opt2_off),
            (2, true) => live_id!(opt2_on),
            (3, false) => live_id!(opt3_off),
            _ => live_id!(opt3_on),
        }
    }

    /// The row view hosting option `i` (0/1 in row ab, 2/3 in row cd).
    fn opt_row_id(i: usize) -> LiveId {
        if i < 2 {
            live_id!(diag_opt_ab)
        } else {
            live_id!(diag_opt_cd)
        }
    }

    /// Full lookup path for an option button: every segment is a direct
    /// child of the previous one (startup-popup lookups are only reliable
    /// along direct-child chains).
    fn opt_path(i: usize, on: bool) -> [LiveId; 5] {
        [
            live_id!(content),
            live_id!(panel),
            live_id!(diag_view),
            Self::opt_row_id(i),
            Self::opt_id(i, on),
        ]
    }

    fn option_letter(i: usize) -> String {
        char::from(b'A' + i as u8).to_string()
    }

    /// Collect the user's answer for the current question (None when they
    /// haven't answered yet).
    fn collect_diag_answer(&mut self) -> Option<String> {
        let q = self.diag_current.as_ref()?;
        match q.kind.as_str() {
            "single" => self.diag_single.map(Self::option_letter),
            "multi" => {
                let sel: Vec<usize> = (0..4)
                    .filter(|&i| self.diag_multi[i] && i < q.options.len())
                    .collect();
                if sel.is_empty() {
                    None
                } else {
                    Some(
                        sel.iter()
                            .map(|&i| Self::option_letter(i))
                            .collect::<Vec<_>>()
                            .join(","),
                    )
                }
            }
            "open" => {
                let text = self
                    .popup_child(
                        live_id!(startup_popup),
                        &[live_id!(content), live_id!(panel), live_id!(diag_view), live_id!(diag_input_box), live_id!(diag_input)],
                    )
                    .as_text_input()
                    .text();
                let t = text.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            _ => None,
        }
    }

    /// Append the current answer to the transcript and fire the next round;
    /// at the cap, plan the route from the transcript alone.
    fn submit_diag_answer(&mut self, cx: &mut Cx) {
        let Some(q) = self.diag_current.clone() else {
            // No question loaded: the button acts as 重试出题 after a
            // question-request failure.
            if self.diag_id == LiveId::empty() {
                self.diag_retried = false;
                self.send_diag_request(cx);
            }
            return;
        };
        let Some(ans) = self.collect_diag_answer() else {
            self.set_diag_status(
                cx,
                if q.kind == "open" {
                    "请输入你的回答"
                } else {
                    "请先选择答案"
                },
            );
            return;
        };
        self.record_diag_answer(cx, q, ans);
    }

    /// The 我不知道 escape hatch: records the round as unknown (never 答对),
    /// available only on choice questions with a loaded question.
    fn submit_diag_unknown(&mut self, cx: &mut Cx) {
        let Some(q) = self.diag_current.clone() else { return };
        if !matches!(q.kind.as_str(), "single" | "multi") {
            return;
        }
        self.record_diag_answer(cx, q, crate::gen::DIAG_UNKNOWN.to_string());
    }

    /// Record the (question, answer) round and advance: finish at the round
    /// cap, otherwise request the next question.
    fn record_diag_answer(&mut self, cx: &mut Cx, q: crate::gen::DiagQuestion, ans: String) {
        self.diag_history.push((q, ans));
        self.diag_current = None;
        if self.diag_history.len() >= MAX_DIAG_ROUNDS {
            self.finish_diag(cx, "");
        } else {
            self.send_diag_request(cx);
        }
    }

    /// Close the popup and plan the route with the interview transcript
    /// (plus the model's summary when it stopped early).
    fn finish_diag(&mut self, cx: &mut Cx, summary: &str) {
        let goal = self.diag_goal.clone();
        let mut diag = crate::gen::format_diag_history(&self.diag_history);
        if !summary.is_empty() {
            if !diag.is_empty() {
                diag.push('\n');
            }
            diag.push_str(&format!("诊断摘要：{summary}"));
        }
        self.reset_diag();
        self.close_startup(cx);
        self.start_route_plan(cx, &goal, &diag);
    }

    fn reset_diag(&mut self) {
        self.diag_id = LiveId::empty();
        self.diag_goal.clear();
        self.diag_history.clear();
        self.diag_current = None;
        self.diag_single = None;
        self.diag_multi = [false; 4];
        self.diag_retried = false;
    }

    /// Switch the startup popup between the goal-input and diag phases.
    fn set_startup_phase(&mut self, cx: &mut Cx, phase: StartupPhase) {
        let goal_view = self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(goal_view)],
        );
        goal_view.set_visible(cx, phase == StartupPhase::Goal);
        let diag_view = self.popup_child(
            live_id!(startup_popup),
            &[live_id!(content), live_id!(panel), live_id!(diag_view)],
        );
        diag_view.set_visible(cx, phase == StartupPhase::Diag);
    }

    /// Kick off learning-route planning for `goal`: ensure the root goal card
    /// exists (creating it on an empty map), then request the route JSON from
    /// the model. Re-planning is refused when the route already has cards.
    fn start_route_plan(&mut self, cx: &mut Cx, goal: &str, diagnostics: &str) {
        if self.route_id != LiveId::empty() || self.route_wait.is_some() {
            return;
        }
        if self.ai_config.api_key.trim().is_empty() {
            self.push_chat_msg(cx, "assistant", "请先在 Setting 中配置 API Key 再生成学习路线");
            self.ensure_ai_panel_open(cx);
            return;
        }
        let base = crate::util::app_base_dir();
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        let Some(map_file) = mind_map.current_map_file() else {
            return;
        };
        let existing = mind_map.card_rel_paths();
        if existing.len() > 1 {
            self.push_chat_msg(
                cx,
                "assistant",
                "当前地图已有学习路线卡片，暂不支持重新规划。",
            );
            self.ensure_ai_panel_open(cx);
            return;
        }
        // Root card: reuse the existing one (menu path) or create it fresh.
        let root_rel = if existing.len() == 1 {
            existing[0].clone()
        } else {
            let Some(rel) = self.create_route_card_file(&map_file, goal) else {
                return;
            };
            // The goal itself is the target knowledge (联结模型): it gets
            // the knowledge-card prompts for 生成/测试.
            let body = format!("#c 知识类型 联结模型\n\n#d 学习目标\n{goal}\n");
            if std::fs::write(base.join(&rel), body).is_err() {
                return;
            }
            if std::fs::write(base.join(&map_file), mindmap::route_map_json(goal, &rel, &[])).is_err() {
                return;
            }
            mind_map.reload_map(cx);
            rel
        };
        self.route_goal = goal.to_string();
        self.route_root = root_rel.clone();
        self.route_diag = diagnostics.to_string();
        self.route_retried = false;
        self.set_card_title_indicator(cx, &root_rel, Some("规划中…"));
        // Visible progress: the root card title flips to 规划中… and the AI
        // panel opens with a status line (it also hosts the success/failure
        // messages later, so the whole flow is one conversation).
        self.push_chat_msg(
            cx,
            "assistant",
            &format!("正在为「{goal}」规划学习路线（诊断 + 路线生成约需 1 分钟）…"),
        );
        self.ensure_ai_panel_open(cx);
        let fallback = self.rag_bm25_context(goal);
        let upgradeable = self.rag.as_ref().is_some_and(|r| {
            r.models().is_some_and(|m| m.embedding_ready()) && r.has_chunks_for(&map_file)
        });
        // A prefetch fired at diagnostic start (goal matches) skips the
        // retrieval wait entirely; a stale/goalless prefetch is dropped.
        let prefetch = match self.route_prefetch.take() {
            Some(p) if p.goal == goal => Some(p),
            _ => None,
        };
        if let Some(p) = prefetch {
            self.route_wait = Some(RouteWait {
                goal: goal.to_string(),
                diag: diagnostics.to_string(),
                rx: p.rx,
                fallback,
                started: p.started,
            });
        } else if upgradeable {
            let rx = self.rag.as_ref().unwrap().retrieve(goal);
            self.route_wait = Some(RouteWait {
                goal: goal.to_string(),
                diag: diagnostics.to_string(),
                rx,
                fallback,
                started: Instant::now(),
            });
        } else {
            self.send_route_request(cx, goal, diagnostics, &fallback);
        }
    }

    /// Create a route card file `cards/<map stem>/<prefix>-<title>.md`,
    /// unique-ified with a numeric suffix when the name is taken.
    fn create_route_card_file(&self, map_file: &str, title: &str) -> Option<String> {
        let stem = map_file
            .strip_prefix("maps/")
            .unwrap_or(map_file)
            .strip_suffix(".json")
            .unwrap_or(map_file);
        let safe = crate::file_panel::normalize_name(title, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let safe = safe.strip_suffix(".md").unwrap_or(&safe).to_string();
        let base = crate::util::app_base_dir();
        for n in 0.. {
            let fname = if n == 0 {
                format!("{safe}.md")
            } else {
                format!("{safe}-{n}.md")
            };
            let p = base.join("cards").join(stem).join(&fname);
            if !p.exists() {
                std::fs::create_dir_all(p.parent()?).ok()?;
                std::fs::write(&p, "").ok()?;
                return Some(format!("cards/{stem}/{fname}"));
            }
        }
        None
    }

    /// Fire the route-plan request (streaming; parsed when the stream ends).
    fn send_route_request(&mut self, cx: &mut Cx, goal: &str, diagnostics: &str, context: &str) {
        self.route_id = LiveId::unique();
        self.route_buf.clear();
        self.route_think.clear();
        self.route_parser = ai::SseParser::new();
        self.route_context = context.to_string();
        let (system, user) = crate::gen::route_plan_messages(goal, context, diagnostics);
        ai::chat_stream_request_max(
            cx,
            self.route_id,
            &self.ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    /// Live progress for route planning: a transient chat_extra assistant
    /// bubble updated per stream chunk, removed when the request ends. The
    /// thinking phase streams reasoning_content only, so show thinking chars
    /// until content starts (else the bubble sits at 0 字 for tens of seconds).
    fn update_route_progress(&mut self, cx: &mut Cx) {
        let text = if self.route_buf.is_empty() {
            format!(
                "正在规划学习路线…思考中（已生成 {} 字思考）",
                self.route_think.chars().count()
            )
        } else {
            format!(
                "正在规划学习路线…生成中（已接收 {} 字）",
                self.route_buf.chars().count()
            )
        };
        if let Some((_, c)) = self
            .chat_extra
            .iter_mut()
            .rev()
            .find(|(_, c)| c.starts_with("正在规划学习路线"))
        {
            *c = text;
        } else {
            self.chat_extra.push(("assistant".to_string(), text));
        }
        self.render_msgs(cx);
    }

    fn clear_route_progress(&mut self) {
        self.chat_extra.retain(|(_, c)| !c.starts_with("正在规划学习路线"));
    }

    /// Abort route planning and surface `msg` in the AI panel.
    fn abort_route(&mut self, cx: &mut Cx, msg: String) {
        self.route_wait = None;
        self.route_id = LiveId::empty();
        if !self.route_root.is_empty() {
            self.set_card_title_indicator(cx, &self.route_root, None);
        }
        self.push_chat_msg(cx, "assistant", &msg);
        self.ensure_ai_panel_open(cx);
    }

    /// Materialize a parsed route plan: write the card files, rebuild the map
    /// tree under the root goal card, and reload the canvas. `think` is the
    /// streamed reasoning chain, attached to the success message when present.
    fn apply_route_plan(&mut self, cx: &mut Cx, content: String, think: String) {
        let mut plan = match crate::gen::parse_route_plan(&content) {
            Ok(p) => p,
            Err(e) => {
                if !self.route_retried {
                    // Same goal/context/diag, fresh draw: intermittent
                    // malformed-JSON output usually parses on the retry.
                    self.route_retried = true;
                    let goal = self.route_goal.clone();
                    let diag = self.route_diag.clone();
                    let ctx = self.route_context.clone();
                    self.send_route_request(cx, &goal, &diag, &ctx);
                    return;
                }
                let preview: String = content.chars().take(200).collect();
                self.abort_route(cx, format!("路线解析失败：{e}\n原始输出预览：{preview}"));
                return;
            }
        };
        // The planner sometimes lists the goal itself as a card; the root
        // already exists, so drop those and re-attach their children to the
        // root (else a "-1" duplicate root gets created).
        crate::gen::drop_goal_duplicates(&mut plan, &self.route_goal);
        let base = crate::util::app_base_dir();
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        let Some(map_file) = mind_map.current_map_file() else {
            self.abort_route(cx, "路线生成失败：当前地图不存在".to_string());
            return;
        };
        if self.route_root.is_empty() {
            self.abort_route(cx, "路线生成失败：缺少根卡片".to_string());
            return;
        }
        // Card files, numbered by learning order (leaves first). Reuse an
        // existing library card when its title matches — never overwrite a
        // non-empty body (other maps may reference the file).
        let existing_cards = crate::file_panel::all_card_files(&base);
        // The root card file is part of the library scan above; pre-marking it
        // as used keeps a planned card whose title equals the goal from
        // reusing the root's file (which would spawn an identical duplicate
        // node connected to the root).
        let mut used: std::collections::HashSet<String> =
            [self.route_root.clone()].into_iter().collect();
        let mut cards: Vec<(String, String, String, Option<String>, Option<u32>)> = Vec::new();
        for (n, &ci) in crate::gen::learning_order(&plan.cards).iter().enumerate() {
            let rc = &plan.cards[ci];
            let rel = match crate::gen::match_card_path(&existing_cards, &rc.title)
                .filter(|p| !used.contains(p))
            {
                Some(p) => {
                    used.insert(p.clone());
                    let full = base.join(&p);
                    let body = std::fs::read_to_string(&full).unwrap_or_default();
                    if body.trim().is_empty() {
                        std::fs::write(&full, route_card_seed_body(rc)).ok();
                    }
                    p
                }
                None => {
                    let Some(rel) = self.create_route_card_file(&map_file, &rc.title) else {
                        self.abort_route(cx, format!("创建卡片失败：{}", rc.title));
                        return;
                    };
                    if std::fs::write(base.join(&rel), route_card_seed_body(rc)).is_err() {
                        self.abort_route(cx, format!("写入卡片失败：{}", rc.title));
                        return;
                    }
                    rel
                }
            };
            // Learning-order number (leaves first); the root goal card stays
            // unnumbered.
            cards.push((
                rc.id.clone(),
                rc.title.clone(),
                rel,
                rc.parent.clone(),
                Some(n as u32 + 1),
            ));
        }
        // Goal analysis lands on the root card.
        let root_path = base.join(&self.route_root);
        let mut body = std::fs::read_to_string(&root_path).unwrap_or_default();
        if !plan.goal_input.is_empty() {
            body = crate::gen::upsert_section(&body, "#c 输入空间", &plan.goal_input);
        }
        if !plan.goal_output.is_empty() {
            body = crate::gen::upsert_section(&body, "#c 输出空间", &plan.goal_output);
        }
        // 用户情况 = the planner's assessment of the user's knowledge state;
        // the raw interview transcript (questions + answers) is prompt input
        // only and must not land on the card.
        if !plan.user_assessment.is_empty() {
            body = crate::gen::upsert_section(&body, "#c 用户情况", &plan.user_assessment);
        }
        std::fs::write(&root_path, body).ok();
        std::fs::write(
            base.join(&map_file),
            mindmap::route_map_json(&self.route_goal, &self.route_root, &cards),
        )
        .ok();
        mind_map.reload_map(cx);
        self.set_card_title_indicator(cx, &self.route_root, None);
        if let Some(rag) = &self.rag {
            rag.set_map(&map_file);
        }
        let summary = format!(
            "学习路线已生成：{} 张卡片（概念卡 {} 张，知识卡 {} 张）。\n\
             每张卡片右键「生成」学习材料、「测试」验证掌握程度；根卡片记录了学习目标的输入输出。",
            plan.cards.len(),
            plan.cards.iter().filter(|c| c.card_type == "concept").count(),
            plan.cards.iter().filter(|c| c.card_type == "knowledge").count(),
        );
        if think.is_empty() {
            self.push_chat_msg(cx, "assistant", &summary);
        } else {
            self.push_chat_msg_thinking(cx, &summary, &think);
        }
        self.ensure_ai_panel_open(cx);
    }

    /// Poll deferred route-plan retrieval and promote it to a real request.
    fn handle_route_rag_tick(&mut self, cx: &mut Cx) {
        let Some(wait) = &mut self.route_wait else { return };
        let now = Instant::now();
        let hits = match wait.rx.try_recv() {
            Ok(r) if r.query == wait.goal => Some(r.hits),
            Ok(_) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if now.duration_since(wait.started) > RAG_RETRIEVE_TIMEOUT {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Err(_) => Some(Vec::new()),
        };
        let Some(hits) = hits else { return };
        let ctx = if hits.is_empty() {
            wait.fallback.clone()
        } else {
            rag::service::format_context(&hits)
        };
        let goal = wait.goal.clone();
        let diag = wait.diag.clone();
        self.route_wait = None;
        self.send_route_request(cx, &goal, &diag, &ctx);
    }

    fn close_startup(&mut self, cx: &mut Cx) {
        // The input can't emit KeyFocusLost once the popup is gone.
        crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed);
        self.reset_diag();
        self.popup_widget(live_id!(startup_popup)).as_popup_panel().hide(cx);
    }

    /// Open the startup welcome page, clearing any leftover input text and
    /// resetting the diagnostic session back to the goal-input phase.
    fn show_startup(&mut self, cx: &mut Cx) {
        self.reset_diag();
        self.set_startup_phase(cx, StartupPhase::Goal);
        self.popup_child(
            live_id!(startup_popup),
            &[
                live_id!(content),
                live_id!(panel),
                live_id!(goal_view),
                live_id!(input_row),
                live_id!(start_input),
            ],
        )
        .as_text_input()
        .set_text(cx, "");
        self.popup_widget(live_id!(startup_popup)).as_popup_panel().show(cx);
    }

    /// File panel tree: map click, context-menu create/delete/rename.
    fn handle_file_panel_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        // File panel tree: clicking a map switches the mindmap to it.
        if let Some(map_file) = self.ui.file_panel(cx, ids!(file_panel)).map_clicked(actions) {
            self.open_map(cx, &map_file);
        }
        // Context menu: create map / dir, delete map, rename.
        let base = crate::util::app_base_dir();
        if let Some(map_file) = self.ui.file_panel(cx, ids!(file_panel)).create_map(actions) {
            std::fs::write(base.join(&map_file), crate::mindmap::new_map_json()).ok();
            self.open_map(cx, &map_file);
            // A fresh map starts from the welcome page: ask what to learn.
            self.show_startup(cx);
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
                    self.open_map(cx, &self.next_map(&base));
                }
            } else if rel.starts_with("cards/") {
                // Card file: confirm first (the dialog lists using maps).
                self.open_card_delete_confirm(cx, &rel);
            } else {
                std::fs::remove_file(base.join(&rel)).ok();
                if mind_map.current_map_file().as_deref() == Some(rel.as_str()) {
                    // Switch to the first remaining map; none left → the
                    // default, whose failed load empties the canvas.
                    self.open_map(cx, &self.next_map(&base));
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

    /// The first remaining map under maps/ (the default when none is left;
    /// a failed load of it empties the canvas).
    fn next_map(&self, base: &std::path::Path) -> String {
        file_panel::all_map_files(base)
            .into_iter()
            .next()
            .unwrap_or_else(|| mindmap::MindMapData::DEFAULT_MAP.to_string())
    }

    /// Open the delete-confirm popup for the card at `rel`, listing every
    /// map that references it.
    fn open_card_delete_confirm(&mut self, cx: &mut Cx, rel: &str) {
        self.pending_delete_card = Some(rel.to_string());
        let base = crate::util::app_base_dir();
        let name = std::path::Path::new(rel)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| rel.to_string());
        let maps = crate::mindmap::maps_using_card(&base, rel);
        let usage = if maps.is_empty() {
            "该卡片没有被任何 map 使用。".to_string()
        } else {
            let list = maps
                .iter()
                .map(|m| format!("• {}", file_panel::display_name(m)))
                .collect::<Vec<_>>()
                .join("\n");
            format!("该卡片被以下 {} 个 map 使用：\n{list}", maps.len())
        };
        let child = |path: &[LiveId]| self.popup_child(live_id!(confirm_popup), path);
        child(&[live_id!(content), live_id!(panel), live_id!(card_name)]).set_text(cx, &name);
        child(&[live_id!(content), live_id!(panel), live_id!(usage)]).set_text(cx, &usage);
        self.popup_widget(live_id!(confirm_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(quiz_popup),
            live_id!(picker_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
    }

    /// Card delete-confirm popup buttons.
    fn handle_card_delete_confirm(&mut self, cx: &mut Cx, actions: &Actions) {
        let popup = live_id!(confirm_popup);
        let child = |path: &[LiveId]| self.popup_child(popup, path);
        if child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(cancel_btn)])
            .as_button()
            .clicked(actions)
        {
            self.pending_delete_card = None;
            self.popup_widget(popup).as_popup_panel().hide(cx);
            return;
        }
        if child(&[live_id!(content), live_id!(panel), live_id!(btn_row), live_id!(delete_btn)])
            .as_button()
            .clicked(actions)
        {
            let rel = self.pending_delete_card.take();
            self.popup_widget(popup).as_popup_panel().hide(cx);
            let Some(rel) = rel else { return };
            let base = crate::util::app_base_dir();
            crate::mindmap::remove_card_node(&base, &rel);
            std::fs::remove_file(base.join(&rel)).ok();
            // Drop the ghost node from the in-memory map (if present) so a
            // later save can't resurrect it; RAG and the file panel follow
            // via their own mtime/fingerprint watchers.
            self.ui.mind_map(cx, ids!(mindmap)).reload_map(cx);
        }
    }

    /// Drop a card dragged from the file panel onto the canvas.
    fn handle_card_drop(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some((rel, abs)) = self.ui.file_panel(cx, ids!(file_panel)).card_dropped(actions) {
            self.ui.mind_map(cx, ids!(mindmap)).drop_card_at(cx, &rel, abs);
        }
    }

    /// Current config as typed in the settings form (empty base_url/model
    /// fall back to the DeepSeek defaults).
    fn form_config(&self, _cx: &Cx) -> AIConfig {
        let child = |path: &[LiveId]| self.popup_child(live_id!(setting_popup), path);
        let mut cfg = self.ai_config.clone();
        cfg.api_key = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(key_row),
            live_id!(key_input),
        ])
        .as_text_input()
        .text();
        let base_url = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(url_row),
            live_id!(url_input),
        ])
        .as_text_input()
        .text();
        let model = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(model_row),
            live_id!(model_input),
        ])
        .as_text_input()
        .text();
        if !base_url.trim().is_empty() {
            cfg.base_url = base_url.trim().to_string();
        }
        if !model.trim().is_empty() {
            cfg.model = model.trim().to_string();
        }
        cfg.thinking = child(&[
            live_id!(content),
            live_id!(panel),
            live_id!(settings_form),
            live_id!(thinking_row),
            live_id!(thinking_input),
        ])
        .as_drop_down()
        .selected_label();
        cfg
    }

    fn open_map(&mut self, cx: &mut Cx, map_file: &str) {
        self.ui.mind_map(cx, ids!(mindmap)).switch_map(cx, map_file);
        self.ui
            .file_panel(cx, ids!(file_panel))
            .set_current_map(cx, Some(map_file));
        self.ui
            .refs_panel(cx, ids!(refs_panel))
            .set_current_map(cx, Some(map_file));
        if let Some(rag) = &self.rag {
            rag.set_map(map_file);
        }
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
        cached_ai_child(cx, &self.ui, &mut self.chat_list_ref, live_id!(chat_list))
    }

    /// The ai_panel's ctx_row View (cached), via live children from the
    /// panel content.
    fn ctx_row(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.ctx_row_ref, live_id!(ctx_row))
    }

    /// The ai_panel's header View, via live children from the panel content.
    fn panel_header(&mut self, cx: &Cx) -> WidgetRef {
        let content = self.ui.float_panel(cx, ids!(ai_panel)).content(cx);
        child_by_name(&content, live_id!(header))
    }

    /// The ai_panel's input_row View (cached), via live children from the
    /// panel content.
    fn panel_input_row(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.input_row_ref, live_id!(input_row))
    }

    /// The ai_panel's tools_row View (cached), via live children from the
    /// panel content.
    fn tools_row(&mut self, cx: &Cx) -> WidgetRef {
        cached_ai_child(cx, &self.ui, &mut self.tools_row_ref, live_id!(tools_row))
    }

    /// Child of `parent` by name, via live children (graph-independent).
    fn child_by_name(&self, parent: &WidgetRef, id: LiveId) -> WidgetRef {
        child_by_name(parent, id)
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
        self.sync_jiangou_btns(cx);
    }

    /// Send the chat input text: append to history and stream a request.
    /// Refuses (with a hint) when the context window is full.
    fn send_chat(&mut self, cx: &mut Cx, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.chat_pending {
            return;
        }
        // BM25 context first (µs): the gauge must count what will actually
        // be injected, plus slack for the async hybrid upgrade.
        let ctx = self.rag_bm25_context(text);
        let upgradeable = self
            .rag
            .as_ref()
            .is_some_and(|r| r.models().is_some_and(|m| m.embedding_ready()));
        let rag_tokens = ai::estimate_tokens(&ctx)
            + if upgradeable { RAG_CONTEXT_SLACK } else { 0 };
        let would_use = self.context_tokens() + ai::estimate_tokens(text) + rag_tokens;
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
        // RAG context: sync BM25 always (fast path); when the models are
        // ready, defer the request until the background hybrid retrieval
        // answers (or times out), so citations reflect reranked results.
        let mut defer = false;
        if let Some(rag) = &self.rag {
            if upgradeable {
                let rx = rag.retrieve(text);
                self.rag_wait = Some(RagWait {
                    query: text.to_string(),
                    rx,
                    fallback: ctx.clone(),
                    started: Instant::now(),
                });
                defer = true;
            }
        }
        if !defer {
            self.fire_chat(cx, self.build_messages(&ctx));
        }
    }

    /// The messages for the next request: chat history plus (when enabled)
    /// the 渐构 format instruction and the RAG context, both as system
    /// messages (injected per request, never stored in chat_history).
    fn build_messages(&self, ctx: &str) -> Vec<(String, String)> {
        let mut messages: Vec<(String, String)> = self
            .chat_history
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        if !self.ai_config.jiangou_sections.is_empty() {
            messages.insert(
                0,
                (
                    "system".to_string(),
                    ai::jiangou_format_prompt(&self.ai_config.jiangou_sections),
                ),
            );
        }
        if !ctx.is_empty() {
            messages.insert(0, ("system".to_string(), ctx.to_string()));
        }
        messages
    }

    /// Fire the chat request with the given (system-prefixed) messages.
    fn fire_chat(&mut self, cx: &mut Cx, messages: Vec<(String, String)>) {
        self.chat_id = LiveId::unique();
        ai::chat_stream_request(cx, self.chat_id, &self.ai_config, &messages);
        self.render_msgs(cx);
    }

    /// Synchronous BM25-only context from the shared index (µs; the
    /// retrieval worker's hybrid upgrade replaces it when available).
    fn rag_bm25_context(&self, query: &str) -> String {
        let Some(rag) = &self.rag else {
            return String::new();
        };
        let hits = rag.bm25_search(query, 5);
        rag::service::format_context(&hits)
    }

    /// Status text for the ai_panel header label; "" when idle and ready.
    fn rag_status_text(&self) -> String {
        if self.rag_wait.is_some() {
            return "检索中…".to_string();
        }
        let Some(rag) = &self.rag else {
            return String::new();
        };
        let Some(models) = rag.models() else {
            return "模型准备中…".to_string();
        };
        let st = models.status.read().unwrap();
        match &*st {
            rag::ModelStatus::Downloading(f) => format!("下载模型 {f}…"),
            rag::ModelStatus::Loading => "加载模型…".to_string(),
            rag::ModelStatus::Failed(e) => format!("RAG 不可用: {e}"),
            rag::ModelStatus::Ready => {
                if rag.indexing() {
                    "索引中…".to_string()
                } else {
                    String::new()
                }
            }
        }
    }

    /// Periodic rag timer: re-sync the index from disk, refresh the status
    /// label, and fire a deferred chat once its retrieval answers.
    fn handle_rag_tick(&mut self, cx: &mut Cx) {
        let now = Instant::now();
        let due = self
            .last_resync
            .map(|t| now.duration_since(t).as_secs() >= RAG_RESYNC_SECS)
            .unwrap_or(false);
        if self.map_opened && due {
            self.last_resync = Some(now);
            if let Some(rag) = &self.rag {
                if let Some(map) = self.ui.mind_map(cx, ids!(mindmap)).current_map_file() {
                    rag.set_map(&map);
                }
            }
        }
        let text = self.rag_status_text();
        if text != self.last_rag_label {
            self.last_rag_label = text.clone();
            self.ui.label(cx, ids!(rag_status)).set_text(cx, &text);
        }
        self.handle_gen_rag_tick(cx);
        self.handle_route_rag_tick(cx);
        let Some(wait) = &mut self.rag_wait else {
            return;
        };
        let hits = match wait.rx.try_recv() {
            Ok(r) if r.query == wait.query => Some(r.hits),
            Ok(_) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if now.duration_since(wait.started) > RAG_RETRIEVE_TIMEOUT {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Err(_) => Some(Vec::new()),
        };
        let Some(hits) = hits else {
            return;
        };
        let fallback = wait.fallback.clone();
        self.rag_wait = None;
        let ctx = if hits.is_empty() {
            fallback
        } else {
            rag::service::format_context(&hits)
        };
        self.fire_chat(cx, self.build_messages(&ctx));
    }

    /// Start a fresh conversation: drop all history and extras.
    fn new_chat(&mut self, cx: &mut Cx) {
        if self.chat_pending {
            cx.cancel_http_request(self.chat_id);
        }
        self.rag_wait = None;
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
        self.rag_wait = None;
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

    /// 渐构 section pills: swap each gray/blue pair by its enabled state.
    fn sync_jiangou_btns(&mut self, cx: &mut Cx) {
        let tools = self.tools_row(cx);
        for (id, _) in ai::JIANGOU_SECTIONS {
            let on = self
                .ai_config
                .jiangou_sections
                .iter()
                .any(|s| s == id);
            let base = LiveId::from_str(&format!("{id}_btn"));
            let on_id = LiveId::from_str(&format!("{id}_on_btn"));
            self.child_by_name(&tools, base).set_visible(cx, !on);
            self.child_by_name(&tools, on_id).set_visible(cx, on);
        }
    }

    /// Card context menu: generate a section, start a quiz, or open the
    /// canvas card picker.
    fn handle_mindmap_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        if let Some((path, section)) = mind_map.generate_clicked(actions) {
            self.start_generation(cx, &path, section);
        }
        if let Some((parent, selected)) = mind_map.subcard_clicked(actions) {
            self.start_subcard_gen(cx, &parent, &selected);
        }
        if let Some(path) = mind_map.quiz_clicked(actions) {
            self.start_quiz(cx, &path);
        }
        if let Some(path) = mind_map.route_clicked(actions) {
            // The menu only offers planning on the root goal card; the goal
            // text is the card's file stem (minus a numeric order prefix).
            // Same as the startup path: run the diagnostic interview first.
            let goal = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .map(|s| crate::gen::strip_order_prefix(&s).to_string())
                .unwrap_or_default();
            if !goal.is_empty() {
                self.begin_diag(cx, &goal);
            }
        }
        if let Some(pos) = mind_map.canvas_menu_clicked(actions) {
            self.open_card_picker(cx, pos);
        }
    }

    /// Open the canvas card picker at `pos` (screen coords): scan cards/,
    /// exclude cards already on the map, and show the popup.
    fn open_card_picker(&mut self, cx: &mut Cx, pos: DVec2) {
        let base = crate::util::app_base_dir();
        let on_map = self.ui.mind_map(cx, ids!(mindmap)).card_rel_paths();
        let candidates: Vec<String> = crate::file_panel::all_card_files(&base)
            .into_iter()
            .filter(|p| !on_map.contains(p))
            .collect();
        self.open_picker_popup(cx);
        self.picker().open(cx, pos, &candidates);
    }

    fn picker(&self) -> crate::card_picker::CardPickerRef {
        self.popup_child(live_id!(picker_popup), &[live_id!(content)])
            .as_card_picker()
    }

    fn open_picker_popup(&self, cx: &mut Cx) {
        self.popup_widget(live_id!(picker_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(quiz_popup),
            live_id!(confirm_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
    }

    /// CardPicker popup: apply the choice (add existing / create new card).
    fn handle_picker_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let picker = self.picker();
        if picker.close_clicked(actions) {
            self.popup_widget(live_id!(picker_popup)).as_popup_panel().hide(cx);
            return;
        }
        let Some(choice) = picker.picked(actions) else {
            return;
        };
        self.popup_widget(live_id!(picker_popup)).as_popup_panel().hide(cx);
        let rel = match choice {
            PickChoice::Card(rel) => Some(rel),
            PickChoice::Create(name) => self.create_card_file(&name),
            PickChoice::None => None,
        };
        if let Some(rel) = rel {
            self.ui.mind_map(cx, ids!(mindmap)).add_card_at(cx, &rel);
        }
    }

    /// Create an empty card file in the default `cards/` dir from the search
    /// text (default "未命名" when empty); unique-ified with a numeric suffix
    /// when the name is taken. Returns the rel path.
    fn create_card_file(&self, name: &str) -> Option<String> {
        let stem = crate::file_panel::normalize_name(name, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let stem = stem.strip_suffix(".md").unwrap_or(&stem).to_string();
        let base = crate::util::app_base_dir();
        for n in 0.. {
            let fname = if n == 0 {
                format!("{stem}.md")
            } else {
                format!("{stem}-{n}.md")
            };
            let p = base.join("cards").join(&fname);
            if !p.exists() {
                std::fs::write(&p, "").ok()?;
                return Some(format!("cards/{fname}"));
            }
        }
        None
    }

    fn start_generation(&mut self, cx: &mut Cx, path: &str, section: GenSection) {
        if self.gen_wait.is_some() || self.gen_id != LiveId::empty() {
            return;
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if title.is_empty() {
            return;
        }
        // "所有" becomes a queue of per-section requests; each section is
        // generated in its own request so a thinking model can't eat the
        // whole output budget with reasoning.
        let sections: Vec<GenSection> = if section == GenSection::All {
            GenSection::all().to_vec()
        } else {
            vec![section]
        };
        let fallback = self.rag_bm25_context(&title);
        let upgradeable = self
            .rag
            .as_ref()
            .is_some_and(|r| r.models().is_some_and(|m| m.embedding_ready()));
        if upgradeable {
            let rx = self.rag.as_ref().unwrap().retrieve(&title);
            self.gen_wait = Some(GenWait {
                path: path.to_string(),
                sections,
                title: title.clone(),
                rx,
                fallback,
                started: Instant::now(),
            });
            self.set_card_title_indicator(cx, path, Some("生成中…"));
        } else {
            self.send_generation(cx, path, sections, &title, &fallback);
        }
    }

    fn send_generation(
        &mut self,
        cx: &mut Cx,
        path: &str,
        mut sections: Vec<GenSection>,
        title: &str,
        context: &str,
    ) {
        self.gen_path = path.to_string();
        self.gen_title = title.to_string();
        self.gen_context = context.to_string();
        self.gen_total = sections.len();
        let Some(first) = sections.drain(..1).next() else {
            return;
        };
        self.gen_sections = sections;
        self.send_gen_section(cx, first);
    }

    /// Fire the HTTP request for one generation section, with progress.
    fn send_gen_section(&mut self, cx: &mut Cx, section: GenSection) {
        self.gen_id = LiveId::unique();
        let done = self.gen_total.saturating_sub(self.gen_sections.len() + 1);
        let indicator = format!("生成中… ({}/{})", done + 1, self.gen_total);
        self.set_card_title_indicator(cx, &self.gen_path, Some(&indicator));
        let body = std::fs::read_to_string(crate::util::app_base_dir().join(&self.gen_path))
            .unwrap_or_default();
        let ctype = crate::gen::card_type(&body);
        let (system, user) = generation_messages(section, &self.gen_title, &self.gen_context, ctype);
        ai::chat_completions(
            cx,
            self.gen_id,
            &self.ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    /// Abort the current generation queue and surface `msg` in the AI panel.
    fn abort_generation(&mut self, cx: &mut Cx, msg: String) {
        self.gen_sections.clear();
        self.set_card_title_indicator(cx, &self.gen_path, None);
        self.push_chat_msg(cx, "assistant", &msg);
        self.ensure_ai_panel_open(cx);
    }

    fn handle_gen_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.gen_id = LiveId::empty();
        let status = response.status_code;
        let full_path = crate::util::app_base_dir().join(&self.gen_path);
        if status != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            self.abort_generation(cx, format!("生成失败 ({})：{}", status, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        let sections = parse_generation_output(&content);
        if sections.is_empty() {
            let debug = ai::response_debug_preview(response);
            self.abort_generation(cx, format!("生成返回为空或格式不正确（{debug}）"));
            return;
        }
        let body = std::fs::read_to_string(&full_path).unwrap_or_default();
        let new_body = upsert_sections(&body, &sections);
        if let Err(e) = std::fs::write(&full_path, &new_body) {
            self.abort_generation(cx, format!("保存卡片失败：{}", e));
            return;
        }
        // Update the card body if the card is still in the current map.
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        mind_map.update_card_body(cx, &full_path, new_body);
        // Continue the queue, or finish when it is exhausted.
        if !self.gen_sections.is_empty() {
            let next = self.gen_sections.remove(0);
            self.send_gen_section(cx, next);
        } else {
            self.set_card_title_indicator(cx, &self.gen_path, None);
            if let Some(rag) = &self.rag {
                rag.set_map(&self.ui.mind_map(cx, ids!(mindmap)).current_map_file().unwrap_or_default());
            }
        }
    }

    fn set_card_title_indicator(&self, cx: &mut Cx, path: &str, indicator: Option<&str>) {
        let full_path = crate::util::app_base_dir().join(path);
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        mind_map.set_card_title_indicator(cx, &full_path, indicator);
    }

    fn ensure_ai_panel_open(&mut self, cx: &mut Cx) {
        let panel = self.ui.float_panel(cx, ids!(ai_panel));
        if !panel.opened() {
            self.toggle_ai_panel(cx);
        }
    }

    /// 划选生成子卡片, phase 1: the model judges type/title/input/output
    /// (a small JSON that cannot be truncated). The body is filled in phase 2
    /// by the existing per-section generation pipeline.
    fn start_subcard_gen(&mut self, cx: &mut Cx, parent: &str, selected: &str) {
        if self.subcard_id != LiveId::empty() {
            return;
        }
        let base = crate::util::app_base_dir();
        let parent_body = std::fs::read_to_string(base.join(parent)).unwrap_or_default();
        let parent_title = std::path::Path::new(parent)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ctx = self.rag_bm25_context(selected);
        let (system, user) =
            crate::gen::subcard_judge_messages(&parent_title, &parent_body, selected, &ctx);
        self.subcard_parent = parent.to_string();
        self.push_chat_msg(cx, "assistant", "正在为划选内容判断类型并生成子卡片…");
        self.ensure_ai_panel_open(cx);
        self.subcard_id = LiveId::unique();
        ai::chat_completions(
            cx,
            self.subcard_id,
            &self.ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    fn handle_subcard_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.subcard_id = LiveId::empty();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            self.push_chat_msg(cx, "assistant", &format!("生成子卡片失败 ({}): {}", response.status_code, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        let judge = match crate::gen::parse_subcard_judge(&content) {
            Ok(v) => v,
            Err(e) => {
                let preview = ai::response_debug_preview(response);
                self.push_chat_msg(
                    cx,
                    "assistant",
                    &format!("生成子卡片失败：{e}（{preview}）"),
                );
                return;
            }
        };
        let base = crate::util::app_base_dir();
        let parent_rel = self.subcard_parent.clone();
        let parent_path = base.join(&parent_rel);
        let dir = parent_path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cards".to_string());
        let safe = crate::file_panel::normalize_name(&judge.title, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let safe = safe.strip_suffix(".md").unwrap_or(&safe).to_string();
        // Seed body: the 知识类型/输入输出/输入输出空间 blocks are assembled
        // here so every subcard carries them; the rest is generated later.
        let seed = match judge.ctype {
            crate::gen::CardType::Knowledge => {
                let mut s = format!(
                    "#c 知识类型 联结模型\n\n#c 输入输出\n输入：{}\n输出：{}\n",
                    judge.input.trim(),
                    judge.output.trim()
                );
                if !judge.input_space.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输入空间\n{}\n",
                        judge.input_space.trim()
                    ));
                }
                if !judge.output_space.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输出空间\n{}\n",
                        judge.output_space.trim()
                    ));
                }
                s
            }
            crate::gen::CardType::Concept => {
                let mut s = "#c 知识类型 概念\n".to_string();
                if !judge.input.trim().is_empty() || !judge.output.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输入输出\n输入：{}\n输出：{}\n",
                        judge.input.trim(),
                        judge.output.trim()
                    ));
                }
                s
            }
        };
        let mut rel = None;
        for n in 0.. {
            let fname = if n == 0 {
                format!("{safe}.md")
            } else {
                format!("{safe}-{n}.md")
            };
            let p = base.join(&dir).join(&fname);
            if !p.exists() {
                std::fs::create_dir_all(p.parent().unwrap_or(&base)).ok();
                if std::fs::write(&p, seed.clone()).is_ok() {
                    rel = Some(format!("{dir}/{fname}"));
                }
                break;
            }
        }
        match rel {
            Some(rel) => {
                let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                mind_map.add_child_card(cx, &parent_rel, &rel);
                let kind = match judge.ctype {
                    crate::gen::CardType::Concept => "概念",
                    crate::gen::CardType::Knowledge => "知识",
                };
                self.push_chat_msg(
                    cx,
                    "assistant",
                    &format!(
                        "已生成{kind}子卡片「{}」，已挂到父卡片下，开始逐板块生成学习材料…",
                        judge.title
                    ),
                );
                // Phase 2: the per-section pipeline fills the card body
                // (each section in its own request — immune to truncation).
                self.start_generation(cx, &rel, crate::gen::GenSection::All);
            }
            None => self.push_chat_msg(cx, "assistant", "生成子卡片失败：无法创建卡片文件。"),
        }
    }

    fn start_quiz(&mut self, cx: &mut Cx, path: &str) {
        let full_path = crate::util::app_base_dir().join(path);
        let body = std::fs::read_to_string(&full_path).unwrap_or_default();
        let title = full_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if let Err(missing) = quiz_ready(&body) {
            quiz_panel.show_error(cx, &format!("卡片缺少以下板块：{}", missing.join("、")));
            return;
        }
        self.quiz_path = Some(path.to_string());
        self.quiz_body = Some(body.clone());
        quiz_panel.show_loading(cx, &title);
        self.quiz_id = LiveId::unique();
        let (system, user) = quiz_generation_messages(&body, crate::gen::card_type(&body));
        ai::chat_completions(
            cx,
            self.quiz_id,
            &self.ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    fn handle_quiz_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.quiz_id = LiveId::empty();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            quiz_panel.show_error(cx, &format!("出题失败：{} {}", response.status_code, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match parse_quiz(&content) {
            Ok(q) => {
                let title = self
                    .quiz_path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let body = self.quiz_body.clone().unwrap_or_default();
                quiz_panel.set_quiz(cx, &title, &body, &q);
            }
            Err(e) => quiz_panel.show_error(cx, &format!("题目解析失败：{}", e)),
        }
    }

    fn handle_quiz_panel_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let quiz_panel = self.quiz_panel();
        if quiz_panel.close_clicked(actions) {
            self.close_quiz_popup(cx);
        }
        if let Some(submission) = quiz_panel.submit_clicked(actions) {
            self.send_grade_request(cx, submission);
        }
    }

    fn send_grade_request(&mut self, cx: &mut Cx, submission: QuizSubmission) {
        let Some(body) = self.quiz_body.as_deref() else { return };
        let questions = submission.open_questions;
        let answers = submission.open;
        if questions.is_empty() || answers.is_empty() {
            return;
        }
        let (system, user) = quiz_grading_messages(body, &questions, &answers);
        self.grade_id = LiveId::unique();
        ai::chat_completions(
            cx,
            self.grade_id,
            &self.ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    fn handle_grade_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.grade_id = LiveId::empty();
        self.open_quiz_popup(cx);
        let quiz_panel = self.quiz_panel();
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            quiz_panel.grade_failed(cx, &format!("评分失败：{} {}", response.status_code, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match parse_grades(&content) {
            Ok(g) => {
                quiz_panel.set_grades(cx, &g);
                // Persist the mastery score (已见/未见) and refresh the canvas
                // badge. The card is keyed by rel path so progress follows it
                // across maps.
                if let Some(score) = quiz_panel.last_score() {
                    if let Some(path) = self.quiz_path.as_deref() {
                        let base = crate::util::app_base_dir();
                        let mut progress = crate::mindmap::model::load_progress(&base);
                        progress.insert(path.to_string(), score);
                        crate::mindmap::model::save_progress(&base, &progress);
                        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                        mind_map.reload_progress(cx);
                    }
                }
            }
            Err(e) => quiz_panel.grade_failed(cx, &format!("评分解析失败：{}", e)),
        }
    }

    fn quiz_panel(&self) -> crate::quiz_panel::QuizPanelRef {
        self.popup_child(live_id!(quiz_popup), &[live_id!(content)])
            .as_quiz_panel()
    }

    fn open_quiz_popup(&self, cx: &mut Cx) {
        self.popup_widget(live_id!(quiz_popup)).as_popup_panel().show(cx);
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(picker_popup),
            live_id!(confirm_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
    }

    fn close_quiz_popup(&self, cx: &mut Cx) {
        self.popup_widget(live_id!(quiz_popup)).as_popup_panel().hide(cx);
    }

    /// Poll deferred card-generation retrieval and promote it to a real request.
    fn handle_gen_rag_tick(&mut self, cx: &mut Cx) {
        let Some(wait) = &mut self.gen_wait else { return };
        let now = Instant::now();
        let hits = match wait.rx.try_recv() {
            Ok(r) if r.query == wait.title => Some(r.hits),
            Ok(_) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if now.duration_since(wait.started) > RAG_RETRIEVE_TIMEOUT {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Err(_) => Some(Vec::new()),
        };
        let Some(hits) = hits else { return };
        let ctx = if hits.is_empty() {
            wait.fallback.clone()
        } else {
            rag::service::format_context(&hits)
        };
        let path = wait.path.clone();
        let sections = std::mem::take(&mut wait.sections);
        let title = wait.title.clone();
        self.gen_wait = None;
        self.send_generation(cx, &path, sections, &title, &ctx);
    }
}

impl MatchEvent for App {
    fn handle_http_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        if request_id == self.test_id && self.testing {
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
            self.popup_child(
                live_id!(setting_popup),
                &[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(status),
                ],
            )
            .set_text(cx, &msg);
            return;
        }
        if request_id == self.gen_id && self.gen_id != LiveId::empty() {
            self.handle_gen_response(cx, response);
            return;
        }
        if request_id == self.subcard_id && self.subcard_id != LiveId::empty() {
            self.handle_subcard_response(cx, response);
            return;
        }
        if request_id == self.diag_id && self.diag_id != LiveId::empty() {
            self.handle_diag_response(cx, response);
            return;
        }
        if request_id == self.quiz_id && self.quiz_id != LiveId::empty() {
            self.handle_quiz_response(cx, response);
            return;
        }
        if request_id == self.grade_id && self.grade_id != LiveId::empty() {
            self.handle_grade_response(cx, response);
            return;
        }
    }

    fn handle_http_request_error(&mut self, cx: &mut Cx, request_id: LiveId, err: &HttpError) {
        if request_id == self.test_id && self.testing {
            self.testing = false;
            self.popup_child(
                live_id!(setting_popup),
                &[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(status),
                ],
            )
            .set_text(cx, &format!("连接失败：{}", err.message));
            return;
        }
        if request_id == self.gen_id && self.gen_id != LiveId::empty() {
            self.gen_id = LiveId::empty();
            self.abort_generation(cx, format!("生成请求失败：{}", err.message));
            return;
        }
        if request_id == self.subcard_id && self.subcard_id != LiveId::empty() {
            self.subcard_id = LiveId::empty();
            self.push_chat_msg(cx, "assistant", &format!("生成子卡片失败：{}", err.message));
            return;
        }
        if request_id == self.route_id && self.route_id != LiveId::empty() {
            self.route_buf.clear();
            self.route_think.clear();
            self.clear_route_progress();
            self.abort_route(cx, format!("路线规划请求失败：{}", err.message));
            return;
        }
        if request_id == self.diag_id && self.diag_id != LiveId::empty() {
            self.diag_id = LiveId::empty();
            self.set_diag_status(cx, &format!("出题请求失败：{}", err.message));
            return;
        }
        if request_id == self.quiz_id && self.quiz_id != LiveId::empty() {
            self.quiz_id = LiveId::empty();
            self.open_quiz_popup(cx);
            self.quiz_panel().show_error(cx, &format!("出题请求失败：{}", err.message));
            return;
        }
        if request_id == self.grade_id && self.grade_id != LiveId::empty() {
            self.grade_id = LiveId::empty();
            self.open_quiz_popup(cx);
            self.quiz_panel().grade_failed(cx, &format!("评分请求失败：{}", err.message));
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
        if request_id == self.route_id && self.route_id != LiveId::empty() {
            if let Some(bytes) = data.body() {
                let (content, thinking) = self.route_parser.feed(bytes);
                for delta in content {
                    self.route_buf.push_str(&delta);
                }
                for delta in thinking {
                    self.route_think.push_str(&delta);
                }
                self.update_route_progress(cx);
            }
            return;
        }
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
        if request_id == self.route_id && self.route_id != LiveId::empty() {
            self.clear_route_progress();
            self.route_id = LiveId::empty();
            // macOS 的 makepad 流式后端把 status_code 硬编码为 0，成功流按
            // "status 0 + [DONE]" 判定（同 chat 的处理）。
            let ok = data.status_code == 200
                || (data.status_code == 0 && self.route_parser.raw().contains("[DONE]"));
            let buf = std::mem::take(&mut self.route_buf);
            let think = std::mem::take(&mut self.route_think);
            if ok {
                self.apply_route_plan(cx, buf, think);
            } else {
                // Non-200 stream: the body was raw JSON (not SSE), recovered
                // from the parser's raw buffer.
                let raw = self.route_parser.raw();
                let detail = ai::body_error_message(&raw)
                    .unwrap_or_else(|| raw.chars().take(200).collect());
                self.abort_route(cx, format!("路线规划失败 ({}): {}", data.status_code, detail));
            }
            return;
        }
        if request_id != self.chat_id || !self.chat_pending {
            return;
        }
        self.chat_pending = false;
        // macOS 的 makepad 流式后端在连接正常结束时把 status_code 硬编码为
        // 0（从不记录真实 HTTP 状态），所以成功流要按 "status 0 + [DONE]"
        // 判定；Linux/Windows 传真实 200，不受影响。
        let ok = data.status_code == 200
            || (data.status_code == 0 && self.chat_parser.raw().contains("[DONE]"));
        let content = if ok {
            self.chat_buf.clone()
        } else {
            // Non-200 stream: the body was raw JSON (not SSE), recovered from
            // the parser's raw buffer.
            let raw = self.chat_parser.raw();
            let detail = ai::body_error_message(&raw)
                .unwrap_or_else(|| raw.chars().take(200).collect());
            format!("请求失败 ({}): {}", data.status_code, detail)
        };
        if ok {
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
        crate::popup_panel::script_mod(vm);
        crate::bottom_bar::script_mod(vm);
        crate::float_panel::script_mod(vm);
        crate::file_panel::script_mod(vm);
        crate::chat_list::script_mod(vm);
        crate::refs_panel::script_mod(vm);
        crate::quiz_panel::script_mod(vm);
        crate::card_picker::script_mod(vm);
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
            // Lazy RAG startup (threads need no cx, but the poll timer does).
            if self.rag.is_none() {
                self.rag = Some(rag::RagService::start());
                self.rag_timer = Some(cx.start_interval(0.25));
                self.last_resync = Some(Instant::now());
                if let Some(map) = self.ui.mind_map(cx, ids!(mindmap)).current_map_file() {
                    if let Some(rag) = &self.rag {
                        rag.set_map(&map);
                    }
                }
            }
            // First launch with no maps: create the default map for the user
            // and open the welcome page.
            let base = crate::util::app_base_dir();
            if crate::file_panel::all_map_files(&base).is_empty() {
                let map = mindmap::MindMapData::DEFAULT_MAP.to_string();
                let _ = std::fs::create_dir_all(base.join("maps"));
                std::fs::write(base.join(&map), mindmap::new_map_json()).ok();
                self.open_map(cx, &map);
                self.show_startup(cx);
            }
        }
        if let Some(timer) = self.rag_timer {
            if timer.is_event(event).is_some() {
                self.handle_rag_tick(cx);
            }
        }
        self.match_event(cx, event);
        if let Event::Actions(actions) = event {
            self.handle_popup_closes(cx, actions);
            self.handle_settings_actions(cx, actions);
            self.handle_chat_actions(cx, actions);
            self.handle_file_panel_actions(cx, actions);
            self.handle_startup_actions(cx, actions);
            self.handle_mindmap_actions(cx, actions);
            self.handle_quiz_panel_actions(cx, actions);
            self.handle_picker_actions(cx, actions);
            self.handle_card_delete_confirm(cx, actions);
            self.handle_card_drop(cx, actions);
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
        // After ui.handle_event: the dock writes pending_click while the
        // event propagates, so poll it afterwards — a click would otherwise
        // wait for the next event (never comes with a still mouse).
        self.handle_dock_clicks(cx);
        // While a card drag is in flight, keep the canvas ghost glued to the
        // pointer (the file panel owns the drag but can't redraw the map).
        // MouseUp redraws unconditionally: the drag state is already cleared
        // by then, and the ghost must not linger after a non-canvas drop.
        match event {
            Event::MouseMove(_) if crate::util::card_drag().is_some() => {
                self.ui.mind_map(cx, ids!(mindmap)).redraw(cx);
            }
            Event::MouseUp(_) => {
                self.ui.mind_map(cx, ids!(mindmap)).redraw(cx);
            }
            _ => {}
        }
        // The dock area covers the whole window on top of the popups, so
        // root dispatch can't reach the popup's buttons/inputs; hand mouse
        // events to the open popup directly (it forwards to its content,
        // which hit-tests geometrically).
        if let Event::MouseDown(_) | Event::MouseUp(_) = event {
            for id in [
                live_id!(setting_popup),
                live_id!(about_popup),
                live_id!(startup_popup),
                live_id!(quiz_popup),
                live_id!(picker_popup),
                live_id!(confirm_popup),
            ] {
                let p = self.popup_widget(id).as_popup_panel();
                if p.opened() {
                    p.handle_event(cx, event, &mut Scope::empty());
                    break;
                }
            }
        }
    }
}
