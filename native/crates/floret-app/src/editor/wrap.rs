//! Wrapping a selection in a region (ForEach / If / Cache) or a Section,
//! and dissolving a region, section or subpatch again ("unwrap").
//!
//! Wrapping draws the frame around the selection's bounds with room for
//! the frame's pins, moves the nodes in (nothing moves on screen) and
//! turns every wire crossing the new boundary into a frame port: the outer
//! wire to the port's outer pin, the inner wire from its inner pin.
//! Sections are only backdrops, so their wires stay as they are.
//!
//! Unwrapping is the reverse: the children move out to the frame's parent
//! (again staying put on screen), and each port's wires join up directly.
//!
//! Both refuse to leave a wire pointing backwards that wasn't before
//! (`floret_engine::flow`): patches flow left to right.

use super::ops::{absolute_pos, add_node, descendants, fresh_edge_id, region_size};
use super::ports::{find_node, find_node_mut, resolve_port, split_handle, Side};
use super::regions::{self, set_parent, set_size, PORT_TAB_W};
use floret_engine::patch::{PatchEdge, PatchFile, PatchNode, PortDirection, Position};
use floret_engine::spec::{Catalogue, PortType, RegionKind};
use std::collections::{HashMap, HashSet};

pub const SECTION: &str = "meta/section";

/// The frames a selection can be wrapped in, in menu order.
pub const WRAP_TYPES: [&str; 4] = ["region/foreach", "region/if", "region/cache", SECTION];

/// Room between the selection and a region's side borders: the port pills
/// sit on the borders, the top-left pin stack inside the left one.
const REGION_PAD_X: f64 = PORT_TAB_W + 28.0;
const SECTION_PAD: f64 = 24.0;
const PAD_Y: f64 = 28.0;

/// Why a wrap / unwrap didn't happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Nothing to wrap, nodes from different scopes, or not a frame.
    Invalid,
    /// It would leave a wire pointing backwards.
    Backward,
}

pub fn is_section(cat: &Catalogue, n: &PatchNode) -> bool {
    cat.get(&n.def_type).and_then(|s| s.region_kind) == Some(RegionKind::Section)
}

/// Whether `id` can be unwrapped: a region, section or subpatch.
pub fn can_unwrap(patch: &PatchFile, cat: &Catalogue, id: &str) -> bool {
    find_node(patch, id)
        .and_then(|n| cat.get(&n.def_type))
        .is_some_and(|s| s.region_kind.is_some())
}

/// The top-level nodes of a selection, if they share one parent.
fn siblings<'a>(
    patch: &'a PatchFile,
    ids: &[String],
) -> Option<(Vec<&'a PatchNode>, Option<String>)> {
    let sel: Vec<&PatchNode> = ids.iter().filter_map(|id| find_node(patch, id)).collect();
    let parent = sel.first()?.parent_id.clone();
    sel.iter()
        .all(|n| n.parent_id == parent)
        .then_some((sel, parent))
}

/// Whether `ids` can be wrapped (one scope, at least one node).
pub fn can_wrap(patch: &PatchFile, ids: &[String]) -> bool {
    siblings(patch, ids).is_some()
}

/// Run `edit` on a copy and keep it only if it leaves no new backward wire.
fn guarded<T>(
    patch: &mut PatchFile,
    edit: impl FnOnce(&mut PatchFile) -> Result<T, Refused>,
) -> Result<T, Refused> {
    let mut work = patch.clone();
    let out = edit(&mut work)?;
    if new_backward(patch, &work) {
        return Err(Refused::Backward);
    }
    *patch = work;
    Ok(out)
}

/// Whether `after` has a backward wire `before` didn't. Wires are compared
/// by their ends: rebuilt wires get new ids.
pub fn new_backward(before: &PatchFile, after: &PatchFile) -> bool {
    type Ends = (String, Option<String>, String, Option<String>);
    let ends = |p: &PatchFile| -> HashSet<Ends> {
        let ids: HashSet<String> = floret_engine::flow::backward_edges(p).into_iter().collect();
        p.edges
            .iter()
            .filter(|e| ids.contains(&e.id))
            .map(|e| {
                (
                    e.source.clone(),
                    e.source_handle.clone(),
                    e.target.clone(),
                    e.target_handle.clone(),
                )
            })
            .collect()
    };
    let old = ends(before);
    ends(after).iter().any(|e| !old.contains(e))
}

