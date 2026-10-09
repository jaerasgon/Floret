//! Arc-length helpers and line styles shared by Trim Path, Shape Morph
//! and Connect Points.

use super::{dist, lerp};
use crate::scene::{Gradient, GradientStop, Paint, ResolvedStyle};
use crate::value::{Color, Vec2};
use std::sync::Arc;

/// A paint with its alpha scaled by `k` (gradient stops too).
pub(crate) fn fade_paint(p: &Paint, k: f64) -> Paint {
    let k = if k.is_finite() {
        k.clamp(0.0, 1.0)
    } else {
        1.0
    };
    match &p.gradient {
        Some(g) => Paint::gradient(Gradient {
            stops: g
                .stops
                .iter()
                .map(|s| GradientStop {
                    pos: s.pos,
                    rgba: [s.rgba[0], s.rgba[1], s.rgba[2], s.rgba[3] * k],
                })
                .collect(),
            ..(**g).clone()
        }),
        None => {
            let [r, g, b, a] = p.rgba;
            Paint::from_color(Color { r, g, b, a: a * k })
        }
    }
}

/// The style a shape's outline is drawn with as a line: no fill, and a
/// filled shape without a stroke strokes in its fill colour.
pub(crate) fn outline_style(st: &Arc<ResolvedStyle>) -> Arc<ResolvedStyle> {
    if st.fill.is_none() {
        return st.clone();
    }
    let mut s = (**st).clone();
    if s.stroke.is_none() {
        s.stroke = s.fill.clone();
    }
    s.fill = None;
    Arc::new(s)
}

/// A polyline measured by arc length: the cleaned points (finite, no
/// repeats) and the length at each vertex, with one more entry for a
/// closed path's closing edge.
#[derive(Clone, Debug)]
pub(crate) struct Measured {
    pub pts: Vec<Vec2>,
    pub cum: Vec<f64>,
    pub closed: bool,
}

impl Measured {
    /// `None` with fewer than two distinct points.
    pub fn new(pts: &[Vec2], closed: bool) -> Option<Measured> {
        let mut clean: Vec<Vec2> = Vec::with_capacity(pts.len());
        for p in pts {
            if !(p.x.is_finite() && p.y.is_finite()) {
                continue;
            }
            if clean.last().is_some_and(|q| dist(*q, *p) < 1e-9) {
                continue;
            }
            clean.push(*p);
        }
        if closed && clean.len() > 2 && dist(clean[0], clean[clean.len() - 1]) < 1e-9 {
            clean.pop();
        }
        if clean.len() < 2 {
            return None;
        }
        let n = clean.len();
        let segs = if closed { n } else { n - 1 };
        let mut cum = Vec::with_capacity(segs + 1);
        cum.push(0.0);
        for i in 0..segs {
            let l = cum[i] + dist(clean[i], clean[(i + 1) % n]);
            cum.push(l);
        }
        Some(Measured {
            pts: clean,
            cum,
            closed,
        })
    }

    pub fn length(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// The edge holding arc length `s`.
    fn edge(&self, s: f64) -> usize {
        let segs = self.cum.len() - 1;
        self.cum[1..segs].partition_point(|c| *c < s).min(segs - 1)
    }

    /// Point and tangent angle at arc length `s`, clamped to the path.
    pub fn at(&self, s: f64) -> (Vec2, f64) {
        let s = s.clamp(0.0, self.length());
        let n = self.pts.len();
        let i = self.edge(s);
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let len = self.cum[i + 1] - self.cum[i];
        let t = if len > 0.0 {
            (s - self.cum[i]) / len
        } else {
            0.0
        };
        (lerp(a, b, t), (b.y - a.y).atan2(b.x - a.x))
    }

    /// The polyline from arc length `s0` to `s1` (clamped; empty when
    /// `s1 < s0`).
    pub fn slice(&self, s0: f64, s1: f64) -> Vec<Vec2> {
        let total = self.length();
        let (s0, s1) = (s0.clamp(0.0, total), s1.clamp(0.0, total));
        if s1 < s0 {
            return Vec::new();
        }
        let n = self.pts.len();
        let mut out = vec![self.at(s0).0];
        for i in self.edge(s0)..self.edge(s1) {
            let p = self.pts[(i + 1) % n];
            if dist(out[out.len() - 1], p) > 1e-9 {
                out.push(p);
            }
        }
        let end = self.at(s1).0;
        if out.len() == 1 || dist(out[out.len() - 1], end) > 1e-9 {
            out.push(end);
        }
        out
    }

    /// `n` points evenly spaced by arc length: round a closed path from
    /// its first point, or from end to end of an open one.
    pub fn even(&self, n: usize) -> Vec<Vec2> {
        let total = self.length();
        match n {
            0 => Vec::new(),
            _ if self.closed => (0..n)
                .map(|i| self.at(i as f64 * total / n as f64).0)
                .collect(),
            1 => vec![self.pts[0]],
            _ => (0..n)
                .map(|i| self.at(i as f64 * total / (n - 1) as f64).0)
                .collect(),
        }
    }
}
