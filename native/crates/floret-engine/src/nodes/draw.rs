//! `draw/*` nodes. Ported from `src/nodes/draw/` (`rect.ts`, `shapes.ts`,
//! `text.ts`, `walker-cluster.ts`).
//!
//! Each node returns one `SceneNode`. With nothing wired into `style`, a
//! shape uses its own default (white fill for filled shapes, thin black
//! stroke for lines / points / open polygons); a wired chain resolves
//! against NEUTRAL (see `scene::resolve_style`).
//!
//! Not ported yet (they need image values): `draw/image`,
//! `draw/image-region`, `draw/image-tile`.

mod stroke_offset;
mod walker_cluster;

use super::convert::js_string_or_empty;
use super::shared::{first, to_vec2};
use super::style::{paint_of, style_arg};
use super::Registrar;
use crate::js;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::scene::{
    default_style, line_default_style, resolve_style, Paint, ResolvedStyle, SceneNode,
};
use crate::value::{one, Value, Vec2};
use std::sync::Arc;
use stroke_offset::{build_offset_polygon, widths_are_variable};

pub fn register(r: &mut Registrar) {
    r.node("draw/rect", rect);
    r.node("draw/ellipse", ellipse);
    r.node("draw/line", line);
    r.node("draw/arc", arc);
    r.node("draw/triangle", triangle);
    r.node("draw/quad", quad);
    r.node("draw/point", point);
    r.node("draw/polygon", polygon);
    r.node("draw/path", path);
    r.node("draw/text", text);
    r.node("draw/walker-cluster", walker_cluster);
}

fn scene(node: SceneNode) -> Value {
    Value::Scene(Arc::new(node))
}

/// `anchorTopLeft`: the box's top-left corner for an anchor name
/// (`center` when unset; unknown names behave as `tl`).
fn anchor_top_left(pos: Vec2, w: f64, h: f64, anchor: &Value) -> (f64, f64) {
    let a = match anchor {
        Value::Undefined | Value::Null => "center",
        Value::Str(s) => s,
        _ => "",
    };
    let (mut x, mut y) = (pos.x, pos.y);
    match a {
        "center" => {
            x -= w / 2.0;
            y -= h / 2.0;
        }
        "tr" => x -= w,
        "bl" => y -= h,
        "br" => {
            x -= w;
            y -= h;
        }
        _ => {}
    }
    (x, y)
}

fn corner() -> Arc<str> {
    Arc::from("corner")
}

// ─── rect.ts ─────────────────────────────────────────────────────────────────

/// `draw/rect` and `draw/ellipse`: (pos, w, h, anchor, style).
fn boxed(args: &[Value], ellipse: bool) -> Value {
    let (w, h) = (args[1].to_number(), args[2].to_number());
    let (x, y) = anchor_top_left(to_vec2(&args[0]), w, h, &args[3]);
    let style = resolve_style(style_arg(&args[4]), default_style());
    let mode = corner();
    scene(if ellipse {
        SceneNode::Ellipse {
            x,
            y,
            w,
            h,
            mode,
            style,
        }
    } else {
        SceneNode::Rect {
            x,
            y,
            w,
            h,
            mode,
            style,
        }
    })
}

fn rect(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| Ok(vec![boxed(args, false)]))
}

fn ellipse(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| Ok(vec![boxed(args, true)]))
}

fn line(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let (a, b) = (to_vec2(&args[0]), to_vec2(&args[1]));
        Ok(vec![scene(SceneNode::Line {
            x1: a.x,
            y1: a.y,
            x2: b.x,
            y2: b.y,
            style: resolve_style(style_arg(&args[2]), line_default_style()),
        })])
    })
}

// ─── shapes.ts ───────────────────────────────────────────────────────────────

fn arc(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let (w, h) = (args[1].to_number(), args[2].to_number());
        let (x, y) = anchor_top_left(to_vec2(&args[0]), w, h, &args[5]);
        Ok(vec![scene(SceneNode::Arc {
            x,
            y,
            w,
            h,
            start: args[3].to_number(),
            stop: args[4].to_number(),
            mode: corner(),
            style: resolve_style(style_arg(&args[6]), default_style()),
        })])
    })
}

fn triangle(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let [a, b, c] = [0, 1, 2].map(|i| to_vec2(&args[i]));
        Ok(vec![scene(SceneNode::Triangle {
            pts: [a.x, a.y, b.x, b.y, c.x, c.y],
            style: resolve_style(style_arg(&args[3]), default_style()),
        })])
    })
}

fn quad(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let [a, b, c, d] = [0, 1, 2, 3].map(|i| to_vec2(&args[i]));
        Ok(vec![scene(SceneNode::Quad {
            pts: [a.x, a.y, b.x, b.y, c.x, c.y, d.x, d.y],
            style: resolve_style(style_arg(&args[4]), default_style()),
        })])
    })
}

