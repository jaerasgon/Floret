//! Custom node bodies on the canvas — the TS `src/editor/nodes/*`
//! components: where their controls sit inside the node box, how they're
//! painted, and what the canvas hit-tests. Everything scales with the
//! zoom; text is dropped when it gets too small to read. The value maths
//! is in `editor::widgets`.

use super::draw::SPREAD_PIN;
use super::geom::{row_box, row_y, Ctx, Field, NodeBox, PinGeom, Row, HEADER_H, NODE_W};
use crate::editor::ports::{self, Side};
use crate::editor::values::{self, EditKey, FieldKind};
use crate::editor::widgets::{self, Custom, Pad, ProbeView, SliderState, Slots};
use crate::theme::{self, BG_0, BG_1, BG_3, BORDER, FG_0, FG_1, FG_2, GOLD};
use eframe::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind,
};
use floret_engine::patch::{PatchFile, PatchNode};
use floret_engine::spec::{Catalogue, NodeSpec, PortType};
use serde_json::{json, Value as Json};
use std::collections::HashSet;

pub const PROBE_SIZE: (f64, f64) = (150.0, 112.0);
const PROBE_HEAD: f32 = 14.0;
const PROBE_LINE: f32 = 12.0;
const PICK_BLUE: Color32 = Color32::from_rgb(124, 182, 255);
const AMBER: Color32 = Color32::from_rgb(255, 200, 100);
/// `.comment-node` ink and paper.
const COMMENT_INK: Color32 = Color32::from_rgb(0xf0, 0xd8, 0x96);
const COMMENT: Color32 = Color32::from_rgb(255, 220, 120);

/// An interactive control in a custom body.
#[derive(Clone, Debug, PartialEq)]
pub enum WidgetKind {
    Slider(SliderState),
    Toggle(bool),
    Pad(Pad),
    Bang,
    /// Momentary button: true while held.
    Hold,
    /// Stepper `−` / `+`.
    Step(f64),
    /// Arity `−` / `+`: the param and the count it sets (`None` at a
    /// bound).
    Slots {
        param: &'static str,
        delta: i64,
        to: Option<i64>,
    },
    /// Open the file picker (`svg`: SVG files only).
    Pick {
        svg: bool,
        replace: bool,
    },
    Clear,
    /// A comment's resize corner.
    Grip,
}

impl WidgetKind {
    pub fn interactive(&self) -> bool {
        !matches!(self, WidgetKind::Slots { to: None, .. })
    }

    /// Driven by press-and-drag rather than a click.
    pub fn drags(&self) -> bool {
        matches!(
            self,
            WidgetKind::Slider(_) | WidgetKind::Pad(_) | WidgetKind::Hold | WidgetKind::Grip
        )
    }
}

#[derive(Clone, Debug)]
pub struct Widget {
    pub rect: Rect,
    pub node: String,
    pub kind: WidgetKind,
}

/// How a piece of body text is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteRole {
    /// Left-aligned secondary text (file name).
    Label,
    /// Right-aligned tertiary text (file size); `Warn` when large.
    Size,
    Warn,
    /// Centred primary value (stepper).
    Value,
    /// Centred faint caption (range labels).
    Dim,
    /// The `− n +` pill around a slot count.
    Pill,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub rect: Rect,
    pub text: String,
    pub role: NoteRole,
}

/// Extra body height between the header and the rows.
pub fn body_h(c: Custom, n: &PatchNode) -> f32 {
    let picked = |n: &PatchNode| {
        n.params
            .get("filename")
            .and_then(Json::as_str)
            .is_some_and(|s| !s.is_empty())
    };
    let file = |n: &PatchNode| if picked(n) { 46.0 } else { 30.0 };
    match c {
        Custom::Slider => 44.0,
        Custom::Toggle => 34.0,
        Custom::XyPad => 132.0,
        Custom::File => file(n),
        Custom::Svg => match n.params.get("mode").and_then(Json::as_str) {
            Some("url") => 26.0,
            Some("markup") => 66.0,
            _ => file(n),
        },
        _ => 0.0,
    }
}

/// Row count when the body replaces the standard rows.
pub fn rows(c: Custom, n: &PatchNode, _cat: &Catalogue) -> Option<usize> {
    match c {
        Custom::Slider => Some(0),
        Custom::Select => Some(2),
        Custom::Palette => Some(widgets::slot_count(n, widgets::PALETTE_STOPS) as usize + 1),
        _ => None,
    }
}

