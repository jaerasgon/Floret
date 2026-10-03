//! Where everything visible sits on screen this frame — node boxes, pins,
//! inline fields, region frames and their port pills, wires — and
//! hit-testing over it.

use crate::editor::ops;
use crate::editor::ports::{self, Side};
use crate::editor::regions::{self, PORT_TAB_H, PORT_TAB_W, TOP_PIN_W};
use crate::editor::values::{self, EditKey, FieldKind, LeftItem};
use crate::editor::widgets::{self, ProbeView};
use eframe::egui::{self, pos2, vec2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use floret_engine::patch::{parse_handle, Handle, PatchFile, PatchNode, PortDirection};
use floret_engine::spec::{Catalogue, PortKind, PortType, RegionKind};
use indexmap::IndexMap;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

/// Width of custom-body nodes, and the drop ghost.
pub const NODE_W: f32 = 168.0;
/// Content-sized nodes stay within these (`.node-grid` / `data-math`
/// min-widths in the web app; the max keeps a long text field sane).
pub const MIN_W: f32 = 84.0;
pub const MATH_MIN_W: f32 = 72.0;
pub const MAX_W: f32 = 360.0;
pub const HEADER_H: f32 = 26.0;
pub const ROW_H: f32 = 20.0;
pub const PIN_R: f32 = 4.5;
/// Pointer distance (screen px) that counts as "on" a pin / wire.
pub const PIN_HIT: f32 = 9.0;
pub const PIN_DROP: f32 = 16.0;
pub const WIRE_HIT: f32 = 6.0;
const FIELD_H: f32 = 15.0;
const RESIZE_BAND: f32 = 5.0;
pub const LABEL_FONT: f32 = 11.0;

/// How a node is drawn at the current view level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Node,
    Comment,
    /// ForEach / If / Cache frame.
    Region,
    /// The entered subpatch, drawn as a frame around its contents.
    Frame,
    /// A subpatch seen from outside: a node with one row per port.
    Collapsed,
    /// A probe: a dashed readout card with a pass-through pin pair.
    Probe,
}

impl Look {
    pub fn is_frame(self) -> bool {
        matches!(self, Look::Region | Look::Frame)
    }
}

pub fn look_of(n: &PatchNode, cat: &Catalogue, scope: Option<&str>) -> Look {
    match cat.get(&n.def_type).and_then(|s| s.region_kind) {
        Some(RegionKind::Subpatch) if scope == Some(n.id.as_str()) => Look::Frame,
        Some(RegionKind::Subpatch) => Look::Collapsed,
        Some(_) => Look::Region,
        None if n.def_type == "meta/comment" => Look::Comment,
        None if n.def_type == "meta/probe" => Look::Probe,
        None => Look::Node,
    }
}

fn subpatch_ports(
    n: &PatchNode,
    dir: PortDirection,
) -> impl Iterator<Item = &floret_engine::patch::RegionPort> {
    n.ports.iter().flatten().filter(move |p| p.direction == dir)
}

fn row_count(n: &PatchNode, cat: &Catalogue, look: Look) -> usize {
    let Some(spec) = cat.get(&n.def_type) else {
        return 2;
    };
    if look == Look::Collapsed {
        let ins = subpatch_ports(n, PortDirection::Input).count();
        let outs = subpatch_ports(n, PortDirection::Output).count();
        return ins.max(outs).max(1);
    }
    if let Some(rows) = widgets::custom_of(&n.def_type).and_then(|c| super::custom::rows(c, n, cat))
    {
        return rows;
    }
    let left = values::left_items(cat, spec, n).len();
    left.max(ports::resolved_outputs(spec, &n.params).len())
}

/// A node's size in canvas units as drawn with `look`, with no inline
/// editors open.
pub fn box_size(patch: &PatchFile, n: &PatchNode, cat: &Catalogue, look: Look) -> (f64, f64) {
    box_size_with(patch, n, cat, look, &HashSet::new())
}

/// A node's size with the inline editors in `expanded` open.
pub fn box_size_with(
    patch: &PatchFile,
    n: &PatchNode,
    cat: &Catalogue,
    look: Look,
    expanded: &HashSet<(String, String)>,
) -> (f64, f64) {
    match look {
        Look::Region | Look::Frame => ops::region_size(n),
        Look::Comment => {
            let param = |k: &str, d: f64| n.params.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
            (param("width", 200.0), param("height", 80.0))
        }
        Look::Probe => super::custom::PROBE_SIZE,
        Look::Node | Look::Collapsed => {
            let rows = row_count(n, cat, look) as f32;
            let body = match look {
                Look::Node => {
                    widgets::custom_of(&n.def_type).map_or(0.0, |c| super::custom::body_h(c, n))
                }
                _ => 0.0,
            };
            (
                f64::from(node_width(patch, n, cat, look, expanded)),
                f64::from(HEADER_H + body + rows * ROW_H + 8.0),
            )
        }
    }
}

/// A node's on-canvas size from the root view (subpatches collapsed).
pub fn node_size(patch: &PatchFile, n: &PatchNode, cat: &Catalogue) -> (f64, f64) {
    box_size(patch, n, cat, look_of(n, cat, None))
}

