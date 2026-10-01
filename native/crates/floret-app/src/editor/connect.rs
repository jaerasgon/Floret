//! Wiring rules and wire edits. Port of `src/editor/connection-rules.ts`
//! plus the store's `onConnect` / `addEdge` / `removeEdge` /
//! `reorderMultiEdge`.

use super::ops::fresh_edge_id;
use super::ports::{find_node, resolve_port, split_handle, Side};
use floret_engine::patch::{PatchEdge, PatchFile};
use floret_engine::spec::{Catalogue, PortKind, PortType};
use std::collections::{HashMap, HashSet};

/// Strict port types with narrow coercions: number ↔ boolean, and `any`
/// matches everything.
pub fn are_ports_compatible(a: PortType, b: PortType) -> bool {
    a == b
        || a == PortType::Any
        || b == PortType::Any
        || matches!(
            (a, b),
            (PortType::Number, PortType::Boolean) | (PortType::Boolean, PortType::Number)
        )
}

/// Source kind → target kind: single→spread wraps, anything→multi gathers,
/// spread→single is refused (it must cross a region boundary).
pub fn are_kinds_compatible(from: PortKind, to: PortKind) -> bool {
    from == to
        || (from == PortKind::Single && to == PortKind::Spread)
        || (to == PortKind::Multi && matches!(from, PortKind::Single | PortKind::Spread))
}

/// True when the input accepts many wires (a `multi` gather pin).
pub fn is_multi_target(patch: &PatchFile, cat: &Catalogue, node: &str, handle: &str) -> bool {
    match split_handle(Some(handle)) {
        Some((Side::In, name)) => resolve_port(patch, cat, node, Side::In, name)
            .is_some_and(|p| p.kind == PortKind::Multi),
        _ => false,
    }
}

/// Edges into a stateful node's `next` input are cross-frame feedback, not
/// part of the within-frame graph.
fn is_stateful_next(patch: &PatchFile, cat: &Catalogue, node: &str, handle: Option<&str>) -> bool {
    let stateful = find_node(patch, node)
        .and_then(|n| cat.get(&n.def_type))
        .is_some_and(|s| s.stateful);
    stateful && matches!(split_handle(handle), Some((Side::In, "next")))
}

/// `wouldCreateCycle`: would `source → target` close a directed loop over
/// `edges`? Region frame ports are virtualised per port (each one becomes
/// its own splicer at compile time), and stateful `next` edges are exempt.
pub fn would_create_cycle(
    patch: &PatchFile,
    cat: &Catalogue,
    edges: &[PatchEdge],
    source: &str,
    source_handle: Option<&str>,
    target: &str,
    target_handle: Option<&str>,
) -> bool {
    if source == target {
        return false;
    }
    if is_stateful_next(patch, cat, target, target_handle) {
        return false;
    }
    let virt = |node: &str, handle: Option<&str>| -> String {
        let region = find_node(patch, node)
            .and_then(|n| cat.get(&n.def_type))
            .is_some_and(|s| s.region_kind.is_some());
        match (region, split_handle(handle)) {
            (true, Some((_, name))) => format!("{node}#{name}"),
            _ => node.to_owned(),
        }
    };
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for e in edges {
        if is_stateful_next(patch, cat, &e.target, e.target_handle.as_deref()) {
            continue;
        }
        adj.entry(virt(&e.source, e.source_handle.as_deref()))
            .or_default()
            .push(virt(&e.target, e.target_handle.as_deref()));
    }
    let goal = virt(source, source_handle);
    let mut seen = HashSet::new();
    let mut stack = vec![virt(target, target_handle)];
    while let Some(cur) = stack.pop() {
        if cur == goal {
            return true;
        }
        if !seen.insert(cur.clone()) {
            continue;
        }
        if let Some(next) = adj.get(&cur) {
            stack.extend(next.iter().cloned());
        }
    }
    false
}

/// A proposed wire, output handle → input handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conn {
    pub source: String,
    pub source_handle: String,
    pub target: String,
    pub target_handle: String,
}

