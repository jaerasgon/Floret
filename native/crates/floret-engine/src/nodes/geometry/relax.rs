//! Lloyd relaxation for `geometry/scatter`: points spread evenly over an
//! area (a rectangle, or the filled area of a shape) by moving each to the
//! centre of the part of the area nearest to it, `iterations` times.
//!
//! Over a rectangle each point moves to the centroid of its exact Voronoi
//! cell. Inside a shape the area is measured on a fine grid of samples
//! (about 32 per point, at most 400 000), each given to its nearest point
//! through a uniform grid of the points; the samples shift a little every
//! pass, so points never lock onto them. Points nothing is nearest to
//! stay where they are.
//!
//! [`weighted_lloyd`] weighs every sample (Secord's weighted Voronoi
//! stippling for `geometry/scatter`'s `stipple` mode): each point moves
//! to the weighted centre of its part of the area, so points gather where
//! the weight (an image's darkness) is high.

use super::connect::Grid;
use super::scatter::Area;
use crate::value::Vec2;

/// Most passes (each costs one nearest-point search per sample).
pub(super) const MAX_ITERATIONS: f64 = 100.0;
const SAMPLES_PER_POINT: usize = 32;
const MAX_SAMPLES: usize = 400_000;
const MIN_SAMPLES: usize = 4096;

/// The area's samples: points of a grid over its bounds (moved by
/// `shift`, in cells, 0..1), kept when inside.
fn samples(area: &Area, n: usize, shift: (f64, f64)) -> Vec<Vec2> {
    let (w, h) = (area.max.x - area.min.x, area.max.y - area.min.y);
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return Vec::new();
    }
    let want = (n * SAMPLES_PER_POINT).clamp(MIN_SAMPLES, MAX_SAMPLES) as f64;
    let step = (w * h / want).sqrt();
    let cols = ((w / step).ceil() as usize).max(1);
    let rows = ((h / step).ceil() as usize).max(1);
    let (sx, sy) = (w / cols as f64, h / rows as f64);
    let mut out = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        for i in 0..cols {
            let p = Vec2 {
                x: area.min.x + (i as f64 + shift.0) * sx,
                y: area.min.y + (j as f64 + shift.1) * sy,
            };
            if area.contains(p) {
                out.push(p);
            }
        }
    }
    out
}

/// `pts` relaxed `iterations` times over `area`.
pub(super) fn lloyd(area: &Area, pts: Vec<Vec2>, iterations: usize) -> Vec<Vec2> {
    weighted_lloyd(area, pts, iterations, None)
}

/// `pts` relaxed `iterations` times over `area`, each sample of the area
/// counting `weight(p)` (≥ 0) towards the centroids when given: points
/// gather where the weight is high (Secord's weighted Voronoi stippling).
pub(super) fn weighted_lloyd(
    area: &Area,
    mut pts: Vec<Vec2>,
    iterations: usize,
    weight: Option<&dyn Fn(Vec2) -> f64>,
) -> Vec<Vec2> {
    pts.retain(|p| p.x.is_finite() && p.y.is_finite());
    let (w, h) = (area.w(), area.h());
    if pts.len() < 2 || iterations == 0 || !(w > 0.0 && h > 0.0 && (w * h).is_finite()) {
        return pts;
    }
    let cell = (w * h / pts.len() as f64).sqrt();
    // Points on top of each other share one cell and would never part:
    // nudge repeats round a tiny spiral.
    let mut seen = std::collections::HashMap::new();
    for p in &mut pts {
        let k: &mut u32 = seen.entry((p.x.to_bits(), p.y.to_bits())).or_default();
        if *k > 0 {
            let a = f64::from(*k) * 2.399_963;
            let r = 1e-3 * cell * f64::from(*k).sqrt();
            p.x += r * a.cos();
            p.y += r * a.sin();
        }
        *k += 1;
    }
    let centre = Vec2 {
        x: (area.min.x + area.max.x) / 2.0,
        y: (area.min.y + area.max.y) / 2.0,
    };
    let mut sum = vec![(0.0, 0.0, 0.0); pts.len()];
    for pass in 0..iterations {
        let centroids: Vec<Option<Vec2>> = if area.is_rect() && weight.is_none() {
            super::cells::voronoi_cells(&pts, centre, Vec2 { x: w, y: h })
                .into_iter()
                .map(|c| c.map(|ring| super::centroid(&ring)))
                .collect()
        } else {
            // A different lattice offset each pass (the R2 sequence).
            let k = pass as f64 + 1.0;
            let shift = (
                (0.5 + k * 0.754_877_666).fract(),
                (0.5 + k * 0.569_840_291).fract(),
            );
            let field = samples(area, pts.len(), shift);
            let grid = Grid::new(&pts, cell);
            sum.iter_mut().for_each(|s| *s = (0.0, 0.0, 0.0));
            for q in &field {
                let w = weight.map_or(1.0, |f| f(*q));
                if !(w > 0.0 && w.is_finite()) {
                    continue;
                }
                if let Some(i) = grid.nearest(&pts, *q) {
                    let s = &mut sum[i];
                    s.0 += q.x * w;
                    s.1 += q.y * w;
                    s.2 += w;
                }
            }
            sum.iter()
                .map(|s| {
                    (s.2 > 0.0).then(|| Vec2 {
                        x: s.0 / s.2,
                        y: s.1 / s.2,
                    })
                })
                .collect()
        };
        let mut moved = 0.0_f64;
        for (p, c) in pts.iter_mut().zip(centroids) {
            if let Some(c) = c.filter(|c| c.x.is_finite() && c.y.is_finite()) {
                moved = moved.max(super::dist(*p, c));
                *p = c;
            }
        }
        if moved < 1e-9 * cell {
            break;
        }
    }
    pts
}