/// How the patch is drawn, for the auto-arrange: sizes, pin heights and
/// frames, read off one layout of every node.
pub struct Metrics {
    sizes: HashMap<String, (f64, f64)>,
    /// (node, handle) → pin height below the node's top, seen from its
    /// parent's scope.
    pins: HashMap<(String, String), f64>,
    /// The same for subpatch frames seen from inside.
    frame_pins: HashMap<(String, String), f64>,
    frames: HashSet<String>,
    /// Frame → the highest its input pills may sit.
    pill_floors: HashMap<String, f64>,
    /// Stateful nodes (their `next` input is feedback).
    stateful: HashSet<String>,
}

impl Metrics {
    pub fn of(patch: &PatchFile, cat: &Catalogue) -> Metrics {
        let painter = measuring_painter();
        let (decor, expanded) = (Decor::default(), HashSet::new());
        let pins_of = |scope: Option<&str>, ids: &HashSet<String>| {
            let cx = Ctx {
                patch,
                cat,
                painter: &painter,
                origin: Pos2::ZERO,
                pan: Vec2::ZERO,
                zoom: 1.0,
                scope,
                expanded: &expanded,
                decor: &decor,
            };
            let mut out = HashMap::new();
            for (id, b) in layout(&cx, ids, &|_| false) {
                for p in &b.pins {
                    let handle = match p.side {
                        Side::In => format!("in-{}", p.port),
                        Side::Out => format!("out-{}", p.port),
                    };
                    out.insert((id.clone(), handle), f64::from(p.pos.y - b.rect.top()));
                }
            }
            out
        };
        let all: HashSet<String> = patch.nodes.iter().map(|n| n.id.clone()).collect();
        let mut frame_pins = HashMap::new();
        for n in &patch.nodes {
            if look_of(n, cat, Some(&n.id)) == Look::Frame {
                frame_pins.extend(pins_of(Some(&n.id), &HashSet::from([n.id.clone()])));
            }
        }
        Metrics {
            sizes: patch
                .nodes
                .iter()
                .map(|n| (n.id.clone(), node_size(patch, n, cat)))
                .collect(),
            pins: pins_of(None, &all),
            frame_pins,
            frames: patch
                .nodes
                .iter()
                .filter(|n| look_of(n, cat, None) == Look::Region)
                .map(|n| n.id.clone())
                .collect(),
            pill_floors: patch
                .nodes
                .iter()
                .filter_map(|n| {
                    let spec = cat.get(&n.def_type).filter(|s| s.region_kind.is_some())?;
                    Some((n.id.clone(), regions::input_pill_floor(spec)))
                })
                .collect(),
            stateful: patch
                .nodes
                .iter()
                .filter(|n| cat.get(&n.def_type).is_some_and(|s| s.stateful))
                .map(|n| n.id.clone())
                .collect(),
        }
    }
}

impl crate::editor::layout::Measure for Metrics {
    fn size(&self, n: &PatchNode) -> (f64, f64) {
        self.sizes.get(&n.id).copied().unwrap_or((0.0, 0.0))
    }

    fn pin_y(&self, n: &PatchNode, handle: &str) -> Option<f64> {
        self.pins.get(&(n.id.clone(), handle.to_owned())).copied()
    }

    fn frame_pin_y(&self, n: &PatchNode, handle: &str) -> Option<f64> {
        let key = (n.id.clone(), handle.to_owned());
        self.frame_pins
            .get(&key)
            .or_else(|| self.pins.get(&key).filter(|_| self.frames.contains(&n.id)))
            .copied()
    }

    fn pill_floor(&self, n: &PatchNode) -> f64 {
        self.pill_floors.get(&n.id).copied().unwrap_or(0.0)
    }

    fn is_frame(&self, n: &PatchNode) -> bool {
        self.frames.contains(&n.id)
    }

    fn is_feedback(&self, n: &PatchNode, handle: &str) -> bool {
        handle == "in-next" && self.stateful.contains(&n.id)
    }
}

/// A painter to measure text with: the canvas's context, else (headless) a
/// fresh one.
pub fn measuring_painter() -> Painter {
    let ctx = MEASURE
        .read()
        .ok()
        .and_then(|m| m.clone())
        .unwrap_or_else(|| {
            let ctx = egui::Context::default();
            ctx.run_ui(egui::RawInput::default(), |_| {})
                .textures_delta
                .clear();
            ctx
        });
    Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING)
}

/// The context whose fonts measure node content (set by the canvas each
/// frame; without one, as in tests, widths are estimated).
static MEASURE: std::sync::RwLock<Option<egui::Context>> = std::sync::RwLock::new(None);

pub fn set_measure(ctx: &egui::Context) {
    if let Ok(mut m) = MEASURE.write() {
        if m.as_ref().is_none_or(|c| c != ctx) {
            *m = Some(ctx.clone());
        }
    }
}

