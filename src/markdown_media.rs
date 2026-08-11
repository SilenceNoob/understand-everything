use makepad_widgets::*;

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event as MdEvent, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.MarkdownMediaLinkBase = #(MarkdownMediaLink::register_widget(vm))

    mod.widgets.MarkdownMediaBase = #(MarkdownMedia::register_widget(vm))

    mod.widgets.MarkdownMediaLink = set_type_default() do mod.widgets.MarkdownMediaLinkBase{
        width: Fit height: Fit
        align: Align{x: 0. y: 0.}

        label_walk: Walk{width: Fit height: Fit}

        draw_icon +: {
            hover: instance(0.0)
            pressed: instance(0.0)

            get_color: fn() {
                return mix(
                    mix(
                        theme.color_label_inner,
                        theme.color_label_inner_hover,
                        self.hover
                    ),
                    theme.color_label_inner_down,
                    self.pressed
                )
            }
        }

        animator: Animator{
            hover: {
                default: @off
                off: AnimatorState{
                    from: {all: Forward {duration: 0.1}}
                    apply: {
                        draw_bg: {pressed: 0.0 hover: 0.0}
                        draw_icon: {pressed: 0.0 hover: 0.0}
                        draw_text: {pressed: 0.0 hover: 0.0}
                    }
                }

                on: AnimatorState{
                    from: {
                        all: Forward {duration: 0.1}
                        pressed: Forward {duration: 0.01}
                    }
                    apply: {
                        draw_bg: {pressed: 0.0 hover: snap(1.0)}
                        draw_icon: {pressed: 0.0 hover: snap(1.0)}
                        draw_text: {pressed: 0.0 hover: snap(1.0)}
                    }
                }

                pressed: AnimatorState{
                    from: {all: Forward {duration: 0.2}}
                    apply: {
                        draw_bg: {pressed: snap(1.0) hover: 1.0}
                        draw_icon: {pressed: snap(1.0) hover: 1.0}
                        draw_text: {pressed: snap(1.0) hover: 1.0}
                    }
                }
            }
        }

        draw_bg +: {
            pressed: instance(0.0)
            hover: instance(0.0)

            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let offset_y = 1.0
                sdf.move_to(0. self.rect_size.y-offset_y)
                sdf.line_to(self.rect_size.x self.rect_size.y-offset_y)
                return sdf.stroke(mix(
                    theme.color_label_inner,
                    theme.color_label_inner_down,
                    self.pressed
                ), mix(0.0, 0.8, self.hover))
            }
        }

        draw_text +: {
            pressed: instance(0.0)
            hover: instance(0.0)

            color_hover: uniform(theme.color_label_inner_hover)
            color_pressed: uniform(theme.color_label_inner_down)

            color: theme.color_label_inner
            text_style: theme.font_regular{
                font_size: theme.font_size_p
            }
            get_color: fn() {
                return mix(
                    mix(
                        self.color,
                        self.color_hover,
                        self.hover
                    ),
                    self.color_pressed,
                    self.pressed
                )
            }
        }
    }

    mod.widgets.MarkdownMedia = set_type_default() do mod.widgets.MarkdownMediaBase{
        width: Fill height: Fit
        flow: Flow.Right{wrap: true}
        padding: theme.mspace_1

        font_size: theme.font_size_p
        font_color: theme.color_label_inner

        paragraph_spacing: 16
        pre_code_spacing: 8
        inline_code_padding: theme.mspace_1
        inline_code_margin: theme.mspace_1
        heading_base_scale: 1.8

        draw_text +: {
            color: theme.color_label_inner
        }

        text_style_normal: theme.font_regular{
            font_size: theme.font_size_p
        }

        text_style_italic: theme.font_italic{
            font_size: theme.font_size_p
        }

        text_style_bold: theme.font_bold{
            font_size: theme.font_size_p
        }

        text_style_bold_italic: theme.font_bold_italic{
            font_size: theme.font_size_p
        }

        text_style_fixed: theme.font_code{
            font_size: theme.font_size_p
        }

        code_layout: Layout{
            flow: Flow.Right{wrap: true}
            padding: Inset{left: theme.space_3, right: theme.space_3, top: theme.space_2, bottom: 10}
        }
        code_walk: Walk{width: Fill height: Fit}

        quote_layout: Layout{
            flow: Flow.Right{wrap: true}
            padding: Inset{left: theme.space_3, right: theme.space_3, top: theme.space_2, bottom: theme.space_2}
        }
        quote_walk: Walk{width: Fill height: Fit}

        list_item_layout: Layout{
            flow: Flow.Right{wrap: true}
            padding: theme.mspace_1
        }
        list_item_walk: Walk{
            height: Fit width: Fill
        }

        sep_walk: Walk{
            width: Fill height: 4.
            margin: theme.mspace_v_1
        }

        draw_block +: {
            line_color: theme.color_label_inner
            sep_color: theme.color_shadow
            quote_bg_color: theme.color_bg_highlight
            quote_fg_color: theme.color_label_inner
            code_color: theme.color_bg_highlight
            selection_color: theme.color_selection_focus
            table_header_bg_color: theme.color_bg_highlight
            table_border_color: theme.color_shadow
            space_1: uniform(theme.space_1)
            space_2: uniform(theme.space_2)
        }

        link := mod.widgets.MarkdownMediaLink{}

        image := Image{
            width: Fill
            fit: ImageFit.Horizontal
            margin: Inset{top: 4, bottom: 4}
        }

        pill_d := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Flow.Right
            align: Align{y: 0.5}
            spacing: 2
            padding: Inset{left: 7, right: 4, top: 0, bottom: 0}
            margin: Inset{left: 2, right: 2}
            show_bg: true
            draw_bg +: {
                color: #2a3f66
                border_radius: 5.0
                border_size: 1.0
                border_color: #3b82f680
            }
            icon := mod.widgets.Icon{
                icon_walk: Walk{width: 7, height: 7}
                draw_icon +: {
                    svg: crate_resource("self:resources/book.svg")
                    color: #93c5fd
                }
            }
            label := mod.widgets.Label{
                draw_text.text_style.font_size: 8.5
                draw_text.color: #93c5fd
            }
        }

        pill_t := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Flow.Right
            align: Align{y: 0.5}
            spacing: 2
            padding: Inset{left: 7, right: 4, top: 0, bottom: 0}
            margin: Inset{left: 2, right: 2}
            show_bg: true
            draw_bg +: {
                color: #55491f
                border_radius: 5.0
                border_size: 1.0
                border_color: #eab30880
            }
            icon := mod.widgets.Icon{
                icon_walk: Walk{width: 7, height: 7}
                draw_icon +: {
                    svg: crate_resource("self:resources/pill-t.svg")
                    color: #fde047
                }
            }
            label := mod.widgets.Label{
                draw_text.text_style.font_size: 8.5
                draw_text.color: #fde047
            }
        }

        pill_e := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Flow.Right
            align: Align{y: 0.5}
            spacing: 2
            padding: Inset{left: 7, right: 4, top: 0, bottom: 0}
            margin: Inset{left: 2, right: 2}
            show_bg: true
            draw_bg +: {
                color: #5c2f35
                border_radius: 5.0
                border_size: 1.0
                border_color: #ef444480
            }
            icon := mod.widgets.Icon{
                icon_walk: Walk{width: 7, height: 7}
                draw_icon +: {
                    svg: crate_resource("self:resources/pill-e.svg")
                    color: #fca5a5
                }
            }
            label := mod.widgets.Label{
                draw_text.text_style.font_size: 8.5
                draw_text.color: #fca5a5
            }
        }

        pill_n := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Flow.Right
            align: Align{y: 0.5}
            spacing: 2
            padding: Inset{left: 7, right: 4, top: 0, bottom: 0}
            margin: Inset{left: 2, right: 2}
            show_bg: true
            draw_bg +: {
                color: #3a4150
                border_radius: 5.0
                border_size: 1.0
                border_color: #94a3b880
            }
            icon := mod.widgets.Icon{
                icon_walk: Walk{width: 7, height: 7}
                draw_icon +: {
                    svg: crate_resource("self:resources/pill-n.svg")
                    color: #cbd5e1
                }
            }
            label := mod.widgets.Label{
                draw_text.text_style.font_size: 8.5
                draw_text.color: #cbd5e1
            }
        }

        pill_c := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Flow.Right
            align: Align{y: 0.5}
            spacing: 2
            padding: Inset{left: 7, right: 4, top: 0, bottom: 0}
            margin: Inset{left: 2, right: 2}
            show_bg: true
            draw_bg +: {
                color: #1f4433
                border_radius: 5.0
                border_size: 1.0
                border_color: #22c55e80
            }
            icon := mod.widgets.Icon{
                icon_walk: Walk{width: 7, height: 7}
                draw_icon +: {
                    svg: crate_resource("self:resources/about.svg")
                    color: #a7f3d0
                }
            }
            label := mod.widgets.Label{
                draw_text.text_style.font_size: 8.5
                draw_text.color: #a7f3d0
            }
        }

        mark := mod.widgets.RoundedView{
            width: Fit
            height: Fit
            flow: Flow.Right
            padding: Inset{left: 4, right: 4}
            margin: Inset{left: 2, right: 2}
            show_bg: true
            draw_bg +: {
                color: #eab3082e
                border_radius: 3.0
                border_size: 0.0
            }
            label := mod.widgets.Label{
                padding: 0
                draw_text.color: #fde68a
            }
        }
    }
}

