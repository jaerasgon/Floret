//! Region frames (ForEach / If / Cache, and an entered subpatch): the frame
//! port layout from `RegionNode.tsx`, frame-port edits
//! (`region-port-slice.ts`), drawing a region around existing nodes,
//! reparenting, resizing and auto-grow. Geometry is in canvas units,
//! relative to the frame's top-left corner.

use super::ops::{absolute_pos, descendants, region_size};
use super::ports::{find_node, find_node_mut, split_handle, type_name, Side};
use floret_engine::patch::PortDirection;
use floret_engine::patch::{PatchFile, PatchNode, Position, RegionPort};
use floret_engine::spec::{Catalogue, NodeSpec, PortType, RegionKind};

pub const PORT_TAB_W: f64 = 56.0;
pub const PORT_TAB_H: f64 = 14.0;
const PORT_ROW_PITCH: f64 = 20.0;
const PORT_TOP_OFFSET: f64 = 14.0;
const PORT_TAB_INSET: f64 = 4.0;
const TOP_PIN_START: f64 = 12.0;
const TOP_PIN_PITCH: f64 = 16.0;
pub const TOP_PIN_W: f64 = 72.0;
/// The region label sits on a strip this tall above the frame (the drag
/// handle, like `.region-header`).
pub const HEADER_H: f64 = 22.0;
pub const MIN_W: f64 = 200.0;
pub const MIN_H: f64 = 140.0;
/// A dragged child this close to an edge grows the region…
pub const GROW_TRIGGER: f64 = 24.0;
/// …until it has this much room again.
pub const GROW_PADDING: f64 = 32.0;
/// Wire drops this close to a region's left / right border add a port.
const EDGE_BAND: f64 = 24.0;

pub const LOOP_VARS: [&str; 3] = ["index", "count", "fraction"];

/// Whether a region kind is drawn as a frame you place nodes in by hand
/// (subpatches are entered instead).
pub fn is_drawable(kind: Option<RegionKind>) -> bool {
    matches!(
        kind,
        Some(RegionKind::Foreach | RegionKind::If | RegionKind::Cache)
    )
}

pub fn is_drawable_node(patch: &PatchFile, cat: &Catalogue, id: &str) -> bool {
    find_node(patch, id)
        .and_then(|n| cat.get(&n.def_type))
        .is_some_and(|s| is_drawable(s.region_kind))
}

/// One pin on a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FramePin {
    pub side: Side,
    pub port: String,
    pub label: String,
    pub ty: PortType,
    pub x: f64,
    pub y: f64,
    /// Frame-port id when the pin belongs to a port pill.
    pub pill: Option<String>,
}

/// A frame port's pill (the tab straddling the border).
#[derive(Clone, Debug, PartialEq)]
pub struct Pill {
    pub port: String,
    pub name: String,
    pub ty: PortType,
    pub direction: PortDirection,
    pub x: f64,
    /// Centre line.
    pub y: f64,
    /// Lowest y the pill may be dragged to.
    pub min_y: f64,
}