fn point(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let p = to_vec2(&args[0]);
        Ok(vec![scene(SceneNode::Point {
            x: p.x,
            y: p.y,
            style: resolve_style(style_arg(&args[1]), line_default_style()),
        })])
    })
}

/// `(inputs[i] ?? []).map(toVec2)`.
fn points_of(inputs: &[crate::value::Spread], i: usize) -> Vec<Vec2> {
    inputs
        .get(i)
        .map_or_else(Vec::new, |s| s.iter().map(to_vec2).collect())
}

/// `(inputs[i] ?? []).map(Number)`.
fn numbers_of(inputs: &[crate::value::Spread], i: usize) -> Vec<f64> {
    inputs
        .get(i)
        .map_or_else(Vec::new, |s| s.iter().map(Value::to_number).collect())
}

/// A variable-width ribbon: a closed, filled, unstroked polygon.
fn ribbon(points: Vec<Vec2>, style: ResolvedStyle) -> Value {
    scene(SceneNode::Polygon {
        points: Arc::from(points),
        closed: true,
        style: Arc::new(style),
        tension: None,
    })
}

fn polygon(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let pts = points_of(inputs, 0);
        // `Boolean(inputs[1]?.[0] ?? true)`
        let closed_v = first(inputs, 1);
        let closed = closed_v.is_nullish() || closed_v.truthy();
        let s = first(inputs, 2);
        let widths = numbers_of(inputs, 3);
        let fallback = if closed {
            default_style()
        } else {
            line_default_style()
        };
        let resolved = resolve_style(style_arg(&s), fallback);

        if widths_are_variable(&widths) && pts.len() >= 2 {
            let outline = build_offset_polygon(&pts, &widths, resolved.stroke_weight, closed);
            let fill = resolved
                .fill
                .clone()
                .or_else(|| resolved.stroke.clone())
                .unwrap_or_else(|| Paint::css("#ffffff"));
            let style = ResolvedStyle {
                fill: Some(fill),
                stroke: None,
                ..(*resolved).clone()
            };
            return Ok(vec![one(ribbon(outline, style))]);
        }
        Ok(vec![one(scene(SceneNode::Polygon {
            points: Arc::from(pts),
            closed,
            style: resolved,
            tension: None,
        }))])
    })
}

fn path(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let pts = points_of(inputs, 0);
        let color_in = first(inputs, 1);
        // `Number(inputs[i]?.[0] ?? d)`
        let num_or = |i: usize, d: f64| {
            let v = first(inputs, i);
            if v.is_nullish() {
                d
            } else {
                v.to_number()
            }
        };
        let width_in = num_or(2, 2.0);
        let tension = num_or(3, 0.0).clamp(0.0, 1.0);
        let widths = numbers_of(inputs, 4);
        let stroke = if color_in.truthy() {
            paint_of(&color_in)?
        } else {
            Paint::css("#000000")
        };
        let base_width = js::max(0.0, width_in);
        let line_default = (**line_default_style()).clone();

        if widths_are_variable(&widths) && pts.len() >= 2 {
            let outline = build_offset_polygon(&pts, &widths, base_width, false);
            let style = ResolvedStyle {
                fill: Some(stroke),
                stroke: None,
                ..line_default
            };
            return Ok(vec![one(ribbon(outline, style))]);
        }
        Ok(vec![one(scene(SceneNode::Polygon {
            points: Arc::from(pts),
            closed: false,
            style: Arc::new(ResolvedStyle {
                stroke: Some(stroke),
                stroke_weight: base_width,
                ..line_default
            }),
            tension: Some(tension),
        }))])
    })
}

// ─── text.ts ─────────────────────────────────────────────────────────────────

fn text(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let p = to_vec2(&args[1]);
        let size = js::max(1.0, args[2].to_number());
        let align: Arc<str> = match &args[3] {
            Value::Undefined | Value::Null => Arc::from("left"),
            Value::Str(s) => s.clone(),
            other => Arc::from(super::convert::js_string(other)),
        };
        // The wired chain supplies family / weight / spacing; size and
        // alignment come from this node.
        let base = resolve_style(style_arg(&args[4]), default_style());
        let style = ResolvedStyle {
            font_size: size,
            text_align: align,
            ..(*base).clone()
        };
        Ok(vec![scene(SceneNode::Text {
            text: Arc::from(js_string_or_empty(&args[0])),
            x: p.x,
            y: p.y,
            style: Arc::new(style),
        })])
    })
}

// ─── walker-cluster.ts ───────────────────────────────────────────────────────

fn walker_cluster(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| Ok(walker_cluster::compute(inputs)))
}

#[cfg(test)]
mod tests;
