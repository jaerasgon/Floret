//! `geometry/layout` (Layout): `count` items arranged as a `grid`, a
//! `ring` (or an arc of one), a phyllotaxis `spiral` or evenly `along` a
//! path, with each item's position, angle (outwards on a ring or spiral,
//! the tangent along a path) and scale.
//!
//! What it adds over the Grid / Ring / Spiral spreads is the move between
//! layouts: when `count`, the mode or a grid's `cols` changes, or the
//! node is edited, every item eases from where it is to its new place
//! over `duration` seconds (`easing`; `stagger` 0..1 starts later items
//! later). Items that appear grow in (scale 0 → 1) at their place; items
//! that go shrink away where they are and drop out once the move ends, so
//! the spreads are longer than `count` until then. A move in progress that
//! is interrupted carries on from where the items are. Animated inputs
//! (a radius on an LFO) are followed exactly, and `reset` jumps straight
//! to the layout.
//!
//! The items' state survives recompiles (an edit hands it to the rebuilt
//! node, see [`NodeImpl::carry`]), so editing the node animates too.

use super::measure::Measured;
use super::{int_pin, layer_contours, live_mode, num_pin, param_str, vec2_pin, Pins};
use crate::node::{CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::nodes::motion::ease_fn;
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Spread, Value, Vec2};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

const MODE: &str = "grid";
/// The modes, in dropdown order.
pub const MODES: [&str; 4] = ["grid", "ring", "spiral", "path"];
/// The easing curves (`motion/ease`'s), in dropdown order.
pub const EASINGS: [&str; 10] = [
    "linear",
    "inQuad",
    "outQuad",
    "inOutQuad",
    "inCubic",
    "outCubic",
    "inOutCubic",
    "inElastic",
    "outElastic",
    "inOutElastic",
];
const COUNT: f64 = 16.0;
const DURATION: f64 = 0.8;
/// Most items laid out (it warns when it stops).
pub const MAX_ITEMS: usize = 20_000;

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![int_pin("count", COUNT), vec2_pin("center", 0.0, 0.0)];
    match param_str(params, "mode", MODE) {
        "ring" => {
            v.push(num_pin("radius", 150.0));
            v.push(num_pin("start", -PI / 2.0));
            v.push(num_pin("sweep", TAU));
        }
        "spiral" => v.push(num_pin("spacing", 14.0)),
        "path" => {
            v.push(PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread));
            v.push(PortSpec::new("closed", PortType::Boolean).default(Value::Bool(false)));
            v.push(PortSpec::new("path", PortType::Layer).kind(PortKind::Spread));
        }
        _ => {
            v.push(int_pin("cols", 0.0));
            v.push(vec2_pin("spacing", 40.0, 40.0));
        }
    }
    v.push(num_pin("duration", DURATION));
    v.push(num_pin("stagger", 0.0));
    let mut easing = PortSpec::new("easing", PortType::String).default(Value::str("inOutCubic"));
    easing.param = Some("easing".into());
    v.push(easing);
    v.push(PortSpec::new("reset", PortType::Boolean).default(Value::Bool(false)));
    v
}

/// One item: where it is, which way it faces, how big.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub pos: Vec2,
    pub angle: f64,
    pub scale: f64,
}

impl Item {
    fn at(pos: Vec2, angle: f64) -> Item {
        Item {
            pos,
            angle,
            scale: 1.0,
        }
    }
}

