//! Subpatches: grouping a selection into one (`subpatch-grouping.ts`) and
//! the view stack (`view-slice.ts` / `use-projected-graph.ts`) — which
//! nodes are visible at each level.

use super::ops::{fresh_edge_id, fresh_node_id};
use super::ports::{find_node, resolve_port, split_handle, type_name, Side};
use floret_engine::patch::{PatchEdge, PatchFile, PatchNode, PortDirection, Position, RegionPort};
use floret_engine::spec::{Catalogue, RegionKind};
use std::collections::HashSet;

pub const SUBPATCH: &str = "meta/subpatch";

pub fn is_subpatch(cat: &Catalogue, n: &PatchNode) -> bool {
    cat.get(&n.def_type).and_then(|s| s.region_kind) == Some(RegionKind::Subpatch)
}

pub fn is_subpatch_id(patch: &PatchFile, cat: &Catalogue, id: &str) -> bool {
    find_node(patch, id).is_some_and(|n| is_subpatch(cat, n))
}

/// The closest subpatch strictly above `id`.
pub fn nearest_subpatch(patch: &PatchFile, cat: &Catalogue, id: &str) -> Option<String> {
    let mut seen = HashSet::new();
    let mut cur = find_node(patch, id)?.parent_id.clone();
    while let Some(p) = cur {
        if !seen.insert(p.clone()) {
            return None;
        }
        let n = find_node(patch, &p)?;
        if is_subpatch(cat, n) {
            return Some(p);
        }
        cur = n.parent_id.clone();
    }
    None
}

/// Nodes shown at this level: the entered subpatch (as an expanded frame)
/// plus everything whose nearest subpatch is it; at the root, everything
/// not inside a subpatch.
pub fn visible(patch: &PatchFile, cat: &Catalogue, stack: &[String]) -> HashSet<String> {
    let scope = stack.last();
    patch
        .nodes
        .iter()
        .filter(|n| Some(&n.id) == scope || nearest_subpatch(patch, cat, &n.id).as_ref() == scope)
        .map(|n| n.id.clone())
        .collect()
}

/// Where the entered subpatch's own coordinates start. The web app shows
/// an entered nested subpatch at the origin, so its children's positions
/// are what the grid (and snapping) line up with; at the root, and for a
/// top-level subpatch, that is the canvas origin.
pub fn view_origin(patch: &PatchFile, stack: &[String]) -> Position {
    match stack.last().and_then(|id| find_node(patch, id)) {
        Some(n) if n.parent_id.is_some() => super::ops::absolute_pos(patch, &n.id),
        _ => Position::default(),
    }
}

/// `setViewStack` validation: keep the prefix that is still a chain of
/// nested subpatches.
pub fn valid_stack(patch: &PatchFile, cat: &Catalogue, stack: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in stack {
        let Some(n) = find_node(patch, id).filter(|n| is_subpatch(cat, n)) else {
            break;
        };
        if nearest_subpatch(patch, cat, &n.id).as_ref() != out.last() {
            break;
        }
        out.push(id.clone());
    }
    out
}

/// The node standing in for `id` at this level: itself when visible,
/// else the collapsed subpatch (or region) that contains it.
pub fn visible_stand_in(patch: &PatchFile, id: &str, visible: &HashSet<String>) -> Option<String> {
    let mut cur = Some(id.to_owned());
    let mut guard = 0;
    while let Some(c) = cur {
        if visible.contains(&c) {
            return Some(c);
        }
        guard += 1;
        if guard > 64 {
            return None;
        }
        cur = find_node(patch, &c).and_then(|n| n.parent_id.clone());
    }
    None
}

