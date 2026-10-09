//! `geometry/connect` (Connect Points): lines between nearby points, the
//! "plexus" look. Modes: `distance` joins every pair closer than
//! `distance` (each point keeping at most `maxLinks`, shortest first);
//! `nearest` joins each point to its `neighbours` nearest (within
//! `distance` when > 0), so nothing is left alone.
//!
//! Each line's `strength` is 1 at zero length falling to 0 at the reach;
//! `fade` takes that much of it off the line's alpha and `taper` off its
//! width. Neighbours are found through a uniform grid, so a few thousand
//! points cost about as much as their links.
//!
//! A wired `colors` spread colours the lines: `colorBy` `points` gives
//! point i colour i (cycled) and each line a gradient between its ends'
//! colours; `length` runs the colours as a palette from the shortest
//! lines (first colour) to those at the reach (last).

use super::measure::fade_paint;
use super::{
    flat, int_pin, live_mode, num_pin, param_str, pin_style, ramp, style_pin, Pins, MAX_POINTS,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{
    Gradient, GradientExtend, GradientShape, GradientStop, GradientUnits, Paint, ResolvedStyle,
    SceneNode,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

const MODE: &str = "distance";
const DISTANCE: f64 = 80.0;
/// Most lines drawn (it warns when it stops).
pub(super) const MAX_LINKS: usize = 100_000;
/// Alpha / width levels the styles are shared at.
const LEVELS: f64 = 64.0;
/// `colorBy` options, in dropdown order.
pub const COLOR_BY: [&str; 2] = ["points", "length"];

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread)];
    if param_str(params, "mode", MODE) == "nearest" {
        v.push(int_pin("neighbours", 3.0));
        v.push(num_pin("distance", 0.0));
    } else {
        v.push(num_pin("distance", DISTANCE));
        v.push(int_pin("maxLinks", 0.0));
    }
    v.push(num_pin("fade", 1.0));
    v.push(num_pin("taper", 0.0));
    v.push(PortSpec::new("colors", PortType::Color).kind(PortKind::Spread));
    let mut by = PortSpec::new("colorBy", PortType::String).default(Value::str(COLOR_BY[0]));
    by.param = Some("colorBy".into());
    v.push(by);
    v.push(style_pin());
    v
}

/// How the lines take the `colors` spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorBy {
    /// Point i has colour i; a line blends between its ends.
    Points,
    /// A palette from short (first colour) to long (last).
    Length,
}

/// A line's stroke paint before fading: the style's own, or one from the
/// `colors` spread.
fn line_paint(
    colors: &[[f64; 4]],
    by: ColorBy,
    l: &Link,
    level: f64,
    (p, q): (Vec2, Vec2),
) -> Option<Paint> {
    if colors.is_empty() {
        return None;
    }
    Some(match by {
        ColorBy::Length => flat(ramp(colors, 1.0 - level)?),
        ColorBy::Points => {
            let (a, b) = (colors[l.a % colors.len()], colors[l.b % colors.len()]);
            if a == b {
                flat(a)
            } else {
                Paint::gradient(Gradient {
                    shape: GradientShape::Linear { start: p, end: q },
                    stops: Arc::from(vec![
                        GradientStop { pos: 0.0, rgba: a },
                        GradientStop { pos: 1.0, rgba: b },
                    ]),
                    units: GradientUnits::Scene,
                    extend: GradientExtend::Pad,
                })
            }
        }
    })
}

/// A line between points `a` and `b` (indices), `d` long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Link {
    pub a: usize,
    pub b: usize,
    pub d: f64,
}