/// A rect inside the node, in canvas units from its top-left.
fn at(b: &NodeBox, z: f32, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(b.rect.min + vec2(x, y) * z, vec2(w, h) * z)
}

fn param_field(n: &PatchNode, spec: &NodeSpec, rect: Rect, key: &str, kind: FieldKind) -> Field {
    let k = EditKey::Param(key.to_owned());
    let v = values::current(n, spec, &k, None);
    Field::new(rect, &n.id, k, kind, v)
}

fn number() -> FieldKind {
    FieldKind::Number { integer: false }
}

pub fn layout(
    cx: &Ctx,
    n: &PatchNode,
    spec: &NodeSpec,
    c: Custom,
    wired: &HashSet<(&str, &str)>,
    b: &mut NodeBox,
) {
    b.body = body_h(c, n);
    let z = cx.zoom;
    let id = n.id.clone();
    let widget = |rect: Rect, kind: WidgetKind| Widget {
        rect,
        node: id.clone(),
        kind,
    };
    match c {
        Custom::Slider => {
            let s = widgets::slider(n);
            let y = HEADER_H + 14.0;
            b.widgets.push(widget(
                at(b, z, 10.0, y - 8.0, NODE_W - 28.0, 16.0),
                WidgetKind::Slider(s),
            ));
            if let Some(out) = spec.outputs.first() {
                b.pins.push(PinGeom {
                    side: Side::Out,
                    port: out.name.clone(),
                    label: values::display_label(out, false),
                    ty: out.ty,
                    pos: pos2(b.rect.right(), b.rect.top() + y * z),
                    spread: false,
                    constant: false,
                });
            }
            let fy = HEADER_H + 26.0;
            for (key, x) in [("min", 8.0), ("max", NODE_W - 62.0)] {
                b.fields.push(Field {
                    step: s.scrub_step(),
                    bare: true,
                    ..param_field(n, spec, at(b, z, x, fy, 44.0, 13.0), key, number())
                });
            }
            b.header_value = Some(format!("{:.2}", s.value));
        }
        Custom::Toggle => {
            let on = values::as_bool(&values::current(
                n,
                spec,
                &EditKey::Param("value".into()),
                None,
            ));
            b.widgets.push(widget(
                at(b, z, 10.0, HEADER_H + 6.0, NODE_W - 20.0, 22.0),
                WidgetKind::Toggle(on),
            ));
            b.header_value = Some(if on { "on" } else { "off" }.into());
            row_box(cx, n, spec, wired, b);
        }
        Custom::XyPad => {
            let pad = widgets::pad(n);
            b.widgets.push(widget(
                at(b, z, 10.0, HEADER_H + 8.0, NODE_W - 20.0, 100.0),
                WidgetKind::Pad(pad),
            ));
            let fy = HEADER_H + 113.0;
            let mut x = 10.0;
            for (axis, keys) in [("x", [0, 1]), ("y", [2, 3])] {
                b.notes.push(Note {
                    rect: at(b, z, x, fy, 8.0, 13.0),
                    text: axis.into(),
                    role: NoteRole::Dim,
                });
                x += 10.0;
                for k in keys {
                    let key = widgets::PAD_RANGE_KEYS[k];
                    let rect = at(b, z, x, fy, 28.0, 13.0);
                    let value = json!(pad.range[k]);
                    b.fields.push(Field {
                        step: 0.1,
                        bare: true,
                        ..Field::new(rect, &n.id, EditKey::Param(key.into()), number(), value)
                    });
                    x += 30.0;
                }
                x += 8.0;
            }
            b.header_value = Some(format!("{:.2} · {:.2}", pad.x, pad.y));
            row_box(cx, n, spec, wired, b);
        }
        Custom::Bang | Custom::Button | Custom::Stepper => {
            row_box(cx, n, spec, wired, b);
            let y = (row_y(b, 0, z) - b.rect.top()) / z;
            match c {
                Custom::Bang => b.widgets.push(widget(
                    at(b, z, 10.0, y - 8.0, 16.0, 16.0),
                    WidgetKind::Bang,
                )),
                Custom::Button => b.widgets.push(widget(
                    at(b, z, 10.0, y - 8.0, 16.0, 16.0),
                    WidgetKind::Hold,
                )),
                _ => {
                    b.widgets.push(widget(
                        at(b, z, 10.0, y - 8.0, 16.0, 16.0),
                        WidgetKind::Step(-1.0),
                    ));
                    let v = values::current(n, spec, &EditKey::Param("value".into()), None);
                    b.notes.push(Note {
                        rect: at(b, z, 28.0, y - 8.0, 26.0, 16.0),
                        text: values::num(values::as_f64(&v), 3),
                        role: NoteRole::Value,
                    });
                    b.widgets.push(widget(
                        at(b, z, 56.0, y - 8.0, 16.0, 16.0),
                        WidgetKind::Step(1.0),
                    ));
                }
            }
        }
        Custom::Select => {
            row_box(cx, n, spec, wired, b);
            let rel = |i: usize, b: &NodeBox| (row_y(b, i, z) - b.rect.top()) / z;
            let (y0, y1) = (rel(0, b), rel(1, b));
            let options = widgets::select_options(n);
            b.fields.push(param_field(
                n,
                spec,
                at(b, z, 10.0, y0 - 7.5, 100.0, 15.0),
                "value",
                FieldKind::Select(options),
            ));
            b.fields.push(Field {
                bare: true,
                ..param_field(
                    n,
                    spec,
                    at(b, z, 10.0, y1 - 6.5, 100.0, 13.0),
                    "options",
                    FieldKind::Text,
                )
            });
        }
        Custom::Slots(s) => {
            row_box(cx, n, spec, wired, b);
            slots(cx, n, s, b);
        }
        Custom::Palette => {
            palette(cx, n, spec, wired, b);
            slots(cx, n, widgets::PALETTE_STOPS, b);
        }
        Custom::File | Custom::Svg => {
            let svg = c == Custom::Svg;
            let mode = n
                .params
                .get("mode")
                .and_then(Json::as_str)
                .unwrap_or("file");
            let top = HEADER_H + 5.0;
            match (svg, mode) {
                (true, "url") => b.fields.push(param_field(
                    n,
                    spec,
                    at(b, z, 10.0, top, NODE_W - 20.0, 15.0),
                    "url",
                    FieldKind::Text,
                )),
                (true, "markup") => b.fields.push(Field {
                    multiline: true,
                    ..param_field(
                        n,
                        spec,
                        at(b, z, 10.0, top, NODE_W - 20.0, 56.0),
                        "markup",
                        FieldKind::Text,
                    )
                }),
                _ => file_body(n, svg, top, z, b),
            }
            row_box(cx, n, spec, wired, b);
        }
        Custom::Probe => {}
    }
}

