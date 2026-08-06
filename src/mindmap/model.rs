use makepad_widgets::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CARD_W: f64 = 360.0;
pub const CARD_H: f64 = 520.0;
// Resize limits (mouse and Shift+arrow keyboard resizing share these).
pub(crate) const CARD_MIN_SIZE: f64 = 100.0;
pub(crate) const CARD_MAX_SIZE: f64 = 1000.0;
const GAP_X: f64 = 120.0;
const GAP_Y: f64 = 40.0;
const CANVAS_MARGIN: f64 = 60.0;
/// Zoom range for the map view (mouse wheel, QE keys, saved view clamp).
pub(crate) const ZOOM_MIN: f64 = 0.3;
pub(crate) const ZOOM_MAX: f64 = 2.5;

#[derive(Deserialize, Serialize)]
struct MapFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pan: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zoom: Option<f64>,
    nodes: Vec<MapNodeFile>,
}

#[derive(Deserialize, Serialize)]
struct MapNodeFile {
    id: String,
    title: String,
    path: String,
    children: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    h: Option<f64>,
}

#[derive(Clone)]
pub struct Node {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub body: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub pos: DVec2,
    pub size: DVec2,
    pub subtree_h: f64,
}

pub struct MindMapData {
    pub nodes: Vec<Node>,
    pub root: Option<usize>,
    pub max_w: f64,
    pub max_h: f64,
    /// View state (pan, zoom) restored from map.json, applied by the widget.
    pub saved_view: Option<(DVec2, f64)>,
}

impl MindMapData {
    /// Default map file, relative to the app base dir.
    pub const DEFAULT_MAP: &'static str = "maps/map.json";

    /// Load the map JSON at `base/map_file`. Node body paths inside the JSON
    /// stay relative to `base` (not the map file's directory).
    pub fn load_from(base: &Path, map_file: &str) -> Option<Self> {
        let map_path = base.join(map_file);
        let map: MapFile = serde_json::from_str(&std::fs::read_to_string(&map_path).ok()?).ok()?;
        let saved_view = match (map.pan, map.zoom) {
            (Some(p), Some(z)) => Some((dvec2(p[0], p[1]), z.clamp(ZOOM_MIN, ZOOM_MAX))),
            _ => None,
        };
        let nodes_json = map.nodes;
        let mut nodes: Vec<Node> = nodes_json
            .iter()
            .map(|n| Node {
                id: n.id.clone(),
                title: n.title.clone(),
                path: base.join(&n.path),
                body: std::fs::read_to_string(base.join(&n.path)).unwrap_or_else(|_| {
                    log!("mindmap: body file missing for node {}: {:?}", n.id, n.path);
                    String::new()
                }),
                parent: None,
                children: Vec::new(),
                pos: DVec2::default(),
                size: dvec2(CARD_W, CARD_H),
                subtree_h: 0.0,
            })
            .collect();
        let id_of = |nodes: &[Node], id: &str| nodes.iter().position(|n| n.id == id);
        let root = id_of(&nodes, "root");
        // Empty maps are valid (zero nodes); a non-empty map without a root
        // is malformed and fails to load, as before.
        if root.is_none() && !nodes.is_empty() {
            return None;
        }
        for i in 0..nodes_json.len() {
            if let Some(children) = &nodes_json[i].children {
                for cid in children {
                    let ci = id_of(&nodes, cid)?;
                    nodes[ci].parent = Some(i);
                    nodes[i].children.push(ci);
                }
            }
        }
        let mut data = MindMapData {
            nodes,
            root,
            max_w: 0.0,
            max_h: 0.0,
            saved_view,
        };
        data.layout();
        // Restore saved card geometry; nodes without it keep the auto layout.
        for (f, j) in nodes_json.iter().zip(&mut data.nodes) {
            if let (Some(x), Some(y)) = (f.x, f.y) {
                j.pos = dvec2(x, y);
            }
            if let (Some(w), Some(h)) = (f.w, f.h) {
                j.size = dvec2(w, h);
            }
        }
        let (mut max_w, mut max_h) = (0.0, 0.0);
        for n in &data.nodes {
            max_w = max_w.max(n.pos.x + n.size.x);
            max_h = max_h.max(n.pos.y + n.size.y);
        }
        data.max_w = max_w + CANVAS_MARGIN;
        data.max_h = max_h + CANVAS_MARGIN;
        Some(data)
    }

    fn layout(&mut self) {
        let Some(root) = self.root else {
            self.max_w = CANVAS_MARGIN;
            self.max_h = CANVAS_MARGIN;
            return;
        };
        self.calc_h(root);
        let mut cursor_y = 0.0;
        let mut max_w = 0.0;
        self.place(root, 0, &mut cursor_y, &mut max_w);
        self.max_h = (cursor_y - GAP_Y).max(CARD_H) + CANVAS_MARGIN;
        self.max_w = max_w + CANVAS_MARGIN;
    }