/// The state of a list at a given nesting level.
struct ListState {
    // Current item number for ordered lists.
    current_number: u64,
    // Start number for ordered lists, None for unordered.
    start_number: Option<u64>,
}

#[derive(Script, ScriptHook, Widget)]
pub struct MarkdownMedia {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    pub text_flow: TextFlow,
    #[live]
    body: ArcStringMut,
    #[live]
    paragraph_spacing: f64,
    #[live]
    pre_code_spacing: f64,
    #[live(false)]
    use_code_block_widget: bool,
    #[rust]
    in_code_block: bool,
    #[rust]
    code_block_string: String,
    #[rust]
    in_splash_block: bool,
    #[rust]
    splash_block_string: String,
    /// Pending `#d/#t/#e/#c/#n` pill: kind plus accumulated text.
    #[rust]
    pill: Option<(PillKind, String)>,
    /// Pending `==...==` mark: accumulated text while an open `==` is active.
    #[rust]
    mark: Option<String>,
    /// Incremented whenever `body` changes, so the parse cache below can be
    /// reused across frames without comparing strings.
    #[rust]
    body_version: u64,
    /// Owned pulldown-cmark event stream (plus the body version it was parsed
    /// from) for the current body. Cards inside a panning canvas re-draw every
    /// frame; caching the parse turns the per-frame cost into a fast owned-vec
    /// iteration.
    #[rust]
    cached_events: Option<(u64, Vec<MdEvent<'static>>)>,
    #[live(false)]
    use_math_widget: bool,
    #[live]
    heading_base_scale: f64,
    /// Base directory for resolving relative image paths in the markdown body.
    #[rust]
    base_dir: Option<PathBuf>,
    /// SVG bytes per image url, so the per-frame re-parse is a no-op (Arc::ptr_eq).
    #[rust]
    svg_cache: HashMap<String, Arc<[u8]>>,
}

impl Widget for MarkdownMedia {
    fn is_interactive(&self) -> bool {
        false
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.text_flow.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        self.begin(cx, walk);
        self.process_markdown_doc(cx);
        self.end(cx);

        DrawStep::done()
    }

