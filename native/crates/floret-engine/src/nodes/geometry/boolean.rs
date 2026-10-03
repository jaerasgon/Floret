//! `geometry/boolean`: union, intersect, subtract (a − b) or xor of the
//! filled areas of two layers, as polygons (with holes where the result
//! has them). Each layer's closed shapes count with nonzero winding; open
//! lines are ignored. With only `a` wired, `union` merges its shapes.

use super::{layer_contours, live_mode, pin_style, poly_outputs, Pins, Poly};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{default_style, ResolvedStyle};
use crate::value::{Value, Vec2};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use std::sync::Arc;

const MODE: &str = "union";

pub(super) fn boolean(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let a = layer_contours(pins.spread(inputs, "a"));
        let b = layer_contours(pins.spread(inputs, "b"));
        let style_v = pins.value(inputs, "style");
        // A wired style, else the first filled shape's own.
        let style: Arc<ResolvedStyle> = if matches!(style_v, Value::Style(_)) {
            pin_style(&style_v, true)
        } else {
            a.iter()
                .chain(&b)
                .find(|c| c.closed)
                .map_or_else(|| default_style().clone(), |c| c.style.clone())
        };
        let rings = |cs: &[super::Contour]| -> Vec<Vec<[f64; 2]>> {
            cs.iter()
                .filter(|c| c.closed && c.pts.len() >= 3)
                .map(|c| c.pts.iter().map(|p| [p.x, p.y]).collect())
                .collect()
        };
        let shapes = overlay(&rings(&a), &rings(&b), &mode);
        let polys: Vec<Poly> = shapes
            .into_iter()
            .map(|rings| Poly {
                rings,
                closed: true,
                style: style.clone(),
            })
            .collect();
        Ok(poly_outputs(&polys))
    })
}

/// `a` (op) `b`: shapes of outer ring + holes.
pub(super) fn overlay(a: &[Vec<[f64; 2]>], b: &[Vec<[f64; 2]>], mode: &str) -> Vec<Vec<Vec<Vec2>>> {
    let finite = |r: &&Vec<[f64; 2]>| r.iter().all(|p| p[0].is_finite() && p[1].is_finite());
    let a: Vec<Vec<[f64; 2]>> = a.iter().filter(finite).cloned().collect();
    let b: Vec<Vec<[f64; 2]>> = b.iter().filter(finite).cloned().collect();
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    let rule = match mode {
        "intersect" => OverlayRule::Intersect,
        "subtract" => OverlayRule::Difference,
        "xor" => OverlayRule::Xor,
        _ => OverlayRule::Union,
    };
    a.overlay(&b, rule, FillRule::NonZero)
        .into_iter()
        .map(|s| {
            s.into_iter()
                .map(|r| r.into_iter().map(|[x, y]| Vec2 { x, y }).collect())
                .collect()
        })
        .collect()
}
