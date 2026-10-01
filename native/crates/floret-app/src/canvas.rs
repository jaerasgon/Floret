//! The node canvas: pan / zoom, nodes, pins, wires and live output values,
//! plus editing — wiring, selection, clipboard, undo / redo, context menus,
//! node search and auto-arrange. The edits themselves are plain functions
//! in `editor::*`; this file maps pointer and keyboard input onto them.

use crate::editor::connect::{self, Conn};
use crate::editor::history::History;
use crate::editor::popups::{self, MenuItem, Outcome, Purpose, SearchState};
use crate::editor::ports::{self, type_name, Side};
use crate::editor::search::{self, Fit};
use crate::editor::{clipboard, layout, ops};
use crate::theme::{self, BG_0, BG_2, BORDER, FG_0, FG_1, FG_2, FG_3, GOLD};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Key, Modifiers, PointerButton, Pos2,
    Rect, Sense, Shape, Stroke, Vec2,
};
use floret_engine::patch::{parse_handle, Handle, PatchFile, PatchNode, PortDirection, Position};
use floret_engine::spec::{Catalogue, PortType};
use floret_engine::tick::Outputs;
use floret_engine::{Engine, Value};
use indexmap::IndexMap;
use std::collections::HashSet;

const NODE_W: f32 = 168.0;
const HEADER_H: f32 = 26.0;
const ROW_H: f32 = 20.0;
const PIN_R: f32 = 4.5;
/// Pointer distance (screen px) that counts as "on" a pin / wire.
const PIN_HIT: f32 = 9.0;
const PIN_DROP: f32 = 16.0;
const WIRE_HIT: f32 = 6.0;

/// A pin on a node: which node, which side, which port, what type.
#[derive(Clone, Debug, PartialEq)]
struct PinRef {
    node: String,
    side: Side,
    port: String,
    ty: PortType,
}

enum Gesture {
    None,
    /// Dragging nodes; `start` holds each node's position at press time.
    Move {
        ids: Vec<String>,
        start: Vec<Position>,
        origin: Pos2,
        /// Alt-drag: the moved nodes are fresh duplicates.
        duplicated: bool,
        moved: bool,
    },
    /// Dragging a new wire out of a pin; `alt` = Alt held while dragging
    /// (drop → probe).
    Wire {
        from: PinRef,
        alt: bool,
    },
    /// Box-select from `origin`; `base` is the selection kept (shift).
    Marquee {
        origin: Pos2,
        base: Vec<String>,
    },
    Pan,
}

#[derive(Clone, Debug)]
enum MenuTarget {
    Pane,
    Node(String),
    Edge(String),
}

#[derive(Clone, Debug)]
enum Action {
    OpenSearch { screen: Pos2, at: Position },
    PickSearch(String),
    AddComment(Position),
    Copy,
    Cut,
    Paste(Position),
    PasteText(String, Position),
    Delete,
    Duplicate,
    SelectAll,
    DeleteEdge(String),
    ReorderEdge(String, i64),
    Connect(Conn),
    Probe { from: PinRef, at: Position },
    MoveOutOfRegion(String),
    Undo,
    Redo,
    Arrange,
    Fit,
}

/// Canvas camera, selection and editing state.
pub struct View {
    pub pan: Vec2,
    pub zoom: f32,
    /// Selected nodes, most recently selected last.
    selection: Vec<String>,
    selected_edge: Option<String>,
    /// Nodes raised above patch order (most recent last): selecting or
    /// dragging a node brings it to the front.
    raised: Vec<String>,
    gesture: Gesture,
    /// Snap dragged / placed nodes to the 24 px grid.
    pub snap: bool,
    /// Marching-ants flow animation on wires.
    pub wire_flow: bool,
    history: History,
    /// Last copied nodes (also mirrored to the system clipboard as JSON).
    clip: Option<PatchFile>,
    search: Option<SearchState>,
    menu: Option<(Pos2, Position, MenuTarget)>,
    /// Last pointer position over the canvas (paste / Tab anchor).
    last_pointer: Option<Pos2>,
    area: Rect,
    /// Text to put on the system clipboard at the end of the frame.
    copy_out: Option<String>,
    recompile: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            pan: vec2(40.0, 40.0),
            zoom: 1.0,
            selection: Vec::new(),
            selected_edge: None,
            raised: Vec::new(),
            gesture: Gesture::None,
            snap: false,
            wire_flow: true,
            history: History::default(),
            clip: None,
            search: None,
            menu: None,
            last_pointer: None,
            area: Rect::ZERO,
            copy_out: None,
            recompile: false,
        }
    }
}

struct Pin {
    /// Port name (handle without the `in-` / `out-` prefix).
    id: String,
    label: String,
    ty: PortType,
}

struct NodeBox {
    rect: Rect,
    label: String,
    category: String,
    inputs: Vec<Pin>,
    outputs: Vec<Pin>,
    region: bool,
    unimplemented: bool,
    /// `meta/comment` body text.
    comment: Option<String>,
}

struct Wire {
    id: String,
    pts: [Pos2; 4],
    flat: Vec<Pos2>,
    ty: PortType,
}

impl View {
    /// The node the inspector shows: the most recently selected one.
    pub fn primary(&self) -> Option<&String> {
        self.selection.last()
    }

    pub fn select_only(&mut self, id: &str) {
        self.selection = vec![id.to_owned()];
        self.selected_edge = None;
        self.raise(id);
    }

    /// A new patch was loaded: settle it (orphan wires, `valueType`
    /// latch), seed undo with it, and drop selection / overlays.
    pub fn on_load(&mut self, patch: &mut PatchFile) {
        crate::editor::settle(patch, Catalogue::builtin());
        self.history.reset(patch);
        self.selection.clear();
        self.selected_edge = None;
        self.raised.clear();
        self.gesture = Gesture::None;
        self.search = None;
        self.menu = None;
    }

    /// Record a structural edit: settle, push an undo step, and ask the
    /// app to recompile the engine.
    pub fn commit(&mut self, patch: &mut PatchFile) {
        crate::editor::settle(patch, Catalogue::builtin());
        self.history.push(patch, None);
        self.recompile = true;
    }

    /// Whether the patch changed structurally since the last call.
    pub fn take_recompile(&mut self) -> bool {
        std::mem::take(&mut self.recompile)
    }

    fn raise(&mut self, id: &str) {
        self.raised.retain(|r| r != id);
        self.raised.push(id.to_owned());
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selection.iter().any(|s| s == id)
    }

    /// Non-region nodes back-to-front: patch order, raised nodes last.
    fn draw_order(&self, boxes: &IndexMap<String, NodeBox>) -> Vec<String> {
        let mut order: Vec<String> = boxes
            .iter()
            .filter(|(id, b)| !b.region && !self.raised.contains(id))
            .map(|(id, _)| id.clone())
            .collect();
        order.extend(
            self.raised
                .iter()
                .filter(|r| boxes.get(r.as_str()).is_some_and(|b| !b.region))
                .cloned(),
        );
        order
    }

