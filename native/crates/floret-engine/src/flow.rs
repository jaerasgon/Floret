//! The flow rule: every patch, subpatch and region reads left to right.
//!
//! A wire may only run from a node to a node to its **right**: the target's
//! left edge (absolute canvas x) must sit strictly right of the source's
//! left edge. Feedback across frames goes through a Write → Read pair
//! (paired by slot, no wire), never through a wire pointing back.
//!
//! Region and subpatch frames: seen from outside, a frame is an ordinary
//! node (its left edge counts). Seen from inside, its input pins sit at its
//! left edge (sources, before everything it holds) and its output pins at
//! its right edge (targets, after everything it holds) — so wires between
//! a frame's inner pins and its own contents always read forward, wherever
//! the contents sit. Sections are only backdrops: their children are
//! compared like any other node.
//!
//! Used by the editor (refusing backward wires, flagging old ones) and by
//! the samples' guard test.

use crate::patch::{parse_handle, Handle, PatchEdge, PatchFile, PatchNode, PortDirection};
use std::collections::HashMap;

/// Loop-variable handles on ForEach / Cache frames (`out-__loopvar_index`):
/// sources inside the frame.
const LOOP_VAR_PREFIX: &str = "__loopvar_";

/// Nodes by id, with absolute positions resolved through the parent chain.
struct Index<'a> {
    nodes: HashMap<&'a str, &'a PatchNode>,
}

