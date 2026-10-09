//! `style/*` nodes. Ported from `src/nodes/style/` (`fill.ts`, `modes.ts`,
//! `text-params.ts`).
//!
//! A TS style node returns a closure `(s) => ({...applyBase(base, s), …})`;
//! here it returns a [`Style`]: the base chain's ops followed by its own.

use super::color::{hex_to_color01, hsl_to_color01};
use super::convert::js_string;
use super::shared::{first, to_color, to_vec2};
use super::Registrar;
use crate::js;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::scene::{
    Gradient, GradientExtend, GradientShape, GradientStop, GradientUnits, Paint, Style, StyleOp,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Color, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("style/fill", fill);
    r.node("style/stroke", stroke);
    r.node("style/stroke-weight", stroke_weight);
    r.node("style/no-fill", no_fill);
    r.node("style/no-stroke", no_stroke);
    r.node("style/blur", blur);
    r.node("style/rect-mode", rect_mode);
    r.node("style/ellipse-mode", ellipse_mode);
    r.node("style/stroke-cap", stroke_cap);
    r.node("style/stroke-join", stroke_join);
    r.node("style/text-params", text_params);
    r.node("style/gradient", gradient);
    r.dynamic("style/gradient", gradient_inputs);
}

// ─── Shared helpers (also used by draw/*) ────────────────────────────────────

/// A style wire's value. TS tests truthiness (`if (!style)`) and calls it;
/// only an actual style chain is callable, so anything else counts as none.
pub(crate) fn style_arg(v: &Value) -> Option<&Style> {
    match v {
        Value::Style(s) => Some(s),
        _ => None,
    }
}

/// `colorToCss(c)` on whatever arrived on a colour wire. Non-`Color`
/// values read their `r/g/b/a` fields like JS would (missing → NaN).
///
/// Errors where TS throws: reading a field of `null` / `undefined`, and
/// `a.toFixed(3)` when the clamped alpha is not a number (`clamp01` passes
/// a non-number through unless it compares < 0 or > 1; `a >= 1` coerces).
pub(crate) fn paint_of(v: &Value) -> Result<Paint, String> {
    let c = match v {
        Value::Color(c) => *c,
        Value::Undefined | Value::Null => {
            return Err(format!("Cannot read properties of {v:?} (reading 'r')"));
        }
        other => {
            let ch = |k: &str| other.get(k).to_number();
            let a = match other.get("a") {
                Value::Num(a) => a,
                a => {
                    let n = a.to_number();
                    if n < 0.0 {
                        0.0
                    } else if n >= 1.0 {
                        // `> 1` clamps to 1; `== 1` stays raw but `>= 1` holds.
                        1.0
                    } else {
                        return Err("a.toFixed is not a function".into());
                    }
                }
            };
            Color {
                r: ch("r"),
                g: ch("g"),
                b: ch("b"),
                a,
            }
        }
    };
    Ok(Paint::from_color(c))
}

/// `(v as string) ?? fallback` for the string-valued style fields.
fn str_or(v: &Value, fallback: &str) -> Arc<str> {
    match v {
        Value::Undefined | Value::Null => Arc::from(fallback),
        Value::Str(s) => s.clone(),
        other => Arc::from(js_string(other)),
    }
}

/// One output: the base chain (`args[base]`) followed by `ops`.
fn chained(args: &[Value], base: usize, ops: Vec<StyleOp>) -> Vec<Value> {
    vec![Value::Style(Style::chain(style_arg(&args[base]), ops))]
}

// ─── style/fill ──────────────────────────────────────────────────────────────

/// `params.param<number>(key, fallback)`, coerced by the arithmetic that
/// consumes it.
fn param_num(param: ParamFn, key: &str, fallback: f64) -> f64 {
    param(key).unwrap_or(Value::Num(fallback)).to_number()
}

/// A node's params by key, as values (live at eval time, or a saved node's
/// in the editor).
type ParamFn<'a> = &'a dyn Fn(&str) -> Option<Value>;