/// Width of `s` in `font` (canvas units at zoom 1).
pub fn text_width(s: &str, font: FontId) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    let ctx = MEASURE.read().ok().and_then(|m| m.clone());
    match ctx {
        Some(ctx) => ctx.fonts_mut(|f| {
            f.layout_no_wrap(s.to_owned(), font, Color32::WHITE)
                .size()
                .x
        }),
        None => {
            let per = if font.family == egui::FontFamily::Monospace {
                0.62
            } else {
                0.56
            };
            s.chars().count() as f32 * font.size * per
        }
    }
}

/// Inputs of `node` with a wire in.
fn wired_inputs<'a>(patch: &'a PatchFile, node: &str) -> HashSet<&'a str> {
    patch
        .edges
        .iter()
        .filter(|e| e.target == node)
        .filter_map(|e| match ports::split_handle(e.target_handle.as_deref()) {
            Some((Side::In, name)) => Some(name),
            _ => None,
        })
        .collect()
}

/// A plain or collapsed-subpatch node's width: its widest row or its
/// title, like the web app's content-sized `GenericNode`.
pub fn node_width(
    patch: &PatchFile,
    n: &PatchNode,
    cat: &Catalogue,
    look: Look,
    expanded: &HashSet<(String, String)>,
) -> f32 {
    let Some(spec) = cat.get(&n.def_type) else {
        return NODE_W;
    };
    let label_font = FontId::proportional(LABEL_FONT);
    let out_font = FontId::proportional(LABEL_FONT);
    if look == Look::Node {
        if let Some(c) = widgets::custom_of(&n.def_type) {
            return custom_width(n, spec, cat, c, &label_font, &out_font);
        }
    }
    let mut widest: f32 = 0.0;
    let mut row = |left: f32, out: Option<f32>| {
        let w = 10.0 + left + out.map_or(0.0, |o| 14.0 + o) + 10.0;
        widest = widest.max(w);
    };
    let math = values::math_symbol(&n.def_type);
    let title = if look == Look::Collapsed {
        let label = n.params.get("label").and_then(|v| v.as_str());
        format!("▶ {}", label.unwrap_or(&spec.label))
    } else {
        math.map_or_else(|| spec.label.clone(), str::to_owned)
    };
    let title_font = if math.is_some() {
        FontId::monospace(13.0)
    } else {
        FontId::proportional(12.5)
    };
    let mut header = super::draw::TITLE_X + text_width(&title, title_font) + 12.0;
    if let Some(slot) = n.params.get("slot").filter(|_| look == Look::Node) {
        let text = format!("#{}", values::short(slot));
        header += 6.0 + text_width(&text, FontId::monospace(10.0)) + 8.0;
    }
    if look == Look::Collapsed {
        header += 8.0 + text_width("subpatch", FontId::proportional(9.5));
        let ins: Vec<_> = subpatch_ports(n, PortDirection::Input).collect();
        let outs: Vec<_> = subpatch_ports(n, PortDirection::Output).collect();
        for i in 0..ins.len().max(outs.len()) {
            let left = ins
                .get(i)
                .map_or(0.0, |p| text_width(&p.name, label_font.clone()));
            let out = outs.get(i).map(|p| text_width(&p.name, out_font.clone()));
            row(left, out);
        }
    } else {
        let wired = wired_inputs(patch, &n.id);
        let items = values::left_items(cat, spec, n);
        let outs = ports::resolved_outputs(spec, &n.params);
        for i in 0..items.len().max(outs.len()) {
            let out = outs.get(i).map(|p| {
                let label = values::display_label(p, false);
                text_width(&label, out_font.clone())
            });
            let left = match items.get(i) {
                None => 0.0,
                Some(LeftItem::Param { key, kind }) => {
                    text_width(key, label_font.clone()) + 6.0 + field_width(kind)
                }
                Some(LeftItem::Input(port)) => {
                    let mut w = text_width(&values::display_label(port, true), label_font.clone());
                    if port.constant {
                        w += 10.0;
                    }
                    let open = expanded.contains(&(n.id.clone(), port.name.clone()));
                    let wired = wired.contains(port.name.as_str());
                    if let Some((key, kind)) = values::input_editor(spec, port, wired) {
                        if open {
                            w += 6.0 + field_width(&kind);
                        } else {
                            let v = values::current(n, spec, &key, Some(port));
                            if !v.is_null() {
                                w += 6.0 + text_width(&values::short(&v), FontId::monospace(10.0));
                            }
                        }
                    }
                    w
                }
            };
            row(left, out);
        }
    }
    let min = if n.def_type.starts_with("math/") {
        MATH_MIN_W
    } else {
        MIN_W
    };
    widest.max(header).clamp(min, MAX_W).ceil()
}

/// A custom-body node is [`NODE_W`] wide. One whose body lays out the
/// standard rows (e.g. a Palette in a generating mode) grows when one of
/// its dropdown rows needs more room, so the dropdown fits its longest
/// option next to the row's output label.
fn custom_width(
    n: &PatchNode,
    spec: &floret_engine::spec::NodeSpec,
    cat: &Catalogue,
    c: widgets::Custom,
    label_font: &FontId,
    out_font: &FontId,
) -> f32 {
    if super::custom::rows(c, n, cat).is_some() {
        return NODE_W;
    }
    let items = values::left_items(cat, spec, n);
    let outs = ports::resolved_outputs(spec, &n.params);
    let widest = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| match item {
            LeftItem::Param {
                key,
                kind: kind @ FieldKind::Select(_),
            } => {
                let left = text_width(key, label_font.clone()) + 6.0 + field_width(kind);
                let out = outs.get(i).map_or(0.0, |p| {
                    14.0 + text_width(&values::display_label(p, false), out_font.clone())
                });
                Some(10.0 + left + out + 10.0)
            }
            _ => None,
        })
        .fold(NODE_W, f32::max);
    widest.min(MAX_W).ceil()
}

