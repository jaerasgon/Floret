//! `geometry/morph` (Shape Morph): blend the outlines of layer `a` into
//! layer `b` by `t` (0 = a, 1 = b).
//!
//! Both sides are read as contours (see [`super::layer_contours`]), and
//! split into outer shapes and holes (a ring running the other way round
//! from the biggest ring of its layer is a hole). Outers pair with outers
//! and holes with holes, in drawing order; a contour with no partner
//! shrinks to (or grows from) its own centre. Each pair is resampled to the
//! same number of points by arc length, turned to run the same way round,
//! and its start rotated to where the two outlines line up best, so the
//! blend doesn't twist. Fill / stroke colours and stroke weight blend too.

use super::measure::Measured;
use super::{centroid, layer_contours, pin_style, poly_outputs, signed_area, Contour, Pins, Poly};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{Paint, ResolvedStyle};
use crate::text::outline::winding;
use crate::value::{Color, Spread, Value, Vec2};
use std::sync::Arc;

/// Points per outline when `points` is 0: about one every 4 units.
const MIN_AUTO: usize = 24;
const MAX_AUTO: usize = 1000;
pub(super) const MAX_RESOLUTION: usize = 4000;

/// A contour ready to pair: outers run clockwise on screen and holes
/// the other way, on both sides, so blends keep their holes.
struct Item {
    pts: Vec<Vec2>,
    closed: bool,
    style: Arc<ResolvedStyle>,
}

/// A prepared pair: matching point lists, ready to blend.
#[derive(Clone, Debug)]
pub(super) struct Pair {
    pub a: Vec<Vec2>,
    pub b: Vec<Vec2>,
    pub closed_a: bool,
    pub closed_b: bool,
    pub style_a: Option<Arc<ResolvedStyle>>,
    pub style_b: Option<Arc<ResolvedStyle>>,
    pub hole: bool,
}

pub(super) fn morph(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    let mut cache: Option<(Vec<Value>, Vec<Value>, usize, Vec<Pair>)> = None;
    raw(move |inputs, _| {
        let sa = spread_of(&pins, inputs, "a");
        let sb = spread_of(&pins, inputs, "b");
        let res = pins
            .num(inputs, "points", 0.0)
            .clamp(0.0, MAX_RESOLUTION as f64) as usize;
        let hit = cache
            .as_ref()
            .is_some_and(|(ca, cb, cr, _)| *cr == res && same(ca, &sa) && same(cb, &sb));
        if !hit {
            let pairs = prepare(&layer_contours(&sa), &layer_contours(&sb), res);
            cache = Some((sa, sb, res, pairs));
        }
        let pairs = cache.as_ref().map_or(&[][..], |c| &c.3[..]);
        let t = pins.num(inputs, "t", 0.5);
        let style_v = pins.value(inputs, "style");
        let wired = matches!(style_v, Value::Style(_));
        let polys = blend(pairs, t, wired.then_some(&style_v));
        Ok(poly_outputs(&polys))
    })
}

fn spread_of(pins: &Pins, inputs: &[Spread], name: &str) -> Vec<Value> {
    pins.spread(inputs, name).to_vec()
}

/// The same layers as last time (`===` per element, as the memo checks).
fn same(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.same(q))
}

/// Contours split into (outer shapes, holes), oriented.
fn classify(cs: &[Contour]) -> (Vec<Item>, Vec<Item>) {
    let (mut outers, mut holes) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < cs.len() {
        let src = cs[i].source;
        let mut j = i;
        while j < cs.len() && cs[j].source == src {
            j += 1;
        }
        let run = &cs[i..j];
        let areas: Vec<f64> = run
            .iter()
            .map(|c| if c.closed { signed_area(&c.pts) } else { 0.0 })
            .collect();
        let biggest = areas
            .iter()
            .copied()
            .fold(0.0_f64, |m, a| if a.abs() > m.abs() { a } else { m });
        // Glyph runs hold many shapes: each outer ring starts a new one,
        // and the holes after it are its own.
        for (k, (c, a)) in run.iter().zip(&areas).enumerate() {
            let hole = k > 0 && c.closed && *a != 0.0 && a.signum() != biggest.signum();
            let mut pts = c.pts.clone();
            let clockwise = !hole;
            if c.closed && *a != 0.0 && (*a > 0.0) != clockwise {
                pts.reverse();
            }
            let item = Item {
                pts,
                closed: c.closed,
                style: c.style.clone(),
            };
            if hole {
                holes.push(item);
            } else {
                outers.push(item);
            }
        }
        i = j;
    }
    (outers, holes)
}