/// `deriveLegacyColor`: the pre-Color-node Fill modes.
fn legacy_color(mode: &str, param: ParamFn) -> Color {
    match mode {
        "hsl" => hsl_to_color01(
            param_num(param, "h", 0.0),
            param_num(param, "s", 50.0),
            param_num(param, "l", 50.0),
            1.0,
        ),
        "hex" => hex_to_color01(&param("hex").unwrap_or_else(|| Value::str("#ffffff"))),
        _ => Color {
            r: param_num(param, "r", 255.0) / 255.0,
            g: param_num(param, "g", 255.0) / 255.0,
            b: param_num(param, "b", 255.0) / 255.0,
            a: param_num(param, "a", 255.0) / 255.0,
        },
    }
}

/// The colour a Fill paints while nothing sets its `color` pin: a legacy
/// multi-mode Fill (`params.mode` set) replays its old colour, any other
/// paints white.
fn fill_color(param: ParamFn) -> Color {
    match param("mode") {
        Some(m) if m.truthy() => {
            let m = match &m {
                Value::Str(s) => s.to_string(),
                _ => String::new(),
            };
            legacy_color(&m, param)
        }
        _ => Color::WHITE,
    }
}

/// Stroke's colour while its `color` pin is unset.
const STROKE_DEFAULT: Color = Color::BLACK;

/// What an unwired colour pin with no value of its own paints with, for
/// the style nodes whose `color` declares no default: Fill's white (or a
/// legacy Fill's old colour), Stroke's black. The editor shows this in the
/// pin's row, so the row and the picture agree. `None` for any other pin.
pub fn unwired_color(def_type: &str, port: &str, params: &Params) -> Option<Color> {
    let param = |k: &str| params.get(k).map(Value::from_json);
    match (def_type, port) {
        ("style/fill", "color") => Some(fill_color(&param)),
        ("style/stroke", "color") => Some(STROKE_DEFAULT),
        _ => None,
    }
}

fn fill(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let wired = first(inputs, 0);
        let base = first(inputs, 1);
        let paint = if wired.truthy() {
            paint_of(&wired)?
        } else {
            Paint::from_color(fill_color(&|k| cx.param(k).map(Value::from_json)))
        };
        let style = Style::chain(style_arg(&base), [StyleOp::Fill(Some(paint))]);
        Ok(vec![one(Value::Style(style))])
    })
}

// ─── style/stroke, stroke-weight, no-fill, no-stroke, blur ───────────────────

fn stroke(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let paint = if args[0].is_nullish() {
            Paint::from_color(STROKE_DEFAULT)
        } else {
            paint_of(&args[0])?
        };
        let w = args[1].to_number();
        let align = match &args[2] {
            Value::Str(s) if &**s == "inside" || &**s == "outside" => s.clone(),
            _ => Arc::from("center"),
        };
        Ok(chained(
            args,
            3,
            vec![
                StyleOp::Stroke(Some(paint)),
                StyleOp::StrokeWeightIfPositive(w),
                StyleOp::StrokeAlign(align),
            ],
        ))
    })
}

fn stroke_weight(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(chained(
            args,
            1,
            vec![StyleOp::StrokeWeight(args[0].to_number())],
        ))
    })
}

fn no_fill(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| Ok(chained(args, 0, vec![StyleOp::Fill(None)])))
}

fn no_stroke(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(chained(args, 0, vec![StyleOp::Stroke(None)]))
    })
}

fn blur(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let a = js::max(0.0, args[0].to_number());
        Ok(chained(args, 1, vec![StyleOp::Blur(a)]))
    })
}

// ─── Mode styles (modes.ts) ──────────────────────────────────────────────────

fn rect_mode(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m = str_or(&args[0], "corner");
        Ok(chained(args, 1, vec![StyleOp::RectMode(m)]))
    })
}

fn ellipse_mode(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m = str_or(&args[0], "center");
        Ok(chained(args, 1, vec![StyleOp::EllipseMode(m)]))
    })
}

fn stroke_cap(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let c = str_or(&args[0], "round");
        Ok(chained(args, 1, vec![StyleOp::StrokeCap(c)]))
    })
}

