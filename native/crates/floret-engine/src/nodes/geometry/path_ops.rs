//! `geometry/path-ops`: reshape point lists and the outlines of layers.
//!
//! Modes: `resample` (points evenly `spacing` apart along the path),
//! `smooth` (Chaikin corner cutting, `iterations` times), `subdivide`
//! (midpoints inserted, `iterations` times), `simplify`
//! (Ramer–Douglas–Peucker within `tolerance`) and `offset` (grow by
//! `distance`, or shrink when negative: closed shapes are offset as
//! areas with `join` corners, open paths shift sideways), `round-corners`
//! (each corner becomes an arc of `radius`), `thicken` (the outline of a
//! `width`-wide stroke, with `cap` ends and `join` corners, as filled
//! polygons), the deformers `wave` (a sine wave along the path's normals),
//! `noise` (Perlin displacement) and `twist` (a swirl round `center`), and
//! `explode` (every edge as its own piece, pushed `distance` away from the
//! outline's centre).
//!
//! The deformers first resample the path every `detail` units (0 keeps the
//! points as they are), so a four-point square still bends. Their `phase`
//! counts whole cycles: 0 → 1 loops seamlessly.
//!
//! A wired `path` layer wins over `points`; each of its shapes keeps its
//! own style unless `style` is wired. Every mode's `layers` output has one
//! layer per outline (per piece in `explode`), so a layer of many shapes
//! can be taken apart with any mode.