/// A pin on a node: which node, which side, which port, what type.
#[derive(Clone, Debug, PartialEq)]
pub struct PinRef {
    pub node: String,
    pub side: Side,
    pub port: String,
    pub ty: PortType,
}

pub struct PinGeom {
    pub side: Side,
    pub port: String,
    pub label: String,
    pub ty: PortType,
    pub pos: Pos2,
    pub spread: bool,
    pub constant: bool,
}

/// An inline value editor on a node row.
#[derive(Clone, Debug)]
pub struct Field {
    pub rect: Rect,
    pub node: String,
    pub key: EditKey,
    pub kind: FieldKind,
    pub value: Json,
    /// Which component a vec2 field edits.
    pub axis: Option<usize>,
    /// Scrub multiplier (value per scrub step).
    pub step: f64,
    /// Typed into with a multi-line editor.
    pub multiline: bool,
    /// Drawn as plain text until hovered (slider / pad range fields).
    pub bare: bool,
}

impl Field {
    pub fn new(rect: Rect, node: &str, key: EditKey, kind: FieldKind, value: Json) -> Field {
        Field {
            rect,
            node: node.to_owned(),
            key,
            kind,
            value,
            axis: None,
            step: 1.0,
            multiline: false,
            bare: false,
        }
    }
}

pub struct Row {
    pub y: f32,
    pub label: Option<String>,
    pub label_rect: Rect,
    /// Input port whose label toggles its inline editor.
    pub toggle: Option<String>,
    pub expanded: bool,
    pub readout: Option<String>,
    pub constant: bool,
    pub integer: bool,
    /// Output on this row: label and port.
    pub output: Option<(String, String)>,
}

pub struct PillGeom {
    pub rect: Rect,
    pub port: String,
    pub name: String,
    pub ty: PortType,
    /// Frame-local y range the pill may be dragged in.
    pub min_y: f64,
    pub max_y: f64,
}

pub struct TopGeom {
    pub rect: Rect,
    pub label: String,
    pub loop_var: bool,
}

pub struct NodeBox {
    pub rect: Rect,
    pub look: Look,
    pub label: String,
    /// `label` is a math symbol.
    pub math: bool,
    pub category: String,
    /// Drag handle: the header row, or the strip above a region frame.
    pub header: Rect,
    pub pins: Vec<PinGeom>,
    pub rows: Vec<Row>,
    pub fields: Vec<Field>,
    pub pills: Vec<PillGeom>,
    pub top: Vec<TopGeom>,
    pub unimplemented: bool,
    pub comment: Option<String>,
    /// `#slot` badge for Read / Write pairs.
    pub slot: Option<String>,
    /// Warning chip: hit rect and text.
    pub warning: Option<(Rect, String)>,
    /// Region iteration badge: rect, text, hover hint, capped.
    pub badge: Option<(Rect, String, String, bool)>,
    /// Height of a custom body between the header and the rows (canvas
    /// units).
    pub body: f32,
    /// Custom-body controls.
    pub widgets: Vec<super::custom::Widget>,
    /// Text drawn in the header's right corner (slider value, on / off …).
    pub header_value: Option<String>,
    /// Custom-body text.
    pub notes: Vec<super::custom::Note>,
    /// A probe's readout.
    pub probe: Option<ProbeView>,
}

impl NodeBox {
    pub fn pin(&self, side: Side, port: &str) -> Option<&PinGeom> {
        self.pins.iter().find(|p| p.side == side && p.port == port)
    }

    pub fn pin_at(&self, h: Handle) -> Option<Pos2> {
        match h {
            Handle::In(n) => self.pin(Side::In, n),
            Handle::Out(n) => self.pin(Side::Out, n),
        }
        .map(|p| p.pos)
    }
}

/// Per-node decorations decided outside layout.
#[derive(Default)]
pub struct Decor {
    pub warnings: HashMap<String, String>,
    /// Region id → (badge text, hover hint, capped).
    pub badges: HashMap<String, (String, String, bool)>,
    /// Probe id → what it reads.
    pub probes: HashMap<String, ProbeView>,
}

pub struct Ctx<'a> {
    pub patch: &'a PatchFile,
    pub cat: &'a Catalogue,
    pub painter: &'a Painter,
    pub origin: Pos2,
    pub pan: Vec2,
    pub zoom: f32,
    pub scope: Option<&'a str>,
    pub expanded: &'a HashSet<(String, String)>,
    pub decor: &'a Decor,
}

