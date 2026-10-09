//! `geometry/contours` (Contours): isolines of a scalar field as closed
//! outlines, by marching squares.
//!
//! Modes pick the field:
//! - `field`: a spread of `values`, `cols` per row, laid over the
//!   `center` / `size` rectangle (row by row from the top left), e.g.
//!   noise sampled on `spread/grid` positions: topographic lines.
//! - `points`: metaballs, Σ radius² / distance² over the `points` (with
//!   `radius`, cycled), sampled every `cell` units over the rectangle.
//! - `image`: an image's brightness (0..1, times alpha), drawn centred on
//!   `center` at `size` (0 = its own size in pixels).
//!
//! It draws `levels` contours, at `threshold`, `threshold + step`, ….
//! Outside the field counts as below every level, so every contour comes
//! out closed, with the higher side on its right-hand side going round
//! (clockwise round a peak): each level fills as one polygon with its
//! holes. `levels` out gives each contour's level.

use super::{
    int_pin, live_mode, num_pin, param_str, pin_style, poly_outputs, style_pin, vec2_pin, Pins,
    Poly, MAX_POINTS,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::collections::HashMap;

const MODE: &str = "field";
const SIZE: Vec2 = Vec2 { x: 400.0, y: 400.0 };
/// Most grid nodes sampled (a side of about 600).
const MAX_NODES: f64 = 360_000.0;
/// Most levels drawn.
const MAX_LEVELS: f64 = 256.0;

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = Vec::new();
    match param_str(params, "mode", MODE) {
        "points" => {
            v.push(PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread));
            v.push(
                PortSpec::new("radius", PortType::Number)
                    .kind(PortKind::Spread)
                    .default(Value::Num(40.0)),
            );
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(vec2_pin("size", SIZE.x, SIZE.y));
            v.push(num_pin("cell", 4.0));
            v.push(num_pin("threshold", 1.0));
            v.push(int_pin("levels", 1.0));
            v.push(num_pin("step", 0.5));
        }
        "image" => {
            v.push(PortSpec::new("image", PortType::Image));
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(vec2_pin("size", 0.0, 0.0));
            v.push(num_pin("threshold", 0.5));
            v.push(int_pin("levels", 1.0));
            v.push(num_pin("step", 0.1));
        }
        _ => {
            v.push(PortSpec::new("values", PortType::Number).kind(PortKind::Spread));
            v.push(int_pin("cols", 16.0));
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(vec2_pin("size", SIZE.x, SIZE.y));
            v.push(num_pin("threshold", 0.0));
            v.push(int_pin("levels", 1.0));
            v.push(num_pin("step", 0.1));
        }
    }
    v.push(style_pin());
    v
}

/// Samples on a grid: `cols × rows` values row by row from the top left,
/// node (i, j) at `origin + (i·dx, j·dy)`.
#[derive(Clone, Debug)]
pub(super) struct Field {
    pub cols: usize,
    pub rows: usize,
    pub values: Vec<f64>,
    pub origin: Vec2,
    pub dx: f64,
    pub dy: f64,
}

impl Field {
    fn at(&self, i: usize, j: usize) -> f64 {
        self.values[j * self.cols + i]
    }
}

/// The grid `nodes` across the rectangle (at least 2 a side), as
/// `(cols, rows, origin, dx, dy)`.
fn lattice(center: Vec2, size: Vec2, cell: f64) -> Option<(usize, usize, Vec2, f64, f64)> {
    let (w, h) = (size.x.abs(), size.y.abs());
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return None;
    }
    let mut cell = if cell.is_finite() && cell > 0.0 {
        cell
    } else {
        4.0
    };
    if (w / cell + 1.0) * (h / cell + 1.0) > MAX_NODES {
        cell = ((w * h) / MAX_NODES).sqrt().max(cell);
    }
    let cols = ((w / cell).ceil() as usize).max(1) + 1;
    let rows = ((h / cell).ceil() as usize).max(1) + 1;
    let origin = Vec2 {
        x: center.x - w / 2.0,
        y: center.y - h / 2.0,
    };
    Some((
        cols,
        rows,
        origin,
        w / (cols - 1) as f64,
        h / (rows - 1) as f64,
    ))
}

/// A spread of values, `cols` a row, over the rectangle (missing values
/// at the end count as below everything).
pub(super) fn grid_field(values: &[f64], cols: usize, center: Vec2, size: Vec2) -> Option<Field> {
    if cols < 2 || values.len() < cols * 2 {
        return None;
    }
    let rows = values.len().div_ceil(cols);
    let (w, h) = (size.x.abs(), size.y.abs());
    if !(w.is_finite() && h.is_finite()) {
        return None;
    }
    let low = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let low = if low.is_finite() { low - 1.0 } else { -1.0 };
    let mut vals: Vec<f64> = values
        .iter()
        .map(|v| if v.is_finite() { *v } else { low })
        .collect();
    vals.resize(cols * rows, low);
    Some(Field {
        cols,
        rows,
        values: vals,
        origin: Vec2 {
            x: center.x - w / 2.0,
            y: center.y - h / 2.0,
        },
        dx: w / (cols - 1) as f64,
        dy: h / (rows - 1).max(1) as f64,
    })
}

