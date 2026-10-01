//! Structural edits on a `PatchFile`: ids, add / delete / duplicate nodes,
//! grid snapping. Plain functions so they test without a window.

use super::ports::find_node;
use floret_engine::patch::{PatchEdge, PatchFile, PatchNode, Position};
use floret_engine::spec::Catalogue;
use std::collections::{HashMap, HashSet};

/// Grid step for "snap to grid" (matches the canvas dot grid).
pub const GRID: f64 = 24.0;

pub fn snap(v: f64) -> f64 {
    (v / GRID).round() * GRID
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
/// (canvas coordinates, root scope). Returns its id.
pub fn add_node(
    patch: &mut PatchFile,
    cat: &Catalogue,
    def_type: &str,
    pos: Position,
) -> Option<String> {
    let spec = cat.get(def_type)?;
    let id = fresh_node_id(patch, def_type);
    let mut params = spec.default_params.clone();
    // Read / Write slots: the smallest integer unused by this type.
    if matches!(
        def_type,
        "state/read" | "state/write" | "state/read-spread" | "state/write-spread"
    ) {
        let used: HashSet<i64> = patch
            .nodes
            .iter()
            .filter(|n| n.def_type == def_type)
            .filter_map(|n| n.params.get("slot").and_then(|v| v.as_f64()))
            .map(|v| v as i64)
            .collect();
        let slot = (1..).find(|s| !used.contains(s)).unwrap_or(1);
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
    let is_region = cat.get(def_type)?.region_kind.is_some();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};

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
    fn snapping() {
        assert_eq!(snap(13.0), 24.0);
        assert_eq!(snap(-13.0), -24.0);
        assert_eq!(snap(11.0), 0.0);
    }
}
