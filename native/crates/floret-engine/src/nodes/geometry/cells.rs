//! `geometry/cells`: the Voronoi cells, Delaunay triangles or convex hull
//! of a points spread, as polygons.
//!
//! Voronoi cells are clipped to the `center` / `size` rectangle. `gap`
//! shrinks every polygon by half its value on each side, so neighbouring
//! cells end up `gap` apart. `colors` (cycled) fills polygon i with colour
//! i, over the wired `style`.

use super::{
    clip_half_plane, inset_convex, live_mode, num_pin, param_str, pin_style, poly_outputs,
    signed_area, style_pin, sub, vec2_pin, Pins, Poly,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{resolve_style, Paint, Style, StyleOp};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Value, Vec2};
use std::sync::Arc;

const MODE: &str = "voronoi";
const SIZE: Vec2 = Vec2 { x: 400.0, y: 400.0 };

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread)];
    if param_str(params, "mode", MODE) == "voronoi" {
        v.push(vec2_pin("center", 0.0, 0.0));
        v.push(vec2_pin("size", SIZE.x, SIZE.y));
    }
    v.push(num_pin("gap", 0.0));
    v.push(PortSpec::new("colors", PortType::Color).kind(PortKind::Spread));
    v.push(style_pin());
    v
}

pub(super) fn cells(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let pts = pins.points(inputs, "points");
        let gap = pins.num(inputs, "gap", 0.0).max(0.0);
        let rings: Vec<Vec<Vec2>> = match mode.as_str() {
            "delaunay" => delaunay(&pts),
            "hull" => {
                let h = convex_hull(&pts);
                if h.len() >= 3 {
                    vec![h]
                } else {
                    Vec::new()
                }
            }
            _ => {
                let center = pins.vec2(inputs, "center", Vec2::default());
                let size = pins.vec2(inputs, "size", SIZE);
                voronoi(&pts, center, size)
            }
        };
        let rings: Vec<Vec<Vec2>> = rings
            .into_iter()
            .map(|r| inset_convex(&r, gap / 2.0))
            .filter(|r| r.len() >= 3)
            .collect();

        let style_v = pins.value(inputs, "style");
        let base = pin_style(&style_v, true);
        let colors: Vec<Paint> = pins
            .spread(inputs, "colors")
            .iter()
            .filter_map(|c| super::super::style::paint_of(c).ok())
            .collect();
        let wired = super::super::style::style_arg(&style_v);
        let polys: Vec<Poly> = rings
            .iter()
            .enumerate()
            .map(|(i, r)| Poly {
                rings: vec![r.clone()],
                closed: true,
                style: if colors.is_empty() {
                    base.clone()
                } else {
                    let fill = StyleOp::Fill(Some(colors[i % colors.len()].clone()));
                    resolve_style(
                        Some(&Style::chain(wired, [fill])),
                        crate::scene::default_style(),
                    )
                },
            })
            .collect();
        let mut out = poly_outputs(&polys);
        out.push(Arc::from(
            rings
                .iter()
                .map(|r| Value::Vec2(super::centroid(r)))
                .collect::<Vec<_>>(),
        ));
        Ok(out)
    })
}

fn triangulate(pts: &[Vec2]) -> delaunator::Triangulation {
    let dp: Vec<delaunator::Point> = pts
        .iter()
        .map(|p| delaunator::Point { x: p.x, y: p.y })
        .collect();
    delaunator::triangulate(&dp)
}

/// Delaunay triangles (2n − h − 2 of them for n points in general position
/// with h on the hull).
pub(super) fn delaunay(pts: &[Vec2]) -> Vec<Vec<Vec2>> {
    let t = triangulate(pts);
    t.triangles
        .as_chunks::<3>()
        .0
        .iter()
        .map(|tri| tri.iter().map(|&i| pts[i]).collect())
        .collect()
}

/// Voronoi cells in site order, each the bounds rectangle cut by the
/// bisectors with its Delaunay neighbours (sites whose cell is empty, e.g.
/// duplicates or sites outside the bounds, are left out).
pub(super) fn voronoi(pts: &[Vec2], center: Vec2, size: Vec2) -> Vec<Vec<Vec2>> {
    let (hw, hh) = (size.x.abs() / 2.0, size.y.abs() / 2.0);
    let bounds = vec![
        Vec2 {
            x: center.x - hw,
            y: center.y - hh,
        },
        Vec2 {
            x: center.x + hw,
            y: center.y - hh,
        },
        Vec2 {
            x: center.x + hw,
            y: center.y + hh,
        },
        Vec2 {
            x: center.x - hw,
            y: center.y + hh,
        },
    ];
    let n = pts.len();
    if n == 1 {
        return vec![bounds];
    }
    let t = triangulate(pts);
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    if t.triangles.is_empty() {
        // All on one line: the hull lists them in order.
        for w in t.hull.windows(2) {
            neighbours[w[0]].push(w[1]);
            neighbours[w[1]].push(w[0]);
        }
    } else {
        for tri in t.triangles.as_chunks::<3>().0 {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                neighbours[a].push(b);
                neighbours[b].push(a);
            }
        }
    }
    let mut out = Vec::new();
    for (i, ns) in neighbours.iter_mut().enumerate() {
        if ns.is_empty() {
            continue;
        }
        ns.sort_unstable();
        ns.dedup();
        let p = pts[i];
        let mut cell = bounds.clone();
        for &j in ns.iter() {
            // Keep the side of the bisector nearer p: d · x <= d · mid.
            let d = sub(pts[j], p);
            if d.x == 0.0 && d.y == 0.0 {
                continue;
            }
            let mid = super::lerp(p, pts[j], 0.5);
            cell = clip_half_plane(&cell, d, d.x * mid.x + d.y * mid.y);
            if cell.len() < 3 {
                break;
            }
        }
        if cell.len() >= 3 && signed_area(&cell).abs() > 1e-9 {
            out.push(cell);
        }
    }
    out
}

/// Convex hull (Andrew's monotone chain), clockwise on screen, without
/// collinear points.
pub(super) fn convex_hull(pts: &[Vec2]) -> Vec<Vec2> {
    let mut p: Vec<Vec2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<Vec2> = Vec::with_capacity(p.len() * 2);
    for pass in [p.as_slice(), &p.iter().rev().copied().collect::<Vec<_>>()] {
        let start = hull.len();
        for &q in pass {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0
            {
                hull.pop();
            }
            hull.push(q);
        }
        hull.pop();
    }
    hull
}
