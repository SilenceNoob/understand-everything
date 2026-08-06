use makepad_widgets::*;

/// Exponential ease rate (1/s); settles in ~0.2s.
const SLIDE_EASE: f64 = 14.0;

/// Slide-in side-panel animation state, shared by the file panel and the
/// refs panel (which were line-by-line mirrors of this logic).
#[derive(Default)]
pub struct SlideState {
    /// 0 = collapsed off the edge, 1 = fully open; eases toward the target
    /// on timer ticks.
    pub opened: bool,
    pub progress: f64,
    pub slide_timer: Option<Timer>,
    pub last_timer_time: f64,
}

impl SlideState {
    /// Ease `slide` toward its target on the 60Hz timer tick; stops the
    /// timer once settled. Returns true when `event` belonged to the slide
    /// timer (the caller then redraws).
    pub fn handle_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Some(timer) = self.slide_timer else {
            return false;
        };
        let Some(te) = timer.is_event(event) else {
            return false;
        };
        let now = te.time.unwrap_or(0.0);
        // first tick has no baseline; fall back to one 60Hz frame
        let dt = if self.last_timer_time == 0.0 {
            1.0 / 60.0
        } else {
            (now - self.last_timer_time).max(0.0)
        };
        self.last_timer_time = now;
        let target = if self.opened { 1.0 } else { 0.0 };
        self.progress += (target - self.progress) * (1.0 - (-dt * SLIDE_EASE).exp());
        if (target - self.progress).abs() < 1e-3 {
            self.progress = target;
            cx.stop_timer(timer);
            self.slide_timer = None;
        }
        true
    }

    /// Flip opened/closed and (re)start the 60Hz slide timer.
    pub fn toggle(&mut self, cx: &mut Cx) {
        self.opened = !self.opened;
        if self.slide_timer.is_none() {
            self.slide_timer = Some(cx.start_interval(1.0 / 60.0));
            self.last_timer_time = 0.0;
        }
    }

    /// Drive the slide toward `opened` (no-op when already there).
    pub fn set(&mut self, cx: &mut Cx, opened: bool) {
        if self.opened == opened {
            return;
        }
        self.toggle(cx);
    }
}

/// Context-menu geometry (shared by both panels' DSL menus, which hardcode
/// the same numbers in script).
pub const MENU_W: f64 = 220.0;
pub const MENU_ITEM_H: f64 = 32.0;
pub const MENU_PAD: f64 = 6.0;

/// Clamp a right-click at `abs` to a menu of `item_count` rows inside
/// `panel` (at the panel's edge when the panel is narrower than the menu).
pub fn menu_rect(panel: Rect, abs: DVec2, item_count: usize) -> Rect {
    let h = MENU_PAD * 2.0 + item_count as f64 * MENU_ITEM_H;
    // clamp(min > max) panics; keep the menu inside the panel, or at its
    // edge when the panel is narrower than the menu.
    let max_x = (panel.pos.x + panel.size.x - MENU_W).max(panel.pos.x);
    let max_y = (panel.pos.y + panel.size.y - h).max(panel.pos.y);
    Rect {
        pos: dvec2(
            abs.x.clamp(panel.pos.x, max_x),
            abs.y.clamp(panel.pos.y, max_y),
        ),
        size: dvec2(MENU_W, h),
    }
}

/// The menu item index under `abs` (None outside the menu or past the last
/// item).
pub fn menu_item_index(menu: Rect, item_count: usize, abs: DVec2) -> Option<usize> {
    if !menu.contains(abs) {
        return None;
    }
    let idx = ((abs.y - menu.pos.y - MENU_PAD) / MENU_ITEM_H).floor() as isize;
    if idx < 0 || idx as usize >= item_count {
        return None;
    }
    Some(idx as usize)
}