    fn text(&self) -> String {
        self.body.as_ref().to_string()
    }

    fn set_text(&mut self, cx: &mut Cx, v: &str) {
        if self.body.as_ref() != v {
            self.body.set(v);
            self.body_version += 1;
            self.text_flow.clear_items();
            self.redraw(cx);
        }
    }
}

impl MarkdownMedia {
    fn process_markdown_doc(&mut self, cx: &mut Cx2d) {
        let tf = &mut self.text_flow;
        // Track state for nested formatting
        let mut list_stack: Vec<ListState> = Vec::new();
        let mut is_first_block = true;
        // Per-column alignments for the current table, and the current cell's
        // column index within its row. Both are reset when a new table starts.
        let mut table_alignments: Vec<Alignment> = Vec::new();
        let mut table_cell_index: usize = 0;

        // Parse once per body version; later frames reuse the owned event
        // list (still re-dispatched every frame — the canvas redraws all
        // children each pass — but the pulldown parse is skipped).
        let events: Vec<MdEvent<'static>> = match self.cached_events.take() {
            Some((version, events)) if version == self.body_version => events,
            _ =>             Parser::new_ext(
                self.body.as_ref(),
                Options::ENABLE_TABLES
                    | Options::ENABLE_MATH
                    | Options::ENABLE_STRIKETHROUGH
                    | Options::ENABLE_SMART_PUNCTUATION,
            )
            .into_iter()
            .map(|e| e.into_static())
            .collect(),
        };

