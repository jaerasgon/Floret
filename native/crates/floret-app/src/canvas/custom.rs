//! Custom node bodies on the canvas — the TS `src/editor/nodes/*`
//! components: where their controls sit inside the node box, how they're
//! painted, and what the canvas hit-tests. Everything scales with the
//! zoom; text is dropped when it gets too small to read. The value maths
//! is in `editor::widgets`.

use super::geom::{
    row_box, row_y, Ctx, Field, NodeBox, PinGeom, Row, HEADER_H, LABEL_FONT, NODE_W, ROW_H,
};
use crate::editor::ports::{self, Side};
use crate::editor::values::{self, EditKey, FieldKind};
use crate::editor::widgets::{self, Custom, Pad, ProbeView, SliderState, Slots};
use crate::theme::{
    self, Token, BG_0, BG_1, BG_3, BORDER, FG_0, FG_1, FG_2, GOLD, GOLD_TEXT, SPREAD_PIN,
};
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
const PICK_BLUE: Token = theme::BLUE;
const AMBER: Token = theme::AMBER;
/// `.comment-node` ink and paper.
const COMMENT_INK: Token = theme::COMMENT_INK;
const COMMENT: Token = theme::COMMENT;

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
    /// The header's bypass button: whether the node is bypassed.
    Bypass(bool),
    /// A text pin's `…` file button: the pin.
    Browse(String),
    /// The Effect node's curve editor (drag a point; double-click resets).
    Curve(widgets::Curves),
    /// A channel tab over the curve editor (0 = RGB).
    CurveChannel {
        channel: u8,
        on: bool,
    },
}

impl WidgetKind {
    pub fn interactive(&self) -> bool {
        !matches!(self, WidgetKind::Slots { to: None, .. })
    }

