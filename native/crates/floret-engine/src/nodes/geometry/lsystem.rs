//! `geometry/lsystem`: an L-system grown from `axiom` by `rules`,
//! `iterations` times, then drawn by a turtle.
//!
//! Rules are `X=…` (or `X->…`), separated by `;`, `,` or new lines. The
//! turtle reads `F` / `G` (draw a step), `f` / `g` (move a step without
//! drawing), `+` / `-` (turn left / right by `angle` degrees), `|` (turn
//! round), `[` / `]` (save / restore position and heading); every other
//! symbol only takes part in the rewriting. `jitter` adds up to that many
//! degrees, at random, to every turn.
//!
//! With `fit` > 0 the drawing is scaled to fit a `fit` × `fit` box centred
//! on `position`; with 0 the turtle starts at `position` and steps `step`.
//!
//! A node of its own rather than a mode of another geometry node: none of
//! its pins (axiom, rules, angle, …) is shared with them.

use super::{lerp, pin_style, poly_outputs, rng, Pins, Poly, MAX_POINTS};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{Value, Vec2};
use std::collections::HashMap;

/// Longest string the rewriting may grow to.
pub(super) const MAX_SYMBOLS: usize = 1_000_000;

/// Rewrite `axiom` by `rules` `iterations` times. Stops early (and says
/// so) when the string would grow past [`MAX_SYMBOLS`].
pub(super) fn expand(
    axiom: &str,
    rules: &HashMap<char, String>,
    iterations: usize,
) -> (String, bool) {
    let mut s: String = axiom.to_owned();
    for _ in 0..iterations {
        let len: usize = s
            .chars()
            .map(|c| rules.get(&c).map_or(1, |r| r.chars().count()))
            .sum();
        if len > MAX_SYMBOLS {
            return (s, true);
        }
        let mut next = String::with_capacity(len);
        for c in s.chars() {
            match rules.get(&c) {
                Some(r) => next.push_str(r),
                None => next.push(c),
            }
        }
        s = next;
    }
    (s, false)
}

/// `F=F+F; X -> F[+X]` → rules by symbol, plus the pieces that aren't
/// rules.
pub(super) fn parse_rules(text: &str) -> (HashMap<char, String>, Vec<String>) {
    let mut rules = HashMap::new();
    let mut bad = Vec::new();
    for piece in text.split([';', ',', '\n']) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let split = ["->", "→", "="]
            .iter()
            .filter_map(|sep| piece.find(sep).map(|i| (i, sep.len())))
            .min_by_key(|(i, _)| *i);
        let Some((i, w)) = split else {
            bad.push(piece.to_owned());
            continue;
        };
        let mut lhs = piece[..i].trim().chars();
        match (lhs.next(), lhs.next()) {
            (Some(c), None) => {
                let rhs: String = piece[i + w..]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                rules.insert(c, rhs);
            }
            _ => bad.push(piece.to_owned()),
        }
    }
    (rules, bad)
}