/// Pair up the contours of both sides and resample each pair.
pub(super) fn prepare(a: &[Contour], b: &[Contour], res: usize) -> Vec<Pair> {
    let (oa, ha) = classify(a);
    let (ob, hb) = classify(b);
    let mut out = Vec::new();
    for (xs, ys, hole) in [(&oa, &ob, false), (&ha, &hb, true)] {
        for i in 0..xs.len().max(ys.len()) {
            out.extend(pair(xs.get(i), ys.get(i), res, hole));
        }
    }
    out
}

fn auto_points(len: f64) -> usize {
    ((len / 4.0).ceil() as usize).clamp(MIN_AUTO, MAX_AUTO)
}

fn pair(a: Option<&Item>, b: Option<&Item>, res: usize, hole: bool) -> Option<Pair> {
    let ma = a.and_then(|c| Measured::new(&c.pts, c.closed));
    let mb = b.and_then(|c| Measured::new(&c.pts, c.closed));
    let n = if res >= 2 {
        res
    } else {
        let len = ma
            .iter()
            .chain(mb.iter())
            .map(Measured::length)
            .fold(0.0, f64::max);
        auto_points(len)
    };
    let closed_a = ma.as_ref().map(|m| m.closed);
    let closed_b = mb.as_ref().map(|m| m.closed);
    let (pa, pb) = match (ma.map(|m| m.even(n)), mb.map(|m| m.even(n))) {
        (Some(pa), Some(pb)) => {
            let pb = align(&pa, pb, closed_a == Some(true) && closed_b == Some(true));
            (pa, pb)
        }
        (Some(pa), None) => (pa.clone(), vec![centroid(&pa); pa.len()]),
        (None, Some(pb)) => (vec![centroid(&pb); pb.len()], pb),
        (None, None) => return None,
    };
    let closed_a = closed_a.or(closed_b).unwrap_or(false);
    Some(Pair {
        a: pa,
        b: pb,
        closed_a,
        closed_b: closed_b.unwrap_or(closed_a),
        style_a: a.map(|c| c.style.clone()),
        style_b: b.map(|c| c.style.clone()),
        hole,
    })
}

/// Squared distance between two outlines, each about its own centre, with
/// `b` read from `shift` (backwards when `rev`).
fn cost(a: &[Vec2], b: &[Vec2], ca: Vec2, cb: Vec2, shift: usize, rev: bool, step: usize) -> f64 {
    let n = a.len();
    let mut s = 0.0;
    let mut i = 0;
    while i < n {
        let j = if rev {
            (shift + n - i) % n
        } else {
            (i + shift) % n
        };
        let (dx, dy) = (
            (a[i].x - ca.x) - (b[j].x - cb.x),
            (a[i].y - ca.y) - (b[j].y - cb.y),
        );
        s += dx * dx + dy * dy;
        i += step;
    }
    s
}

/// `b` reordered to line up with `a` (same length): for closed outlines
/// (both clockwise) the starting point with the least total distance
/// between matching points; otherwise whichever direction matches better.
pub(super) fn align(a: &[Vec2], b: Vec<Vec2>, closed: bool) -> Vec<Vec2> {
    let n = a.len();
    if n != b.len() || n < 2 {
        return b;
    }
    let (ca, cb) = (centroid(a), centroid(&b));
    let read = |shift: usize, rev: bool| -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                if rev {
                    b[(shift + n - i) % n]
                } else {
                    b[(i + shift) % n]
                }
            })
            .collect()
    };
    if !closed {
        let fwd = cost(a, &b, ca, cb, 0, false, 1);
        // Reversed open path: index n-1-i, i.e. shift n-1 read backwards.
        let back = cost(a, &b, ca, cb, n - 1, true, 1);
        return if back < fwd { read(n - 1, true) } else { b };
    }
    // Both run clockwise: try every start, first with a sparse sample,
    // then refine round the best few.
    let step = (n / 64).max(1);
    let mut cands: Vec<(f64, usize, bool)> = Vec::new();
    let mut s = 0;
    while s < n {
        cands.push((cost(a, &b, ca, cb, s, false, step), s, false));
        s += step;
    }
    cands.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut best = (f64::INFINITY, 0, false);
    for &(_, s, rev) in cands.iter().take(4) {
        for d in 0..(2 * step + 1) {
            let shift = (s + n + d - step) % n;
            let c = cost(a, &b, ca, cb, shift, rev, 1);
            if c < best.0 {
                best = (c, shift, rev);
            }
        }
    }
    read(best.1, best.2)
}