/// A top-left stack pill: a static input (`delay`, `condition`) or a loop
/// variable.
#[derive(Clone, Debug, PartialEq)]
pub struct TopPin {
    pub label: String,
    pub y: f64,
    pub loop_var: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameLayout {
    pub pins: Vec<FramePin>,
    pub pills: Vec<Pill>,
    pub top: Vec<TopPin>,
}

/// Lay out a region (or an expanded subpatch) frame of width `w`, height `h`.
pub fn frame_layout(n: &PatchNode, spec: &NodeSpec, w: f64, h: f64) -> FrameLayout {
    let mut out = FrameLayout::default();
    let loop_vars = matches!(
        spec.region_kind,
        Some(RegionKind::Foreach | RegionKind::Cache)
    );
    let half = PORT_TAB_H / 2.0;
    let mut row = 0.0;
    for p in &spec.inputs {
        let y = TOP_PIN_START + row * TOP_PIN_PITCH;
        row += 1.0;
        let label = p.label.clone().unwrap_or_else(|| p.name.clone());
        out.top.push(TopPin {
            label: label.clone(),
            y,
            loop_var: false,
        });
        out.pins.push(FramePin {
            side: Side::In,
            port: p.name.clone(),
            label,
            ty: p.ty,
            x: PORT_TAB_INSET + half,
            y,
            pill: None,
        });
    }
    if loop_vars {
        for v in LOOP_VARS {
            let y = TOP_PIN_START + row * TOP_PIN_PITCH;
            row += 1.0;
            out.top.push(TopPin {
                label: v.into(),
                y,
                loop_var: true,
            });
            out.pins.push(FramePin {
                side: Side::Out,
                port: format!("__loopvar_{v}"),
                label: v.into(),
                ty: PortType::Number,
                x: PORT_TAB_INSET + TOP_PIN_W - half,
                y,
                pill: None,
            });
        }
    }
    let min_input_top = if row > 0.0 {
        TOP_PIN_START + row * TOP_PIN_PITCH
    } else {
        PORT_TOP_OFFSET
    };
    let ports = n.ports.as_deref().unwrap_or_default();
    let (mut i_in, mut i_out) = (0.0, 0.0);
    for p in ports {
        let ty = PortType::parse(&p.ty).unwrap_or(PortType::Any);
        let input = p.direction == PortDirection::Input;
        let (y, min_y) = if input {
            let d = min_input_top + i_in * PORT_ROW_PITCH;
            i_in += 1.0;
            (p.y.unwrap_or(d).max(min_input_top), min_input_top)
        } else {
            let d = PORT_TOP_OFFSET + i_out * PORT_ROW_PITCH;
            i_out += 1.0;
            (p.y.unwrap_or(d), 8.0)
        };
        let tab_x = if input {
            PORT_TAB_INSET
        } else {
            w - PORT_TAB_W - PORT_TAB_INSET
        };
        out.pills.push(Pill {
            port: p.id.clone(),
            name: p.name.clone(),
            ty,
            direction: p.direction,
            x: tab_x,
            y,
            min_y: min_y.min(h - 8.0),
        });
        // Input port: outer `in-` on the border, inner `out-` inside.
        // Output port: inner `in-` inside, outer `out-` on the border.
        let (left, right) = (tab_x + half, tab_x + PORT_TAB_W - half);
        for (side, x) in [(Side::In, left), (Side::Out, right)] {
            out.pins.push(FramePin {
                side,
                port: p.id.clone(),
                label: p.name.clone(),
                ty,
                x,
                y,
                pill: Some(p.id.clone()),
            });
        }
    }
    out
}

/// `uniquePortName`: the type name, suffixed until unused on the region.
pub fn unique_port_name(existing: &[RegionPort], ty: PortType) -> String {
    let base = type_name(ty);
    let taken = |s: &str| existing.iter().any(|p| p.name == s);
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|i| format!("{base}{i}"))
        .find(|s| !taken(s))
        .expect("unbounded")
}

fn fresh_port_id(patch: &PatchFile) -> String {
    let used = |id: &str| {
        patch
            .nodes
            .iter()
            .flat_map(|n| n.ports.iter().flatten())
            .any(|p| p.id == id)
    };
    (1..)
        .map(|i| format!("p_rp_{i}"))
        .find(|id| !used(id))
        .expect("unbounded")
}

/// `addRegionPort`: a new frame port. Returns its id.
pub fn add_port(
    patch: &mut PatchFile,
    region: &str,
    ty: PortType,
    direction: PortDirection,
    y: Option<f64>,
) -> Option<String> {
    let id = fresh_port_id(patch);
    let n = find_node_mut(patch, region)?;
    let ports = n.ports.get_or_insert_with(Vec::new);
    let name = unique_port_name(ports, ty);
    ports.push(RegionPort {
        id: id.clone(),
        name,
        ty: type_name(ty).into(),
        direction,
        y,
    });
    Some(id)
}

/// `removeRegionPort`: drop the port and every wire on either of its pins.
pub fn remove_port(patch: &mut PatchFile, region: &str, port: &str) -> bool {
    let Some(n) = find_node_mut(patch, region) else {
        return false;
    };
    let Some(ports) = n.ports.as_mut() else {
        return false;
    };
    let before = ports.len();
    ports.retain(|p| p.id != port);
    if ports.len() == before {
        return false;
    }
    let on_port = |node: &str, h: Option<&str>| {
        node == region && split_handle(h).is_some_and(|(_, name)| name == port)
    };
    patch.edges.retain(|e| {
        !on_port(&e.source, e.source_handle.as_deref())
            && !on_port(&e.target, e.target_handle.as_deref())
    });
    true
}

pub fn rename_port(patch: &mut PatchFile, region: &str, port: &str, name: &str) -> bool {
    let name = name.trim();
    let Some(p) = find_node_mut(patch, region)
        .and_then(|n| n.ports.as_mut())
        .and_then(|ps| ps.iter_mut().find(|p| p.id == port))
    else {
        return false;
    };
    if name.is_empty() || p.name == name {
        return false;
    }
    p.name = name.to_owned();
    true
}

