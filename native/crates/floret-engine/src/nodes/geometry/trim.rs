//! `geometry/trim` (Trim Path): the part of each outline between `start`
//! and `end` (fractions of its arc length), slid round by `offset`, which
//! wraps. The classic draw-on / write-on animation.
//!
//! Modes: `each` trims every outline (sub-path) by the same fractions at
//! once; `sequence` lays the outlines end to end and trims the whole run,
//! so they draw one after another (handwriting a glyph outline, a plotter
//! drawing). The visible pieces come out as lines; `heads` / `angles`
//! give where each piece's drawing end is and which way it's heading, for
//! a pen tip or an arrowhead.

use super::measure::{outline_style, Measured};
use super::{layer_contours, live_mode, pin_style, poly_outputs, Contour, Pins, Poly};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{Spread, Value, Vec2};

const MODE: &str = "each";

pub(super) fn trim(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let style_v = pins.value(inputs, "style");
        let wired = matches!(style_v, Value::Style(_));
        let mut contours = layer_contours(pins.spread(inputs, "path"));
        if contours.is_empty() {
            let closed = pins.bool(inputs, "closed", false);
            contours.push(Contour {
                pts: pins.points(inputs, "points"),
                closed,
                style: pin_style(&style_v, false),
                source: 0,
            });
        }
        for c in &mut contours {
            c.style = if wired {
                pin_style(&style_v, false)
            } else {
                outline_style(&c.style)
            };
        }
        let window = Window::new(
            pins.num(inputs, "start", 0.0),
            pins.num(inputs, "end", 1.0),
            pins.num(inputs, "offset", 0.0),
        );
        let paths: Vec<(Measured, &Contour)> = contours
            .iter()
            .filter_map(|c| Some((Measured::new(&c.pts, c.closed)?, c)))
            .filter(|(m, _)| m.length() > 1e-9)
            .collect();
        let trimmed = if mode == "sequence" {
            trim_sequence(&paths, window)
        } else {
            trim_each(&paths, window)
        };
        let mut out = poly_outputs(&trimmed.polys);
        out.push(Spread::from(
            trimmed
                .heads
                .iter()
                .map(|(p, _)| Value::Vec2(*p))
                .collect::<Vec<_>>(),
        ));
        out.push(Spread::from(
            trimmed
                .heads
                .iter()
                .map(|(_, a)| Value::Num(*a))
                .collect::<Vec<_>>(),
        ));
        Ok(out)
    })
}

/// The visible stretch as fractions of a length: from `from` (in 0..1)
/// for `span` (0..=1); it may run past 1, and then wraps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Window {
    pub from: f64,
    pub span: f64,
}

impl Window {
    /// `start` / `end` are clamped to 0..1 (and swapped when reversed);
    /// `offset` slides both and wraps.
    pub fn new(start: f64, end: f64, offset: f64) -> Window {
        let unit = |v: f64, d: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        let (s, e) = (unit(start, 0.0), unit(end, 1.0));
        let (s, e) = if e < s { (e, s) } else { (s, e) };
        let offset = if offset.is_finite() { offset } else { 0.0 };
        let span = e - s;
        // A full window starts at the offset itself (so a closed path's
        // head keeps turning as the offset moves).
        let from = (s + offset).rem_euclid(1.0);
        Window {
            from: if from >= 1.0 { 0.0 } else { from },
            span,
        }
    }

    pub fn full(&self) -> bool {
        self.span >= 1.0 - 1e-12
    }
}

pub(super) struct Trimmed {
    pub polys: Vec<Poly>,
    /// Drawing end of each trimmed outline (`each`) or of the whole run
    /// (`sequence`), with its tangent angle.
    pub heads: Vec<(Vec2, f64)>,
}

/// The pieces of one path inside arc lengths `a..b` (`0 <= a < len`,
/// `a <= b <= a + len`): one piece, or two when it runs past the end of
/// an open path (a closed one carries on round its first point).
fn pieces(m: &Measured, a: f64, b: f64) -> Vec<(Vec<Vec2>, bool)> {
    let len = m.length();
    if b - a >= len - 1e-9 {
        // All of it, from `a`.
        if m.closed {
            let mut ring = m.slice(a, len);
            let rest = m.slice(0.0, a);
            ring.extend(rest.into_iter().skip(1));
            ring.pop();
            return vec![(ring, true)];
        }
        return vec![(m.pts.clone(), false)];
    }
    if b <= len {
        return vec![(m.slice(a, b), false)];
    }
    let (first, second) = (m.slice(a, len), m.slice(0.0, b - len));
    if m.closed {
        let mut joined = first;
        joined.extend(second.into_iter().skip(1));
        vec![(joined, false)]
    } else {
        vec![(first, false), (second, false)]
    }
}

fn push_pieces(polys: &mut Vec<Poly>, c: &Contour, ps: Vec<(Vec<Vec2>, bool)>) {
    for (pts, closed) in ps {
        if pts.len() >= 2 {
            polys.push(Poly {
                rings: vec![pts],
                closed,
                style: c.style.clone(),
            });
        }
    }
}

/// Every path trimmed by the same window.
pub(super) fn trim_each(paths: &[(Measured, &Contour)], w: Window) -> Trimmed {
    let mut polys = Vec::new();
    let mut heads = Vec::new();
    if w.span <= 0.0 {
        return Trimmed { polys, heads };
    }
    for (m, c) in paths {
        let len = m.length();
        let a = w.from * len;
        let b = a + w.span.min(1.0) * len;
        push_pieces(&mut polys, c, pieces(m, a, b));
        let end = if w.full() { a } else { b };
        heads.push(m.at(if end > len { end - len } else { end }));
    }
    Trimmed { polys, heads }
}

/// The paths laid end to end and trimmed as one.
pub(super) fn trim_sequence(paths: &[(Measured, &Contour)], w: Window) -> Trimmed {
    let mut polys = Vec::new();
    let total: f64 = paths.iter().map(|(m, _)| m.length()).sum();
    if w.span <= 0.0 || total <= 0.0 {
        return Trimmed {
            polys,
            heads: Vec::new(),
        };
    }
    let a = w.from * total;
    let b = a + w.span.min(1.0) * total;
    // The window, as up to two intervals of the run.
    let intervals: Vec<(f64, f64)> = if b <= total {
        vec![(a, b)]
    } else {
        vec![(a, total), (0.0, b - total)]
    };
    let mut start = 0.0;
    for (m, c) in paths {
        let len = m.length();
        let (lo, hi) = (start, start + len);
        let covered: Vec<(f64, f64)> = intervals
            .iter()
            .filter_map(|&(x, y)| {
                let (x, y) = (x.max(lo), y.min(hi));
                (y > x).then_some((x - lo, y - lo))
            })
            .collect();
        match covered.as_slice() {
            [] => {}
            // The window wraps inside this path: one stretch through its
            // end and start.
            [(x, y), (z, v)] if *y >= len - 1e-9 && *z <= 1e-9 => {
                push_pieces(&mut polys, c, pieces(m, *x, (len + v).min(x + len)));
            }
            _ => {
                for (x, y) in covered {
                    push_pieces(&mut polys, c, pieces(m, x, y));
                }
            }
        }
        start = hi;
    }
    // The head: where the window ends along the run.
    let mut at = if w.full() { a } else { b };
    if at > total {
        at -= total;
    }
    let mut head = None;
    let mut start = 0.0;
    for (m, _) in paths {
        let len = m.length();
        if at <= start + len + 1e-9 {
            head = Some(m.at(at - start));
            break;
        }
        start += len;
    }
    Trimmed {
        polys,
        heads: head.into_iter().collect(),
    }
}