    fn calc_h(&mut self, i: usize) -> f64 {
        let children = self.nodes[i].children.clone();
        let sum: f64 = children
            .iter()
            .map(|&c| self.calc_h(c))
            .sum::<f64>()
            + (children.len() as f64 - 1.0).max(0.0) * GAP_Y;
        let h = sum.max(CARD_H);
        self.nodes[i].subtree_h = h;
        h
    }

    fn place(&mut self, i: usize, depth: usize, cursor_y: &mut f64, max_w: &mut f64) {
        let x = CANVAS_MARGIN + depth as f64 * (CARD_W + GAP_X);
        *max_w = (*max_w).max(x + CARD_W);
        let y_start = *cursor_y;
        let y = if self.nodes[i].children.is_empty() {
            let y = *cursor_y;
            *cursor_y += CARD_H + GAP_Y;
            y
        } else {
            for c in self.nodes[i].children.clone() {
                self.place(c, depth + 1, cursor_y, max_w);
            }
            (y_start + (*cursor_y - GAP_Y)) / 2.0
        };
        self.nodes[i].pos = dvec2(x, y);
    }

    pub fn edges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.parent.map(|p| (p, i)))
    }
}

/// Remove every node whose card path lives under the deleted dir `dir_rel`
/// ("cards/docs/") from all maps under maps/. The root is never removed (its
/// card body just goes missing), and surviving children of removed nodes are
/// re-attached to their nearest surviving ancestor so no dangling children
/// references remain. Only touched maps are written back.
pub(crate) fn remove_dir_nodes(base: &Path, dir_rel: &str) {
    let Some(entries) = std::fs::read_dir(base.join("maps")).ok() else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(json) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(mut map) = serde_json::from_str::<MapFile>(&json) else {
            continue;
        };
        // Owned parent map: id -> parent id (children arrays may be missing).
        let mut parent_of: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for n in &map.nodes {
            if let Some(children) = &n.children {
                for c in children {
                    parent_of.insert(c.clone(), n.id.clone());
                }
            }
        }
        // The removed set, excluding the root (a map must keep a loadable root).
        let root_id = map.nodes.iter().find(|n| n.id == "root").map(|n| n.id.clone());
        let removed: Vec<String> = map
            .nodes
            .iter()
            .filter(|n| Some(&n.id) != root_id.as_ref() && n.path.starts_with(dir_rel))
            .map(|n| n.id.clone())
            .collect();
        if removed.is_empty() {
            continue;
        }
        let is_removed = |id: &str| removed.iter().any(|r| r == id);
        // Strip removed ids from every children list (including the doomed
        // nodes' own lists, so only survivors remain there for re-attach).
        for n in &mut map.nodes {
            if let Some(children) = n.children.as_mut() {
                children.retain(|c| !is_removed(c));
            }
        }
        // Re-attach each removed node's surviving children to its nearest
        // surviving ancestor (walk past other removed nodes).
        for n in &removed {
            let mut ancestor = parent_of.get(n).cloned();
            while let Some(a) = &ancestor {
                if is_removed(a) {
                    ancestor = parent_of.get(a).cloned();
                } else {
                    break;
                }
            }
            let Some(ancestor) = ancestor else {
                continue;
            };
            let survivors: Vec<String> = map
                .nodes
                .iter()
                .find(|node| &node.id == n)
                .and_then(|node| node.children.as_deref())
                .unwrap_or(&[])
                .to_vec();
            if survivors.is_empty() {
                continue;
            }
            if let Some(a) = map.nodes.iter_mut().find(|node| node.id == ancestor) {
                let list = a.children.get_or_insert_with(Vec::new);
                for s in survivors {
                    if !list.contains(&s) {
                        list.push(s);
                    }
                }
            }
        }
        map.nodes.retain(|n| !is_removed(&n.id));
        if let Ok(out) = serde_json::to_string_pretty(&map) {
            std::fs::write(&p, out).ok();
        }
    }
}

/// Minimal map file content for a brand-new map: zero nodes (a truly empty
/// map — the user starts from a blank canvas).
pub fn new_map_json() -> String {
    serde_json::json!({"nodes":[]}).to_string()
}

/// Card display title: the stem of its body file (the same name the file
/// panel shows); falls back to the legacy JSON title when there's no file.
pub(crate) fn card_title(node: &Node) -> String {
    node.path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| node.title.clone())
}

