//! `geometry/path-ops`: reshape point lists and the outlines of layers.
//!
//! Modes: `resample` (points evenly `spacing` apart along the path),
//! `smooth` (Chaikin corner cutting, `iterations` times), `subdivide`
//! (midpoints inserted, `iterations` times), `simplify`
//! (Ramer–Douglas–Peucker within `tolerance`) and `offset` (grow by
//! `distance`, or shrink when negative: closed shapes are offset as
//! areas with `join` corners, open paths shift sideways).
//!
//! A wired `path` layer wins over `points`; each of its shapes keeps its
//! own style unless `style` is wired.

use super::{
    dist, int_pin, layer_contours, lerp, live_mode, num_pin, param_str, pin_style, poly_outputs,
    style_pin, Contour, Pins, Poly, MAX_POINTS,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::text::path::Path;
use crate::value::{Value, Vec2};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};

const MODE: &str = "resample";
const SPACING: f64 = 10.0;
const TOLERANCE: f64 = 2.0;
const DISTANCE: f64 = 10.0;
/// Chaikin / subdivision passes (each doubles the points).
const MAX_ITERATIONS: f64 = 8.0;

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![
        PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread),
        PortSpec::new("closed", PortType::Boolean).default(Value::Bool(false)),
        super::layer_pin("path"),
    ];
    match param_str(params, "mode", MODE) {
        "smooth" => v.push(int_pin("iterations", 3.0)),
        "subdivide" => v.push(int_pin("iterations", 1.0)),
        "simplify" => v.push(num_pin("tolerance", TOLERANCE)),
        "offset" => {
            v.push(num_pin("distance", DISTANCE));
            let mut join = PortSpec::new("join", PortType::String).default(Value::str("round"));
            join.param = Some("join".into());
            v.push(join);
        }
        _ => v.push(num_pin("spacing", SPACING)),
    }
    v.push(style_pin());
    v
}

pub(super) fn path_ops(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let style_v = pins.value(inputs, "style");
        let wired = matches!(style_v, Value::Style(_));
        let mut contours = layer_contours(pins.spread(inputs, "path"));
        if contours.is_empty() {
            let closed = pins.bool(inputs, "closed", false);
            contours.push(Contour {
                pts: pins.points(inputs, "points"),
                closed,
                style: pin_style(&style_v, closed),
                source: 0,
            });
        } else if wired {
            for c in &mut contours {
                c.style = pin_style(&style_v, c.closed);
            }
        }
        contours.retain(|c| !c.pts.is_empty());
        let iterations = || {
            pins.num(inputs, "iterations", 1.0)
                .clamp(0.0, MAX_ITERATIONS) as usize
        };
        let polys: Vec<Poly> = match mode.as_str() {
            "offset" => {
                let d = pins.num(inputs, "distance", DISTANCE);
                let join = match pins.value(inputs, "join") {
                    Value::Str(s) => s.to_string(),
                    _ => "round".into(),
                };
                offset(&contours, d, &join)
            }
            other => contours
                .iter()
                .map(|c| {
                    let pts = match other {
                        "smooth" => chaikin(&c.pts, c.closed, iterations()),
                        "subdivide" => subdivide(&c.pts, c.closed, iterations()),
                        "simplify" => {
                            simplify(&c.pts, c.closed, pins.num(inputs, "tolerance", TOLERANCE))
                        }
                        _ => resample(&c.pts, c.closed, pins.num(inputs, "spacing", SPACING)),
                    };
                    Poly {
                        rings: vec![pts],
                        closed: c.closed,
                        style: c.style.clone(),
                    }
                })
                .collect(),
        };
        let total: usize = polys.iter().flat_map(|p| &p.rings).map(Vec::len).sum();
        if total > MAX_POINTS {
            cx.warn(format!("{total} points, over {MAX_POINTS}: nothing drawn."));
            return Ok(poly_outputs(&[]));
        }
        Ok(poly_outputs(&polys))
    })
}

