//! `geometry/cells`: the Voronoi cells, Delaunay triangles or convex hull
//! of a points spread, as polygons, or a rectangle split recursively.
//!
//! Voronoi cells are clipped to the `center` / `size` rectangle. `gap`
//! shrinks every polygon by half its value on each side, so neighbouring
//! cells end up `gap` apart. `colors` (cycled) fills polygon i with colour
//! i, over the wired `style`.
//!
//! `subdivide` splits the `center` / `size` rectangle in two (across its
//! longer side more often than not) or in four (`quad`), then splits the
//! parts again, down to `depth` levels: past the first split each part
//! splits with probability `chance`, and never into parts thinner than
//! `minSize`. `ratio` (0..1) is how far from the middle a cut may land.
//! With `points` wired, a part splits while it holds more than one of them
//! instead (a k-d tree / quadtree of the points: small cells where they
//! crowd). Colours are picked per rectangle at random (repeat a colour to
//! make it likelier), and `depth` out gives each one's level: a Mondrian,
//! a quadtree or a city plan from one seed.

use super::{
    clip_half_plane, inset_convex, int_pin, live_mode, num_pin, param_str, pin_style, poly_outputs,
    signed_area, style_pin, sub, vec2_pin, Pins, Poly,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{resolve_style, Paint, Style, StyleOp};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Value, Vec2};
use std::sync::Arc;

const MODE: &str = "voronoi";
const SIZE: Vec2 = Vec2 { x: 400.0, y: 400.0 };