fn file_body(n: &PatchNode, svg: bool, top: f32, z: f32, b: &mut NodeBox) {
    let s = |k: &str| n.params.get(k).and_then(Json::as_str).unwrap_or("");
    let name = s("filename");
    let picked = !name.is_empty();
    let push = |b: &mut NodeBox, rect: Rect, kind: WidgetKind| {
        b.widgets.push(Widget {
            rect,
            node: n.id.clone(),
            kind,
        });
    };
    push(
        b,
        at(b, z, 10.0, top, 92.0, 19.0),
        WidgetKind::Pick {
            svg,
            replace: picked,
        },
    );
    if !picked {
        return;
    }
    push(b, at(b, z, 106.0, top, 19.0, 19.0), WidgetKind::Clear);
    let size = widgets::data_url_size(s("dataUrl"));
    b.notes.push(Note {
        rect: at(b, z, 10.0, top + 23.0, 98.0, 13.0),
        text: name.to_owned(),
        role: NoteRole::Label,
    });
    if !svg {
        b.notes.push(Note {
            rect: at(b, z, 108.0, top + 23.0, NODE_W - 118.0, 13.0),
            text: widgets::format_bytes(size),
            role: if size > widgets::SIZE_HINT_LIMIT {
                NoteRole::Warn
            } else {
                NoteRole::Size
            },
        });
    }
}