/// Points evenly spaced along the path, about `spacing` apart (exactly
/// `length / n`). An open path keeps both ends.
pub(super) fn resample(pts: &[Vec2], closed: bool, spacing: f64) -> Vec<Vec2> {
    let Some(path) = Path::new(pts.iter().copied(), closed) else {
        return pts.to_vec();
    };
    if spacing.is_nan() || spacing <= 0.0 {
        return pts.to_vec();
    }
    let len = path.length();
    let n = (len / spacing).round().min(MAX_POINTS as f64);
    if closed {
        let n = n.max(3.0) as usize;
        (0..n)
            .map(|i| path.at(i as f64 * len / n as f64).0)
            .collect()
    } else {
        let n = n.max(1.0) as usize;
        (0..=n)
            .map(|i| path.at(i as f64 * len / n as f64).0)
            .collect()
    }
}

/// Chaikin's corner cutting: each edge becomes its ¼ and ¾ points. Open
/// paths keep their end points.
pub(super) fn chaikin(pts: &[Vec2], closed: bool, iterations: usize) -> Vec<Vec2> {
    let mut cur = pts.to_vec();
    for _ in 0..iterations {
        let n = cur.len();
        if !(3..=MAX_POINTS).contains(&n) {
            break;
        }
        let mut next = Vec::with_capacity(n * 2);
        if !closed {
            next.push(cur[0]);
        }
        let edges = if closed { n } else { n - 1 };
        for i in 0..edges {
            let (a, b) = (cur[i], cur[(i + 1) % n]);
            if closed || i > 0 {
                next.push(lerp(a, b, 0.25));
            }
            if closed || i < edges - 1 {
                next.push(lerp(a, b, 0.75));
            }
        }
        if !closed {
            next.push(cur[n - 1]);
        }
        cur = next;
    }
    cur
}

/// Insert every edge's midpoint, `iterations` times.
pub(super) fn subdivide(pts: &[Vec2], closed: bool, iterations: usize) -> Vec<Vec2> {
    let mut cur = pts.to_vec();
    for _ in 0..iterations {
        let n = cur.len();
        if !(2..=MAX_POINTS).contains(&n) {
            break;
        }
        let mut next = Vec::with_capacity(n * 2);
        for i in 0..n {
            next.push(cur[i]);
            if closed || i + 1 < n {
                next.push(lerp(cur[i], cur[(i + 1) % n], 0.5));
            }
        }
        cur = next;
    }
    cur
}

/// Ramer–Douglas–Peucker. A closed path is split at the point farthest
/// from its first, and both halves simplified.
pub(super) fn simplify(pts: &[Vec2], closed: bool, tolerance: f64) -> Vec<Vec2> {
    if pts.len() < 3 || tolerance.is_nan() || tolerance <= 0.0 {
        return pts.to_vec();
    }
    if !closed {
        let mut keep = vec![false; pts.len()];
        keep[0] = true;
        keep[pts.len() - 1] = true;
        rdp(pts, 0, pts.len() - 1, tolerance, &mut keep);
        return kept(pts, &keep);
    }
    let far = (1..pts.len())
        .max_by(|&a, &b| dist(pts[0], pts[a]).total_cmp(&dist(pts[0], pts[b])))
        .unwrap_or(0);
    let mut ring = pts.to_vec();
    ring.push(pts[0]);
    let mut keep = vec![false; ring.len()];
    keep[0] = true;
    keep[far] = true;
    rdp(&ring, 0, far, tolerance, &mut keep);
    rdp(&ring, far, ring.len() - 1, tolerance, &mut keep);
    keep.pop();
    ring.pop();
    kept(&ring, &keep)
}