    /// Driven by press-and-drag rather than a click.
    pub fn drags(&self) -> bool {
        matches!(
            self,
            WidgetKind::Slider(_)
                | WidgetKind::Pad(_)
                | WidgetKind::Hold
                | WidgetKind::Grip
                | WidgetKind::Curve(_)
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
        Custom::Video => {
            if widgets::video_name(n).is_some() {
                46.0
            } else {
                30.0
            }
        }
        Custom::Font => {
            if font_file(n).is_empty() {
                30.0
            } else {
                46.0
            }
        }
        Custom::Expression => expression_box_h(n) + 10.0,
        Custom::Curves => CURVES_BODY,
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
        // Generated palettes use the standard rows (mode, their pins).
        Custom::Palette if widgets::palette_generates(n) => None,
        // The mode, a row per stop, the output.
        Custom::Palette => Some(widgets::slot_count(n, widgets::PALETTE_STOPS) as usize + 2),
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
        Custom::Palette if widgets::palette_generates(n) => {
            row_box(cx, n, spec, wired, b);
            // A cosine palette's colour count is `stopCount`, like the stops.
            if n.params.get("mode").and_then(Json::as_str) == Some("cosine") {
                slots(cx, n, widgets::PALETTE_STOPS, b);
            }
        }
        Custom::Palette => {
            palette(cx, n, spec, wired, b);
            slots(cx, n, widgets::PALETTE_STOPS, b);
        }
        Custom::Expression => {
            b.fields.push(Field {
                multiline: true,
                ..param_field(
                    n,
                    spec,
                    at(
                        b,
                        z,
                        10.0,
                        HEADER_H + 5.0,
                        NODE_W - 20.0,
                        expression_box_h(n),
                    ),
                    "expr",
                    FieldKind::Text,
                )
            });
            row_box(cx, n, spec, wired, b);
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
        Custom::Video => {
            video_body(n, HEADER_H + 5.0, z, b);
            row_box(cx, n, spec, wired, b);
        }
        Custom::Font => {
            font_body(n, HEADER_H + 5.0, z, b);
            row_box(cx, n, spec, wired, b);
        }
        Custom::Curves => {
            curves_body(cx, n, spec, wired, b);
            row_box(cx, n, spec, wired, b);
        }
        Custom::Probe => {}
    }
}

/// The curve editor's body: channel tabs over the graph.
pub const CURVES_BODY: f32 = 116.0;
const CURVE_TAB_H: f32 = 15.0;
const CURVE_GRAPH_H: f32 = 84.0;

/// `gpu/effect` in curves mode: RGB / R / G / B tabs, then the curve
/// graph over the five colour points (their pins stay below as rows).
fn curves_body(
    cx: &Ctx,
    n: &PatchNode,
    spec: &NodeSpec,
    wired: &HashSet<(&str, &str)>,
    b: &mut NodeBox,
) {
    let z = cx.zoom;
    let channel = cx.decor.curve_channels.get(&n.id).copied().unwrap_or(0);
    let inputs = cx.cat.resolved_inputs(spec, &n.params);
    let mut c = widgets::Curves {
        channel,
        points: [[0.0; 3]; 5],
        wired: [false; 5],
    };
    for (i, name) in widgets::CURVE_PORTS.iter().enumerate() {
        let port = inputs.iter().find(|p| p.name == *name);
        let v = values::current(n, spec, &EditKey::Port((*name).to_owned()), port);
        let [r, g, bl, _] = values::as_rgba(&v);
        c.points[i] = [r, g, bl];
        c.wired[i] = wired.contains(&(n.id.as_str(), *name));
    }
    let w = b.rect.width() / z - 20.0;
    let top = HEADER_H + 6.0;
    let mut x = 10.0;
    for ch in 0..widgets::CURVE_CHANNELS.len() {
        let tw = if ch == 0 { 32.0 } else { 20.0 };
        b.widgets.push(Widget {
            rect: at(b, z, x, top, tw, CURVE_TAB_H),
            node: n.id.clone(),
            kind: WidgetKind::CurveChannel {
                channel: ch as u8,
                on: ch as u8 == channel,
            },
        });
        x += tw + 2.0;
    }
    b.widgets.push(Widget {
        rect: at(b, z, 10.0, top + CURVE_TAB_H + 5.0, w, CURVE_GRAPH_H),
        node: n.id.clone(),
        kind: WidgetKind::Curve(c),
    });
}

/// A curve point's position in the graph `r` (screen space).
pub fn curve_point(r: Rect, i: usize, level: f64) -> Pos2 {
    pos2(
        r.left() + r.width() * widgets::Curves::x(i) as f32,
        r.bottom() - r.height() * level as f32,
    )
}

/// The draggable point nearest `p` in the graph `r`, if one is close
/// (wired points don't move).
pub fn curve_hit(c: &widgets::Curves, r: Rect, p: Pos2, z: f32) -> Option<usize> {
    (0..5)
        .filter(|&i| !c.wired[i])
        .map(|i| (i, curve_point(r, i, c.level(i)).distance(p)))
        .filter(|(_, d)| *d <= 12.0 * z.max(0.5))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Channel ink: red, green, blue (deeper on the light theme).
fn channel_color(c: usize) -> Color32 {
    match c {
        0 => *theme::CURVE_RED,
        1 => *theme::CURVE_GREEN,
        _ => *theme::CURVE_BLUE,
    }
}

/// The curve graph: quarter grid, the identity line, each channel's
/// curve (the selected tab's bold), and the five points.
fn draw_curves(p: &Painter, r: Rect, c: &widgets::Curves, st: &BodyState, z: f32) {
    p.rect(
        r,
        3.0 * z,
        BG_0,
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    let grid = Stroke::new(1.0, BORDER.gamma_multiply(0.7));
    for k in 1..4 {
        let t = k as f32 / 4.0;
        let x = r.left() + r.width() * t;
        let y = r.top() + r.height() * t;
        p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], grid);
        p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], grid);
    }
    p.line_segment(
        [r.left_bottom(), r.right_top()],
        Stroke::new(1.0, theme::FG_3.gamma_multiply(0.6)),
    );
    let clip = p.with_clip_rect(r.expand(7.0 * z.max(0.6)));
    let path = |ys: [f64; 5]| -> Vec<Pos2> {
        (0..=48)
            .map(|k| {
                let x = f64::from(k) / 48.0;
                pos2(
                    r.left() + r.width() * x as f32,
                    r.bottom() - r.height() * widgets::curve_at(ys, x) as f32,
                )
            })
            .collect()
    };
    let selected = c.channels();
    for ch in 0..3 {
        let ys = std::array::from_fn(|i| c.points[i][ch]);
        let on = c.channel == 0 || selected.contains(&ch);
        let (w, a) = if c.channel == 0 {
            (1.5, 0.85)
        } else if on {
            (2.0, 1.0)
        } else {
            (1.0, 0.3)
        };
        clip.line(
            path(ys),
            Stroke::new(w * z.max(0.6), channel_color(ch).gamma_multiply(a)),
        );
    }
    if c.channel == 0 {
        // The RGB tab's handles sit on the mean of the three.
        let ys = std::array::from_fn(|i| c.level(i));
        clip.line(
            path(ys),
            Stroke::new(1.0 * z.max(0.6), FG_1.gamma_multiply(0.5)),
        );
    }
    let hot = st.hover.and_then(|h| curve_hit(c, r, h, z));
    for i in 0..5 {
        let at = curve_point(r, i, c.level(i));
        let ink = if c.channel == 0 {
            *FG_0
        } else {
            channel_color(usize::from(c.channel - 1))
        };
        let rad = if hot == Some(i) { 5.0 } else { 4.0 } * z.max(0.6);
        if c.wired[i] {
            clip.circle(at, rad, BG_0, Stroke::new(1.5, ink.gamma_multiply(0.6)));
        } else {
            clip.circle(at, rad, ink, Stroke::new(1.5 * z.max(0.6), BG_0));
        }
    }
}

