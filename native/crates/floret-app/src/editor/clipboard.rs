//! Copy / paste. The clipboard payload is a `.flora` patch holding the
//! copied nodes and the wires between them, so it round-trips through the
//! system clipboard as plain JSON (and any `.flora` text pastes too).
//! Port of `src/editor/clipboard.ts` (which kept an in-memory buffer).

use super::ops::{absolute_pos, descendants, empty_like, fresh_edge_id, fresh_node_ids};
use floret_engine::patch::{PatchEdge, PatchFile, Position};
use std::collections::{HashMap, HashSet};

/// The selected nodes (regions bring their contents) and every wire with
/// both ends inside, as a standalone patch. Top-level copied nodes carry
/// absolute positions. `None` when nothing is selected.
pub fn copy(patch: &PatchFile, ids: &[String]) -> Option<PatchFile> {
    let mut set: HashSet<String> = HashSet::new();
    for id in ids {
        if patch.nodes.iter().any(|n| &n.id == id) {
            set.insert(id.clone());
            set.extend(descendants(patch, id));
        }
    }
    if set.is_empty() {
        return None;
    }
    let mut out = empty_like(patch);
    for n in patch.nodes.iter().filter(|n| set.contains(&n.id)) {
        let mut c = n.clone();
        if !c.parent_id.as_ref().is_some_and(|p| set.contains(p)) {
            c.parent_id = None;
            c.position = absolute_pos(patch, &n.id);
        }
        out.nodes.push(c);
    }
    out.edges = patch
        .edges
        .iter()
        .filter(|e| set.contains(&e.source) && set.contains(&e.target))
        .cloned()
        .collect();
    Some(out)
}

/// Parse clipboard text as a `.flora` patch (version migrations and
/// legacy fix-ups included). Non-patch text yields `None`.
pub fn parse(text: &str) -> Option<PatchFile> {
    let t = text.trim_start();
    if !t.starts_with('{') {
        return None;
    }
    floret_engine::load_patch(t).ok()
}

