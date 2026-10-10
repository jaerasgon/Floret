//! Structural edits on a `PatchFile`: ids, add / delete / duplicate nodes,
//! grid snapping. Plain functions so they test without a window.

use super::ports::find_node;
use floret_engine::patch::{PatchEdge, PatchFile, PatchNode, Position};
use floret_engine::spec::Catalogue;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

/// Width × height seeded into a new Renderer (Preferences → Canvas).
static DEFAULT_RENDER_SIZE: AtomicU64 = AtomicU64::new((1080 << 32) | 1920);

pub fn set_default_render_size(w: u32, h: u32) {
    DEFAULT_RENDER_SIZE.store((u64::from(w) << 32) | u64::from(h), Ordering::Relaxed);
}

pub fn default_render_size() -> (u32, u32) {
    let v = DEFAULT_RENDER_SIZE.load(Ordering::Relaxed);
    ((v >> 32) as u32, v as u32)
}

/// Default grid step for "snap to grid" (matches the canvas dot grid).
pub const GRID: f64 = 24.0;
/// Grid steps offered in Preferences (`SettingsModal.tsx`).
pub const GRID_STEPS: [u32; 4] = [8, 16, 24, 32];

pub fn snap_to(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

/// Snap a point to the grid of `step` anchored at `origin`.
pub fn snap_point(p: Position, step: f64, origin: Position) -> Position {
    Position {
        x: origin.x + snap_to(p.x - origin.x, step),
        y: origin.y + snap_to(p.y - origin.y, step),
    }
}

fn id_prefix(def_type: &str) -> String {
    def_type
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A node id not used in `patch`: `<type with / → _>_<n>`.
pub fn fresh_node_id(patch: &PatchFile, def_type: &str) -> String {
    fresh_node_ids(patch, [def_type]).remove(0)
}

/// Fresh ids for new nodes of the given types, unique against the patch
/// and each other.
pub fn fresh_node_ids<'a>(
    patch: &PatchFile,
    types: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut used: HashSet<String> = patch.nodes.iter().map(|n| n.id.clone()).collect();
    types
        .into_iter()
        .map(|t| {
            let prefix = id_prefix(t);
            let id = (1..)
                .map(|i| format!("{prefix}_{i}"))
                .find(|id| !used.contains(id))
                .expect("unbounded");
            used.insert(id.clone());
            id
        })
        .collect()
}

/// An empty patch with the same header as `like`.
pub fn empty_like(like: &PatchFile) -> PatchFile {
    PatchFile {
        kind: like.kind.clone(),
        version: like.version,
        app_version: like.app_version.clone(),
        meta: None,
        nodes: Vec::new(),
        edges: Vec::new(),
        loop_length: None,
        seed: 0,
        presets: Vec::new(),
        // Same header: how the nodes treat spreads, too.
        legacy_slot0: like.legacy_slot0,
    }
}

pub fn fresh_edge_id(patch: &PatchFile) -> String {
    let used: HashSet<&str> = patch.edges.iter().map(|e| e.id.as_str()).collect();
    (patch.edges.len() + 1..)
        .map(|i| format!("e{i}"))
        .find(|id| !used.contains(id.as_str()))
        .expect("unbounded")
}

/// `addNode`: a fresh node with the type's default params at `pos`
/// (canvas coordinates, root scope). Returns its id. `key` is a node type,
/// or a search entry key presetting an op family's op (`math/math#sin`).
pub fn add_node(
    patch: &mut PatchFile,
    cat: &Catalogue,
    key: &str,
    pos: Position,
) -> Option<String> {
    let e = super::search::entry(cat, key)?;
    let (spec, def_type) = (e.spec, e.spec.ty.as_str());
    let id = fresh_node_id(patch, def_type);
    let mut params = e.params();
    if def_type == "render/renderer" {
        let (w, h) = default_render_size();
        params.insert("width".into(), w.into());
        params.insert("height".into(), h.into());
    }
    // Read / Write halves: a slot no Read or Write of the family uses.
    if super::pairs::is_pair_type(def_type) {
        let slot = super::pairs::fresh_slot(patch, def_type, &HashSet::new());
        params.insert("slot".into(), slot.into());
    }
    let (width, height) = if spec.region_kind.is_some() {
        (
            params.get("width").and_then(|v| v.as_f64()),
            params.get("height").and_then(|v| v.as_f64()),
        )
    } else {
        (None, None)
    };
    patch.nodes.push(PatchNode {
        id: id.clone(),
        def_type: def_type.to_owned(),
        position: pos,
        params,
        parent_id: None,
        port_values: None,
        ports: if spec.region_kind.is_some() {
            Some(Vec::new())
        } else {
            None
        },
        width,
        height,
        bypassed: false,
    });
    Some(id)
}

/// Every node inside `id` (children, grandchildren, …).
pub fn descendants(patch: &PatchFile, id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![id.to_owned()];
    while let Some(cur) = stack.pop() {
        for n in &patch.nodes {
            if n.parent_id.as_deref() == Some(cur.as_str()) && !out.contains(&n.id) {
                out.push(n.id.clone());
                stack.push(n.id.clone());
            }
        }
    }
    out
}

/// Remove nodes (and everything inside removed regions) plus their wires.
/// Returns how many nodes went.
pub fn delete_nodes(patch: &mut PatchFile, ids: &[String]) -> usize {
    let mut gone: HashSet<String> = ids.iter().cloned().collect();
    for id in ids {
        gone.extend(descendants(patch, id));
    }
    let before = patch.nodes.len();
    patch.nodes.retain(|n| !gone.contains(&n.id));
    patch
        .edges
        .retain(|e| !gone.contains(&e.source) && !gone.contains(&e.target));
    before - patch.nodes.len()
}

/// `pruneOrphanEdges`: drop wires whose endpoint node vanished.
pub fn prune_orphan_edges(patch: &mut PatchFile) -> bool {
    let ids: HashSet<&str> = patch.nodes.iter().map(|n| n.id.as_str()).collect();
    let keep: Vec<PatchEdge> = patch
        .edges
        .iter()
        .filter(|e| ids.contains(e.source.as_str()) && ids.contains(e.target.as_str()))
        .cloned()
        .collect();
    let changed = keep.len() != patch.edges.len();
    patch.edges = keep;
    changed
}

/// `duplicateSelection`: copy the nodes (regions bring their contents) and
/// the wires between them, offset by `(dx, dy)`. Returns the new ids in
/// the same order as `ids` (descendants appended).
pub fn duplicate(patch: &mut PatchFile, ids: &[String], dx: f64, dy: f64) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for id in ids {
        if find_node(patch, id).is_some() && !all.contains(id) {
            all.push(id.clone());
        }
        for d in descendants(patch, id) {
            if !all.contains(&d) {
                all.push(d);
            }
        }
    }
    let types: Vec<String> = all
        .iter()
        .filter_map(|id| find_node(patch, id).map(|n| n.def_type.clone()))
        .collect();
    let new_ids = fresh_node_ids(patch, types.iter().map(String::as_str));
    let map: HashMap<String, String> = all.iter().cloned().zip(new_ids.iter().cloned()).collect();
    let mut nodes = Vec::new();
    for id in &all {
        let Some(n) = find_node(patch, id) else {
            continue;
        };
        let mut copy = n.clone();
        copy.id = map[id].clone();
        match n.parent_id.as_ref().and_then(|p| map.get(p)) {
            // Inside a copied region: keep the relative position.
            Some(new_parent) => copy.parent_id = Some(new_parent.clone()),
            None => {
                copy.position.x += dx;
                copy.position.y += dy;
            }
        }
        nodes.push(copy);
    }
    patch.nodes.extend(nodes);
    let mut edges = Vec::new();
    for e in &patch.edges {
        if let (Some(s), Some(t)) = (map.get(&e.source), map.get(&e.target)) {
            edges.push(PatchEdge {
                id: String::new(),
                source: s.clone(),
                target: t.clone(),
                source_handle: e.source_handle.clone(),
                target_handle: e.target_handle.clone(),
            });
        }
    }
    for mut e in edges {
        e.id = fresh_edge_id(patch);
        patch.edges.push(e);
    }
    new_ids
}