use super::{
    centroid, dist, int_pin, layer_contours, lerp, live_mode, num_pin, param_str, pin_style,
    poly_outputs, style_pin, vec2_pin, Contour, Pins, Poly, MAX_POINTS,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{line_default_style, ResolvedStyle};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::text::path::Path;
use crate::value::{Value, Vec2};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

const MODE: &str = "resample";
const SPACING: f64 = 10.0;
const TOLERANCE: f64 = 2.0;
const DISTANCE: f64 = 10.0;
/// Chaikin / subdivision passes (each doubles the points).
const MAX_ITERATIONS: f64 = 8.0;
const RADIUS: f64 = 10.0;
const WIDTH: f64 = 10.0;
/// The deformers' resampling step.
const DETAIL: f64 = 4.0;

/// A String pin bound to a dropdown param of the same name.
fn choice_pin(name: &str, default: &str) -> PortSpec {
    let mut p = PortSpec::new(name, PortType::String).default(Value::str(default));
    p.param = Some(name.into());
    p
}

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
            v.push(choice_pin("join", "round"));
        }
        "round-corners" => v.push(num_pin("radius", RADIUS)),
        "thicken" => {
            v.push(num_pin("width", WIDTH));
            v.push(choice_pin("cap", "round"));
            v.push(choice_pin("join", "round"));
        }
        "wave" => {
            v.push(num_pin("amplitude", 8.0));
            v.push(num_pin("wavelength", 40.0));
            v.push(num_pin("phase", 0.0));
            v.push(num_pin("detail", DETAIL));
        }
        "noise" => {
            v.push(num_pin("amount", 12.0));
            v.push(num_pin("scale", 0.02));
            v.push(int_pin("seed", 0.0));
            v.push(num_pin("phase", 0.0));
            v.push(num_pin("detail", DETAIL));
        }
        "twist" => {
            v.push(num_pin("angle", PI / 2.0));
            v.push(num_pin("radius", 200.0));
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(num_pin("detail", DETAIL));
        }
        "explode" => v.push(num_pin("distance", DISTANCE)),
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
        let choice = |name: &str, fallback: &str| match pins.value(inputs, name) {
            Value::Str(s) => s.to_string(),
            _ => match cx.param(name) {
                Some(serde_json::Value::String(s)) => s.clone(),
                _ => fallback.to_owned(),
            },
        };
        let detail = |c: &Contour| resample(&c.pts, c.closed, pins.num(inputs, "detail", DETAIL));
        let polys: Vec<Poly> = match mode.as_str() {
            "offset" => {
                let d = pins.num(inputs, "distance", DISTANCE);
                offset(&contours, d, &choice("join", "round"))
            }
            "thicken" => {
                let width = pins.num(inputs, "width", WIDTH);
                let (cap, join) = (choice("cap", "round"), choice("join", "round"));
                contours
                    .iter()
                    .flat_map(|c| {
                        let style = if wired {
                            pin_style(&style_v, true)
                        } else {
                            stroke_as_fill(&c.style)
                        };
                        thicken(&c.pts, c.closed, width, &cap, &join)
                            .into_iter()
                            .map(move |rings| Poly {
                                rings,
                                closed: true,
                                style: style.clone(),
                            })
                    })
                    .collect()
            }
            "explode" => {
                let d = pins.num(inputs, "distance", DISTANCE);
                contours
                    .iter()
                    .flat_map(|c| {
                        let style = if wired {
                            pin_style(&style_v, false)
                        } else {
                            fill_as_stroke(&c.style)
                        };
                        explode(&c.pts, c.closed, d)
                            .into_iter()
                            .map(move |piece| Poly {
                                rings: vec![piece],
                                closed: false,
                                style: style.clone(),
                            })
                    })
                    .collect()
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
                        "round-corners" => {
                            round_corners(&c.pts, c.closed, pins.num(inputs, "radius", RADIUS))
                        }
                        "wave" => wave(
                            &detail(c),
                            c.closed,
                            pins.num(inputs, "amplitude", 8.0),
                            pins.num(inputs, "wavelength", 40.0),
                            pins.num(inputs, "phase", 0.0),
                        ),
                        "noise" => noise(
                            &detail(c),
                            pins.num(inputs, "amount", 12.0),
                            pins.num(inputs, "scale", 0.02),
                            pins.num(inputs, "seed", 0.0),
                            pins.num(inputs, "phase", 0.0),
                        ),
                        "twist" => twist(
                            &detail(c),
                            pins.vec2(inputs, "center", Vec2::default()),
                            pins.num(inputs, "angle", PI / 2.0),
                            pins.num(inputs, "radius", 200.0),
                        ),
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

// ─── Round corners ───────────────────────────────────────────────────────────

/// Every corner replaced by a circular arc of `radius`, tangent to both
/// edges. The radius shrinks where an edge is too short to fit it (each
/// edge gives at most half its length to either end). An open path keeps
/// its end points; straight-through points and repeated points stay.
pub(super) fn round_corners(pts: &[Vec2], closed: bool, radius: f64) -> Vec<Vec2> {
    let n = pts.len();
    if n < 3 || !radius.is_finite() || radius <= 0.0 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n * 8);
    for i in 0..n {
        let interior = closed || (i > 0 && i + 1 < n);
        if !interior {
            out.push(pts[i]);
            continue;
        }
        let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
        let (la, lc) = (dist(a, b), dist(c, b));
        if la < 1e-9 || lc < 1e-9 {
            out.push(b);
            continue;
        }
        let u1 = Vec2 {
            x: (a.x - b.x) / la,
            y: (a.y - b.y) / la,
        };
        let u2 = Vec2 {
            x: (c.x - b.x) / lc,
            y: (c.y - b.y) / lc,
        };
        // The angle between the two edges at the corner.
        let theta = (u1.x * u2.x + u1.y * u2.y).clamp(-1.0, 1.0).acos();
        if !(1e-6..=PI - 1e-6).contains(&theta) {
            out.push(b);
            continue;
        }
        let half = theta / 2.0;
        let t = (radius / half.tan()).min(la / 2.0).min(lc / 2.0);
        let r = t * half.tan();
        let bis = Vec2 {
            x: u1.x + u2.x,
            y: u1.y + u2.y,
        };
        let k = r / half.sin() / bis.x.hypot(bis.y);
        let centre = Vec2 {
            x: b.x + bis.x * k,
            y: b.y + bis.y * k,
        };
        let a1 = (u1.y * t + b.y - centre.y).atan2(u1.x * t + b.x - centre.x);
        let a2 = (u2.y * t + b.y - centre.y).atan2(u2.x * t + b.x - centre.x);
        // The short way round (the arc turns by π − θ).
        let mut sweep = a2 - a1;
        while sweep > PI {
            sweep -= TAU;
        }
        while sweep < -PI {
            sweep += TAU;
        }
        // About 3 units per segment.
        let steps = ((sweep.abs() * r / 3.0).ceil() as usize).clamp(2, 64);
        for k in 0..=steps {
            let ang = a1 + sweep * k as f64 / steps as f64;
            out.push(Vec2 {
                x: centre.x + r * ang.cos(),
                y: centre.y + r * ang.sin(),
            });
        }
    }
    out
}

// ─── Thicken ─────────────────────────────────────────────────────────────────

/// The outline of the path stroked `width` wide: filled shapes (outer ring,
/// then holes; a closed path gives a ring with a hole). `cap`: butt, round
/// or square ends (open paths); `join`: round, miter or bevel corners.
pub(super) fn thicken(
    pts: &[Vec2],
    closed: bool,
    width: f64,
    cap: &str,
    join: &str,
) -> Vec<Vec<Vec<Vec2>>> {
    let path: Vec<[f64; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
    if path.len() < 2 || !width.is_finite() || width <= 0.0 {
        return Vec::new();
    }
    let cap = || match cap {
        "butt" => LineCap::Butt,
        "square" => LineCap::Square,
        _ => LineCap::Round(0.1),
    };
    let join = match join {
        "miter" => LineJoin::Miter(PI / 6.0),
        "bevel" => LineJoin::Bevel,
        _ => LineJoin::Round(0.1),
    };
    let style = StrokeStyle::new(width)
        .start_cap(cap())
        .end_cap(cap())
        .line_join(join);
    path.stroke(style, closed)
        .into_iter()
        .map(|s| {
            s.into_iter()
                .map(|r| r.into_iter().map(|[x, y]| Vec2 { x, y }).collect())
                .collect()
        })
        .collect()
}

/// A stroked shape's style as a fill: the stroke's paint fills (the fill's
/// when it had no stroke), nothing strokes.
fn stroke_as_fill(st: &Arc<ResolvedStyle>) -> Arc<ResolvedStyle> {
    Arc::new(ResolvedStyle {
        fill: st.stroke.clone().or_else(|| st.fill.clone()),
        stroke: None,
        ..(**st).clone()
    })
}

/// A filled shape's style as a line: its stroke when it has one, else its
/// fill's paint as a stroke of the default weight.
fn fill_as_stroke(st: &Arc<ResolvedStyle>) -> Arc<ResolvedStyle> {
    match (&st.stroke, &st.fill) {
        (Some(_), _) => Arc::new(ResolvedStyle {
            fill: None,
            ..(**st).clone()
        }),
        (None, Some(f)) => Arc::new(ResolvedStyle {
            fill: None,
            stroke: Some(f.clone()),
            ..(**line_default_style()).clone()
        }),
        (None, None) => line_default_style().clone(),
    }
}

// ─── Deformers ───────────────────────────────────────────────────────────────

/// Unit normals (left of the direction of travel) at every point, from its
/// neighbours: wrapping round when closed, one-sided at an open path's ends.
fn normals(pts: &[Vec2], closed: bool) -> Vec<Vec2> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let prev = if closed || i > 0 {
                pts[(i + n - 1) % n]
            } else {
                pts[i]
            };
            let next = if closed || i + 1 < n {
                pts[(i + 1) % n]
            } else {
                pts[i]
            };
            let (dx, dy) = (next.x - prev.x, next.y - prev.y);
            let l = dx.hypot(dy);
            if l < 1e-12 {
                Vec2::default()
            } else {
                Vec2 {
                    x: dy / l,
                    y: -dx / l,
                }
            }
        })
        .collect()
}