/// Metaballs: Σ r² / d² over the points.
pub(super) fn metaballs(
    pts: &[Vec2],
    radii: &[f64],
    center: Vec2,
    size: Vec2,
    cell: f64,
) -> Option<Field> {
    let (cols, rows, origin, dx, dy) = lattice(center, size, cell)?;
    let mut values = vec![0.0; cols * rows];
    for (k, p) in pts.iter().enumerate() {
        let r = radii
            .get(k % radii.len().max(1))
            .copied()
            .filter(|r| r.is_finite())
            .unwrap_or(40.0);
        let r2 = r * r;
        if r2 <= 0.0 {
            continue;
        }
        // Far nodes get under 1/400 of the peak: skip them past 20 r.
        let reach = 20.0 * r;
        let i0 = (((p.x - reach - origin.x) / dx).floor().max(0.0) as usize).min(cols);
        let i1 = (((p.x + reach - origin.x) / dx).ceil().max(0.0) as usize).min(cols);
        let j0 = (((p.y - reach - origin.y) / dy).floor().max(0.0) as usize).min(rows);
        let j1 = (((p.y + reach - origin.y) / dy).ceil().max(0.0) as usize).min(rows);
        for j in j0..j1 {
            let y = origin.y + j as f64 * dy;
            for i in i0..i1 {
                let x = origin.x + i as f64 * dx;
                let d2 = (x - p.x).powi(2) + (y - p.y).powi(2);
                values[j * cols + i] += r2 / d2.max(1e-9);
            }
        }
    }
    Some(Field {
        cols,
        rows,
        values,
        origin,
        dx,
        dy,
    })
}

/// An image's brightness × alpha (0..1), centred on `center` at `size`
/// (0 = its pixel size), sampled at most about 600 a side.
pub(super) fn image_field(img: &crate::image::Image, center: Vec2, size: Vec2) -> Option<Field> {
    let (iw, ih) = (img.width() as usize, img.height() as usize);
    if iw < 2 || ih < 2 {
        return None;
    }
    let w = if size.x > 0.0 { size.x } else { iw as f64 };
    let h = if size.y > 0.0 { size.y } else { ih as f64 };
    let stride = ((iw * ih) as f64 / MAX_NODES).sqrt().ceil().max(1.0) as usize;
    let cols = iw.div_ceil(stride);
    let rows = ih.div_ceil(stride);
    if cols < 2 || rows < 2 {
        return None;
    }
    let mut values = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        for i in 0..cols {
            let [r, g, b, a] = img.pixel(
                (i * stride).min(iw - 1) as u32,
                (j * stride).min(ih - 1) as u32,
            );
            let l = (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) / 255.0;
            values.push(l * f64::from(a) / 255.0);
        }
    }
    Some(Field {
        cols,
        rows,
        values,
        origin: Vec2 {
            x: center.x - w / 2.0,
            y: center.y - h / 2.0,
        },
        dx: w / (cols - 1) as f64,
        dy: h / (rows - 1) as f64,
    })
}