fn stroke_join(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let j = str_or(&args[0], "miter");
        Ok(chained(args, 1, vec![StyleOp::StrokeJoin(j)]))
    })
}

// ─── style/text-params ───────────────────────────────────────────────────────

/// `String.prototype.trim`: JS whitespace is Unicode White_Space minus
/// U+0085, plus the BOM.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// `Number(v) || fallback`.
fn number_or(v: &Value, fallback: f64) -> f64 {
    let n = v.to_number();
    if n.is_nan() || n == 0.0 {
        fallback
    } else {
        n
    }
}

fn text_params(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        // `((font as string) || 'sans-serif').trim() || 'sans-serif'`
        // A truthy non-string has no `.trim` (TS throws).
        let family = match &args[0] {
            v if !v.truthy() => "sans-serif".to_owned(),
            Value::Str(s) => js_trim(s).to_owned(),
            _ => return Err("font.trim is not a function".into()),
        };
        let family = if family.is_empty() {
            "sans-serif".to_owned()
        } else {
            family
        };
        let weight = if args[1].truthy() { "bold" } else { "normal" };
        let font_style = if args[2].truthy() { "italic" } else { "normal" };
        let align = str_or(&args[3], "left");
        let baseline = str_or(&args[4], "alphabetic");
        let ls = js::max(0.0, number_or(&args[5], 0.0));
        let lh = js::max(0.0, number_or(&args[6], 1.0));
        Ok(chained(
            args,
            7,
            vec![
                StyleOp::FontFamily(Arc::from(family)),
                StyleOp::FontWeight(Arc::from(weight)),
                StyleOp::FontStyle(Arc::from(font_style)),
                StyleOp::TextAlign(align),
                StyleOp::TextBaseline(baseline),
                StyleOp::LetterSpacing(ls),
                StyleOp::LineHeight(lh),
            ],
        ))
    })
}

// ─── style/gradient ──────────────────────────────────────────────────────────

fn vec2_port(name: &str, x: f64, y: f64) -> PortSpec {
    PortSpec::new(name, PortType::Vec2).default(Value::Vec2(Vec2 { x, y }))
}

/// `colors` and `positions` (the stops), the mode's two geometry pins,
/// then `base`.
fn gradient_inputs(params: &Params) -> Vec<PortSpec> {
    let mode = match params.get("mode") {
        Some(serde_json::Value::String(s)) => s.as_str(),
        _ => "linear",
    };
    let geometry = match mode {
        "radial" => [
            vec2_port("center", 0.0, 0.0),
            PortSpec::new("radius", PortType::Number).default(Value::Num(1.0)),
        ],
        "conic" => [
            vec2_port("center", 0.0, 0.0),
            PortSpec::new("angle", PortType::Number).default(Value::Num(0.0)),
        ],
        _ => [vec2_port("start", -1.0, 0.0), vec2_port("end", 1.0, 0.0)],
    };
    let mut ports = vec![
        PortSpec::new("colors", PortType::Color).kind(PortKind::Spread),
        PortSpec::new("positions", PortType::Number).kind(PortKind::Spread),
    ];
    ports.extend(geometry);
    ports.push(PortSpec::new("base", PortType::Style));
    ports
}

/// Stops from a colour spread and optional positions: evenly spaced unless
/// a finite position is given, clamped to 0..1 and never decreasing (like
/// CSS). No colours → white to black.
pub(crate) fn gradient_stops(colors: &[Value], positions: &[Value]) -> Vec<GradientStop> {
    let colors: Vec<Color> = if colors.is_empty() {
        vec![Color::WHITE, Color::BLACK]
    } else {
        colors.iter().map(to_color).collect()
    };
    let n = colors.len();
    let mut prev = 0.0f64;
    colors
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let even = if n > 1 {
                i as f64 / (n - 1) as f64
            } else {
                0.0
            };
            let pos = positions
                .get(i)
                .and_then(Value::as_f64)
                .filter(|p| p.is_finite())
                .unwrap_or(even)
                .clamp(0.0, 1.0)
                .max(prev);
            prev = pos;
            // Quantised like a flat Fill colour.
            let rgba = Paint::from_color(*c)
                .rgba
                .map(|v| if v.is_finite() { v } else { 0.0 });
            GradientStop { pos, rgba }
        })
        .collect()
}