/// `buildSubpatchGrouping`: wrap sibling nodes in a new `meta/subpatch`.
/// Wires crossing the selection become frame ports (outer wire to the
/// outer pin, inner wire to the inner pin). The subpatch sits at the
/// selection's centroid and the children keep their offsets from it, as in
/// the web app. Refuses an empty selection, mixed parents, or drawable
/// regions. Returns the subpatch id.
pub fn group(patch: &mut PatchFile, cat: &Catalogue, ids: &[String]) -> Option<String> {
    let sel: Vec<PatchNode> = ids
        .iter()
        .filter_map(|id| find_node(patch, id).cloned())
        .collect();
    let first = sel.first()?;
    let parent = first.parent_id.clone();
    if sel.iter().any(|n| n.parent_id != parent) {
        return None;
    }
    if sel.iter().any(|n| {
        cat.get(&n.def_type)
            .and_then(|s| s.region_kind)
            .is_some_and(|k| k != RegionKind::Subpatch)
    }) {
        return None;
    }
    let inside: HashSet<&str> = sel.iter().map(|n| n.id.as_str()).collect();
    let k = sel.len() as f64;
    let centroid = Position {
        x: sel.iter().map(|n| n.position.x).sum::<f64>() / k,
        y: sel.iter().map(|n| n.position.y).sum::<f64>() / k,
    };

    let sub_id = fresh_node_id(patch, SUBPATCH);
    let mut ports: Vec<RegionPort> = Vec::new();
    // (port id, outer wire, inner wire)
    let mut rewires: Vec<(String, PatchEdge, PatchEdge)> = Vec::new();
    let mut dropped: HashSet<String> = HashSet::new();
    let (mut n_in, mut n_out) = (0, 0);
    for e in &patch.edges {
        let (src_in, dst_in) = (
            inside.contains(e.source.as_str()),
            inside.contains(e.target.as_str()),
        );
        if src_in == dst_in {
            continue;
        }
        let (Some((_, sname)), Some((_, tname))) = (
            split_handle(e.source_handle.as_deref()),
            split_handle(e.target_handle.as_deref()),
        ) else {
            continue;
        };
        let (direction, ty) = if dst_in {
            let ty = resolve_port(patch, cat, &e.target, Side::In, tname).map(|p| p.ty);
            (PortDirection::Input, ty)
        } else {
            let ty = resolve_port(patch, cat, &e.source, Side::Out, sname).map(|p| p.ty);
            (PortDirection::Output, ty)
        };
        let Some(ty) = ty else {
            continue;
        };
        let pid = format!("p_subport_{}_{sub_id}", ports.len() + 1);
        let name = if dst_in {
            n_in += 1;
            format!("in{n_in}")
        } else {
            n_out += 1;
            format!("out{n_out}")
        };
        ports.push(RegionPort {
            id: pid.clone(),
            name,
            ty: type_name(ty).into(),
            direction,
            y: None,
        });
        let wire = |s: &str, sh: &str, t: &str, th: &str| PatchEdge {
            id: String::new(),
            source: s.into(),
            target: t.into(),
            source_handle: Some(sh.into()),
            target_handle: Some(th.into()),
        };
        let (sh, th) = (
            e.source_handle.clone().unwrap_or_default(),
            e.target_handle.clone().unwrap_or_default(),
        );
        let (inp, outp) = (Side::In.handle(&pid), Side::Out.handle(&pid));
        // Input port: outer `in-` ← source, inner `out-` → target.
        // Output port: inner `in-` ← source, outer `out-` → target.
        let (a, b) = (
            wire(&e.source, &sh, &sub_id, &inp),
            wire(&sub_id, &outp, &e.target, &th),
        );
        rewires.push((pid, a, b));
        dropped.insert(e.id.clone());
    }

    let spec = cat.get(SUBPATCH)?;
    let mut params = spec.default_params.clone();
    params
        .entry("label".to_owned())
        .or_insert_with(|| "Subpatch".into());
    let sub = PatchNode {
        id: sub_id.clone(),
        def_type: SUBPATCH.into(),
        position: centroid,
        params,
        parent_id: parent,
        port_values: None,
        ports: Some(ports),
        width: None,
        height: None,
        bypassed: false,
        extra: Default::default(),
    };
    for n in patch
        .nodes
        .iter_mut()
        .filter(|n| inside.contains(n.id.as_str()))
    {
        n.parent_id = Some(sub_id.clone());
        n.position = Position {
            x: n.position.x - centroid.x,
            y: n.position.y - centroid.y,
        };
    }
    patch.nodes.push(sub);
    patch.edges.retain(|e| !dropped.contains(&e.id));
    for (_, a, b) in rewires {
        for mut e in [a, b] {
            e.id = fresh_edge_id(patch);
            patch.edges.push(e);
        }
    }
    Some(sub_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};

    fn at(mut n: PatchNode, x: f64, y: f64) -> PatchNode {
        n.position = Position { x, y };
        n
    }

    #[test]
    fn grouping_turns_crossing_wires_into_ports() {
        let cat = Catalogue::builtin();
        let mut p = patch(
            vec![
                at(node("t", "time/seconds"), 0.0, 0.0),
                at(node("a", "math/sin"), 200.0, 0.0),
                at(node("b", "math/cos"), 400.0, 100.0),
                at(node("out", "vec2/make"), 600.0, 0.0),
            ],
            vec![
                edge("t", "out-t", "a", "in-x"),
                edge("a", "out-result", "b", "in-x"),
                edge("b", "out-result", "out", "in-x"),
            ],
        );
        let sub = group(&mut p, cat, &["a".into(), "b".into()]).unwrap();
        let s = find_node(&p, &sub).unwrap();
        assert_eq!(
            s.position,
            Position { x: 300.0, y: 50.0 },
            "at the centroid"
        );
        let ports = s.ports.as_ref().unwrap();
        assert_eq!(ports.len(), 2);
        assert_eq!(
            (ports[0].name.as_str(), ports[0].direction),
            ("in1", PortDirection::Input)
        );
        assert_eq!(
            (ports[1].name.as_str(), ports[1].direction),
            ("out1", PortDirection::Output)
        );
        assert_eq!(
            find_node(&p, "a").unwrap().parent_id.as_deref(),
            Some(sub.as_str())
        );
        // Internal wire kept; each crossing wire split in two.
        assert_eq!(p.edges.len(), 5);
        let pin = |e: &PatchEdge| (e.source.clone(), e.target.clone());
        let pairs: Vec<_> = p.edges.iter().map(pin).collect();
        assert!(pairs.contains(&("t".into(), sub.clone())));
        assert!(pairs.contains(&(sub.clone(), "a".into())));
        assert!(pairs.contains(&("b".into(), sub.clone())));
        assert!(pairs.contains(&(sub.clone(), "out".into())));
        // Children keep their offsets from the centroid, so nothing moves
        // on screen.
        assert_eq!(
            find_node(&p, "a").unwrap().position,
            Position {
                x: -100.0,
                y: -50.0
            }
        );
        assert_eq!(
            crate::editor::ops::absolute_pos(&p, "b"),
            Position { x: 400.0, y: 100.0 }
        );
        // And the grouped patch still compiles to the same wiring.
        assert!(floret_engine::Engine::from_patch(&p).is_ok());
    }

    #[test]
    fn a_nested_subpatch_view_starts_at_its_corner() {
        let mut inner = at(node("inner", SUBPATCH), 30.0, 40.0);
        inner.parent_id = Some("outer".into());
        let p = patch(
            vec![at(node("outer", SUBPATCH), 100.0, 200.0), inner],
            vec![],
        );
        assert_eq!(view_origin(&p, &["outer".into()]), Position::default());
        assert_eq!(
            view_origin(&p, &["outer".into(), "inner".into()]),
            Position { x: 130.0, y: 240.0 }
        );
    }

    #[test]
    fn grouping_refuses_mixed_parents_and_regions() {
        let cat = Catalogue::builtin();
        let mut child = node("c", "math/add");
        child.parent_id = Some("r".into());
        let mut p = patch(
            vec![node("a", "math/add"), node("r", "region/foreach"), child],
            vec![],
        );
        assert!(group(&mut p, cat, &["a".into(), "c".into()]).is_none());
        assert!(group(&mut p, cat, &["r".into()]).is_none());
        assert!(group(&mut p, cat, &[]).is_none());
    }

    #[test]
    fn view_stack_shows_one_level() {
        let cat = Catalogue::builtin();
        let mut inner = node("inner", SUBPATCH);
        inner.parent_id = Some("outer".into());
        let mut a = node("a", "math/add");
        a.parent_id = Some("outer".into());
        let mut deep = node("deep", "math/add");
        deep.parent_id = Some("inner".into());
        let mut r = node("r", "region/foreach");
        r.parent_id = Some("outer".into());
        let mut in_r = node("in_r", "math/add");
        in_r.parent_id = Some("r".into());
        let p = patch(
            vec![
                node("root", "math/add"),
                node("outer", SUBPATCH),
                inner,
                a,
                deep,
                r,
                in_r,
            ],
            vec![],
        );
        let set = |s: &[&str]| -> HashSet<String> { s.iter().map(|x| x.to_string()).collect() };
        assert_eq!(visible(&p, cat, &[]), set(&["root", "outer"]));
        assert_eq!(
            visible(&p, cat, &["outer".into()]),
            set(&["outer", "inner", "a", "r", "in_r"])
        );
        assert_eq!(
            visible(&p, cat, &["outer".into(), "inner".into()]),
            set(&["inner", "deep"])
        );
        assert_eq!(
            valid_stack(&p, cat, &["outer".into(), "inner".into()]),
            ["outer", "inner"]
        );
        assert_eq!(
            valid_stack(&p, cat, &["inner".into()]),
            Vec::<String>::new()
        );
        assert_eq!(
            valid_stack(&p, cat, &["outer".into(), "a".into()]),
            ["outer"]
        );
        assert_eq!(view_origin(&p, &[]), Position::default());
        let root = visible(&p, cat, &[]);
        assert_eq!(
            visible_stand_in(&p, "deep", &root).as_deref(),
            Some("outer")
        );
    }
}