        for event in &events {
            match event {
                MdEvent::Start(Tag::Heading { level, .. }) => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    }
                    is_first_block = false;
                    let heading_base = self.heading_base_scale;
                    let scale = match level {
                        HeadingLevel::H1 => heading_base,
                        HeadingLevel::H2 => heading_base * 0.75,
                        HeadingLevel::H3 => heading_base * 0.58,
                        HeadingLevel::H4 => heading_base * 0.5,
                        HeadingLevel::H5 => heading_base * 0.42,
                        HeadingLevel::H6 => heading_base * 0.33,
                    };
                    tf.push_size_abs_scale(scale);
                    tf.bold.push();
                }
                MdEvent::End(TagEnd::Heading(_level)) => {
                    flush_pending(tf, cx, &mut self.pill, &mut self.mark);
                    tf.bold.pop();
                    tf.font_sizes.pop();
                    tf.new_line_collapsed(cx);
                }
                MdEvent::Start(Tag::Paragraph) => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    }
                    is_first_block = false;
                }
                MdEvent::End(TagEnd::Paragraph) => {
                    flush_pending(tf, cx, &mut self.pill, &mut self.mark);
                }
                MdEvent::Start(Tag::BlockQuote(_)) => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    }
                    is_first_block = false;
                    tf.begin_quote(cx);
                }
                MdEvent::End(TagEnd::BlockQuote(_quote_kind)) => {
                    tf.end_quote(cx);
                }
                MdEvent::Start(Tag::List(first_number)) => {
                    list_stack.push(ListState {
                        start_number: *first_number,
                        current_number: (*first_number).unwrap_or(1),
                    });
                }
                MdEvent::End(TagEnd::List(_is_ordered)) => {
                    list_stack.pop();
                }
                MdEvent::Start(Tag::Item) => {
                    if !is_first_block {
                        tf.new_line_collapsed(cx);
                    }
                    is_first_block = false;
                    let marker = if let Some(state) = list_stack.last_mut() {
                        if state.start_number.is_some() {
                            // Ordered list - use and increment the counter
                            let num = state.current_number;
                            state.current_number += 1;
                            format!("{}.", num)
                        } else {
                            // Unordered list - use bullet
                            "•".to_string()
                        }
                    } else {
                        "•".to_string()
                    };
                    tf.begin_list_item(cx, &marker, 2.5);
                }
                MdEvent::End(TagEnd::Item) => {
                    tf.end_list_item(cx);
                }
                MdEvent::Start(Tag::Emphasis) => {
                    tf.italic.push();
                }
                MdEvent::End(TagEnd::Emphasis) => {
                    tf.italic.pop();
                }
                MdEvent::Start(Tag::Strong) => {
                    tf.bold.push();
                }
                MdEvent::End(TagEnd::Strong) => {
                    tf.bold.pop();
                }
                MdEvent::Start(Tag::Strikethrough) => {
                    tf.strikethrough.push();
                }
                MdEvent::End(TagEnd::Strikethrough) => {
                    tf.strikethrough.pop();
                }
                MdEvent::Start(Tag::Link { .. }) => {
                    let entry_id = tf.new_counted_id();
                    let item = tf.item(cx, entry_id, live_id!(link));
                    item.draw_all_unscoped(cx);
                }
                MdEvent::End(TagEnd::Link) => {
                    // Link handling is done in Start event
                }
                MdEvent::Start(Tag::Image {
                    dest_url, title, ..
                }) => {
                    let base_dir = self.base_dir.clone();
                    let url = dest_url.as_ref();
                    let entry_id = tf.new_counted_id();
                    let item = tf.item(cx, entry_id, live_id!(image));
                    if let Some(base_dir) = &base_dir {
                        let path = base_dir.join(url);
                        if path.exists() {
                            if url.to_lowercase().ends_with(".svg") {
                                let data = self
                                    .svg_cache
                                    .entry(url.to_string())
                                    .or_insert_with(|| {
                                        std::fs::read(&path)
                                            .map(Arc::from)
                                            .unwrap_or_else(|_| Arc::from(Vec::new()))
                                    })
                                    .clone();
                                if !data.is_empty() {
                                    let _ = item.as_image().load_svg_from_shared_data(cx, data);
                                }
                            } else {
                                let _ = item
                                    .as_image()
                                    .load_image_file_by_path_async(cx, &path);
                            }
                        } else {
                            let _ = item.as_image().load_image_http_by_url_async(cx, url);
                        }
                    }
                    item.draw_all(cx, &mut Scope::empty());
                    let _ = title;
                }
                MdEvent::Start(Tag::CodeBlock(kind)) => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.pre_code_spacing);
                    }
                    is_first_block = false;
                    // Check if this is a runsplash block
                    let is_runsplash = matches!(kind, CodeBlockKind::Fenced(lang) if lang.as_ref() == "runsplash");
                    if is_runsplash {
                        self.in_splash_block = true;
                        self.splash_block_string.clear();
                    } else if self.use_code_block_widget {
                        self.in_code_block = true;
                        self.code_block_string.clear();
                    } else {
                        const FIXED_FONT_SIZE_SCALE: f64 = 0.85;
                        tf.push_size_rel_scale(FIXED_FONT_SIZE_SCALE);
                        tf.combine_spaces.push(false);
                        tf.fixed.push();
                        tf.begin_code(cx);
                    }
                }
                MdEvent::End(TagEnd::CodeBlock) => {
                    if self.in_splash_block {
                        self.in_splash_block = false;
                        let entry_id = tf.new_counted_id();
                        let sbs = &self.splash_block_string;

                        // Draw the splash block using the $splash_block template
                        tf.item_with(cx, entry_id, id!(splash_block), |cx, item, _tf| {
                            //let tree = item.widget_tree();
                            //cx.with_vm(|vm| {
                            //    log!("$splash_block widget tree:\n{}", tree.display(vm.heap()));
                            //});
                            item.widget(cx, ids!(splash_view)).set_text(cx, sbs);
                            item.draw_all_unscoped(cx);
                        });
                    } else if self.in_code_block {
                        self.in_code_block = false;
                        let entry_id = tf.new_counted_id();
                        let cbs = &self.code_block_string;

                        // Draw the code block and capture the CodeView widget ref
                        let mut code_view_ref = WidgetRef::empty();
                        tf.item_with(cx, entry_id, id!(code_block), |cx, item, _tf| {
                            item.widget(cx, ids!(code_view)).set_text(cx, cbs);
                            item.draw_all_unscoped(cx);
                            code_view_ref = item.widget(cx, ids!(code_view));
                        });

                        // Register the code view widget for cross-child selection
                        // (its area will be queried at event time, not draw time)
                        tf.push_widget_text_for_selection(code_view_ref, &self.code_block_string);
                    } else {
                        tf.font_sizes.pop();
                        tf.fixed.pop();
                        tf.combine_spaces.pop();
                        tf.end_code(cx);
                    }
                }
                // Inline code
                MdEvent::Code(text) => {
                    const FIXED_FONT_SIZE_SCALE: f64 = 0.85;
                    tf.push_size_rel_scale(FIXED_FONT_SIZE_SCALE);
                    tf.fixed.push();
                    tf.inline_code.push();
                    tf.draw_text(cx, text);
                    tf.font_sizes.pop();
                    tf.fixed.pop();
                    tf.inline_code.pop();
                }
                // Inline math ($...$)
                MdEvent::InlineMath(text) => {
                    if self.use_math_widget {
                        let entry_id = tf.new_counted_id();
                        tf.item_with(cx, entry_id, live_id!(inline_math), |cx, item, _tf| {
                            item.set_text(cx, text);
                            item.draw_all_unscoped(cx);
                        });
                    } else {
                        // Fallback: render as inline code style
                        const FIXED_FONT_SIZE_SCALE: f64 = 0.85;
                        tf.push_size_rel_scale(FIXED_FONT_SIZE_SCALE);
                        tf.fixed.push();
                        tf.inline_code.push();
                        tf.draw_text(cx, text);
                        tf.font_sizes.pop();
                        tf.fixed.pop();
                        tf.inline_code.pop();
                    }
                }
                // Display math ($$...$$)
                MdEvent::DisplayMath(text) => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    }
                    is_first_block = false;

                    if self.use_math_widget {
                        let entry_id = tf.new_counted_id();
                        tf.item_with(cx, entry_id, live_id!(display_math), |cx, item, _tf| {
                            item.set_text(cx, text);
                            item.draw_all_unscoped(cx);
                        });
                    } else {
                        // Fallback: render as code block style
                        tf.begin_code(cx);
                        tf.fixed.push();
                        tf.draw_text(cx, text);
                        tf.fixed.pop();
                        tf.end_code(cx);
                    }
                }
                MdEvent::Text(text) => {
                    if self.in_splash_block {
                        self.splash_block_string.push_str(text);
                    } else if self.in_code_block {
                        self.code_block_string.push_str(text);
                    } else {
                        let text = text.trim_end_matches("\n");
                        let mut in_mark = self.mark.is_some();
                        for (is_mark, seg) in scan_marks(text, in_mark) {
                            if is_mark {
                                if let Some(m) = &mut self.mark {
                                    m.push_str(seg);
                                } else {
                                    flush_pending(tf, cx, &mut self.pill, &mut self.mark);
                                    self.mark = Some(seg.to_string());
                                }
                            } else {
                                if self.mark.is_some() {
                                    flush_mark(tf, cx, &mut self.mark);
                                }
                                for (kind, seg) in scan_pills(seg) {
                                    match kind {
                                        Some(kind) => {
                                            flush_pending(tf, cx, &mut self.pill, &mut self.mark);
                                            self.pill = Some((kind, seg.to_string()));
                                        }
                                        None => {
                                            if let Some((_, pill_text)) = &mut self.pill {
                                                pill_text.push_str(seg);
                                            } else {
                                                tf.draw_text(cx, seg);
                                            }
                                        }
                                    }
                                }
                            }
                            in_mark = is_mark;
                        }
                    }
                }
                MdEvent::SoftBreak | MdEvent::HardBreak => {
                    if self.in_splash_block {
                        self.splash_block_string.push('\n');
                    } else if self.in_code_block {
                        self.code_block_string.push('\n');
                    } else {
                        flush_pending(tf, cx, &mut self.pill, &mut self.mark);
                        tf.new_line_collapsed(cx);
                    }
                }
                MdEvent::Rule => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    }
                    is_first_block = false;
                    tf.sep(cx);
                    tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                }
                MdEvent::TaskListMarker(_) => {
                    // TODO: Implement task list markers
                }
                MdEvent::Start(Tag::Table(alignments)) => {
                    if !is_first_block {
                        tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    }
                    is_first_block = false;
                    tf.begin_table(cx, alignments.len());
                    table_alignments = alignments.to_vec();
                    table_cell_index = 0;
                }
                MdEvent::End(TagEnd::Table) => {
                    tf.end_table(cx);
                    tf.new_line_collapsed_with_spacing(cx, self.paragraph_spacing);
                    table_alignments.clear();
                    table_cell_index = 0;
                }
                MdEvent::Start(Tag::TableHead) => {
                    tf.begin_table_header_row(cx);
                    table_cell_index = 0;
                }
                MdEvent::End(TagEnd::TableHead) => {
                    tf.end_table_row(cx);
                    tf.in_table_header = false;
                }
                MdEvent::Start(Tag::TableRow) => {
                    tf.begin_table_row(cx);
                    table_cell_index = 0;
                }
                MdEvent::End(TagEnd::TableRow) => {
                    tf.end_table_row(cx);
                }
                MdEvent::Start(Tag::TableCell) => {
                    let align_x = table_alignments
                        .get(table_cell_index)
                        .map(alignment_to_x)
                        .unwrap_or(0.0);
                    tf.begin_table_cell(cx, align_x);
                    if tf.in_table_header {
                        tf.bold.push();
                    }
                }
                MdEvent::End(TagEnd::TableCell) => {
                    if tf.in_table_header {
                        tf.bold.pop();
                    }
                    flush_pending(tf, cx, &mut self.pill, &mut self.mark);
                    tf.end_table_cell(cx);
                    table_cell_index += 1;
                }
                MdEvent::InlineHtml(text) => {
                    // Support a handful of inline HTML tags that have no
                    // CommonMark equivalent. Anything not matched is ignored,
                    // matching the pre-existing behavior.
                    match text.trim().to_ascii_lowercase().as_str() {
                        "<sub>" => {
                            tf.push_size_rel_scale(0.7);
                            tf.y_shift_scales.push(0.55);
                        }
                        "</sub>" => {
                            tf.font_sizes.pop();
                            tf.y_shift_scales.pop();
                        }
                        "<sup>" => {
                            tf.push_size_rel_scale(0.7);
                            tf.y_shift_scales.push(-0.2);
                        }
                        "</sup>" => {
                            tf.font_sizes.pop();
                            tf.y_shift_scales.pop();
                        }
                        _ => {}
                    }
                }
                _ => {} // Unimplemented or unnecessary events
            }
        }

        // Put the event list back for the next frame.
        self.cached_events = Some((self.body_version, events));
    }
}