/// Wrap the sibling nodes `ids` in a new frame of `def_type` (one of
/// [`WRAP_TYPES`]). `size_of` gives a node's on-canvas size. Returns the
/// frame's id.
pub fn wrap(
    patch: &mut PatchFile,
    cat: &Catalogue,
    ids: &[String],
    def_type: &str,
    size_of: &dyn Fn(&PatchNode) -> (f64, f64),
) -> Result<String, Refused> {
    let kind = cat
        .get(def_type)
        .and_then(|s| s.region_kind)
        .filter(|k| *k != RegionKind::Subpatch)
        .ok_or(Refused::Invalid)?;
    guarded(patch, |p| wrap_in(p, cat, ids, def_type, kind, size_of))
}

fn wrap_in(
    patch: &mut PatchFile,
    cat: &Catalogue,
    ids: &[String],
    def_type: &str,
    kind: RegionKind,
    size_of: &dyn Fn(&PatchNode) -> (f64, f64),
) -> Result<String, Refused> {
    let (sel, parent) = siblings(patch, ids).ok_or(Refused::Invalid)?;
    let sel_ids: Vec<String> = sel.iter().map(|n| n.id.clone()).collect();
    // Canvas bounds of the selection.
    let (mut min, mut max) = (
        Position {
            x: f64::INFINITY,
            y: f64::INFINITY,
        },
        Position {
            x: f64::NEG_INFINITY,
            y: f64::NEG_INFINITY,
        },
    );
    for n in &sel {
        let at = absolute_pos(patch, &n.id);
        // Regions and sections are drawn at their frame size; a subpatch is
        // a collapsed node here.
        let framed = cat
            .get(&n.def_type)
            .is_some_and(|s| regions::is_drawable(s.region_kind));
        let (w, h) = if framed { region_size(n) } else { size_of(n) };
        // A frame's title strip sits above it.
        let top = if framed { regions::HEADER_H } else { 0.0 };
        min.x = min.x.min(at.x);
        min.y = min.y.min(at.y - top);
        max.x = max.x.max(at.x + w);
        max.y = max.y.max(at.y + h);
    }
    let section = kind == RegionKind::Section;
    let (pad_x, pad_top) = if section {
        (SECTION_PAD, SECTION_PAD)
    } else {
        // Below the top-left pin stack (static inputs, loop variables).
        let floor = cat.get(def_type).map_or(0.0, regions::input_pill_floor);
        (REGION_PAD_X, PAD_Y.max(floor - 8.0))
    };
    let pad_bottom = if section { SECTION_PAD } else { PAD_Y };
    let corner = Position {
        x: min.x - pad_x,
        y: min.y - pad_top,
    };
    let (w, h) = (
        (max.x - min.x + 2.0 * pad_x).max(regions::MIN_W),
        (max.y - min.y + pad_top + pad_bottom).max(regions::MIN_H),
    );
    let base = parent
        .as_deref()
        .map(|p| absolute_pos(patch, p))
        .unwrap_or_default();
    let local = Position {
        x: corner.x - base.x,
        y: corner.y - base.y,
    };
    let id = add_node(patch, cat, def_type, local).ok_or(Refused::Invalid)?;
    {
        let r = find_node_mut(patch, &id).expect("just added");
        r.parent_id = parent;
        set_size(r, w, h);
    }
    for c in &sel_ids {
        set_parent(patch, c, Some(&id));
    }
    if !section {
        add_crossing_ports(patch, cat, &id, &sel_ids);
    }
    // Wrapped inside a region or section: that one grows to fit.
    let in_frame = find_node(patch, &id)
        .and_then(|n| n.parent_id.as_deref())
        .is_some_and(|p| regions::is_drawable_node(patch, cat, p));
    if in_frame {
        regions::auto_grow(patch, &id, (w, h), &[]);
    }
    Ok(id)
}