    fn to_screen(&self, origin: Pos2, p: Pos2) -> Pos2 {
        origin + (p.to_vec2() + self.pan) * self.zoom
    }

    fn to_canvas(&self, p: Pos2) -> Position {
        let c = (p - self.area.min) / self.zoom - self.pan;
        Position {
            x: f64::from(c.x),
            y: f64::from(c.y),
        }
    }

    fn snapped(&self, p: Position) -> Position {
        if self.snap {
            Position {
                x: ops::snap(p.x),
                y: ops::snap(p.y),
            }
        } else {
            p
        }
    }

    /// Fit the whole patch into `area`.
    pub fn fit(&mut self, patch: &PatchFile, area: Rect) {
        self.fit_nodes(patch, area, |_| true);
    }

    fn fit_nodes(&mut self, patch: &PatchFile, area: Rect, keep: impl Fn(&str) -> bool) {
        let cat = Catalogue::builtin();
        let mut bb = Rect::NOTHING;
        for n in patch.nodes.iter().filter(|n| keep(&n.id)) {
            let p = ops::absolute_pos(patch, &n.id);
            let (w, h) = node_size(n, cat);
            bb = bb.union(Rect::from_min_size(
                pos2(p.x as f32, p.y as f32),
                vec2(w as f32, h as f32),
            ));
        }
        if !bb.is_finite() || bb.width() <= 0.0 {
            self.pan = vec2(40.0, 40.0);
            self.zoom = 1.0;
            return;
        }
        let margin = 60.0;
        self.zoom = ((area.width() - margin) / bb.width())
            .min((area.height() - margin) / bb.height())
            .clamp(0.4, 1.25);
        let content = bb.size() * self.zoom;
        let offset = (area.size() - content) * 0.5;
        self.pan = offset / self.zoom - bb.min.to_vec2();
    }

    /// Selected ids without those whose region is also selected (a region
    /// moves / copies its contents already).
    fn top_level_selection(&self, patch: &PatchFile) -> Vec<String> {
        self.selection
            .iter()
            .filter(|id| {
                let mut cur = ports::find_node(patch, id).and_then(|n| n.parent_id.clone());
                while let Some(p) = cur {
                    if self.is_selected(&p) {
                        return false;
                    }
                    cur = ports::find_node(patch, &p).and_then(|n| n.parent_id.clone());
                }
                true
            })
            .cloned()
            .collect()
    }

    fn prune_selection(&mut self, patch: &PatchFile) {
        let ids: HashSet<&str> = patch.nodes.iter().map(|n| n.id.as_str()).collect();
        self.selection.retain(|s| ids.contains(s.as_str()));
        self.raised.retain(|s| ids.contains(s.as_str()));
        if let Some(e) = &self.selected_edge {
            if !patch.edges.iter().any(|x| &x.id == e) {
                self.selected_edge = None;
            }
        }
    }
}

/// A node's on-canvas size in canvas units.
pub fn node_size(n: &PatchNode, cat: &Catalogue) -> (f64, f64) {
    let Some(spec) = cat.get(&n.def_type) else {
        return (f64::from(NODE_W), 80.0);
    };
    if spec.region_kind.is_some() {
        return ops::region_size(n);
    }
    if n.def_type == "meta/comment" {
        let param = |k: &str, d: f64| n.params.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        return (param("width", 200.0), param("height", 80.0));
    }
    let (ins, outs) = pins(n, spec, cat, false);
    let rows = ins.len().max(outs.len()) as f32;
    (f64::from(NODE_W), f64::from(HEADER_H + rows * ROW_H + 8.0))
}

fn layout_boxes(
    patch: &PatchFile,
    engine: &Engine,
    cat: &Catalogue,
    view: &View,
    origin: Pos2,
) -> IndexMap<String, NodeBox> {
    let mut boxes = IndexMap::new();
    for n in &patch.nodes {
        let Some(spec) = cat.get(&n.def_type) else {
            continue;
        };
        let region = spec.region_kind.is_some();
        let (inputs, outputs) = pins(n, spec, cat, region);
        let (w, h) = node_size(n, cat);
        let p = ops::absolute_pos(patch, &n.id);
        let min = view.to_screen(origin, pos2(p.x as f32, p.y as f32));
        let unimplemented = engine
            .graph
            .idx(&n.id)
            .is_some_and(|i| engine.graph.nodes[i].unimplemented);
        boxes.insert(
            n.id.clone(),
            NodeBox {
                rect: Rect::from_min_size(min, vec2(w as f32, h as f32) * view.zoom),
                label: spec.label.clone(),
                category: spec.category.clone(),
                inputs,
                outputs,
                region,
                unimplemented,
                comment: (n.def_type == "meta/comment").then(|| {
                    n.params
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned()
                }),
            },
        );
    }
    boxes
}

fn pins(
    n: &PatchNode,
    spec: &floret_engine::spec::NodeSpec,
    cat: &Catalogue,
    region: bool,
) -> (Vec<Pin>, Vec<Pin>) {
    if region {
        let ports = n.ports.clone().unwrap_or_default();
        let side = |d: PortDirection| {
            ports
                .iter()
                .filter(|p| p.direction == d)
                .map(|p| Pin {
                    id: p.id.clone(),
                    label: p.name.clone(),
                    ty: PortType::parse(&p.ty).unwrap_or(PortType::Any),
                })
                .collect()
        };
        return (side(PortDirection::Input), side(PortDirection::Output));
    }
    let pin = |p: &floret_engine::spec::PortSpec| Pin {
        id: p.name.clone(),
        label: p.label.clone().unwrap_or_else(|| p.name.clone()),
        ty: p.ty,
    };
    (
        cat.resolved_inputs(spec, &n.params)
            .iter()
            .map(pin)
            .collect(),
        ports::resolved_outputs(spec, &n.params)
            .iter()
            .map(pin)
            .collect(),
    )
}

fn pin_pos(b: &NodeBox, side: Handle, zoom: f32) -> Option<Pos2> {
    let (list, x) = match side {
        Handle::In(_) => (&b.inputs, b.rect.left()),
        Handle::Out(_) => (&b.outputs, b.rect.right()),
    };
    let name = match side {
        Handle::In(n) | Handle::Out(n) => n,
    };
    if let Some(stripped) = name.strip_prefix("__loopvar_") {
        let i = ["index", "count", "fraction"]
            .iter()
            .position(|v| *v == stripped)? as f32;
        return Some(pos2(
            b.rect.left() + 10.0 * zoom,
            b.rect.top() + (HEADER_H + 10.0 + i * ROW_H) * zoom,
        ));
    }
    let i = list.iter().position(|p| p.id.as_str() == name)? as f32;
    Some(pos2(
        x,
        b.rect.top() + (HEADER_H + ROW_H * (i + 0.5)) * zoom,
    ))
}