impl<'a> Index<'a> {
    fn new(patch: &'a PatchFile) -> Index<'a> {
        Index {
            nodes: patch.nodes.iter().map(|n| (n.id.as_str(), n)).collect(),
        }
    }

    /// Absolute canvas x of a node's left edge (children are stored
    /// relative to their parent).
    fn left(&self, id: &str) -> Option<f64> {
        let mut x = 0.0;
        let mut cur = Some(*self.nodes.get(id)?);
        let mut guard = 0;
        while let Some(n) = cur {
            x += n.position.x;
            guard += 1;
            if guard > 64 {
                break;
            }
            cur = n
                .parent_id
                .as_deref()
                .and_then(|p| self.nodes.get(p).copied());
        }
        Some(x)
    }

    /// Whether `id` sits (at any depth) inside `frame`.
    fn inside(&self, id: &str, frame: &str) -> bool {
        let mut cur = self.nodes.get(id).and_then(|n| n.parent_id.as_deref());
        let mut guard = 0;
        while let Some(p) = cur {
            if p == frame {
                return true;
            }
            guard += 1;
            if guard > 64 {
                return false;
            }
            cur = self.nodes.get(p).and_then(|n| n.parent_id.as_deref());
        }
        false
    }

    fn port_direction(&self, node: &str, port: &str) -> Option<PortDirection> {
        self.nodes
            .get(node)?
            .ports
            .iter()
            .flatten()
            .find(|p| p.id == port)
            .map(|p| p.direction)
    }

    /// Whether this end of a wire is a frame's inner pin: an input port's
    /// or loop variable's source side, or an output port's target side.
    fn inner(&self, node: &str, handle: Handle) -> bool {
        match handle {
            Handle::Out(name) => {
                name.starts_with(LOOP_VAR_PREFIX)
                    || self.port_direction(node, name) == Some(PortDirection::Input)
            }
            Handle::In(name) => self.port_direction(node, name) == Some(PortDirection::Output),
        }
    }

    fn is_forward(
        &self,
        source: &str,
        source_handle: Option<&str>,
        target: &str,
        target_handle: Option<&str>,
    ) -> bool {
        let (Some(sh), Some(th)) = (parse_handle(source_handle), parse_handle(target_handle))
        else {
            // Not a wire the editor makes: nothing to judge.
            return true;
        };
        let (src_inner, dst_inner) = (self.inner(source, sh), self.inner(target, th));
        // A frame's inner pins bracket its contents: input pins come before
        // everything inside, output pins after.
        if src_inner && (target == source || self.inside(target, source)) {
            return true;
        }
        if dst_inner && self.inside(source, target) {
            return true;
        }
        let (Some(sx), Some(tx)) = (self.left(source), self.left(target)) else {
            return true;
        };
        // An inner output pin seen from somewhere it doesn't hold sits on
        // the frame's right edge.
        let tx = if dst_inner {
            let w = self.nodes.get(target).map_or(0.0, |n| frame_width(n));
            tx + w
        } else {
            tx
        };
        tx > sx
    }
}

/// A frame's width as the editor draws it (its own size, else its params,
/// else the ForEach default).
fn frame_width(n: &PatchNode) -> f64 {
    n.width
        .or_else(|| n.params.get("width").and_then(|v| v.as_f64()))
        .unwrap_or(480.0)
}

/// Whether a plain wire `from → to` reads left to right: `to`'s left edge
/// sits strictly right of `from`'s (absolute canvas x). Unknown nodes are
/// not judged (true).
pub fn is_forward(patch: &PatchFile, from: &str, to: &str) -> bool {
    let ix = Index::new(patch);
    match (ix.left(from), ix.left(to)) {
        (Some(a), Some(b)) => b > a,
        _ => true,
    }
}

/// Whether the wire `source.source_handle → target.target_handle`
/// (handles as saved: `out-<port>` / `in-<port>`) reads left to right,
/// with frame pins placed as described in the module docs.
pub fn is_forward_wire(
    patch: &PatchFile,
    source: &str,
    source_handle: &str,
    target: &str,
    target_handle: &str,
) -> bool {
    Index::new(patch).is_forward(source, Some(source_handle), target, Some(target_handle))
}

/// Whether a saved edge reads left to right.
pub fn is_forward_edge(patch: &PatchFile, e: &PatchEdge) -> bool {
    Index::new(patch).is_forward(
        &e.source,
        e.source_handle.as_deref(),
        &e.target,
        e.target_handle.as_deref(),
    )
}

/// Ids of every wire in the patch that points backwards (right to left).
pub fn backward_edges(patch: &PatchFile) -> Vec<String> {
    let ix = Index::new(patch);
    patch
        .edges
        .iter()
        .filter(|e| {
            !ix.is_forward(
                &e.source,
                e.source_handle.as_deref(),
                &e.target,
                e.target_handle.as_deref(),
            )
        })
        .map(|e| e.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{PatchEdge, Position, RegionPort, CURRENT_PATCH_VERSION, PATCH_KIND};

    fn node(id: &str, x: f64, parent: Option<&str>) -> PatchNode {
        PatchNode {
            id: id.into(),
            def_type: "math/math".into(),
            position: Position { x, y: 0.0 },
            params: Default::default(),
            parent_id: parent.map(str::to_owned),
            port_values: None,
            ports: None,
            width: None,
            height: None,
            bypassed: false,
            extra: Default::default(),
        }
    }

    fn frame(id: &str, ty: &str, x: f64, w: f64, ports: &[(&str, PortDirection)]) -> PatchNode {
        let mut n = node(id, x, None);
        n.def_type = ty.into();
        n.width = Some(w);
        n.ports = Some(
            ports
                .iter()
                .map(|(p, d)| RegionPort {
                    id: (*p).into(),
                    name: (*p).into(),
                    ty: "number".into(),
                    direction: *d,
                    y: None,
                })
                .collect(),
        );
        n
    }

    fn edge(id: &str, s: &str, sh: &str, t: &str, th: &str) -> PatchEdge {
        PatchEdge {
            id: id.into(),
            source: s.into(),
            target: t.into(),
            source_handle: Some(sh.into()),
            target_handle: Some(th.into()),
        }
    }

    fn patch(nodes: Vec<PatchNode>, edges: Vec<PatchEdge>) -> PatchFile {
        PatchFile {
            kind: PATCH_KIND.into(),
            version: CURRENT_PATCH_VERSION,
            app_version: None,
            meta: None,
            nodes,
            edges,
            loop_length: None,
            seed: 0,
            presets: Vec::new(),
            legacy_slot0: false,
            extra: Default::default(),
        }
    }

    #[test]
    fn plain_wires_must_point_right() {
        let p = patch(
            vec![node("a", 0.0, None), node("b", 200.0, None)],
            vec![
                edge("fwd", "a", "out-result", "b", "in-a"),
                edge("back", "b", "out-result", "a", "in-a"),
            ],
        );
        assert!(is_forward(&p, "a", "b"));
        assert!(!is_forward(&p, "b", "a"));
        assert!(!is_forward(&p, "a", "a"), "strictly to the right");
        assert_eq!(backward_edges(&p), ["back"]);
        // Same column: not to the right.
        let p = patch(vec![node("a", 100.0, None), node("b", 100.0, None)], vec![]);
        assert!(!is_forward_wire(&p, "a", "out-x", "b", "in-y"));
    }

    #[test]
    fn positions_are_absolute_through_parents() {
        // `c` sits at 300 + 20 = 320 on the canvas, right of `a` at 250.
        let p = patch(
            vec![
                frame("r", "region/foreach", 300.0, 400.0, &[]),
                node("c", 20.0, Some("r")),
                node("a", 250.0, None),
            ],
            vec![],
        );
        assert!(is_forward(&p, "a", "c"));
        assert!(!is_forward(&p, "c", "a"));
    }

    #[test]
    fn frames_from_outside_are_plain_nodes() {
        use PortDirection::*;
        let p = patch(
            vec![
                node("src", 0.0, None),
                frame(
                    "r",
                    "region/foreach",
                    200.0,
                    400.0,
                    &[("pi", Input), ("po", Output)],
                ),
                node("dst", 700.0, None),
                node("behind", 100.0, None),
            ],
            vec![],
        );
        // Into the input port's outer pin (left edge) and out of the output
        // port's outer pin: judged by the frame's left edge.
        assert!(is_forward_wire(&p, "src", "out-x", "r", "in-pi"));
        assert!(is_forward_wire(&p, "r", "out-po", "dst", "in-a"));
        assert!(!is_forward_wire(&p, "r", "out-po", "behind", "in-a"));
        assert!(!is_forward_wire(&p, "dst", "out-x", "r", "in-pi"));
        // A static frame input (ForEach `delay`) likewise.
        assert!(!is_forward_wire(&p, "dst", "out-x", "r", "in-delay"));
        // The output port's outer pin back into the input port: a loop.
        assert!(!is_forward_wire(&p, "r", "out-po", "r", "in-pi"));
    }

    #[test]
    fn frames_from_inside_bracket_their_contents() {
        use PortDirection::*;
        let p = patch(
            vec![
                frame(
                    "r",
                    "region/foreach",
                    200.0,
                    400.0,
                    &[("pi", Input), ("po", Output)],
                ),
                // At the frame's very left edge, and (an old file) past its
                // right edge: still inside.
                node("c1", 0.0, Some("r")),
                node("c2", 500.0, Some("r")),
                node("out", 900.0, None),
            ],
            vec![],
        );
        assert!(is_forward_wire(&p, "r", "out-pi", "c1", "in-a"));
        assert!(is_forward_wire(
            &p,
            "r",
            "out-__loopvar_index",
            "c1",
            "in-a"
        ));
        assert!(is_forward_wire(&p, "c2", "out-x", "r", "in-po"));
        // Input port straight through to the output port.
        assert!(is_forward_wire(&p, "r", "out-pi", "r", "in-po"));
        // Inside, children still flow left to right among themselves.
        assert!(is_forward_wire(&p, "c1", "out-x", "c2", "in-a"));
        assert!(!is_forward_wire(&p, "c2", "out-x", "c1", "in-a"));
        // An inner output pin reached from outside sits on the right edge
        // (600): `out` at 900 is past it.
        assert!(!is_forward_wire(&p, "out", "out-x", "r", "in-po"));
    }

    #[test]
    fn subpatches_work_like_regions_and_nest() {
        use PortDirection::*;
        // Grouped subpatches keep children at offsets from the centroid,
        // some negative: inner pins still bracket them.
        let p = patch(
            vec![
                node("a", 0.0, None),
                frame(
                    "s",
                    "meta/subpatch",
                    300.0,
                    480.0,
                    &[("i", Input), ("o", Output)],
                ),
                node("k1", -200.0, Some("s")),
                node("k2", 100.0, Some("s")),
                frame("inner", "region/if", 200.0, 200.0, &[("ii", Input)]),
                node("b", 900.0, None),
            ],
            vec![
                edge("e1", "a", "out-x", "s", "in-i"),
                edge("e2", "s", "out-i", "k1", "in-a"),
                edge("e3", "k1", "out-x", "k2", "in-a"),
                edge("e4", "k2", "out-x", "s", "in-o"),
                edge("e5", "s", "out-o", "b", "in-a"),
            ],
        );
        assert!(backward_edges(&p).is_empty());
        // A region nested in the subpatch (its child, positioned relatively).
        let mut p = p;
        p.nodes[4].parent_id = Some("s".into());
        p.nodes.push(node("deep", 10.0, Some("inner")));
        assert!(is_forward_wire(&p, "inner", "out-ii", "deep", "in-a"));
        // Into the nested frame's outer pin from a sibling to its right
        // (k2 at 400 vs the frame at 500): forward; reversed, not.
        assert!(is_forward_wire(&p, "k2", "out-x", "inner", "in-ii"));
        assert!(!is_forward_wire(&p, "inner", "out-zz", "k1", "in-a"));
    }

    #[test]
    fn section_children_compare_by_their_canvas_position() {
        let mut sec = node("sec", 400.0, None);
        sec.def_type = "meta/section".into();
        let p = patch(
            vec![
                sec,
                node("in_sec", 10.0, Some("sec")),
                node("x", 300.0, None),
            ],
            vec![],
        );
        assert!(is_forward(&p, "x", "in_sec"));
        assert!(!is_forward(&p, "in_sec", "x"));
    }
}