/// Turn every wire crossing into / out of the frame `region` (holding
/// `sel` and their contents) into a frame port.
fn add_crossing_ports(patch: &mut PatchFile, cat: &Catalogue, region: &str, sel: &[String]) {
    let mut inside: HashSet<String> = sel.iter().cloned().collect();
    for s in sel {
        inside.extend(descendants(patch, s));
    }
    // One port per outside source (fanning out to several inner pins), and
    // one per inner source (fanning out to several outside pins).
    let mut by_source: HashMap<(String, String, bool), String> = HashMap::new();
    let mut add: Vec<PatchEdge> = Vec::new();
    let mut drop: HashSet<String> = HashSet::new();
    let crossing: Vec<PatchEdge> = patch
        .edges
        .iter()
        .filter(|e| inside.contains(&e.source) != inside.contains(&e.target))
        .cloned()
        .collect();
    for e in crossing {
        let (Some((_, sname)), Some((_, tname))) = (
            split_handle(e.source_handle.as_deref()),
            split_handle(e.target_handle.as_deref()),
        ) else {
            continue;
        };
        let into = inside.contains(&e.target);
        let (direction, ty) = if into {
            (
                PortDirection::Input,
                resolve_port(patch, cat, &e.target, Side::In, tname).map(|p| p.ty),
            )
        } else {
            (
                PortDirection::Output,
                resolve_port(patch, cat, &e.source, Side::Out, sname).map(|p| p.ty),
            )
        };
        let ty = ty.unwrap_or(PortType::Any);
        let sh = e.source_handle.clone().unwrap_or_default();
        let th = e.target_handle.clone().unwrap_or_default();
        let key = (e.source.clone(), sh.clone(), into);
        let (pid, first) = match by_source.get(&key) {
            Some(pid) => (pid.clone(), false),
            None => {
                let Some(pid) = regions::add_port(patch, region, ty, direction, None) else {
                    continue;
                };
                by_source.insert(key, pid.clone());
                (pid, true)
            }
        };
        let wire = |s: &str, sh: &str, t: &str, th: &str| PatchEdge {
            id: String::new(),
            source: s.into(),
            target: t.into(),
            source_handle: Some(sh.into()),
            target_handle: Some(th.into()),
        };
        let (inp, outp) = (Side::In.handle(&pid), Side::Out.handle(&pid));
        // Input port: source → outer `in-`, inner `out-` → target.
        // Output port: source → inner `in-`, outer `out-` → target.
        if first {
            add.push(wire(&e.source, &sh, region, &inp));
        }
        add.push(wire(region, &outp, &e.target, &th));
        drop.insert(e.id.clone());
    }
    patch.edges.retain(|e| !drop.contains(&e.id));
    for mut e in add {
        e.id = fresh_edge_id(patch);
        patch.edges.push(e);
    }
}

/// Dissolve the region, section or subpatch `id`: its children move to its
/// parent where they are on screen, and each frame port's wires join up
/// directly (loop variables and the frame's own inputs, which only mean
/// something on the frame, go). Returns the freed children.
pub fn unwrap(patch: &mut PatchFile, cat: &Catalogue, id: &str) -> Result<Vec<String>, Refused> {
    if !can_unwrap(patch, cat, id) {
        return Err(Refused::Invalid);
    }
    guarded(patch, |p| Ok(unwrap_in(p, id)))
}