impl Ctx<'_> {
    pub fn screen(&self, x: f64, y: f64) -> Pos2 {
        self.origin + (vec2(x as f32, y as f32) + self.pan) * self.zoom
    }

    /// Label width in canvas units.
    pub fn text_w(&self, s: &str, size: f32) -> f32 {
        self.painter
            .layout_no_wrap(s.to_owned(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    }
}

/// Room a dropdown keeps for its ▾ and insets around the option text.
const SELECT_CHROME: f32 = 20.0;

fn field_width(kind: &FieldKind) -> f32 {
    match kind {
        FieldKind::Number { .. } => 48.0,
        FieldKind::Vec2 => 84.0,
        FieldKind::Bool => 12.0,
        FieldKind::Text | FieldKind::Color => 70.0,
        // Wide enough for the longest option.
        FieldKind::Select(opts) => opts
            .iter()
            .map(|o| text_width(values::option_label(o), FontId::monospace(10.0)) + SELECT_CHROME)
            .fold(56.0, f32::max)
            .min(160.0),
    }
}

pub fn layout(
    cx: &Ctx,
    visible: &HashSet<String>,
    unimplemented: &dyn Fn(&str) -> bool,
) -> IndexMap<String, NodeBox> {
    let wired: HashSet<(&str, &str)> = cx
        .patch
        .edges
        .iter()
        .filter_map(|e| match ports::split_handle(e.target_handle.as_deref()) {
            Some((Side::In, name)) => Some((e.target.as_str(), name)),
            _ => None,
        })
        .collect();
    let mut boxes = IndexMap::new();
    for n in cx.patch.nodes.iter().filter(|n| visible.contains(&n.id)) {
        let Some(spec) = cx.cat.get(&n.def_type) else {
            continue;
        };
        let look = look_of(n, cx.cat, cx.scope);
        let (w, h) = box_size_with(cx.patch, n, cx.cat, look, cx.expanded);
        let p = ops::absolute_pos(cx.patch, &n.id);
        let z = cx.zoom;
        let rect = Rect::from_min_size(cx.screen(p.x, p.y), vec2(w as f32, h as f32) * z);
        let label = n
            .params
            .get("label")
            .and_then(|v| v.as_str())
            .filter(|_| look.is_frame() || look == Look::Collapsed)
            .map(str::to_owned)
            .or_else(|| values::math_symbol(&n.def_type).map(str::to_owned))
            .unwrap_or_else(|| spec.label.clone());
        let header = if look.is_frame() {
            Rect::from_min_max(
                rect.min - vec2(0.0, regions::HEADER_H as f32 * z),
                pos2(rect.right(), rect.top()),
            )
        } else {
            Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H * z))
        };
        let mut b = NodeBox {
            rect,
            look,
            math: look == Look::Node && values::math_symbol(&n.def_type).is_some(),
            label,
            category: spec.category.clone(),
            header,
            pins: Vec::new(),
            rows: Vec::new(),
            fields: Vec::new(),
            pills: Vec::new(),
            top: Vec::new(),
            unimplemented: unimplemented(&n.id),
            comment: (look == Look::Comment).then(|| {
                n.params
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned()
            }),
            slot: n
                .params
                .get("slot")
                .filter(|_| look == Look::Node)
                .map(|v| format!("#{}", values::short(v))),
            warning: None,
            badge: None,
            body: 0.0,
            widgets: Vec::new(),
            header_value: None,
            notes: Vec::new(),
            probe: None,
        };
        match look {
            Look::Region | Look::Frame => frame_box(cx, n, spec, (w, h), &mut b),
            Look::Collapsed => collapsed_box(cx, n, &mut b),
            Look::Node => match widgets::custom_of(&n.def_type) {
                Some(c) => super::custom::layout(cx, n, spec, c, &wired, &mut b),
                None => row_box(cx, n, spec, &wired, &mut b),
            },
            Look::Probe => super::custom::probe_box(cx, n, &mut b),
            Look::Comment => super::custom::comment_box(cx, n, &mut b),
        }
        chips(cx, n, &mut b);
        boxes.insert(n.id.clone(), b);
    }
    boxes
}

/// Header chips: region badge, then the warning chip.
fn chips(cx: &Ctx, n: &PatchNode, b: &mut NodeBox) {
    let z = cx.zoom;
    let size = vec2(16.0, 14.0) * z;
    if b.look.is_frame() {
        let mut x = b.header.left() + (8.0 + cx.text_w(&b.label, 12.0) + 8.0) * z;
        if let Some((text, hint, capped)) = cx.decor.badges.get(&n.id) {
            let w = (cx.text_w(text, 10.0) + if *capped { 22.0 } else { 12.0 }) * z;
            let r = Rect::from_min_size(pos2(x, b.header.center().y - 7.0 * z), vec2(w, 14.0 * z));
            b.badge = Some((r, text.clone(), hint.clone(), *capped));
            x = r.right() + 6.0 * z;
        }
        if let Some(text) = cx.decor.warnings.get(&n.id) {
            let r = Rect::from_min_size(pos2(x, b.header.center().y - 7.0 * z), size);
            b.warning = Some((r, text.clone()));
        }
    } else if let Some(text) = cx.decor.warnings.get(&n.id) {
        let r = Rect::from_center_size(b.header.right_center() - vec2(18.0 * z, 0.0), size);
        b.warning = Some((r, text.clone()));
    }
}