/// Maps pulldown_cmark table-column alignment to `Layout::align.x`.
fn alignment_to_x(alignment: &Alignment) -> f64 {
    match alignment {
        Alignment::None | Alignment::Left => 0.0,
        Alignment::Center => 0.5,
        Alignment::Right => 1.0,
    }
}

/// The kind of a `#d/#t/#e/#c/#n` pill tag.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PillKind {
    /// `#d` — 描述, blue.
    Desc,
    /// `#t` — 迁移, yellow.
    Move,
    /// `#e` — 例子, red.
    Example,
    /// `#c` — 评论/作用, green.
    Comment,
    /// `#n` — 作用, gray.
    Effect,
}

/// Splits a text run into plain-text and pill segments. A `#x` token
/// (`x` in d/t/e/c/n) starts a pill that extends to the next token or the
/// end of the run. A token is only recognized when not followed by an
/// ASCII alphanumeric or `#`, so `#data`/`##d` don't match.
fn scan_pills(text: &str) -> Vec<(Option<PillKind>, &str)> {
    let mut out: Vec<(Option<PillKind>, &str)> = Vec::new();
    let mut region: Option<PillKind> = None;
    let mut region_start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'#' {
            let kind = match bytes[i + 1] {
                b'd' => Some(PillKind::Desc),
                b't' => Some(PillKind::Move),
                b'e' => Some(PillKind::Example),
                b'c' => Some(PillKind::Comment),
                b'n' => Some(PillKind::Effect),
                _ => None,
            };
            if let Some(kind) = kind {
                let after = bytes.get(i + 2).copied().unwrap_or(0);
                if !after.is_ascii_alphanumeric() && after != b'#' {
                    if region_start < i {
                        out.push((region, &text[region_start..i]));
                    }
                    region = Some(kind);
                    region_start = i + 2;
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    if region_start < text.len() {
        out.push((region, &text[region_start..]));
    }
    out
}

/// Splits a text run into alternating non-mark/mark segments on `==`,
/// honoring a pending mark state from a previous run (`in_mark`).
fn scan_marks<'a>(text: &'a str, in_mark: bool) -> Vec<(bool, &'a str)> {
    let mut out = Vec::new();
    let mut flag = in_mark;
    for seg in text.split("==") {
        out.push((flag, seg));
        flag = !flag;
    }
    out
}

/// Draws a pill widget for `text` into the text flow.
fn draw_pill(tf: &mut TextFlow, cx: &mut Cx2d, kind: PillKind, text: &str) {
    if text.is_empty() {
        return;
    }
    let template = match kind {
        PillKind::Desc => live_id!(pill_d),
        PillKind::Move => live_id!(pill_t),
        PillKind::Example => live_id!(pill_e),
        PillKind::Comment => live_id!(pill_c),
        PillKind::Effect => live_id!(pill_n),
    };
    let entry_id = tf.new_counted_id();
    tf.item_with(cx, entry_id, template, |cx, item, _tf| {
        item.widget(cx, ids!(label)).set_text(cx, text.trim());
        item.draw_all_unscoped(cx);
    });
}

/// Flushes a pending pill (if any) into the text flow.
fn flush_pill(tf: &mut TextFlow, cx: &mut Cx2d, pill: &mut Option<(PillKind, String)>) {
    if let Some((kind, text)) = pill.take() {
        draw_pill(tf, cx, kind, &text);
    }
}

/// Draws a `==...==` highlight widget for `text` into the text flow.
fn draw_mark(tf: &mut TextFlow, cx: &mut Cx2d, text: &str) {
    if text.is_empty() {
        return;
    }
    let entry_id = tf.new_counted_id();
    tf.item_with(cx, entry_id, live_id!(mark), |cx, item, tf| {
        // Mirror TextFlow's own run-style selection so the mark label uses
        // the exact font, size, line spacing and metrics of the surrounding
        // text, whatever the context (chat, thinking, card, heading scale).
        let mut style = if tf.fixed.value() > 0 {
            tf.text_style_fixed.clone()
        } else if tf.bold.value() > 0 {
            if tf.italic.value() > 0 {
                tf.text_style_bold_italic.clone()
            } else {
                tf.text_style_bold.clone()
            }
        } else if tf.italic.value() > 0 {
            tf.text_style_italic.clone()
        } else {
            tf.text_style_normal.clone()
        };
        style.font_size = *tf.font_sizes.last().unwrap_or(&tf.font_size);
        let label = item.widget(cx, ids!(label));
        if let Some(mut inner) = label.borrow_mut::<Label>() {
            inner.draw_text.text_style = style;
        } else if let Some(mut inner) = label.cast_inner_mut::<Label>() {
            inner.draw_text.text_style = style;
        }
        label.set_text(cx, text.trim());
        item.draw_all_unscoped(cx);
    });
}

/// Flushes a pending mark (if any) into the text flow.
fn flush_mark(tf: &mut TextFlow, cx: &mut Cx2d, mark: &mut Option<String>) {
    if let Some(text) = mark.take() {
        draw_mark(tf, cx, &text);
    }
}

/// Flushes any pending pill and mark into the text flow.
fn flush_pending(
    tf: &mut TextFlow,
    cx: &mut Cx2d,
    pill: &mut Option<(PillKind, String)>,
    mark: &mut Option<String>,
) {
    flush_pill(tf, cx, pill);
    flush_mark(tf, cx, mark);
}

impl MarkdownMediaRef {
    pub fn set_text(&mut self, cx: &mut Cx, v: &str) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.set_text(cx, v)
    }

    pub fn set_base_dir(&self, path: PathBuf) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.base_dir = Some(path);
        }
    }

    /// The text currently selected in the body ("" when none or not
    /// selectable). Used by 划选生成子卡片.
    pub fn selected_text(&self, _cx: &Cx) -> String {
        self.borrow()
            .map(|w| w.text_flow.selected_text())
            .unwrap_or_default()
    }

}

