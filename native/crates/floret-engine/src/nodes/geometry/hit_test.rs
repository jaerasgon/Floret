//! `geometry/hit-test` (Hit Test): which shapes a list of points is in —
//! paddles and balls, bricks, buttons under the mouse, grid cells.
//!
//! Shapes are axis-aligned `rects` (centres and sizes), `circles` (centres
//! and radii) or every shape drawn by a `layer` (each drawn shape counts as
//! one: a polygon, a rect, a path, a glyph run with its holes; closed
//! outlines by their area, lines by their stroke). `margin` grows every
//! shape (negative shrinks it), e.g. by a ball's radius.
//!
//! Outputs, per point: `inside` (in any shape) and `index` (the first
//! shape it is in, by list order; −1 for none); per shape: `hits` (how many
//! points are in it).

use super::{layer_contours, num_pin, Contour, Pins};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};

#[cfg(test)]
const MODES: [&str; 3] = ["rects", "circles", "layer"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Rects,
    Circles,
    Layer,
}

fn mode_of(params: &Params) -> Mode {
    match super::param_str(params, "mode", "rects") {
        "circles" => Mode::Circles,
        "layer" => Mode::Layer,
        _ => Mode::Rects,
    }
}

fn spread(name: &str, ty: PortType, default: Option<Value>) -> PortSpec {
    let p = PortSpec::new(name, ty).kind(PortKind::Spread);
    match default {
        Some(d) => p.default(d),
        None => p,
    }
}

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![spread("points", PortType::Vec2, None)];
    match mode_of(params) {
        Mode::Rects => {
            v.push(spread(
                "centers",
                PortType::Vec2,
                Some(Value::Vec2(Vec2::default())),
            ));
            v.push(spread(
                "sizes",
                PortType::Vec2,
                Some(Value::Vec2(Vec2 { x: 100.0, y: 100.0 })),
            ));
        }
        Mode::Circles => {
            v.push(spread(
                "centers",
                PortType::Vec2,
                Some(Value::Vec2(Vec2::default())),
            ));
            v.push(spread("radii", PortType::Number, Some(Value::Num(50.0))));
        }
        Mode::Layer => v.push(super::layer_pin("shapes")),
    }
    v.push(num_pin("margin", 0.0));
    v
}

/// A finite point, or `None`.
fn point(v: &Value) -> Option<Vec2> {
    let p = match v {
        Value::Vec2(p) => *p,
        Value::Object(_) => super::to_vec2(v),
        _ => return None,
    };
    (p.x.is_finite() && p.y.is_finite()).then_some(p)
}

/// Element `i` cycling; `None` for an empty spread.
fn cyc(s: &[Value], i: usize) -> Option<&Value> {
    (!s.is_empty()).then(|| &s[i % s.len()])
}

/// A shape a point can be tested against.
enum Shape {
    /// Half extents grown by the margin (empty when either is negative).
    Rect {
        c: Vec2,
        hw: f64,
        hh: f64,
    },
    Circle {
        c: Vec2,
        r: f64,
    },
    /// One drawn shape's outlines: closed ones by area, open ones by
    /// their stroke's half width. `margin` grows (or shrinks) both.
    Outline {
        closed: Vec<Vec<Vec2>>,
        open: Vec<(Vec<Vec2>, f64)>,
        margin: f64,
        /// Bounds grown by the reach, for a quick reject.
        lo: Vec2,
        hi: Vec2,
    },
}

/// Distance from `p` to segment `a`–`b`.
fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// Distance from `p` to a polyline (closing it when `closed`).
fn line_dist(p: Vec2, pts: &[Vec2], closed: bool) -> f64 {
    match pts.len() {
        0 => f64::INFINITY,
        1 => (p.x - pts[0].x).hypot(p.y - pts[0].y),
        n => {
            let mut d = f64::INFINITY;
            for i in 0..n - 1 {
                d = d.min(seg_dist(p, pts[i], pts[i + 1]));
            }
            if closed {
                d = d.min(seg_dist(p, pts[n - 1], pts[0]));
            }
            d
        }
    }
}

/// Nonzero winding number of `rings` around `p`.
fn winding(p: Vec2, rings: &[Vec<Vec2>]) -> i32 {
    let mut w = 0;
    for ring in rings {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let cross = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
            if a.y <= p.y {
                if b.y > p.y && cross > 0.0 {
                    w += 1;
                }
            } else if b.y <= p.y && cross < 0.0 {
                w -= 1;
            }
        }
    }
    w
}

