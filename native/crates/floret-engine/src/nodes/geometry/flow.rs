//! `geometry/contours`, mode `flow`: streamlines of a vector field, the
//! plotter-art flow-field look.
//!
//! The field is a wired `vectors` spread, `cols` a row, laid over the
//! `center` / `size` rectangle like the `field` mode's values (e.g. Noise
//! in curl mode on Grid positions), read between nodes bilinearly. Unwired,
//! it is built in: `field` `angle` turns each point's direction by Perlin
//! noise (lines that gather into rivers) and `curl` follows curl noise
//! (swirls that never cross), both at `scale` and `seed`. `turn` (radians)
//! turns every direction, wired or built in, so animating it sweeps the
//! lines round.
//!
//! Lines start from the wired `seeds`, or, unwired, the way Jobard and
//! Lefer (1997) space them evenly: each new line seeds the next ones
//! `spacing` to either side of it, and a shuffled grid of starts fills
//! whatever that leaves. A line runs both ways from its start in steps of
//! `step` (midpoint rule) until it leaves the rectangle, the field dies
//! out, it grows `maxLength` long, or it comes within half a `spacing` of
//! another line; lines shorter than `minLength` are dropped. `spacing` 0
//! lets lines cross and bunch (every seed traced in full).
//!
//! Each line is an open path in the `style` (a white line unwired), or
//! with `colors` wired a colour picked per line at random; `levels` out
//! gives each line's length.