impl Conn {
    pub fn new(source: &str, source_handle: &str, target: &str, target_handle: &str) -> Conn {
        Conn {
            source: source.to_owned(),
            source_handle: source_handle.to_owned(),
            target: target.to_owned(),
            target_handle: target_handle.to_owned(),
        }
    }

    /// Build from a dragged pin and the pin it was dropped on, whichever
    /// end the drag started from.
    pub fn between(a: (&str, Side, &str), b: (&str, Side, &str)) -> Option<Conn> {
        let ((sn, _, sp), (tn, _, tp)) = match (a.1, b.1) {
            (Side::Out, Side::In) => (a, b),
            (Side::In, Side::Out) => (b, a),
            _ => return None,
        };
        Some(Conn::new(
            sn,
            &Side::Out.handle(sp),
            tn,
            &Side::In.handle(tp),
        ))
    }

    fn occupies(&self, e: &PatchEdge) -> bool {
        e.target == self.target && e.target_handle.as_deref() == Some(self.target_handle.as_str())
    }
}

/// `buildIsValidConnection`. With `replace` the editor's drop semantics
/// apply: an occupied single input is not a refusal (the old wire goes),
/// and the cycle check runs against the edges that would remain.
pub fn check_connection(patch: &PatchFile, cat: &Catalogue, c: &Conn, replace: bool) -> bool {
    let (Some((Side::Out, sname)), Some((Side::In, tname))) = (
        split_handle(Some(&c.source_handle)),
        split_handle(Some(&c.target_handle)),
    ) else {
        return false;
    };
    if c.source == c.target {
        // Self-wires only on regions, between two different frame ports.
        let region = find_node(patch, &c.source)
            .and_then(|n| cat.get(&n.def_type))
            .is_some_and(|s| s.region_kind.is_some());
        if !region || sname == tname {
            return false;
        }
    }
    let (Some(sp), Some(tp)) = (
        resolve_port(patch, cat, &c.source, Side::Out, sname),
        resolve_port(patch, cat, &c.target, Side::In, tname),
    ) else {
        return false;
    };
    if !are_ports_compatible(sp.ty, tp.ty) || !are_kinds_compatible(sp.kind, tp.kind) {
        return false;
    }
    let multi = tp.kind == PortKind::Multi;
    let edges: Vec<PatchEdge> = if multi || !replace {
        if !multi && patch.edges.iter().any(|e| c.occupies(e)) {
            return false;
        }
        patch.edges.clone()
    } else {
        patch
            .edges
            .iter()
            .filter(|e| !c.occupies(e))
            .cloned()
            .collect()
    };
    !would_create_cycle(
        patch,
        cat,
        &edges,
        &c.source,
        Some(&c.source_handle),
        &c.target,
        Some(&c.target_handle),
    )
}

/// `onConnect`: add the wire, replacing whatever fed a non-multi input.
/// Returns the new edge id, or `None` when refused or already present.
pub fn connect(patch: &mut PatchFile, cat: &Catalogue, c: &Conn) -> Option<String> {
    let exists = patch.edges.iter().any(|e| {
        e.source == c.source
            && e.source_handle.as_deref() == Some(c.source_handle.as_str())
            && c.occupies(e)
    });
    if exists || !check_connection(patch, cat, c, true) {
        return None;
    }
    if !is_multi_target(patch, cat, &c.target, &c.target_handle) {
        patch.edges.retain(|e| !c.occupies(e));
    }
    let id = fresh_edge_id(patch);
    patch.edges.push(PatchEdge {
        id: id.clone(),
        source: c.source.clone(),
        target: c.target.clone(),
        source_handle: Some(c.source_handle.clone()),
        target_handle: Some(c.target_handle.clone()),
    });
    Some(id)
}

pub fn remove_edge(patch: &mut PatchFile, edge_id: &str) -> bool {
    let before = patch.edges.len();
    patch.edges.retain(|e| e.id != edge_id);
    patch.edges.len() != before
}