pub fn set_port_y(patch: &mut PatchFile, region: &str, port: &str, y: f64) {
    if let Some(p) = find_node_mut(patch, region)
        .and_then(|n| n.ports.as_mut())
        .and_then(|ps| ps.iter_mut().find(|p| p.id == port))
    {
        p.y = Some(y);
    }
}

/// Which frame border a canvas point is on, for a wire drop.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeHit {
    pub region: String,
    pub direction: PortDirection,
    /// Frame-local y, clamped inside the body.
    pub y: f64,
}

/// `findRegionEdgeAtFlow`: the smallest of `candidates` whose left
/// (input) or right (output) border band contains `p` (absolute coords).
pub fn edge_at<'a>(
    patch: &PatchFile,
    candidates: impl IntoIterator<Item = &'a str>,
    p: Position,
) -> Option<EdgeHit> {
    let mut best: Option<(f64, EdgeHit)> = None;
    for id in candidates {
        let Some(n) = find_node(patch, id) else {
            continue;
        };
        let (w, h) = region_size(n);
        let pos = absolute_pos(patch, id);
        if p.y < pos.y - EDGE_BAND || p.y > pos.y + h + EDGE_BAND {
            continue;
        }
        let direction = if (p.x - pos.x).abs() <= EDGE_BAND {
            PortDirection::Input
        } else if (p.x - (pos.x + w)).abs() <= EDGE_BAND {
            PortDirection::Output
        } else {
            continue;
        };
        let area = w * h;
        if best.as_ref().is_none_or(|(a, _)| area < *a) {
            let y = (p.y - pos.y).clamp(8.0, (h - 8.0).max(8.0));
            best = Some((
                area,
                EdgeHit {
                    region: id.to_owned(),
                    direction,
                    y,
                },
            ));
        }
    }
    best.map(|(_, h)| h)
}

/// `findRegionContaining`: the smallest of `candidates` containing `p`
/// (absolute coords), skipping the dragged nodes and anything inside them.
pub fn containing<'a>(
    patch: &PatchFile,
    candidates: impl IntoIterator<Item = &'a str>,
    p: Position,
    dragged: &[String],
) -> Option<String> {
    let mut blocked: Vec<String> = dragged.to_vec();
    for d in dragged {
        blocked.extend(descendants(patch, d));
    }
    let mut best: Option<(f64, String)> = None;
    for id in candidates {
        if blocked.iter().any(|b| b == id) {
            continue;
        }
        let Some(n) = find_node(patch, id) else {
            continue;
        };
        let (w, h) = region_size(n);
        let pos = absolute_pos(patch, id);
        let inside = p.x >= pos.x && p.x <= pos.x + w && p.y >= pos.y && p.y <= pos.y + h;
        if inside && best.as_ref().is_none_or(|(a, _)| w * h < *a) {
            best = Some((w * h, id.to_owned()));
        }
    }
    best.map(|(_, id)| id)
}

/// Which of the moved `ids` a press on `hit` grabbed: `hit` itself, or
/// the moved region holding it.
pub fn grabbed_of(patch: &PatchFile, ids: &[String], hit: &str) -> String {
    let mut cur = Some(hit.to_owned());
    let mut guard = 0;
    while let Some(c) = cur {
        if ids.contains(&c) || guard > 64 {
            return c;
        }
        guard += 1;
        cur = find_node(patch, &c).and_then(|n| n.parent_id.clone());
    }
    hit.to_owned()
}

/// `setNodeParent` via `moveNodeToParent`: re-home a node, keeping where
/// it sits on screen. Refuses cycles. Returns whether anything changed.
pub fn set_parent(patch: &mut PatchFile, id: &str, parent: Option<&str>) -> bool {
    let Some(cur) = find_node(patch, id) else {
        return false;
    };
    if cur.parent_id.as_deref() == parent {
        return false;
    }
    if let Some(p) = parent {
        if p == id || descendants(patch, id).iter().any(|d| d == p) || find_node(patch, p).is_none()
        {
            return false;
        }
    }
    let abs = absolute_pos(patch, id);
    let base = parent.map(|p| absolute_pos(patch, p)).unwrap_or_default();
    let n = find_node_mut(patch, id).expect("checked");
    n.parent_id = parent.map(str::to_owned);
    n.position = Position {
        x: abs.x - base.x,
        y: abs.y - base.y,
    };
    true
}

