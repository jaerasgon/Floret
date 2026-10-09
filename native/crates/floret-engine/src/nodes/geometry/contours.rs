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
//! - `shape`: distance from the outlines of a `shape` layer, sampled every
//!   `cell` units: the levels are distances (0 = the outline, positive
//!   outside, negative inside), so `step` > 0 draws rings round the shape,
//!   outer rings underneath.
//!
//! It draws `levels` contours, at `threshold`, `threshold + step`, ….
//! Outside the field counts as below every level, so every contour comes
//! out closed, with the higher side on its right-hand side going round
//! (clockwise round a peak): each level fills as one polygon with its
//! holes. `levels` out gives each contour's level. A wired `colors`
//! spread colours the levels (fill, else stroke), spread across them as
//! a palette from the first level to the last.
//!
//! Mode `flow` traces a vector field instead: streamlines, evenly spaced
//! (see [`super::flow`]).

use super::{
    flat, int_pin, layer_pin, live_mode, num_pin, param_str, pin_style, poly_outputs, ramp,
    style_pin, vec2_pin, Pins, Poly, MAX_POINTS,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::ResolvedStyle;
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

const MODE: &str = "field";
const SIZE: Vec2 = Vec2 { x: 400.0, y: 400.0 };
/// Most grid nodes sampled (a side of about 600).
const MAX_NODES: f64 = 360_000.0;
/// Most levels drawn.
const MAX_LEVELS: f64 = 256.0;

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = Vec::new();
    match param_str(params, "mode", MODE) {
        "flow" => v.extend(super::flow::inputs()),
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
        "shape" => {
            v.push(layer_pin("shape"));
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(vec2_pin("size", SIZE.x, SIZE.y));
            v.push(num_pin("cell", 2.0));
            v.push(num_pin("threshold", 0.0));
            v.push(int_pin("levels", 6.0));
            v.push(num_pin("step", 12.0));
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
    v.push(PortSpec::new("colors", PortType::Color).kind(PortKind::Spread));
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

/// Signed distance from a layer's outlines, sampled every `cell` units
/// over the rectangle: positive inside a filled area (nonzero winding),
/// negative outside, so contour `t` is the outline grown by −t (the
/// caller negates the levels: users give distances, outward positive).
/// Open paths count by distance only (never inside).
///
/// Exact at the nodes near an outline and on the rectangle's border; the
/// rest by "dead reckoning" (each node takes the nearest outline point its
/// neighbours found, measured exactly from itself), which stays within a
/// fraction of a cell.
pub(super) fn shape_field(layers: &[Value], center: Vec2, size: Vec2, cell: f64) -> Option<Field> {
    let contours = super::layer_contours(layers);
    let segs: Vec<(Vec2, Vec2)> = contours
        .iter()
        .flat_map(|c| {
            let n = c.pts.len();
            let count = match n {
                0 => 0,
                1 => 1,
                _ if c.closed => n,
                _ => n - 1,
            };
            (0..count).map(move |i| (c.pts[i], c.pts[(i + 1) % n]))
        })
        .filter(|(a, b)| a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite())
        .collect();
    if segs.is_empty() {
        return None;
    }
    let (cols, rows, origin, dx, dy) = lattice(center, size, cell)?;
    let node = |i: usize, j: usize| Vec2 {
        x: origin.x + i as f64 * dx,
        y: origin.y + j as f64 * dy,
    };
    let closest = |p: Vec2, (a, b): (Vec2, Vec2)| {
        let (ex, ey) = (b.x - a.x, b.y - a.y);
        let len2 = ex * ex + ey * ey;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * ex + (p.y - a.y) * ey) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Vec2 {
            x: a.x + ex * t,
            y: a.y + ey * t,
        }
    };
    // Each node's nearest outline point found so far, and its distance².
    let mut near: Vec<Option<Vec2>> = vec![None; cols * rows];
    let mut best = vec![f64::INFINITY; cols * rows];
    let offer = |best: &mut [f64], near: &mut [Option<Vec2>], k: usize, p: Vec2, q: Vec2| {
        let d = (p.x - q.x).powi(2) + (p.y - q.y).powi(2);
        if d < best[k] {
            best[k] = d;
            near[k] = Some(q);
        }
    };
    // Exact near every segment (two cells round its box) …
    let reach = 2.0 * dx.max(dy);
    let span = |lo: f64, hi: f64, o: f64, step: f64, n: usize| {
        let a = ((lo - reach - o) / step).floor().max(0.0);
        let b = ((hi + reach - o) / step).ceil().max(0.0);
        (a.min(n as f64) as usize, (b + 1.0).min(n as f64) as usize)
    };
    for &s in &segs {
        let (i0, i1) = span(s.0.x.min(s.1.x), s.0.x.max(s.1.x), origin.x, dx, cols);
        let (j0, j1) = span(s.0.y.min(s.1.y), s.0.y.max(s.1.y), origin.y, dy, rows);
        for j in j0..j1 {
            for i in i0..i1 {
                let p = node(i, j);
                offer(&mut best, &mut near, j * cols + i, p, closest(p, s));
            }
        }
    }
    // … and on the border, so outlines off the rectangle still reach in.
    let border = (0..cols)
        .flat_map(|i| [(i, 0), (i, rows - 1)])
        .chain((0..rows).flat_map(|j| [(0, j), (cols - 1, j)]));
    for (i, j) in border {
        let p = node(i, j);
        for &s in &segs {
            offer(&mut best, &mut near, j * cols + i, p, closest(p, s));
        }
    }
    // Dead reckoning: a forward sweep, then a backward one.
    for pass in 0..2 {
        let nbrs: [(isize, isize); 4] = if pass == 0 {
            [(-1, -1), (0, -1), (1, -1), (-1, 0)]
        } else {
            [(1, 0), (-1, 1), (0, 1), (1, 1)]
        };
        for step in 0..rows * cols {
            let k = if pass == 0 {
                step
            } else {
                rows * cols - 1 - step
            };
            let (i, j) = (k % cols, k / cols);
            let p = node(i, j);
            for (di, dj) in nbrs {
                let (ni, nj) = (i as isize + di, j as isize + dj);
                if ni < 0 || nj < 0 || ni as usize >= cols || nj as usize >= rows {
                    continue;
                }
                if let Some(q) = near[nj as usize * cols + ni as usize] {
                    offer(&mut best, &mut near, k, p, q);
                }
            }
        }
    }
    // Inside: nonzero winding of the closed outlines, row by row.
    let mut inside = vec![false; cols * rows];
    let mut crossings: Vec<Vec<(f64, i32)>> = vec![Vec::new(); rows];
    for c in contours.iter().filter(|c| c.closed && c.pts.len() >= 3) {
        let n = c.pts.len();
        for k in 0..n {
            let (a, b) = (c.pts[k], c.pts[(k + 1) % n]);
            if a.y == b.y || !(a.y.is_finite() && b.y.is_finite()) {
                continue;
            }
            let (lo, hi, dir) = if a.y < b.y {
                (a.y, b.y, 1)
            } else {
                (b.y, a.y, -1)
            };
            // Rows with lo <= y < hi.
            let j0 = ((lo - origin.y) / dy).ceil().max(0.0) as usize;
            let mut j = j0;
            while j < rows {
                let y = origin.y + j as f64 * dy;
                if y >= hi {
                    break;
                }
                if y >= lo {
                    let x = a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y);
                    crossings[j].push((x, dir));
                }
                j += 1;
            }
        }
    }
    for (j, row) in crossings.iter_mut().enumerate() {
        row.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut wind = 0;
        let mut i = 0;
        for k in 0..row.len() {
            wind += row[k].1;
            let next = row.get(k + 1).map_or(f64::INFINITY, |c| c.0);
            if wind == 0 {
                continue;
            }
            // Nodes between this crossing and the next are inside.
            while i < cols && origin.x + i as f64 * dx < row[k].0 {
                i += 1;
            }
            while i < cols && origin.x + (i as f64) * dx < next {
                inside[j * cols + i] = true;
                i += 1;
            }
        }
    }
    let values = best
        .iter()
        .zip(&inside)
        .map(|(d, inn)| {
            let d = d.sqrt();
            if *inn {
                d
            } else {
                -d
            }
        })
        .collect();
    Some(Field {
        cols,
        rows,
        values,
        origin,
        dx,
        dy,
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

/// `style` in a level's colour: its fill, or its stroke when it has no
/// fill.
fn level_style(style: &Arc<ResolvedStyle>, rgba: [f64; 4]) -> Arc<ResolvedStyle> {
    let mut st = (**style).clone();
    if st.fill.is_some() {
        st.fill = Some(flat(rgba));
    } else if st.stroke.is_some() {
        st.stroke = Some(flat(rgba));
    }
    Arc::new(st)
}

pub(super) fn contours(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        if mode == "flow" {
            return Ok(super::flow::flow(&pins, inputs, cx));
        }
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
            "shape" => shape_field(
                pins.spread(inputs, "shape"),
                center,
                pins.vec2(inputs, "size", SIZE),
                pins.num(inputs, "cell", 2.0),
            ),
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
            "points" => (1.0, 0.5, 1.0),
            "image" => (0.5, 0.1, 1.0),
            "shape" => (0.0, 12.0, 6.0),
            _ => (0.0, 0.1, 1.0),
        };
        let threshold = pins.num(inputs, "threshold", fallback.0);
        let step = pins.num(inputs, "step", fallback.1);
        let levels = pins
            .num(inputs, "levels", fallback.2)
            .clamp(0.0, MAX_LEVELS) as usize;
        let style = pin_style(&pins.value(inputs, "style"), true);
        let colors = pins.colors(inputs, "colors");
        // Shape distances run outwards: the field is inside-high.
        let distance = mode == "shape";
        let mut polys = Vec::new();
        let mut level_of = Vec::new();
        let mut total = 0;
        if let Some(f) = &field {
            for k in 0..levels {
                let t = threshold + step * k as f64;
                let rings = isolines(f, if distance { -t } else { t });
                if rings.is_empty() {
                    continue;
                }
                total += rings.iter().map(Vec::len).sum::<usize>();
                if total > MAX_POINTS {
                    cx.warn(format!("Over {MAX_POINTS} points: stopped at level {k}."));
                    break;
                }
                level_of.extend(rings.iter().map(|_| Value::Num(t)));
                let at = if levels > 1 {
                    k as f64 / (levels - 1) as f64
                } else {
                    0.0
                };
                polys.push(Poly {
                    rings,
                    closed: true,
                    style: match ramp(&colors, at) {
                        Some(c) => level_style(&style, c),
                        None => style.clone(),
                    },
                });
            }
        }
        // Rings growing outwards: the outer ones go underneath.
        if distance && step > 0.0 {
            polys.reverse();
            let counts: Vec<usize> = polys.iter().map(|p| p.rings.len()).collect();
            let mut rest = &level_of[..];
            let mut flipped = Vec::with_capacity(level_of.len());
            for n in counts.iter().rev() {
                let (head, tail) = rest.split_at(*n);
                flipped.push(head);
                rest = tail;
            }
            level_of = flipped.into_iter().rev().flatten().cloned().collect();
        }
        let mut out = poly_outputs(&polys);
        out.push(Spread::from(level_of));
        Ok(out)
    })
}