/// The closed contours of `f` at level `t` (marching squares, saddles
/// split by the cell's mean, the field's outside counting as below `t`).
/// Higher values lie on the right going round.
pub(super) fn isolines(f: &Field, t: f64) -> Vec<Vec<Vec2>> {
    if !t.is_finite() || f.cols < 2 || f.rows < 2 {
        return Vec::new();
    }
    // Padded by one node all round, at the edge's own position, below t.
    let (pc, pr) = (f.cols + 2, f.rows + 2);
    let below = t - 1.0;
    let value = |i: usize, j: usize| -> f64 {
        if i == 0 || j == 0 || i == pc - 1 || j == pr - 1 {
            below
        } else {
            let v = f.at(i - 1, j - 1);
            if v.is_nan() {
                below
            } else {
                v
            }
        }
    };
    let pos = |i: usize, j: usize| Vec2 {
        x: f.origin.x + (i.clamp(1, f.cols) - 1) as f64 * f.dx,
        y: f.origin.y + (j.clamp(1, f.rows) - 1) as f64 * f.dy,
    };
    // Edge keys: horizontal edge (i, j)–(i+1, j) and vertical (i, j)–(i, j+1).
    let h_key = |i: usize, j: usize| 2 * (j * pc + i);
    let v_key = |i: usize, j: usize| 2 * (j * pc + i) + 1;
    let cross = |a: (usize, usize), b: (usize, usize)| -> Vec2 {
        let (va, vb) = (value(a.0, a.1), value(b.0, b.1));
        let (pa, pb) = (pos(a.0, a.1), pos(b.0, b.1));
        let k = if (vb - va).abs() > 1e-300 {
            ((t - va) / (vb - va)).clamp(0.0, 1.0)
        } else {
            0.5
        };
        super::lerp(pa, pb, k)
    };
    // Directed segments: from-key → (to-key, from point).
    let mut next: HashMap<usize, (usize, Vec2)> = HashMap::new();
    for j in 0..pr - 1 {
        for i in 0..pc - 1 {
            // Corners clockwise on screen: TL, TR, BR, BL.
            let c = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let inside = c.map(|(x, y)| value(x, y) >= t);
            if inside.iter().all(|b| *b) || inside.iter().all(|b| !*b) {
                continue;
            }
            let keys = [h_key(i, j), v_key(i + 1, j), h_key(i, j + 1), v_key(i, j)];
            // Crossings in clockwise order: (edge, leaving the inside?).
            let mut xs: Vec<(usize, bool)> = Vec::with_capacity(4);
            for e in 0..4 {
                let (a, b) = (inside[e], inside[(e + 1) % 4]);
                if a != b {
                    xs.push((e, a));
                }
            }
            let centre_in = c.iter().map(|&(x, y)| value(x, y)).sum::<f64>() / 4.0 >= t;
            let n = xs.len();
            for k in 0..n {
                let (e, exit) = xs[k];
                if !exit {
                    continue;
                }
                // An exit joins the entry after it (centre inside) or the
                // one before it (centre outside); the same one unless this
                // is a saddle.
                let entry = if centre_in {
                    xs[(k + 1) % n].0
                } else {
                    xs[(k + n - 1) % n].0
                };
                let (a, b) = (c[e], c[(e + 1) % 4]);
                next.insert(keys[e], (keys[entry], cross(a, b)));
            }
        }
    }
    // Follow the segments round.
    let mut out = Vec::new();
    let mut starts: Vec<usize> = next.keys().copied().collect();
    starts.sort_unstable();
    for s in starts {
        let mut ring = Vec::new();
        let mut k = s;
        while let Some((to, p)) = next.remove(&k) {
            ring.push(p);
            k = to;
            if k == s {
                break;
            }
        }
        // Drop repeats (crossings at a shared corner).
        ring.dedup_by(|a, b| super::dist(*a, *b) < 1e-9);
        if ring.len() > 1 && super::dist(ring[0], ring[ring.len() - 1]) < 1e-9 {
            ring.pop();
        }
        if ring.len() >= 3 {
            out.push(ring);
        }
    }
    out
}

pub(super) fn contours(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let center = pins.vec2(inputs, "center", Vec2::default());
        let field = match mode.as_str() {
            "points" => {
                let radii: Vec<f64> = pins
                    .spread(inputs, "radius")
                    .iter()
                    .map(Value::to_number)
                    .collect();
                metaballs(
                    &pins.points(inputs, "points"),
                    &radii,
                    center,
                    pins.vec2(inputs, "size", SIZE),
                    pins.num(inputs, "cell", 4.0),
                )
            }
            "image" => match crate::image::ready(&pins.value(inputs, "image")) {
                Some(img) => image_field(img, center, pins.vec2(inputs, "size", Vec2::default())),
                None => None,
            },
            _ => {
                let values: Vec<f64> = pins
                    .spread(inputs, "values")
                    .iter()
                    .map(Value::to_number)
                    .collect();
                let cols = pins.num(inputs, "cols", 16.0).max(0.0) as usize;
                grid_field(&values, cols, center, pins.vec2(inputs, "size", SIZE))
            }
        };
        let fallback = match mode.as_str() {
            "points" => (1.0, 0.5),
            "image" => (0.5, 0.1),
            _ => (0.0, 0.1),
        };
        let threshold = pins.num(inputs, "threshold", fallback.0);
        let step = pins.num(inputs, "step", fallback.1);
        let levels = pins.num(inputs, "levels", 1.0).clamp(0.0, MAX_LEVELS) as usize;
        let style = pin_style(&pins.value(inputs, "style"), true);
        let mut polys = Vec::new();
        let mut level_of = Vec::new();
        let mut total = 0;
        if let Some(f) = &field {
            for k in 0..levels {
                let t = threshold + step * k as f64;
                let rings = isolines(f, t);
                if rings.is_empty() {
                    continue;
                }
                total += rings.iter().map(Vec::len).sum::<usize>();
                if total > MAX_POINTS {
                    cx.warn(format!("Over {MAX_POINTS} points: stopped at level {k}."));
                    break;
                }
                level_of.extend(rings.iter().map(|_| Value::Num(t)));
                polys.push(Poly {
                    rings,
                    closed: true,
                    style: style.clone(),
                });
            }
        }
        let mut out = poly_outputs(&polys);
        out.push(Spread::from(level_of));
        Ok(out)
    })
}