impl Shape {
    fn contains(&self, p: Vec2) -> bool {
        match self {
            Shape::Rect { c, hw, hh } => (p.x - c.x).abs() <= *hw && (p.y - c.y).abs() <= *hh,
            Shape::Circle { c, r } => (p.x - c.x).hypot(p.y - c.y) <= *r,
            Shape::Outline {
                closed,
                open,
                margin,
                lo,
                hi,
            } => {
                if p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y {
                    return false;
                }
                if open
                    .iter()
                    .any(|(pts, half)| line_dist(p, pts, false) <= half + margin)
                {
                    return true;
                }
                if closed.is_empty() {
                    return false;
                }
                let inside = winding(p, closed) != 0;
                if *margin == 0.0 {
                    return inside;
                }
                let d = closed
                    .iter()
                    .map(|r| line_dist(p, r, true))
                    .fold(f64::INFINITY, f64::min);
                if *margin > 0.0 {
                    inside || d <= *margin
                } else {
                    inside && d >= -margin
                }
            }
        }
    }
}

/// Group a layer's contours into one shape per drawn source.
fn outlines(contours: Vec<Contour>, margin: f64) -> Vec<Shape> {
    let mut shapes: Vec<(usize, Shape)> = Vec::new();
    for c in contours {
        if c.pts.is_empty() || c.pts.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
            continue;
        }
        let half = if c.closed {
            0.0
        } else {
            c.style.stroke_weight.max(0.0) / 2.0
        };
        let reach = (half + margin).max(0.0);
        let (mut lo, mut hi) = (
            Vec2 {
                x: f64::INFINITY,
                y: f64::INFINITY,
            },
            Vec2 {
                x: f64::NEG_INFINITY,
                y: f64::NEG_INFINITY,
            },
        );
        for p in &c.pts {
            lo.x = lo.x.min(p.x - reach);
            lo.y = lo.y.min(p.y - reach);
            hi.x = hi.x.max(p.x + reach);
            hi.y = hi.y.max(p.y + reach);
        }
        if shapes.last().map(|(s, _)| *s) != Some(c.source) {
            shapes.push((
                c.source,
                Shape::Outline {
                    closed: Vec::new(),
                    open: Vec::new(),
                    margin,
                    lo,
                    hi,
                },
            ));
        }
        let Some((
            _,
            Shape::Outline {
                closed,
                open,
                lo: l,
                hi: h,
                ..
            },
        )) = shapes.last_mut()
        else {
            continue;
        };
        l.x = l.x.min(lo.x);
        l.y = l.y.min(lo.y);
        h.x = h.x.max(hi.x);
        h.y = h.y.max(hi.y);
        if c.closed {
            closed.push(c.pts);
        } else {
            open.push((c.pts, half));
        }
    }
    shapes.into_iter().map(|(_, s)| s).collect()
}

