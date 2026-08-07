pub use makepad_widgets;

use makepad_widgets::*;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::ai::{AIConfig, SseParser};
use crate::bottom_bar::BottomBarWidgetRefExt;
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::util::cached_widget;

app_main!(App);

mod ai;
mod bottom_bar;
mod chat_list;
mod file_panel;
mod float_panel;
mod markdown_media;
mod mindmap;
mod popup_panel;
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
                                desc_btn := JiangouSectionBtn{ text: "标准描述" }
                                desc_on_btn := JiangouSectionBtnOn{ visible: false, text: "标准描述" }
                                spacer1 := mod.widgets.View{ width: Fill, height: Fit }
                                plain_btn := JiangouSectionBtn{ text: "通俗描述" }
                                plain_on_btn := JiangouSectionBtnOn{ visible: false, text: "通俗描述" }
                                spacer2 := mod.widgets.View{ width: Fill, height: Fit }
                                pos_btn := JiangouSectionBtn{ text: "正面例子" }
                                pos_on_btn := JiangouSectionBtnOn{ visible: false, text: "正面例子" }
                                spacer3 := mod.widgets.View{ width: Fill, height: Fit }
                                neg_btn := JiangouSectionBtn{ text: "反面例子" }
                                neg_on_btn := JiangouSectionBtnOn{ visible: false, text: "反面例子" }
                                spacer4 := mod.widgets.View{ width: Fill, height: Fit }
                                affect_btn := JiangouSectionBtn{ text: "影响什么" }
                                affect_on_btn := JiangouSectionBtnOn{ visible: false, text: "影响什么" }
                                spacer5 := mod.widgets.View{ width: Fill, height: Fit }
                                affected_btn := JiangouSectionBtn{ text: "被啥影响" }
                                affected_on_btn := JiangouSectionBtnOn{ visible: false, text: "被啥影响" }
                                spacer6 := mod.widgets.View{ width: Fill, height: Fit }
                                use_btn := JiangouSectionBtn{ text: "概念作用" }
                                use_on_btn := JiangouSectionBtnOn{ visible: false, text: "概念作用" }
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

/// A send_chat deferred until the background RAG retrieval answers (or the
/// timeout falls back to the BM25 context computed at send time).
struct RagWait {
    query: String,
    rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    fallback: String,
    started: Instant,
}

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
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
        // After ui.handle_event: the dock writes pending_click while the
        // event propagates, so poll it afterwards — a click would otherwise
        // wait for the next event (never comes with a still mouse).
        self.handle_dock_clicks(cx);
        // The dock area covers the whole window on top of the popups, so
        // root dispatch can't reach the popup's buttons/inputs; hand mouse
        // events to the open popup directly (it forwards to its content,
        // which hit-tests geometrically).
        if let Event::MouseDown(_) | Event::MouseUp(_) = event {
            for id in [live_id!(setting_popup), live_id!(about_popup)] {
                let p = self.popup_widget(id).as_popup_panel();
                if p.opened() {
                    p.handle_event(cx, event, &mut Scope::empty());
                    break;
                }
            }
        }
    }
}
