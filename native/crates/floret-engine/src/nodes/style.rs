//! `style/*` nodes. Ported from `src/nodes/style/` (`fill.ts`, `modes.ts`,
//! `text-params.ts`).
//!
//! A TS style node returns a closure `(s) => ({...applyBase(base, s), …})`;
//! here it returns a [`Style`]: the base chain's ops followed by its own.

use super::color::{hex_to_color01, hsl_to_color01};
use super::convert::js_string;
use super::shared::first;
use super::Registrar;
use crate::js;
use crate::node::{raw, scalar, EvalCtx, NodeImpl, NodeInit};
use crate::scene::{Paint, Style, StyleOp};
use crate::value::{one, Color, Value};
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
/// objects read their `r/g/b/a` fields like JS would (missing → NaN).
pub(crate) fn paint_of(v: &Value) -> Paint {
    match v {
        Value::Color(c) => Paint::from_color(*c),
        other => {
            let ch = |k: &str| other.get(k).to_number();
            Paint::from_color(Color {
                r: ch("r"),
                g: ch("g"),
                b: ch("b"),
                a: ch("a"),
            })
        }
    }
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
fn param_num(cx: &EvalCtx, key: &str, fallback: f64) -> f64 {
    cx.param(key)
        .map_or(Value::Num(fallback), Value::from_json)
        .to_number()
}

/// `deriveLegacyColor`: the pre-Color-node Fill modes.
fn legacy_color(mode: &str, cx: &EvalCtx) -> Color {
    match mode {
        "hsl" => hsl_to_color01(
            param_num(cx, "h", 0.0),
            param_num(cx, "s", 50.0),
            param_num(cx, "l", 50.0),
            1.0,
        ),
        "hex" => hex_to_color01(
            &cx.param("hex")
                .map_or(Value::str("#ffffff"), Value::from_json),
        ),
        _ => Color {
            r: param_num(cx, "r", 255.0) / 255.0,
            g: param_num(cx, "g", 255.0) / 255.0,
            b: param_num(cx, "b", 255.0) / 255.0,
            a: param_num(cx, "a", 255.0) / 255.0,
        },
    }
}

fn fill(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let wired = first(inputs, 0);
        let base = first(inputs, 1);
        let paint = if wired.truthy() {
            paint_of(&wired)
        } else {
            // A legacy multi-mode Fill (params.mode set) replays its colour.
            let mode = cx.param("mode").map(Value::from_json);
            match mode {
                Some(m) if m.truthy() => {
                    let m = match &m {
                        Value::Str(s) => s.to_string(),
                        _ => String::new(),
                    };
                    Paint::from_color(legacy_color(&m, cx))
                }
                _ => Paint::from_color(Color::WHITE),
            }
        };
        let style = Style::chain(style_arg(&base), [StyleOp::Fill(Some(paint))]);
        Ok(vec![one(Value::Style(style))])
    })
}

// ─── style/stroke, stroke-weight, no-fill, no-stroke, blur ───────────────────

fn stroke(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let paint = if args[0].is_nullish() {
            Paint::from_color(Color::BLACK)
        } else {
            paint_of(&args[0])
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
        let family = match &args[0] {
            v if !v.truthy() => "sans-serif".to_owned(),
            Value::Str(s) => s.trim().to_owned(),
            other => js_string(other).trim().to_owned(),
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