/// A region's frame size: the node's own size, else its params, else the
/// ForEach default.
pub fn region_size(n: &PatchNode) -> (f64, f64) {
    let param = |k: &str| n.params.get(k).and_then(|v| v.as_f64());
    (
        n.width.or_else(|| param("width")).unwrap_or(480.0),
        n.height.or_else(|| param("height")).unwrap_or(280.0),
    )
}

/// `findRegionAtFlow`: the smallest ForEach / If / Cache region sitting
/// directly in `scope` that contains `p` (absolute canvas coordinates).
pub fn region_at(
    patch: &PatchFile,
    cat: &Catalogue,
    p: Position,
    exclude: &str,
    scope: Option<&str>,
) -> Option<String> {
    patch
        .nodes
        .iter()
        .filter(|n| n.id != exclude && n.parent_id.as_deref() == scope)
        .filter(|n| {
            cat.get(&n.def_type)
                .is_some_and(|s| super::regions::is_drawable(s.region_kind))
        })
        .filter_map(|n| {
            let (w, h) = region_size(n);
            let at = absolute_pos(patch, &n.id);
            let inside = p.x >= at.x && p.x <= at.x + w && p.y >= at.y && p.y <= at.y + h;
            inside.then_some((w * h, n))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, n)| n.id.clone())
}