fn gradient(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let spread = |i: usize| inputs.get(i).map_or(&[][..], |s| &s[..]);
        let num = |i: usize, d: f64| match first(inputs, i) {
            v if v.is_nullish() => d,
            v => v.to_number(),
        };
        let vec2 = |i: usize, x: f64, y: f64| match first(inputs, i) {
            v if v.is_nullish() => Vec2 { x, y },
            v => to_vec2(&v),
        };
        let param = |k: &str, d: &str| match cx.param(k) {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => d.to_owned(),
        };
        let shape = match param("mode", "linear").as_str() {
            "radial" => GradientShape::Radial {
                center: vec2(2, 0.0, 0.0),
                radius: num(3, 1.0),
            },
            "conic" => GradientShape::Conic {
                center: vec2(2, 0.0, 0.0),
                angle: num(3, 0.0),
            },
            _ => GradientShape::Linear {
                start: vec2(2, -1.0, 0.0),
                end: vec2(3, 1.0, 0.0),
            },
        };
        let units = match param("units", "shape").as_str() {
            "canvas" => GradientUnits::Scene,
            _ => GradientUnits::Box,
        };
        let extend = match param("extend", "pad").as_str() {
            "repeat" => GradientExtend::Repeat,
            "mirror" => GradientExtend::Mirror,
            _ => GradientExtend::Pad,
        };
        let paint = Paint::gradient(Gradient {
            shape,
            stops: Arc::from(gradient_stops(spread(0), spread(1))),
            units,
            extend,
        });
        let op = match param("target", "fill").as_str() {
            "stroke" => StyleOp::Stroke(Some(paint)),
            _ => StyleOp::Fill(Some(paint)),
        };
        let base = first(inputs, 4);
        Ok(vec![one(Value::Style(Style::chain(
            style_arg(&base),
            [op],
        )))])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::node;
    use super::*;
    use crate::scene::{default_style, neutral_style, resolve_style};
    use crate::spec::Catalogue;
    use serde_json::json;

    fn col(r: f64, g: f64, b: f64) -> Value {
        Value::Color(Color { r, g, b, a: 1.0 })
    }

    fn resolved(v: &Value) -> crate::scene::ResolvedStyle {
        match v {
            Value::Style(s) => s.apply(neutral_style()),
            other => panic!("not a style: {other:?}"),
        }
    }

    #[test]
    fn gradient_pins_follow_the_mode() {
        let cat = Catalogue::builtin();
        let spec = cat.get("style/gradient").unwrap();
        let names = |mode: &str| -> Vec<String> {
            let params: Params = serde_json::from_value(json!({ "mode": mode })).unwrap();
            cat.resolved_inputs(spec, &params)
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(
            names("linear"),
            ["colors", "positions", "start", "end", "base"]
        );
        assert_eq!(
            names("radial"),
            ["colors", "positions", "center", "radius", "base"]
        );
        assert_eq!(
            names("conic"),
            ["colors", "positions", "center", "angle", "base"]
        );
    }

    #[test]
    fn stops_are_even_unless_placed() {
        let rgb = [col(1.0, 0.0, 0.0), col(0.0, 1.0, 0.0), col(0.0, 0.0, 1.0)];
        let s = gradient_stops(&rgb, &[]);
        let pos = |s: &[GradientStop]| s.iter().map(|s| s.pos).collect::<Vec<_>>();
        assert_eq!(pos(&s), [0.0, 0.5, 1.0]);
        assert_eq!(s[1].rgba, [0.0, 1.0, 0.0, 1.0]);
        // Given positions are clamped and never go backwards; missing ones
        // fall back to even spacing.
        let s = gradient_stops(&rgb, &[Value::Num(0.8), Value::Num(0.2)]);
        assert_eq!(pos(&s), [0.8, 0.8, 1.0]);
        let s = gradient_stops(&[], &[]);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].rgba, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn gradient_sets_the_fill_or_the_stroke() {
        let mut h = node(
            "style/gradient",
            json!({ "mode": "radial", "units": "canvas" }),
        );
        let out = h
            .eval(vec![
                vec![col(1.0, 1.0, 1.0), col(0.0, 0.0, 0.0)],
                vec![],
                vec![Value::Vec2(Vec2 { x: 10.0, y: 20.0 })],
                vec![Value::Num(50.0)],
                vec![],
            ])
            .unwrap();
        let st = resolved(&out[0][0]);
        let g = st.fill.as_ref().and_then(|p| p.gradient.clone()).unwrap();
        assert_eq!(
            g.shape,
            GradientShape::Radial {
                center: Vec2 { x: 10.0, y: 20.0 },
                radius: 50.0
            }
        );
        assert_eq!(g.units, GradientUnits::Scene);
        assert_eq!(g.extend, GradientExtend::Pad);
        assert!(st.stroke.is_none());
        // Stroke target, chained on a base that set a flat fill.
        let base = Style::chain(None, [StyleOp::Fill(Some(Paint::css("#ff0000")))]);
        let mut h = node(
            "style/gradient",
            json!({ "target": "stroke", "extend": "mirror" }),
        );
        let out = h
            .eval(vec![
                vec![],
                vec![],
                vec![],
                vec![],
                vec![Value::Style(base)],
            ])
            .unwrap();
        let st = resolved(&out[0][0]);
        assert_eq!(st.fill.as_ref().unwrap().css.as_ref(), "#ff0000");
        let g = st.stroke.as_ref().and_then(|p| p.gradient.clone()).unwrap();
        assert_eq!(g.extend, GradientExtend::Mirror);
        assert_eq!(
            g.shape,
            GradientShape::Linear {
                start: Vec2 { x: -1.0, y: 0.0 },
                end: Vec2 { x: 1.0, y: 0.0 }
            }
        );
        let Value::Style(s) = &out[0][0] else {
            unreachable!()
        };
        assert!(resolve_style(Some(s), default_style()).stroke.is_some());
    }

    /// The colour the editor shows for an unset `color` pin is the one the
    /// node paints with.
    #[test]
    fn unwired_colour_is_what_fill_and_stroke_paint() {
        let cases = [
            ("style/fill", json!({})),
            ("style/fill", json!({"mode": "color"})),
            (
                "style/fill",
                json!({"mode": "rgb", "r": 220, "g": 100, "b": 80, "a": 255}),
            ),
            ("style/fill", json!({"mode": "hsl", "h": 200, "l": 30})),
            ("style/fill", json!({"mode": "hex", "hex": "#336699"})),
            ("style/stroke", json!({})),
            ("style/stroke", json!({"align": "inside"})),
        ];
        for (ty, params) in cases {
            let p: Params = serde_json::from_value(params.clone()).unwrap();
            let want = Paint::from_color(unwired_color(ty, "color", &p).unwrap());
            let pins = Catalogue::builtin().get(ty).unwrap().inputs.len();
            let out = node(ty, params)
                .eval(vec![vec![Value::Undefined]; pins])
                .unwrap();
            let st = resolved(&out[0][0]);
            let got = if ty == "style/fill" {
                st.fill
            } else {
                st.stroke
            };
            assert_eq!(
                format!("{got:?}"),
                format!("{:?}", Some(want)),
                "{ty} {p:?}"
            );
        }
        let white = unwired_color("style/fill", "color", &Params::new());
        assert_eq!(white, Some(Color::WHITE));
        let black = unwired_color("style/stroke", "color", &Params::new());
        assert_eq!(black, Some(Color::BLACK));
        // Pins whose unset value means "none" (an image tint) show none.
        assert!(unwired_color("draw/image", "tint", &Params::new()).is_none());
        assert!(unwired_color("style/stroke", "width", &Params::new()).is_none());
    }
}