/// The `− n +` pill after the header label.
fn slots(cx: &Ctx, n: &PatchNode, s: Slots, b: &mut NodeBox) {
    let z = cx.zoom;
    let count = widgets::slot_count(n, s);
    let label_w = if b.math {
        cx.text_w(&b.label, 13.0)
    } else {
        cx.text_w(&b.label, 12.5)
    };
    let x = 22.0 + label_w + 8.0;
    let y = (HEADER_H - 16.0) / 2.0;
    b.notes.push(Note {
        rect: at(b, z, x, y, 46.0, 16.0),
        text: count.to_string(),
        role: NoteRole::Pill,
    });
    for (delta, dx) in [(-1, 0.0), (1, 31.0)] {
        b.widgets.push(Widget {
            rect: at(b, z, x + dx, y, 15.0, 16.0),
            node: n.id.clone(),
            kind: WidgetKind::Slots {
                param: s.param,
                delta,
                to: widgets::step_slots(n, s, delta),
            },
        });
    }
}

/// Palette rows: one pin + hex swatch per stop, the spread output last.
fn palette(
    cx: &Ctx,
    n: &PatchNode,
    spec: &NodeSpec,
    wired: &HashSet<(&str, &str)>,
    b: &mut NodeBox,
) {
    let z = cx.zoom;
    let inputs = cx.cat.resolved_inputs(spec, &n.params);
    let empty_row = |y: f32| Row {
        y,
        label: None,
        label_rect: Rect::NOTHING,
        toggle: None,
        expanded: false,
        readout: None,
        constant: false,
        integer: false,
        output: None,
    };
    for (i, port) in inputs.iter().enumerate() {
        let y = row_y(b, i, z);
        let label = values::display_label(port, true);
        b.pins.push(PinGeom {
            side: Side::In,
            port: port.name.clone(),
            label: label.clone(),
            ty: port.ty,
            pos: pos2(b.rect.left(), y),
            spread: false,
            constant: false,
        });
        let mut row = empty_row(y);
        row.label = Some(label);
        if !wired.contains(&(n.id.as_str(), port.name.as_str())) {
            let rel = (y - b.rect.top()) / z;
            b.fields.push(Field::new(
                at(b, z, 26.0, rel - 7.0, NODE_W - 36.0, 14.0),
                &n.id,
                EditKey::Param(port.name.clone()),
                FieldKind::Color,
                Json::String(widgets::palette_stop(n, i)),
            ));
        }
        b.rows.push(row);
    }
    if let Some(out) = spec.outputs.first() {
        let y = row_y(b, inputs.len(), z);
        b.pins.push(PinGeom {
            side: Side::Out,
            port: out.name.clone(),
            label: values::display_label(out, false),
            ty: out.ty,
            pos: pos2(b.rect.right(), y),
            spread: true,
            constant: false,
        });
        let mut row = empty_row(y);
        row.output = Some((values::display_label(out, false), out.name.clone()));
        b.rows.push(row);
    }
}

/// The wire type flowing into a probe (it echoes it on `out`).
fn probe_type(patch: &PatchFile, cat: &Catalogue, id: &str) -> PortType {
    patch
        .edges
        .iter()
        .find(|e| e.target == id)
        .and_then(|e| {
            let (_, name) = ports::split_handle(e.source_handle.as_deref())?;
            ports::resolve_port(patch, cat, &e.source, Side::Out, name)
        })
        .map_or(PortType::Any, |p| p.ty)
}

/// A probe card's size for what it shows (`min-width 72`, `max-width
/// 168`, body up to 92 tall; pictures are 132 × 100).
pub fn probe_size(view: &ProbeView) -> (f32, f32) {
    const CHAR: f32 = 5.5;
    let head = 8.0 * CHAR + view.meta().chars().count() as f32 * 4.9 + 18.0;
    let (w, body) = match view {
        ProbeView::Image { .. } | ProbeView::Layer => (138.0, 106.0),
        ProbeView::Text { count, rows, .. } if *count > 1 => {
            let longest = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
            let lines = rows.len() + usize::from(*count > rows.len());
            (
                34.0 + longest as f32 * CHAR,
                (lines as f32 * PROBE_LINE + 4.0).min(92.0),
            )
        }
        ProbeView::Text { rows, .. } => {
            let longest = rows.first().map_or(9, |r| r.chars().count());
            (16.0 + longest as f32 * CHAR, PROBE_LINE + 4.0)
        }
        ProbeView::NoData => (16.0 + 23.0 * CHAR, PROBE_LINE + 4.0),
    };
    (w.max(head).clamp(72.0, 168.0), PROBE_HEAD + body + 2.0)
}

