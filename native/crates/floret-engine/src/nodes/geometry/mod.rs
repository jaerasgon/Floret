//! `geometry/*` nodes (native-only): point scattering, Voronoi / Delaunay /
//! hull cells, path operations, shape booleans, L-systems, trimming,
//! morphing and connecting points.
//!
//! The polygon-producing nodes share one output shape: `shape` (every
//! polygon in one group layer), `layers` (one layer per polygon; `cells`
//! on `geometry/cells`), `points` (every ring's vertices, flat) and
//! `counts` (vertices per ring), which pairs with `spread/at-group` to pick
//! one ring.
//!
//! Layers are read as contours (see [`layer_contours`]): polygons, paths,
//! lines, rects, ellipses, arcs, triangles, quads, glyph outlines and
//! `draw/text` (laid out as it draws), with group transforms applied.
//!
//! Every random draw starts from [`rng`], the one place a patch-wide seed
//! can be mixed in.

mod boolean;
mod cells;
mod connect;
mod contours;
mod lsystem;
mod measure;
pub(crate) use measure::fade_paint;
mod morph;
#[cfg(test)]
mod motion_tests;
mod path_ops;
mod relax;
mod scatter;
#[cfg(test)]
mod tests;
mod trim;

use super::random::hash32;
use super::shared::to_vec2;
use super::style::style_arg;
use super::Registrar;
use crate::js::Mulberry32;
use crate::node::NodeInit;
use crate::scene::{
    self, default_style, line_default_style, resolve_style, Affine, ResolvedStyle, SceneNode,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("geometry/scatter", scatter::scatter);
    r.dynamic("geometry/scatter", scatter::inputs);
    r.node("geometry/cells", cells::cells);
    r.dynamic("geometry/cells", cells::inputs);
    r.node("geometry/path-ops", path_ops::path_ops);
    r.dynamic("geometry/path-ops", path_ops::inputs);
    r.node("geometry/boolean", boolean::boolean);
    r.node("geometry/lsystem", lsystem::lsystem);
    r.node("geometry/trim", trim::trim);
    r.node("geometry/connect", connect::connect);
    r.dynamic("geometry/connect", connect::inputs);
    r.node("geometry/morph", morph::morph);
    r.node("geometry/contours", contours::contours);
    r.dynamic("geometry/contours", contours::inputs);
}

/// Most points / vertices a geometry node emits (it warns when it stops).
pub(crate) const MAX_POINTS: usize = 50_000;

// ─── Seeds ───────────────────────────────────────────────────────────────────

/// The random stream for a node's `seed`. Every geometry node draws its
/// randomness from here, with the patch seed mixed in ([`crate::seed`]).
pub(crate) fn rng(seed: f64) -> Mulberry32 {
    Mulberry32::new(hash32(
        crate::seed::mix(seed),
        0.0,
        f64::from(0x6765_6f6d_u32),
    ))
}

// ─── Modes and pins ──────────────────────────────────────────────────────────

/// A string param (`mode`), or `fallback` when unset / not a string.
pub(crate) fn param_str<'a>(params: &'a Params, key: &str, fallback: &'a str) -> &'a str {
    match params.get(key) {
        Some(serde_json::Value::String(s)) => s,
        _ => fallback,
    }
}

/// The node's live `mode` (falls back to the saved one, then `fallback`).
pub(crate) fn live_mode(cx: &crate::node::EvalCtx, fallback: &str) -> String {
    match cx.param("mode") {
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => fallback.to_owned(),
    }
}

pub(crate) fn num_pin(name: &str, default: f64) -> PortSpec {
    PortSpec::new(name, PortType::Number).default(Value::Num(default))
}

pub(crate) fn int_pin(name: &str, default: f64) -> PortSpec {
    let mut p = num_pin(name, default);
    p.integer = true;
    p
}

pub(crate) fn vec2_pin(name: &str, x: f64, y: f64) -> PortSpec {
    PortSpec::new(name, PortType::Vec2).default(Value::Vec2(Vec2 { x, y }))
}

pub(crate) fn layer_pin(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Layer).kind(PortKind::Spread)
}

pub(crate) fn style_pin() -> PortSpec {
    PortSpec::new("style", PortType::Style)
}

/// A node's inputs looked up by name: with dynamic inputs the pins move
/// with the mode, and a pin the compiled node doesn't have reads as unset.
pub(crate) struct Pins(Vec<String>);

impl Pins {
    pub fn new(init: &NodeInit) -> Pins {
        Pins(init.inputs.iter().map(|p| p.name.clone()).collect())
    }

