use makepad_widgets::*;
use std::path::PathBuf;
use std::sync::Mutex;

/// App working directory: the Cargo manifest dir under `cargo run`, the
/// current dir otherwise.
pub fn app_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::current_dir() {
        return dir;
    }
    PathBuf::from(".")
}

/// Lazy widget lookup: return the cached ref, or fill `cache` via `f` once
/// (a failed lookup is never cached, so it retries).
pub fn cached_widget(cache: &mut Option<WidgetRef>, f: impl FnOnce() -> WidgetRef) -> Option<WidgetRef> {
    if cache.is_none() {
        let found = f();
        if !found.is_empty() {
            *cache = Some(found);
        }
    }
    cache.clone()
}

/// On-screen rects of the overlay panels (file/refs/float panels), keyed by
/// widget uid. Each panel re-registers its rect on every draw; the mindmap
/// uses this to skip pointer/wheel events over panels.
static PANEL_RECTS: Mutex<Vec<(u64, Rect)>> = Mutex::new(Vec::new());

/// Side (file/refs) panels occupy this fraction of the body height, centered
/// vertically; the freed strips pass pointer events through to the canvas.
pub const SIDE_PANEL_H_FRAC: f64 = 0.95;

/// Register (or, with `None`, unregister) a panel's window-coord rect.
pub fn set_panel_rect(uid: u64, rect: Option<Rect>) {
    let mut rects = PANEL_RECTS.lock().unwrap();
    rects.retain(|(u, _)| *u != uid);
    if let Some(r) = rect {
        rects.push((uid, r));
    }
}

/// True when `p` (window coords) lands on any registered panel rect.
pub fn over_any_panel(p: DVec2) -> bool {
    PANEL_RECTS.lock().unwrap().iter().any(|(_, r)| r.contains(p))
}

/// Resize direction bitmask (shared by the mindmap cards and FloatPanel,
/// which were line-by-line mirrors of this math).
pub const RESIZE_LEFT: u8 = 1;
pub const RESIZE_RIGHT: u8 = 2;
pub const RESIZE_TOP: u8 = 4;
pub const RESIZE_BOTTOM: u8 = 8;

/// The resize direction(s) for a pointer at `p` within `t` px of `rect`'s
/// edges (0 = not on any edge). Corners return both directions.
pub fn resize_dir(rect: Rect, p: DVec2, t: f64) -> u8 {
    let on_l = (p.x - rect.pos.x).abs() <= t;
    let on_r = (p.x - (rect.pos.x + rect.size.x)).abs() <= t;
    let on_t = (p.y - rect.pos.y).abs() <= t;
    let on_b = (p.y - (rect.pos.y + rect.size.y)).abs() <= t;
    let in_x = p.x >= rect.pos.x - t && p.x <= rect.pos.x + rect.size.x + t;
    let in_y = p.y >= rect.pos.y - t && p.y <= rect.pos.y + rect.size.y + t;
    let mut dir = 0;
    if (on_l || on_r) && in_y {
        dir |= if on_l { RESIZE_LEFT } else { RESIZE_RIGHT };
    }
    if (on_t || on_b) && in_x {
        dir |= if on_t { RESIZE_TOP } else { RESIZE_BOTTOM };
    }
    dir
}

/// Apply one resize drag step: move/resize `pos`/`size` so the edge under
/// `p` follows the pointer, clamped to `min`/`max` (world or screen coords,
/// whatever the caller uses).
pub fn apply_resize(pos: &mut DVec2, size: &mut DVec2, p: DVec2, dir: u8, min: DVec2, max: DVec2) {
    if dir & RESIZE_LEFT != 0 {
        let w = (size.x + pos.x - p.x).clamp(min.x, max.x);
        pos.x += size.x - w;
        size.x = w;
    }
    if dir & RESIZE_RIGHT != 0 {
        size.x = (p.x - pos.x).clamp(min.x, max.x);
    }
    if dir & RESIZE_TOP != 0 {
        let h = (size.y + pos.y - p.y).clamp(min.y, max.y);
        pos.y += size.y - h;
        size.y = h;
    }
    if dir & RESIZE_BOTTOM != 0 {
        size.y = (p.y - pos.y).clamp(min.y, max.y);
    }
}