/// The turtle's polylines: a branch (`[`) starts a new one, and the
/// parent's carries on after `]`.
pub(super) fn turtle(
    s: &str,
    angle: f64,
    step: f64,
    heading: f64,
    jitter: f64,
    seed: f64,
) -> Vec<Vec<Vec2>> {
    #[derive(Clone, Copy)]
    struct State {
        pos: Vec2,
        dir: f64,
        line: Option<usize>,
    }
    let mut r = rng(seed);
    let mut turn = |deg: f64| {
        let j = if jitter > 0.0 {
            (r.next_f64() * 2.0 - 1.0) * jitter
        } else {
            0.0
        };
        (deg + j).to_radians()
    };
    let mut lines: Vec<Vec<Vec2>> = Vec::new();
    let mut points = 0usize;
    let mut st = State {
        pos: Vec2::default(),
        dir: heading.to_radians(),
        line: None,
    };
    let mut stack: Vec<State> = Vec::new();
    for c in s.chars() {
        match c {
            'F' | 'G' => {
                let to = Vec2 {
                    x: st.pos.x + st.dir.cos() * step,
                    y: st.pos.y + st.dir.sin() * step,
                };
                let line = match st.line {
                    Some(l) => l,
                    None => {
                        lines.push(vec![st.pos]);
                        points += 1;
                        lines.len() - 1
                    }
                };
                lines[line].push(to);
                points += 1;
                st.line = Some(line);
                st.pos = to;
                if points >= MAX_POINTS {
                    break;
                }
            }
            'f' | 'g' => {
                st.pos = Vec2 {
                    x: st.pos.x + st.dir.cos() * step,
                    y: st.pos.y + st.dir.sin() * step,
                };
                st.line = None;
            }
            // y points down: a left turn on screen lowers the heading.
            '+' => st.dir -= turn(angle),
            '-' => st.dir += turn(angle),
            '|' => st.dir += std::f64::consts::PI,
            '[' => {
                stack.push(st);
                st.line = None;
            }
            ']' => {
                if let Some(saved) = stack.pop() {
                    st = saved;
                }
            }
            _ => {}
        }
    }
    lines
}

pub(super) fn lsystem(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let text = |name: &str| match pins.value(inputs, name) {
            Value::Str(s) => s.to_string(),
            _ => String::new(),
        };
        let (rules, bad) = parse_rules(&text("rules"));
        if !bad.is_empty() {
            cx.warn(format!("Not a rule (write X=…): {}", bad.join(", ")));
        }
        let iterations = pins.num(inputs, "iterations", 4.0).clamp(0.0, 64.0) as usize;
        let (s, capped) = expand(&text("axiom"), &rules, iterations);
        if capped {
            cx.warn(format!(
                "Stopped growing at {} symbols (over {MAX_SYMBOLS}).",
                s.chars().count()
            ));
        }
        let angle = pins.num(inputs, "angle", 25.0);
        let step = pins.num(inputs, "step", 10.0);
        let heading = pins.num(inputs, "heading", -90.0);
        let jitter = pins.num(inputs, "jitter", 0.0).abs();
        let seed = pins.num(inputs, "seed", 1.0);
        let fit = pins.num(inputs, "fit", 400.0);
        let pos = pins.vec2(inputs, "position", Vec2::default());
        let mut lines = turtle(&s, angle, step, heading, jitter, seed);
        place(&mut lines, pos, fit);
        let style = pin_style(&pins.value(inputs, "style"), false);
        let polys: Vec<Poly> = lines
            .into_iter()
            .map(|l| Poly {
                rings: vec![l],
                closed: false,
                style: style.clone(),
            })
            .collect();
        Ok(poly_outputs(&polys))
    })
}

/// Move the drawing to `pos`: scaled to fit `fit` × `fit` centred there,
/// or (`fit` <= 0) as drawn from there.
fn place(lines: &mut [Vec<Vec2>], pos: Vec2, fit: f64) {
    let all = lines.iter().flatten();
    let Some((min, max)) = all.fold(None, |acc: Option<(Vec2, Vec2)>, p| {
        Some(match acc {
            None => (*p, *p),
            Some((lo, hi)) => (
                Vec2 {
                    x: lo.x.min(p.x),
                    y: lo.y.min(p.y),
                },
                Vec2 {
                    x: hi.x.max(p.x),
                    y: hi.y.max(p.y),
                },
            ),
        })
    }) else {
        return;
    };
    let (k, from, to) = if fit > 0.0 {
        let extent = (max.x - min.x).max(max.y - min.y);
        let k = if extent > 0.0 { fit / extent } else { 1.0 };
        (k, lerp(min, max, 0.5), pos)
    } else {
        (1.0, Vec2::default(), pos)
    };
    for p in lines.iter_mut().flatten() {
        *p = Vec2 {
            x: to.x + (p.x - from.x) * k,
            y: to.y + (p.y - from.y) * k,
        };
    }
}