    pub fn spread<'a>(&self, inputs: &'a [Spread], name: &str) -> &'a [Value] {
        self.0
            .iter()
            .position(|n| n == name)
            .and_then(|i| inputs.get(i))
            .map_or(&[], |s| &s[..])
    }

    pub fn value(&self, inputs: &[Spread], name: &str) -> Value {
        self.spread(inputs, name)
            .first()
            .cloned()
            .unwrap_or(Value::Undefined)
    }

    /// A finite number, else `fallback`.
    pub fn num(&self, inputs: &[Spread], name: &str, fallback: f64) -> f64 {
        match self.value(inputs, name) {
            Value::Num(n) if n.is_finite() => n,
            Value::Bool(b) => f64::from(u8::from(b)),
            _ => fallback,
        }
    }

    pub fn vec2(&self, inputs: &[Spread], name: &str, fallback: Vec2) -> Vec2 {
        match self.value(inputs, name) {
            v @ (Value::Vec2(_) | Value::Object(_)) => to_vec2(&v),
            _ => fallback,
        }
    }

    pub fn bool(&self, inputs: &[Spread], name: &str, fallback: bool) -> bool {
        match self.value(inputs, name) {
            Value::Undefined | Value::Null => fallback,
            v => v.truthy(),
        }
    }

    /// Finite points of a vec2 spread.
    pub fn points(&self, inputs: &[Spread], name: &str) -> Vec<Vec2> {
        self.spread(inputs, name)
            .iter()
            .filter(|v| matches!(v, Value::Vec2(_) | Value::Object(_)))
            .map(to_vec2)
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect()
    }
}

// ─── Layers in: contours ─────────────────────────────────────────────────────

/// One outline read from a layer, in scene coordinates.
#[derive(Clone, Debug)]
pub(crate) struct Contour {
    pub pts: Vec<Vec2>,
    pub closed: bool,
    pub style: Arc<ResolvedStyle>,
    /// The drawn shape it came from: a glyph run's outer and hole
    /// contours share one, so they can be offset together.
    pub source: usize,
}

/// Every contour of the layers on a wire.
pub(crate) fn layer_contours(layers: &[Value]) -> Vec<Contour> {
    let mut out = Vec::new();
    let mut source = 0;
    for v in layers {
        if let Value::Scene(n) = v {
            walk(n, &scene::IDENTITY, &mut source, &mut out);
        }
    }
    out
}

/// Line segments per Catmull–Rom span (as the renderer draws them).
const SPLINE_STEPS: usize = 14;

