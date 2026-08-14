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

/// User-data root: `$UE_DATA_DIR` override, else the platform data dir
/// (Linux `~/.local/share`, macOS `~/Library/Application Support`, Windows
/// `%LOCALAPPDATA%`), always under `.../understand-everything`.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("UE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .map(|d| d.join("understand-everything"))
        .unwrap_or_else(app_base_dir)
}

/// Directory the app's fonts/icons ship in (NOT compiled into the binary;
/// DSL resources load from here at runtime). Resolution order: `$UE_RESOURCES_DIR`
/// override, `resources/` next to the executable (release layout: exe +
/// resources/ shipped together), `CARGO_MANIFEST_DIR/resources` (dev under
/// `cargo run`), then the current directory.
pub fn resources_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("UE_RESOURCES_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("resources");
            if p.is_dir() {
                return p;
            }
        }
    }
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(dir).join("resources");
    }
    std::env::current_dir()
        .map(|d| d.join("resources"))
        .unwrap_or_else(|_| PathBuf::from("resources"))
}

/// Absolute path of one resource file in `resources_dir()` (what the DSL's
/// `file_resource(#(...))` needs).
pub fn resource_path(name: &str) -> String {
    resources_dir().join(name).to_string_lossy().into_owned()
}

/// Re-point every registered script resource at our shipped resources/ dir.
///
/// Widgets' theme/component defaults resolve `crate_resource("self:...")`
/// FontMembers at registration time to the BUILD machine's absolute path
/// (`~/.cargo/git/checkouts/...`), which breaks on any other machine — the
/// renderer reads fonts by mmap'ing `abs_path` (Cx::get_resource_font_bytes),
/// so rewriting the path to our local copy fixes every font/icon regardless
/// of where its member was created. Resources whose basename is absent from
/// `resources_dir()` (widgets-only files like NewCMMath, back.svg) are left
/// untouched — harmless, nothing renders through them here.
pub fn relocate_resources(cx: &mut Cx) {
    let dir = resources_dir();
    let mut resources = cx.script_data.resources.resources.borrow_mut();
    for res in resources.iter_mut() {
        let name = res.abs_path.rsplit(['/', '\\']).next().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let local = dir.join(name);
        if !local.is_file() {
            continue;
        }
        let local = local.to_string_lossy().into_owned();
        if res.abs_path != local {
            res.abs_path = local;
        }
    }
}

/// One-time migration: move user data sitting next to the binary (legacy
/// layout) into the platform data dir. Per-item and idempotent — an item is
/// moved only when the data dir lacks it, so a pre-existing data dir (e.g. a
/// leftover cache) never blocks the rest.
pub fn migrate_legacy_data() {
    let data = data_dir();
    let base = app_base_dir();
    migrate_paths(&base, &data);
}

fn migrate_paths(base: &std::path::Path, data: &std::path::Path) {
    const DATA_ITEMS: &[&str] = &[
        "cards",
        "maps",
        "refs",
        "docs",
        "settings.json",
        "progress.json",
        ".rag_cache",
        "models",
    ];
    if base == data {
        return;
    }
    for it in DATA_ITEMS {
        let src = base.join(it);
        if src.exists() && !data.join(it).exists() {
            move_path(&src, &data.join(it));
        }
    }
}

fn move_path(src: &std::path::Path, dst: &std::path::Path) {
    if std::fs::rename(src, dst).is_ok() {
        return;
    }
    if src.is_file() {
        if std::fs::copy(src, dst).is_ok() {
            let _ = std::fs::remove_file(src);
        }
    } else if copy_dir(src, dst).is_ok() {
        let _ = std::fs::remove_dir_all(src);
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let s = e.path();
        let d = dst.join(e.file_name());
        if s.is_dir() {
            copy_dir(&s, &d)?;
        } else {
            std::fs::copy(&s, &d)?;
        }
    }
    Ok(())
}

/// Shared isolated data dir for all tests that touch disk: a per-process
/// temp dir exported as UE_DATA_DIR exactly once (any module calling
/// `data_dir()` thereafter gets this dir, never the real one).
#[cfg(test)]
pub fn test_data_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("ue_test_{}", std::process::id()));
        std::env::set_var("UE_DATA_DIR", &d);
        d
    });
    dir.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ue_util_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn migrate_moves_legacy_data() {
        let base = tmp("migrate_base");
        let data = tmp("migrate_data");
        std::fs::create_dir_all(base.join("cards/sub")).unwrap();
        std::fs::create_dir_all(base.join("maps")).unwrap();
        std::fs::write(base.join("cards/sub/a.md"), "x").unwrap();
        std::fs::write(base.join("maps/m.json"), "{}").unwrap();
        std::fs::write(base.join("settings.json"), "{}").unwrap();
        migrate_paths(&base, &data);
        assert!(data.join("cards/sub/a.md").exists());
        assert!(data.join("maps/m.json").exists());
        assert!(data.join("settings.json").exists());
        assert!(!base.join("cards").exists());
        assert!(!base.join("settings.json").exists());
    }

    #[test]
    fn migrate_backfills_missing_items_only() {
        let base = tmp("migrate_base2");
        let data = tmp("migrate_data2");
        std::fs::create_dir_all(base.join("maps")).unwrap();
        std::fs::create_dir_all(base.join("cards")).unwrap();
        std::fs::write(base.join("maps/a.json"), "{}").unwrap();
        std::fs::write(base.join("cards/b.md"), "y").unwrap();
        std::fs::create_dir_all(data.join("maps")).unwrap();
        std::fs::write(data.join("maps/z.json"), "keep").unwrap();
        migrate_paths(&base, &data);
        // Item already in the data dir stays untouched in both places.
        assert!(base.join("maps/a.json").exists());
        assert!(data.join("maps/z.json").exists());
        // Missing item is moved.
        assert!(data.join("cards/b.md").exists());
        assert!(!base.join("cards").exists());
    }

    #[test]
    fn migrate_same_dir_is_noop() {
        let dir = tmp("migrate_same");
        std::fs::write(dir.join("settings.json"), "{}").unwrap();
        migrate_paths(&dir, &dir);
        assert!(dir.join("settings.json").exists());
    }

    #[test]
    fn resources_dir_honors_override() {
        let dir = tmp("resources_override");
        std::env::set_var("UE_RESOURCES_DIR", &dir);
        assert_eq!(resources_dir(), dir);
        assert_eq!(resource_path("x.ttf"), dir.join("x.ttf").to_string_lossy());
    }
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

/// Side (file/refs) panels occupy this fraction of the window height,
/// centered vertically; the freed strips pass pointer events through to the
/// canvas.
pub const SIDE_PANEL_H_FRAC: f64 = 0.95;

/// Gap (px) kept between a side panel (or its collapsed tab) and the window
/// edge, so open/collapsed panels float slightly off the border.
pub const SIDE_PANEL_GAP: f64 = 8.0;

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

/// In-flight card drag from the file panel onto the canvas: the card's
/// display title and the pointer's current screen position. Written by the
/// file panel while dragging, read by the MindMap to draw the drop ghost.
#[derive(Clone, Debug)]
pub struct CardDrag {
    pub title: String,
    pub pos: DVec2,
}

static CARD_DRAG: Mutex<Option<CardDrag>> = Mutex::new(None);

/// Set (or, with `None`, end) the file-panel card drag.
pub fn set_card_drag(drag: Option<CardDrag>) {
    *CARD_DRAG.lock().unwrap() = drag;
}

/// The active card drag, if any.
pub fn card_drag() -> Option<CardDrag> {
    CARD_DRAG.lock().unwrap().clone()
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
