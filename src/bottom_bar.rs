use makepad_widgets::*;

use crate::slide_panel::SlideState;
use crate::util::set_panel_rect;

/// Bottom-edge hot zone (px) that reveals the dock when the cursor enters.
const HOT_ZONE: f64 = 60.0;
/// Slide travel for a fully hidden dock. Must stay >= dock height + bottom
/// margin + a few px so progress 0 is fully off-screen; bump if the dock
/// DSL grows taller.
const HIDDEN_Y: f64 = 72.0;
/// Dock tray width (px). Must stay in sync with the `bar` width in the
/// main.rs BottomBar DSL; the hint line is 4/5 of this.
const DOCK_W: f64 = 184.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.BottomBarBase = #(BottomBar::register_widget(vm))

    // Auto-hiding bottom dock: the app's content slot (the 25/50/25
    // button row) is drawn at a window-sized walk shifted down by
    // (1 - progress) * HIDDEN_Y, so progress 0 sits fully below the
    // window bottom and slides up on hover.
    mod.widgets.BottomBar = set_type_default() do mod.widgets.BottomBarBase{
        width: Fill
        height: Fill
        clip_x: false
        clip_y: false

        content := mod.widgets.View{}
        // Bottom-edge hint bar, visible while the dock is hidden; the Rust
        // side fades it out as the dock slides in (alpha tied to slide
        // progress). Same DrawColor pattern as the file panel's divider.
        draw_hint +: {
            color: #ffffff
        }
        // Dock magnification layer (macOS-dock style): per-button icons and
        // labels drawn scaled over the buttons while the cursor is near.
        // The button widgets below stay untouched (clicks, hover, layout).
        draw_icon_setting +: {
            svg: crate_resource("self:resources/setting.svg")
            color: #aab0bc
        }
        draw_icon_about +: {
            svg: crate_resource("self:resources/about.svg")
            color: #aab0bc
        }
        draw_icon_debug +: {
            svg: crate_resource("self:resources/debug.svg")
            color: #aab0bc
        }
        draw_icon_ai +: {
            svg: crate_resource("self:resources/ai.svg")
            color: #aab0bc
        }
        draw_label_setting +: {
            text_style: theme.font_regular{
                font_size: 8.0
            }
            color: #aab0bc
        }
        draw_label_about +: {
            text_style: theme.font_regular{
                font_size: 8.0
            }
            color: #aab0bc
        }
        draw_label_debug +: {
            text_style: theme.font_regular{
                font_size: 8.0
            }
            color: #aab0bc
        }
        draw_label_ai +: {
            text_style: theme.font_regular{
                font_size: 8.0
            }
            color: #aab0bc
        }
        // Hover highlight behind the dock icon under the cursor.
        draw_hover +: {
            color: #2a3242
            border_radius: 4.0
        }
    }
}

#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct BottomBar {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[rust]
    area: Area,
    #[rust]
    content_ref: Option<WidgetRef>,
    #[rust]
    window_size: DVec2,
    /// 0 = hidden below the window bottom, 1 = fully shown.
    #[rust]
    slide: SlideState,
    #[live]
    draw_hint: DrawColor,
    #[live]
    draw_icon_setting: DrawSvg,
    #[live]
    draw_icon_about: DrawSvg,
    #[live]
    draw_icon_debug: DrawSvg,
    #[live]
    draw_icon_ai: DrawSvg,
    #[live]
    draw_label_setting: DrawText,
    #[live]
    draw_label_about: DrawText,
    #[live]
    draw_label_debug: DrawText,
    #[live]
    draw_label_ai: DrawText,
    /// Last cursor x (window coords) while over the dock, for magnification.
    #[rust]
    cursor_x: f64,
    /// Hover highlight behind the icon under the cursor (button bg
    /// replacement — the dock is drawn entirely from Rust now).
    #[live]
    draw_hover: DrawColor,
    /// The four dock slots (window coords), recomputed every frame by
    /// layout_dock. The single source of truth for drawing (icons, labels,
    /// hover) and click hit-testing — no widget lookups involved.
    #[rust]
    col_rects: [Rect; 4],
    /// Column index + press position of an in-progress click (tap = press
    /// and release within 6px on the same slot). Kept across MouseUps:
    /// macOS may deliver a burst of events per click (and the dock may
    /// still be sliding), so the first release that matches wins instead of
    /// the first release consuming the press and losing the tap.
    #[rust]
    pressed_col: Option<(usize, DVec2)>,
    /// True once the current press has delivered its tap, so the event
    /// burst can't re-fire it.
    #[rust]
    tap_sent: bool,
    /// A completed tap, consumed by the app via take_clicked().
    #[rust]
    pending_click: Option<usize>,
}

