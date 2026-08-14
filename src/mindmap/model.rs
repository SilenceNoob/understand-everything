use makepad_widgets::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CARD_W: f64 = 360.0;
pub const CARD_H: f64 = 520.0;
// Resize limits (mouse and Shift+arrow keyboard resizing share these).
pub(crate) const CARD_MIN_SIZE: f64 = 100.0;
pub(crate) const CARD_MAX_SIZE: f64 = 1000.0;
const GAP_X: f64 = 120.0;
const GAP_Y: f64 = 40.0;
const CANVAS_MARGIN: f64 = 60.0;
/// Space between a group's frame border and its members' rects (cards, or
/// nested groups' frames). Nested frames are padded recursively, so each
/// level's border sits GROUP_PAD outside its children's.
pub(crate) const GROUP_PAD: f64 = 36.0;
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<GroupFile>,
}

#[derive(Deserialize, Serialize)]
struct GroupFile {
    id: String,
    title: String,
    #[serde(default)]
    cards: Vec<String>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
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
    /// Learning-order number shown as the card's badge; per-map, not a
    /// property of the card file. None = no badge (e.g. the root goal card).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<u32>,
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
    /// Learning-order number for the card badge (per-map, None = no badge).
    pub order: Option<u32>,
}

/// A card group: a titled frame wrapping member cards and/or nested groups.
/// `cards`/`groups` hold resolved node/group indices; the containment graph
/// is a forest (a card or group belongs to at most one parent group).
#[derive(Clone)]
pub struct Group {
    pub id: String,
    pub title: String,
    pub cards: Vec<usize>,
    pub groups: Vec<usize>,
    /// Frame color as "#rrggbb"; None = default (script shader default).
    pub color: Option<String>,
}

pub struct MindMapData {
    pub nodes: Vec<Node>,
    pub groups: Vec<Group>,
    pub root: Option<usize>,
    pub max_w: f64,
    pub max_h: f64,
    /// View state (pan, zoom) restored from map.json, applied by the widget.
    pub saved_view: Option<(DVec2, f64)>,
}

impl MindMapData {
    /// Default map file, relative to the app base dir.
    pub const DEFAULT_MAP: &'static str = "maps/map.json";

    /// Remove node `i` from the tree, re-attaching its children to its parent.
    /// Returns false when `i` is out of bounds, or when it is a parent-less
    /// node with children (a learning-route root — removing it would orphan
    /// the whole route). Parent-less leaves (independent root cards) are
    /// removable.
    pub fn remove_node(&mut self, i: usize) -> bool {
        if i >= self.nodes.len() {
            return false;
        }
        if self.nodes[i].parent.is_none() && !self.nodes[i].children.is_empty() {
            return false;
        }
        let parent = self.nodes[i].parent;
        for &c in self.nodes[i].children.clone().iter() {
            if c >= self.nodes.len() {
                continue;
            }
            self.nodes[c].parent = parent;
            if let Some(p) = parent {
                if !self.nodes[p].children.contains(&c) {
                    self.nodes[p].children.push(c);
                }
            }
        }
        if let Some(p) = parent {
            self.nodes[p].children.retain(|&c| c != i);
        }

        let mut map: Vec<Option<usize>> = Vec::with_capacity(self.nodes.len());
        let mut next = 0;
        for old in 0..self.nodes.len() {
            if old == i {
                map.push(None);
            } else {
                map.push(Some(next));
                next += 1;
            }
        }

        let mut nodes: Vec<Node> = Vec::with_capacity(next);
        for (old, n) in self.nodes.iter().enumerate() {
            if old == i {
                continue;
            }
            nodes.push(Node {
                id: n.id.clone(),
                title: n.title.clone(),
                path: n.path.clone(),
                body: n.body.clone(),
                parent: n.parent.and_then(|p| map[p]),
                children: n.children.iter().filter_map(|&c| map[c]).collect(),
                pos: n.pos,
                size: n.size,
                subtree_h: n.subtree_h,
                order: n.order,
            });
        }
        self.nodes = nodes;
        self.root = self.nodes.iter().position(|n| n.parent.is_none());

        for g in &mut self.groups {
            g.cards = g.cards.iter().filter_map(|&c| map[c]).collect();
        }
        self.prune_empty_groups();
        self.recompute_bounds();
        true
    }