pub fn probe_box(cx: &Ctx, n: &PatchNode, b: &mut NodeBox) {
    let z = cx.zoom;
    let view = cx
        .decor
        .probes
        .get(&n.id)
        .cloned()
        .unwrap_or(ProbeView::NoData);
    let (w, h) = probe_size(&view);
    b.rect = Rect::from_min_size(b.rect.min, vec2(w, h) * z);
    b.header = Rect::from_min_size(b.rect.min, vec2(b.rect.width(), PROBE_HEAD * z));
    let ty = probe_type(cx.patch, cx.cat, &n.id);
    let y = b.rect.center().y;
    for (side, port, x) in [
        (Side::In, "in", b.rect.left()),
        (Side::Out, "out", b.rect.right()),
    ] {
        b.pins.push(PinGeom {
            side,
            port: port.into(),
            label: port.into(),
            ty,
            pos: pos2(x, y),
            spread: true,
            constant: false,
        });
    }
    b.probe = Some(view);
}

/// The probe body (below its header strip), screen space.
pub fn probe_body(b: &NodeBox, z: f32) -> Rect {
    Rect::from_min_max(
        b.rect.min + vec2(0.0, PROBE_HEAD * z),
        b.rect.max - vec2(0.0, 2.0 * z),
    )
}

/// How far a probe's list can scroll (canvas units).
pub fn probe_max_scroll(b: &NodeBox, z: f32) -> f32 {
    match &b.probe {
        Some(ProbeView::Text { rows, count, .. }) if *count > 1 => {
            let extra = usize::from(*count > rows.len());
            let content = (rows.len() + extra) as f32 * PROBE_LINE + 4.0;
            (content - probe_body(b, z).height() / z).max(0.0)
        }
        _ => 0.0,
    }
}

pub fn comment_box(cx: &Ctx, n: &PatchNode, b: &mut NodeBox) {
    let z = cx.zoom;
    b.header = b.rect;
    let r = b.rect;
    b.widgets.push(Widget {
        rect: Rect::from_min_max(r.max - vec2(12.0, 12.0) * z, r.max),
        node: n.id.clone(),
        kind: WidgetKind::Grip,
    });
}

// ─── Painting ───────────────────────────────────────────────────────────────

/// Per-node state the custom painters need.
pub struct BodyState {
    pub hover: Option<Pos2>,
    pub selected: bool,
    /// A held button / a bang that just fired / a dragged control.
    pub active: bool,
    /// Probe list scroll (canvas units).
    pub scroll: f32,
    /// A comment whose text is open in the editor.
    pub editing: bool,
    /// A file is being dragged over this picker.
    pub drop: bool,
    /// Probe picture, when the canvas has one.
    pub thumb: Option<eframe::egui::TextureId>,
}

fn hovered(st: &BodyState, r: Rect) -> bool {
    st.hover.is_some_and(|p| r.contains(p))
}