fn kept(pts: &[Vec2], keep: &[bool]) -> Vec<Vec2> {
    pts.iter()
        .zip(keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

fn rdp(pts: &[Vec2], a: usize, b: usize, tol: f64, keep: &mut [bool]) {
    if b <= a + 1 {
        return;
    }
    let (pa, pb) = (pts[a], pts[b]);
    let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
    let len = dx.hypot(dy);
    let d = |p: Vec2| {
        if len < 1e-12 {
            dist(p, pa)
        } else {
            ((p.x - pa.x) * dy - (p.y - pa.y) * dx).abs() / len
        }
    };
    let (mut best, mut at) = (0.0, a);
    for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
        let e = d(*p);
        if e > best {
            best = e;
            at = i;
        }
    }
    if best > tol {
        keep[at] = true;
        rdp(pts, a, at, tol, keep);
        rdp(pts, at, b, tol, keep);
    }
}

/// Offset every shape by `d`: closed contours from one drawn shape (a
/// glyph and its holes) are offset together as an area, open ones shift
/// sideways (left of the direction of travel for positive `d`).
pub(super) fn offset(contours: &[Contour], d: f64, join: &str) -> Vec<Poly> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < contours.len() {
        let c = &contours[i];
        if !c.closed {
            out.push(Poly {
                rings: vec![offset_open(&c.pts, d)],
                closed: false,
                style: c.style.clone(),
            });
            i += 1;
            continue;
        }
        let mut group: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut j = i;
        while j < contours.len() && contours[j].closed && contours[j].source == c.source {
            group.push(contours[j].pts.iter().map(|p| [p.x, p.y]).collect());
            j += 1;
        }
        for shape in offset_area(&group, d, join) {
            out.push(Poly {
                rings: shape,
                closed: true,
                style: c.style.clone(),
            });
        }
        i = j;
    }
    out
}

/// The filled area of `rings` (nonzero), grown by `d` (shrunk when < 0):
/// shapes of outer ring + holes.
pub(super) fn offset_area(rings: &[Vec<[f64; 2]>], d: f64, join: &str) -> Vec<Vec<Vec<Vec2>>> {
    let rings: Vec<Vec<[f64; 2]>> = rings
        .iter()
        .filter(|r| r.len() >= 3 && r.iter().all(|p| p[0].is_finite() && p[1].is_finite()))
        .cloned()
        .collect();
    if rings.is_empty() || !d.is_finite() {
        return Vec::new();
    }
    let shapes = rings.simplify_shape(FillRule::NonZero);
    let shapes = if d == 0.0 {
        shapes
    } else {
        let join = match join {
            "miter" => LineJoin::Miter(std::f64::consts::PI / 6.0),
            "bevel" => LineJoin::Bevel,
            _ => LineJoin::Round(0.1),
        };
        shapes.outline(&OutlineStyle::new(d).line_join(join))
    };
    shapes
        .into_iter()
        .map(|s| {
            s.into_iter()
                .map(|r| r.into_iter().map(|[x, y]| Vec2 { x, y }).collect())
                .collect()
        })
        .collect()
}

/// An open polyline moved `d` to its left (mitred corners, limited).
fn offset_open(pts: &[Vec2], d: f64) -> Vec<Vec2> {
    let n = pts.len();
    if n < 2 || d == 0.0 {
        return pts.to_vec();
    }
    let normal = |a: Vec2, b: Vec2| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = dx.hypot(dy);
        (l >= 1e-12).then(|| Vec2 {
            x: dy / l,
            y: -dx / l,
        })
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let before = (i > 0).then(|| normal(pts[i - 1], pts[i])).flatten();
        let after = (i + 1 < n).then(|| normal(pts[i], pts[i + 1])).flatten();
        let m = match (before, after) {
            (Some(a), Some(b)) => {
                let s = Vec2 {
                    x: a.x + b.x,
                    y: a.y + b.y,
                };
                let l = s.x.hypot(s.y);
                if l < 1e-9 {
                    a
                } else {
                    // Miter: along the bisector, lengthened (up to 4×) to
                    // keep both edges `d` away.
                    let u = Vec2 {
                        x: s.x / l,
                        y: s.y / l,
                    };
                    let k = 1.0 / (u.x * a.x + u.y * a.y).max(0.25);
                    Vec2 {
                        x: u.x * k,
                        y: u.y * k,
                    }
                }
            }
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => Vec2::default(),
        };
        out.push(Vec2 {
            x: pts[i].x + m.x * d,
            y: pts[i].y + m.y * d,
        });
    }
    out
}