fn wire_points(a: Pos2, b: Pos2, z: f32) -> [Pos2; 4] {
    let dx = ((b.x - a.x).abs() * 0.5).max(40.0 * z);
    [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b]
}

fn bezier(pts: [Pos2; 4], stroke: Stroke) -> egui::epaint::CubicBezierShape {
    egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, stroke)
}

fn dist_to_polyline(p: Pos2, line: &[Pos2]) -> f32 {
    line.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            (a + ab * t).distance(p)
        })
        .fold(f32::INFINITY, f32::min)
}

fn wires(patch: &PatchFile, boxes: &IndexMap<String, NodeBox>, z: f32) -> Vec<Wire> {
    let mut out = Vec::new();
    for e in &patch.edges {
        let (Some(src), Some(dst)) = (boxes.get(&e.source), boxes.get(&e.target)) else {
            continue;
        };
        let (Some(sh), Some(th)) = (
            parse_handle(e.source_handle.as_deref()),
            parse_handle(e.target_handle.as_deref()),
        ) else {
            continue;
        };
        let (Some(a), Some(b)) = (pin_pos(src, sh, z), pin_pos(dst, th, z)) else {
            continue;
        };
        let ty = match sh {
            Handle::Out(name) if ports::is_loop_var(name) => PortType::Number,
            Handle::Out(name) => src
                .outputs
                .iter()
                .find(|p| p.id.as_str() == name)
                .map_or(PortType::Any, |p| p.ty),
            Handle::In(_) => PortType::Any,
        };
        let pts = wire_points(a, b, z);
        out.push(Wire {
            id: e.id.clone(),
            pts,
            flat: bezier(pts, Stroke::NONE).flatten(Some(0.5)),
            ty,
        });
    }
    out
}

/// Hit-testing over one frame's layout.
struct Hits<'a> {
    boxes: &'a IndexMap<String, NodeBox>,
    order: &'a [String],
    wires: &'a [Wire],
    z: f32,
}

impl Hits<'_> {
    /// Topmost node (then region header) under `p`.
    fn node(&self, p: Pos2) -> Option<String> {
        if let Some(id) = self
            .order
            .iter()
            .rev()
            .find(|id| self.boxes[id.as_str()].rect.contains(p))
        {
            return Some(id.clone());
        }
        self.boxes
            .iter()
            .rev()
            .filter(|(_, b)| b.region)
            .find(|(_, b)| {
                Rect::from_min_size(b.rect.min, vec2(b.rect.width(), HEADER_H * self.z)).contains(p)
            })
            .map(|(id, _)| id.clone())
    }

    /// Every pin with its screen position, topmost node first.
    fn all_pins(&self) -> Vec<(PinRef, Pos2)> {
        let mut ids: Vec<&String> = self.order.iter().rev().collect();
        ids.extend(
            self.boxes
                .iter()
                .filter(|(_, b)| b.region)
                .map(|(id, _)| id),
        );
        let mut out = Vec::new();
        for id in ids {
            let b = &self.boxes[id.as_str()];
            for (side, list) in [(Side::In, &b.inputs), (Side::Out, &b.outputs)] {
                for p in list {
                    let h = match side {
                        Side::In => Handle::In(&p.id),
                        Side::Out => Handle::Out(&p.id),
                    };
                    if let Some(pos) = pin_pos(b, h, self.z) {
                        out.push((
                            PinRef {
                                node: id.clone(),
                                side,
                                port: p.id.clone(),
                                ty: p.ty,
                            },
                            pos,
                        ));
                    }
                }
            }
        }
        out
    }

    fn pin(&self, p: Pos2, radius: f32, side: Option<Side>) -> Option<PinRef> {
        self.all_pins()
            .into_iter()
            .filter(|(r, _)| side.is_none_or(|s| s == r.side))
            .map(|(r, pos)| (r, pos.distance(p)))
            .filter(|(_, d)| *d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(r, _)| r)
    }

    fn wire(&self, p: Pos2) -> Option<String> {
        self.wires
            .iter()
            .rev()
            .map(|w| (w, dist_to_polyline(p, &w.flat)))
            .filter(|(_, d)| *d <= WIRE_HIT)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(w, _)| w.id.clone())
    }
}

fn conn_between(a: &PinRef, b: &PinRef) -> Option<Conn> {
    Conn::between((&a.node, a.side, &a.port), (&b.node, b.side, &b.port))
}