pub(super) fn connect(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let mut pts = pins.points(inputs, "points");
        if pts.len() > MAX_POINTS {
            cx.warn(format!(
                "{} points, over {MAX_POINTS}: the rest are left out.",
                pts.len()
            ));
            pts.truncate(MAX_POINTS);
        }
        let distance = pins.num(inputs, "distance", DISTANCE).max(0.0);
        let (mut links, reach) = if mode == "nearest" {
            let k = pins.num(inputs, "neighbours", 3.0).clamp(0.0, 64.0) as usize;
            let links = nearest(&pts, k, distance);
            let reach = if distance > 0.0 {
                distance
            } else {
                links.iter().map(|l| l.d).fold(0.0, f64::max)
            };
            (links, reach)
        } else {
            let max = pins.num(inputs, "maxLinks", 0.0).max(0.0) as usize;
            (within(&pts, distance, max), distance)
        };
        if links.len() > MAX_LINKS {
            cx.warn(format!(
                "{} lines, over {MAX_LINKS}: the longest are left out.",
                links.len()
            ));
            links.sort_by(|x, y| x.d.total_cmp(&y.d));
            links.truncate(MAX_LINKS);
        }
        let fade = pins.num(inputs, "fade", 1.0).clamp(0.0, 1.0);
        let taper = pins.num(inputs, "taper", 0.0).clamp(0.0, 1.0);
        let base = pin_style(&pins.value(inputs, "style"), false);
        let strength = |d: f64| {
            if reach > 0.0 {
                (1.0 - d / reach).clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        let colors = pins.colors(inputs, "colors");
        let by = match pins.value(inputs, "colorBy") {
            Value::Str(s) if &*s == "length" => ColorBy::Length,
            _ => ColorBy::Points,
        };
        // Styles shared by level (and colour, for flat lines).
        let mut styles: HashMap<(u32, usize, usize), Arc<ResolvedStyle>> = HashMap::new();
        let mut lines = Vec::with_capacity(links.len());
        let (mut from, mut to, mut strengths) = (
            Vec::with_capacity(links.len()),
            Vec::with_capacity(links.len()),
            Vec::with_capacity(links.len()),
        );
        for l in &links {
            let s = strength(l.d);
            let level = (s * LEVELS).round() as u32;
            let q = f64::from(level) / LEVELS;
            let alpha = 1.0 - fade * (1.0 - q);
            let width = 1.0 - taper * (1.0 - q);
            if alpha <= 0.0 || width <= 0.0 {
                continue;
            }
            let (p, q) = (pts[l.a], pts[l.b]);
            let n = colors.len().max(1);
            let key = match by {
                _ if colors.is_empty() => Some((level, 0, 0)),
                ColorBy::Length => Some((level, n, n)),
                // Flat when both ends have the same colour.
                ColorBy::Points if colors[l.a % n] == colors[l.b % n] => {
                    Some((level, l.a % n, l.a % n))
                }
                ColorBy::Points => None,
            };
            let make = || {
                let paint = line_paint(&colors, by, l, f64::from(level) / LEVELS, (p, q));
                line_style(&base, paint, alpha, width)
            };
            let style = match key {
                Some(k) => styles.entry(k).or_insert_with(make).clone(),
                None => make(),
            };
            lines.push(Arc::new(SceneNode::Line {
                x1: p.x,
                y1: p.y,
                x2: q.x,
                y2: q.y,
                style,
            }));
            from.push(Value::Vec2(p));
            to.push(Value::Vec2(q));
            strengths.push(Value::Num(s));
        }
        Ok(vec![
            crate::value::one(Value::Scene(Arc::new(SceneNode::group(lines)))),
            Spread::from(from),
            Spread::from(to),
            Spread::from(strengths),
        ])
    })
}

/// `base` with its stroke replaced by `paint` (when given), faded by
/// `alpha` and thinned by `width`.
fn line_style(
    base: &Arc<ResolvedStyle>,
    paint: Option<Paint>,
    alpha: f64,
    width: f64,
) -> Arc<ResolvedStyle> {
    if alpha >= 1.0 && width >= 1.0 && paint.is_none() {
        return base.clone();
    }
    let mut st = (**base).clone();
    if let (Some(p), Some(_)) = (paint, &st.stroke) {
        st.stroke = Some(p);
    }
    st.stroke = st.stroke.as_ref().map(|p| fade_paint(p, alpha));
    st.fill = None;
    st.stroke_weight *= width;
    Arc::new(st)
}

/// Points bucketed into square cells `cell` wide.
pub(super) struct Grid {
    cell: f64,
    min: Vec2,
    cols: usize,
    rows: usize,
    /// Point indices by cell (row-major), as start offsets into `items`.
    starts: Vec<usize>,
    items: Vec<usize>,
}