fn frame_box(
    cx: &Ctx,
    n: &PatchNode,
    spec: &floret_engine::spec::NodeSpec,
    (w, h): (f64, f64),
    b: &mut NodeBox,
) {
    let fl = regions::frame_layout(n, spec, w, h);
    let at = |x: f64, y: f64| b.rect.min + vec2(x as f32, y as f32) * cx.zoom;
    let gathers = look_spreads(spec.region_kind);
    for p in &fl.pins {
        // The outer pin of a ForEach / Cache port carries the whole spread.
        let input = |id: &String| {
            n.ports
                .iter()
                .flatten()
                .any(|rp| &rp.id == id && rp.direction == PortDirection::Input)
        };
        let outer = p
            .pill
            .as_ref()
            .is_some_and(|id| input(id) == (p.side == Side::In));
        b.pins.push(PinGeom {
            side: p.side,
            port: p.port.clone(),
            label: p.label.clone(),
            ty: p.ty,
            pos: at(p.x, p.y),
            spread: gathers && outer,
            constant: false,
        });
    }
    let half = PORT_TAB_H / 2.0;
    for p in &fl.pills {
        b.pills.push(PillGeom {
            rect: Rect::from_min_size(
                at(p.x, p.y - half),
                vec2(PORT_TAB_W as f32, PORT_TAB_H as f32) * cx.zoom,
            ),
            port: p.port.clone(),
            name: p.name.clone(),
            ty: p.ty,
            min_y: p.min_y,
            max_y: (h - 8.0).max(p.min_y),
        });
    }
    for t in &fl.top {
        b.top.push(TopGeom {
            rect: Rect::from_min_size(
                at(4.0, t.y - half),
                vec2(TOP_PIN_W as f32, PORT_TAB_H as f32) * cx.zoom,
            ),
            label: t.label.clone(),
            loop_var: t.loop_var,
        });
    }
}

/// ForEach / Cache frame ports gather across iterations; If and subpatch
/// ports pass a value straight through.
fn look_spreads(kind: Option<RegionKind>) -> bool {
    matches!(kind, Some(RegionKind::Foreach | RegionKind::Cache))
}

pub fn row_y(b: &NodeBox, i: usize, z: f32) -> f32 {
    b.rect.top() + (HEADER_H + b.body + ROW_H * (i as f32 + 0.5)) * z
}

fn collapsed_box(cx: &Ctx, n: &PatchNode, b: &mut NodeBox) {
    let z = cx.zoom;
    let ins: Vec<_> = subpatch_ports(n, PortDirection::Input).collect();
    let outs: Vec<_> = subpatch_ports(n, PortDirection::Output).collect();
    for i in 0..ins.len().max(outs.len()).max(1) {
        let y = row_y(b, i, z);
        let input = ins.get(i);
        if let Some(p) = input {
            b.pins.push(PinGeom {
                side: Side::In,
                port: p.id.clone(),
                label: p.name.clone(),
                ty: PortType::parse(&p.ty).unwrap_or(PortType::Any),
                pos: pos2(b.rect.left(), y),
                spread: false,
                constant: false,
            });
        }
        if let Some(p) = outs.get(i) {
            b.pins.push(PinGeom {
                side: Side::Out,
                port: p.id.clone(),
                label: p.name.clone(),
                ty: PortType::parse(&p.ty).unwrap_or(PortType::Any),
                pos: pos2(b.rect.right(), y),
                spread: false,
                constant: false,
            });
        }
        b.rows.push(Row {
            y,
            label: input.map(|p| p.name.clone()),
            label_rect: Rect::NOTHING,
            toggle: None,
            expanded: false,
            readout: None,
            constant: false,
            integer: false,
            output: outs.get(i).map(|p| (p.name.clone(), p.id.clone())),
        });
    }
}