fn walk(node: &SceneNode, m: &Affine, source: &mut usize, out: &mut Vec<Contour>) {
    let at = |x: f64, y: f64| apply(m, Vec2 { x, y });
    let mut push = |pts: Vec<Vec2>, closed: bool, style: &Arc<ResolvedStyle>| {
        out.push(Contour {
            pts,
            closed,
            style: style.clone(),
            source: *source,
        });
    };
    match node {
        SceneNode::Group {
            children,
            transform,
            ..
        } => {
            let gm = transform.map_or(*m, |t| scene::mul(m, &t));
            for c in children.iter() {
                walk(c, &gm, source, out);
            }
            return;
        }
        SceneNode::Polygon {
            points,
            closed,
            style,
            tension,
        } => {
            let t = tension.unwrap_or(0.0);
            let pts: Vec<Vec2> = if t != 0.0 && t.is_finite() && points.len() > 2 {
                let mut flat = vec![points[0]];
                let mut p0 = points[0];
                for bz in scene::catmull_rom_beziers(points, *closed, t) {
                    for k in 1..=SPLINE_STEPS {
                        let u = k as f64 / SPLINE_STEPS as f64;
                        let w = 1.0 - u;
                        let (a, b, c, d) = (w * w * w, 3.0 * w * w * u, 3.0 * w * u * u, u * u * u);
                        flat.push(Vec2 {
                            x: a * p0.x + b * bz.c1.x + c * bz.c2.x + d * bz.end.x,
                            y: a * p0.y + b * bz.c1.y + c * bz.c2.y + d * bz.end.y,
                        });
                    }
                    p0 = bz.end;
                }
                if *closed {
                    flat.pop();
                }
                flat
            } else {
                points.to_vec()
            };
            push(
                pts.into_iter().map(|p| apply(m, p)).collect(),
                *closed,
                style,
            );
        }
        SceneNode::Line {
            x1,
            y1,
            x2,
            y2,
            style,
        } => push(vec![at(*x1, *y1), at(*x2, *y2)], false, style),
        SceneNode::Rect {
            x,
            y,
            w,
            h,
            mode,
            style,
        } => {
            let [bx, by, bw, bh] = scene::resolve_box(*x, *y, *w, *h, mode);
            let pts = vec![
                at(bx, by),
                at(bx + bw, by),
                at(bx + bw, by + bh),
                at(bx, by + bh),
            ];
            push(pts, true, style);
        }
        SceneNode::Ellipse {
            x,
            y,
            w,
            h,
            mode,
            style,
        } => {
            let [bx, by, bw, bh] = scene::resolve_box(*x, *y, *w, *h, mode);
            let (cx, cy, rx, ry) = (bx + bw / 2.0, by + bh / 2.0, bw / 2.0, bh / 2.0);
            let n = circle_steps(rx.abs().max(ry.abs()) * scale_of(m));
            let pts = (0..n)
                .map(|i| {
                    let a = i as f64 / n as f64 * std::f64::consts::TAU;
                    at(cx + a.cos() * rx, cy + a.sin() * ry)
                })
                .collect();
            push(pts, true, style);
        }
        SceneNode::Arc {
            x,
            y,
            w,
            h,
            start,
            stop,
            mode,
            style,
        } => {
            let [bx, by, bw, bh] = scene::resolve_box(*x, *y, *w, *h, mode);
            let (cx, cy, rx, ry) = (bx + bw / 2.0, by + bh / 2.0, bw / 2.0, bh / 2.0);
            let sweep = (stop - start).abs().min(std::f64::consts::TAU);
            let full = circle_steps(rx.abs().max(ry.abs()) * scale_of(m));
            let n = ((full as f64 * sweep / std::f64::consts::TAU).ceil() as usize).max(2);
            let pts = (0..=n)
                .map(|i| {
                    let a = start + (stop - start) * (i as f64 / n as f64);
                    at(cx + a.cos() * rx, cy + a.sin() * ry)
                })
                .collect();
            // Drawn filled as a chord when it has a fill.
            push(pts, style.fill.is_some(), style);
        }
        SceneNode::Triangle { pts: p, style } => push(
            vec![at(p[0], p[1]), at(p[2], p[3]), at(p[4], p[5])],
            true,
            style,
        ),
        SceneNode::Quad { pts: p, style } => push(
            vec![
                at(p[0], p[1]),
                at(p[2], p[3]),
                at(p[4], p[5]),
                at(p[6], p[7]),
            ],
            true,
            style,
        ),
        SceneNode::Glyphs(run) => {
            for c in crate::text::outline::run_contours(run) {
                push(
                    c.into_iter().map(|p| apply(m, p)).collect(),
                    true,
                    &run.style,
                );
            }
        }
        SceneNode::Text { text, x, y, style } => {
            // `draw/text`: its glyphs, laid out as it draws them.
            let Some(b) = crate::text::block(text, style, style.letter_spacing) else {
                return;
            };
            let (ox, oy) = b.origin(style, *x, *y);
            let run = scene::GlyphRun {
                glyphs: b
                    .glyphs
                    .iter()
                    .map(|g| scene::PlacedGlyph {
                        face: g.glyph.face,
                        id: g.glyph.id,
                        x: ox + g.glyph.x,
                        y: oy + g.glyph.y,
                        angle: 0.0,
                    })
                    .collect(),
                size: style.font_size,
                style: style.clone(),
            };
            for c in crate::text::outline::run_contours(&run) {
                push(c.into_iter().map(|p| apply(m, p)).collect(), true, style);
            }
        }
        _ => return,
    }
    *source += 1;
}

/// Segments for a full circle of radius `r` (about 3 units each).
fn circle_steps(r: f64) -> usize {
    if !r.is_finite() {
        return 24;
    }
    ((std::f64::consts::TAU * r / 3.0).ceil() as usize).clamp(24, 256)
}

fn apply(m: &Affine, p: Vec2) -> Vec2 {
    Vec2 {
        x: m[0] * p.x + m[2] * p.y + m[4],
        y: m[1] * p.x + m[3] * p.y + m[5],
    }
}

fn scale_of(m: &Affine) -> f64 {
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
}

// ─── Polygons out ────────────────────────────────────────────────────────────

/// A polygon (outer ring, then holes) or an open polyline (one ring).
#[derive(Clone, Debug)]
pub(crate) struct Poly {
    pub rings: Vec<Vec<Vec2>>,
    pub closed: bool,
    pub style: Arc<ResolvedStyle>,
}

/// The style a node's `style` pin gives its polygons (a wired chain, else
/// the draw nodes' defaults: white fill for closed shapes, a thin white
/// line for open ones).
pub(crate) fn pin_style(style: &Value, closed: bool) -> Arc<ResolvedStyle> {
    let fallback = if closed {
        default_style()
    } else {
        line_default_style()
    };
    resolve_style(style_arg(style), fallback)
}