impl Grid {
    pub(super) fn new(pts: &[Vec2], cell: f64) -> Grid {
        let (mut min, mut max) = (
            Vec2 {
                x: f64::INFINITY,
                y: f64::INFINITY,
            },
            Vec2 {
                x: f64::NEG_INFINITY,
                y: f64::NEG_INFINITY,
            },
        );
        for p in pts {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        // Never more cells than a few per point (a huge reach over a
        // small cloud, or a tiny one over a big cloud).
        let budget = (pts.len() * 4).max(1) as f64;
        let (w, h) = ((max.x - min.x).max(0.0), (max.y - min.y).max(0.0));
        let mut cell = cell.max(1e-9);
        if (w / cell + 1.0) * (h / cell + 1.0) > budget {
            cell = cell.max(((w * h) / budget).sqrt()).max(w.max(h) / budget);
        }
        let cols = (w / cell) as usize + 1;
        let rows = (h / cell) as usize + 1;
        let mut counts = vec![0usize; cols * rows + 1];
        let at = |p: &Vec2| {
            let c = (((p.x - min.x) / cell) as usize).min(cols - 1);
            let r = (((p.y - min.y) / cell) as usize).min(rows - 1);
            r * cols + c
        };
        for p in pts {
            counts[at(p) + 1] += 1;
        }
        for i in 1..counts.len() {
            counts[i] += counts[i - 1];
        }
        let mut fill = counts.clone();
        let mut items = vec![0; pts.len()];
        for (i, p) in pts.iter().enumerate() {
            let c = at(p);
            items[fill[c]] = i;
            fill[c] += 1;
        }
        Grid {
            cell,
            min,
            cols,
            rows,
            starts: counts,
            items,
        }
    }

    fn cell_of(&self, p: Vec2) -> (isize, isize) {
        // Far-off points clamp to a few grids away (the search finds the
        // same nearest point from there, scaled distances aside).
        let lim = (self.cols.max(self.rows) as f64 + 1.0) * 4.0;
        let at = |v: f64, lo: f64| ((v - lo) / self.cell).floor().clamp(-lim, lim) as isize;
        (at(p.x, self.min.x), at(p.y, self.min.y))
    }

    fn cell_items(&self, c: isize, r: isize) -> &[usize] {
        if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
            return &[];
        }
        let i = r as usize * self.cols + c as usize;
        &self.items[self.starts[i]..self.starts[i + 1]]
    }

    /// The point of `pts` (the ones the grid was built from) nearest to
    /// `q`, searching outwards ring by ring.
    pub(super) fn nearest(&self, pts: &[Vec2], q: Vec2) -> Option<usize> {
        if self.items.is_empty() || !(q.x.is_finite() && q.y.is_finite()) {
            return None;
        }
        let (c, r) = self.cell_of(q);
        let (cols, rows) = (self.cols as isize, self.rows as isize);
        // Rings nearer than the grid's edge are empty.
        let outside = |v: isize, n: isize| (-v).max(v - (n - 1)).max(0);
        let first = outside(c, cols).max(outside(r, rows));
        let last = first + cols.max(rows);
        let mut best: Option<(f64, usize)> = None;
        let visit = |cc: isize, rr: isize, best: &mut Option<(f64, usize)>| {
            for &j in self.cell_items(cc, rr) {
                let d = (pts[j].x - q.x).powi(2) + (pts[j].y - q.y).powi(2);
                if best.is_none_or(|(b, _)| d < b) {
                    *best = Some((d, j));
                }
            }
        };
        for ring in first..=last {
            if ring == 0 {
                visit(c, r, &mut best);
            } else {
                // The ring's edge cells only.
                for d in -ring..=ring {
                    visit(c + d, r - ring, &mut best);
                    visit(c + d, r + ring, &mut best);
                }
                for d in 1 - ring..ring {
                    visit(c - ring, r + d, &mut best);
                    visit(c + ring, r + d, &mut best);
                }
            }
            // Anything in a further ring is at least `ring` cells away.
            if let Some((b, _)) = best {
                let bound = ring as f64 * self.cell;
                if b <= bound * bound {
                    break;
                }
            }
        }
        best.map(|(_, j)| j)
    }
}