/// Store a region's frame size (node size and params, as the TS resizer
/// does).
pub fn set_size(n: &mut PatchNode, w: f64, h: f64) {
    n.width = Some(w);
    n.height = Some(h);
    n.params.insert("width".into(), w.into());
    n.params.insert("height".into(), h.into());
}

/// Resize a region to `rect` (absolute min corner + size). Moving the
/// left / top edge shifts the children back so they stay put on screen.
pub fn resize(patch: &mut PatchFile, id: &str, min: Position, w: f64, h: f64) {
    let Some(parent_abs) = find_node(patch, id).map(|n| {
        let a = absolute_pos(patch, id);
        Position {
            x: a.x - n.position.x,
            y: a.y - n.position.y,
        }
    }) else {
        return;
    };
    let old = absolute_pos(patch, id);
    let (dx, dy) = (min.x - old.x, min.y - old.y);
    let n = find_node_mut(patch, id).expect("checked");
    n.position = Position {
        x: min.x - parent_abs.x,
        y: min.y - parent_abs.y,
    };
    set_size(n, w.max(MIN_W), h.max(MIN_H));
    if dx != 0.0 || dy != 0.0 {
        for c in patch.nodes.iter_mut() {
            if c.parent_id.as_deref() == Some(id) {
                c.position.x -= dx;
                c.position.y -= dy;
            }
        }
    }
}

/// Auto-grow (`onNodeDrag`): grow `child`'s region so the child keeps
/// `GROW_PADDING` of room on every side. Growing left / up moves the
/// region and shifts its other children (not `moving`) so they stay put.
/// Returns whether the region changed.
pub fn auto_grow(
    patch: &mut PatchFile,
    child: &str,
    child_size: (f64, f64),
    moving: &[String],
) -> bool {
    let Some(c) = find_node(patch, child) else {
        return false;
    };
    let Some(region) = c.parent_id.clone() else {
        return false;
    };
    let Some(r) = find_node(patch, &region) else {
        return false;
    };
    let (pw, ph) = region_size(r);
    let (x, y) = (c.position.x, c.position.y);
    let grow = |needed: bool, amount: f64| if needed { amount.ceil() } else { 0.0 };
    let left = grow(x < GROW_TRIGGER, GROW_PADDING - x);
    let top = grow(y < GROW_TRIGGER, GROW_PADDING - y);
    let right = grow(
        x + child_size.0 + GROW_TRIGGER > pw,
        x + child_size.0 + GROW_PADDING - pw,
    );
    let bottom = grow(
        y + child_size.1 + GROW_TRIGGER > ph,
        y + child_size.1 + GROW_PADDING - ph,
    );
    if left + top + right + bottom <= 0.0 {
        return false;
    }
    let r = find_node_mut(patch, &region).expect("checked");
    r.position.x -= left;
    r.position.y -= top;
    set_size(r, pw + left + right, ph + top + bottom);
    if left > 0.0 || top > 0.0 {
        for n in patch.nodes.iter_mut() {
            if n.parent_id.as_deref() == Some(region.as_str()) && !moving.contains(&n.id) {
                n.position.x += left;
                n.position.y += top;
            }
        }
    }
    true
}