/// This edge's slot among the wires sharing its target pin: `(index, count)`.
pub fn edge_slot(patch: &PatchFile, edge_id: &str) -> Option<(usize, usize)> {
    let e = patch.edges.iter().find(|e| e.id == edge_id)?;
    let siblings: Vec<&PatchEdge> = patch
        .edges
        .iter()
        .filter(|x| x.target == e.target && x.target_handle == e.target_handle)
        .collect();
    let idx = siblings.iter().position(|x| x.id == edge_id)?;
    Some((idx, siblings.len()))
}

/// `reorderMultiEdge`: move a wire within the gather order of its multi
/// pin by `delta` (clamped), keeping every other edge where it was.
pub fn reorder_multi_edge(
    patch: &mut PatchFile,
    cat: &Catalogue,
    edge_id: &str,
    delta: i64,
) -> bool {
    if delta == 0 {
        return false;
    }
    let Some(e) = patch.edges.iter().find(|e| e.id == edge_id).cloned() else {
        return false;
    };
    let Some(th) = e.target_handle.as_deref() else {
        return false;
    };
    if !is_multi_target(patch, cat, &e.target, th) {
        return false;
    }
    let slots: Vec<usize> = patch
        .edges
        .iter()
        .enumerate()
        .filter(|(_, x)| x.target == e.target && x.target_handle == e.target_handle)
        .map(|(i, _)| i)
        .collect();
    let mut subset: Vec<PatchEdge> = slots.iter().map(|&i| patch.edges[i].clone()).collect();
    let Some(from) = subset.iter().position(|x| x.id == edge_id) else {
        return false;
    };
    let to = (from as i64 + delta).clamp(0, subset.len() as i64 - 1) as usize;
    if to == from {
        return false;
    }
    let moved = subset.remove(from);
    subset.insert(to, moved);
    for (slot, edge) in slots.into_iter().zip(subset) {
        patch.edges[slot] = edge;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};
    use PortKind::*;
    use PortType::*;

    #[test]
    fn port_compatibility_matches_ts() {
        assert!(are_ports_compatible(Number, Number));
        assert!(are_ports_compatible(Number, Boolean));
        assert!(are_ports_compatible(Boolean, Number));
        assert!(are_ports_compatible(Any, Layer));
        assert!(are_ports_compatible(Color, Any));
        assert!(!are_ports_compatible(Number, Vec2));
        assert!(!are_ports_compatible(Color, Vec2));
        assert!(!are_ports_compatible(String, Number));
        assert!(!are_ports_compatible(Layer, Style));
    }

    #[test]
    fn kind_compatibility_matches_ts() {
        assert!(are_kinds_compatible(Single, Single));
        assert!(are_kinds_compatible(Spread, Spread));
        assert!(are_kinds_compatible(Single, Spread));
        assert!(are_kinds_compatible(Single, Multi));
        assert!(are_kinds_compatible(Spread, Multi));
        assert!(!are_kinds_compatible(Spread, Single));
    }

    fn cat() -> &'static Catalogue {
        Catalogue::builtin()
    }

    #[test]
    fn cycle_check_matches_ts() {
        // Ported from src/editor/__tests__/cycle-check.test.ts.
        let p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
                node("s", "state/value"),
                node("walk", "math/add"),
            ],
            vec![],
        );
        let e = |s: &str, t: &str, th: &str| edge(s, "out-result", t, th);
        let cyc = |edges: &[PatchEdge], s: &str, t: &str, th: &str| {
            would_create_cycle(&p, cat(), edges, s, Some("out-result"), t, Some(th))
        };
        assert!(!cyc(&[], "a", "b", "in-a"));
        assert!(cyc(&[e("a", "b", "in-a")], "b", "a", "in-a"));
        let chain = [e("a", "b", "in-a"), e("b", "c", "in-a")];
        assert!(cyc(&chain, "c", "a", "in-a"));
        assert!(!cyc(&chain, "a", "c", "in-b"));
        // Stateful `next` is cross-frame: feedback through it is legal…
        assert!(!cyc(&[e("s", "walk", "in-a")], "walk", "s", "in-next"));
        // …but a loop into a non-next input is not.
        assert!(cyc(&[e("s", "walk", "in-a")], "walk", "s", "in-init"));
        let via_next = [e("a", "b", "in-a"), e("b", "s", "in-next")];
        assert!(!cyc(&via_next, "s", "a", "in-b"));
    }

    #[test]
    fn connection_rules() {
        let mut p = patch(
            vec![
                node("t", "time/seconds"),
                node("add", "math/add"),
                node("mk", "vec2/make"),
                node("range", "spread/range"),
                node("num", "value/number"),
            ],
            vec![],
        );
        let c = cat();
        // number → adaptive `any` input: legal.
        let ok = Conn::new("t", "out-t", "add", "in-a");
        assert!(check_connection(&p, c, &ok, true));
        // Output → output / input → input are not wires.
        assert!(!check_connection(
            &p,
            c,
            &Conn::new("t", "in-x", "add", "in-a"),
            true
        ));
        // No self-wires on plain nodes.
        assert!(!check_connection(
            &p,
            c,
            &Conn::new("add", "out-result", "add", "in-a"),
            true
        ));
        // Type mismatch refused (vec2 → number).
        assert!(!check_connection(
            &p,
            c,
            &Conn::new("mk", "out-vector", "range", "in-count"),
            true
        ));
        // spread → single refused.
        let spread_out = Conn::new("range", "out-indices", "mk", "in-x");
        let range = resolve_port(&p, c, "range", Side::Out, "indices").unwrap();
        assert_eq!(range.kind, Spread);
        assert!(!check_connection(&p, c, &spread_out, true));
        // Occupied single input: refused by the strict TS rule, replaced
        // by the editor drop.
        connect(&mut p, c, &ok).unwrap();
        let other = Conn::new("num", "out-number", "add", "in-a");
        assert!(!check_connection(&p, c, &other, false));
        assert!(check_connection(&p, c, &other, true));
        connect(&mut p, c, &other).unwrap();
        let into_a: Vec<_> = p
            .edges
            .iter()
            .filter(|e| e.target == "add" && e.target_handle.as_deref() == Some("in-a"))
            .collect();
        assert_eq!(into_a.len(), 1);
        assert_eq!(into_a[0].source, "num");
        // Duplicate wire is a no-op.
        assert!(connect(&mut p, c, &other).is_none());
    }

    #[test]
    fn multi_pins_gather_and_reorder() {
        let cat = Catalogue::from_json(
            r#"{"nodes":[
              {"type":"g","category":"x","label":"G","eval":"raw",
               "inputs":[{"name":"items","type":"any","kind":"multi"}],
               "outputs":[{"name":"out","type":"any"}]},
              {"type":"v","category":"x","label":"V","eval":"raw","inputs":[],
               "outputs":[{"name":"out","type":"number"}]}]}"#,
        )
        .unwrap();
        let mut p = patch(
            vec![
                node("g", "g"),
                node("a", "v"),
                node("b", "v"),
                node("c", "v"),
            ],
            vec![],
        );
        let ids: Vec<std::string::String> = ["a", "b", "c"]
            .iter()
            .map(|s| connect(&mut p, &cat, &Conn::new(s, "out-out", "g", "in-items")).unwrap())
            .collect();
        assert_eq!(p.edges.len(), 3, "multi inputs keep every wire");
        assert_eq!(edge_slot(&p, &ids[2]), Some((2, 3)));
        assert!(reorder_multi_edge(&mut p, &cat, &ids[2], -1));
        let order: Vec<&str> = p.edges.iter().map(|e| e.source.as_str()).collect();
        assert_eq!(order, ["a", "c", "b"]);
        assert!(reorder_multi_edge(&mut p, &cat, &ids[1], -3)); // to top
        let order: Vec<&str> = p.edges.iter().map(|e| e.source.as_str()).collect();
        assert_eq!(order, ["b", "a", "c"]);
        assert!(!reorder_multi_edge(&mut p, &cat, &ids[1], -1)); // already first
    }
}