/// Every pair closer than `reach`; with `max_links` > 0 each point keeps
/// at most that many, shortest first.
pub(super) fn within(pts: &[Vec2], reach: f64, max_links: usize) -> Vec<Link> {
    if pts.len() < 2 || reach <= 0.0 || reach.is_nan() {
        return Vec::new();
    }
    let grid = Grid::new(pts, reach);
    let span = (reach / grid.cell).ceil() as isize;
    let r2 = reach * reach;
    let mut links = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let (c, r) = grid.cell_of(*p);
        for dr in -span..=span {
            for dc in -span..=span {
                for &j in grid.cell_items(c + dc, r + dr) {
                    if j <= i {
                        continue;
                    }
                    let q = pts[j];
                    let d2 = (q.x - p.x).powi(2) + (q.y - p.y).powi(2);
                    if d2 < r2 {
                        links.push(Link {
                            a: i,
                            b: j,
                            d: d2.sqrt(),
                        });
                    }
                }
            }
        }
        if links.len() > MAX_LINKS * 4 {
            break;
        }
    }
    if max_links > 0 {
        links.sort_by(|x, y| x.d.total_cmp(&y.d).then(x.a.cmp(&y.a)).then(x.b.cmp(&y.b)));
        let mut degree = vec![0usize; pts.len()];
        links.retain(|l| {
            let keep = degree[l.a] < max_links && degree[l.b] < max_links;
            if keep {
                degree[l.a] += 1;
                degree[l.b] += 1;
            }
            keep
        });
    }
    links
}

/// Each point joined to its `k` nearest (within `reach` when > 0); a pair
/// that picks each other is one line.
pub(super) fn nearest(pts: &[Vec2], k: usize, reach: f64) -> Vec<Link> {
    let n = pts.len();
    if n < 2 || k == 0 {
        return Vec::new();
    }
    let k = k.min(n - 1);
    // Cells sized so a cell holds about k points on average.
    let (mut lo, mut hi) = (pts[0], pts[0]);
    for p in pts {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    let area = ((hi.x - lo.x) * (hi.y - lo.y)).max(1e-9);
    let mut cell = (area * (k as f64 + 1.0) / n as f64).sqrt();
    if reach > 0.0 {
        cell = cell.min(reach);
    }
    let grid = Grid::new(pts, cell.max(1e-6));
    let max_ring = grid.cols.max(grid.rows) as isize;
    let mut links = Vec::new();
    let mut best: Vec<(f64, usize)> = Vec::with_capacity(k + 1);
    for (i, p) in pts.iter().enumerate() {
        best.clear();
        let (c, r) = grid.cell_of(*p);
        let mut ring = 0isize;
        loop {
            // The cells on the square ring `ring` steps out.
            for dr in -ring..=ring {
                for dc in -ring..=ring {
                    if dr.abs() != ring && dc.abs() != ring {
                        continue;
                    }
                    for &j in grid.cell_items(c + dc, r + dr) {
                        if j == i {
                            continue;
                        }
                        let d = super::dist(*p, pts[j]);
                        if reach > 0.0 && d >= reach {
                            continue;
                        }
                        if best.len() < k || d < best[best.len() - 1].0 {
                            let at = best.partition_point(|b| b.0 <= d);
                            best.insert(at, (d, j));
                            best.truncate(k);
                        }
                    }
                }
            }
            // Anything further out is at least `ring` cells away.
            let bound = ring as f64 * grid.cell;
            let done = (best.len() == k && best[k - 1].0 <= bound)
                || (reach > 0.0 && bound >= reach)
                || ring > max_ring;
            if done {
                break;
            }
            ring += 1;
        }
        for &(d, j) in &best {
            let (a, b) = if i < j { (i, j) } else { (j, i) };
            links.push(Link { a, b, d });
        }
    }
    links.sort_by(|x, y| x.a.cmp(&y.a).then(x.b.cmp(&y.b)));
    links.dedup_by(|x, y| x.a == y.a && x.b == y.b);
    links
}
