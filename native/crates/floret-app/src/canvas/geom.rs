//! Where everything visible sits on screen this frame — node boxes, pins,
//! inline fields, region frames and their port pills, wires — and
//! hit-testing over it.

use crate::editor::ports::{self, Side};
use crate::editor::regions::{self, PORT_TAB_H, PORT_TAB_W, TOP_PIN_W};
use crate::editor::values::{self, EditKey, FieldKind, LeftItem};
use crate::editor::widgets::{self, ProbeView};
use crate::editor::{ops, pairs};
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
pub const ROW_H: f32 = 22.0;
pub const PIN_R: f32 = 3.75;
/// Pointer distance (screen px) that counts as "on" a pin / wire.
pub const PIN_HIT: f32 = 9.0;
pub const PIN_DROP: f32 = 16.0;
pub const WIRE_HIT: f32 = 6.0;
const FIELD_H: f32 = 15.0;
const RESIZE_BAND: f32 = 5.0;
pub const LABEL_FONT: f32 = 11.0;
/// The outputs' footer: rows about 3/4 of [`ROW_H`], labels a point smaller
/// than [`LABEL_FONT`], a little padding above and below the rows, and the
/// gap between the last input row (or custom body) and the footer.
pub const FOOT_ROW_H: f32 = 16.5;
pub const FOOT_FONT: f32 = LABEL_FONT - 1.0;
pub const FOOT_PAD: f32 = 3.0;
pub const FOOT_GAP: f32 = 3.0;
/// Margin under the input rows of a node without outputs.
pub const BOTTOM_PAD: f32 = 8.0;

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

/// Rows above the outputs' footer: one per input or param (or what a
/// custom body lays out).
fn input_rows(n: &PatchNode, cat: &Catalogue, look: Look) -> usize {
    let Some(spec) = cat.get(&n.def_type) else {
        return 2;
    };
    if look == Look::Collapsed {
        return subpatch_ports(n, PortDirection::Input).count();
    }
    if let Some(rows) = widgets::custom_for(n).and_then(|c| super::custom::rows(c, n, cat)) {
        return rows;
    }
    values::left_items(cat, spec, n).len()
}

/// How many outputs the footer lists.
fn output_count(n: &PatchNode, cat: &Catalogue, look: Look) -> usize {
    let Some(spec) = cat.get(&n.def_type) else {
        return 0;
    };
    if look == Look::Collapsed {
        return subpatch_ports(n, PortDirection::Output).count();
    }
    ports::resolved_outputs(cat, spec, &n.params).len()
}

/// Height under the input rows: the outputs' footer (with its gap when
/// something sits `above` it), or a bottom margin without outputs.
pub fn footer_h(outs: usize, above: bool) -> f32 {
    if outs == 0 {
        return BOTTOM_PAD;
    }
    let gap = if above { FOOT_GAP } else { 0.0 };
    gap + 2.0 * FOOT_PAD + outs as f32 * FOOT_ROW_H
}