fn unwrap_in(patch: &mut PatchFile, id: &str) -> Vec<String> {
    let frame = find_node(patch, id).expect("checked").clone();
    let children: Vec<String> = patch
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == Some(id))
        .map(|n| n.id.clone())
        .collect();
    for c in &children {
        set_parent(patch, c, frame.parent_id.as_deref());
    }
    // Each port joins what feeds its `in-` pin to what its `out-` pin feeds.
    let mut joined: Vec<PatchEdge> = Vec::new();
    for port in frame.ports.iter().flatten() {
        let (inp, outp) = (Side::In.handle(&port.id), Side::Out.handle(&port.id));
        let feeds: Vec<&PatchEdge> = patch
            .edges
            .iter()
            .filter(|e| e.target == id && e.target_handle.as_deref() == Some(inp.as_str()))
            .collect();
        for e in patch
            .edges
            .iter()
            .filter(|e| e.source == id && e.source_handle.as_deref() == Some(outp.as_str()))
        {
            for f in &feeds {
                joined.push(PatchEdge {
                    id: String::new(),
                    source: f.source.clone(),
                    target: e.target.clone(),
                    source_handle: f.source_handle.clone(),
                    target_handle: e.target_handle.clone(),
                });
            }
        }
    }
    patch.nodes.retain(|n| n.id != id);
    patch.edges.retain(|e| e.source != id && e.target != id);
    for mut e in joined {
        let dup = patch.edges.iter().any(|x| {
            x.source == e.source
                && x.target == e.target
                && x.source_handle == e.source_handle
                && x.target_handle == e.target_handle
        });
        if !dup {
            e.id = fresh_edge_id(patch);
            patch.edges.push(e);
        }
    }
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};
    use floret_engine::{Engine, FrameCtx};

    fn at(mut n: PatchNode, x: f64, y: f64) -> PatchNode {
        n.position = Position { x, y };
        n
    }

    fn size(_: &PatchNode) -> (f64, f64) {
        (120.0, 60.0)
    }

    fn with_param(mut n: PatchNode, k: &str, v: serde_json::Value) -> PatchNode {
        n.params.insert(k.into(), v);
        n
    }

    fn with_pin(mut n: PatchNode, k: &str, v: f64) -> PatchNode {
        n.port_values
            .get_or_insert_with(Default::default)
            .insert(k.into(), v.into());
        n
    }

    /// value 3 → (×2) → (+1) → probe: the last node reads 7.
    fn chain() -> PatchFile {
        patch(
            vec![
                at(
                    with_param(node("v", "value/number"), "value", 3.0.into()),
                    0.0,
                    0.0,
                ),
                at(
                    with_pin(
                        with_param(node("mul", "math/math"), "op", "multiply".into()),
                        "b",
                        2.0,
                    ),
                    300.0,
                    0.0,
                ),
                at(
                    with_pin(
                        with_param(node("add", "math/math"), "op", "add".into()),
                        "b",
                        1.0,
                    ),
                    600.0,
                    0.0,
                ),
                at(node("probe", "meta/probe"), 900.0, 0.0),
            ],
            vec![
                edge("v", "out-number", "mul", "in-a"),
                edge("mul", "out-result", "add", "in-a"),
                edge("add", "out-result", "probe", "in-in"),
            ],
        )
    }

    /// What reaches the probe after one tick.
    fn probe_value(p: &PatchFile) -> String {
        let (mut engine, warnings) = Engine::from_patch(p).expect("compiles");
        assert!(warnings.is_empty(), "{warnings:?}");
        let out = engine.tick(&FrameCtx::default()).outputs;
        let idx = engine.graph.idx("probe").expect("probe");
        format!("{:?}", out[idx].as_ref().expect("ran")[0])
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_chain_computes() {
        let v = probe_value(&chain());
        assert!(v.contains('7') && !v.contains('6'), "{v}");
    }

    #[test]
    fn wrapping_in_a_region_makes_ports_and_computes_the_same() {
        let cat = Catalogue::builtin();
        for ty in ["region/foreach", "region/cache"] {
            let mut p = chain();
            let before = probe_value(&p);
            let r = wrap(&mut p, cat, &ids(&["mul", "add"]), ty, &size).unwrap();
            let frame = find_node(&p, &r).unwrap();
            let ports = frame.ports.as_ref().unwrap();
            assert_eq!(ports.len(), 2, "{ty}: one in, one out");
            assert_eq!(ports[0].direction, PortDirection::Input);
            assert_eq!(ports[1].direction, PortDirection::Output);
            // The frame holds the nodes, which stay where they were.
            assert_eq!(
                find_node(&p, "mul").unwrap().parent_id.as_deref(),
                Some(r.as_str())
            );
            assert_eq!(absolute_pos(&p, "add"), Position { x: 600.0, y: 0.0 });
            let fp = absolute_pos(&p, &r);
            let (w, _) = region_size(frame);
            assert!(fp.x < 300.0 - PORT_TAB_W && fp.x + w > 720.0 + PORT_TAB_W);
            // Inner wire kept, each crossing wire split in two.
            assert_eq!(p.edges.len(), 5);
            assert!(floret_engine::flow::backward_edges(&p).is_empty());
            assert_eq!(probe_value(&p), before, "{ty}");
            // Unwrap: back to three direct wires, same result.
            let freed = unwrap(&mut p, cat, &r).unwrap();
            assert_eq!(freed.len(), 2);
            assert!(find_node(&p, &r).is_none());
            assert_eq!(p.edges.len(), 3);
            assert!(find_node(&p, "mul").unwrap().parent_id.is_none());
            assert_eq!(absolute_pos(&p, "add"), Position { x: 600.0, y: 0.0 });
            assert_eq!(probe_value(&p), before, "{ty} unwrapped");
        }
    }

    #[test]
    fn wrapping_in_if_makes_ports_and_unwraps_cleanly() {
        let cat = Catalogue::builtin();
        let mut p = chain();
        let before = probe_value(&p);
        let r = wrap(&mut p, cat, &ids(&["mul"]), "region/if", &size).unwrap();
        assert_eq!(find_node(&p, &r).unwrap().ports.as_ref().unwrap().len(), 2);
        unwrap(&mut p, cat, &r).unwrap();
        assert_eq!(probe_value(&p), before);
    }

    #[test]
    fn sections_wrap_without_ports_and_change_nothing() {
        let cat = Catalogue::builtin();
        let mut p = chain();
        let before = probe_value(&p);
        let s = wrap(&mut p, cat, &ids(&["v", "mul"]), SECTION, &size).unwrap();
        let sec = find_node(&p, &s).unwrap();
        assert!(sec.ports.as_ref().is_none_or(|v| v.is_empty()));
        assert_eq!(p.edges.len(), 3, "wires untouched");
        assert_eq!(
            find_node(&p, "v").unwrap().parent_id.as_deref(),
            Some(s.as_str())
        );
        assert_eq!(absolute_pos(&p, "mul"), Position { x: 300.0, y: 0.0 });
        assert_eq!(probe_value(&p), before);
        // A region inside a section still compiles in the parent scope.
        let r = wrap(&mut p, cat, &ids(&["add"]), "region/cache", &size).unwrap();
        assert_eq!(probe_value(&p), before);
        unwrap(&mut p, cat, &s).unwrap();
        unwrap(&mut p, cat, &r).unwrap();
        assert!(p.nodes.iter().all(|n| n.parent_id.is_none()));
        assert_eq!(probe_value(&p), before);
    }

    #[test]
    fn ungrouping_a_subpatch_restores_the_wires() {
        let cat = Catalogue::builtin();
        let mut p = chain();
        let before = probe_value(&p);
        let sub = super::super::subpatch::group(&mut p, cat, &ids(&["mul", "add"])).unwrap();
        assert_eq!(probe_value(&p), before);
        unwrap(&mut p, cat, &sub).unwrap();
        assert_eq!(p.edges.len(), 3);
        assert_eq!(absolute_pos(&p, "mul"), Position { x: 300.0, y: 0.0 });
        assert_eq!(probe_value(&p), before);
    }

    #[test]
    fn refuses_mixed_scopes_and_new_backward_wires() {
        let cat = Catalogue::builtin();
        let mut p = chain();
        let r = wrap(&mut p, cat, &ids(&["add"]), "region/foreach", &size).unwrap();
        // `v` is outside, `add` inside the region.
        assert_eq!(
            wrap(&mut p, cat, &ids(&["v", "add"]), SECTION, &size),
            Err(Refused::Invalid)
        );
        assert_eq!(
            wrap(&mut p, cat, &[], SECTION, &size),
            Err(Refused::Invalid)
        );
        assert_eq!(unwrap(&mut p, cat, "v"), Err(Refused::Invalid));
        unwrap(&mut p, cat, &r).unwrap();
        // `mul` at 300 feeds `add` at 600; wrapping `v` (0) and `add`
        // draws the frame from before 0, so the wire from `mul` into it
        // would point back. Nothing changes.
        let snapshot = p.to_json();
        assert_eq!(
            wrap(&mut p, cat, &ids(&["v", "add"]), "region/cache", &size),
            Err(Refused::Backward)
        );
        assert_eq!(p.to_json(), snapshot);
    }
}