use super::{
    as_line, flat, int_pin, num_pin, pin_style, poly_outputs, rng, rng_salted, vec2_pin, Pins,
    Poly, MAX_POINTS,
};
use crate::nodes::noise::fbm_2d;
use crate::spec::{PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::f64::consts::TAU;
use std::sync::Arc;

const SIZE: Vec2 = Vec2 { x: 400.0, y: 400.0 };
/// Most lines traced.
pub(super) const MAX_LINES: usize = 10_000;

/// The mode's pins, before `colors` and `style`.
pub(super) fn inputs() -> Vec<PortSpec> {
    let mut field = PortSpec::new("field", PortType::String).default(Value::str("angle"));
    field.param = Some("field".into());
    vec![
        PortSpec::new("seeds", PortType::Vec2).kind(PortKind::Spread),
        PortSpec::new("vectors", PortType::Vec2).kind(PortKind::Spread),
        int_pin("cols", 16.0),
        vec2_pin("center", 0.0, 0.0),
        vec2_pin("size", SIZE.x, SIZE.y),
        field,
        num_pin("scale", 0.006),
        int_pin("seed", 1.0),
        num_pin("turn", 0.0),
        num_pin("spacing", 10.0),
        num_pin("step", 2.0),
        num_pin("maxLength", 400.0),
        num_pin("minLength", 20.0),
    ]
}

/// A vector field over a rectangle.
pub(super) enum Field {
    /// Perlin noise turned into a direction.
    Angle { scale: f64, seed: f64 },
    /// Curl noise.
    Curl { scale: f64, seed: f64 },
    /// Vectors on a grid, `cols` a row, node (0, 0) at `origin`.
    Grid {
        cols: usize,
        rows: usize,
        v: Vec<Vec2>,
        origin: Vec2,
        dx: f64,
        dy: f64,
    },
}

impl Field {
    /// A wired grid of vectors over the rectangle (`None` when there are
    /// fewer than two rows of `cols`).
    pub fn grid(v: Vec<Vec2>, cols: usize, center: Vec2, size: Vec2) -> Option<Field> {
        if cols < 2 || v.len() < cols * 2 {
            return None;
        }
        let rows = v.len() / cols;
        let (w, h) = (size.x.abs(), size.y.abs());
        Some(Field::Grid {
            cols,
            rows,
            v: v[..cols * rows].to_vec(),
            origin: Vec2 {
                x: center.x - w / 2.0,
                y: center.y - h / 2.0,
            },
            dx: w / (cols - 1) as f64,
            dy: h / (rows - 1) as f64,
        })
    }

    /// The field's direction at `p` (not normalised), `None` where it is
    /// undefined.
    pub fn at(&self, p: Vec2) -> Option<Vec2> {
        let v = match self {
            Field::Angle { scale, seed } => {
                // Perlin sits mostly in 0.25..0.75: that span sweeps about
                // one and a half turns.
                let n = fbm_2d(p.x * scale, p.y * scale, *seed, 2.0, 0.5);
                let a = (n - 0.5) * TAU * 3.0;
                Vec2 {
                    x: a.cos(),
                    y: a.sin(),
                }
            }
            Field::Curl { scale, seed } => {
                let (x, y, e) = (p.x * scale, p.y * scale, 1e-3);
                let n = |x: f64, y: f64| fbm_2d(x, y, *seed, 2.0, 0.5);
                Vec2 {
                    x: (n(x, y + e) - n(x, y - e)) / (2.0 * e),
                    y: -(n(x + e, y) - n(x - e, y)) / (2.0 * e),
                }
            }
            Field::Grid {
                cols,
                rows,
                v,
                origin,
                dx,
                dy,
            } => {
                let fx = if *dx > 0.0 {
                    (p.x - origin.x) / dx
                } else {
                    0.0
                };
                let fy = if *dy > 0.0 {
                    (p.y - origin.y) / dy
                } else {
                    0.0
                };
                if !(fx >= 0.0 && fy >= 0.0) {
                    return None;
                }
                let (i, j) = (fx.floor() as usize, fy.floor() as usize);
                if i >= *cols || j >= *rows {
                    return None;
                }
                let (i1, j1) = ((i + 1).min(cols - 1), (j + 1).min(rows - 1));
                let (u, w) = (fx - i as f64, fy - j as f64);
                let g = |i: usize, j: usize| v[j * cols + i];
                let mix = |a: Vec2, b: Vec2, t: f64| super::lerp(a, b, t);
                mix(mix(g(i, j), g(i1, j), u), mix(g(i, j1), g(i1, j1), u), w)
            }
        };
        (v.x.is_finite() && v.y.is_finite()).then_some(v)
    }

    /// The unit direction at `p` turned by `turn` radians.
    fn dir(&self, p: Vec2, turn: (f64, f64)) -> Option<Vec2> {
        let v = self.at(p)?;
        let m = v.x.hypot(v.y);
        if m < 1e-12 {
            return None;
        }
        let (c, s) = turn;
        Some(Vec2 {
            x: (v.x * c - v.y * s) / m,
            y: (v.x * s + v.y * c) / m,
        })
    }
}

/// How lines are traced.
pub(super) struct Trace {
    pub center: Vec2,
    pub size: Vec2,
    /// Radians added to every direction.
    pub turn: f64,
    pub spacing: f64,
    pub step: f64,
    pub max_len: f64,
    pub min_len: f64,
    pub seed: f64,
}

/// Points already drawn, bucketed by `cell`, each tagged with its line.
struct Buckets {
    cell: f64,
    origin: Vec2,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<(Vec2, usize)>>,
}

impl Buckets {
    fn new(origin: Vec2, w: f64, h: f64, cell: f64) -> Buckets {
        let cols = ((w / cell).ceil() as usize).clamp(1, 2048);
        let rows = ((h / cell).ceil() as usize).clamp(1, 2048);
        // Wider buckets when the rectangle has more than the cap allows.
        let cell = cell.max(w / cols as f64).max(h / rows as f64);
        Buckets {
            cell,
            origin,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
        }
    }

    fn key(&self, p: Vec2) -> (isize, isize) {
        (
            ((p.x - self.origin.x) / self.cell).floor() as isize,
            ((p.y - self.origin.y) / self.cell).floor() as isize,
        )
    }

    fn add(&mut self, p: Vec2, line: usize) {
        let (i, j) = self.key(p);
        if i >= 0 && j >= 0 && (i as usize) < self.cols && (j as usize) < self.rows {
            let k = j as usize * self.cols + i as usize;
            self.cells[k].push((p, line));
        }
    }

    /// Whether a point of another line than `own` lies within `r`.
    fn near(&self, p: Vec2, r: f64, own: usize) -> bool {
        let (ci, cj) = self.key(p);
        let reach = (r / self.cell).ceil() as isize;
        let r2 = r * r;
        for j in cj - reach..=cj + reach {
            for i in ci - reach..=ci + reach {
                if i < 0 || j < 0 || i as usize >= self.cols || j as usize >= self.rows {
                    continue;
                }
                for (q, l) in &self.cells[j as usize * self.cols + i as usize] {
                    if *l != own && (q.x - p.x).powi(2) + (q.y - p.y).powi(2) < r2 {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// The streamlines of `field` (each a polyline at least `min_len` long),
/// and whether a cap stopped the tracing early.
pub(super) fn streamlines(field: &Field, seeds: &[Vec2], o: &Trace) -> (Vec<Vec<Vec2>>, bool) {
    let (w, h) = (o.size.x.abs(), o.size.y.abs());
    let ok = |v: f64| v.is_finite();
    if !(ok(w) && ok(h) && ok(o.center.x) && ok(o.center.y)) || w <= 0.0 || h <= 0.0 {
        return (Vec::new(), false);
    }
    let lo = Vec2 {
        x: o.center.x - w / 2.0,
        y: o.center.y - h / 2.0,
    };
    let inside = |p: Vec2| p.x >= lo.x && p.y >= lo.y && p.x <= lo.x + w && p.y <= lo.y + h;
    let spacing = if o.spacing.is_finite() {
        o.spacing.max(0.0)
    } else {
        0.0
    };
    // Steps at least 1/2000 of the rectangle, at most half a spacing.
    let mut step = if o.step.is_finite() && o.step > 0.0 {
        o.step
    } else {
        2.0
    };
    step = step.max(w.max(h) / 2000.0);
    if spacing > 0.0 {
        step = step.min(spacing / 2.0);
    }
    let max_len = if o.max_len.is_finite() && o.max_len > 0.0 {
        o.max_len
    } else {
        400.0
    };
    let max_steps = ((max_len / step).ceil() as usize).min(MAX_POINTS);
    let min_len = if o.min_len.is_finite() {
        o.min_len.max(0.0)
    } else {
        0.0
    };
    let turn = (o.turn.cos(), o.turn.sin());
    let turn = if turn.0.is_finite() && turn.1.is_finite() {
        turn
    } else {
        (1.0, 0.0)
    };
    let test = spacing * 0.5;
    let mut buckets = Buckets::new(lo, w, h, if spacing > 0.0 { spacing } else { w.max(h) });
    let mut lines: Vec<Vec<Vec2>> = Vec::new();
    let mut total = 0usize;
    let mut capped = false;

    // One half of a line from `p`, along (`sign` 1) or against the field.
    let half = |p: Vec2, sign: f64, id: usize, buckets: &Buckets, budget: usize| {
        let mut pts = Vec::new();
        let mut p = p;
        for _ in 0..budget {
            let Some(d1) = field.dir(p, turn) else { break };
            let mid = Vec2 {
                x: p.x + d1.x * step * 0.5 * sign,
                y: p.y + d1.y * step * 0.5 * sign,
            };
            let Some(d2) = field.dir(mid, turn) else {
                break;
            };
            let q = Vec2 {
                x: p.x + d2.x * step * sign,
                y: p.y + d2.y * step * sign,
            };
            if !inside(q) || (spacing > 0.0 && buckets.near(q, test, id)) {
                break;
            }
            pts.push(q);
            p = q;
        }
        pts
    };

    let auto = seeds.is_empty();
    let mut queue: std::collections::VecDeque<Vec2> = seeds.iter().copied().collect();
    // The grid of fallback starts, shuffled by the seed.
    let mut grid: Vec<Vec2> = Vec::new();
    if auto {
        let cell = if spacing > 0.0 {
            spacing * 2.0
        } else {
            w.max(h) / 24.0
        };
        let cols = ((w / cell).ceil() as usize).clamp(1, 512);
        let rows = ((h / cell).ceil() as usize).clamp(1, 512);
        for j in 0..rows {
            for i in 0..cols {
                grid.push(Vec2 {
                    x: lo.x + (i as f64 + 0.5) * w / cols as f64,
                    y: lo.y + (j as f64 + 0.5) * h / rows as f64,
                });
            }
        }
        let mut r = rng(o.seed);
        for i in (1..grid.len()).rev() {
            let j = (r.next_f64() * (i + 1) as f64) as usize;
            grid.swap(i, j.min(i));
        }
        grid.reverse();
    }
    loop {
        let start = match queue.pop_front() {
            Some(p) => p,
            None => match grid.pop() {
                Some(p) => p,
                None => break,
            },
        };
        if lines.len() >= MAX_LINES || total >= MAX_POINTS {
            capped = true;
            break;
        }
        if !inside(start) || (spacing > 0.0 && buckets.near(start, spacing * 0.999, usize::MAX)) {
            continue;
        }
        let id = lines.len();
        let budget = max_steps.min(MAX_POINTS - total);
        let fwd = half(start, 1.0, id, &buckets, budget);
        let back = half(start, -1.0, id, &buckets, budget.saturating_sub(fwd.len()));
        let mut line: Vec<Vec2> = back.into_iter().rev().collect();
        line.push(start);
        line.extend(fwd);
        // Too long for the budget: trim the back half's start.
        if line.len() > max_steps + 1 {
            let extra = line.len() - (max_steps + 1);
            line.drain(..extra);
        }
        let len = line
            .windows(2)
            .map(|p| super::dist(p[0], p[1]))
            .sum::<f64>();
        if line.len() < 2 || len < min_len {
            continue;
        }
        for p in &line {
            buckets.add(*p, id);
        }
        if auto && spacing > 0.0 {
            // New starts `spacing` to either side, every few steps.
            let every = ((spacing / step).round() as usize).max(1);
            for k in (0..line.len()).step_by(every) {
                let (a, b) = (line[k.saturating_sub(1)], line[(k + 1).min(line.len() - 1)]);
                let (tx, ty) = (b.x - a.x, b.y - a.y);
                let m = tx.hypot(ty);
                if m < 1e-12 {
                    continue;
                }
                let (nx, ny) = (-ty / m * spacing, tx / m * spacing);
                let p = line[k];
                queue.push_back(Vec2 {
                    x: p.x + nx,
                    y: p.y + ny,
                });
                queue.push_back(Vec2 {
                    x: p.x - nx,
                    y: p.y - ny,
                });
            }
        }
        total += line.len();
        lines.push(line);
    }
    (lines, capped)
}

/// The `flow` mode's outputs: `shape`, `layers`, `points`, `counts`,
/// `levels` (each line's length).
pub(super) fn flow(pins: &Pins, inputs: &[Spread], cx: &crate::node::EvalCtx) -> Vec<Spread> {
    let center = pins.vec2(inputs, "center", Vec2::default());
    let size = pins.vec2(inputs, "size", SIZE);
    let seed = pins.num(inputs, "seed", 1.0);
    let scale = pins.num(inputs, "scale", 0.006);
    let vectors = pins.points(inputs, "vectors");
    let cols = pins.num(inputs, "cols", 16.0).max(0.0) as usize;
    let field = if vectors.is_empty() {
        let kind = match pins.value(inputs, "field") {
            Value::Str(s) => s.to_string(),
            _ => "angle".into(),
        };
        let seed = crate::seed::mix(seed);
        if kind == "curl" {
            Some(Field::Curl { scale, seed })
        } else {
            Some(Field::Angle { scale, seed })
        }
    } else {
        Field::grid(vectors, cols, center, size)
    };
    let o = Trace {
        center,
        size,
        turn: pins.num(inputs, "turn", 0.0),
        spacing: pins.num(inputs, "spacing", 10.0),
        step: pins.num(inputs, "step", 2.0),
        max_len: pins.num(inputs, "maxLength", 400.0),
        min_len: pins.num(inputs, "minLength", 20.0),
        seed,
    };
    let (lines, capped) = match &field {
        Some(f) => streamlines(f, &pins.points(inputs, "seeds"), &o),
        None => (Vec::new(), false),
    };
    if capped {
        cx.warn(format!(
            "Stopped at {} lines / {MAX_POINTS} points.",
            lines.len()
        ));
    }
    let style = as_line(&pin_style(&pins.value(inputs, "style"), false));
    let colors = pins.colors(inputs, "colors");
    let mut pick = rng_salted(seed, 1.0);
    let lengths: Vec<Value> = lines
        .iter()
        .map(|l| Value::Num(l.windows(2).map(|p| super::dist(p[0], p[1])).sum()))
        .collect();
    let polys: Vec<Poly> = lines
        .into_iter()
        .map(|l| {
            let u = pick.next_f64();
            let style = if colors.is_empty() {
                style.clone()
            } else {
                let c = colors[((u * colors.len() as f64) as usize).min(colors.len() - 1)];
                let mut st = (*style).clone();
                st.stroke = Some(flat(c));
                Arc::new(st)
            };
            Poly {
                rings: vec![l],
                closed: false,
                style,
            }
        })
        .collect();
    let mut out = poly_outputs(&polys);
    out.push(Spread::from(lengths));
    out
}
