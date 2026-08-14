pub use makepad_widgets;

use makepad_widgets::*;

use std::time::Instant;

use crate::ai::AIConfig;

app_main!(App);

mod ai;
mod bottom_bar;
mod card_picker;
mod chat_list;
mod create_card_popup;
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
mod app;

use crate::app::chat::ChatController;
use crate::app::diag::DiagController;
use crate::app::generation::GenController;
use crate::app::quiz::QuizController;
use crate::app::route::RouteController;
use crate::mindmap::MindMapWidgetRefExt;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // Feature-task notifications (route/subcard/generation results): a
    // corner toast that never touches the AI-panel conversation.
    let ToastContent = mod.widgets.RoundedView{
        width: 380
        height: Fit
        flow: Down
        padding: Inset{left: 14, right: 14, top: 10, bottom: 10}
        margin: Inset{right: 16, top: 16}
        show_bg: true
        draw_bg +: {
            color: #1f2430f2
            border_radius: 8.0
            border_size: 1.0
            border_color: #ffffff14
        }
        label := mod.widgets.Label{
            width: Fill
            height: Fit
            text: ""
            draw_text.text_style: theme.font_regular{font_size: 12.0}
            draw_text.color: #e6e9f0
        }
    }

    let NewChatBtn = mod.widgets.ButtonFlatIcon{        padding: Inset{left: 3, right: 3, top: 3, bottom: 3}
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

    // Shared popup panel chrome (title/body_box/settings_form/close), sized
    // per instance: Setting keeps the default 420, About overrides to a wide
    // 760×480 rectangle.
    let PanelInner = mod.widgets.RoundedView{
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
        panel := PanelInner{}
    }

    // About 专用内容：横置大矩形（760×480），节点 id 与共享模板一致
    // （content/panel/title、body_box/body）。
    let AboutPopupContent = mod.widgets.View{
        width: Fill
        height: Fill
        flow: Overlay
        align: Align{x: 0.5, y: 0.5}
        draw_bg +: {
            pixel: fn(){
                #000000cc
            }
        }
        panel := PanelInner{
            width: 760
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
                            svg: file_resource(#(crate::util::resource_path("send.svg")))
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
                        content := AboutPopupContent{}
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
                    create_card_popup := mod.widgets.PopupPanel{
                        content := mod.widgets.CreateCardPopup{}
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
                                        svg: file_resource(#(crate::util::resource_path("plus.svg")))
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
                                                text_style_fixed: mod.widgets.app_code_font{
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
                                                svg: file_resource(#(crate::util::resource_path("copy.svg")))
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
                                                svg: file_resource(#(crate::util::resource_path("check.svg")))
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
                                                        text_style_fixed: mod.widgets.app_code_font{
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
                                                text_style_fixed: mod.widgets.app_code_font{
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
                                                svg: file_resource(#(crate::util::resource_path("copy.svg")))
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
                                                svg: file_resource(#(crate::util::resource_path("check.svg")))
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
                                        svg: file_resource(#(crate::util::resource_path("send.svg")))
                                        color: #aab0bc
                                    }
                                    icon_walk: Walk{width: 16, height: 16}
                                }
                                stop_btn := SendBtn{
                                    visible: false
                                    draw_icon +: {
                                        svg: file_resource(#(crate::util::resource_path("stop.svg")))
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
                    toast := mod.widgets.PopupNotification{
                        content := ToastContent{}
                    }
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
    /// Chat state and logic (history, streaming, RAG wait), extracted into
    /// ChatController; App keeps thin forwarding shims.
    #[rust]
    chat: ChatController,
    /// Diagnostic interview state (startup popup), incl. the map-naming
    /// request; extracted into DiagController.
    #[rust]
    diag: DiagController,
    /// Learning-route planning state; extracted into RouteController.
    #[rust]
    route: RouteController,
    /// Card/subcard generation state; extracted into GenController.
    #[rust]
    gen: GenController,
    /// Quiz generation/grading state; extracted into QuizController.
    #[rust]
    quiz: QuizController,
    /// RAG backend (two worker threads), created on first draw.
    #[rust]
    rag: Option<rag::RagService>,
    /// Drives status label refresh, periodic re-sync and retrieval polling.
    #[rust]
    rag_timer: Option<Timer>,
    /// Auto-close time of the feature toast (None = toast hidden).
    #[rust]
    toast_until: Option<Instant>,
    /// Last periodic index re-sync time.
    #[rust]
    last_resync: Option<Instant>,
    /// Last text shown in the rag_status label (set_text only on change, to
    /// avoid a redraw every 250ms tick).
    #[rust]
    last_rag_label: String,
    /// Card awaiting delete confirmation (rel path), set while the confirm
    /// popup is open.
    #[rust]
    pending_delete_card: Option<String>,
    /// Root card (with its subtree) awaiting removal confirmation: (rel path,
    /// title, subtree card count), set while the confirm popup is open.
    #[rust]
    pending_remove_root: Option<(String, String, usize)>,
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
        crate::create_card_popup::script_mod(vm);
        self::script_mod(vm)
    }

    fn after_new_from_script(vm: &mut ScriptVm, app: &mut Self) {
        crate::util::migrate_legacy_data();
        app.ai_config = ai::load_config();
        // The controllers carry their own copy of the ui handle for widget
        // lookups; mirror it here once the DSL tree exists.
        app.chat.ui = app.ui.clone();
        app.diag.ui = app.ui.clone();
        app.route.ui = app.ui.clone();
        app.gen.ui = app.ui.clone();
        app.quiz.ui = app.ui.clone();
        if let Some(cx) = vm.host.downcast_mut::<Cx>() {
            crate::util::relocate_resources(cx);
        }
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
            // Every launch: start from a fresh temporary map with the startup
            // page. Old temp maps (from sessions that ended without a goal)
            // are swept first.
            let base = crate::util::data_dir();
            if !self.map_opened {
                let maps = base.join("maps");
                let _ = std::fs::create_dir_all(&maps);
                if let Ok(it) = std::fs::read_dir(&maps) {
                    for e in it.flatten() {
                        let name = e.file_name();
                        if name.to_string_lossy().starts_with(".startup-") {
                            let _ = std::fs::remove_file(e.path());
                        }
                    }
                }
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let temp = format!("maps/.startup-{stamp}.json");
                std::fs::write(base.join(&temp), mindmap::new_map_json()).ok();
                self.open_map(cx, &temp);
            }
            // First launch with no maps: also keep a real map around so the
            // no-maps-left fallback (next_map) has something to open.
            if crate::file_panel::all_map_files(&base).is_empty() {
                let map = mindmap::MindMapData::DEFAULT_MAP.to_string();
                std::fs::write(base.join(&map), mindmap::new_map_json()).ok();
            }
        }
        if let Some(timer) = self.rag_timer {
            if timer.is_event(event).is_some() {
                self.handle_rag_tick(cx);
                // Auto-close an expired feature toast (same 0.25s tick).
                if self.toast_until.is_some_and(|t| Instant::now() >= t) {
                    self.toast_until = None;
                    self.toast_widget().as_popup_notification().close(cx);
                }
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
            self.handle_create_card_actions(cx, actions);
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
        // No special popup mouse routing: the root dispatch above already
        // forwards every event to the open PopupPanel's content (View
        // dispatches to all children, which hit-test their own areas).
        // Forwarding MouseDown/MouseUp a second time broke the DropDown:
        // the first delivery opened its menu, the duplicate hit the
        // "clicked outside the menu" branch and closed it immediately.
    }
}
