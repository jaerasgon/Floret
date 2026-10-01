//! Text along a path: arc-length parameterised polylines and glyph
//! placement with each glyph turned to the path's tangent.

use super::layout::{ShapedGlyph, ShapedLine};
use crate::scene::{self, Affine, SceneNode};
use crate::value::Vec2;

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pts: Vec<Vec2>,
    /// Arc length at each point (`cum[0] = 0`); for a closed path one more
    /// entry for the closing segment.
    cum: Vec<f64>,
    closed: bool,
}

impl Path {
    /// A polyline through `pts` (non-finite and repeated points dropped).
    /// `None` with fewer than two distinct points.
    pub fn new(pts: impl IntoIterator<Item = Vec2>, closed: bool) -> Option<Path> {
        let mut clean: Vec<Vec2> = Vec::new();
        for p in pts {
            if !(p.x.is_finite() && p.y.is_finite()) {
                continue;
            }
            if clean.last().is_some_and(|q| dist(*q, p) < 1e-9) {
                continue;
            }
            clean.push(p);
        }
        if closed && clean.len() > 2 && dist(clean[0], clean[clean.len() - 1]) < 1e-9 {
            clean.pop();
        }
        if clean.len() < 2 {
            return None;
        }
        let n = clean.len();
        let segs = if closed { n } else { n - 1 };
        let mut cum = vec![0.0];
        for i in 0..segs {
            let l = cum[i] + dist(clean[i], clean[(i + 1) % n]);
            cum.push(l);
        }
        Some(Path {
            pts: clean,
            cum,
            closed,
        })
    }

    pub fn length(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Point and tangent angle (radians) at arc length `s`. A closed path
    /// wraps; an open one continues straight past its ends.
    pub fn at(&self, s: f64) -> (Vec2, f64) {
        let total = self.length();
        let n = self.pts.len();
        let s = if self.closed && total > 0.0 {
            s.rem_euclid(total)
        } else {
            s
        };
        let segs = self.cum.len() - 1;
        let i = self.cum[1..segs].partition_point(|c| *c < s).min(segs - 1);
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let len = self.cum[i + 1] - self.cum[i];
        let t = if len > 0.0 {
            (s - self.cum[i]) / len
        } else {
            0.0
        };
        (
            Vec2 {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            },
            (b.y - a.y).atan2(b.x - a.x),
        )
    }

    /// The path a drawn layer traces: a polygon / path (Catmull–Rom curves
    /// flattened), line, ellipse, rect, or the first such shape in a group
    /// (through its transform).
    pub fn from_layer(node: &SceneNode) -> Option<Path> {
        let (pts, closed) = layer_points(node, &scene::IDENTITY)?;
        Path::new(pts, closed)
    }
}

fn apply(m: &Affine, p: Vec2) -> Vec2 {
    Vec2 {
        x: m[0] * p.x + m[2] * p.y + m[4],
        y: m[1] * p.x + m[3] * p.y + m[5],
    }
}

fn layer_points(node: &SceneNode, m: &Affine) -> Option<(Vec<Vec2>, bool)> {
    let pt = |x: f64, y: f64| apply(m, Vec2 { x, y });
    match node {
        SceneNode::Group {
            children,
            transform,
            ..
        } => {
            let gm = transform.map_or(*m, |t| scene::mul(m, &t));
            children.iter().find_map(|c| layer_points(c, &gm))
        }
        SceneNode::Polygon {
            points,
            closed,
            tension,
            ..
        } => {
            let t = tension.unwrap_or(0.0);
            let pts: Vec<Vec2> = if t != 0.0 && t.is_finite() && points.len() > 2 {
                const STEPS: usize = 12;
                let mut out = vec![points[0]];
                let mut p0 = points[0];
                for bz in scene::catmull_rom_beziers(points, *closed, t) {
                    for k in 1..=STEPS {
                        let u = k as f64 / STEPS as f64;
                        let w = 1.0 - u;
                        let (a, b, c, d) = (w * w * w, 3.0 * w * w * u, 3.0 * w * u * u, u * u * u);
                        out.push(Vec2 {
                            x: a * p0.x + b * bz.c1.x + c * bz.c2.x + d * bz.end.x,
                            y: a * p0.y + b * bz.c1.y + c * bz.c2.y + d * bz.end.y,
                        });
                    }
                    p0 = bz.end;
                }
                out
            } else {
                points.to_vec()
            };
            Some((pts.into_iter().map(|p| apply(m, p)).collect(), *closed))
        }
        SceneNode::Line { x1, y1, x2, y2, .. } => Some((vec![pt(*x1, *y1), pt(*x2, *y2)], false)),
        SceneNode::Ellipse {
            x, y, w, h, mode, ..
        } => {
            let [bx, by, bw, bh] = scene::resolve_box(*x, *y, *w, *h, mode);
            let (cx, cy, rx, ry) = (bx + bw / 2.0, by + bh / 2.0, bw / 2.0, bh / 2.0);
            const N: usize = 128;
            let pts = (0..N)
                .map(|i| {
                    let a = i as f64 / N as f64 * std::f64::consts::TAU;
                    pt(cx + a.cos() * rx, cy + a.sin() * ry)
                })
                .collect();
            Some((pts, true))
        }
        SceneNode::Rect {
            x, y, w, h, mode, ..
        } => {
            let [bx, by, bw, bh] = scene::resolve_box(*x, *y, *w, *h, mode);
            Some((
                vec![
                    pt(bx, by),
                    pt(bx + bw, by),
                    pt(bx + bw, by + bh),
                    pt(bx, by + bh),
                ],
                true,
            ))
        }
        _ => None,
    }
}

fn dist(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// A glyph placed on a path: its origin (baseline, pen position) and the
/// rotation it's drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathGlyph {
    pub glyph: ShapedGlyph,
    pub pos: Vec2,
    pub angle: f64,
}

/// Lay a shaped line along `path`. The line starts `offset` units along
/// the path (`align` `center` / `right` first centre it on / end it at the
/// path's end); `shift` raises the baseline off the path. Each glyph sits
/// with the middle of its advance on the path, turned to the tangent there.
/// On an open path, glyphs whose middle falls off either end are dropped.
pub fn on_path(
    line: &ShapedLine,
    path: &Path,
    offset: f64,
    align: &str,
    shift: f64,
) -> Vec<PathGlyph> {
    let total = path.length();
    let start = offset
        + match align {
            "center" => (total - line.width) / 2.0,
            "right" => total - line.width,
            _ => 0.0,
        };
    let mut out = Vec::with_capacity(line.glyphs.len());
    for g in &line.glyphs {
        let mid = start + g.x + g.advance / 2.0;
        if !mid.is_finite() || (!path.closed() && (mid < 0.0 || mid > total)) {
            continue;
        }
        let (p, a) = path.at(mid);
        let (s, c) = a.sin_cos();
        let (dx, dy) = (-g.advance / 2.0, g.y - shift);
        out.push(PathGlyph {
            glyph: *g,
            pos: Vec2 {
                x: p.x + dx * c - dy * s,
                y: p.y + dx * s + dy * c,
            },
            angle: a,
        });
    }
    out
}