fn lerp_paint(a: Option<&Paint>, b: Option<&Paint>, t: f64) -> Option<Paint> {
    let solid = |p: &Paint| p.gradient.is_none();
    match (a, b) {
        (None, None) => None,
        (Some(p), Some(q)) if solid(p) && solid(q) => Some(mix(p.rgba, q.rgba, t)),
        // Fading in or out: the other side's colour, transparent.
        (Some(p), None) if solid(p) => Some(mix(p.rgba, [p.rgba[0], p.rgba[1], p.rgba[2], 0.0], t)),
        (None, Some(q)) if solid(q) => Some(mix([q.rgba[0], q.rgba[1], q.rgba[2], 0.0], q.rgba, t)),
        _ => {
            if t < 0.5 {
                a.cloned()
            } else {
                b.cloned()
            }
        }
    }
}

fn mix(p: [f64; 4], q: [f64; 4], t: f64) -> Paint {
    let m = |i: usize| p[i] + (q[i] - p[i]) * t;
    Paint::from_color(Color {
        r: m(0),
        g: m(1),
        b: m(2),
        a: m(3),
    })
}

pub(super) fn blend_style(
    a: Option<&Arc<ResolvedStyle>>,
    b: Option<&Arc<ResolvedStyle>>,
    t: f64,
) -> Arc<ResolvedStyle> {
    match (a, b) {
        (Some(a), Some(b)) if !Arc::ptr_eq(a, b) => {
            let base = if t < 0.5 { a } else { b };
            let mut st = (**base).clone();
            st.fill = lerp_paint(a.fill.as_ref(), b.fill.as_ref(), t);
            st.stroke = lerp_paint(a.stroke.as_ref(), b.stroke.as_ref(), t);
            st.stroke_weight = a.stroke_weight + (b.stroke_weight - a.stroke_weight) * t;
            Arc::new(st)
        }
        (Some(s), _) | (None, Some(s)) => s.clone(),
        (None, None) => crate::scene::default_style().clone(),
    }
}

/// The pairs blended at `t`. Closed outlines in the same styles fill as
/// one polygon; each hole joins the polygon whose outline it sits in at
/// this `t` (outers run clockwise and holes the other way, so under
/// nonzero filling it cuts it). Open outlines are lines of their own.
pub(super) fn blend(pairs: &[Pair], t: f64, wired: Option<&Value>) -> Vec<Poly> {
    let t = if t.is_finite() { t } else { 0.0 };
    // A contour with no partner takes the other side's first style, so it
    // fades in (or out) with the rest.
    let first_a = pairs.iter().find_map(|p| p.style_a.clone());
    let first_b = pairs.iter().find_map(|p| p.style_b.clone());
    let mut groups: Vec<(StyleKey, Poly)> = Vec::new();
    // Outers first, so the holes find them.
    for hole_pass in [false, true] {
        for p in pairs.iter().filter(|p| p.hole == hole_pass) {
            let pts: Vec<Vec2> =
                p.a.iter()
                    .zip(&p.b)
                    .map(|(x, y)| super::lerp(*x, *y, t))
                    .collect();
            let closed = if t < 0.5 { p.closed_a } else { p.closed_b };
            let sa = p.style_a.clone().or_else(|| first_a.clone());
            let sb = p.style_b.clone().or_else(|| first_b.clone());
            let key = (sa.as_ref().map(Arc::as_ptr), sb.as_ref().map(Arc::as_ptr));
            let found = if !closed {
                None
            } else if hole_pass {
                let c = centroid(&pts);
                groups
                    .iter()
                    .position(|(_, g)| g.closed && winding(&g.rings, c) != 0)
                    .or_else(|| groups.iter().position(|(k, g)| g.closed && *k == key))
            } else {
                groups.iter().position(|(k, g)| g.closed && *k == key)
            };
            match found {
                Some(i) => groups[i].1.rings.push(pts),
                None => {
                    let style = match wired {
                        Some(v) => pin_style(v, closed),
                        None => blend_style(sa.as_ref(), sb.as_ref(), t),
                    };
                    groups.push((
                        key,
                        Poly {
                            rings: vec![pts],
                            closed,
                            style,
                        },
                    ));
                }
            }
        }
    }
    groups.into_iter().map(|(_, p)| p).collect()
}

type StyleKey = (Option<*const ResolvedStyle>, Option<*const ResolvedStyle>);