/// A node's size in canvas units as drawn with `look`.
pub fn box_size(patch: &PatchFile, n: &PatchNode, cat: &Catalogue, look: Look) -> (f64, f64) {
    match look {
        Look::Region | Look::Frame => ops::region_size(n),
        Look::Comment => {
            let param = |k: &str, d: f64| n.params.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
            (param("width", 200.0), param("height", 80.0))
        }
        Look::Probe => super::custom::PROBE_SIZE,
        Look::Node | Look::Collapsed => {
            let rows = input_rows(n, cat, look);
            let body = match look {
                Look::Node => widgets::custom_for(n).map_or(0.0, |c| super::custom::body_h(c, n)),
                _ => 0.0,
            };
            let foot = footer_h(output_count(n, cat, look), rows > 0 || body > 0.0);
            (
                f64::from(node_width(patch, n, cat, look)),
                f64::from(HEADER_H + body + rows as f32 * ROW_H + foot),
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
        let decor = Decor::default();
        let pins_of = |scope: Option<&str>, ids: &HashSet<String>| {
            let cx = Ctx {
                patch,
                cat,
                painter: &painter,
                origin: Pos2::ZERO,
                pan: Vec2::ZERO,
                zoom: 1.0,
                scope,
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
            crate::theme::install_fonts(&ctx);
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
pub fn node_width(patch: &PatchFile, n: &PatchNode, cat: &Catalogue, look: Look) -> f32 {
    let Some(spec) = cat.get(&n.def_type) else {
        return NODE_W;
    };
    let label_font = FontId::proportional(LABEL_FONT);
    let out_font = FontId::proportional(FOOT_FONT);
    if look == Look::Node {
        if let Some(c) = widgets::custom_for(n) {
            return custom_width(n, spec, cat, c, &out_font);
        }
    }
    let mut widest: f32 = 0.0;
    let math = values::header_symbol(n);
    let title = if look == Look::Collapsed {
        let label = n.params.get("label").and_then(|v| v.as_str());
        format!("▶ {}", label.unwrap_or(&spec.label))
    } else {
        math.map_or_else(
            || pairs::title(n, &spec.label).unwrap_or_else(|| spec.label.clone()),
            str::to_owned,
        )
    };
    let title_font = if math.is_some() {
        FontId::monospace(13.0)
    } else {
        crate::theme::medium(12.5)
    };
    let mut header = super::draw::TITLE_X + text_width(&title, title_font) + 12.0;
    if look == Look::Node && floret_engine::bypass::can_bypass(spec) {
        header += BYPASS_ROOM;
    }
    if let Some(slot) = slot_badge(n, look) {
        let text = slot;
        header += 6.0 + text_width(&text, FontId::monospace(10.0)) + 8.0;
    }
    if look == Look::Node && pairs::is_pair_type(&n.def_type) {
        // Room for the warning chip of a half without its partner.
        header += 20.0;
    }
    if look == Look::Collapsed {
        header += 8.0 + text_width("subpatch", FontId::proportional(9.5));
        for p in subpatch_ports(n, PortDirection::Input) {
            let left = text_width(&p.name, label_font.clone());
            widest = widest.max(row_need(10.0, left, None));
        }
        for p in subpatch_ports(n, PortDirection::Output) {
            let spread = passthrough_spread(patch, cat, &n.id, &p.id);
            widest = widest.max(foot_need(out_w(&p.name, spread, &out_font)));
        }
    } else {
        let wired = wired_inputs(patch, &n.id);
        let items = values::left_items(cat, spec, n);
        for p in ports::resolved_outputs(cat, spec, &n.params) {
            let label = values::display_label(&p, false);
            let spread = p.kind == PortKind::Spread;
            widest = widest.max(foot_need(out_w(&label, spread, &out_font)));
        }
        for item in &items {
            match item {
                LeftItem::Param { key, kind } => {
                    let v = values::current(n, spec, &EditKey::Param(key.clone()), None);
                    let w = box_width(key, (false, false), kind, &v);
                    widest = widest.max(row_need(BOX_X, w, None));
                }
                LeftItem::Input(port) => {
                    let label = values::display_label(port, true);
                    let browse = if values::browse_filter(&n.def_type, &port.name).is_some() {
                        BROWSE_W + 4.0
                    } else {
                        0.0
                    };
                    let wired = wired.contains(port.name.as_str());
                    let spread = port.kind == PortKind::Spread;
                    match values::input_editor(spec, port, wired) {
                        Some((key, kind)) => {
                            let v = values::current(n, spec, &key, Some(port));
                            let w = box_width(&label, (port.constant, spread), &kind, &v);
                            // The `…` button keeps 10 from the right edge.
                            let w = if browse > 0.0 {
                                w + browse + 10.0 - BOX_X
                            } else {
                                w
                            };
                            widest = widest.max(row_need(BOX_X, w, None));
                        }
                        None => {
                            let integer = port.integer && port.ty == PortType::Number;
                            let w = text_width(&label, label_font.clone())
                                + marks_w(port.constant, integer, spread);
                            widest = widest.max(row_need(10.0, w + browse, None));
                        }
                    }
                }
            }
        }
    }
    let min = if n.def_type.starts_with("math/") {
        MATH_MIN_W
    } else {
        MIN_W
    };
    widest.max(header).clamp(min, MAX_W).ceil()
}

/// A custom-body node is [`NODE_W`] wide, or wider for a long output
/// name. One whose body lays out the standard rows (e.g. a Palette in a
/// generating mode) grows when one of its dropdown rows needs more room,
/// so the dropdown fits its longest option.
fn custom_width(
    n: &PatchNode,
    spec: &floret_engine::spec::NodeSpec,
    cat: &Catalogue,
    c: widgets::Custom,
    out_font: &FontId,
) -> f32 {
    let outs = ports::resolved_outputs(cat, spec, &n.params)
        .iter()
        .map(|p| {
            let label = values::display_label(p, false);
            foot_need(out_w(&label, p.kind == PortKind::Spread, out_font))
        })
        .fold(NODE_W, f32::max);
    if super::custom::rows(c, n, cat).is_some() {
        return outs.min(MAX_W).ceil();
    }
    let items = values::left_items(cat, spec, n);
    let widest = items
        .iter()
        .filter_map(|item| match item {
            LeftItem::Param {
                key,
                kind: kind @ FieldKind::Select(_),
            } => {
                let v = values::current(n, spec, &EditKey::Param(key.clone()), None);
                Some(row_need(
                    BOX_X,
                    box_width(key, (false, false), kind, &v),
                    None,
                ))
            }
            _ => None,
        })
        .fold(outs, f32::max);
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
    /// Where the value is drawn and typed into: right of a value box's
    /// name, else the whole field.
    pub value_rect: Rect,
    /// A row's value box: the row's name drawn inside it.
    pub label: Option<FieldLabel>,
}

/// A value box's name (Blender style: one rounded box per row, the name at
/// its left, the value right-aligned).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldLabel {
    pub text: String,
    /// The constant `C` mark after the name.
    pub constant: bool,
    /// The spread icon after the name (and `C`), in this type's colour.
    pub spread: Option<PortType>,
    /// The whole box (a vec2's X and Y cells sit inside it).
    pub frame: Rect,
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
            value_rect: rect,
            label: None,
        }
    }
}

/// An input row (inputs and params get rows of their own; outputs are
/// listed in the footer under them).
pub struct Row {
    pub y: f32,
    /// A plain label (wired inputs, inputs with no editor); a row with a
    /// value box has its name inside the box instead.
    pub label: Option<String>,
    pub label_rect: Rect,
    pub constant: bool,
    pub integer: bool,
    /// The spread icon after the name, in this pin type's colour.
    pub spread: Option<PortType>,
}

/// An output's row in the footer: its label, right-aligned against its
/// pin on the node's right edge.
pub struct OutRow {
    pub y: f32,
    pub label: String,
    /// The spread icon between the label and the pin, in this type's colour.
    pub spread: Option<PortType>,
}

pub struct PillGeom {
    pub rect: Rect,
    pub port: String,
    pub name: String,
    pub ty: PortType,
    /// The spread icon after the name: a side of the port carries a spread.
    pub spread: bool,
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
    /// Browser group: picks the accent colour.
    pub group: String,
    /// Drag handle: the header row, or the strip above a region frame.
    pub header: Rect,
    pub pins: Vec<PinGeom>,
    pub rows: Vec<Row>,
    /// Where the outputs' footer starts (screen y; `None` without outputs).
    pub footer: Option<f32>,
    pub outs: Vec<OutRow>,
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
    /// Whether the node is bypassed; `None` for nodes that can't be.
    pub bypass: Option<bool>,
    /// Where the spread icons are drawn (screen space), for their tooltip.
    pub marks: Vec<Rect>,
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

/// The `#slot` badge of an unnamed Read / Write half (a named pair shows
/// its name in the title instead).
fn slot_badge(n: &PatchNode, look: Look) -> Option<String> {
    let named = pairs::is_pair_type(&n.def_type) && pairs::name(n).is_some();
    n.params
        .get("slot")
        .filter(|_| look == Look::Node && !named)
        .map(|v| format!("#{}", values::short(v)))
}

/// Per-node decorations decided outside layout.
#[derive(Default)]
pub struct Decor {
    pub warnings: HashMap<String, String>,
    /// Region id → (badge text, hover hint, capped).
    pub badges: HashMap<String, (String, String, bool)>,
    /// Probe id → what it reads.
    pub probes: HashMap<String, ProbeView>,
    /// Curve-editor node → its channel tab (absent: RGB).
    pub curve_channels: HashMap<String, u8>,
}

pub struct Ctx<'a> {
    pub patch: &'a PatchFile,
    pub cat: &'a Catalogue,
    pub painter: &'a Painter,
    pub origin: Pos2,
    pub pan: Vec2,
    pub zoom: f32,
    pub scope: Option<&'a str>,
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

/// Value boxes, in canvas units: the box's left inset in its row, height,
/// inner padding and the least gap between its name and its value.
pub const BOX_X: f32 = 8.0;
pub const BOX_H: f32 = 18.0;
pub const BOX_PAD: f32 = 6.0;
pub const BOX_GAP: f32 = 6.0;
/// The constant `C` mark after a name.
pub const MARK_W: f32 = 10.0;
/// The marks after a pin's name: the gap before the first, how far the `C`
/// and the integer `⌊⌋` advance, and the spread icon's width at
/// [`LABEL_FONT`] (it scales with its label's font).
pub const MARK_GAP: f32 = 3.0;
pub const C_ADV: f32 = 9.0;
pub const INT_ADV: f32 = 13.0;
pub const SPREAD_ICON_W: f32 = 7.0;
/// Room a footer label leaves for the spread icon before its pin.
pub const SPREAD_W: f32 = MARK_GAP + SPREAD_ICON_W;

/// The spread icon's width next to a label in `font_size`.
pub fn spread_icon_w(font_size: f32) -> f32 {
    SPREAD_ICON_W * font_size / LABEL_FONT
}

/// Where the spread icon starts after a name: past the gap, `C` and `⌊⌋`.
pub fn spread_x(constant: bool, integer: bool) -> f32 {
    MARK_GAP + if constant { C_ADV } else { 0.0 } + if integer { INT_ADV } else { 0.0 }
}

/// Room the marks after a pin's name take. Without a spread icon this is
/// the `C` alone, as before (`⌊⌋` gets room only with an icon after it).
pub fn marks_w(constant: bool, integer: bool, spread: bool) -> f32 {
    match (spread, constant) {
        (true, _) => spread_x(constant, integer) + SPREAD_ICON_W,
        (false, true) => MARK_W,
        (false, false) => 0.0,
    }
}

/// A footer label's width, with its spread icon.
fn out_w(label: &str, spread: bool, font: &FontId) -> f32 {
    text_width(label, font.clone()) + if spread { SPREAD_W } else { 0.0 }
}

/// A spread icon's screen rect (with a little slack for the pointer)
/// starting at `x`, centred on `y`.
fn mark_rect(x: f32, y: f32, font_size: f32, z: f32) -> Rect {
    Rect::from_min_size(
        pos2(x, y - font_size * 0.4 * z),
        vec2(spread_icon_w(font_size), font_size * 0.8) * z,
    )
    .expand(2.0)
}

/// Whether a pass-through frame port (a subpatch's or an If's) carries a
/// spread: what feeds it does. An output port is fed from inside, an input
/// from outside; a feeding subpatch / If is followed through.
pub fn passthrough_spread(patch: &PatchFile, cat: &Catalogue, frame: &str, port: &str) -> bool {
    fn go(patch: &PatchFile, cat: &Catalogue, frame: &str, port: &str, depth: u8) -> bool {
        if depth > 16 {
            return false;
        }
        let handle = format!("in-{port}");
        patch
            .edges
            .iter()
            .filter(|e| e.target == frame && e.target_handle.as_deref() == Some(handle.as_str()))
            .any(|e| {
                let Some((Side::Out, name)) = ports::split_handle(e.source_handle.as_deref())
                else {
                    return false;
                };
                let kind = ports::find_node(patch, &e.source)
                    .and_then(|n| cat.get(&n.def_type))
                    .and_then(|s| s.region_kind);
                if matches!(kind, Some(RegionKind::Subpatch | RegionKind::If)) {
                    go(patch, cat, &e.source, name, depth + 1)
                } else {
                    ports::resolve_port(patch, cat, &e.source, Side::Out, name)
                        .is_some_and(|p| p.kind == PortKind::Spread)
                }
            })
    }
    go(patch, cat, frame, port, 0)
}

/// A checkbox's side, and the gap before its name.
pub const CHECK: f32 = 12.0;
pub const CHECK_GAP: f32 = 6.0;
/// Room a dropdown keeps right of its option text for the ▾.
pub const SELECT_ARROW: f32 = 12.0;
/// The widest a dropdown's option text gets room for (a longer one, e.g.
/// a font name, is cut with "…").
pub const SELECT_MAX: f32 = 100.0;
/// A colour box's swatch and the gap before its hex.
pub const SWATCH_W: f32 = 10.0;
/// The gap between a vec2 box's X and Y cells, and a cell's inner padding.
pub const CELL_GAP: f32 = 3.0;
const CELL_PAD: f32 = 3.0;
/// Size of a value box's value text.
pub const VALUE_FONT: f32 = 10.0;

/// A row's width: `lead` to its content, `left` wide, then the output
/// label (10 from the right edge, 14 from the content) or `lead` again.
fn row_need(lead: f32, left: f32, out: Option<f32>) -> f32 {
    lead + left + out.map_or(lead, |o| 14.0 + o + 10.0)
}

/// A footer row's width for an output label `out` wide.
fn foot_need(out: f32) -> f32 {
    row_need(10.0, 0.0, Some(out))
}

/// What a value box shows (a vec2's cells show [`values::num`] of each
/// axis).
pub fn value_text(kind: &FieldKind, value: &Json) -> String {
    match kind {
        _ if value.is_null() && !matches!(kind, FieldKind::Color) => "—".into(),
        FieldKind::Number { .. } => values::num(values::as_f64(value), 3),
        FieldKind::Color => values::hex(values::as_rgba(value)),
        FieldKind::Select(_) => values::option_label(&values::as_text(value)).to_owned(),
        FieldKind::Vec2 => {
            let (x, y) = values::as_vec2(value);
            format!("{}, {}", values::num(x, 3), values::num(y, 3))
        }
        FieldKind::Bool | FieldKind::Text => values::as_text(value),
    }
}

/// Width of a vec2 box's X or Y cell.
fn cell_width(value: &Json) -> f32 {
    let (x, y) = values::as_vec2(value);
    let font = || FontId::monospace(VALUE_FONT);
    text_width(&values::num(x, 3), font())
        .max(text_width(&values::num(y, 3), font()))
        .max(text_width("00", font()))
        + 2.0 * CELL_PAD
}

/// Room a value box's value needs: what it shows, and at least a short
/// number's worth so a scrub doesn't keep resizing the node. A dropdown
/// fits its longest option.
fn value_width(kind: &FieldKind, value: &Json) -> f32 {
    let mono = || FontId::monospace(VALUE_FONT);
    match kind {
        FieldKind::Number { .. } => {
            text_width(&value_text(kind, value), mono()).max(text_width("0.000", mono()))
        }
        FieldKind::Vec2 => 2.0 * cell_width(value) + CELL_GAP,
        FieldKind::Bool => 0.0,
        FieldKind::Text => text_width(&values::short(value), mono()).clamp(24.0, 100.0),
        FieldKind::Color => SWATCH_W + 4.0 + text_width(&value_text(kind, value), mono()),
        FieldKind::Select(opts) => {
            opts.iter()
                .map(|o| text_width(values::option_label(o), mono()))
                .fold(text_width(&value_text(kind, value), mono()), f32::max)
                .min(SELECT_MAX)
                + 4.0
                + SELECT_ARROW
        }
    }
}

/// Width of a value box named `label` (a checkbox and its name for a
/// bool). `(constant, spread)`: the marks after its name.
fn box_width(label: &str, (constant, spread): (bool, bool), kind: &FieldKind, value: &Json) -> f32 {
    let name =
        text_width(label, FontId::proportional(LABEL_FONT)) + marks_w(constant, false, spread);
    match kind {
        FieldKind::Bool => CHECK + CHECK_GAP + name,
        _ => BOX_PAD + name + BOX_GAP + value_width(kind, value) + BOX_PAD,
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
        let (w, h) = box_size(cx.patch, n, cx.cat, look);
        let p = ops::absolute_pos(cx.patch, &n.id);
        let z = cx.zoom;
        let rect = Rect::from_min_size(cx.screen(p.x, p.y), vec2(w as f32, h as f32) * z);
        let label = n
            .params
            .get("label")
            .and_then(|v| v.as_str())
            .filter(|_| look.is_frame() || look == Look::Collapsed)
            .map(str::to_owned)
            .or_else(|| values::header_symbol(n).map(str::to_owned))
            .or_else(|| pairs::title(n, &spec.label).filter(|_| look == Look::Node))
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
            math: look == Look::Node && values::header_symbol(n).is_some(),
            label,
            group: spec.group.clone(),
            header,
            pins: Vec::new(),
            rows: Vec::new(),
            footer: None,
            outs: Vec::new(),
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
            slot: slot_badge(n, look),
            warning: None,
            badge: None,
            body: 0.0,
            widgets: Vec::new(),
            header_value: None,
            notes: Vec::new(),
            probe: None,
            bypass: None,
            marks: Vec::new(),
        };
        match look {
            Look::Region | Look::Frame => frame_box(cx, n, spec, (w, h), &mut b),
            Look::Collapsed => collapsed_box(cx, n, &mut b),
            Look::Node => match widgets::custom_for(n) {
                Some(c) => super::custom::layout(cx, n, spec, c, &wired, &mut b),
                None => row_box(cx, n, spec, &wired, &mut b),
            },
            Look::Probe => super::custom::probe_box(cx, n, &mut b),
            Look::Comment => super::custom::comment_box(cx, n, &mut b),
        }
        if matches!(look, Look::Node | Look::Collapsed) {
            footer(cx, n, spec, &mut b);
        }
        if look == Look::Node && floret_engine::bypass::can_bypass(spec) {
            b.bypass = Some(n.bypassed);
            b.widgets.push(super::custom::Widget {
                rect: bypass_rect(b.header, z),
                node: n.id.clone(),
                kind: super::custom::WidgetKind::Bypass(n.bypassed),
            });
        }
        chips(cx, n, &mut b);
        boxes.insert(n.id.clone(), b);
    }
    boxes
}

/// Width of a row's `…` file button (canvas units).
pub const BROWSE_W: f32 = 18.0;

/// Header room the bypass button takes at the right end (canvas units).
pub const BYPASS_ROOM: f32 = 18.0;

/// The bypass button in a node header (screen space).
pub fn bypass_rect(header: Rect, z: f32) -> Rect {
    Rect::from_center_size(
        header.right_center() - vec2(13.0 * z, 0.0),
        vec2(16.0, 16.0) * z,
    )
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
        // Left of the bypass button, when there is one.
        let inset = if b.bypass.is_some() {
            18.0 + BYPASS_ROOM
        } else {
            18.0
        };
        let r = Rect::from_center_size(b.header.right_center() - vec2(inset * z, 0.0), size);
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
    let z = cx.zoom;
    // A pass-through frame's (If, subpatch) output port carries a spread
    // when what feeds it inside does: both its pins, and its pill.
    let passes = |id: &str| {
        !gathers
            && n.ports
                .iter()
                .flatten()
                .any(|rp| rp.id == id && rp.direction == PortDirection::Output)
            && passthrough_spread(cx.patch, cx.cat, &n.id, id)
    };
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
            spread: (gathers && outer) || p.pill.as_deref().is_some_and(passes),
            constant: false,
        });
    }
    let half = PORT_TAB_H / 2.0;
    for p in &fl.pills {
        let rect = Rect::from_min_size(
            at(p.x, p.y - half),
            vec2(PORT_TAB_W as f32, PORT_TAB_H as f32) * z,
        );
        // Every ForEach / Cache port has a spread side (its outer pin).
        let spread = gathers || passes(&p.port);
        if spread && z > 0.3 {
            let x = pill_mark_x(rect, cx.text_w(&p.name, PILL_FONT) * z, z);
            b.marks.push(mark_rect(x, rect.center().y, PILL_FONT, z));
        }
        b.pills.push(PillGeom {
            rect,
            port: p.port.clone(),
            name: p.name.clone(),
            ty: p.ty,
            spread,
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

/// A frame pill's name size.
pub const PILL_FONT: f32 = 9.0;

/// Where a pill's spread icon starts (screen x): its name (`name_w` wide
/// on screen) and the icon are centred in the pill together.
pub fn pill_mark_x(pill: Rect, name_w: f32, z: f32) -> f32 {
    let group = name_w + (MARK_GAP + spread_icon_w(PILL_FONT)) * z;
    pill.center().x - group / 2.0 + name_w + MARK_GAP * z
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
    for (i, p) in subpatch_ports(n, PortDirection::Input).enumerate() {
        let y = row_y(b, i, z);
        b.pins.push(PinGeom {
            side: Side::In,
            port: p.id.clone(),
            label: p.name.clone(),
            ty: PortType::parse(&p.ty).unwrap_or(PortType::Any),
            pos: pos2(b.rect.left(), y),
            spread: false,
            constant: false,
        });
        b.rows.push(Row {
            y,
            label: Some(p.name.clone()),
            label_rect: Rect::NOTHING,
            constant: false,
            integer: false,
            spread: None,
        });
    }
}

/// The outputs' footer at the bottom of a node or collapsed subpatch: one
/// compact row per output, its label right-aligned against its pin.
fn footer(cx: &Ctx, n: &PatchNode, spec: &floret_engine::spec::NodeSpec, b: &mut NodeBox) {
    let z = cx.zoom;
    // (label, port, type, spread)
    let outs: Vec<(String, String, PortType, bool)> = if b.look == Look::Collapsed {
        subpatch_ports(n, PortDirection::Output)
            .map(|p| {
                let ty = PortType::parse(&p.ty).unwrap_or(PortType::Any);
                let spread = passthrough_spread(cx.patch, cx.cat, &n.id, &p.id);
                (p.name.clone(), p.id.clone(), ty, spread)
            })
            .collect()
    } else {
        ports::resolved_outputs(cx.cat, spec, &n.params)
            .iter()
            .map(|p| {
                let spread = p.kind == PortKind::Spread;
                (
                    values::display_label(p, false),
                    p.name.clone(),
                    p.ty,
                    spread,
                )
            })
            .collect()
    };
    if outs.is_empty() {
        return;
    }
    // The footer fills the bottom of the box (see `box_size`).
    let top = b.rect.bottom() - (2.0 * FOOT_PAD + outs.len() as f32 * FOOT_ROW_H) * z;
    b.footer = Some(top);
    for (i, (label, port, ty, spread)) in outs.into_iter().enumerate() {
        let y = top + (FOOT_PAD + FOOT_ROW_H * (i as f32 + 0.5)) * z;
        b.pins.push(PinGeom {
            side: Side::Out,
            port,
            label: label.clone(),
            ty,
            pos: pos2(b.rect.right(), y),
            spread,
            constant: false,
        });
        if spread && z > 0.3 {
            let x = b.rect.right() - (10.0 + spread_icon_w(FOOT_FONT)) * z;
            b.marks.push(mark_rect(x, y, FOOT_FONT, z));
        }
        b.outs.push(OutRow {
            y,
            label,
            spread: spread.then_some(ty),
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
    for (i, item) in items.iter().enumerate() {
        let y = row_y(b, i, z);
        let mut row = Row {
            y,
            label: None,
            label_rect: Rect::NOTHING,
            constant: false,
            integer: false,
            spread: None,
        };
        // (name, constant, key, kind, value) of this row's value box.
        let mut boxed: Option<(String, bool, EditKey, FieldKind, Json)> = None;
        // The type of a spread input (its icon's colour).
        let mut spread = None;
        // A `…` file button sits at the row's right end.
        let mut browse = false;
        match item {
            LeftItem::Param { key, kind } => {
                let k = EditKey::Param(key.clone());
                let v = values::current(n, spec, &k, None);
                boxed = Some((key.clone(), false, k, kind.clone(), v));
            }
            LeftItem::Input(port) => {
                let is_wired = wired.contains(&(n.id.as_str(), port.name.as_str()));
                let label = values::display_label(port, true);
                spread = (port.kind == PortKind::Spread).then_some(port.ty);
                b.pins.push(PinGeom {
                    side: Side::In,
                    port: port.name.clone(),
                    label: label.clone(),
                    ty: port.ty,
                    pos: pos2(b.rect.left(), y),
                    spread: spread.is_some(),
                    constant: port.constant,
                });
                if !is_wired && values::browse_filter(&n.def_type, &port.name).is_some() {
                    browse = true;
                    b.widgets.push(super::custom::Widget {
                        rect: Rect::from_center_size(
                            pos2(b.rect.right() - (10.0 + BROWSE_W / 2.0) * z, y),
                            vec2(BROWSE_W, FIELD_H) * z,
                        ),
                        node: n.id.clone(),
                        kind: super::custom::WidgetKind::Browse(port.name.clone()),
                    });
                }
                match values::input_editor(spec, port, is_wired) {
                    // The live value: the param for param-backed pins
                    // (Renderer width / height), else portValues or the
                    // declared default.
                    Some((key, kind)) => {
                        let v = values::current(n, spec, &key, Some(port));
                        boxed = Some((label, port.constant, key, kind, v));
                    }
                    // Wired, or no editor for its type: just the name.
                    None => {
                        row.label = Some(label);
                        row.constant = port.constant;
                        row.integer = port.integer && port.ty == PortType::Number;
                        row.spread = spread;
                    }
                }
            }
        }
        if let Some(label) = &row.label {
            let text = cx.text_w(label, LABEL_FONT);
            let lw = text + marks_w(row.constant, row.integer, row.spread.is_some());
            row.label_rect = Rect::from_min_size(
                pos2(b.rect.left() + 8.0 * z, y - ROW_H * z / 2.0),
                vec2(lw + 4.0, ROW_H) * z,
            );
            if row.spread.is_some() && z > 0.3 {
                let x = 10.0 + text + spread_x(row.constant, row.integer);
                b.marks
                    .push(mark_rect(b.rect.left() + x * z, y, LABEL_FONT, z));
            }
        }
        if let Some((label, constant, key, kind, value)) = boxed {
            // The box runs to the row's `…` button or the node's edge.
            let width = b.rect.width() / z;
            let right = if browse {
                width - 10.0 - BROWSE_W - 4.0
            } else {
                width - BOX_X
            };
            if right - BOX_X >= 12.0 {
                let span = |x0: f32, x1: f32, h: f32| {
                    Rect::from_min_max(
                        pos2(b.rect.left() + x0 * z, y - h * z / 2.0),
                        pos2(b.rect.left() + x1 * z, y + h * z / 2.0),
                    )
                };
                let text = cx.text_w(&label, LABEL_FONT);
                let name = text + marks_w(constant, false, spread.is_some());
                let frame = span(BOX_X, right, BOX_H);
                if spread.is_some() && z > 0.3 {
                    // The name starts past the checkbox, or the box's padding.
                    let start = if kind == FieldKind::Bool {
                        BOX_X + CHECK + CHECK_GAP
                    } else {
                        BOX_X + BOX_PAD
                    };
                    let x = start + text + spread_x(constant, false);
                    b.marks
                        .push(mark_rect(b.rect.left() + x * z, y, LABEL_FONT, z));
                }
                let tag = FieldLabel {
                    text: label,
                    constant,
                    spread,
                    frame,
                };
                // Where the value part starts: past the name.
                let vx = (BOX_X + BOX_PAD + name + BOX_GAP).min(right - BOX_PAD);
                match kind {
                    FieldKind::Bool => {
                        // A checkbox, then its name: clicking either toggles.
                        let r = span(BOX_X, (BOX_X + CHECK + CHECK_GAP + name).min(right), BOX_H);
                        b.fields.push(Field {
                            value_rect: r,
                            label: Some(FieldLabel { frame: r, ..tag }),
                            ..Field::new(r, &n.id, key, kind, value)
                        });
                    }
                    FieldKind::Vec2 => {
                        // X and Y cells right of the name, each its own field.
                        let end = right - CELL_PAD / 2.0;
                        let cell = ((end - vx - CELL_GAP) / 2.0).max(8.0);
                        for axis in 0..2 {
                            let x1 = end - (1 - axis) as f32 * (cell + CELL_GAP);
                            let r = span(x1 - cell, x1, BOX_H - 4.0);
                            b.fields.push(Field {
                                axis: Some(axis),
                                value_rect: r,
                                label: Some(tag.clone()),
                                ..Field::new(r, &n.id, key.clone(), kind.clone(), value.clone())
                            });
                        }
                    }
                    _ => {
                        b.fields.push(Field {
                            value_rect: span(vx, right - BOX_PAD, BOX_H),
                            label: Some(tag),
                            ..Field::new(frame, &n.id, key, kind, value)
                        });
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

    /// Whether `p` is on a spread icon (for its "spread" tooltip).
    pub fn spread_mark(&self, p: Pos2) -> bool {
        let on = |b: &NodeBox| b.marks.iter().any(|r| r.contains(p));
        match self.body(p) {
            // Only the topmost node's own icons, not one it covers.
            Some(id) => on(&self.boxes[id.as_str()]),
            None => self.frames().into_iter().any(|(_, b)| on(b)),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Node `n` of type `ty` laid out at zoom 1, with `wired` inputs fed
    /// by a Number node.
    fn laid_out(ty: &str, wired: &[&str]) -> NodeBox {
        super::super::samples::real_fonts();
        let cat = Catalogue::builtin();
        let edges: Vec<Json> = wired
            .iter()
            .map(|p| {
                json!({"id": format!("e_{p}"), "source": "src", "sourceHandle": "out-value",
                       "target": "n", "targetHandle": format!("in-{p}")})
            })
            .collect();
        let flora = json!({
            "kind": "floret.patch", "version": 2, "edges": edges,
            "nodes": [
                {"id": "src", "defType": "value/number", "position": {"x": -300, "y": 0}},
                {"id": "n", "defType": ty, "position": {"x": 0, "y": 0}},
            ],
        });
        let mut patch = floret_engine::load_patch(&flora.to_string()).expect("loads");
        crate::editor::settle(&mut patch, cat);
        let painter = measuring_painter();
        let decor = Decor::default();
        let cx = Ctx {
            patch: &patch,
            cat,
            painter: &painter,
            origin: Pos2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0,
            scope: None,
            decor: &decor,
        };
        let all: HashSet<String> = ["n".to_owned()].into();
        layout(&cx, &all, &|_| false)
            .swap_remove("n")
            .expect("laid out")
    }

    fn fields_of<'a>(b: &'a NodeBox, port: &str) -> Vec<&'a Field> {
        b.fields
            .iter()
            .filter(|f| match &f.key {
                EditKey::Param(k) | EditKey::Port(k) => k == port,
            })
            .collect()
    }

    fn row_named<'a>(b: &'a NodeBox, name: &str) -> Option<&'a Row> {
        b.rows.iter().find(|r| r.label.as_deref() == Some(name))
    }

    #[test]
    fn unwired_inputs_are_value_boxes_with_their_name_inside() {
        let b = laid_out("draw/rect", &[]);
        let w = fields_of(&b, "w");
        assert_eq!(w.len(), 1);
        let f = w[0];
        assert!(matches!(f.kind, FieldKind::Number { .. }));
        let tag = f.label.as_ref().expect("an inline name");
        assert_eq!(tag.text, "w");
        // The whole box scrubs; the value sits right of the name, inside it.
        assert_eq!(f.rect, tag.frame);
        assert!(f.value_rect.left() > tag.frame.left() + BOX_PAD);
        assert!(f.value_rect.right() <= tag.frame.right());
        assert!((f.rect.left() - (b.rect.left() + BOX_X)).abs() < 0.01);
        // No separate label or read-out on the row.
        assert!(row_named(&b, "w").is_none());
    }

    #[test]
    fn a_wired_input_shows_just_its_name() {
        let b = laid_out("draw/rect", &["w"]);
        assert!(fields_of(&b, "w").is_empty());
        assert!(row_named(&b, "w").is_some());
        // The other inputs keep their boxes.
        assert_eq!(fields_of(&b, "h").len(), 1);
    }

    #[test]
    fn each_kind_gets_its_value_box() {
        let b = laid_out("collider/circle", &[]);
        let one = |port: &str| {
            let f = fields_of(&b, port);
            assert_eq!(f.len(), 1, "{port}");
            f[0].clone()
        };
        assert!(matches!(one("mode").kind, FieldKind::Select(_)));
        let inverted = one("inverted");
        assert_eq!(inverted.kind, FieldKind::Bool);
        // A checkbox and its name: one hit area.
        let tag = inverted.label.as_ref().expect("named");
        assert!(tag.constant);
        assert!(inverted.rect.width() >= CHECK + CHECK_GAP);
        // Vec2: two cells side by side in one box with the name.
        let center = fields_of(&b, "center");
        assert_eq!(center.len(), 2);
        assert_eq!(center[0].axis, Some(0));
        assert_eq!(center[1].axis, Some(1));
        assert!(center[0].rect.right() < center[1].rect.left());
        let frame = center[0].label.as_ref().expect("named").frame;
        assert!(frame.contains_rect(center[0].rect) && frame.contains_rect(center[1].rect));
        assert_eq!(center[0].label.as_ref().unwrap().text, "center");
        let fill = laid_out("style/fill", &[]);
        assert_eq!(fields_of(&fill, "color")[0].kind, FieldKind::Color);
    }

    fn out_pins(b: &NodeBox) -> Vec<&PinGeom> {
        b.pins.iter().filter(|p| p.side == Side::Out).collect()
    }

    /// Height of a plain node: header, a row per input, then the footer.
    fn expected_h(b: &NodeBox) -> f32 {
        let outs = b.outs.len();
        let foot = if outs == 0 {
            BOTTOM_PAD
        } else {
            let gap = if b.rows.is_empty() { 0.0 } else { FOOT_GAP };
            gap + 2.0 * FOOT_PAD + outs as f32 * FOOT_ROW_H
        };
        HEADER_H + b.body + b.rows.len() as f32 * ROW_H + foot
    }

    #[test]
    fn a_single_output_sits_in_a_one_row_footer() {
        let b = laid_out("draw/rect", &[]);
        let outs = out_pins(&b);
        assert_eq!(outs.len(), 1);
        assert_eq!(b.outs.len(), 1);
        assert_eq!(outs[0].port, "shape");
        let top = b.footer.expect("a footer");
        let last = b.rows.iter().map(|r| r.y).fold(f32::MIN, f32::max);
        assert!(top >= last + ROW_H / 2.0, "footer under the inputs");
        let pin = outs[0].pos;
        assert_eq!(pin.x, b.rect.right());
        assert!((pin.y - (top + FOOT_PAD + FOOT_ROW_H / 2.0)).abs() < 0.01);
        assert!((pin.y - b.outs[0].y).abs() < 0.01);
        // Nothing on the title row: the header has no pins.
        assert!(b.pins.iter().all(|p| p.pos.y > b.header.bottom()));
        // Every value box runs the node's full inner width.
        let right = b.rect.right() - BOX_X;
        for f in b.fields.iter().filter(|f| f.kind != FieldKind::Bool) {
            let frame = f.label.as_ref().expect("a value box").frame;
            assert!((frame.right() - right).abs() < 0.01, "{:?}", f.key);
            assert!((frame.left() - (b.rect.left() + BOX_X)).abs() < 0.01);
        }
        assert!((b.rect.height() - expected_h(&b)).abs() < 0.01);
    }

    #[test]
    fn several_outputs_are_listed_under_the_inputs() {
        let b = laid_out("geometry/layout", &[]);
        assert_eq!(b.outs.len(), 4);
        let outs = out_pins(&b);
        assert_eq!(outs.len(), 4);
        let last = b.rows.iter().map(|r| r.y).fold(f32::MIN, f32::max);
        let top = b.footer.expect("a footer");
        assert!(top >= last + ROW_H / 2.0);
        for (i, o) in b.outs.iter().enumerate() {
            let want = top + FOOT_PAD + FOOT_ROW_H * (i as f32 + 0.5);
            assert!((o.y - want).abs() < 0.01, "{}", o.label);
            let pin = outs[i];
            assert_eq!(pin.label, o.label);
            assert_eq!(pin.pos, pos2(b.rect.right(), o.y));
        }
        for w in b.outs.windows(2) {
            assert!((w[1].y - w[0].y - FOOT_ROW_H).abs() < 0.01);
        }
        // The footer ends at the bottom edge, its padding below the last row.
        let end = b.outs.last().unwrap().y + FOOT_ROW_H / 2.0 + FOOT_PAD;
        assert!((end - b.rect.bottom()).abs() < 0.01);
        assert!((b.rect.height() - expected_h(&b)).abs() < 0.01);
    }

    #[test]
    fn a_node_without_outputs_has_no_footer() {
        let b = laid_out("sound/output", &[]);
        assert!(b.footer.is_none());
        assert!(b.outs.is_empty());
        assert!(out_pins(&b).is_empty());
        assert!((b.rect.height() - expected_h(&b)).abs() < 0.01);
    }

    #[test]
    fn custom_bodies_list_their_outputs_in_the_footer_too() {
        for ty in [
            "io/slider",
            "io/bang",
            "io/xy-pad",
            "io/select",
            "color/palette",
        ] {
            let b = laid_out(ty, &[]);
            let top = b.footer.unwrap_or_else(|| panic!("{ty}: no footer"));
            assert!(
                top >= b.rect.top() + (HEADER_H + b.body) * 1.0 - 0.01,
                "{ty}"
            );
            for p in out_pins(&b) {
                assert!(p.pos.y > top, "{ty}: {} above the footer", p.port);
            }
            for r in &b.rows {
                assert!(r.y + ROW_H / 2.0 <= top + 0.01, "{ty}: a row in the footer");
            }
            let controls = b.widgets.iter().map(|w| w.rect);
            for r in controls.chain(b.fields.iter().map(|f| f.rect)) {
                assert!(r.bottom() <= top + 0.01, "{ty}: a control in the footer");
            }
        }
    }

    #[test]
    fn nodes_stay_about_as_wide_as_with_the_read_outs() {
        // Widths with the old name + read-out rows.
        for (ty, before) in [
            ("draw/rect", 109.0),
            ("collider/circle", 136.0),
            ("style/fill", 134.0),
            ("draw/text", 125.0),
        ] {
            let w = laid_out(ty, &[]).rect.width();
            assert!((w - before).abs() <= 30.0, "{ty}: {before} -> {w}");
        }
    }

    /// Every node of `nodes` / `edges` laid out at `zoom` in `scope`.
    fn laid_out_patch(
        nodes: Json,
        edges: Json,
        scope: Option<&str>,
        zoom: f32,
    ) -> IndexMap<String, NodeBox> {
        super::super::samples::real_fonts();
        let cat = Catalogue::builtin();
        let flora = json!({"kind": "floret.patch", "version": 4, "nodes": nodes, "edges": edges});
        let patch = floret_engine::load_patch(&flora.to_string()).expect("loads");
        let painter = measuring_painter();
        let decor = Decor::default();
        let cx = Ctx {
            patch: &patch,
            cat,
            painter: &painter,
            origin: Pos2::ZERO,
            pan: Vec2::ZERO,
            zoom,
            scope,
            decor: &decor,
        };
        let all: HashSet<String> = patch.nodes.iter().map(|n| n.id.clone()).collect();
        layout(&cx, &all, &|_| false)
    }

    fn label_w(s: &str, size: f32) -> f32 {
        measuring_painter()
            .layout_no_wrap(s.to_owned(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    }

    #[test]
    fn spread_outputs_carry_the_icon_between_label_and_pin() {
        let b = laid_out("geometry/layout", &[]);
        let spread: Vec<&str> = out_pins(&b)
            .iter()
            .filter(|p| p.spread)
            .map(|p| p.port.as_str())
            .collect();
        assert_eq!(spread, ["positions", "angles", "scales"]);
        assert_eq!(b.marks.len(), 3, "one icon per spread output");
        let want = [PortType::Vec2, PortType::Number, PortType::Number];
        for ((o, m), ty) in b.outs.iter().zip(&b.marks).zip(want) {
            assert_eq!(o.spread, Some(ty), "{}", o.label);
            // The label, then the icon, then 10 to the pin; all inside.
            let label_right = b.rect.right() - 10.0 - SPREAD_W;
            assert!(label_right - label_w(&o.label, FOOT_FONT) > b.rect.left() + 8.0);
            let icon = m.shrink(2.0);
            assert!(icon.left() >= label_right + MARK_GAP - 0.01, "{}", o.label);
            assert!(icon.right() <= b.rect.right() - 10.0 + 0.01);
            assert!((m.center().y - o.y).abs() < 0.01);
        }
        let progress = b.outs.iter().find(|o| o.label == "progress").unwrap();
        assert_eq!(progress.spread, None);
    }

    #[test]
    fn spread_inputs_carry_the_icon_after_their_name() {
        // A label row: the Renderer's layers (layers have no value box).
        let b = laid_out("render/renderer", &[]);
        let row = row_named(&b, "layer").expect("the layers input reads layer");
        assert_eq!(row.spread, Some(PortType::Layer));
        let pin = b.pin(Side::In, "layers").unwrap();
        assert!(pin.spread);
        assert_eq!(pin.label, "layer");
        let icon = b.marks.first().expect("an icon").shrink(2.0);
        assert!(icon.left() >= b.rect.left() + 10.0 + label_w("layer", LABEL_FONT));
        assert!(
            icon.right() <= row.label_rect.right() + 0.01,
            "room reserved"
        );
        assert!(row.label_rect.right() < b.rect.right() - 8.0);
        // Its output reads layer too, with an icon.
        assert_eq!(b.outs[0].label, "layer");
        assert_eq!(b.outs[0].spread, Some(PortType::Layer));
        // A value box: an unwired number spread keeps its icon left of
        // its value.
        let b = laid_out("math/statistics", &[]);
        let f = fields_of(&b, "spread")[0];
        let tag = f.label.as_ref().unwrap();
        assert_eq!(tag.spread, Some(PortType::Number));
        let need = label_w(&tag.text, LABEL_FONT) + marks_w(tag.constant, false, true);
        assert!(need <= f.value_rect.left() - tag.frame.left() - BOX_PAD + 0.5);
        let icon = b.marks.first().expect("an icon").shrink(2.0);
        assert!(icon.right() <= f.value_rect.left() + 0.01);
        // Read Spread: in and out.
        let b = laid_out("state/read-spread", &[]);
        assert!(b.pin(Side::In, "init").unwrap().spread);
        assert!(b.pin(Side::Out, "current").unwrap().spread);
        assert_eq!(b.marks.len(), 2);
    }

    #[test]
    fn single_pins_have_no_icon() {
        let b = laid_out("draw/rect", &[]);
        assert!(b.marks.is_empty());
        assert!(b.pins.iter().all(|p| !p.spread));
    }

    #[test]
    fn foreach_pills_and_outer_pins_are_spreads() {
        let boxes = laid_out_patch(
            json!([{"id": "fe", "defType": "region/foreach", "position": {"x": 0, "y": 0},
                    "ports": [{"id": "p1", "name": "layer", "type": "layer", "direction": "input"},
                              {"id": "p2", "name": "out", "type": "layer", "direction": "output"}],
                    "width": 300, "height": 200}]),
            json!([]),
            None,
            1.0,
        );
        let b = &boxes["fe"];
        // Input port: the outer `in-` gathers; output port: the outer `out-`.
        assert!(b.pin(Side::In, "p1").unwrap().spread);
        assert!(!b.pin(Side::Out, "p1").unwrap().spread);
        assert!(!b.pin(Side::In, "p2").unwrap().spread);
        assert!(b.pin(Side::Out, "p2").unwrap().spread);
        assert!(b.pills.iter().all(|p| p.spread));
        assert_eq!(b.marks.len(), 2);
        for (pill, m) in b.pills.iter().zip(&b.marks) {
            assert!(pill.rect.contains_rect(m.shrink(2.0)), "{}", pill.name);
        }
    }

    /// A subpatch whose output `p1` is fed from inside by `inner`'s `port`.
    fn subpatch_fed_by(inner: &str, port: &str, scope: Option<&str>) -> NodeBox {
        laid_out_patch(
            json!([
                {"id": "sub", "defType": "meta/subpatch", "position": {"x": 0, "y": 0},
                 "params": {"label": "sub"},
                 "ports": [{"id": "p1", "name": "values", "type": "number", "direction": "output"}],
                 "width": 400, "height": 300},
                {"id": "c", "defType": inner, "position": {"x": 80, "y": 40}, "parentId": "sub"},
            ]),
            json!([{"id": "e1", "source": "c", "sourceHandle": format!("out-{port}"),
                    "target": "sub", "targetHandle": "in-p1"}]),
            scope,
            1.0,
        )
        .swap_remove("sub")
        .expect("laid out")
    }

    #[test]
    fn collapsed_subpatch_outputs_carry_their_real_spread_flag() {
        let b = subpatch_fed_by("spread/range", "spread", None);
        assert_eq!(b.look, Look::Collapsed);
        assert!(b.pin(Side::Out, "p1").unwrap().spread);
        assert_eq!(b.outs[0].spread, Some(PortType::Number));
        assert_eq!(b.marks.len(), 1);
        let label_right = b.rect.right() - 10.0 - SPREAD_W;
        assert!(label_right - label_w("values", FOOT_FONT) > b.rect.left() + 8.0);
        let b = subpatch_fed_by("value/number", "value", None);
        assert!(!b.pin(Side::Out, "p1").unwrap().spread);
        assert!(b.marks.is_empty());
        // Entered, its frame's output pill and pins say the same.
        let b = subpatch_fed_by("spread/range", "spread", Some("sub"));
        assert_eq!(b.look, Look::Frame);
        assert!(b.pills[0].spread);
        assert!(b.pin(Side::Out, "p1").unwrap().spread);
        let b = subpatch_fed_by("value/number", "value", Some("sub"));
        assert!(!b.pills[0].spread);
        assert!(!b.pin(Side::Out, "p1").unwrap().spread);
    }

    #[test]
    fn spread_pins_take_their_type_colour() {
        let b = laid_out("render/renderer", &[]);
        let pin = b.pin(Side::In, "layers").unwrap();
        assert!(pin.spread);
        assert_eq!(
            super::super::draw::pin_color(pin.ty, pin.constant),
            crate::theme::port_color("layer")
        );
        assert_ne!(
            super::super::draw::pin_color(PortType::Layer, false),
            *crate::theme::SPREAD_PIN
        );
    }

    #[test]
    fn far_out_the_icons_are_hidden() {
        let node = json!([{"id": "n", "defType": "geometry/layout", "position": {"x": 0, "y": 0}}]);
        let at = |z: f32| {
            laid_out_patch(node.clone(), json!([]), None, z)["n"]
                .marks
                .len()
        };
        assert_eq!(at(0.3), 0);
        assert_eq!(at(0.31), 3);
    }
}