impl ScriptHook for BottomBar {}

impl WidgetNode for BottomBar {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }

    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        self.walk
    }

    fn area(&self) -> Area {
        self.area
    }

    fn redraw(&mut self, cx: &mut Cx) {
        cx.redraw_area_and_children(self.area);
    }

    fn children(&self, visit: &mut dyn FnMut(LiveId, WidgetRef)) {
        self.view.children(visit);
    }
}

impl Widget for BottomBar {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.slide.handle_event(cx, event) {
            self.redraw(cx);
        }
        // Reveal while the cursor is in the bottom-edge hot zone; hide once
        // it leaves. The area rect (full window) is refreshed every draw.
        if let Event::MouseMove(e) = event {
            let rect = self.area.rect(cx);
            let want = e.abs.y > rect.pos.y + rect.size.y - HOT_ZONE;
            if want != self.slide.opened {
                self.slide.set(cx, want);
            }
            self.cursor_x = e.abs.x;
            if self.slide.opened {
                self.redraw(cx);
            }
        }
        // Tap detection on the dock slots (pure coordinates — the dock
        // draws no widgets of its own anymore).
        if let Event::MouseDown(fd) = event {
            if fd.button.is_primary() && self.slide.opened {
                if let Some(i) = self.hit_col(fd.abs) {
                    self.pressed_col = Some((i, fd.abs));
                    self.tap_sent = false;
                }
            }
        }
        if let Event::MouseUp(fu) = event {
            if let Some((i, down)) = self.pressed_col {
                if !self.tap_sent
                    && (fu.abs - down).length() < 6.0
                    && self.hit_col(fu.abs) == Some(i)
                {
                    self.tap_sent = true;
                    self.pending_click = Some(i);
                }
            }
        }
        if let Some(content) = self.content_widget(cx) {
            content.handle_event(cx, event, scope);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        self.window_size = cx.current_pass_size();
        cx.begin_turtle(self.walk, self.layout);
        // Let the mindmap skip presses over the dock hot zone (it draws no
        // widgets that could grab the finger itself).
        set_panel_rect(
            self.uid.0,
            Some(Rect {
                pos: dvec2(0.0, self.window_size.y - HOT_ZONE),
                size: dvec2(self.window_size.x, HOT_ZONE),
            }),
        );
        if let Some(content) = self.content_widget(cx) {
            // Same clip trick as FloatPanel: this turtle's clip is disabled,
            // so push the window rect — draw_clip data and hit-testing then
            // resolve correctly, and the dock is clipped at the bottom edge
            // while sliding.
            let window_rect = Rect {
                pos: DVec2::default(),
                size: self.window_size,
            };
            cx.push_clip_rect(window_rect);
            // Hint bar: fully visible while the dock is hidden (progress 0),
            // fades out as the dock slides in and rides the same eased
            // translation (HIDDEN_Y travel), drawn under it so the rising
            // dock never shows the fading line over its surface.
            let a = 1.0 - self.slide.progress;
            if a > 0.0 {
                self.draw_hint.color = Vec4f {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                    w: a as f32 * 0.4,
                };
                let w = DOCK_W * 0.8;
                self.draw_hint.draw_abs(
                    cx,
                    Rect {
                        pos: dvec2(
                            self.window_size.x / 2.0 - w / 2.0,
                            self.window_size.y - 16.0 - self.slide.progress * HIDDEN_Y,
                        ),
                        size: dvec2(w, 4.0),
                    },
                );
            }
            if self.slide.opened {
                self.layout_dock();
            }
            let walk = Walk {
                abs_pos: Some(dvec2(0.0, (1.0 - self.slide.progress) * HIDDEN_Y)),
                width: Size::Fixed(self.window_size.x),
                height: Size::Fixed(self.window_size.y),
                ..Walk::default()
            };
            let _ = content.draw_walk(cx, scope, walk);
            if self.slide.opened {
                self.draw_dock(cx);
            }
            cx.pop_clip_rect();
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl BottomBar {
    fn content_widget(&mut self, cx: &Cx) -> Option<WidgetRef> {
        if self.content_ref.is_none() {
            self.content_ref = Some(self.view.widget(cx, ids!(content)));
        }
        self.content_ref.clone()
    }

    /// The dock slot index under `abs` (window coords), None between slots.
    fn hit_col(&self, abs: DVec2) -> Option<usize> {
        self.col_rects
            .iter()
            .position(|r| r.size.x > 0.0 && r.contains(abs))
    }

    /// Fixed compact layout (no magnification): the four 34px slots with a
    /// 4px gap are centered on the window, riding the slide. Writes
    /// col_rects — the single source of truth for drawing and clicks.
    fn layout_dock(&mut self) {
        const BASE_W: f64 = 34.0;
        const BASE_H: f64 = 51.0;
        const GAP: f64 = 12.0;
        // Tray top: window bottom - bottom margin 14 - tray height 59 +
        // padding 4, riding the slide.
        let base_y = self.window_size.y - 69.0 + (1.0 - self.slide.progress) * HIDDEN_Y;
        let total = BASE_W * 4.0 + GAP * 3.0;
        let left = self.window_size.x * 0.5 - total * 0.5;
        for i in 0..4 {
            self.col_rects[i] = Rect {
                pos: dvec2(left + i as f64 * (BASE_W + GAP), base_y),
                size: dvec2(BASE_W, BASE_H),
            };
        }
    }

    /// Draw the whole dock from Rust: hover highlight, fixed-size icon and
    /// label per slot, all from col_rects. Single layer — no ghosting, no
    /// widget lookups.
    fn draw_dock(&mut self, cx: &mut Cx2d) {
        for i in 0..4 {
            let r = self.col_rects[i];
            if r.size.x <= 0.0 {
                continue;
            }
            // Hover highlight (replaces the old button bg): icon area only,
            // keeping the label text below clear of the highlight.
            if self.cursor_x >= r.pos.x && self.cursor_x <= r.pos.x + r.size.x {
                self.draw_hover.draw_abs(
                    cx,
                    Rect {
                        pos: dvec2(r.pos.x + 3.0, r.pos.y + 12.0),
                        size: dvec2(r.size.x - 6.0, 20.0),
                    },
                );
            }
            let icon_center = dvec2(r.center().x, r.pos.y + 14.0 + 8.0);
            self.draw_icon(i).draw_walk(
                cx,
                Walk {
                    abs_pos: Some(icon_center - dvec2(8.0, 8.0)),
                    width: Size::Fixed(16.0),
                    height: Size::Fixed(16.0),
                    ..Walk::default()
                },
            );
            self.draw_label(i).draw_walk(
                cx,
                Walk {
                    // Wide area centered on the icon, so the text is truly
                    // centered.
                    abs_pos: Some(dvec2(
                        r.center().x - 32.0,
                        r.pos.y + 14.0 + 16.0 + 3.0,
                    )),
                    width: Size::Fixed(64.0),
                    height: Size::Fit {
                        min: None,
                        max: None,
                    },
                    ..Walk::default()
                },
                Align {
                    x: 0.5,
                    y: 0.5,
                },
                Self::label_text(i),
            );
        }
    }

    fn draw_icon(&mut self, i: usize) -> &mut DrawSvg {
        match i {
            0 => &mut self.draw_icon_setting,
            1 => &mut self.draw_icon_about,
            2 => &mut self.draw_icon_debug,
            _ => &mut self.draw_icon_ai,
        }
    }

    fn draw_label(&mut self, i: usize) -> &mut DrawText {
        match i {
            0 => &mut self.draw_label_setting,
            1 => &mut self.draw_label_about,
            2 => &mut self.draw_label_debug,
            _ => &mut self.draw_label_ai,
        }
    }

    fn label_text(i: usize) -> &'static str {
        match i {
            0 => "Setting",
            1 => "About",
            2 => "Debug",
            _ => "AI",
        }
    }
}

impl BottomBarRef {
    /// A completed dock-slot tap (0=Setting, 1=About, 2=Debug, 3=AI),
    /// consumed once per tap.
    pub fn take_clicked(&self) -> Option<usize> {
        self.borrow_mut().and_then(|mut w| w.pending_click.take())
    }
}