pub fn show(
    ui: &mut egui::Ui,
    patch: &mut PatchFile,
    engine: &Engine,
    outputs: &Outputs,
    cat: &Catalogue,
    view: &mut View,
) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let area = resp.rect;
    view.area = area;
    let origin = area.min;
    let ctx = ui.ctx().clone();
    painter.rect_filled(area, 0.0, BG_0);

    // ── Camera: two-finger scroll pans, pinch / ⌘-scroll zooms around the pointer.
    if resp.hovered() {
        let (scroll, zoom_delta, pointer) =
            ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        if zoom_delta != 1.0 {
            if let Some(p) = pointer {
                let before = (p - origin) / view.zoom - view.pan;
                view.zoom = (view.zoom * zoom_delta).clamp(0.15, 3.0);
                view.pan = (p - origin) / view.zoom - before;
            }
        } else if scroll != Vec2::ZERO {
            view.pan += scroll / view.zoom;
        }
    }

    // ── Dot grid (24 px — the snap grid).
    let step = 24.0 * view.zoom;
    if step > 6.0 {
        let start = origin
            + vec2(
                (view.pan.x * view.zoom).rem_euclid(step),
                (view.pan.y * view.zoom).rem_euclid(step),
            );
        // Subtle, and fading out as the grid gets dense when zoomed out.
        let fade = ((step - 6.0) / 10.0).clamp(0.0, 1.0);
        let dot = FG_3.gamma_multiply(if view.snap { 0.5 } else { 0.28 } * fade);
        let radius = 0.8 * view.zoom.clamp(0.6, 1.2);
        let mut y = start.y;
        while y < area.bottom() {
            let mut x = start.x;
            while x < area.right() {
                painter.circle_filled(pos2(x, y), radius, dot);
                x += step;
            }
            y += step;
        }
    }

    let z = view.zoom;
    let boxes = layout_boxes(patch, engine, cat, view, origin);
    let order = view.draw_order(&boxes);
    let wire_list = wires(patch, &boxes, z);
    let hits = Hits {
        boxes: &boxes,
        order: &order,
        wires: &wire_list,
        z,
    };
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let mods = ui.input(|i| i.modifiers);
    if let Some(p) = pointer.filter(|p| area.contains(*p)) {
        view.last_pointer = Some(p);
    }
    let mut actions: Vec<Action> = Vec::new();

    // ── Gestures.
    if resp.drag_started_by(PointerButton::Primary) {
        let start = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(start) = start {
            if let Some(pin) = hits.pin(start, PIN_HIT, None) {
                view.selected_edge = None;
                view.gesture = Gesture::Wire {
                    from: pin,
                    alt: mods.alt,
                };
            } else if let Some(id) = hits.node(start) {
                if !view.is_selected(&id) {
                    if mods.shift {
                        view.selection.push(id.clone());
                    } else {
                        view.selection = vec![id.clone()];
                    }
                }
                view.selected_edge = None;
                view.raise(&id);
                let mut ids = view.top_level_selection(patch);
                let duplicated = mods.alt;
                if duplicated {
                    // Alt-drag: duplicates follow the pointer, originals stay.
                    ids = ops::duplicate(patch, &ids, 0.0, 0.0);
                    view.selection = ids.clone();
                    for i in &ids {
                        view.raise(i);
                    }
                }
                let start_pos = ids
                    .iter()
                    .map(|i| {
                        ports::find_node(patch, i)
                            .map(|n| n.position)
                            .unwrap_or_default()
                    })
                    .collect();
                view.gesture = Gesture::Move {
                    ids,
                    start: start_pos,
                    origin: start,
                    duplicated,
                    moved: false,
                };
            } else if mods.alt {
                view.gesture = Gesture::Pan;
            } else {
                let base = if mods.shift {
                    view.selection.clone()
                } else {
                    Vec::new()
                };
                view.selected_edge = None;
                view.gesture = Gesture::Marquee {
                    origin: start,
                    base,
                };
            }
        }
    } else if resp.drag_started_by(PointerButton::Middle) {
        view.gesture = Gesture::Pan;
    }

    if resp.dragged() {
        match &mut view.gesture {
            Gesture::Pan => view.pan += resp.drag_delta() / z,
            Gesture::Move {
                ids,
                start,
                origin: o,
                moved,
                ..
            } => {
                if let Some(p) = pointer {
                    let d = (p - *o) / z;
                    *moved = true;
                    for (id, s) in ids.iter().zip(start.iter()) {
                        let mut np = Position {
                            x: s.x + f64::from(d.x),
                            y: s.y + f64::from(d.y),
                        };
                        if view.snap {
                            np = Position {
                                x: ops::snap(np.x),
                                y: ops::snap(np.y),
                            };
                        }
                        if let Some(n) = ports::find_node_mut(patch, id) {
                            n.position = np;
                        }
                    }
                }
            }
            Gesture::Marquee { origin: o, base } => {
                if let Some(p) = pointer {
                    let r = Rect::from_two_pos(*o, p);
                    let mut sel = base.clone();
                    for (id, b) in &boxes {
                        if r.contains_rect(b.rect) && !sel.contains(id) {
                            sel.push(id.clone());
                        }
                    }
                    view.selection = sel;
                }
            }
            Gesture::Wire { alt, .. } => *alt = mods.alt,
            Gesture::None => {}
        }
    }

    // Wire-drag affinity: which pins would accept the dragged wire.
    let affinity: Option<(PinRef, Vec<PinMatch>)> = match &view.gesture {
        Gesture::Wire { from, .. } => Some((
            from.clone(),
            hits.all_pins()
                .into_iter()
                .map(|(r, pos)| {
                    let ok = conn_between(from, &r)
                        .is_some_and(|c| connect::check_connection(patch, cat, &c, true));
                    (r, pos, ok)
                })
                .collect(),
        )),
        _ => None,
    };
    let drop_target = |p: Pos2| -> Option<PinRef> {
        let (_, pins) = affinity.as_ref()?;
        pins.iter()
            .filter(|(_, _, ok)| *ok)
            .map(|(r, pos, _)| (r, pos.distance(p)))
            .filter(|(_, d)| *d <= PIN_DROP)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(r, _)| r.clone())
    };

    if resp.drag_stopped() {
        match std::mem::replace(&mut view.gesture, Gesture::None) {
            Gesture::Move {
                duplicated, moved, ..
            } => {
                if duplicated {
                    view.commit(patch);
                } else if moved {
                    view.history.push(patch, None);
                }
            }
            Gesture::Wire { from, alt } => {
                if let Some(p) = pointer {
                    if let Some(to) = drop_target(p) {
                        if let Some(c) = conn_between(&from, &to) {
                            actions.push(Action::Connect(c));
                        }
                    } else if hits.node(p).is_none() && hits.pin(p, PIN_HIT, None).is_none() {
                        let at = view.to_canvas(p);
                        if alt || mods.alt {
                            actions.push(Action::Probe { from, at });
                        } else {
                            view.search = Some(SearchState::new(
                                p,
                                at,
                                Fit::Pin {
                                    side: from.side,
                                    ty: from.ty,
                                },
                                Purpose::Wire {
                                    node: from.node.clone(),
                                    side: from.side,
                                    port: from.port.clone(),
                                },
                            ));
                        }
                    }
                }
            }
            Gesture::Marquee { .. } | Gesture::Pan | Gesture::None => {}
        }
    }

    // ── Clicks.
    if resp.clicked() {
        if let Some(p) = pointer {
            if hits.pin(p, PIN_HIT, None).is_some() {
                // Pins only start wires.
            } else if let Some(id) = hits.node(p) {
                if mods.shift || mods.command {
                    if view.is_selected(&id) {
                        view.selection.retain(|s| s != &id);
                    } else {
                        view.selection.push(id.clone());
                    }
                } else {
                    view.selection = vec![id.clone()];
                }
                view.selected_edge = None;
                view.raise(&id);
            } else if let Some(e) = hits.wire(p) {
                view.selected_edge = Some(e);
                view.selection.clear();
            } else if !mods.shift {
                view.selection.clear();
                view.selected_edge = None;
            }
        }
    }
    if resp.double_clicked() {
        if let Some(p) = pointer.filter(|p| hits.node(*p).is_none()) {
            let at = view.to_canvas(p);
            match hits.wire(p) {
                Some(eid) => {
                    if let Some(s) = insertion_search(patch, cat, &eid, p, at) {
                        view.search = Some(s);
                    }
                }
                None => actions.push(Action::OpenSearch { screen: p, at }),
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(p) = pointer {
            let at = view.to_canvas(p);
            let target = if let Some(id) = hits.node(p) {
                if !view.is_selected(&id) {
                    view.select_only(&id);
                }
                MenuTarget::Node(id)
            } else if let Some(e) = hits.wire(p) {
                view.selection.clear();
                view.selected_edge = Some(e.clone());
                MenuTarget::Edge(e)
            } else {
                MenuTarget::Pane
            };
            view.search = None;
            view.menu = Some((p, at, target));
        }
    }

    // ── Keyboard (not while typing in a text field or the search).
    if !ctx.text_edit_focused() && view.search.is_none() {
        let anchor = view.to_canvas(view.last_pointer.unwrap_or(area.center()));
        let cursor = view.last_pointer.unwrap_or(area.center());
        let selected_edge = view.selected_edge.clone();
        let menu_open = view.menu.is_some();
        ui.input_mut(|i| {
            let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
            if i.consume_key(cmd_shift, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y) {
                actions.push(Action::Redo);
            } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
                actions.push(Action::Undo);
            }
            if i.consume_key(Modifiers::COMMAND, Key::D) {
                actions.push(Action::Duplicate);
            }
            if i.consume_key(Modifiers::COMMAND, Key::A) {
                actions.push(Action::SelectAll);
            }
            if i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
            {
                actions.push(Action::Delete);
            }
            if let Some(e) = &selected_edge {
                if i.consume_key(Modifiers::NONE, Key::OpenBracket) {
                    actions.push(Action::ReorderEdge(e.clone(), -1));
                }
                if i.consume_key(Modifiers::NONE, Key::CloseBracket) {
                    actions.push(Action::ReorderEdge(e.clone(), 1));
                }
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                actions.push(Action::Fit);
            }
            if !menu_open && i.consume_key(Modifiers::NONE, Key::Tab) {
                actions.push(Action::OpenSearch {
                    screen: cursor,
                    at: anchor,
                });
            }
            for ev in &i.events {
                match ev {
                    egui::Event::Copy => actions.push(Action::Copy),
                    egui::Event::Cut => actions.push(Action::Cut),
                    egui::Event::Paste(text) => {
                        actions.push(Action::PasteText(text.clone(), anchor));
                    }
                    _ => {}
                }
            }
            if !menu_open && i.consume_key(Modifiers::NONE, Key::Escape) {
                view.gesture = Gesture::None;
                view.selection.clear();
                view.selected_edge = None;
            }
        });
    }

    // ── Regions (behind everything).
    let flow_t = ui.input(|i| i.time) as f32;
    for (id, b) in boxes.iter().filter(|(_, b)| b.region) {
        let selected = view.is_selected(id);
        painter.rect(
            b.rect,
            CornerRadius::same((9.0 * z) as u8),
            GOLD.gamma_multiply(0.05),
            Stroke::new(
                if selected { 2.0 } else { 1.5 },
                GOLD.gamma_multiply(if selected { 0.95 } else { 0.55 }),
            ),
            egui::StrokeKind::Inside,
        );
        painter.text(
            b.rect.min + vec2(12.0, 7.0) * z,
            Align2::LEFT_TOP,
            &b.label,
            FontId::proportional(12.0 * z),
            GOLD,
        );
    }

    // ── Wires, coloured by the source pin's type.
    let hovered_wire = match (&view.gesture, pointer) {
        (Gesture::None, Some(p))
            if resp.hovered() && hits.node(p).is_none() && hits.pin(p, PIN_HIT, None).is_none() =>
        {
            hits.wire(p)
        }
        _ => None,
    };
    let width = (1.8 * z).clamp(1.0, 3.0);
    for w in &wire_list {
        let base = theme::port_color(type_name(w.ty));
        let selected = view.selected_edge.as_ref() == Some(&w.id);
        let hovered = hovered_wire.as_ref() == Some(&w.id);
        if selected {
            painter.add(bezier(
                w.pts,
                Stroke::new(width + 5.0, GOLD.gamma_multiply(0.18)),
            ));
        }
        let (color, wdt) = if selected {
            (GOLD, width + 0.6)
        } else if hovered {
            (base, width + 0.8)
        } else {
            (mix(base, FG_2, 0.75), width)
        };
        painter.add(bezier(w.pts, Stroke::new(wdt, color)));
        if view.wire_flow && z > 0.3 {
            // Marching ants: small bright dashes flowing source → target.
            let ants = Shape::dashed_line_with_offset(
                &w.flat,
                Stroke::new(wdt, if selected { GOLD } else { base }),
                &[2.0 * z],
                &[9.0 * z],
                (flow_t * 20.0 * z).rem_euclid(11.0 * z),
            );
            painter.extend(ants);
        }
    }

    // ── In-progress wire.
    if let (Some((from, _)), Some(p)) = (&affinity, pointer) {
        if let Some(a) = boxes
            .get(&from.node)
            .and_then(|b| pin_pos(b, side_handle(from), z))
        {
            let target = drop_target(p);
            let end = target
                .as_ref()
                .and_then(|t| {
                    boxes
                        .get(&t.node)
                        .and_then(|b| pin_pos(b, side_handle(t), z))
                })
                .unwrap_or(p);
            let pts = match from.side {
                Side::Out => wire_points(a, end, z),
                Side::In => wire_points(end, a, z),
            };
            let flat = bezier(pts, Stroke::NONE).flatten(Some(0.5));
            let color = if target.is_some() {
                GOLD
            } else {
                GOLD.gamma_multiply(0.85)
            };
            painter.add(bezier(
                pts,
                Stroke::new(width + 4.0, GOLD.gamma_multiply(0.12)),
            ));
            painter.extend(Shape::dashed_line(&flat, Stroke::new(1.6, color), 5.0, 4.0));
        }
    }

    // ── Nodes.
    let values = |id: &str| -> Option<Vec<String>> {
        let idx = engine.graph.idx(id)?;
        let out = outputs.get(idx)?.as_ref()?;
        Some(out.iter().map(format_spread).collect())
    };
    let pin_look = |node: &str, side: Side, port: &str| -> PinLook {
        match &affinity {
            None => PinLook::Normal,
            Some((from, pins)) => {
                if from.node == node && from.side == side && from.port == port {
                    PinLook::Source
                } else if pins
                    .iter()
                    .any(|(r, _, ok)| *ok && r.node == node && r.side == side && r.port == port)
                {
                    PinLook::Match
                } else {
                    PinLook::Dim
                }
            }
        }
    };
    let pulse = 0.5 + 0.5 * (flow_t * std::f32::consts::TAU / 1.2).sin();
    let draw_pin = |pos: Pos2, ty: PortType, look: PinLook| {
        let color = theme::port_color(type_name(ty));
        match look {
            PinLook::Dim => {
                painter.circle(pos, PIN_R * z, color.gamma_multiply(0.18), Stroke::NONE);
            }
            PinLook::Match => {
                painter.circle_filled(
                    pos,
                    (PIN_R + 3.0 + 2.0 * pulse) * z,
                    GOLD.gamma_multiply(0.35 + 0.2 * pulse),
                );
                painter.circle(pos, PIN_R * z, color, Stroke::new(1.5, BG_2));
            }
            PinLook::Source => {
                painter.circle(pos, (PIN_R + 1.0) * z, GOLD, Stroke::new(1.5, BG_2));
            }
            PinLook::Normal => {
                painter.circle(pos, PIN_R * z, color, Stroke::new(1.5, BG_2));
            }
        }
    };
    for (id, b) in boxes.iter().filter(|(_, b)| b.region) {
        for (side, list) in [(Side::In, &b.inputs), (Side::Out, &b.outputs)] {
            for p in list {
                let h = match side {
                    Side::In => Handle::In(&p.id),
                    Side::Out => Handle::Out(&p.id),
                };
                if let Some(pos) = pin_pos(b, h, z) {
                    draw_pin(pos, p.ty, pin_look(id, side, &p.id));
                }
            }
        }
    }
    for id in &order {
        let b = &boxes[id.as_str()];
        let selected = view.is_selected(id);
        let r = CornerRadius::same((6.0 * z) as u8);
        if selected {
            // Soft gold glow (the web app's --glow-gold), not a second ring.
            let glow = egui::Shadow {
                offset: [0, 0],
                blur: 14,
                spread: 0,
                color: GOLD.gamma_multiply(0.22),
            };
            painter.add(glow.as_shape(b.rect, r));
        }
        painter.rect(
            b.rect,
            r,
            BG_2,
            Stroke::new(
                1.0,
                if selected {
                    GOLD.gamma_multiply(0.85)
                } else {
                    BORDER
                },
            ),
            egui::StrokeKind::Inside,
        );
        let header = Rect::from_min_size(b.rect.min, vec2(b.rect.width(), HEADER_H * z));
        painter.line_segment(
            [header.left_bottom(), header.right_bottom()],
            Stroke::new(1.0, BORDER),
        );
        let accent = theme::category_color(&b.category);
        painter.circle_filled(header.left_center() + vec2(12.0 * z, 0.0), 3.2 * z, accent);
        if z > 0.25 {
            painter.text(
                header.left_center() + vec2(22.0 * z, 0.0),
                Align2::LEFT_CENTER,
                &b.label,
                FontId::proportional(12.5 * z),
                FG_0,
            );
            if b.unimplemented {
                painter.text(
                    header.right_center() - vec2(8.0 * z, 0.0),
                    Align2::RIGHT_CENTER,
                    "not ported",
                    FontId::proportional(9.5 * z),
                    FG_2,
                );
            }
        }
        if let (Some(text), true) = (&b.comment, z > 0.3) {
            let body = b.rect.shrink2(vec2(10.0, 0.0) * z);
            let galley = painter.layout(
                text.clone(),
                FontId::proportional(11.5 * z),
                FG_1,
                body.width(),
            );
            let clip = Rect::from_min_max(
                pos2(body.left(), header.bottom() + 6.0 * z),
                pos2(body.right(), b.rect.bottom() - 6.0 * z),
            );
            painter.with_clip_rect(clip).galley(clip.min, galley, FG_1);
        }
        let vals = values(id);
        for (i, p) in b.inputs.iter().enumerate() {
            let y = b.rect.top() + (HEADER_H + ROW_H * (i as f32 + 0.5)) * z;
            draw_pin(pos2(b.rect.left(), y), p.ty, pin_look(id, Side::In, &p.id));
            if z > 0.3 {
                painter.text(
                    pos2(b.rect.left() + 10.0 * z, y),
                    Align2::LEFT_CENTER,
                    p.label.as_str(),
                    FontId::proportional(11.0 * z),
                    FG_1,
                );
            }
        }
        for (i, p) in b.outputs.iter().enumerate() {
            let y = b.rect.top() + (HEADER_H + ROW_H * (i as f32 + 0.5)) * z;
            draw_pin(
                pos2(b.rect.right(), y),
                p.ty,
                pin_look(id, Side::Out, &p.id),
            );
            if z > 0.3 {
                let text = match vals.as_ref().and_then(|v| v.get(i)) {
                    Some(v) if !v.is_empty() => v.clone(),
                    _ => p.label.as_str().to_owned(),
                };
                painter.text(
                    pos2(b.rect.right() - 10.0 * z, y),
                    Align2::RIGHT_CENTER,
                    text,
                    FontId::monospace(10.5 * z),
                    if vals.is_some() { FG_0 } else { FG_1 },
                );
            }
        }
    }

    // ── Marquee.
    if let (Gesture::Marquee { origin: o, .. }, Some(p)) = (&view.gesture, pointer) {
        let r = Rect::from_two_pos(*o, p);
        painter.rect(
            r,
            2.0,
            GOLD.gamma_multiply(0.06),
            Stroke::new(1.0, GOLD.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
    }

    // ── Corner toolbar.
    let bar = Rect::from_min_size(area.min + vec2(10.0, 10.0), vec2(area.width() - 20.0, 26.0));
    let mut tb = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    tb.spacing_mut().item_spacing.x = 6.0;
    if popups::chip(
        &mut tb,
        "Arrange",
        false,
        "Auto-arrange the patch left → right",
    )
    .clicked()
    {
        actions.push(Action::Arrange);
    }
    if popups::chip(&mut tb, "Snap", view.snap, "Snap nodes to the 24 px grid").clicked() {
        view.snap = !view.snap;
    }
    if popups::chip(
        &mut tb,
        "Flow",
        view.wire_flow,
        "Animate data flow along wires",
    )
    .clicked()
    {
        view.wire_flow = !view.wire_flow;
    }
    tb.add_space(4.0);
    tb.label(
        egui::RichText::new("Dbl-click / Tab: add node · drag pins to wire · right-click for more")
            .color(FG_3)
            .small(),
    );

    // ── Overlays.
    if let Some((at_screen, at, target)) = view.menu.clone() {
        let items = menu_items(patch, cat, view, &target, at);
        match popups::menu_popup(&ctx, area, at_screen, &items) {
            Outcome::Open => {}
            Outcome::Close => view.menu = None,
            Outcome::Pick(a) => {
                view.menu = None;
                actions.push(a);
            }
        }
    }
    if let Some(st) = view.search.as_mut() {
        match popups::search_popup(&ctx, area, cat, st) {
            Outcome::Open => {}
            Outcome::Close => view.search = None,
            Outcome::Pick(ty) => actions.push(Action::PickSearch(ty)),
        }
    }

    for a in actions {
        apply(a, patch, cat, view);
    }
    if let Some(text) = view.copy_out.take() {
        ctx.copy_text(text);
    }
}

/// A pin, its screen position, and whether the dragged wire may land on it.
type PinMatch = (PinRef, Pos2, bool);

#[derive(Clone, Copy)]
enum PinLook {
    Normal,
    Source,
    Match,
    Dim,
}

fn side_handle(p: &PinRef) -> Handle<'_> {
    match p.side {
        Side::In => Handle::In(&p.port),
        Side::Out => Handle::Out(&p.port),
    }
}

/// Linear blend `a·t + b·(1−t)`.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// The search for splitting wire `eid` with a new node.
fn insertion_search(
    patch: &PatchFile,
    cat: &Catalogue,
    eid: &str,
    screen: Pos2,
    at: Position,
) -> Option<SearchState> {
    let e = patch.edges.iter().find(|e| e.id == eid)?.clone();
    let (_, sname) = ports::split_handle(e.source_handle.as_deref())?;
    let (_, tname) = ports::split_handle(e.target_handle.as_deref())?;
    let src = ports::resolve_port(patch, cat, &e.source, Side::Out, sname)?.ty;
    let dst = ports::resolve_port(patch, cat, &e.target, Side::In, tname)?.ty;
    Some(SearchState::new(
        screen,
        at,
        Fit::Insert { src, dst },
        Purpose::Insert { edge: e },
    ))
}

fn menu_items(
    patch: &PatchFile,
    cat: &Catalogue,
    view: &View,
    target: &MenuTarget,
    at: Position,
) -> Vec<MenuItem<Action>> {
    let has_clip = view.clip.is_some();
    match target {
        MenuTarget::Pane => vec![
            MenuItem::new(
                "Add node…",
                Action::OpenSearch {
                    screen: view.menu.as_ref().map_or(Pos2::ZERO, |m| m.0),
                    at,
                },
            )
            .key("Dbl-click"),
            MenuItem::new("Add comment", Action::AddComment(at)),
            MenuItem::separator(),
            MenuItem::new("Paste", Action::Paste(at))
                .key(shortcut("V"))
                .enabled(has_clip),
            MenuItem::new("Select all", Action::SelectAll).key(shortcut("A")),
            MenuItem::separator(),
            MenuItem::new("Auto-arrange", Action::Arrange),
            MenuItem::new("Fit view", Action::Fit).key("F"),
            MenuItem::separator(),
            MenuItem::new("Undo", Action::Undo)
                .key(shortcut("Z"))
                .enabled(view.history.can_undo()),
            MenuItem::new("Redo", Action::Redo)
                .key(shortcut("⇧Z"))
                .enabled(view.history.can_redo()),
        ],
        MenuTarget::Node(id) => {
            let n = view.selection.len();
            let mut items = vec![
                MenuItem::new(
                    if n > 1 {
                        format!("Duplicate {n} nodes")
                    } else {
                        "Duplicate".into()
                    },
                    Action::Duplicate,
                )
                .key(shortcut("D")),
                MenuItem::new("Copy", Action::Copy).key(shortcut("C")),
                MenuItem::new("Cut", Action::Cut).key(shortcut("X")),
                MenuItem::new(
                    "Paste",
                    Action::Paste(Position {
                        x: at.x + 24.0,
                        y: at.y + 24.0,
                    }),
                )
                .key(shortcut("V"))
                .enabled(has_clip),
            ];
            if ports::find_node(patch, id).is_some_and(|n| n.parent_id.is_some()) {
                items.push(MenuItem::separator());
                items.push(MenuItem::new(
                    "Move out of region",
                    Action::MoveOutOfRegion(id.clone()),
                ));
            }
            items.push(MenuItem::separator());
            items.push(
                MenuItem::new(
                    if n > 1 {
                        format!("Delete {n} nodes")
                    } else {
                        "Delete".into()
                    },
                    Action::Delete,
                )
                .key("Del")
                .danger(),
            );
            items
        }
        MenuTarget::Edge(eid) => {
            let mut items = Vec::new();
            let multi = patch
                .edges
                .iter()
                .find(|e| &e.id == eid)
                .and_then(|e| e.target_handle.as_deref().map(|h| (e, h)))
                .is_some_and(|(e, h)| connect::is_multi_target(patch, cat, &e.target, h));
            if let (true, Some((idx, count))) = (multi, connect::edge_slot(patch, eid)) {
                let c = count as i64;
                items.extend([
                    MenuItem::new(
                        format!("Move up · ({}/{count})", idx + 1),
                        Action::ReorderEdge(eid.clone(), -1),
                    )
                    .key("[")
                    .enabled(idx > 0),
                    MenuItem::new("Move down", Action::ReorderEdge(eid.clone(), 1))
                        .key("]")
                        .enabled(idx + 1 < count),
                    MenuItem::new("Move to top", Action::ReorderEdge(eid.clone(), -c))
                        .enabled(idx > 0),
                    MenuItem::new("Move to bottom", Action::ReorderEdge(eid.clone(), c))
                        .enabled(idx + 1 < count),
                    MenuItem::separator(),
                ]);
            }
            items.push(
                MenuItem::new("Delete wire", Action::DeleteEdge(eid.clone()))
                    .key("Del")
                    .danger(),
            );
            items
        }
    }
}

/// "⌘V" on macOS, "Ctrl+V" elsewhere.
fn shortcut(k: &str) -> &'static str {
    let mac = cfg!(target_os = "macos");
    match (mac, k) {
        (true, "V") => "⌘V",
        (true, "A") => "⌘A",
        (true, "Z") => "⌘Z",
        (true, "⇧Z") => "⇧⌘Z",
        (true, "D") => "⌘D",
        (true, "C") => "⌘C",
        (true, "X") => "⌘X",
        (false, "V") => "Ctrl+V",
        (false, "A") => "Ctrl+A",
        (false, "Z") => "Ctrl+Z",
        (false, "⇧Z") => "Ctrl+Shift+Z",
        (false, "D") => "Ctrl+D",
        (false, "C") => "Ctrl+C",
        (false, "X") => "Ctrl+X",
        _ => "",
    }
}