/// Rename a card's body file to `new_name` (extension defaults to .md when
/// absent) and rewrite its path in every map under maps/ (reuses
/// rewrite_node_paths). A no-op name returns the old path; rename failure or
/// an empty name returns None and leaves everything unchanged.
pub(crate) fn rename_card_file(base: &Path, old: &Path, new_name: &str) -> Option<PathBuf> {
    let name = crate::file_panel::normalize_name(new_name, Some(".md"))?;
    let new_path = old.with_file_name(name);
    if new_path == old {
        return Some(old.to_path_buf());
    }
    std::fs::rename(old, &new_path).ok()?;
    let from_rel = old.strip_prefix(base).ok()?;
    let to_rel = new_path.strip_prefix(base).ok()?;
    rewrite_node_paths(base, &from_rel.to_string_lossy(), &to_rel.to_string_lossy());
    Some(new_path)
}

/// Rewrite node `path` references in every map under maps/ so a renamed
/// card/dir keeps its content wired up. Files match exactly; dirs (trailing
/// "/") match by prefix. Only touched maps are written back.
pub(crate) fn rewrite_node_paths(base: &Path, from_rel: &str, to_rel: &str) {
    let Some(entries) = std::fs::read_dir(base.join("maps")).ok() else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(json) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(mut map) = serde_json::from_str::<MapFile>(&json) else {
            continue;
        };
        let mut changed = false;
        for n in &mut map.nodes {
            if from_rel.ends_with('/') {
                if let Some(rest) = n.path.strip_prefix(from_rel) {
                    n.path = format!("{to_rel}{rest}");
                    changed = true;
                }
            } else if n.path == from_rel {
                n.path = to_rel.to_string();
                changed = true;
            }
        }
        if changed {
            if let Ok(out) = serde_json::to_string_pretty(&map) {
                std::fs::write(&p, out).ok();
            }
        }
    }
}