/// Each point moved along its normal by `amplitude` · sin(2π(s / λ +
/// phase)), s = distance along the path. A closed path fits a whole number
/// of waves round it (λ rounds to the nearest that does), so it joins up.
pub(super) fn wave(
    pts: &[Vec2],
    closed: bool,
    amplitude: f64,
    wavelength: f64,
    phase: f64,
) -> Vec<Vec2> {
    if pts.len() < 2 || wavelength.abs() < 1e-9 || amplitude == 0.0 {
        return pts.to_vec();
    }
    let mut s = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            acc += dist(pts[i - 1], *p);
        }
        s.push(acc);
    }
    let mut lambda = wavelength;
    if closed {
        let total = acc + dist(pts[pts.len() - 1], pts[0]);
        let waves = (total / wavelength.abs()).round().max(1.0);
        lambda = total / waves * wavelength.signum();
    }
    normals(pts, closed)
        .into_iter()
        .zip(pts)
        .zip(s)
        .map(|((nm, p), s)| {
            let d = amplitude * (TAU * (s / lambda + phase)).sin();
            Vec2 {
                x: p.x + nm.x * d,
                y: p.y + nm.y * d,
            }
        })
        .collect()
}

/// Each point pushed by up to `amount` in x and y by Perlin noise at
/// `scale` × the point. `phase` walks the noise round a circle, so it loops
/// every whole cycle.
pub(super) fn noise(pts: &[Vec2], amount: f64, scale: f64, seed: f64, phase: f64) -> Vec<Vec2> {
    if amount == 0.0 {
        return pts.to_vec();
    }
    // The loop's radius in noise space.
    const R: f64 = 1.5;
    let seed = crate::seed::mix(seed);
    let (ps, pc) = (TAU * phase).sin_cos();
    let field = |x: f64, y: f64, salt: f64| {
        let n = crate::nodes::noise::fbm_2d(x + pc * R + salt, y + ps * R, seed + salt, 2.0, 0.5);
        2.0 * n - 1.0
    };
    pts.iter()
        .map(|p| {
            let (x, y) = (p.x * scale, p.y * scale);
            Vec2 {
                x: p.x + amount * field(x, y, 0.0),
                y: p.y + amount * field(x, y, 57.0),
            }
        })
        .collect()
}