/// Most levels `subdivide` goes down.
const MAX_DEPTH: f64 = 16.0;
/// Most rectangles `subdivide` makes.
pub(super) const MAX_RECTS: usize = 20_000;

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread)];
    let mode = param_str(params, "mode", MODE);
    if matches!(mode, "voronoi" | "subdivide") {
        v.push(vec2_pin("center", 0.0, 0.0));
        v.push(vec2_pin("size", SIZE.x, SIZE.y));
    }
    if mode == "subdivide" {
        v.push(int_pin("depth", 5.0));
        v.push(num_pin("chance", 0.7));
        v.push(num_pin("minSize", 16.0));
        v.push(num_pin("ratio", 0.6));
        v.push(PortSpec::new("quad", PortType::Boolean).default(Value::Bool(false)));
        v.push(int_pin("seed", 1.0));
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
        let mut depths: Vec<f64> = Vec::new();
        let mut picks: Option<Vec<f64>> = None;
        let rings: Vec<Vec<Vec2>> = match mode.as_str() {
            "subdivide" => {
                let opts = Split {
                    depth: pins.num(inputs, "depth", 5.0).clamp(0.0, MAX_DEPTH) as usize,
                    chance: pins.num(inputs, "chance", 0.7).clamp(0.0, 1.0),
                    min_size: pins.num(inputs, "minSize", 16.0).max(0.0),
                    ratio: pins.num(inputs, "ratio", 0.6).clamp(0.0, 1.0),
                    quad: pins.bool(inputs, "quad", false),
                };
                let center = pins.vec2(inputs, "center", Vec2::default());
                let size = pins.vec2(inputs, "size", SIZE);
                let seed = pins.num(inputs, "seed", 1.0);
                let (rects, capped) = subdivide(center, size, &pts, &opts, seed);
                if capped {
                    cx.warn(format!("Stopped at {MAX_RECTS} rectangles."));
                }
                let mut r = super::rng_salted(seed, 1.0);
                picks = Some(rects.iter().map(|_| r.next_f64()).collect());
                depths = rects.iter().map(|c| c.depth as f64).collect();
                rects.into_iter().map(|c| c.ring()).collect()
            }
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
        // Cells a gap swallows drop out (with their depth and colour).
        let mut kept = Vec::with_capacity(rings.len());
        let rings: Vec<Vec<Vec2>> = rings
            .into_iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let r = inset_convex(&r, gap / 2.0);
                (r.len() >= 3).then(|| {
                    kept.push(i);
                    r
                })
            })
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
                    // Subdivide picks at random; the others cycle.
                    let n = colors.len();
                    let k = match &picks {
                        Some(p) => ((p[kept[i]] * n as f64) as usize).min(n - 1),
                        None => i % n,
                    };
                    let fill = StyleOp::Fill(Some(colors[k].clone()));
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
        out.push(Arc::from(
            kept.iter()
                .map(|&i| Value::Num(depths.get(i).copied().unwrap_or(0.0)))
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
    voronoi_cells(pts, center, size)
        .into_iter()
        .flatten()
        .collect()
}

/// [`voronoi`], one entry per site (`None`: its cell is empty).
pub(super) fn voronoi_cells(pts: &[Vec2], center: Vec2, size: Vec2) -> Vec<Option<Vec<Vec2>>> {
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
        return vec![Some(bounds)];
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
            out.push(None);
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
        out.push((cell.len() >= 3 && signed_area(&cell).abs() > 1e-9).then_some(cell));
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

// ─── Subdivide ───────────────────────────────────────────────────────────────

/// How `subdivide` splits.
pub(super) struct Split {
    pub depth: usize,
    pub chance: f64,
    pub min_size: f64,
    pub ratio: f64,
    pub quad: bool,
}

/// One rectangle of a subdivision: its top-left corner, size and level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Cell {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub depth: usize,
}

impl Cell {
    fn new(x: f64, y: f64, w: f64, h: f64, depth: usize) -> Cell {
        Cell { x, y, w, h, depth }
    }

    /// Clockwise on screen from the top-left corner.
    pub fn ring(&self) -> Vec<Vec2> {
        let (x, y, w, h) = (self.x, self.y, self.w, self.h);
        vec![
            Vec2 { x, y },
            Vec2 { x: x + w, y },
            Vec2 { x: x + w, y: y + h },
            Vec2 { x, y: y + h },
        ]
    }

    fn holds(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }
}

/// The rectangle `center` / `size` split recursively (depth first, so
/// neighbours in the list are neighbours on the canvas), and whether it
/// stopped at [`MAX_RECTS`]. With `points`, a part splits while it holds
/// more than one of them; without, by `chance` (the first split always
/// happens). Deterministic for a seed.
pub(super) fn subdivide(
    center: Vec2,
    size: Vec2,
    points: &[Vec2],
    o: &Split,
    seed: f64,
) -> (Vec<Cell>, bool) {
    let (w, h) = (size.x.abs(), size.y.abs());
    if !(w.is_finite() && h.is_finite() && center.x.is_finite() && center.y.is_finite())
        || w <= 0.0
        || h <= 0.0
    {
        return (Vec::new(), false);
    }
    let mut rng = super::rng(seed);
    let root = Cell::new(center.x - w / 2.0, center.y - h / 2.0, w, h, 0);
    let mut out = Vec::new();
    // Last in, first out, children pushed in reverse: depth-first order.
    let mut stack = vec![root];
    let mut capped = false;
    // A cut lands within ±ratio/2 of the middle (5%..95% at most).
    let at = |u: f64| 0.5 + (u - 0.5) * o.ratio.min(0.9);
    while let Some(c) = stack.pop() {
        // Every part draws the same four numbers whatever is decided.
        let (roll, ua, ub, dir) = (
            rng.next_f64(),
            rng.next_f64(),
            rng.next_f64(),
            rng.next_f64(),
        );
        let wants = if points.is_empty() {
            c.depth == 0 || roll < o.chance
        } else {
            points.iter().filter(|p| c.holds(**p)).take(2).count() > 1
        };
        let (fa, fb) = (at(ua), at(ub));
        let can_x = c.w * fa.min(1.0 - fa) >= o.min_size;
        let can_y = c.h * fb.min(1.0 - fb) >= o.min_size;
        if c.depth >= o.depth || !wants || !(can_x || can_y) {
            out.push(c);
            continue;
        }
        if out.len() + stack.len() + 4 > MAX_RECTS {
            capped = true;
            out.push(c);
            continue;
        }
        let d = c.depth + 1;
        let kids: Vec<Cell> = if o.quad && can_x && can_y {
            let (sx, sy) = (c.w * fa, c.h * fb);
            vec![
                Cell::new(c.x, c.y, sx, sy, d),
                Cell::new(c.x + sx, c.y, c.w - sx, sy, d),
                Cell::new(c.x, c.y + sy, sx, c.h - sy, d),
                Cell::new(c.x + sx, c.y + sy, c.w - sx, c.h - sy, d),
            ]
        } else {
            // Across the longer side more often: a vertical cut with
            // probability w / (w + h).
            let vertical = if can_x && can_y {
                dir < c.w / (c.w + c.h)
            } else {
                can_x
            };
            if vertical {
                let s = c.w * fa;
                vec![
                    Cell::new(c.x, c.y, s, c.h, d),
                    Cell::new(c.x + s, c.y, c.w - s, c.h, d),
                ]
            } else {
                let s = c.h * fb;
                vec![
                    Cell::new(c.x, c.y, c.w, s, d),
                    Cell::new(c.x, c.y + s, c.w, c.h - s, d),
                ]
            }
        };
        stack.extend(kids.into_iter().rev());
    }
    (out, capped)
}