/// Materialise `clip` into `patch` with fresh ids, its top-level nodes
/// centred on `anchor` (layout preserved). Returns the new top-level ids
/// followed by their contents.
pub fn paste(patch: &mut PatchFile, clip: &PatchFile, anchor: Position) -> Vec<String> {
    if clip.nodes.is_empty() {
        return Vec::new();
    }
    let in_clip: HashSet<&str> = clip.nodes.iter().map(|n| n.id.as_str()).collect();
    let is_top = |n: &floret_engine::patch::PatchNode| {
        !n.parent_id
            .as_deref()
            .is_some_and(|p| in_clip.contains(p))
    };
    let tops: Vec<_> = clip.nodes.iter().filter(|n| is_top(n)).collect();
    let k = tops.len().max(1) as f64;
    let cx = tops.iter().map(|n| n.position.x).sum::<f64>() / k;
    let cy = tops.iter().map(|n| n.position.y).sum::<f64>() / k;

    let new_ids = fresh_node_ids(patch, clip.nodes.iter().map(|n| n.def_type.as_str()));
    let map: HashMap<&str, String> = clip
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .zip(new_ids.iter().cloned())
        .collect();
    for n in &clip.nodes {
        let mut c = n.clone();
        c.id = map[n.id.as_str()].clone();
        if is_top(n) {
            c.parent_id = None;
            c.position = Position {
                x: anchor.x + n.position.x - cx,
                y: anchor.y + n.position.y - cy,
            };
        } else {
            c.parent_id = n.parent_id.as_deref().map(|p| map[p].clone());
        }
        patch.nodes.push(c);
    }
    for e in &clip.edges {
        let (Some(s), Some(t)) = (map.get(e.source.as_str()), map.get(e.target.as_str())) else {
            continue;
        };
        let id = fresh_edge_id(patch);
        patch.edges.push(PatchEdge {
            id,
            source: s.clone(),
            target: t.clone(),
            source_handle: e.source_handle.clone(),
            target_handle: e.target_handle.clone(),
        });
    }
    let mut ordered: Vec<String> = clip
        .nodes
        .iter()
        .filter(|n| is_top(n))
        .map(|n| map[n.id.as_str()].clone())
        .collect();
    ordered.extend(
        clip.nodes
            .iter()
            .filter(|n| !is_top(n))
            .map(|n| map[n.id.as_str()].clone()),
    );
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};

    fn at(p: &PatchFile, id: &str) -> Position {
        p.nodes.iter().find(|n| n.id == id).unwrap().position
    }

    #[test]
    fn paste_remaps_ids_and_keeps_internal_wires() {
        let mut a = node("a", "math/add");
        a.position = Position { x: 0.0, y: 0.0 };
        let mut b = node("b", "math/add");
        b.position = Position { x: 200.0, y: 100.0 };
        let mut p = patch(
            vec![a, b, node("x", "time/seconds")],
            vec![
                edge("a", "out-result", "b", "in-a"),
                edge("x", "out-t", "b", "in-b"),
            ],
        );
        let clip = copy(&p, &["a".into(), "b".into()]).unwrap();
        assert_eq!(clip.edges.len(), 1, "external wire not copied");

        let ids = paste(&mut p, &clip, Position { x: 1000.0, y: 1000.0 });
        assert_eq!(ids, ["math_add_1", "math_add_2"]);
        assert_eq!(p.nodes.len(), 5);
        // Centroid of (0,0) and (200,100) lands on the anchor.
        assert_eq!(at(&p, "math_add_1"), Position { x: 900.0, y: 950.0 });
        assert_eq!(at(&p, "math_add_2"), Position { x: 1100.0, y: 1050.0 });
        let pasted: Vec<_> = p.edges.iter().filter(|e| ids.contains(&e.source)).collect();
        assert_eq!(pasted.len(), 1);
        assert_eq!(pasted[0].source, "math_add_1");
        assert_eq!(pasted[0].target, "math_add_2");
        assert_eq!(pasted[0].target_handle.as_deref(), Some("in-a"));
        // Pasting twice never collides.
        let again = paste(&mut p, &clip, Position { x: 0.0, y: 0.0 });
        assert_eq!(again, ["math_add_3", "math_add_4"]);
        let edge_ids: HashSet<_> = p.edges.iter().map(|e| &e.id).collect();
        assert_eq!(edge_ids.len(), p.edges.len());
    }

    #[test]
    fn round_trips_through_flora_json() {
        let mut p = patch(
            vec![node("a", "math/add"), node("b", "math/add")],
            vec![edge("a", "out-result", "b", "in-a")],
        );
        let clip = copy(&p, &["a".into(), "b".into()]).unwrap();
        let text = clip.to_json();
        let back = parse(&text).expect("valid .flora");
        assert_eq!(back.nodes.len(), 2);
        assert!(parse("hello").is_none());
        assert!(parse("{\"kind\":\"something-else\"}").is_none());
        paste(&mut p, &back, Position::default());
        assert_eq!(p.nodes.len(), 4);
        assert_eq!(p.edges.len(), 2);
    }

    #[test]
    fn region_contents_stay_relative() {
        let mut r = node("r", "region/foreach");
        r.position = Position { x: 100.0, y: 100.0 };
        let mut c = node("c", "math/add");
        c.parent_id = Some("r".into());
        c.position = Position { x: 10.0, y: 20.0 };
        let mut p = patch(vec![r, c], vec![]);
        // Copying only the child lifts it to an absolute position.
        let only_child = copy(&p, &["c".into()]).unwrap();
        assert_eq!(only_child.nodes[0].parent_id, None);
        assert_eq!(only_child.nodes[0].position, Position { x: 110.0, y: 120.0 });
        // Copying the region brings the child along, still relative.
        let clip = copy(&p, &["r".into()]).unwrap();
        let ids = paste(&mut p, &clip, Position { x: 0.0, y: 0.0 });
        assert_eq!(ids.len(), 2);
        let child = p.nodes.iter().find(|n| n.id == ids[1]).unwrap();
        assert_eq!(child.parent_id.as_deref(), Some(ids[0].as_str()));
        assert_eq!(child.position, Position { x: 10.0, y: 20.0 });
    }
}