pub(super) fn hit_test(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    let mode = mode_of(init.params);
    raw(move |inputs, _| {
        let margin = pins.num(inputs, "margin", 0.0);
        let shapes: Vec<Shape> = match mode {
            Mode::Rects | Mode::Circles => {
                let centers = pins.spread(inputs, "centers");
                let other = pins.spread(
                    inputs,
                    if mode == Mode::Rects {
                        "sizes"
                    } else {
                        "radii"
                    },
                );
                let n = if centers.is_empty() || other.is_empty() {
                    0
                } else {
                    centers.len().max(other.len())
                };
                (0..n)
                    .map(|i| {
                        let c = cyc(centers, i).and_then(point).unwrap_or(Vec2 {
                            x: f64::NAN,
                            y: f64::NAN,
                        });
                        let o = cyc(other, i).cloned().unwrap_or(Value::Undefined);
                        if mode == Mode::Rects {
                            let s = point(&o).unwrap_or(Vec2 {
                                x: f64::NAN,
                                y: f64::NAN,
                            });
                            Shape::Rect {
                                c,
                                hw: s.x.abs() / 2.0 + margin,
                                hh: s.y.abs() / 2.0 + margin,
                            }
                        } else {
                            let r = o.as_f64().unwrap_or(f64::NAN);
                            Shape::Circle {
                                c,
                                r: r.abs() + margin,
                            }
                        }
                    })
                    .collect()
            }
            Mode::Layer => outlines(layer_contours(pins.spread(inputs, "shapes")), margin),
        };
        let points = pins.spread(inputs, "points");
        let mut inside = Vec::with_capacity(points.len());
        let mut index = Vec::with_capacity(points.len());
        let mut hits = vec![0.0; shapes.len()];
        for v in points {
            let mut first = None;
            if let Some(p) = point(v) {
                for (k, s) in shapes.iter().enumerate() {
                    if s.contains(p) {
                        hits[k] += 1.0;
                        first.get_or_insert(k);
                    }
                }
            }
            inside.push(Value::Bool(first.is_some()));
            index.push(Value::Num(first.map_or(-1.0, |k| k as f64)));
        }
        let hits: Vec<Value> = hits.into_iter().map(Value::Num).collect();
        Ok(vec![
            Spread::from(inside),
            Spread::from(index),
            Spread::from(hits),
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::{n, node};
    use crate::scene::{default_style, SceneNode};
    use serde_json::json;
    use std::sync::Arc;

    fn v2(x: f64, y: f64) -> Value {
        Value::Vec2(Vec2 { x, y })
    }

    /// (inside, index, hits)
    fn run(mode: &str, given: &[(&str, Vec<Value>)]) -> (Vec<bool>, Vec<f64>, Vec<f64>) {
        let params = json!({ "mode": mode });
        let p: Params = serde_json::from_value(params.clone()).unwrap();
        let ins: Vec<Vec<Value>> = inputs(&p)
            .iter()
            .map(|pin| {
                given
                    .iter()
                    .find(|(k, _)| *k == pin.name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| pin.default.clone().into_iter().collect())
            })
            .collect();
        let out = node("geometry/hit-test", params).eval(ins).unwrap();
        (
            out[0].iter().map(Value::truthy).collect(),
            out[1].iter().map(|v| v.as_f64().unwrap()).collect(),
            out[2].iter().map(|v| v.as_f64().unwrap()).collect(),
        )
    }

    #[test]
    fn points_in_rects() {
        let (inside, index, hits) = run(
            "rects",
            &[
                (
                    "points",
                    vec![
                        v2(0.0, 0.0),
                        v2(-55.0, 0.0),
                        v2(500.0, 0.0),
                        v2(110.0, 10.0),
                    ],
                ),
                ("centers", vec![v2(0.0, 0.0), v2(100.0, 0.0)]),
                ("sizes", vec![v2(100.0, 40.0)]),
            ],
        );
        assert_eq!(inside, [true, false, false, true]);
        assert_eq!(index, [0.0, -1.0, -1.0, 1.0]);
        assert_eq!(hits, [1.0, 1.0]);
        // Edges count; margin grows the shapes; overlap → first index.
        let (_, index, hits) = run(
            "rects",
            &[
                ("points", vec![v2(50.0, 20.0), v2(55.0, 0.0)]),
                ("centers", vec![v2(0.0, 0.0), v2(100.0, 0.0)]),
                ("sizes", vec![v2(100.0, 40.0)]),
                ("margin", vec![n(5.0)]),
            ],
        );
        assert_eq!(index, [0.0, 0.0]);
        assert_eq!(hits, [2.0, 2.0]);
    }

    #[test]
    fn points_in_circles() {
        let (inside, index, hits) = run(
            "circles",
            &[
                ("points", vec![v2(0.0, 0.0), v2(30.0, 40.0), v2(31.0, 40.0)]),
                ("centers", vec![v2(0.0, 0.0)]),
                ("radii", vec![n(50.0)]),
            ],
        );
        assert_eq!(inside, [true, true, false]);
        assert_eq!(index, [0.0, 0.0, -1.0]);
        assert_eq!(hits, [2.0]);
        // A negative margin shrinks.
        let (inside, _, _) = run(
            "circles",
            &[
                ("points", vec![v2(30.0, 40.0)]),
                ("radii", vec![n(50.0)]),
                ("margin", vec![n(-1.0)]),
            ],
        );
        assert_eq!(inside, [false]);
    }

    fn poly(points: &[(f64, f64)], closed: bool) -> Value {
        Value::Scene(Arc::new(SceneNode::Polygon {
            points: Arc::from(
                points
                    .iter()
                    .map(|(x, y)| Vec2 { x: *x, y: *y })
                    .collect::<Vec<_>>(),
            ),
            closed,
            style: default_style().clone(),
            tension: None,
        }))
    }

    #[test]
    fn points_in_layer_shapes() {
        let tri = poly(&[(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)], true);
        let square = Value::Scene(Arc::new(SceneNode::Group {
            children: Arc::from(vec![match poly(
                &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
                true,
            ) {
                Value::Scene(s) => s,
                _ => unreachable!(),
            }]),
            transform: Some([1.0, 0.0, 0.0, 1.0, 200.0, 0.0]),
            blend_mode: None,
            clip: None,
        }));
        let (inside, index, hits) = run(
            "layer",
            &[
                (
                    "points",
                    vec![
                        v2(10.0, 10.0),
                        v2(90.0, 90.0),
                        v2(205.0, 5.0),
                        v2(5.0, 205.0),
                    ],
                ),
                ("shapes", vec![tri, square]),
            ],
        );
        assert_eq!(inside, [true, false, true, false]);
        assert_eq!(index, [0.0, -1.0, 1.0, -1.0]);
        assert_eq!(hits, [1.0, 1.0]);
    }

    #[test]
    fn holes_lines_and_margins_in_layers() {
        // A glyph-like shape: outer ring and a reversed hole, one source.
        let outer = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let hole = [(30.0, 30.0), (30.0, 70.0), (70.0, 70.0), (70.0, 30.0)];
        let mut contours = Vec::new();
        for ring in [&outer[..], &hole[..]] {
            contours.push(Contour {
                pts: ring.iter().map(|(x, y)| Vec2 { x: *x, y: *y }).collect(),
                closed: true,
                style: default_style().clone(),
                source: 0,
            });
        }
        let shapes = outlines(contours, 0.0);
        assert_eq!(shapes.len(), 1);
        assert!(shapes[0].contains(Vec2 { x: 10.0, y: 10.0 }));
        assert!(!shapes[0].contains(Vec2 { x: 50.0, y: 50.0 }));
        // An open line is hit within its stroke (default weight 1 → 0.5)
        // plus the margin.
        let line = poly(&[(0.0, 0.0), (100.0, 0.0)], false);
        let (inside, _, _) = run(
            "layer",
            &[
                ("points", vec![v2(50.0, 0.4), v2(50.0, 3.0)]),
                ("shapes", vec![line.clone()]),
            ],
        );
        assert_eq!(inside, [true, false]);
        let (inside, _, _) = run(
            "layer",
            &[
                ("points", vec![v2(50.0, 3.0)]),
                ("shapes", vec![line]),
                ("margin", vec![n(3.0)]),
            ],
        );
        assert_eq!(inside, [true]);
        // A margin grows a closed shape outward and a negative one inward.
        let sq = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], true);
        let (inside, _, _) = run(
            "layer",
            &[
                ("points", vec![v2(12.0, 5.0), v2(5.0, 5.0)]),
                ("shapes", vec![sq.clone()]),
                ("margin", vec![n(2.5)]),
            ],
        );
        assert_eq!(inside, [true, true]);
        let (inside, _, _) = run(
            "layer",
            &[
                ("points", vec![v2(1.0, 5.0), v2(5.0, 5.0)]),
                ("shapes", vec![sq]),
                ("margin", vec![n(-2.0)]),
            ],
        );
        assert_eq!(inside, [false, true]);
    }

    #[test]
    fn empty_and_odd_inputs() {
        // No points: nothing per point, zero hits per shape.
        let (inside, _, hits) = run("rects", &[("points", vec![])]);
        assert!(inside.is_empty());
        assert_eq!(hits, [0.0]);
        // No shapes: every point misses.
        let (inside, index, hits) = run(
            "circles",
            &[("points", vec![v2(0.0, 0.0)]), ("centers", vec![])],
        );
        assert_eq!((inside, index), (vec![false], vec![-1.0]));
        assert!(hits.is_empty());
        // NaN points and shapes never hit; zero sizes hit only the centre.
        let (inside, _, _) = run(
            "rects",
            &[
                (
                    "points",
                    vec![v2(f64::NAN, 0.0), v2(0.0, 0.0), v2(0.1, 0.0)],
                ),
                ("sizes", vec![v2(0.0, 0.0)]),
            ],
        );
        assert_eq!(inside, [false, true, false]);
        let (inside, _, _) = run(
            "circles",
            &[("points", vec![v2(0.0, 0.0)]), ("radii", vec![n(f64::NAN)])],
        );
        assert_eq!(inside, [false]);
        let (inside, _, hits) = run("layer", &[("points", vec![v2(0.0, 0.0)])]);
        assert_eq!(inside, [false]);
        assert!(hits.is_empty());
    }

    #[test]
    fn static_spec_matches_default_mode() {
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("geometry/hit-test").unwrap();
        let st: Vec<String> = spec.inputs.iter().map(|p| p.name.clone()).collect();
        let dy: Vec<String> = inputs(&spec.default_params)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(st, dy);
        assert_eq!(spec.param_options["mode"], MODES);
    }
}