#[derive(Script, ScriptHook, Widget)]
struct MarkdownMediaLink {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    link: LinkLabel,
}

impl Widget for MarkdownMediaLink {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.link.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.link.draw_walk(cx, scope, walk)
    }

    fn text(&self) -> String {
        self.link.text()
    }

    fn set_text(&mut self, cx: &mut Cx, v: &str) {
        self.link.set_text(cx, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_pills_basic() {
        assert_eq!(
            scan_pills("#d 描述文字 #e 例子文字"),
            vec![
                (Some(PillKind::Desc), " 描述文字 "),
                (Some(PillKind::Example), " 例子文字"),
            ]
        );
    }

    #[test]
    fn scan_pills_all_kinds_and_plain() {
        assert_eq!(
            scan_pills("前置 #d 一 #t 二 #c 三 #n 四 #e 五 后置"),
            vec![
                (None, "前置 "),
                (Some(PillKind::Desc), " 一 "),
                (Some(PillKind::Move), " 二 "),
                (Some(PillKind::Comment), " 三 "),
                (Some(PillKind::Effect), " 四 "),
                (Some(PillKind::Example), " 五 后置"),
            ]
        );
    }

    #[test]
    fn scan_pills_no_tokens() {
        assert_eq!(scan_pills("普通文本"), vec![(None, "普通文本")]);
    }

    #[test]
    fn scan_pills_boundaries() {
        // Alphanumeric right after the token: no match. `##` blocks the
        // first token; a trailing token with no text draws nothing.
        assert_eq!(scan_pills("#data"), vec![(None, "#data")]);
        assert_eq!(scan_pills("x #d"), vec![(None, "x ")]);
        assert_eq!(scan_pills("#t#e"), vec![(None, "#t")]);
    }

    #[test]
    fn scan_marks_alternates() {
        assert_eq!(
            scan_marks("前 ==中== 后", false),
            vec![(false, "前 "), (true, "中"), (false, " 后")]
        );
    }

    #[test]
    fn scan_marks_continues_pending_state() {
        // A run arriving while a mark is open keeps alternating from there.
        assert_eq!(
            scan_marks("中== 后", true),
            vec![(true, "中"), (false, " 后")]
        );
        assert_eq!(
            scan_marks("后", false),
            vec![(false, "后")]
        );
    }

    #[test]
    fn scan_marks_no_tokens() {
        assert_eq!(scan_marks("普通文本", false), vec![(false, "普通文本")]);
        assert_eq!(scan_marks("", false), vec![(false, "")]);
    }

    #[test]
    fn parse_cache_roundtrip_preserves_events() {
        // The cached owned event stream must match a fresh parse exactly.
        let src = "#d 描述 **加粗** `x` [链接](https://x)\n\n- 列表\n- 二\n\n| a | b |\n|---|---|\n| 1 | 2 |";
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_MATH
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_SMART_PUNCTUATION;
        let owned: Vec<MdEvent<'static>> =
            Parser::new_ext(src, options).into_iter().map(|e| e.into_static()).collect();
        let reparsed: Vec<MdEvent<'static>> =
            Parser::new_ext(src, options).into_iter().map(|e| e.into_static()).collect();
        assert_eq!(owned, reparsed);
    }
}
