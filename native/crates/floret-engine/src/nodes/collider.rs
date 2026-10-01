//! `collider/*` nodes. Ported from `src/nodes/collider/` (incl. `_shared.ts`).
//!
//! Each node outputs a [`Collider`] handle (shape constants + `apply`),
//! which the emitter runs after integration. `particles/bounds` lives with
//! the other `particles/*` nodes.

use super::particles::{clamp, handle, memo, num_or, vec2_or};
use super::shared::first;
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::particles::{Collider, ColliderMode, ColliderShape, Handle, PolygonEdges};
use crate::scene::SceneNode;
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("collider/circle", circle);
    r.node("collider/rect", rect);
    r.node("collider/from-layer", from_layer);
}

fn collider(shape: ColliderShape, fn_name: &'static str) -> Spread {
    handle(Handle::Collider(Arc::new(Collider::new(shape, fn_name))))
}

/// Inputs shared by every collider: `mode` (constant), `restitution`,
/// `inverted` (constant), at the given input indices.
fn common(
    inputs: &[Spread],
    mode: usize,
    rest: usize,
    inverted: usize,
) -> (ColliderMode, f64, bool) {
    (
        ColliderMode::coerce(&first(inputs, mode)),
        clamp(num_or(&first(inputs, rest), 0.8), 0.0, 1.0),
        first(inputs, inverted).truthy(),
    )
}

/// `collider/circle`.
fn circle(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let center = vec2_or(&first(inputs, 0), Vec2 { x: 0.0, y: 0.0 });
        let r = crate::js::max(0.0, num_or(&first(inputs, 1), 80.0));
        let (mode, rest, inverted) = common(inputs, 2, 3, 4);
        Ok(vec![collider(
            ColliderShape::Circle {
                mode,
                rest,
                inverted,
                cx: center.x,
                cy: center.y,
                r,
            },
            "apply",
        )])
    }))
}

/// `collider/rect`.
fn rect(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let center = vec2_or(&first(inputs, 0), Vec2 { x: 0.0, y: 0.0 });
        let size = vec2_or(&first(inputs, 1), Vec2 { x: 200.0, y: 200.0 });
        let (mode, rest, inverted) = common(inputs, 2, 3, 4);
        let hw = size.x.abs() * 0.5;
        let hh = size.y.abs() * 0.5;
        Ok(vec![collider(
            ColliderShape::Rect {
                mode,
                rest,
                inverted,
                min_x: center.x - hw,
                min_y: center.y - hh,
                max_x: center.x + hw,
                max_y: center.y + hh,
            },
            "apply",
        )])
    }))
}

/// `collider/from-layer`: rect, ellipse (as a circle of the mean radius),
/// triangle and polygon layers; anything else collides with nothing.
/// Group transforms are not followed (as in TS v1).
fn from_layer(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let layer = first(inputs, 0);
        let (mode, rest, inverted) = common(inputs, 1, 2, 3);
        if !layer.truthy() {
            return Ok(vec![collider(ColliderShape::None, "apply")]);
        }
        let shape = match &layer {
            Value::Scene(n) => layer_shape(n, mode, rest, inverted),
            _ => ColliderShape::None,
        };
        // The TS closures built here are anonymous.
        Ok(vec![collider(shape, "")])
    }))
}

fn layer_shape(n: &SceneNode, mode: ColliderMode, rest: f64, inverted: bool) -> ColliderShape {
    match n {
        SceneNode::Rect {
            x,
            y,
            w,
            h,
            mode: m,
            ..
        } => {
            let (mut x, mut y, mut w, mut h) = (*x, *y, *w, *h);
            match &**m {
                "center" => {
                    x -= w / 2.0;
                    y -= h / 2.0;
                }
                "corners" => {
                    w -= x;
                    h -= y;
                }
                "radius" => {
                    x -= w;
                    y -= h;
                    w *= 2.0;
                    h *= 2.0;
                }
                _ => {}
            }
            ColliderShape::Rect {
                mode,
                rest,
                inverted,
                min_x: x,
                min_y: y,
                max_x: x + w,
                max_y: y + h,
            }
        }
        SceneNode::Ellipse {
            x,
            y,
            w,
            h,
            mode: m,
            ..
        } => {
            let (mut cx, mut cy, mut rx, mut ry) = (*x, *y, w / 2.0, h / 2.0);
            match &**m {
                "corner" => {
                    cx += rx;
                    cy += ry;
                }
                "radius" => {
                    rx = *w;
                    ry = *h;
                }
                _ => {}
            }
            ColliderShape::Circle {
                mode,
                rest,
                inverted,
                cx,
                cy,
                r: (rx + ry) * 0.5,
            }
        }
        SceneNode::Triangle { pts: p, .. } => ColliderShape::Polygon {
            mode,
            rest,
            inverted,
            edges: Box::new(PolygonEdges::new(vec![
                (p[0], p[1]),
                (p[2], p[3]),
                (p[4], p[5]),
            ])),
        },
        SceneNode::Polygon { points, .. } => ColliderShape::Polygon {
            mode,
            rest,
            inverted,
            edges: Box::new(PolygonEdges::new(
                points.iter().map(|p| (p.x, p.y)).collect(),
            )),
        },
        _ => ColliderShape::None,
    }
}