/// `addRegion` with `autoReparent`: a new region at `min` (absolute) sized
/// `w × h`, inside `scope`; every non-region sibling whose centre falls
/// inside moves into it. Returns the region id.
pub fn add_region(
    patch: &mut PatchFile,
    cat: &Catalogue,
    def_type: &str,
    min: Position,
    (w, h): (f64, f64),
    scope: Option<&str>,
    size_of: &dyn Fn(&PatchNode) -> (f64, f64),
) -> Option<String> {
    let base = scope.map(|s| absolute_pos(patch, s)).unwrap_or_default();
    let local = Position {
        x: min.x - base.x,
        y: min.y - base.y,
    };
    let id = super::ops::add_node(patch, cat, def_type, local)?;
    let r = find_node_mut(patch, &id).expect("just added");
    r.parent_id = scope.map(str::to_owned);
    set_size(r, w, h);
    let inside: Vec<String> = patch
        .nodes
        .iter()
        .filter(|n| n.id != id && n.parent_id.as_deref() == scope)
        .filter(|n| cat.get(&n.def_type).is_none_or(|s| s.region_kind.is_none()))
        .filter(|n| {
            let (nw, nh) = size_of(n);
            let (cx, cy) = (n.position.x + nw / 2.0, n.position.y + nh / 2.0);
            cx >= local.x && cx <= local.x + w && cy >= local.y && cy <= local.y + h
        })
        .map(|n| n.id.clone())
        .collect();
    for c in inside {
        set_parent(patch, &c, Some(&id));
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};

    fn region(id: &str, x: f64, y: f64, w: f64, h: f64) -> PatchNode {
        let mut r = node(id, "region/foreach");
        r.position = Position { x, y };
        r.width = Some(w);
        r.height = Some(h);
        r.ports = Some(Vec::new());
        r
    }

    fn at(mut n: PatchNode, x: f64, y: f64, parent: Option<&str>) -> PatchNode {
        n.position = Position { x, y };
        n.parent_id = parent.map(str::to_owned);
        n
    }

    #[test]
    fn the_grabbed_node_is_the_moved_one_under_the_pointer() {
        let p = patch(
            vec![
                region("r", 0.0, 0.0, 300.0, 200.0),
                at(node("a", "math/add"), 10.0, 10.0, Some("r")),
                node("b", "math/add"),
            ],
            vec![],
        );
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(grabbed_of(&p, &ids(&["a", "b"]), "b"), "b");
        // A child of a moved region: the region is what moves.
        assert_eq!(grabbed_of(&p, &ids(&["r", "b"]), "a"), "r");
        assert_eq!(grabbed_of(&p, &ids(&["b"]), "a"), "a");
    }

    #[test]
    fn foreach_frame_layout_clears_the_top_stack() {
        let cat = Catalogue::builtin();
        let mut r = region("r", 0.0, 0.0, 480.0, 280.0);
        r.ports = Some(vec![
            RegionPort {
                id: "a".into(),
                name: "a".into(),
                ty: "number".into(),
                direction: PortDirection::Input,
                y: Some(2.0),
            },
            RegionPort {
                id: "o".into(),
                name: "o".into(),
                ty: "layer".into(),
                direction: PortDirection::Output,
                y: None,
            },
        ]);
        let spec = cat.get("region/foreach").unwrap();
        let l = frame_layout(&r, spec, 480.0, 280.0);
        // delay + index / count / fraction.
        assert_eq!(l.top.len(), 4);
        let pin = |side: Side, port: &str| {
            l.pins
                .iter()
                .find(|p| p.side == side && p.port == port)
                .unwrap()
                .clone()
        };
        let a_outer = pin(Side::In, "a");
        let a_inner = pin(Side::Out, "a");
        assert!(a_outer.x < a_inner.x, "outer pin on the border");
        assert_eq!(a_outer.y, 12.0 + 4.0 * 16.0, "clamped below the top stack");
        let o_inner = pin(Side::In, "o");
        let o_outer = pin(Side::Out, "o");
        assert!(o_inner.x < o_outer.x && o_outer.x > 460.0);
        assert_eq!(o_outer.y, 14.0);
        assert!(l.pins.iter().any(|p| p.port == "__loopvar_fraction"));
    }

    #[test]
    fn port_names_and_removal() {
        let mut p = patch(
            vec![region("r", 0.0, 0.0, 300.0, 200.0), node("x", "math/add")],
            vec![],
        );
        let a = add_port(&mut p, "r", PortType::Number, PortDirection::Input, None).unwrap();
        let b = add_port(&mut p, "r", PortType::Number, PortDirection::Output, None).unwrap();
        assert_ne!(a, b);
        let names: Vec<_> = find_node(&p, "r")
            .unwrap()
            .ports
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert_eq!(names, ["number", "number2"]);
        p.edges
            .push(edge("x", "out-result", "r", &format!("in-{a}")));
        p.edges.push(edge("r", &format!("out-{a}"), "x", "in-a"));
        p.edges.push(edge("x", "out-result", "x", "in-b"));
        assert!(rename_port(&mut p, "r", &a, " count "));
        assert_eq!(
            find_node(&p, "r").unwrap().ports.as_ref().unwrap()[0].name,
            "count"
        );
        assert!(remove_port(&mut p, "r", &a));
        assert_eq!(p.edges.len(), 1, "both pins' wires go");
    }

    #[test]
    fn hit_tests_pick_the_smallest_region() {
        let p = patch(
            vec![
                region("big", 0.0, 0.0, 1000.0, 1000.0),
                region("small", 100.0, 100.0, 200.0, 200.0),
            ],
            vec![],
        );
        let ids = ["big", "small"];
        let pt = Position { x: 150.0, y: 150.0 };
        assert_eq!(containing(&p, ids, pt, &[]).as_deref(), Some("small"));
        assert_eq!(
            containing(&p, ids, pt, &["small".into()]).as_deref(),
            Some("big")
        );
        let e = edge_at(&p, ids, Position { x: 290.0, y: 120.0 }).unwrap();
        assert_eq!(
            (e.region.as_str(), e.direction),
            ("small", PortDirection::Output)
        );
        assert_eq!(e.y, 20.0);
        assert!(edge_at(&p, ids, Position { x: 500.0, y: 500.0 }).is_none());
    }

    #[test]
    fn reparent_keeps_the_screen_position() {
        let mut p = patch(
            vec![
                region("r", 100.0, 50.0, 300.0, 200.0),
                at(node("n", "math/add"), 150.0, 80.0, None),
            ],
            vec![],
        );
        assert!(set_parent(&mut p, "n", Some("r")));
        assert_eq!(
            find_node(&p, "n").unwrap().position,
            Position { x: 50.0, y: 30.0 }
        );
        assert_eq!(absolute_pos(&p, "n"), Position { x: 150.0, y: 80.0 });
        assert!(!set_parent(&mut p, "r", Some("n")), "no cycles");
        assert!(set_parent(&mut p, "n", None));
        assert_eq!(
            find_node(&p, "n").unwrap().position,
            Position { x: 150.0, y: 80.0 }
        );
    }

    #[test]
    fn auto_grow_extends_and_keeps_siblings_in_place() {
        let mut p = patch(
            vec![
                region("r", 100.0, 100.0, 300.0, 200.0),
                at(node("drag", "math/add"), 10.0, 60.0, Some("r")),
                at(node("other", "math/add"), 100.0, 60.0, Some("r")),
            ],
            vec![],
        );
        let before = absolute_pos(&p, "other");
        assert!(auto_grow(&mut p, "drag", (168.0, 66.0), &["drag".into()]));
        let r = find_node(&p, "r").unwrap();
        assert_eq!(r.position.x, 78.0);
        assert_eq!(region_size(r).0, 322.0);
        assert_eq!(absolute_pos(&p, "other"), before);
        // Right edge: just wider.
        find_node_mut(&mut p, "drag").unwrap().position.x = 200.0;
        assert!(auto_grow(&mut p, "drag", (168.0, 66.0), &["drag".into()]));
        assert_eq!(region_size(find_node(&p, "r").unwrap()).0, 400.0);
        assert!(!auto_grow(&mut p, "drag", (168.0, 66.0), &[]), "settled");
    }

    #[test]
    fn resize_from_the_left_keeps_children() {
        let mut p = patch(
            vec![
                region("r", 100.0, 100.0, 300.0, 200.0),
                at(node("c", "math/add"), 50.0, 50.0, Some("r")),
            ],
            vec![],
        );
        resize(&mut p, "r", Position { x: 60.0, y: 100.0 }, 340.0, 200.0);
        assert_eq!(absolute_pos(&p, "c"), Position { x: 150.0, y: 150.0 });
        assert_eq!(region_size(find_node(&p, "r").unwrap()), (340.0, 200.0));
        resize(&mut p, "r", Position { x: 60.0, y: 100.0 }, 10.0, 10.0);
        assert_eq!(region_size(find_node(&p, "r").unwrap()), (MIN_W, MIN_H));
    }

    #[test]
    fn drawing_a_region_lassos_nodes() {
        let cat = Catalogue::builtin();
        let mut p = patch(
            vec![
                at(node("in", "math/add"), 120.0, 120.0, None),
                at(node("out", "math/add"), 900.0, 120.0, None),
            ],
            vec![],
        );
        let id = add_region(
            &mut p,
            cat,
            "region/foreach",
            Position { x: 100.0, y: 100.0 },
            (400.0, 300.0),
            None,
            &|_| (100.0, 50.0),
        )
        .unwrap();
        assert_eq!(
            find_node(&p, "in").unwrap().parent_id.as_deref(),
            Some(id.as_str())
        );
        assert_eq!(
            find_node(&p, "in").unwrap().position,
            Position { x: 20.0, y: 20.0 }
        );
        assert!(find_node(&p, "out").unwrap().parent_id.is_none());
        assert_eq!(region_size(find_node(&p, &id).unwrap()), (400.0, 300.0));
    }
}