/// A swirl: each point turned round `centre` by `angle` (radians) times
/// (1 − d / radius)², so the centre turns fully and the turn fades out at
/// `radius`. A radius ≤ 0 turns everything by `angle`.
pub(super) fn twist(pts: &[Vec2], centre: Vec2, angle: f64, radius: f64) -> Vec<Vec2> {
    if !angle.is_finite() || angle == 0.0 {
        return pts.to_vec();
    }
    pts.iter()
        .map(|p| {
            let (dx, dy) = (p.x - centre.x, p.y - centre.y);
            let w = if radius > 0.0 {
                let f = (1.0 - dx.hypot(dy) / radius).max(0.0);
                f * f
            } else {
                1.0
            };
            let (s, c) = (angle * w).sin_cos();
            Vec2 {
                x: centre.x + c * dx - s * dy,
                y: centre.y + s * dx + c * dy,
            }
        })
        .collect()
}

// ─── Explode ─────────────────────────────────────────────────────────────────

/// Every edge as its own two-point piece, moved `distance` away from the
/// outline's centre (along the edge's normal when its midpoint is the
/// centre). Zero-length edges are dropped.
pub(super) fn explode(pts: &[Vec2], closed: bool, distance: f64) -> Vec<Vec<Vec2>> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let centre = centroid(pts);
    let edges = if closed && n > 2 { n } else { n - 1 };
    let distance = if distance.is_finite() { distance } else { 0.0 };
    (0..edges)
        .filter_map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let e = dist(a, b);
            if e < 1e-12 {
                return None;
            }
            let mid = lerp(a, b, 0.5);
            let (mut dx, mut dy) = (mid.x - centre.x, mid.y - centre.y);
            let mut l = dx.hypot(dy);
            if l < 1e-9 {
                (dx, dy, l) = ((b.y - a.y) / e, -(b.x - a.x) / e, 1.0);
            }
            let (ox, oy) = (dx / l * distance, dy / l * distance);
            let shift = |p: Vec2| Vec2 {
                x: p.x + ox,
                y: p.y + oy,
            };
            Some(vec![shift(a), shift(b)])
        })
        .collect()
}