pub fn draw_body(p: &Painter, b: &NodeBox, st: &BodyState, z: f32) {
    let text = z > 0.3;
    let io = theme::category_color("io");
    for nt in &b.notes {
        let r = nt.rect;
        match nt.role {
            NoteRole::Pill => {
                p.rect(
                    r,
                    4.0 * z,
                    BG_1,
                    Stroke::new(1.0, BORDER),
                    StrokeKind::Inside,
                );
                let mid = Rect::from_center_size(r.center(), vec2(16.0 * z, r.height()));
                for x in [mid.left(), mid.right()] {
                    p.line_segment(
                        [pos2(x, r.top() + 1.0), pos2(x, r.bottom() - 1.0)],
                        Stroke::new(1.0, BORDER),
                    );
                }
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        &nt.text,
                        FontId::monospace(10.0 * z),
                        *FG_0,
                    );
                }
            }
            _ if !text => {}
            NoteRole::Label => {
                p.with_clip_rect(r).text(
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    &nt.text,
                    FontId::monospace(10.0 * z),
                    *FG_1,
                );
            }
            NoteRole::Size | NoteRole::Warn => {
                p.text(
                    r.right_center(),
                    Align2::RIGHT_CENTER,
                    &nt.text,
                    FontId::monospace(10.0 * z),
                    if nt.role == NoteRole::Warn {
                        AMBER
                    } else {
                        *FG_2
                    },
                );
            }
            NoteRole::Value => {
                p.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &nt.text,
                    FontId::monospace(11.0 * z),
                    *FG_0,
                );
            }
            NoteRole::Dim => {
                p.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &nt.text,
                    FontId::monospace(9.0 * z),
                    *FG_2,
                );
            }
        }
    }
    for w in &b.widgets {
        let r = w.rect;
        let hot = hovered(st, r);
        match &w.kind {
            WidgetKind::Slider(s) => {
                let track = Rect::from_center_size(r.center(), vec2(r.width(), 6.0 * z));
                p.rect_filled(track, 3.0 * z, BG_1);
                let inner = r.shrink2(vec2(7.0 * z, 0.0));
                let x = inner.left() + inner.width() * s.t() as f32;
                let c = pos2(x, r.center().y);
                if hot || st.active {
                    p.circle_filled(c, 10.0 * z, GOLD.gamma_multiply(0.18));
                }
                p.circle(c, 6.0 * z, GOLD, Stroke::new(2.0 * z, BG_0));
            }
            WidgetKind::Toggle(on) => {
                let (fill, ink, label) = if *on {
                    (GOLD, BG_0, "● ON")
                } else {
                    (if hot { BG_3 } else { BG_1 }, FG_1, "○ OFF")
                };
                let stroke = if *on { GOLD } else { BORDER };
                p.rect(
                    r,
                    4.0 * z,
                    fill,
                    Stroke::new(1.0, stroke),
                    StrokeKind::Inside,
                );
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::monospace(11.0 * z),
                        *ink,
                    );
                }
            }
            WidgetKind::Pad(pad) => {
                p.rect(
                    r,
                    4.0 * z,
                    BG_1,
                    Stroke::new(1.0, BORDER),
                    StrokeKind::Inside,
                );
                let grid = Stroke::new(1.0, BORDER);
                p.line_segment([r.center_top(), r.center_bottom()], grid);
                p.line_segment([r.left_center(), r.right_center()], grid);
                let (u, v) = pad.uv();
                let c = r.min + vec2(u as f32 * r.width(), v as f32 * r.height());
                if st.active {
                    let guide = Stroke::new(1.0, GOLD.gamma_multiply(0.35));
                    p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom())], guide);
                    p.line_segment([pos2(r.left(), c.y), pos2(r.right(), c.y)], guide);
                }
                p.circle(c, 6.0 * z, GOLD, Stroke::new(2.0 * z, BG_0));
            }
            WidgetKind::Bang => {
                let c = r.center();
                if hot || st.active {
                    p.circle_filled(c, 12.0 * z, io.gamma_multiply(0.28));
                }
                let s = if st.active { 1.0 } else { 0.86 };
                p.circle_filled(c, 8.0 * z * s, if st.active { *FG_0 } else { io });
            }
            WidgetKind::Hold => {
                let c = r.center();
                let fill = if st.active { io } else { Color32::TRANSPARENT };
                let rad = if st.active { 7.0 } else { 8.0 } * z;
                p.circle(c, rad, fill, Stroke::new(1.5 * z, io));
            }
            WidgetKind::Step(d) => {
                p.rect(
                    r,
                    4.0 * z,
                    BG_1,
                    Stroke::new(1.0, if hot { io } else { *BORDER }),
                    StrokeKind::Inside,
                );
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        if *d < 0.0 { "−" } else { "+" },
                        FontId::proportional(12.0 * z),
                        if hot { *FG_0 } else { *FG_1 },
                    );
                }
            }
            WidgetKind::Slots { delta, to, .. } => {
                if hot && to.is_some() {
                    p.rect_filled(r.shrink(1.0), 3.0 * z, GOLD.gamma_multiply(0.22));
                }
                let ink = match (to, hot) {
                    (None, _) => FG_2.gamma_multiply(0.4),
                    (_, true) => *FG_0,
                    _ => *FG_1,
                };
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        if *delta < 0 { "−" } else { "+" },
                        FontId::proportional(12.0 * z),
                        ink,
                    );
                }
            }
            WidgetKind::Pick { svg, replace } => {
                let a = if hot { 0.32 } else { 0.18 };
                p.rect(
                    r,
                    3.0 * z,
                    PICK_BLUE.gamma_multiply(a),
                    Stroke::new(1.0, PICK_BLUE.gamma_multiply(0.5)),
                    StrokeKind::Inside,
                );
                let label = match (replace, svg) {
                    (true, _) => "Replace…",
                    (false, true) => "Pick SVG…",
                    (false, false) => "Pick file…",
                };
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(11.0 * z),
                        *FG_0,
                    );
                }
            }
            WidgetKind::Clear => {
                p.rect(
                    r,
                    3.0 * z,
                    Color32::TRANSPARENT,
                    Stroke::new(1.0, if hot { FG_2 } else { BORDER }),
                    StrokeKind::Inside,
                );
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        "×",
                        FontId::proportional(10.0 * z),
                        if hot { *FG_0 } else { *FG_2 },
                    );
                }
            }
            WidgetKind::Grip => {}
        }
    }
    if st.drop {
        let body = Rect::from_min_max(
            b.rect.min + vec2(0.0, HEADER_H * z),
            pos2(b.rect.right(), b.rect.top() + (HEADER_H + b.body) * z),
        );
        p.rect(
            body.shrink(2.0 * z),
            3.0 * z,
            GOLD.gamma_multiply(0.12),
            Stroke::new(1.0, GOLD.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
        if text {
            p.text(
                body.center(),
                Align2::CENTER_CENTER,
                "Drop to load",
                FontId::monospace(10.0 * z),
                *GOLD,
            );
        }
    }
}

/// `.node-probe`: a dashed translucent card with a type / count header,
/// then an index → value list (scrollable) or a picture.
pub fn draw_probe(p: &Painter, b: &NodeBox, st: &BodyState, z: f32) {
    let r = b.rect;
    let edge = if st.selected {
        *GOLD
    } else {
        Color32::from_rgb(0x56, 0x5c, 0x66)
    };
    p.rect_filled(r, 5.0 * z, Color32::from_rgba_unmultiplied(16, 19, 24, 184));
    if st.selected {
        p.rect_stroke(r, 5.0 * z, Stroke::new(1.0, edge), StrokeKind::Inside);
    } else {
        dashed_rect(p, r.shrink(0.5), Stroke::new(1.0, edge), z);
    }
    let head = Rect::from_min_size(r.min, vec2(r.width(), PROBE_HEAD * z));
    p.extend(eframe::egui::Shape::dashed_line(
        &[head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, edge.gamma_multiply(0.5)),
        3.0 * z,
        3.0 * z,
    ));
    if z <= 0.3 {
        return;
    }
    let view = b.probe.clone().unwrap_or(ProbeView::NoData);
    p.text(
        head.left_center() + vec2(6.0 * z, 0.0),
        Align2::LEFT_CENTER,
        "PROBE",
        FontId::proportional(7.0 * z),
        FG_2.gamma_multiply(0.7),
    );
    p.with_clip_rect(head).text(
        head.right_center() - vec2(6.0 * z, 0.0),
        Align2::RIGHT_CENTER,
        view.meta(),
        FontId::monospace(8.0 * z),
        SPREAD_PIN,
    );
    let body = probe_body(b, z);
    let clip = p.with_clip_rect(body);
    let font = FontId::monospace(9.0 * z);
    let line = |i: usize| body.top() + (2.0 + PROBE_LINE * (i as f32 + 0.5)) * z;
    let dim = |text: &str| {
        clip.text(
            pos2(body.left() + 6.0 * z, line(0)),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            FG_2.gamma_multiply(0.7),
        );
    };
    match &view {
        ProbeView::NoData => dim("no data (run the patch)"),
        ProbeView::Text { count: 0, .. } => dim("[ ] empty"),
        ProbeView::Text { count: 1, rows, .. } => {
            clip.text(
                pos2(body.left() + 6.0 * z, line(0)),
                Align2::LEFT_CENTER,
                rows.first().map_or("", String::as_str),
                font,
                *FG_0,
            );
        }
        ProbeView::Text { count, rows, .. } => {
            let off = st.scroll * z;
            for (i, row) in rows.iter().enumerate() {
                let y = line(i) - off;
                if y < body.top() - PROBE_LINE * z || y > body.bottom() + PROBE_LINE * z {
                    continue;
                }
                clip.text(
                    pos2(body.left() + 22.0 * z, y),
                    Align2::RIGHT_CENTER,
                    i.to_string(),
                    font.clone(),
                    FG_2.gamma_multiply(0.6),
                );
                clip.text(
                    pos2(body.left() + 28.0 * z, y),
                    Align2::LEFT_CENTER,
                    row,
                    font.clone(),
                    *FG_0,
                );
            }
            if *count > rows.len() {
                clip.text(
                    pos2(body.left() + 6.0 * z, line(rows.len()) - off),
                    Align2::LEFT_CENTER,
                    format!("+{} more…", count - rows.len()),
                    font.clone(),
                    FG_2.gamma_multiply(0.7),
                );
            }
            let max = probe_max_scroll(b, z);
            if max > 0.0 {
                // Slim scrollbar.
                let h = body.height() * body.height() / (body.height() + max * z);
                let t = (st.scroll / max).clamp(0.0, 1.0);
                let y = body.top() + (body.height() - h) * t;
                p.rect_filled(
                    Rect::from_min_size(pos2(body.right() - 4.0 * z, y), vec2(3.0 * z, h)),
                    2.0 * z,
                    SPREAD_PIN.gamma_multiply(0.4),
                );
            }
        }
        ProbeView::Image { .. } | ProbeView::Layer => {
            let frame = body.shrink(3.0 * z);
            p.rect_filled(frame, 3.0 * z, Color32::from_rgb(0x0d, 0x0f, 0x14));
            match st.thumb {
                Some(tex) => {
                    p.image(
                        tex,
                        frame,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    clip.text(
                        frame.center(),
                        Align2::CENTER_CENTER,
                        view.meta(),
                        font,
                        *FG_2,
                    );
                }
            }
        }
    }
}

/// `.comment-node`: a dashed amber note, no header; resizable from its
/// corner.
pub fn draw_comment(p: &Painter, b: &NodeBox, st: &BodyState, z: f32) {
    let r = b.rect;
    if st.selected {
        p.rect(
            r,
            4.0 * z,
            COMMENT.gamma_multiply(0.12),
            Stroke::new(1.0, COMMENT.gamma_multiply(0.7)),
            StrokeKind::Inside,
        );
    } else {
        p.rect_filled(r, 4.0 * z, COMMENT.gamma_multiply(0.08));
        dashed_rect(
            p,
            r.shrink(0.5),
            Stroke::new(1.0, COMMENT.gamma_multiply(0.45)),
            z,
        );
    }
    let grip = hovered(st, r) || st.selected;
    if grip {
        let c = r.right_bottom() - vec2(3.0, 3.0) * z;
        for k in [4.0, 8.0] {
            p.line_segment(
                [c - vec2(k * z, 0.0), c - vec2(0.0, k * z)],
                Stroke::new(1.0, COMMENT.gamma_multiply(0.7)),
            );
        }
    }
    if z <= 0.3 || st.editing {
        return;
    }
    let body = r.shrink2(vec2(10.0, 8.0) * z);
    let text = b.comment.as_deref().unwrap_or("");
    let (text, color) = if text.is_empty() {
        ("double-click to edit", COMMENT_INK.gamma_multiply(0.4))
    } else {
        (text, COMMENT_INK)
    };
    let mut job = eframe::egui::text::LayoutJob::single_section(
        text.to_owned(),
        eframe::egui::TextFormat {
            font_id: FontId::proportional(12.0 * z),
            color,
            line_height: Some(12.0 * 1.45 * z),
            italics: text.is_empty(),
            ..Default::default()
        },
    );
    job.wrap.max_width = body.width();
    let galley = p.layout_job(job);
    p.with_clip_rect(body).galley(body.min, galley, color);
}

/// A dashed rectangle outline (egui has none built in).
fn dashed_rect(p: &Painter, r: Rect, stroke: Stroke, z: f32) {
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    p.extend(eframe::egui::Shape::dashed_line(
        &pts,
        stroke,
        (4.0 * z).max(2.0),
        (3.0 * z).max(1.5),
    ));
}

/// Card corner radius for the TS rack nodes, clamped for egui.
pub fn radius(z: f32) -> CornerRadius {
    CornerRadius::same((6.0 * z).min(255.0) as u8)
}

/// Hover lift: a soft drop shadow under a node (stronger when hovered).
pub fn lift(p: &Painter, r: Rect, hovered: bool, z: f32) {
    let shadow = eframe::egui::Shadow {
        offset: [0, if hovered { 4 } else { 2 }],
        blur: if hovered { 14 } else { 8 },
        spread: 0,
        color: Color32::from_black_alpha(if hovered { 90 } else { 60 }),
    };
    p.add(shadow.as_shape(r, radius(z)));
}

/// Whether the bare range field at `r` should look like an input.
pub fn bare_hot(hover: Option<Pos2>, r: Rect) -> bool {
    hover.is_some_and(|p| r.expand(2.0).contains(p))
}