/// Add a node at an absolute canvas point inside `scope` (the entered
/// subpatch, or the root), auto-parenting it into the region the point
/// falls in (`placeNodeAtFlow`).
pub fn place_node(
    patch: &mut PatchFile,
    cat: &Catalogue,
    def_type: &str,
    at: Position,
    scope: Option<&str>,
) -> Option<String> {
    let is_region = super::search::entry(cat, def_type)?
        .spec
        .region_kind
        .is_some();
    let parent = if is_region {
        scope.map(str::to_owned)
    } else {
        region_at(patch, cat, at, "", scope).or_else(|| scope.map(str::to_owned))
    };
    let base = parent
        .as_deref()
        .map(|p| absolute_pos(patch, p))
        .unwrap_or_default();
    let local = Position {
        x: at.x - base.x,
        y: at.y - base.y,
    };
    let id = add_node(patch, cat, def_type, local)?;
    super::ports::find_node_mut(patch, &id)
        .expect("just added")
        .parent_id = parent;
    // A Write comes with its Read and a Read with its Write
    // (`pairs::partners` finds it).
    if super::pairs::is_pair_type(def_type) {
        super::pairs::add_partner(patch, cat, &id);
    }
    Some(id)
}

/// Canvas-space position, walking up the parent chain (children are
/// stored relative to their region).
pub fn absolute_pos(patch: &PatchFile, id: &str) -> Position {
    let mut p = Position::default();
    let mut cur = find_node(patch, id);
    let mut guard = 0;
    while let Some(n) = cur {
        p.x += n.position.x;
        p.y += n.position.y;
        guard += 1;
        if guard > 64 {
            break;
        }
        cur = n.parent_id.as_deref().and_then(|pid| find_node(patch, pid));
    }
    p
}