/// `count` items laid out by `mode`.
fn arrange(mode: &str, count: usize, pins: &Pins, inputs: &[Spread]) -> Vec<Item> {
    let c = pins.vec2(inputs, "center", Vec2::default());
    let off = |x: f64, y: f64| Vec2 {
        x: c.x + x,
        y: c.y + y,
    };
    match mode {
        "ring" => {
            let r = pins.num(inputs, "radius", 150.0);
            let start = pins.num(inputs, "start", -PI / 2.0);
            let sweep = pins.num(inputs, "sweep", TAU);
            // A full turn spaces n round it; an arc puts its ends on the
            // first and last items.
            let full = sweep.abs() >= TAU - 1e-9;
            let gaps = if full || count < 2 {
                count.max(1)
            } else {
                count - 1
            };
            (0..count)
                .map(|i| {
                    let a = start + sweep * i as f64 / gaps as f64;
                    Item::at(off(r * a.cos(), r * a.sin()), a)
                })
                .collect()
        }
        "spiral" => {
            // Vogel's sunflower: equal area per item.
            let s = pins.num(inputs, "spacing", 14.0);
            let golden = PI * (3.0 - 5f64.sqrt());
            (0..count)
                .map(|i| {
                    let a = i as f64 * golden;
                    let r = s * (i as f64 + 0.5).sqrt();
                    Item::at(off(r * a.cos(), r * a.sin()), a)
                })
                .collect()
        }
        "path" => along(count, pins, inputs, c),
        _ => {
            let cols = pins.num(inputs, "cols", 0.0);
            let cols = if cols >= 1.0 {
                (cols.floor() as usize).max(1)
            } else {
                ((count as f64).sqrt().ceil() as usize).max(1)
            };
            let rows = count.div_ceil(cols).max(1);
            let sp = pins.vec2(inputs, "spacing", Vec2 { x: 40.0, y: 40.0 });
            let (w, h) = ((cols - 1) as f64 * sp.x, (rows - 1) as f64 * sp.y);
            (0..count)
                .map(|i| {
                    let (col, row) = ((i % cols) as f64, (i / cols) as f64);
                    Item::at(off(col * sp.x - w / 2.0, row * sp.y - h / 2.0), 0.0)
                })
                .collect()
        }
    }
}

/// Items spaced evenly by arc length along the wired `path` layer's
/// outlines (end to end) or the `points` polyline, facing along it.
fn along(count: usize, pins: &Pins, inputs: &[Spread], center: Vec2) -> Vec<Item> {
    let mut paths: Vec<Measured> = layer_contours(pins.spread(inputs, "path"))
        .iter()
        .filter_map(|c| Measured::new(&c.pts, c.closed))
        .collect();
    if paths.is_empty() {
        let pts = pins.points(inputs, "points");
        paths.extend(Measured::new(&pts, pins.bool(inputs, "closed", false)));
    }
    let total: f64 = paths.iter().map(Measured::length).sum();
    if paths.is_empty() || total.is_nan() || total <= 0.0 {
        return vec![Item::at(center, 0.0); count];
    }
    // One path: its ends (open) or round from its start (closed); several:
    // centred in equal shares of the whole length.
    let (gaps, half) = match (paths.len(), paths[0].closed) {
        (1, true) => (count as f64, 0.0),
        (1, false) => ((count.max(2) - 1) as f64, 0.0),
        _ => (count as f64, 0.5),
    };
    let ends: Vec<f64> = paths
        .iter()
        .scan(0.0, |acc, p| {
            *acc += p.length();
            Some(*acc)
        })
        .collect();
    (0..count)
        .map(|i| {
            let s = if count == 1 && half == 0.0 {
                0.0
            } else {
                (i as f64 + half) * total / gaps
            };
            let k = ends.partition_point(|e| *e < s).min(paths.len() - 1);
            let before = if k == 0 { 0.0 } else { ends[k - 1] };
            let (p, a) = paths[k].at(s - before);
            Item::at(p, a)
        })
        .collect()
}

/// The angle from `a` towards `b` the short way round, `t` of the way.
fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let d = (b - a + PI).rem_euclid(TAU) - PI;
    a + d * t
}

/// A move between layouts in progress.
#[derive(Clone, Debug, Default)]
struct Move {
    /// Where every item was when it started (beyond `count`: leaving).
    from: Vec<Item>,
    elapsed: f64,
}