pub fn row_box(
    cx: &Ctx,
    n: &PatchNode,
    spec: &floret_engine::spec::NodeSpec,
    wired: &HashSet<(&str, &str)>,
    b: &mut NodeBox,
) {
    let z = cx.zoom;
    let items = values::left_items(cx.cat, spec, n);
    let outs = ports::resolved_outputs(spec, &n.params);
    for i in 0..items.len().max(outs.len()) {
        let y = row_y(b, i, z);
        let output = outs
            .get(i)
            .map(|p| (values::display_label(p, false), p.name.clone()));
        if let Some(p) = outs.get(i) {
            b.pins.push(PinGeom {
                side: Side::Out,
                port: p.name.clone(),
                label: values::display_label(p, false),
                ty: p.ty,
                pos: pos2(b.rect.right(), y),
                spread: p.kind == PortKind::Spread,
                constant: false,
            });
        }
        let mut row = Row {
            y,
            label: None,
            label_rect: Rect::NOTHING,
            toggle: None,
            expanded: false,
            readout: None,
            constant: false,
            integer: false,
            output,
        };
        // (key, kind, value) of the editor shown on this row.
        let mut editor: Option<(EditKey, FieldKind, Json)> = None;
        match items.get(i) {
            None => {}
            Some(LeftItem::Param { key, kind }) => {
                row.label = Some(key.clone());
                let k = EditKey::Param(key.clone());
                let v = values::current(n, spec, &k, None);
                editor = Some((k, kind.clone(), v));
            }
            Some(LeftItem::Input(port)) => {
                let is_wired = wired.contains(&(n.id.as_str(), port.name.as_str()));
                b.pins.push(PinGeom {
                    side: Side::In,
                    port: port.name.clone(),
                    label: values::display_label(port, true),
                    ty: port.ty,
                    pos: pos2(b.rect.left(), y),
                    spread: port.kind == PortKind::Spread,
                    constant: port.constant,
                });
                row.label = Some(values::display_label(port, true));
                row.constant = port.constant;
                row.integer = port.integer && port.ty == PortType::Number;
                if let Some((key, kind)) = values::input_editor(spec, port, is_wired) {
                    row.toggle = Some(port.name.clone());
                    row.expanded = cx.expanded.contains(&(n.id.clone(), port.name.clone()));
                    let v = values::current(n, spec, &key, Some(port));
                    if row.expanded {
                        editor = Some((key, kind, v));
                    } else if !v.is_null() {
                        // The live value: the param for param-backed pins
                        // (Renderer width / height), else portValues or the
                        // declared default.
                        row.readout = Some(values::short(&v));
                    }
                }
            }
        }
        if let Some(label) = &row.label {
            let mut lw = cx.text_w(label, LABEL_FONT);
            if row.constant {
                lw += 10.0;
            }
            row.label_rect = Rect::from_min_size(
                pos2(b.rect.left() + 8.0 * z, y - ROW_H * z / 2.0),
                vec2(lw + 4.0, ROW_H) * z,
            );
            if let Some((key, kind, value)) = editor {
                let x0 = 10.0 + lw + 6.0;
                // Leave room for the row's output label.
                let width = b.rect.width() / z;
                let right = match outs.get(i) {
                    Some(p) => {
                        let label = values::display_label(p, false);
                        let out = text_width(&label, FontId::proportional(LABEL_FONT));
                        width - 10.0 - out - 14.0
                    }
                    None => width - 10.0,
                };
                let w = field_width(&kind).min(right - x0);
                if w >= 12.0 {
                    let mk = |x: f32, w: f32| {
                        Rect::from_min_size(
                            pos2(b.rect.left() + x * z, y - FIELD_H * z / 2.0),
                            vec2(w, FIELD_H) * z,
                        )
                    };
                    if kind == FieldKind::Vec2 {
                        let half = (w - 4.0) / 2.0;
                        for (axis, x) in [(0, x0), (1, x0 + half + 4.0)] {
                            b.fields.push(Field {
                                axis: Some(axis),
                                ..Field::new(
                                    mk(x, half),
                                    &n.id,
                                    key.clone(),
                                    kind.clone(),
                                    value.clone(),
                                )
                            });
                        }
                    } else {
                        let w = if kind == FieldKind::Bool { 12.0 } else { w };
                        b.fields
                            .push(Field::new(mk(x0, w), &n.id, key, kind, value));
                    }
                }
            }
        }
        b.rows.push(row);
    }
}

pub struct Wire {
    pub id: String,
    pub pts: [Pos2; 4],
    pub flat: Vec<Pos2>,
    pub ty: PortType,
    pub source: PinRef,
}

pub fn wire_points(a: Pos2, b: Pos2, z: f32) -> [Pos2; 4] {
    let dx = ((b.x - a.x).abs() * 0.5).max(40.0 * z);
    [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b]
}

pub fn bezier(pts: [Pos2; 4], stroke: Stroke) -> egui::epaint::CubicBezierShape {
    egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, stroke)
}

pub fn dist_to_polyline(p: Pos2, line: &[Pos2]) -> f32 {
    line.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            (a + ab * t).distance(p)
        })
        .fold(f32::INFINITY, f32::min)
}

pub fn wires(patch: &PatchFile, boxes: &IndexMap<String, NodeBox>, z: f32) -> Vec<Wire> {
    let mut out = Vec::new();
    for e in &patch.edges {
        let (Some(src), Some(dst)) = (boxes.get(&e.source), boxes.get(&e.target)) else {
            continue;
        };
        let (Some(sh @ Handle::Out(sname)), Some(th)) = (
            parse_handle(e.source_handle.as_deref()),
            parse_handle(e.target_handle.as_deref()),
        ) else {
            continue;
        };
        let (Some(a), Some(b)) = (src.pin_at(sh), dst.pin_at(th)) else {
            continue;
        };
        let ty = src.pin(Side::Out, sname).map_or(PortType::Any, |p| p.ty);
        let pts = wire_points(a, b, z);
        out.push(Wire {
            id: e.id.clone(),
            pts,
            flat: bezier(pts, Stroke::NONE).flatten(Some(0.5)),
            ty,
            source: PinRef {
                node: e.source.clone(),
                side: Side::Out,
                port: sname.to_owned(),
                ty,
            },
        });
    }
    out
}

/// Hit-testing over one frame's layout.
pub struct Hits<'a> {
    pub boxes: &'a IndexMap<String, NodeBox>,
    /// Non-frame nodes back to front.
    pub order: &'a [String],
    pub wires: &'a [Wire],
}