/// Outputs `(shape, layers, points, counts)` for a list of polygons.
pub(crate) fn poly_outputs(polys: &[Poly]) -> Vec<Spread> {
    let layers: Vec<Arc<SceneNode>> = polys.iter().filter_map(poly_layer).collect();
    let mut points = Vec::new();
    let mut counts = Vec::new();
    for p in polys {
        for r in &p.rings {
            points.extend(r.iter().map(|v| Value::Vec2(*v)));
            counts.push(Value::Num(r.len() as f64));
        }
    }
    vec![
        crate::value::one(Value::Scene(Arc::new(SceneNode::group(layers.clone())))),
        Arc::from(layers.into_iter().map(Value::Scene).collect::<Vec<_>>()),
        Arc::from(points),
        Arc::from(counts),
    ]
}

fn polygon(points: Vec<Vec2>, closed: bool, style: Arc<ResolvedStyle>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Polygon {
        points: Arc::from(points),
        closed,
        style,
        tension: None,
    })
}

/// One polygon as a layer. A polygon with holes fills as one outline that
/// visits every ring from the outer ring's first point and comes back
/// (each bridge is crossed both ways, so it cancels under nonzero winding,
/// and holes run the other way round), and strokes each ring on its own so
/// the bridges never show.
fn poly_layer(p: &Poly) -> Option<Arc<SceneNode>> {
    let rings: Vec<&Vec<Vec2>> = p.rings.iter().filter(|r| !r.is_empty()).collect();
    match rings.as_slice() {
        [] => None,
        [one] => Some(polygon((*one).clone(), p.closed, p.style.clone())),
        _ => {
            let mut parts = Vec::new();
            if p.style.fill.is_some() {
                // Outer ring, then out to each hole and back again.
                let home = rings[0][0];
                let mut all = Vec::new();
                for r in &rings {
                    all.extend_from_slice(r);
                    all.push(r[0]);
                    all.push(home);
                }
                let fill = ResolvedStyle {
                    stroke: None,
                    ..(*p.style).clone()
                };
                parts.push(polygon(all, true, Arc::new(fill)));
            }
            if p.style.stroke.is_some() {
                let line = Arc::new(ResolvedStyle {
                    fill: None,
                    ..(*p.style).clone()
                });
                for r in &rings {
                    parts.push(polygon((*r).clone(), true, line.clone()));
                }
            }
            Some(Arc::new(SceneNode::group(parts)))
        }
    }
}

// ─── Shared maths ────────────────────────────────────────────────────────────

pub(crate) fn sub(a: Vec2, b: Vec2) -> Vec2 {
    Vec2 {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

pub(crate) fn dist(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

pub(crate) fn lerp(a: Vec2, b: Vec2, t: f64) -> Vec2 {
    Vec2 {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

/// Signed area (positive when the ring runs clockwise on screen, y down).
pub(crate) fn signed_area(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Area centroid of a ring (the vertex mean when it has no area).
pub(crate) fn centroid(ring: &[Vec2]) -> Vec2 {
    let n = ring.len();
    if n == 0 {
        return Vec2::default();
    }
    let a = signed_area(ring);
    if a.abs() < 1e-12 {
        let s = ring.iter().fold(Vec2::default(), |s, p| Vec2 {
            x: s.x + p.x,
            y: s.y + p.y,
        });
        return Vec2 {
            x: s.x / n as f64,
            y: s.y / n as f64,
        };
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        let k = p.x * q.y - q.x * p.y;
        cx += (p.x + q.x) * k;
        cy += (p.y + q.y) * k;
    }
    Vec2 {
        x: cx / (6.0 * a),
        y: cy / (6.0 * a),
    }
}

/// Keep the part of a convex polygon where `n · p <= c` (Sutherland–Hodgman).
pub(crate) fn clip_half_plane(poly: &[Vec2], n: Vec2, c: f64) -> Vec<Vec2> {
    let side = |p: Vec2| n.x * p.x + n.y * p.y - c;
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa <= 0.0) != (sb <= 0.0) {
            out.push(lerp(a, b, sa / (sa - sb)));
        }
    }
    out
}

/// A convex polygon shrunk by `d` on every side (empty when it vanishes).
pub(crate) fn inset_convex(poly: &[Vec2], d: f64) -> Vec<Vec2> {
    if d <= 0.0 || poly.len() < 3 {
        return poly.to_vec();
    }
    // Which side of each edge is outside depends on the ring's direction.
    let sign = if signed_area(poly) > 0.0 { 1.0 } else { -1.0 };
    let mut out = poly.to_vec();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let e = sub(b, a);
        let len = e.x.hypot(e.y);
        if len < 1e-12 {
            continue;
        }
        // Outward unit normal; keep n · p <= n · a − d.
        let n = Vec2 {
            x: e.y / len * sign,
            y: -e.x / len * sign,
        };
        out = clip_half_plane(&out, n, n.x * a.x + n.y * a.y - d);
        if out.len() < 3 {
            return Vec::new();
        }
    }
    out
}