fn apply(action: Action, patch: &mut PatchFile, cat: &Catalogue, view: &mut View) {
    match action {
        Action::OpenSearch { screen, at } => {
            view.menu = None;
            view.search = Some(SearchState::new(screen, at, Fit::Any, Purpose::Plain));
        }
        Action::PickSearch(ty) => {
            let Some(st) = view.search.take() else {
                return;
            };
            place_from_search(patch, cat, view, st, &ty);
        }
        Action::AddComment(at) => {
            if let Some(id) = ops::place_node(patch, cat, "meta/comment", view.snapped(at)) {
                view.select_only(&id);
                view.commit(patch);
            }
        }
        Action::Copy => copy(patch, view),
        Action::Cut => {
            copy(patch, view);
            apply(Action::Delete, patch, cat, view);
        }
        Action::Paste(at) => {
            if let Some(clip) = view.clip.clone() {
                paste(patch, view, &clip, at);
            }
        }
        Action::PasteText(text, at) => match clipboard::parse(&text) {
            Some(clip) => {
                paste(patch, view, &clip, at);
                view.clip = Some(clip);
            }
            None => apply(Action::Paste(at), patch, cat, view),
        },
        Action::Delete => {
            if !view.selection.is_empty() {
                let ids = std::mem::take(&mut view.selection);
                if ops::delete_nodes(patch, &ids) > 0 {
                    view.commit(patch);
                }
            } else if let Some(e) = view.selected_edge.take() {
                if connect::remove_edge(patch, &e) {
                    view.commit(patch);
                }
            }
            view.prune_selection(patch);
        }
        Action::Duplicate => {
            let ids = view.top_level_selection(patch);
            if ids.is_empty() {
                return;
            }
            let new = ops::duplicate(patch, &ids, 24.0, 24.0);
            let tops = new.len().min(ids.len());
            view.selection = new[..tops].to_vec();
            for i in &new[..tops] {
                view.raise(i);
            }
            view.commit(patch);
        }
        Action::SelectAll => {
            view.selection = patch.nodes.iter().map(|n| n.id.clone()).collect();
            view.selected_edge = None;
        }
        Action::DeleteEdge(e) => {
            if connect::remove_edge(patch, &e) {
                view.commit(patch);
            }
            view.prune_selection(patch);
        }
        Action::ReorderEdge(e, d) => {
            if connect::reorder_multi_edge(patch, cat, &e, d) {
                view.commit(patch);
            }
        }
        Action::Connect(c) => {
            if connect::connect(patch, cat, &c).is_some() {
                view.selected_edge = None;
                view.commit(patch);
            }
        }
        Action::Probe { from, at } => {
            let at = view.snapped(match from.side {
                Side::Out => at,
                Side::In => Position {
                    x: at.x - f64::from(NODE_W),
                    y: at.y,
                },
            });
            let Some(id) = ops::place_node(patch, cat, "meta/probe", at) else {
                return;
            };
            let c = match from.side {
                Side::Out => Conn::new(&from.node, &Side::Out.handle(&from.port), &id, "in-in"),
                Side::In => Conn::new(&id, "out-out", &from.node, &Side::In.handle(&from.port)),
            };
            connect::connect(patch, cat, &c);
            view.select_only(&id);
            view.commit(patch);
        }
        Action::MoveOutOfRegion(id) => {
            let abs = ops::absolute_pos(patch, &id);
            if let Some(n) = ports::find_node_mut(patch, &id) {
                n.parent_id = None;
                n.position = abs;
                view.commit(patch);
            }
        }
        Action::Undo => {
            if view.history.undo(patch) {
                view.recompile = true;
                view.prune_selection(patch);
            }
        }
        Action::Redo => {
            if view.history.redo(patch) {
                view.recompile = true;
                view.prune_selection(patch);
            }
        }
        Action::Arrange => {
            layout::auto_layout(patch, &|n| node_size(n, cat));
            view.commit(patch);
            let area = view.area;
            view.fit(patch, area);
        }
        Action::Fit => {
            let area = view.area;
            if view.selection.is_empty() {
                view.fit(patch, area);
            } else {
                let sel = view.selection.clone();
                view.fit_nodes(patch, area, |id| sel.iter().any(|s| s == id));
            }
        }
    }
}