/// Pick / clear, and the picked video's name.
fn video_body(n: &PatchNode, top: f32, z: f32, b: &mut NodeBox) {
    let name = widgets::video_name(n);
    b.widgets.push(Widget {
        rect: at(b, z, 10.0, top, 92.0, 19.0),
        node: n.id.clone(),
        kind: WidgetKind::Pick {
            svg: false,
            replace: name.is_some(),
        },
    });
    let Some(name) = name else {
        return;
    };
    b.widgets.push(Widget {
        rect: at(b, z, 106.0, top, 19.0, 19.0),
        node: n.id.clone(),
        kind: WidgetKind::Clear,
    });
    b.notes.push(Note {
        rect: at(b, z, 10.0, top + 23.0, NODE_W - 20.0, 13.0),
        text: name,
        role: NoteRole::Label,
    });
}

fn font_file(n: &PatchNode) -> &str {
    n.params.get("file").and_then(Json::as_str).unwrap_or("")
}

/// `text/font`: pick / clear a font file; the file's name underneath.
fn font_body(n: &PatchNode, top: f32, z: f32, b: &mut NodeBox) {
    let path = font_file(n);
    let picked = !path.is_empty();
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
            svg: false,
            replace: picked,
        },
    );
    if !picked {
        return;
    }
    push(b, at(b, z, 106.0, top, 19.0, 19.0), WidgetKind::Clear);
    let name = std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |f| f.to_string_lossy().into_owned());
    b.notes.push(Note {
        rect: at(b, z, 10.0, top + 23.0, NODE_W - 20.0, 13.0),
        text: name,
        role: NoteRole::Label,
    });
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
    let font = if b.math {
        FontId::monospace(13.0)
    } else {
        theme::medium(12.5)
    };
    let label_w = cx
        .painter
        .layout_no_wrap(b.label.clone(), font, Color32::WHITE)
        .size()
        .x;
    let x = super::draw::TITLE_X + label_w + 8.0;
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

/// The expression node's formula box: tall enough for the formula
/// wrapped at about 24 characters a line, 2 to 6 lines.
fn expression_box_h(n: &PatchNode) -> f32 {
    let len = n
        .params
        .get("expr")
        .and_then(Json::as_str)
        .map_or(0, |s| s.chars().count());
    len.div_ceil(24).clamp(2, 6) as f32 * 11.0 + 6.0
}