pub(crate) fn write_map(base: &Path, data: &MindMapData, pan: DVec2, zoom: f64, map_file: &str) {
    let nodes = data
        .nodes
        .iter()
        .map(|n| MapNodeFile {
            id: n.id.clone(),
            title: n.title.clone(),
            path: n
                .path
                .strip_prefix(base)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            children: if n.children.is_empty() {
                None
            } else {
                Some(n.children.iter().map(|&c| data.nodes[c].id.clone()).collect())
            },
            x: Some(n.pos.x),
            y: Some(n.pos.y),
            w: Some(n.size.x),
            h: Some(n.size.y),
        })
        .collect();
    let map = MapFile {
        pan: Some([pan.x, pan.y]),
        zoom: Some(zoom),
        nodes,
    };
    if let Ok(json) = serde_json::to_string_pretty(&map) {
        if let Err(e) = std::fs::write(base.join(map_file), json) {
            log!("mindmap: save {map_file} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_write_reload_preserves_title_and_children() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"Rust","path":"a.md","children":["child"]},{"id":"child","title":"","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load_from(&dir, MindMapData::DEFAULT_MAP).unwrap();
        assert_eq!(data.nodes[0].title, "Rust");
        assert_eq!(data.saved_view, None);
        data.nodes[0].title = "Rust2".into();
        data.nodes[0].pos = dvec2(11.0, 22.0);
        data.nodes[0].size = dvec2(300.0, 400.0);
        write_map(&dir, &data, dvec2(5.0, 6.0), 1.5, MindMapData::DEFAULT_MAP);
        let again = MindMapData::load_from(&dir, MindMapData::DEFAULT_MAP).unwrap();
        assert_eq!(again.nodes[0].title, "Rust2");
        assert_eq!(again.nodes[0].children, vec![1]);
        assert_eq!(again.nodes[1].children, Vec::<usize>::new());
        assert_eq!(again.nodes[0].body, "hello");
        assert_eq!(again.nodes[0].pos, dvec2(11.0, 22.0));
        assert_eq!(again.nodes[0].size, dvec2(300.0, 400.0));
        assert_eq!(again.saved_view, Some((dvec2(5.0, 6.0), 1.5)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_map_preserves_subdir_path() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test2-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("content")).unwrap();
        std::fs::write(dir.join("content/a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"Rust","path":"content/a.md","children":null}]}"#,
        )
        .unwrap();
        let data = MindMapData::load_from(&dir, MindMapData::DEFAULT_MAP).unwrap();
        assert_eq!(data.nodes[0].body, "hello");
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, MindMapData::DEFAULT_MAP);
        let json = std::fs::read_to_string(dir.join("maps/map.json")).unwrap();
        assert!(json.contains("\"path\": \"content/a.md\""), "{json}");
        let again = MindMapData::load_from(&dir, MindMapData::DEFAULT_MAP).unwrap();
        assert_eq!(again.nodes[0].body, "hello");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_switches_map_file() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test3-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("a.md"), "hello").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"One","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("maps/other.json"),
            r#"{"nodes":[{"id":"root","title":"Two","path":"a.md","children":null}]}"#,
        )
        .unwrap();
        let one = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let two = MindMapData::load_from(&dir, "maps/other.json").unwrap();
        assert_eq!(one.nodes[one.root.unwrap()].title, "One");
        assert_eq!(two.nodes[two.root.unwrap()].title, "Two");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn new_map_json_loads_empty() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test4-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("maps/x.json"), new_map_json()).unwrap();
        let data = MindMapData::load_from(&dir, "maps/x.json").unwrap();
        assert!(data.nodes.is_empty());
        assert_eq!(data.root, None);
        // An empty map survives save/reload (write_map iterates all nodes,
        // zero of them, and produces a loadable file again).
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, "maps/x.json");
        let again = MindMapData::load_from(&dir, "maps/x.json").unwrap();
        assert!(again.nodes.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rewrite_node_paths_updates_all_referencing_maps() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test5-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("content/docs")).unwrap();
        let map = |root, path| {
            serde_json::json!({
                "nodes": [
                    {"id": "root", "title": root, "path": path, "children": ["kid"]},
                    {"id": "kid", "title": "", "path": path, "children": null}
                ]
            })
            .to_string()
        };
        std::fs::write(
            dir.join("maps/map.json"),
            map("One", "content/docs/a.md"),
        )
        .unwrap();
        std::fs::write(
            dir.join("maps/other.json"),
            map("Two", "content/docs/a.md"),
        )
        .unwrap();
        std::fs::write(dir.join("maps/untouched.json"), map("Three", "content/b.md")).unwrap();
        // file rename rewrites exact matches in every map
        rewrite_node_paths(&dir, "content/docs/a.md", "content/docs/b.md");
        for f in ["maps/map.json", "maps/other.json"] {
            let json = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(json.contains("content/docs/b.md"), "{f}: {json}");
            assert!(!json.contains("content/docs/a.md"), "{f}: {json}");
        }
        // dir rename rewrites by prefix
        rewrite_node_paths(&dir, "content/docs/", "content/renamed/");
        for f in ["maps/map.json", "maps/other.json"] {
            let json = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(json.contains("content/renamed/b.md"), "{f}: {json}");
        }
        // non-referencing map is untouched
        let json = std::fs::read_to_string(dir.join("maps/untouched.json")).unwrap();
        assert!(json.contains("content/b.md"), "{json}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_card_file_renames_file_and_updates_maps() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test7-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("cards/a.md"), "body").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"","path":"cards/a.md","children":null}]}"#,
        )
        .unwrap();
        let old = dir.join("cards/a.md");
        let new = rename_card_file(&dir, &old, "b").unwrap();
        assert_eq!(new, dir.join("cards/b.md"));
        assert!(new.exists() && !old.exists());
        // map.json now references the new path; the header title follows it
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let node = &data.nodes[data.root.unwrap()];
        assert_eq!(node.path, dir.join("cards/b.md"));
        assert_eq!(card_title(node), "b");
        // unchanged name is a no-op
        assert_eq!(
            rename_card_file(&dir, &new, "b").unwrap(),
            dir.join("cards/b.md")
        );
        // empty name leaves the file alone
        assert_eq!(rename_card_file(&dir, &new, "  "), None);
        assert!(new.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_dir_nodes_drops_cards_and_reparents_survivors() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test6-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        let json = r#"{"nodes":[
            {"id":"root","title":"R","path":"cards/r.md","children":["A"]},
            {"id":"A","title":"A","path":"cards/docs/a.md","children":["B","C"]},
            {"id":"B","title":"B","path":"cards/b.md","children":null},
            {"id":"C","title":"C","path":"cards/docs/c.md","children":["D"]},
            {"id":"D","title":"D","path":"cards/d.md","children":null}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        // untouched: no refs under the doomed dir
        std::fs::write(
            dir.join("maps/other.json"),
            r#"{"nodes":[{"id":"root","title":"O","path":"cards/x.md","children":null}]}"#,
        )
        .unwrap();
        // root lives inside the doomed dir: it must survive anyway
        std::fs::write(
            dir.join("maps/rootdir.json"),
            r#"{"nodes":[{"id":"root","title":"RD","path":"cards/docs/root.md","children":null}]}"#,
        )
        .unwrap();
        remove_dir_nodes(&dir, "cards/docs/");
        // A and C removed; B and D re-parented to the root
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(data.nodes.len(), 3, "root + B + D");
        assert_eq!(data.nodes[data.root.unwrap()].children, vec![1, 2]);
        assert_eq!(data.nodes[1].title, "B");
        assert_eq!(data.nodes[2].title, "D");
        // root in the doomed dir is kept
        let data = MindMapData::load_from(&dir, "maps/rootdir.json").unwrap();
        assert_eq!(data.nodes.len(), 1);
        // untouched map unchanged
        let json = std::fs::read_to_string(dir.join("maps/other.json")).unwrap();
        assert!(json.contains("cards/x.md"), "{json}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