fn copy(patch: &PatchFile, view: &mut View) {
    let ids = view.top_level_selection(patch);
    if let Some(clip) = clipboard::copy(patch, &ids) {
        // Also onto the system clipboard as `.flora` JSON.
        view.copy_out = Some(clip.to_json());
        view.clip = Some(clip);
    }
}

fn paste(patch: &mut PatchFile, view: &mut View, clip: &PatchFile, at: Position) {
    let ids = clipboard::paste(patch, clip, view.snapped(at));
    if ids.is_empty() {
        return;
    }
    let tops = clip
        .nodes
        .iter()
        .filter(|n| n.parent_id.is_none())
        .count()
        .min(ids.len());
    view.selection = ids[..tops].to_vec();
    view.selected_edge = None;
    for i in &ids[..tops] {
        view.raise(i);
    }
    view.commit(patch);
}

fn place_from_search(
    patch: &mut PatchFile,
    cat: &Catalogue,
    view: &mut View,
    st: SearchState,
    ty: &str,
) {
    let Some(spec) = cat.get(ty) else {
        return;
    };
    let at = match (&st.purpose, &st.fit) {
        // Dragged out of an input: put the node to the left of the drop.
        (Purpose::Wire { side: Side::In, .. }, _) => Position {
            x: st.at.x - f64::from(NODE_W),
            y: st.at.y - 30.0,
        },
        (Purpose::Wire { .. }, _) => Position {
            x: st.at.x,
            y: st.at.y - 30.0,
        },
        _ => Position {
            x: st.at.x - f64::from(NODE_W) / 2.0,
            y: st.at.y - 20.0,
        },
    };
    let Some(id) = ops::place_node(patch, cat, ty, view.snapped(at)) else {
        return;
    };
    match (&st.purpose, &st.fit) {
        (Purpose::Wire { node, side, port }, Fit::Pin { ty: wty, .. }) => {
            if let Some(p) = search::first_compatible(cat, spec, *side, *wty) {
                let c = match side {
                    Side::Out => {
                        Conn::new(node, &Side::Out.handle(port), &id, &Side::In.handle(&p))
                    }
                    Side::In => Conn::new(&id, &Side::Out.handle(&p), node, &Side::In.handle(port)),
                };
                connect::connect(patch, cat, &c);
            }
        }
        (Purpose::Insert { edge }, Fit::Insert { src, dst }) => {
            if let Some((i, o)) = search::insertion_ports(cat, spec, *src, *dst) {
                connect::remove_edge(patch, &edge.id);
                if let (Some(sh), Some(th)) = (&edge.source_handle, &edge.target_handle) {
                    connect::connect(
                        patch,
                        cat,
                        &Conn::new(&edge.source, sh, &id, &Side::In.handle(&i)),
                    );
                    connect::connect(
                        patch,
                        cat,
                        &Conn::new(&id, &Side::Out.handle(&o), &edge.target, th),
                    );
                }
            }
        }
        _ => {}
    }
    view.select_only(&id);
    view.commit(patch);
}

/// Short live-value text for an output pin.
fn format_spread(s: &floret_engine::Spread) -> String {
    match s.len() {
        0 => String::new(),
        1 => format_value(&s[0]),
        n => format!("[{n}]"),
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e9 {
                format!("{n:.0}")
            } else {
                format!("{n:.3}")
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => {
            let s: String = s.chars().take(14).collect();
            format!("\"{s}\"")
        }
        Value::Vec2(p) => format!("{:.1}, {:.1}", p.x, p.y),
        Value::Color(c) => format!(
            "#{:02x}{:02x}{:02x}",
            (c.r.clamp(0.0, 1.0) * 255.0) as u8,
            (c.g.clamp(0.0, 1.0) * 255.0) as u8,
            (c.b.clamp(0.0, 1.0) * 255.0) as u8
        ),
        Value::List(items) => format!("[{}]", items.len()),
        Value::Undefined | Value::Null => String::new(),
        Value::Object(_) => "{…}".into(),
        Value::Scene(n) => n.kind().to_owned(),
        Value::Style(_) => "style".into(),
        Value::Handle(h) => h.type_name().to_owned(),
    }
}