/// Palette rows: the mode, then one pin + hex swatch per stop, the spread
/// output last.
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
    // Row 0: the mode dropdown (stops, cosine, harmonies).
    {
        let y = row_y(b, 0, z);
        let mut row = empty_row(y);
        let label = "mode".to_owned();
        // Placed like a generic node's param row (`row_box`).
        let lw = cx.text_w(&label, LABEL_FONT);
        row.label_rect = Rect::from_min_size(
            pos2(b.rect.left() + 8.0 * z, y - ROW_H * z / 2.0),
            vec2(lw + 4.0, ROW_H) * z,
        );
        row.label = Some(label);
        let opts = spec.param_options.get("mode").cloned().unwrap_or_default();
        let rel = (y - b.rect.top()) / z;
        let x0 = 10.0 + lw + 6.0;
        b.fields.push(param_field(
            n,
            spec,
            at(b, z, x0, rel - 7.5, (NODE_W - 10.0 - x0).min(100.0), 15.0),
            "mode",
            FieldKind::Select(opts),
        ));
        b.rows.push(row);
    }
    for (i, port) in inputs.iter().enumerate() {
        let y = row_y(b, i + 1, z);
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
        let y = row_y(b, inputs.len() + 1, z);
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
                        *AMBER
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
            WidgetKind::Browse(_) => {
                p.rect(
                    r,
                    3.0 * z,
                    if hot { *BG_3 } else { *BG_1 },
                    Stroke::new(1.0, if hot { *theme::FG_3 } else { *BORDER }),
                    StrokeKind::Inside,
                );
                if text {
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        "…",
                        FontId::proportional(11.0 * z),
                        if hot { *FG_0 } else { *FG_1 },
                    );
                }
            }
            WidgetKind::Curve(c) => draw_curves(p, r, c, st, z),
            WidgetKind::CurveChannel { channel, on } => {
                let (fill, ink) = if *on {
                    (GOLD.gamma_multiply(0.22), *FG_0)
                } else if hot {
                    (*BG_3, *FG_0)
                } else {
                    (Color32::TRANSPARENT, *FG_1)
                };
                p.rect(
                    r,
                    3.0 * z,
                    fill,
                    Stroke::new(
                        1.0,
                        if *on {
                            GOLD.gamma_multiply(0.6)
                        } else {
                            *BORDER
                        },
                    ),
                    StrokeKind::Inside,
                );
                if text {
                    let ch = usize::from(*channel);
                    let ink = if ch == 0 || !*on {
                        ink
                    } else {
                        channel_color(ch - 1)
                    };
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        widgets::CURVE_CHANNELS[ch],
                        theme::medium(9.5 * z),
                        ink,
                    );
                }
            }
            WidgetKind::Grip | WidgetKind::Bypass(_) => {}
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
                *GOLD_TEXT,
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
        *theme::PROBE_EDGE
    };
    let card = if theme::is_light() {
        Color32::from_rgba_unmultiplied(255, 255, 255, 200)
    } else {
        Color32::from_rgba_unmultiplied(16, 19, 24, 184)
    };
    p.rect_filled(r, radius(z), card);
    if st.selected {
        p.rect_stroke(r, radius(z), Stroke::new(1.0, edge), StrokeKind::Inside);
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
        *FG_2,
    );
    p.with_clip_rect(head).text(
        head.right_center() - vec2(6.0 * z, 0.0),
        Align2::RIGHT_CENTER,
        view.meta(),
        FontId::monospace(8.0 * z),
        *SPREAD_PIN,
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
            *FG_2,
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
            radius(z),
            COMMENT.gamma_multiply(0.12),
            Stroke::new(1.0, COMMENT.gamma_multiply(0.7)),
            StrokeKind::Inside,
        );
    } else {
        p.rect_filled(r, radius(z), COMMENT.gamma_multiply(0.08));
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
        ("double-click to edit", COMMENT_INK.gamma_multiply(0.6))
    } else {
        (text, *COMMENT_INK)
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
    let pts = rounded_outline(r, f32::from(radius(z).nw));
    p.extend(eframe::egui::Shape::dashed_line(
        &pts,
        stroke,
        (4.0 * z).max(2.0),
        (3.0 * z).max(1.5),
    ));
}

/// A closed outline of `r` with corners rounded by `radius` (a quarter
/// circle each), clockwise from the top edge.
fn rounded_outline(r: Rect, radius: f32) -> Vec<Pos2> {
    use std::f32::consts::FRAC_PI_2;
    let rad = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    let corners = [
        (pos2(r.right() - rad, r.top() + rad), -FRAC_PI_2),
        (pos2(r.right() - rad, r.bottom() - rad), 0.0),
        (pos2(r.left() + rad, r.bottom() - rad), FRAC_PI_2),
        (pos2(r.left() + rad, r.top() + rad), 2.0 * FRAC_PI_2),
    ];
    let steps = 6;
    let mut pts = vec![pos2(r.left() + rad, r.top())];
    for (c, start) in corners {
        for k in 0..=steps {
            let a = start + FRAC_PI_2 * k as f32 / steps as f32;
            pts.push(c + vec2(a.cos(), a.sin()) * rad);
        }
    }
    pts.push(pos2(r.left() + rad, r.top()));
    pts
}

/// Card corner radius for nodes, probes and comments, clamped for egui.
pub fn radius(z: f32) -> CornerRadius {
    CornerRadius::same((6.0 * z).min(255.0) as u8)
}

/// Hover lift: a soft drop shadow under a node (a little deeper when
/// hovered).
pub fn lift(p: &Painter, r: Rect, hovered: bool, z: f32) {
    let shadow = eframe::egui::Shadow {
        offset: [0, if hovered { 3 } else { 1 }],
        blur: if hovered { 12 } else { 6 },
        spread: 0,
        color: Color32::from_black_alpha(if hovered { 64 } else { 36 }),
    };
    p.add(shadow.as_shape(r, radius(z)));
}

/// Whether the bare range field at `r` should look like an input.
pub fn bare_hot(hover: Option<Pos2>, r: Rect) -> bool {
    hover.is_some_and(|p| r.expand(2.0).contains(p))
}