/// Bypass `ids` (the ones that can be: not regions, comments, probes,
/// State nodes or the Renderer), or turn them all back on when every one
/// already is. Returns whether anything changed.
pub fn toggle_bypass(patch: &mut PatchFile, cat: &Catalogue, ids: &[String]) -> bool {
    let can = |n: &PatchNode| {
        ids.contains(&n.id)
            && cat
                .get(&n.def_type)
                .is_some_and(floret_engine::bypass::can_bypass)
    };
    let targets: Vec<usize> = (0..patch.nodes.len())
        .filter(|&i| can(&patch.nodes[i]))
        .collect();
    if targets.is_empty() {
        return false;
    }
    let on = !targets.iter().all(|&i| patch.nodes[i].bypassed);
    for i in targets {
        patch.nodes[i].bypassed = on;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};

    #[test]
    fn bypass_toggles_bypassable_nodes_as_one_undo_step() {
        let cat = Catalogue::builtin();
        let mut p = patch(
            vec![
                node("fx", "gpu/effect"),
                node("m", "math/math"),
                node("r", "region/foreach"),
                node("out", "render/renderer"),
            ],
            vec![],
        );
        let mut history = crate::editor::history::History::default();
        history.reset(&p);
        // Regions and the Renderer can't be bypassed.
        assert!(!toggle_bypass(&mut p, cat, &["r".into(), "out".into()]));
        assert!(toggle_bypass(&mut p, cat, &["fx".into(), "r".into()]));
        assert!(p.nodes[0].bypassed && !p.nodes[2].bypassed);
        history.push(&p, None);
        // A mixed selection bypasses all; all bypassed turns them back on.
        assert!(toggle_bypass(&mut p, cat, &["fx".into(), "m".into()]));
        assert!(p.nodes[0].bypassed && p.nodes[1].bypassed);
        assert!(toggle_bypass(&mut p, cat, &["fx".into(), "m".into()]));
        assert!(!p.nodes[0].bypassed && !p.nodes[1].bypassed);
        // Undo restores the flag.
        let mut p2 = p.clone();
        assert!(history.undo(&mut p2));
        assert!(!p2.nodes[0].bypassed);
        assert!(history.redo(&mut p2));
        assert!(p2.nodes[0].bypassed);
        // Saved only while on, and read back.
        let text = p2.to_json();
        assert_eq!(text.matches("\"bypassed\"").count(), 1);
        let back = floret_engine::load_patch(&text).unwrap();
        assert!(back.nodes[0].bypassed && !back.nodes[1].bypassed);
    }

    #[test]
    fn fresh_ids_are_unique() {
        let p = patch(vec![node("math_add_1", "math/add")], vec![]);
        assert_eq!(fresh_node_id(&p, "math/add"), "math_add_2");
        let ids = fresh_node_ids(&p, ["math/add", "math/add", "time/seconds"]);
        assert_eq!(ids, ["math_add_2", "math_add_3", "time_seconds_1"]);
    }

    #[test]
    fn delete_takes_wires_and_region_contents() {
        let mut inner = node("c", "math/add");
        inner.parent_id = Some("r".into());
        let mut p = patch(
            vec![node("a", "math/add"), node("r", "region/foreach"), inner],
            vec![edge("a", "out-result", "c", "in-a")],
        );
        assert_eq!(delete_nodes(&mut p, &["r".into()]), 2);
        assert_eq!(p.nodes.len(), 1);
        assert!(p.edges.is_empty());
    }

    #[test]
    fn duplicate_remaps_internal_wires_only() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("x", "math/add"),
            ],
            vec![
                edge("a", "out-result", "b", "in-a"),
                edge("x", "out-result", "b", "in-b"),
            ],
        );
        let new = duplicate(&mut p, &["a".into(), "b".into()], 24.0, 24.0);
        assert_eq!(new.len(), 2);
        assert_eq!(p.nodes.len(), 5);
        let copied: Vec<_> = p.edges.iter().filter(|e| new.contains(&e.target)).collect();
        assert_eq!(copied.len(), 1, "only the a→b wire is internal");
        assert_eq!(copied[0].source, new[0]);
        assert_eq!(find_node(&p, &new[0]).unwrap().position.x, 24.0);
        let ids: HashSet<_> = p.edges.iter().map(|e| e.id.clone()).collect();
        assert_eq!(ids.len(), p.edges.len(), "edge ids unique");
    }

    #[test]
    fn placement_parents_into_regions_and_scope() {
        let cat = Catalogue::builtin();
        let mut r = node("r", "region/foreach");
        r.position = Position { x: 100.0, y: 100.0 };
        r.width = Some(300.0);
        r.height = Some(200.0);
        let mut sub = node("sub", "meta/subpatch");
        sub.position = Position { x: 1000.0, y: 0.0 };
        let mut p = patch(vec![r, sub], vec![]);
        let inside = place_node(
            &mut p,
            cat,
            "math/add",
            Position { x: 150.0, y: 120.0 },
            None,
        )
        .unwrap();
        let n = find_node(&p, &inside).unwrap();
        assert_eq!(n.parent_id.as_deref(), Some("r"));
        assert_eq!(n.position, Position { x: 50.0, y: 20.0 });
        // A collapsed subpatch is not a drop target.
        let beside = place_node(
            &mut p,
            cat,
            "math/add",
            Position { x: 1010.0, y: 10.0 },
            None,
        )
        .unwrap();
        assert!(find_node(&p, &beside).unwrap().parent_id.is_none());
        // Inside an entered subpatch, new nodes belong to it.
        let deep = place_node(
            &mut p,
            cat,
            "math/add",
            Position { x: 1010.0, y: 10.0 },
            Some("sub"),
        )
        .unwrap();
        let n = find_node(&p, &deep).unwrap();
        assert_eq!(n.parent_id.as_deref(), Some("sub"));
        assert_eq!(absolute_pos(&p, &deep), Position { x: 1010.0, y: 10.0 });
    }

    #[test]
    fn a_search_key_places_a_node_preset_to_its_op() {
        let cat = Catalogue::builtin();
        let mut p = patch(vec![], vec![]);
        let id = add_node(&mut p, cat, "math/math#sin", Position::default()).unwrap();
        let n = find_node(&p, &id).unwrap();
        assert_eq!(n.def_type, "math/math");
        assert_eq!(n.params["op"], "sin");
        assert_eq!(n.params["slotCount"], 2);
        assert!(id.starts_with("math_math_"));
        let plain = add_node(&mut p, cat, "math/math", Position::default()).unwrap();
        assert_eq!(find_node(&p, &plain).unwrap().params["op"], "add");
        assert!(add_node(&mut p, cat, "math/math#nope", Position::default()).is_none());
    }

    #[test]
    fn snapping() {
        assert_eq!(snap_to(13.0, GRID), 24.0);
        assert_eq!(snap_to(-13.0, GRID), -24.0);
        assert_eq!(snap_to(11.0, GRID), 0.0);
        assert_eq!(snap_to(13.0, 8.0), 16.0);
        let o = Position { x: 5.0, y: -3.0 };
        assert_eq!(
            snap_point(Position { x: 20.0, y: 10.0 }, 16.0, o),
            Position { x: 21.0, y: 13.0 }
        );
    }
}