/// What a Layout node keeps between ticks (and across recompiles).
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    /// The items as last shown.
    shown: Vec<Item>,
    count: usize,
    mode: String,
    cols: f64,
    moving: Option<Move>,
    /// Rebuilt after an edit: move to the new layout if it differs.
    edited: bool,
}

impl State {
    /// One tick: the items to show for target layout `target` (`key`:
    /// count, mode and cols, whose change starts a move).
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        target: &[Item],
        mode: &str,
        cols: f64,
        dt: f64,
        duration: f64,
        stagger: f64,
        easing: &str,
        reset: bool,
    ) -> (Vec<Item>, f64) {
        let first = self.shown.is_empty() && self.mode.is_empty();
        let changed = target.len() != self.count || mode != self.mode || cols != self.cols;
        let edited = std::mem::take(&mut self.edited) && !same(&self.shown, target);
        self.count = target.len();
        self.mode = mode.to_owned();
        self.cols = cols;
        if reset || first || duration.is_nan() || duration <= 0.0 {
            self.moving = None;
            self.shown = target.to_vec();
            return (self.shown.clone(), 1.0);
        }
        if changed || edited {
            self.moving = Some(Move {
                from: self.shown.clone(),
                elapsed: 0.0,
            });
        }
        let Some(mv) = &mut self.moving else {
            self.shown = target.to_vec();
            return (self.shown.clone(), 1.0);
        };
        mv.elapsed += if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let progress = (mv.elapsed / duration).clamp(0.0, 1.0);
        let stagger = if stagger.is_finite() {
            stagger.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let n = target.len().max(mv.from.len());
        // Item i's own eased progress: a `1 − stagger` long window that
        // starts `stagger × i / (n − 1)` of the way in.
        let local = |i: usize| {
            let start = if n > 1 {
                stagger * i as f64 / (n - 1) as f64
            } else {
                0.0
            };
            let span = (1.0 - stagger).max(1e-9);
            ease_fn(easing, ((progress - start) / span).clamp(0.0, 1.0))
        };
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let u = local(i);
            out.push(match (mv.from.get(i), target.get(i)) {
                (Some(a), Some(b)) => Item {
                    pos: super::lerp(a.pos, b.pos, u),
                    angle: lerp_angle(a.angle, b.angle, u),
                    scale: a.scale + (b.scale - a.scale) * u,
                },
                // Arriving: grows in at its place.
                (None, Some(b)) => Item { scale: u, ..*b },
                // Leaving: shrinks away where it was.
                (Some(a), None) => Item {
                    scale: a.scale * (1.0 - u),
                    ..*a
                },
                (None, None) => unreachable!("i < n"),
            });
        }
        if progress >= 1.0 {
            self.moving = None;
            out.truncate(target.len());
        }
        self.shown.clone_from(&out);
        (out, progress)
    }
}

/// The same layout (to a hair).
fn same(a: &[Item], b: &[Item]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            super::dist(x.pos, y.pos) < 1e-6
                && (x.angle - y.angle).abs() < 1e-9
                && (x.scale - y.scale).abs() < 1e-9
        })
}

struct Layout {
    pins: Pins,
    state: State,
}

pub(super) fn layout(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Layout {
        pins: Pins::new(init),
        state: State::default(),
    })
}