impl Hits<'_> {
    /// Frames, smallest (innermost) first.
    fn frames(&self) -> Vec<(&String, &NodeBox)> {
        let mut f: Vec<_> = self
            .boxes
            .iter()
            .filter(|(_, b)| b.look.is_frame())
            .collect();
        f.sort_by(|a, b| (a.1.rect.area()).total_cmp(&b.1.rect.area()));
        f
    }

    /// Topmost node body under `p` (not frames).
    pub fn body(&self, p: Pos2) -> Option<&String> {
        self.order
            .iter()
            .rev()
            .find(|id| self.boxes[id.as_str()].rect.contains(p))
    }

    /// The node a press at `p` grabs: a body, else a region's header strip.
    pub fn node(&self, p: Pos2) -> Option<String> {
        if let Some(id) = self.body(p) {
            return Some(id.clone());
        }
        self.frames()
            .into_iter()
            .find(|(_, b)| b.look == Look::Region && b.header.contains(p))
            .map(|(id, _)| id.clone())
    }

    /// Every pin with its screen position, topmost node first.
    pub fn all_pins(&self) -> Vec<(PinRef, Pos2)> {
        let mut ids: Vec<&String> = self.order.iter().rev().collect();
        ids.extend(self.frames().into_iter().map(|(id, _)| id));
        let mut out = Vec::new();
        for id in ids {
            for p in &self.boxes[id.as_str()].pins {
                out.push((
                    PinRef {
                        node: id.clone(),
                        side: p.side,
                        port: p.port.clone(),
                        ty: p.ty,
                    },
                    p.pos,
                ));
            }
        }
        out
    }

    pub fn pin(&self, p: Pos2, radius: f32, side: Option<Side>) -> Option<PinRef> {
        self.all_pins()
            .into_iter()
            .filter(|(r, _)| side.is_none_or(|s| s == r.side))
            .map(|(r, pos)| (r, pos.distance(p)))
            .filter(|(_, d)| *d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(r, _)| r)
    }

    pub fn wire(&self, p: Pos2) -> Option<&Wire> {
        self.wires
            .iter()
            .rev()
            .map(|w| (w, dist_to_polyline(p, &w.flat)))
            .filter(|(_, d)| *d <= WIRE_HIT)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(w, _)| w)
    }

    pub fn field(&self, p: Pos2) -> Option<&Field> {
        let id = self.body(p)?;
        self.boxes[id.as_str()]
            .fields
            .iter()
            .find(|f| f.rect.expand(2.0).contains(p))
    }

    /// A custom-body control under `p`.
    pub fn widget(&self, p: Pos2) -> Option<&super::custom::Widget> {
        let id = self.body(p)?;
        self.boxes[id.as_str()]
            .widgets
            .iter()
            .find(|w| w.kind.interactive() && w.rect.expand(1.0).contains(p))
    }

    /// An input label that toggles its editor: (node, port).
    pub fn toggle(&self, p: Pos2) -> Option<(String, String)> {
        let id = self.body(p)?;
        self.boxes[id.as_str()]
            .rows
            .iter()
            .find(|r| r.toggle.is_some() && r.label_rect.contains(p))
            .map(|r| (id.clone(), r.toggle.clone().expect("checked")))
    }

    /// A frame-port pill: (region, port).
    pub fn pill(&self, p: Pos2) -> Option<(String, String)> {
        if self.body(p).is_some() {
            return None;
        }
        self.frames().into_iter().find_map(|(id, b)| {
            b.pills
                .iter()
                .find(|pl| pl.rect.contains(p))
                .map(|pl| (id.clone(), pl.port.clone()))
        })
    }

    /// A frame border / corner: (region, x direction, y direction).
    pub fn resize(&self, p: Pos2) -> Option<(String, i8, i8)> {
        if self.body(p).is_some() {
            return None;
        }
        self.frames().into_iter().find_map(|(id, b)| {
            let r = b.rect;
            if !r.expand(RESIZE_BAND).contains(p) {
                return None;
            }
            let near = |v: f32, edge: f32| (v - edge).abs() <= RESIZE_BAND;
            let dx = if near(p.x, r.left()) {
                -1
            } else if near(p.x, r.right()) {
                1
            } else {
                0
            };
            let dy = if near(p.y, r.top()) {
                -1
            } else if near(p.y, r.bottom()) {
                1
            } else {
                0
            };
            (dx != 0 || dy != 0).then(|| (id.clone(), dx, dy))
        })
    }

    /// A node's warning chip.
    pub fn chip(&self, p: Pos2) -> Option<(&String, &String)> {
        self.boxes.iter().find_map(|(id, b)| {
            b.warning
                .as_ref()
                .filter(|(r, _)| r.expand(2.0).contains(p))
                .map(|(_, t)| (id, t))
        })
    }

    /// A region's iteration badge hint.
    pub fn badge(&self, p: Pos2) -> Option<&String> {
        self.boxes.values().find_map(|b| {
            b.badge
                .as_ref()
                .filter(|(r, ..)| r.contains(p))
                .map(|(_, _, hint, _)| hint)
        })
    }
}