    /// Manual edge ops. `connect` attaches `child` under `parent` (the target
    /// card becomes the parent; the moved card loses its learning-order
    /// number so a stale 序号 badge can't mislead the learning order).
    /// Refused for self/duplicate/cycle (a cycle would make the map file
    /// unloadable). `disconnect` detaches `child` from its parent — the
    /// subtree becomes an independent root card, order cleared.
    pub fn connect(&mut self, parent: usize, child: usize) -> Result<(), &'static str> {
        if parent >= self.nodes.len() || child >= self.nodes.len() {
            return Err("卡片不存在");
        }
        if parent == child {
            return Err("不能把卡片连到自己下面");
        }
        if self.nodes[child].parent == Some(parent) {
            return Ok(()); // already connected; idempotent
        }
        // Cycle check: walk up from `parent`; if `child` sits on that chain,
        // connecting would create a cycle (and make the map file unloadable).
        let mut cur = Some(parent);
        while let Some(i) = cur {
            if i == child {
                return Err("不能连线：会形成循环");
            }
            cur = self.nodes[i].parent;
        }
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
        self.nodes[child].order = None;
        Ok(())
    }

    pub fn disconnect(&mut self, child: usize) {
        if child >= self.nodes.len() {
            return;
        }
        let Some(p) = self.nodes[child].parent else {
            return;
        };
        self.nodes[p].children.retain(|&c| c != child);
        self.nodes[child].parent = None;
        self.nodes[child].order = None;
    }

    /// Remove node `i` and its entire subtree from the map (nodes only; card
    /// files stay on disk). Groups lose the removed members and cascade-drop
    /// when empty. Returns the number of nodes removed.
    pub fn remove_subtree(&mut self, i: usize) -> usize {
        if i >= self.nodes.len() {
            return 0;
        }
        // Collect the subtree (pre-order from i).
        let mut drop: Vec<usize> = Vec::new();
        let mut stack = vec![i];
        while let Some(x) = stack.pop() {
            drop.push(x);
            stack.extend(self.nodes[x].children.iter().copied());
        }
        drop.sort_unstable();
        drop.dedup();

        let mut map: Vec<Option<usize>> = Vec::with_capacity(self.nodes.len());
        let mut next = 0;
        for old in 0..self.nodes.len() {
            if drop.contains(&old) {
                map.push(None);
            } else {
                map.push(Some(next));
                next += 1;
            }
        }
        let mut nodes: Vec<Node> = Vec::with_capacity(next);
        for (old, n) in self.nodes.iter().enumerate() {
            if drop.contains(&old) {
                continue;
            }
            nodes.push(Node {
                id: n.id.clone(),
                title: n.title.clone(),
                path: n.path.clone(),
                body: n.body.clone(),
                parent: n.parent.and_then(|p| map[p]),
                children: n.children.iter().filter_map(|&c| map[c]).collect(),
                pos: n.pos,
                size: n.size,
                subtree_h: n.subtree_h,
                order: n.order,
            });
        }
        self.nodes = nodes;
        self.root = self.nodes.iter().position(|n| n.parent.is_none());

        for g in &mut self.groups {
            g.cards = g.cards.iter().filter_map(|&c| map[c]).collect();
        }
        self.prune_empty_groups();
        self.recompute_bounds();
        drop.len()
    }

    /// Add a standalone (parent-less) node for the card at `path`, placed at
    /// `pos`. Returns its index. The body file must already exist; the node
    /// keeps its manual position (layout() only places tree children).
    pub fn add_detached(&mut self, path: PathBuf, body: String, pos: DVec2) -> usize {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut n = 0;
        let id = loop {
            let id = if n == 0 {
                format!("c{ms}")
            } else {
                format!("c{ms}_{n}")
            };
            if !self.nodes.iter().any(|x| x.id == id) {
                break id;
            }
            n += 1;
        };
        let i = self.nodes.len();
        self.nodes.push(Node {
            id,
            title: String::new(),
            path,
            body,
            parent: None,
            children: Vec::new(),
            pos,
            size: dvec2(CARD_W, CARD_H),
            subtree_h: 0.0,
            order: None,
        });
        self.recompute_bounds();
        i
    }

    /// Attach a generated learning route to root node `root`: one node per
    /// `cards` entry (planned id, title, rel path, parent planned id, learning
    /// order), bodies read from disk. Planned ids only drive parent wiring —
    /// the nodes get fresh ids; unknown parents fall back to `root`. Returns
    /// the new node indices in `cards` order (skipped duplicates stay
    /// unrepresented). Pure data-level op: callers position the new nodes and
    /// refresh widgets/edges.
    pub fn attach_route_nodes(
        &mut self,
        root: usize,
        base: &Path,
        cards: &[(String, String, String, Option<String>, Option<u32>)],
    ) -> Vec<usize> {
        let mut index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut added: Vec<usize> = Vec::with_capacity(cards.len());
        for c in cards {
            let path = base.join(&c.2);
            if self.nodes.iter().any(|n| n.path == path) {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            let i = self.add_detached(path, body, dvec2(0.0, 0.0));
            self.nodes[i].order = c.4;
            index.insert(c.0.clone(), i);
            added.push(i);
        }
        for c in cards {
            let Some(&ci) = index.get(&c.0) else { continue };
            let pi = c
                .3
                .as_deref()
                .and_then(|p| index.get(p).copied())
                .unwrap_or(root);
            self.nodes[ci].parent = Some(pi);
            self.nodes[pi].children.push(ci);
        }
        added
    }

    /// Recompute `max_w`/`max_h` from current node/group geometry.
    fn recompute_bounds(&mut self) {
        let (mut max_w, mut max_h) = (0.0, 0.0);
        for n in &self.nodes {
            max_w = max_w.max(n.pos.x + n.size.x);
            max_h = max_h.max(n.pos.y + n.size.y);
        }
        for gi in 0..self.groups.len() {
            if let Some((p, s)) = group_bounds(&self.groups, &self.nodes, gi, GROUP_PAD) {
                max_w = max_w.max(p.x + s.x);
                max_h = max_h.max(p.y + s.y);
            }
        }
        self.max_w = max_w + CANVAS_MARGIN;
        self.max_h = max_h + CANVAS_MARGIN;
    }

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
                order: n.order,
            })
            .collect();
        let id_of = |nodes: &[Node], id: &str| nodes.iter().position(|n| n.id == id);
        for i in 0..nodes_json.len() {
            if let Some(children) = &nodes_json[i].children {
                for cid in children {
                    let ci = id_of(&nodes, cid)?;
                    nodes[ci].parent = Some(i);
                    nodes[i].children.push(ci);
                }
            }
        }
        // Empty maps are valid (zero nodes). A non-empty map must contain at
        // least one parent-less node (a forest root); if every node has a
        // parent the links form a cycle and the file is malformed.
        if !nodes.is_empty() && nodes.iter().all(|n| n.parent.is_some()) {
            return None;
        }
        // Primary root = the first parent-less node in node order. It drives
        // the auto layout and the startup-page check; further parent-less
        // nodes are independent root cards (extra learning-route goals).
        // The legacy "root" node id is just the primary root's id.
        let root = nodes.iter().position(|n| n.parent.is_none());
        // Resolve groups: member ids -> indices. Empty groups and groups
        // unreachable from any root group (cycles, dangling references) are
        // dropped; the survivors form a forest.
        let mut groups: Vec<Group> = Vec::new();
        let mut raw_grp_ids: Vec<Vec<String>> = Vec::new();
        for gf in &map.groups {
            let mut cards: Vec<usize> = gf.cards.iter().filter_map(|c| id_of(&nodes, c)).collect();
            cards.sort_unstable();
            cards.dedup();
            raw_grp_ids.push(gf.groups.clone());
            groups.push(Group {
                id: gf.id.clone(),
                title: gf.title.clone(),
                cards,
                groups: Vec::new(),
                color: gf.color.clone(),
            });
        }
        for (i, ids) in raw_grp_ids.iter().enumerate() {
            let mut grps: Vec<usize> = ids
                .iter()
                .filter_map(|c| groups.iter().position(|g| g.id == *c))
                .collect();
            grps.sort_unstable();
            grps.dedup();
            grps.retain(|&x| x != i);
            groups[i].groups = grps;
        }
        let mut remove = vec![false; groups.len()];
        for (i, g) in groups.iter().enumerate() {
            if g.cards.is_empty() && g.groups.is_empty() {
                remove[i] = true;
            }
        }
        // Reachability from groups no other group references (computed on the
        // RAW references, so a self-reference alone still marks a group as
        // non-root): anything unreachable is part of a cycle and gets dropped.
        let mut is_child = vec![false; groups.len()];
        for ids in &raw_grp_ids {
            for c in ids {
                if let Some(ci) = groups.iter().position(|g| g.id == *c) {
                    is_child[ci] = true;
                }
            }
        }
        let mut reachable = vec![false; groups.len()];
        let mut stack: Vec<usize> = (0..groups.len()).filter(|&i| !is_child[i]).collect();
        while let Some(gi) = stack.pop() {
            if reachable[gi] {
                continue;
            }
            reachable[gi] = true;
            for &c in &groups[gi].groups {
                if c < groups.len() {
                    stack.push(c);
                }
            }
        }
        for (i, r) in reachable.iter().enumerate() {
            if !r {
                remove[i] = true;
            }
        }
        let mut old_to_new = vec![usize::MAX; groups.len()];
        let mut kept = Vec::new();
        for (i, g) in groups.drain(..).enumerate() {
            if remove[i] {
                continue;
            }
            old_to_new[i] = kept.len();
            kept.push(g);
        }
        for g in &mut kept {
            g.groups = g
                .groups
                .iter()
                .filter_map(|&c| (old_to_new[c] != usize::MAX).then_some(old_to_new[c]))
                .collect();
        }
        let mut data = MindMapData {
            nodes,
            groups: kept,
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
        for gi in 0..data.groups.len() {
            if let Some((p, s)) = group_bounds(&data.groups, &data.nodes, gi, GROUP_PAD) {
                max_w = max_w.max(p.x + s.x);
                max_h = max_h.max(p.y + s.y);
            }
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

    /// Index of the group that directly contains `gi`, if any. NOTE: index
    /// order is not a depth order — ⌘/Ctrl+G appends new wrapper groups after
    /// their children, so parents usually have HIGHER indices.
    pub fn group_parent(&self, gi: usize) -> Option<usize> {
        self.groups.iter().position(|g| g.groups.contains(&gi))
    }

    /// Index of the group directly containing card `ci`, if any.
    pub fn group_of_card(&self, ci: usize) -> Option<usize> {
        self.groups.iter().position(|g| g.cards.contains(&ci))
    }

    /// Whether `from` transitively contains `to` (same group excluded).
    pub fn group_reaches(&self, from: usize, to: usize) -> bool {
        let mut visited = vec![false; self.groups.len()];
        let mut stack = vec![from];
        while let Some(g) = stack.pop() {
            if g >= visited.len() || visited[g] {
                continue;
            }
            visited[g] = true;
            if g == to {
                return true;
            }
            stack.extend(self.groups[g].groups.iter().copied());
        }
        false
    }

    /// Turn a raw selection into group members: cards that already belong to
    /// a group stay there and the group itself is nested instead (so ⌘/Ctrl+G
    /// over cards of existing groups wraps the groups, never pulls the cards
    /// out); groups already transitively contained in another selected group
    /// are dropped (keeps the containment graph a forest).
    pub fn fold_selection(&self, cards: &[usize], grps: &[usize]) -> (Vec<usize>, Vec<usize>) {
        let mut out_cards: Vec<usize> = Vec::new();
        let mut out_grps: Vec<usize> = Vec::new();
        for &c in cards {
            match self.group_of_card(c) {
                None => out_cards.push(c),
                Some(gi) => {
                    if !out_grps.contains(&gi) {
                        out_grps.push(gi);
                    }
                }
            }
        }
        for &gi in grps {
            if !out_grps.contains(&gi) {
                out_grps.push(gi);
            }
        }
        out_cards.sort_unstable();
        out_cards.dedup();
        out_grps.sort_unstable();
        out_grps.dedup();
        let kept: Vec<usize> = out_grps
            .iter()
            .copied()
            .filter(|&gi| !out_grps.iter().any(|&o| o != gi && self.group_reaches(o, gi)))
            .collect();
        out_grps = kept;
        (out_cards, out_grps)
    }

    /// Remove groups left with no members (cascading: a group whose only
    /// members were removed groups dies too). Group indices are renumbered;
    /// node indices are untouched.
    pub fn prune_empty_groups(&mut self) {
        let mut removed = vec![false; self.groups.len()];
        loop {
            let dead: Vec<usize> = (0..self.groups.len())
                .filter(|&i| {
                    !removed[i] && self.groups[i].cards.is_empty() && self.groups[i].groups.is_empty()
                })
                .collect();
            if dead.is_empty() {
                break;
            }
            for &i in &dead {
                removed[i] = true;
            }
            for g in &mut self.groups {
                g.groups.retain(|&c| !removed[c]);
            }
        }
        let mut old_to_new = vec![usize::MAX; self.groups.len()];
        let mut kept = Vec::new();
        for (i, g) in self.groups.drain(..).enumerate() {
            if removed[i] {
                continue;
            }
            old_to_new[i] = kept.len();
            kept.push(g);
        }
        for g in &mut kept {
            g.groups = g
                .groups
                .iter()
                .filter_map(|&c| (old_to_new[c] != usize::MAX).then_some(old_to_new[c]))
                .collect();
        }
        self.groups = kept;
    }
}

/// Bounding box of a group's members (member cards plus nested groups'
/// frames — each nested frame expanded by `pad` — recursively), excluding
/// the outer frame padding. None when empty.
pub(crate) fn group_bounds(
    groups: &[Group],
    nodes: &[Node],
    gi: usize,
    pad: f64,
) -> Option<(DVec2, DVec2)> {
    let mut bbox: Option<(DVec2, DVec2)> = None;
    collect_group_bounds(
        groups,
        nodes,
        gi,
        pad,
        &mut vec![false; groups.len()],
        &mut bbox,
    );
    bbox
}

fn collect_group_bounds(
    groups: &[Group],
    nodes: &[Node],
    gi: usize,
    pad: f64,
    visited: &mut [bool],
    bbox: &mut Option<(DVec2, DVec2)>,
) {
    if gi >= visited.len() || visited[gi] {
        return;
    }
    visited[gi] = true;
    let g = &groups[gi];
    for &c in &g.cards {
        if let Some(n) = nodes.get(c) {
            expand_bbox(bbox, n.pos.x, n.pos.y, n.pos.x + n.size.x, n.pos.y + n.size.y);
        }
    }
    for &gi2 in &g.groups {
        let mut child: Option<(DVec2, DVec2)> = None;
        collect_group_bounds(groups, nodes, gi2, pad, visited, &mut child);
        // A nested group contributes its frame (member bbox + pad), so the
        // outer border sits a full pad clear of the inner one.
        if let Some((p, s)) = child {
            expand_bbox(bbox, p.x - pad, p.y - pad, p.x + s.x + pad, p.y + s.y + pad);
        }
    }
}

fn expand_bbox(bbox: &mut Option<(DVec2, DVec2)>, x0: f64, y0: f64, x1: f64, y1: f64) {
    if let Some((p, s)) = bbox {
        let (maxx, maxy) = (p.x + s.x, p.y + s.y);
        let (minx, miny) = (p.x.min(x0), p.y.min(y0));
        p.x = minx;
        p.y = miny;
        s.x = maxx.max(x1) - minx;
        s.y = maxy.max(y1) - miny;
    } else {
        *bbox = Some((dvec2(x0, y0), dvec2(x1 - x0, y1 - y0)));
    }
}

/// Remove every node whose card path lives under the deleted dir `dir_rel`
/// ("cards/docs/") from all maps under maps/. The root is never removed (its
/// card body just goes missing), and surviving children of removed nodes are
/// re-attached to their nearest surviving ancestor so no dangling children
/// references remain. Only touched maps are written back.
pub(crate) fn remove_dir_nodes(base: &Path, dir_rel: &str) {
    remove_matching(base, &|p| p.starts_with(dir_rel));
}

/// Remove the node referencing exactly `card_rel` from every map under maps/,
/// with the same re-attach/group-cleanup rules as `remove_dir_nodes`.
pub(crate) fn remove_card_node(base: &Path, card_rel: &str) {
    remove_matching(base, &|p| p == card_rel);
}

/// Rel paths of every map (under maps/, recursively) whose nodes reference
/// the card at `card_rel` (e.g. "cards/foo.md").
pub(crate) fn maps_using_card(base: &Path, card_rel: &str) -> Vec<String> {
    let mut out = Vec::new();
    for map_rel in map_jsons(base) {
        let Ok(json) = std::fs::read_to_string(base.join(&map_rel)) else {
            continue;
        };
        let Ok(map) = serde_json::from_str::<MapFile>(&json) else {
            continue;
        };
        if map.nodes.iter().any(|n| n.path == card_rel) {
            out.push(map_rel);
        }
    }
    out
}

/// All map json rel paths under maps/, recursively, sorted.
fn map_jsons(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![String::from("maps")];
    while let Some(dir) = stack.pop() {
        if let Ok(it) = std::fs::read_dir(base.join(&dir)) {
            for e in it.flatten() {
                let rel = e
                    .path()
                    .strip_prefix(base)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if e.path().is_dir() {
                    stack.push(rel);
                } else if rel.ends_with(".json") {
                    out.push(rel);
                }
            }
        }
    }
    out.sort();
    out
}

/// Shared removal body: drop every node whose path matches `pred` from all
/// maps, re-attaching survivors to their nearest surviving ancestor and
/// cascade-dropping groups left empty.
fn remove_matching(base: &Path, pred: &dyn Fn(&str) -> bool) {
    for map_rel in map_jsons(base) {
        let p = base.join(&map_rel);
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
            .filter(|n| Some(&n.id) != root_id.as_ref() && pred(&n.path))
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
        // Groups: prune dead card members, then cascade-drop groups left
        // empty (removing a group never dangles anything: membership is
        // stored on the container, so orphaned groups just become top-level).
        for g in &mut map.groups {
            g.cards.retain(|c| !is_removed(c));
        }
        let mut removed_groups: Vec<String> = Vec::new();
        loop {
            let dead: Vec<String> = map
                .groups
                .iter()
                .filter(|g| {
                    g.cards.is_empty() && g.groups.is_empty() && !removed_groups.contains(&g.id)
                })
                .map(|g| g.id.clone())
                .collect();
            if dead.is_empty() {
                break;
            }
            removed_groups.extend(dead);
            let is_dead = |id: &str| removed_groups.iter().any(|r| r == id);
            for g in &mut map.groups {
                g.groups.retain(|c| !is_dead(c));
            }
        }
        map.groups.retain(|g| !removed_groups.contains(&g.id));
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

/// Per-card quiz mastery: rel card path -> latest quiz score (0..=1). A card
/// absent from this map is 未见 (never tested); 已见 = score >= PASS_SCORE.
/// Stored at the repo root as progress.json (gitignored).
pub type Progress = std::collections::HashMap<String, f64>;

/// The quiz score at/above which a card counts as 已见 (handleable by
/// 经验预测 / direct recall). Below it the 判别/联结 model needs work.
pub const PASS_SCORE: f64 = 0.8;

pub fn load_progress(base: &Path) -> Progress {
    std::fs::read_to_string(base.join("progress.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_progress(base: &Path, progress: &Progress) {
    if let Ok(json) = serde_json::to_string_pretty(progress) {
        let _ = std::fs::write(base.join("progress.json"), json);
    }
}


/// Build map JSON for a generated learning route: a root node (id "root",
/// required for non-empty maps to load) plus the planned cards attached by
/// parent id. `cards` is (id, title, rel_path, parent_id, order) in DFS order;
/// unknown/missing parents fall back to the root. Positions are omitted so
/// the auto-layout arranges the tree.
pub fn route_map_json(
    root_title: &str,
    root_path: &str,
    cards: &[(String, String, String, Option<String>, Option<u32>)],
) -> String {
    let mut nodes = vec![MapNodeFile {
        id: "root".to_string(),
        title: root_title.to_string(),
        path: root_path.to_string(),
        children: None,
        x: None,
        y: None,
        w: None,
        h: None,
        order: None,
    }];
    let mut index = std::collections::HashMap::new();
    index.insert("root", 0usize);
    for (i, c) in cards.iter().enumerate() {
        index.insert(c.0.as_str(), i + 1);
        nodes.push(MapNodeFile {
            id: c.0.clone(),
            title: c.1.clone(),
            path: c.2.clone(),
            children: None,
            x: None,
            y: None,
            w: None,
            h: None,
            order: c.4,
        });
    }
    let mut child_ids: Vec<Vec<String>> = vec![Vec::new(); nodes.len()];
    for c in cards {
        let pi = c
            .3
            .as_deref()
            .and_then(|p| index.get(p).copied())
            .unwrap_or(0);
        child_ids[pi].push(c.0.clone());
    }
    for (i, ids) in child_ids.into_iter().enumerate() {
        nodes[i].children = if ids.is_empty() { None } else { Some(ids) };
    }
    let map = MapFile {
        pan: None,
        zoom: None,
        nodes,
        groups: Vec::new(),
    };
    serde_json::to_string_pretty(&map).unwrap_or_default()
}

/// Card display title: the stem of its body file (the same name the file
/// panel shows); falls back to the legacy JSON title when there's no file.
pub(crate) fn card_title(node: &Node) -> String {
    node.path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| node.title.clone())
}

/// "#rrggbb" -> RGBA with the frame/glow shader alpha (0.45), so every color
/// path uses the same stroke weight. Invalid input -> None (caller falls back
/// to the shader's default color).
pub(crate) fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([
        c(0)? as f32 / 255.0,
        c(2)? as f32 / 255.0,
        c(4)? as f32 / 255.0,
        0.45,
    ])
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
    rewrite_progress_paths(base, &from_rel.to_string_lossy(), &to_rel.to_string_lossy());
    Some(new_path)
}

/// Rewrite a rel path for a rename: files match exactly, dirs (trailing "/")
/// match by prefix. Returns None when `rel` is not affected.
fn rewrite_rel(from_rel: &str, to_rel: &str, rel: &str) -> Option<String> {
    if from_rel.ends_with('/') {
        rel.strip_prefix(from_rel)
            .map(|rest| format!("{to_rel}{rest}"))
    } else if rel == from_rel {
        Some(to_rel.to_string())
    } else {
        None
    }
}

/// Rewrite node `path` references in every map under maps/ (recursively, so
/// maps in subdirectories are covered too) so a renamed card/dir keeps its
/// content wired up. Only touched maps are written back.
pub(crate) fn rewrite_node_paths(base: &Path, from_rel: &str, to_rel: &str) {
    for map_rel in map_jsons(base) {
        let p = base.join(&map_rel);
        let Ok(json) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(mut map) = serde_json::from_str::<MapFile>(&json) else {
            continue;
        };
        let mut changed = false;
        for n in &mut map.nodes {
            if let Some(new_path) = rewrite_rel(from_rel, to_rel, &n.path) {
                n.path = new_path;
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

/// Rewrite progress.json keys (rel card paths -> quiz scores) for a renamed
/// card/dir, so 已见/未见 mastery survives renames. No-op without a file.
pub(crate) fn rewrite_progress_paths(base: &Path, from_rel: &str, to_rel: &str) {
    let path = base.join("progress.json");
    let Ok(json) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(progress) = serde_json::from_str::<Progress>(&json) else {
        return;
    };
    let mut rewritten = Progress::new();
    let mut changed = false;
    for (key, score) in progress {
        match rewrite_rel(from_rel, to_rel, &key) {
            Some(new_key) => {
                rewritten.insert(new_key, score);
                changed = true;
            }
            None => {
                rewritten.insert(key, score);
            }
        }
    }
    if changed {
        save_progress(base, &rewritten);
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
            order: n.order,
        })
        .collect();
    let groups = data
        .groups
        .iter()
        .map(|g| GroupFile {
            id: g.id.clone(),
            title: g.title.clone(),
            cards: g.cards.iter().map(|&c| data.nodes[c].id.clone()).collect(),
            groups: g.groups.iter().map(|&gi| data.groups[gi].id.clone()).collect(),
            color: g.color.clone(),
        })
        .collect();
    let map = MapFile {
        pan: Some([pan.x, pan.y]),
        zoom: Some(zoom),
        nodes,
        groups,
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
    fn add_detached_keeps_pos_and_path_after_reload() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-detached-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        std::fs::write(dir.join("cards/foo.md"), "body").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[{"id":"root","title":"R","path":"cards/root.md","children":null}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("cards/root.md"), "").unwrap();
        let mut data = MindMapData::load_from(&dir, MindMapData::DEFAULT_MAP).unwrap();
        let i = data.add_detached(dir.join("cards/foo.md"), "body".into(), dvec2(123.0, 456.0));
        assert_eq!(data.nodes[i].parent, None);
        assert_eq!(data.nodes[i].pos, dvec2(123.0, 456.0));
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, MindMapData::DEFAULT_MAP);
        let again = MindMapData::load_from(&dir, MindMapData::DEFAULT_MAP).unwrap();
        let node = again
            .nodes
            .iter()
            .find(|n| n.path == dir.join("cards/foo.md"))
            .expect("detached node survives reload");
        assert_eq!(node.pos, dvec2(123.0, 456.0));
        assert_eq!(node.parent, None);
        assert_eq!(node.body, "body");
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
    fn route_map_json_builds_tree() {
        let dir = std::env::temp_dir().join(format!("ue-route-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("cards/route")).unwrap();
        for f in ["00-goal.md", "01-a.md", "02-b.md", "03-c.md"] {
            std::fs::write(dir.join("cards/route").join(f), "").unwrap();
        }
        let json = route_map_json(
            "学会浮力",
            "cards/route/00-goal.md",
            &[
                ("c1".into(), "浮力".into(), "cards/route/01-a.md".into(), None, Some(1)),
                ("c2".into(), "密度".into(), "cards/route/02-b.md".into(), Some("c1".into()), Some(2)),
                ("c3".into(), "浮沉条件".into(), "cards/route/03-c.md".into(), Some("missing".into()), Some(3)),
            ],
        );
        std::fs::write(dir.join("maps/x.json"), json).unwrap();
        let data = MindMapData::load_from(&dir, "maps/x.json").expect("route map loads");
        let root = data.root.expect("has root");
        assert_eq!(data.nodes[root].id, "root");
        assert_eq!(data.nodes[root].title, "学会浮力");
        assert_eq!(data.nodes[root].order, None);
        // c1 and c3 (missing parent) hang off the root; c2 under c1.
        let c1 = data.nodes.iter().position(|n| n.id == "c1").unwrap();
        let c2 = data.nodes.iter().position(|n| n.id == "c2").unwrap();
        let c3 = data.nodes.iter().position(|n| n.id == "c3").unwrap();
        assert_eq!(data.nodes[root].children, vec![c1, c3]);
        assert_eq!(data.nodes[c1].children, vec![c2]);
        assert_eq!(data.nodes[c2].children, Vec::<usize>::new());
        assert_eq!(data.nodes[c1].order, Some(1));
        assert_eq!(data.nodes[c3].order, Some(3));
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
        // a map in a subdirectory is covered too (recursive scan)
        std::fs::create_dir_all(dir.join("maps/nested")).unwrap();
        std::fs::write(
            dir.join("maps/nested/deep.json"),
            map("Four", "content/docs/a.md"),
        )
        .unwrap();
        // file rename rewrites exact matches in every map
        rewrite_node_paths(&dir, "content/docs/a.md", "content/docs/b.md");
        for f in ["maps/map.json", "maps/other.json", "maps/nested/deep.json"] {
            let json = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(json.contains("content/docs/b.md"), "{f}: {json}");
            assert!(!json.contains("content/docs/a.md"), "{f}: {json}");
        }
        // dir rename rewrites by prefix
        rewrite_node_paths(&dir, "content/docs/", "content/renamed/");
        for f in ["maps/map.json", "maps/other.json", "maps/nested/deep.json"] {
            let json = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(json.contains("content/renamed/b.md"), "{f}: {json}");
        }
        // non-referencing map is untouched
        let json = std::fs::read_to_string(dir.join("maps/untouched.json")).unwrap();
        assert!(json.contains("content/b.md"), "{json}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rewrite_progress_paths_rewrites_keys() {
        let dir = std::env::temp_dir().join(format!("ue-progress-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Progress::new();
        p.insert("cards/docs/a.md".to_string(), 0.9);
        p.insert("cards/docs/b.md".to_string(), 0.4);
        p.insert("cards/other.md".to_string(), 0.7);
        save_progress(&dir, &p);
        // dir rename rewrites matching keys by prefix, others survive
        rewrite_progress_paths(&dir, "cards/docs/", "cards/renamed/");
        let loaded = load_progress(&dir);
        assert_eq!(loaded.get("cards/renamed/a.md"), Some(&0.9));
        assert_eq!(loaded.get("cards/renamed/b.md"), Some(&0.4));
        assert_eq!(loaded.get("cards/other.md"), Some(&0.7));
        assert!(loaded.get("cards/docs/a.md").is_none());
        // file rename rewrites the exact key
        rewrite_progress_paths(&dir, "cards/renamed/a.md", "cards/renamed/top.md");
        let loaded = load_progress(&dir);
        assert_eq!(loaded.get("cards/renamed/top.md"), Some(&0.9));
        assert_eq!(loaded.get("cards/renamed/b.md"), Some(&0.4));
        // unrelated rename leaves everything untouched
        rewrite_progress_paths(&dir, "cards/nope/", "cards/x/");
        assert_eq!(load_progress(&dir).len(), 3);
        // missing file is a no-op
        std::fs::remove_file(dir.join("progress.json")).unwrap();
        rewrite_progress_paths(&dir, "cards/docs/", "cards/y/");
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

    #[test]
    fn groups_persist_with_nesting_across_write_reload() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test8-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["a.md", "b.md", "c.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        let json = r#"{"nodes":[
            {"id":"root","title":"R","path":"root.md","children":null},
            {"id":"a","title":"","path":"a.md","children":null},
            {"id":"b","title":"","path":"b.md","children":null},
            {"id":"c","title":"","path":"c.md","children":null}
        ],"groups":[
            {"id":"g1","title":"组 1","cards":["a","b"],"groups":[]},
            {"id":"g2","title":"组 2","cards":["c"],"groups":["g1"]}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        let mut data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(data.groups.len(), 2);
        assert_eq!(data.groups[0].cards, vec![1, 2]);
        assert!(data.groups[0].groups.is_empty());
        assert_eq!(data.groups[1].cards, vec![3]);
        assert_eq!(data.groups[1].groups, vec![0]);
        assert_eq!(data.group_parent(0), Some(1));
        assert_eq!(data.group_of_card(2), Some(0));
        // a leaves g1 -> g1 keeps b; round-trip through write_map
        data.groups[0].cards.retain(|&c| c != 1);
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, "maps/map.json");
        let again = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(again.groups[0].cards, vec![2]);
        assert_eq!(again.groups[1].groups, vec![0]);
        assert_eq!(again.groups[1].cards, vec![3]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_drops_empty_dangling_and_cyclic_groups() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test9-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["root.md", "a.md", "b.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        let json = r#"{"nodes":[
            {"id":"root","title":"","path":"root.md","children":null},
            {"id":"a","title":"","path":"a.md","children":null},
            {"id":"b","title":"","path":"b.md","children":null}
        ],"groups":[
            {"id":"g1","title":"ok","cards":["a","b"],"groups":[]},
            {"id":"g2","title":"dangling","cards":["ghost"],"groups":[]},
            {"id":"g3","title":"empty","cards":[],"groups":[]},
            {"id":"g4","title":"cycle-a","cards":["root"],"groups":["g5"]},
            {"id":"g5","title":"cycle-b","cards":["root"],"groups":["g4"]},
            {"id":"g6","title":"selfref","cards":["root"],"groups":["g6"]}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(data.groups.len(), 1);
        assert_eq!(data.groups[0].id, "g1");
        assert_eq!(data.groups[0].cards, vec![1, 2]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn maps_using_card_lists_referencing_maps_recursively() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-using-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps/backup")).unwrap();
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        std::fs::write(dir.join("cards/a.md"), "x").unwrap();
        let map = |path: &str| {
            format!(r#"{{"nodes":[{{"id":"root","title":"R","path":"{path}","children":null}}]}}"#)
        };
        std::fs::write(dir.join("maps/map.json"), map("cards/a.md")).unwrap();
        std::fs::write(dir.join("maps/backup/old.json"), map("cards/a.md")).unwrap();
        std::fs::write(dir.join("maps/other.json"), map("cards/b.md")).unwrap();
        assert_eq!(
            maps_using_card(&dir, "cards/a.md"),
            vec!["maps/backup/old.json", "maps/map.json"]
        );
        assert_eq!(maps_using_card(&dir, "cards/none.md"), Vec::<String>::new());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_card_node_reparents_survivors_and_cleans_groups() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-remcard-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps/backup")).unwrap();
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        let json = r#"{"nodes":[
            {"id":"root","title":"R","path":"cards/root.md","children":["A"]},
            {"id":"A","title":"A","path":"cards/a.md","children":["B"]},
            {"id":"B","title":"B","path":"cards/b.md","children":null}
        ],"groups":[
            {"id":"g1","title":"grp","cards":["A","B"],"groups":[]}
        ]}"#;
        std::fs::write(dir.join("cards/root.md"), "x").unwrap();
        std::fs::write(dir.join("cards/a.md"), "x").unwrap();
        std::fs::write(dir.join("cards/b.md"), "x").unwrap();
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        std::fs::write(dir.join("maps/backup/old.json"), json).unwrap();
        remove_card_node(&dir, "cards/a.md");
        // A removed: B re-attaches to root; g1 keeps B.
        for f in ["maps/map.json", "maps/backup/old.json"] {
            let data = MindMapData::load_from(&dir, f).unwrap();
            let a = data.nodes.iter().find(|n| n.path == dir.join("cards/a.md"));
            assert!(a.is_none(), "{f} still references the removed card");
            let b = data.nodes.iter().find(|n| n.path == dir.join("cards/b.md")).unwrap();
            let root = data.nodes.iter().find(|n| n.id == "root").unwrap();
            assert_eq!(b.parent, Some(root_idx(&data)), "{f} B not re-attached to root");
            assert!(root.children.contains(&b_idx(&data)), "{f} root lost B");
            assert_eq!(data.groups[0].cards, vec![b_idx(&data)]);
        }
        assert_eq!(maps_using_card(&dir, "cards/a.md"), Vec::<String>::new());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn root_idx(data: &MindMapData) -> usize {
        data.nodes.iter().position(|n| n.id == "root").unwrap()
    }

    fn b_idx(data: &MindMapData) -> usize {
        data.nodes.iter().position(|n| n.path.ends_with("cards/b.md")).unwrap()
    }

    #[test]
    fn remove_dir_nodes_prunes_groups_cascading() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test10-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        let json = r#"{"nodes":[
            {"id":"root","title":"R","path":"cards/root.md","children":null},
            {"id":"A","title":"A","path":"cards/docs/a.md","children":null},
            {"id":"B","title":"B","path":"cards/b.md","children":null}
        ],"groups":[
            {"id":"g1","title":"keeps B","cards":["A","B"],"groups":[]},
            {"id":"g2","title":"chain top","cards":[],"groups":["g3"]},
            {"id":"g3","title":"chain mid","cards":["A"],"groups":["g4"]},
            {"id":"g4","title":"chain leaf","cards":["B"],"groups":[]}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        remove_dir_nodes(&dir, "cards/docs/");
        // A removed: g3 loses its only card but keeps g4 -> survives; g2's
        // card list was empty all along. Nothing cascades; load should work.
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(data.groups.len(), 4);
        assert_eq!(data.groups[0].cards, vec![1], "g1 keeps B");
        let by_id = |id: &str| data.groups.iter().position(|g| g.id == id).unwrap();
        let (g2, g3, g4) = (by_id("g2"), by_id("g3"), by_id("g4"));
        assert_eq!(data.groups[g2].groups, vec![g3]);
        assert_eq!(data.groups[g3].cards, Vec::<usize>::new());
        assert_eq!(data.groups[g3].groups, vec![g4]);
        assert_eq!(data.groups[g4].cards, vec![1]);
        // cascade: drop B too -> g1 empty, g4 empty -> g3 loses g4 AND its
        // cards -> g3 empty -> g2 loses g3 -> g2 empty. All groups gone.
        std::fs::remove_file(dir.join("cards/b.md")).ok();
        let json = std::fs::read_to_string(dir.join("maps/map.json")).unwrap();
        let mut map: MapFile = serde_json::from_str(&json).unwrap();
        for n in &mut map.nodes {
            if n.id == "B" {
                n.path = "cards/docs/b.md".into();
            }
        }
        std::fs::write(dir.join("maps/map.json"), serde_json::to_string_pretty(&map).unwrap()).unwrap();
        remove_dir_nodes(&dir, "cards/docs/");
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert!(data.groups.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fold_selection_nests_cards_into_their_groups() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test11-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["root.md", "a.md", "b.md", "c.md", "d.md", "e.md", "f.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        // nodes: root(0) a(1) b(2) c(3) d(4) e(5) f(6, ungrouped)
        // g1={a,b} g2={c,d} g3={e, groups:[g2]}
        let json = r#"{"nodes":[
            {"id":"root","title":"","path":"root.md","children":null},
            {"id":"a","title":"","path":"a.md","children":null},
            {"id":"b","title":"","path":"b.md","children":null},
            {"id":"c","title":"","path":"c.md","children":null},
            {"id":"d","title":"","path":"d.md","children":null},
            {"id":"e","title":"","path":"e.md","children":null},
            {"id":"f","title":"","path":"f.md","children":null}
        ],"groups":[
            {"id":"g1","title":"g1","cards":["a","b"],"groups":[]},
            {"id":"g2","title":"g2","cards":["c","d"],"groups":[]},
            {"id":"g3","title":"g3","cards":["e"],"groups":["g2"]}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let (g1, g2, g3) = (0, 1, 2);
        // all cards of two groups -> the groups are nested, cards stay put
        let (cards, grps) = data.fold_selection(&[1, 2, 3, 4], &[]);
        assert!(cards.is_empty());
        assert_eq!(grps, vec![g1, g2]);
        // one ungrouped card + one card of g1
        let (cards, grps) = data.fold_selection(&[6, 1], &[]);
        assert_eq!(cards, vec![6]);
        assert_eq!(grps, vec![g1]);
        // all cards of a single group -> pure wrap
        let (cards, grps) = data.fold_selection(&[1, 2], &[]);
        assert!(cards.is_empty());
        assert_eq!(grps, vec![g1]);
        // g2's cards + selected g3: g2 is dropped (g3 already contains it)
        let (cards, grps) = data.fold_selection(&[3, 4], &[g3]);
        assert!(cards.is_empty());
        assert_eq!(grps, vec![g3]);
        // plain ungrouped selection is untouched
        let (cards, grps) = data.fold_selection(&[6], &[]);
        assert_eq!(cards, vec![6]);
        assert!(grps.is_empty());
        // folded groups keep their members (no side effects)
        assert_eq!(data.groups[g1].cards, vec![1, 2]);
        assert_eq!(data.groups[g2].cards, vec![3, 4]);
        assert_eq!(data.groups[g3].groups, vec![g2]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn group_bounds_pads_nested_group_frames() {
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test12-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["root.md", "c.md", "d.md", "e.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        // g2={c,d} g3={e, groups:[g2]}
        let json = r#"{"nodes":[
            {"id":"root","title":"","path":"root.md","children":null},
            {"id":"c","title":"","path":"c.md","children":null},
            {"id":"d","title":"","path":"d.md","children":null},
            {"id":"e","title":"","path":"e.md","children":null}
        ],"groups":[
            {"id":"g2","title":"inner","cards":["c","d"],"groups":[]},
            {"id":"g3","title":"outer","cards":["e"],"groups":["g2"]}
        ]}"#;
        std::fs::write(dir.join("maps/map.json"), json).unwrap();
        let data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let pad = GROUP_PAD;
        // inner group's bounds: raw member-card bbox only
        let mut b2: Option<(DVec2, DVec2)> = None;
        for &ci in &data.groups[0].cards {
            let n = &data.nodes[ci];
            expand_bbox(&mut b2, n.pos.x, n.pos.y, n.pos.x + n.size.x, n.pos.y + n.size.y);
        }
        assert_eq!(group_bounds(&data.groups, &data.nodes, 0, pad), b2);
        // outer group's bounds: inner frame (bbox + pad) ∪ its own cards
        let mut b3: Option<(DVec2, DVec2)> = None;
        for &ci in &data.groups[1].cards {
            let n = &data.nodes[ci];
            expand_bbox(&mut b3, n.pos.x, n.pos.y, n.pos.x + n.size.x, n.pos.y + n.size.y);
        }
        if let Some((p, s)) = b2 {
            expand_bbox(&mut b3, p.x - pad, p.y - pad, p.x + s.x + pad, p.y + s.y + pad);
        }
        assert_eq!(group_bounds(&data.groups, &data.nodes, 1, pad), b3);
        // the outer frame clears the inner one by a full pad on every side
        let (op, os) = group_bounds(&data.groups, &data.nodes, 1, pad).unwrap();
        let (ip, is) = b2.unwrap();
        assert!(op.x <= ip.x - pad && op.x + os.x >= ip.x + is.x + pad);
        assert!(op.y <= ip.y - pad && op.y + os.y >= ip.y + is.y + pad);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn progress_roundtrips_via_json() {
        let dir = std::env::temp_dir().join(format!("ue-progress-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Progress::new();
        p.insert("cards/a.md".to_string(), 0.86);
        p.insert("cards/b.md".to_string(), 0.4);
        save_progress(&dir, &p);
        let loaded = load_progress(&dir);
        assert_eq!(loaded.get("cards/a.md"), Some(&0.86));
        assert_eq!(loaded.get("cards/b.md"), Some(&0.4));
        assert!(loaded.get("cards/c.md").is_none());
        // missing file -> empty map, never an error
        std::fs::remove_file(dir.join("progress.json")).unwrap();
        assert!(load_progress(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn group_color_persists_and_parses() {
        // parse_hex_color: valid + invalid
        let c = parse_hex_color("#7d8bd4").unwrap();
        assert!((c[0] - 0.4902).abs() < 1e-3 && (c[1] - 0.5451).abs() < 1e-3);
        assert!((c[2] - 0.8314).abs() < 1e-3 && (c[3] - 0.45).abs() < 1e-6);
        assert!(parse_hex_color("#xyz123").is_none());
        assert!(parse_hex_color("7d8bd4").is_none());
        assert!(parse_hex_color("#7d8bd").is_none());
        // round-trip through write/load
        let dir = std::env::temp_dir().join(format!("ue-mindmap-test13-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["root.md", "a.md", "b.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"root","title":"","path":"root.md","children":null},
                {"id":"a","title":"","path":"a.md","children":null},
                {"id":"b","title":"","path":"b.md","children":null}
            ],"groups":[{"id":"g1","title":"g1","cards":["a","b"],"groups":[]}]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(data.groups[0].color, None);
        data.groups[0].color = Some("#ff0000".into());
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, "maps/map.json");
        let again = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        assert_eq!(again.groups[0].color.as_deref(), Some("#ff0000"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forest_map_loads_multiple_parentless_roots() {
        let dir = std::env::temp_dir().join(format!("ue-forest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["a.md", "b.md", "c.md", "d.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        // Two trees: root(r1)->a->b and r2->c, plus a detached leaf d.
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"r1","title":"R1","path":"a.md","children":["k1"]},
                {"id":"k1","title":"K1","path":"b.md","children":["k2"]},
                {"id":"k2","title":"K2","path":"c.md","children":null},
                {"id":"r2","title":"R2","path":"d.md","children":null}
            ]}"#,
        )
        .unwrap();
        let data = MindMapData::load_from(&dir, "maps/map.json").expect("forest loads");
        assert_eq!(data.nodes.len(), 4);
        // Primary root = first parent-less node (r1).
        let r1 = data.nodes.iter().position(|n| n.id == "r1").unwrap();
        let r2 = data.nodes.iter().position(|n| n.id == "r2").unwrap();
        assert_eq!(data.root, Some(r1));
        assert_eq!(data.nodes[r1].parent, None);
        assert_eq!(data.nodes[r2].parent, None);
        // write/reload keeps the forest (r2 stays a parent-less root).
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, "maps/map.json");
        let again = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let r2 = again.nodes.iter().position(|n| n.id == "r2").unwrap();
        assert_eq!(again.nodes[r2].parent, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cycle_map_fails_to_load() {
        let dir = std::env::temp_dir().join(format!("ue-cycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        // Every node has a parent (a cycle): malformed.
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"a","title":"A","path":"a.md","children":["b"]},
                {"id":"b","title":"B","path":"a.md","children":["a"]}
            ]}"#,
        )
        .unwrap();
        assert!(MindMapData::load_from(&dir, "maps/map.json").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_node_guards_route_roots() {
        let dir = std::env::temp_dir().join(format!("ue-remove-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        for f in ["r.md", "a.md", "b.md"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"root","title":"R","path":"r.md","children":["a"]},
                {"id":"a","title":"A","path":"a.md","children":null},
                {"id":"b","title":"B","path":"b.md","children":null}
            ]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let root = data.root.unwrap();
        // Parent-less node WITH children (route root) is protected.
        assert!(!data.remove_node(root));
        // Independent parent-less leaf is removable.
        let b = data.nodes.iter().position(|n| n.id == "b").unwrap();
        assert!(data.remove_node(b));
        // A tree child with its own children re-attaches them to the parent.
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"root","title":"R","path":"r.md","children":["a"]},
                {"id":"a","title":"A","path":"a.md","children":["c"]},
                {"id":"c","title":"C","path":"b.md","children":null}
            ]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let a = data.nodes.iter().position(|n| n.id == "a").unwrap();
        let root = data.root.unwrap();
        assert!(data.remove_node(a));
        assert_eq!(data.nodes[root].children, vec![a]);
        assert_eq!(data.nodes[a].parent, Some(root));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attach_route_nodes_wires_parents_orders_and_fallbacks() {
        let dir = std::env::temp_dir().join(format!("ue-attach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        for f in ["r.md", "01.md", "02.md", "03.md"] {
            std::fs::write(dir.join("cards").join(f), "body").unwrap();
        }
        let mut data = MindMapData {
            nodes: Vec::new(),
            groups: Vec::new(),
            root: None,
            max_w: 0.0,
            max_h: 0.0,
            saved_view: None,
        };
        let ri = data.add_detached(dir.join("cards/r.md"), "root".into(), dvec2(0.0, 0.0));
        let cards = [
            ("c1".into(), "一".into(), "cards/01.md".into(), None, Some(1u32)),
            ("c2".into(), "二".into(), "cards/02.md".into(), Some("c1".into()), Some(2)),
            ("c3".into(), "三".into(), "cards/03.md".into(), Some("missing".into()), Some(3)),
        ];
        let added = data.attach_route_nodes(ri, &dir, &cards);
        assert_eq!(added.len(), 3);
        let c1 = added[0];
        let c2 = added[1];
        let c3 = added[2];
        assert_eq!(data.nodes[c1].parent, Some(ri));
        assert_eq!(data.nodes[c2].parent, Some(c1));
        // Unknown planned parent falls back to the route root.
        assert_eq!(data.nodes[c3].parent, Some(ri));
        assert_eq!(data.nodes[ri].children, vec![c1, c3]);
        assert_eq!(data.nodes[c1].children, vec![c2]);
        assert_eq!(data.nodes[c1].order, Some(1));
        assert_eq!(data.nodes[c2].order, Some(2));
        assert_eq!(data.nodes[c3].order, Some(3));
        // Duplicate card path is skipped.
        let again = data.attach_route_nodes(ri, &dir, &cards[..1]);
        assert!(again.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn connect_disconnect_guard_cycles_and_clear_order() {
        let dir = std::env::temp_dir().join(format!("ue-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        std::fs::write(dir.join("cards/a.md"), "x").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"r1","title":"","path":"cards/a.md","children":["k1"]},
                {"id":"k1","title":"","path":"cards/a.md","children":null},
                {"id":"r2","title":"","path":"cards/a.md","children":null}
            ]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let r1 = data.nodes.iter().position(|n| n.id == "r1").unwrap();
        let k1 = data.nodes.iter().position(|n| n.id == "k1").unwrap();
        let r2 = data.nodes.iter().position(|n| n.id == "r2").unwrap();
        data.nodes[k1].order = Some(3);
        // connect r2 under k1: moved card's order clears
        data.connect(k1, r2).unwrap();
        assert_eq!(data.nodes[r2].parent, Some(k1));
        assert_eq!(data.nodes[r2].order, None);
        assert_eq!(data.nodes[k1].children, vec![r2]);
        // duplicate connect is a no-op
        data.connect(k1, r2).unwrap();
        assert_eq!(data.nodes[k1].children, vec![r2]);
        // self connect refused
        assert!(data.connect(r2, r2).is_err());
        // cycle: connecting k1 under r2 (r2 is k1's child) refused
        assert!(data.connect(r2, k1).is_err());
        // cycle through the chain: r1 under r2 refused
        assert!(data.connect(r2, r1).is_err());
        // disconnect: r2 becomes an independent root, order stays cleared
        data.disconnect(r2);
        assert_eq!(data.nodes[r2].parent, None);
        assert!(data.nodes[k1].children.is_empty());
        // write/reload keeps the forest
        write_map(&dir, &data, dvec2(0.0, 0.0), 1.0, "maps/map.json");
        let again = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let r2 = again.nodes.iter().position(|n| n.id == "r2").unwrap();
        assert_eq!(again.nodes[r2].parent, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_subtree_drops_descendants_and_prunes_groups() {
        let dir = std::env::temp_dir().join(format!("ue-subtree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        std::fs::write(dir.join("cards/a.md"), "x").unwrap();
        std::fs::write(
            dir.join("maps/map.json"),
            r#"{"nodes":[
                {"id":"r1","title":"","path":"cards/a.md","children":["k1","k3"]},
                {"id":"k1","title":"","path":"cards/a.md","children":["k2"]},
                {"id":"k2","title":"","path":"cards/a.md","children":null},
                {"id":"k3","title":"","path":"cards/a.md","children":null},
                {"id":"r2","title":"","path":"cards/a.md","children":null}
            ],"groups":[{"id":"g1","title":"g","cards":["k2","k3"],"groups":[]}]}"#,
        )
        .unwrap();
        let mut data = MindMapData::load_from(&dir, "maps/map.json").unwrap();
        let r1 = data.nodes.iter().position(|n| n.id == "r1").unwrap();
        // r1 subtree = r1,k1,k2,k3 → removed; r2 survives; group g1 keeps k3? no:
        // k2/k3 both removed → g1 pruned entirely.
        let removed = data.remove_subtree(r1);
        assert_eq!(removed, 4);
        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.nodes[0].id, "r2");
        assert_eq!(data.nodes[0].parent, None);
        assert!(data.groups.is_empty());
        assert_eq!(data.root, Some(0));
        std::fs::remove_dir_all(&dir).ok();
    }
}