impl NodeImpl for Layout {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let pins = &self.pins;
        let mode = live_mode(cx, MODE);
        let mode = MODES.iter().find(|m| **m == mode).copied().unwrap_or(MODE);
        let wanted = pins.num(inputs, "count", COUNT).max(0.0);
        let count = (wanted as usize).min(MAX_ITEMS);
        if wanted > MAX_ITEMS as f64 {
            cx.warn(format!("Stopped at {MAX_ITEMS} items."));
        }
        let target = arrange(mode, count, pins, inputs);
        let easing = match pins.value(inputs, "easing") {
            Value::Str(s) if EASINGS.contains(&&*s) => s.to_string(),
            _ => "inOutCubic".to_owned(),
        };
        // The transport restarting starts afresh.
        if cx.tick_frame.frame_count == 0.0 {
            self.state = State::default();
        }
        let cols = if mode == "grid" {
            pins.num(inputs, "cols", 0.0)
        } else {
            0.0
        };
        let (items, progress) = self.state.step(
            &target,
            mode,
            cols,
            cx.tick_frame.dt,
            pins.num(inputs, "duration", DURATION),
            pins.num(inputs, "stagger", 0.0),
            &easing,
            pins.bool(inputs, "reset", false),
        );
        let positions: Vec<Value> = items.iter().map(|i| Value::Vec2(i.pos)).collect();
        let angles: Vec<Value> = items.iter().map(|i| Value::Num(i.angle)).collect();
        let scales: Vec<Value> = items.iter().map(|i| Value::Num(i.scale)).collect();
        Ok(vec![
            Arc::from(positions),
            Arc::from(angles),
            Arc::from(scales),
            one(Value::Num(progress)),
        ])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(s) = state.downcast::<State>() {
            self.state = *s;
            self.state.edited = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::node;
    use serde_json::json;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    fn items(pts: &[Vec2]) -> Vec<Item> {
        pts.iter().map(|p| Item::at(*p, 0.0)).collect()
    }

    fn step(s: &mut State, target: &[Item], mode: &str, dt: f64) -> (Vec<Item>, f64) {
        s.step(target, mode, 0.0, dt, 1.0, 0.0, "linear", false)
    }

    #[test]
    fn first_layout_shows_at_once_then_follows() {
        let mut s = State::default();
        let a = items(&[v(0.0, 0.0), v(10.0, 0.0)]);
        assert_eq!(step(&mut s, &a, "grid", 0.1), (a.clone(), 1.0));
        // Same count and mode: the target is followed exactly (animated
        // inputs aren't smoothed).
        let b = items(&[v(5.0, 0.0), v(15.0, 0.0)]);
        assert_eq!(step(&mut s, &b, "grid", 0.1).0, b);
    }

    #[test]
    fn count_and_mode_changes_ease_across() {
        let mut s = State::default();
        let a = items(&[v(0.0, 0.0), v(100.0, 0.0)]);
        step(&mut s, &a, "grid", 0.0);
        // A third item: the first two move, the new one grows in.
        let b = items(&[v(0.0, 100.0), v(100.0, 100.0), v(50.0, 50.0)]);
        let (mid, p) = step(&mut s, &b, "grid", 0.5);
        assert!((p - 0.5).abs() < 1e-12);
        assert_eq!(mid.len(), 3);
        assert!((mid[0].pos.y - 50.0).abs() < 1e-9);
        assert!((mid[2].scale - 0.5).abs() < 1e-9);
        assert_eq!(mid[2].pos, v(50.0, 50.0));
        let (end, p) = step(&mut s, &b, "grid", 0.5);
        assert_eq!((end, p), (b.clone(), 1.0));
        // Back to one, in another mode: the others shrink where they are,
        // then drop out.
        let c = items(&[v(0.0, 0.0)]);
        let (mid, _) = step(&mut s, &c, "ring", 0.25);
        assert_eq!(mid.len(), 3);
        assert_eq!(mid[1].pos, v(100.0, 100.0));
        assert!((mid[1].scale - 0.75).abs() < 1e-9);
        let (end, _) = step(&mut s, &c, "ring", 1.0);
        assert_eq!(end, c);
    }

    #[test]
    fn interrupted_moves_carry_on_from_where_items_are() {
        let mut s = State::default();
        step(&mut s, &items(&[v(0.0, 0.0)]), "grid", 0.0);
        step(&mut s, &items(&[v(100.0, 0.0), v(0.0, 0.0)]), "grid", 0.5);
        // Halfway, the mode changes: the move restarts from x = 50.
        let (o, p) = step(&mut s, &items(&[v(50.0, 100.0), v(0.0, 0.0)]), "ring", 0.0);
        assert_eq!(p, 0.0);
        assert!((o[0].pos.x - 50.0).abs() < 1e-9 && o[0].pos.y.abs() < 1e-9);
    }

    #[test]
    fn reset_zero_duration_and_edits() {
        let mut s = State::default();
        step(&mut s, &items(&[v(0.0, 0.0)]), "grid", 0.0);
        let b = items(&[v(9.0, 9.0), v(1.0, 1.0)]);
        let (o, _) = s.step(&b, "grid", 0.0, 0.1, 1.0, 0.0, "linear", true);
        assert_eq!(o, b, "reset jumps");
        let c = items(&[v(0.0, 0.0)]);
        let (o, _) = s.step(&c, "grid", 0.0, 0.1, 0.0, 0.0, "linear", false);
        assert_eq!(o, c, "no duration jumps");
        // After an edit (a recompile), a changed layout eases across even
        // with the same count and mode; an unchanged one doesn't move.
        s.edited = true;
        let (o, p) = step(&mut s, &c, "grid", 0.5);
        assert_eq!((o, p), (c.clone(), 1.0));
        s.edited = true;
        let d = items(&[v(10.0, 0.0)]);
        let (o, p) = step(&mut s, &d, "grid", 0.5);
        assert!((p - 0.5).abs() < 1e-12 && (o[0].pos.x - 5.0).abs() < 1e-9);
    }

    #[test]
    fn stagger_starts_later_items_later() {
        let mut s = State::default();
        step(
            &mut s,
            &items(&[v(0.0, 0.0), v(0.0, 0.0), v(0.0, 0.0)]),
            "grid",
            0.0,
        );
        let t = items(&[v(10.0, 0.0), v(10.0, 0.0), v(10.0, 0.0)]);
        let (o, _) = s.step(&t, "ring", 0.0, 0.25, 1.0, 0.5, "linear", false);
        // Windows of 0.5: item 0 is half way, item 1 just starting, item 2
        // waiting.
        assert!((o[0].pos.x - 5.0).abs() < 1e-9, "{:?}", o[0]);
        assert!(o[1].pos.x.abs() < 1e-9);
        assert!(o[2].pos.x.abs() < 1e-9);
        let (o, _) = s.step(&t, "ring", 0.0, 0.75, 1.0, 0.5, "linear", false);
        assert_eq!(o, t);
    }

    #[test]
    fn angles_take_the_short_way_round() {
        assert!((lerp_angle(3.0, -3.0, 0.5) - (3.0 + (TAU - 6.0) / 2.0)).abs() < 1e-12);
        assert!((lerp_angle(0.0, 1.0, 0.5) - 0.5).abs() < 1e-12);
    }

    /// Positions, angles, scales, progress after one tick of a fresh node.
    fn run(params: serde_json::Value, inputs: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
        node("geometry/layout", params).eval(inputs).unwrap()
    }

    fn points(out: &[Value]) -> Vec<Vec2> {
        out.iter()
            .map(|p| match p {
                Value::Vec2(p) => *p,
                o => panic!("{o:?}"),
            })
            .collect()
    }

    #[test]
    fn layouts() {
        // grid: count, center, cols, spacing — 5 in 3 columns, centred.
        let out = run(
            json!({ "mode": "grid" }),
            vec![
                vec![Value::Num(5.0)],
                vec![],
                vec![Value::Num(3.0)],
                vec![Value::Vec2(v(10.0, 20.0))],
            ],
        );
        let p = points(&out[0]);
        assert_eq!(p[0], v(-10.0, -10.0));
        assert_eq!(p[4], v(0.0, 10.0));
        assert_eq!(out[3][0].to_number(), 1.0);
        // Auto columns: 9 → 3 × 3.
        let p = points(&run(json!({}), vec![vec![Value::Num(9.0)]])[0]);
        assert_eq!(p[8], v(40.0, 40.0));
        // ring: a full turn from the top; angles point outwards.
        let out = run(
            json!({ "mode": "ring" }),
            vec![vec![Value::Num(4.0)], vec![], vec![Value::Num(100.0)]],
        );
        let p = points(&out[0]);
        assert!((p[0].y + 100.0).abs() < 1e-9 && p[0].x.abs() < 1e-9);
        assert!((p[1].x - 100.0).abs() < 1e-9);
        assert_eq!(out[1][1].to_number(), 0.0);
        // An arc puts items on both ends.
        let out = run(
            json!({ "mode": "ring" }),
            vec![
                vec![Value::Num(3.0)],
                vec![],
                vec![Value::Num(10.0)],
                vec![Value::Num(0.0)],
                vec![Value::Num(PI)],
            ],
        );
        let p = points(&out[0]);
        assert!((p[2].x + 10.0).abs() < 1e-9);
        // spiral: equal area, growing like √i.
        let p = points(&run(json!({ "mode": "spiral" }), vec![vec![Value::Num(50.0)]])[0]);
        let r = |q: Vec2| q.x.hypot(q.y);
        assert!((r(p[49]) / r(p[11]) - (49.5f64 / 11.5).sqrt()).abs() < 1e-9);
        // path: along a polyline from end to end, facing along it.
        let line = vec![Value::Vec2(v(0.0, 0.0)), Value::Vec2(v(100.0, 0.0))];
        let out = run(
            json!({ "mode": "path" }),
            vec![vec![Value::Num(5.0)], vec![], line],
        );
        let p = points(&out[0]);
        assert_eq!(p[0], v(0.0, 0.0));
        assert_eq!(p[4], v(100.0, 0.0));
        assert_eq!(out[1][2].to_number(), 0.0);
        // Nothing to follow: every item at the centre.
        let out = run(
            json!({ "mode": "path" }),
            vec![vec![Value::Num(2.0)], vec![Value::Vec2(v(3.0, 4.0))]],
        );
        assert_eq!(points(&out[0]), [v(3.0, 4.0), v(3.0, 4.0)]);
        // Zero and negative counts: nothing.
        assert!(run(json!({}), vec![vec![Value::Num(-3.0)]])[0].is_empty());
    }

    #[test]
    fn statics_match_the_default_mode() {
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("geometry/layout").unwrap();
        let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
        let ins: Vec<String> = cat
            .resolved_inputs(spec, &Params::new())
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(statics, ins);
        assert!(spec.effectful);
    }

    #[test]
    fn state_carries_over_a_recompile() {
        let patch = |count: f64| {
            json!({
                "kind": "floret.patch", "version": 2,
                "nodes": [{
                    "id": "l", "defType": "geometry/layout", "position": {"x": 0, "y": 0},
                    "params": { "mode": "grid" },
                    "portValues": { "count": count, "duration": 1.0, "easing": "linear" },
                }],
                "edges": [],
            })
            .to_string()
        };
        let frame = |f: f64| crate::FrameCtx {
            dt: 0.25,
            frame_count: f,
            ..Default::default()
        };
        let mut a = crate::Engine::load(&patch(1.0)).unwrap();
        a.tick(&frame(0.0));
        a.tick(&frame(1.0));
        // The edit: two items now. The rebuilt node moves the first one
        // from where it was, a quarter of the way per tick.
        let mut b = crate::Engine::load(&patch(2.0)).unwrap();
        b.adopt_state(&mut a);
        let r = b.tick(&frame(2.0));
        let idx = b.graph.idx("l").unwrap();
        let progress = r.outputs[idx].as_ref().unwrap()[3][0].to_number();
        assert!((progress - 0.25).abs() < 1e-9, "{progress}");
        // A fresh engine (a restart) starts settled.
        let mut c = crate::Engine::load(&patch(2.0)).unwrap();
        let r = c.tick(&frame(2.0));
        let progress = r.outputs[idx].as_ref().unwrap()[3][0].to_number();
        assert_eq!(progress, 1.0);
    }
}
