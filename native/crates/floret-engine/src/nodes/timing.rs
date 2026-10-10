//! Time-driven building blocks (native-only): `motion/integrate`,
//! `motion/envelope` and `time/beat`.
//!
//! - **Integrate** accumulates `rate · dt` every frame, per element (number,
//!   vec2 or colour): phases, playheads, scrolling, timers, positions from
//!   velocities. It holds or resets on demand and can keep the value
//!   between `min` and `max` (clamp, wrap or bounce).
//! - **Envelope** shapes the time since an event: exponential decay,
//!   attack–hold–decay, a linear fade, a damped "boing" and a spring that
//!   overshoots and settles. It takes ages in seconds, or times its own
//!   trigger per element.
//! - **Beat** is a musical step clock: step, step phase, bar, a gate and a
//!   trigger on every new step, with swing; free-running or locked to the
//!   patch loop.
//!
//! All read the tick's unshifted frame (`tick_frame`), so they are
//! effectful in `nodes.json` (never memoised), and carry their state across
//! patch edits.

use super::geometry::{num_pin, Pins};
use super::math::adaptive::{components_of, from_components, kind_of, Kind};
use super::time::loop_phase;
use super::Registrar;
use crate::node::{CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Spread, Value};
use std::f64::consts::{LN_10, TAU};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("motion/integrate", integrate);
    r.dynamic("motion/integrate", integrate_inputs);
    r.node("motion/envelope", envelope);
    r.dynamic("motion/envelope", envelope_inputs);
    r.node("time/beat", beat);
    r.dynamic("time/beat", beat_inputs);
}

fn param_str<'a>(params: &'a Params, key: &str, fallback: &'a str) -> &'a str {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

/// A string pin bound to a dropdown param of the same name.
fn choice_pin(name: &str, default: &str) -> PortSpec {
    let mut p = PortSpec::new(name, PortType::String).default(Value::str(default));
    p.param = Some(name.into());
    p
}

fn spread_pin(name: &str, ty: PortType, default: Value) -> PortSpec {
    PortSpec::new(name, ty)
        .kind(PortKind::Spread)
        .default(default)
}

/// A dropdown's value: the wired / typed pin, else the param.
fn choice(pins: &Pins, inputs: &[Spread], cx: &EvalCtx, name: &str, fallback: &str) -> String {
    match pins.value(inputs, name) {
        Value::Str(s) => s.to_string(),
        _ => cx
            .param(name)
            .and_then(|v| v.as_str())
            .unwrap_or(fallback)
            .to_owned(),
    }
}

/// Element `i` of a spread, cycling (`Undefined` when empty).
fn at(s: &[Value], i: usize) -> &Value {
    static UNDEF: Value = Value::Undefined;
    if s.is_empty() {
        &UNDEF
    } else {
        &s[i % s.len()]
    }
}

/// A finite number at element `i`, else `fallback`.
fn num_at(s: &[Value], i: usize, fallback: f64) -> f64 {
    match at(s, i) {
        Value::Num(x) if x.is_finite() => *x,
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => fallback,
    }
}

/// The longest of these spreads, or 0 when any is empty (shorter ones
/// cycle).
fn count(spreads: &[&[Value]]) -> usize {
    if spreads.iter().any(|s| s.is_empty()) {
        0
    } else {
        spreads.iter().map(|s| s.len()).max().unwrap_or(0)
    }
}

fn bools(v: impl IntoIterator<Item = bool>) -> Spread {
    v.into_iter().map(Value::Bool).collect::<Vec<_>>().into()
}

fn nums(v: impl IntoIterator<Item = f64>) -> Spread {
    v.into_iter().map(Value::Num).collect::<Vec<_>>().into()
}

// ─── Integrate ───────────────────────────────────────────────────────────────

/// How Integrate keeps its value between `min` and `max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Limit {
    None,
    Clamp,
    Wrap,
    /// Ping-pong: reflect off the ends and run back.
    Bounce,
}

impl Limit {
    fn parse(s: &str) -> Limit {
        match s {
            "clamp" => Limit::Clamp,
            "wrap" => Limit::Wrap,
            "bounce" => Limit::Bounce,
            _ => Limit::None,
        }
    }
}

const MAX_DT: f64 = 0.1;

/// `rate`, `start`, `min`, `max` adapt to number / vec2 / colour
/// (`valueType`); `min` / `max` only while `limit` isn't `none`.
fn integrate_inputs(params: &Params) -> Vec<PortSpec> {
    let ty = super::math::adaptive::port_type_for_value_type(params);
    let kind = super::math::adaptive::kind_for_value_type(params);
    let val = |x: f64| super::math::adaptive::identity_value(kind, x);
    let mut pins = vec![
        spread_pin("rate", ty, val(1.0)),
        spread_pin("start", ty, val(0.0)),
        spread_pin("run", PortType::Boolean, Value::Bool(true)),
        spread_pin("reset", PortType::Boolean, Value::Bool(false)),
        choice_pin("limit", "none"),
    ];
    if Limit::parse(param_str(params, "limit", "none")) != Limit::None {
        pins.push(spread_pin("min", ty, val(0.0)));
        pins.push(spread_pin("max", ty, val(1.0)));
    }
    pins.push(num_pin("maxDt", MAX_DT).label("max dt"));
    pins
}

/// Per-element state: value components and, for `bounce`, the direction
/// each component runs (±1).
#[derive(Default)]
struct IntegrateState {
    value: Vec<[f64; 4]>,
    dir: Vec<[f64; 4]>,
}

struct Integrate {
    pins: Pins,
    state: IntegrateState,
}

fn integrate(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Integrate {
        pins: Pins::new(init),
        state: IntegrateState::default(),
    })
}

fn comps(v: &Value, kind: Kind) -> [f64; 4] {
    let mut out = [0.0; 4];
    for (o, c) in out.iter_mut().zip(components_of(v, kind)) {
        *o = if c.is_finite() { c } else { 0.0 };
    }
    out
}

/// Keep `v` (running in direction `dir`) within `[lo, hi]`; true when it
/// wrapped, bounced or arrived at a clamped end.
fn limit(v: &mut f64, dir: &mut f64, lo: f64, hi: f64, mode: Limit, prev: f64) -> bool {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    if !(lo.is_finite() && hi.is_finite()) || mode == Limit::None {
        return false;
    }
    let w = hi - lo;
    if w == 0.0 {
        let hit = *v != lo;
        *v = lo;
        return hit && prev != lo;
    }
    match mode {
        Limit::None => false,
        Limit::Clamp => {
            let c = v.clamp(lo, hi);
            let hit = c != *v && prev != c;
            *v = c;
            hit
        }
        Limit::Wrap => {
            if *v >= lo && *v < hi {
                return false;
            }
            *v = lo + (*v - lo).rem_euclid(w);
            // `rem_euclid` can round up to `w` for a tiny negative input.
            if *v >= hi {
                *v = lo;
            }
            true
        }
        Limit::Bounce => {
            if *v >= lo && *v <= hi {
                return false;
            }
            // Fold into a triangle wave of period 2w; an odd number of
            // reflections turns the direction round.
            let u = (*v - lo).rem_euclid(2.0 * w);
            let reflections = ((*v - lo) / w).floor().abs() as i64;
            *v = if u > w { lo + 2.0 * w - u } else { lo + u };
            if reflections % 2 == 1 {
                *dir = -*dir;
            }
            true
        }
    }
}

impl NodeImpl for Integrate {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let p = &self.pins;
        let rate = p.spread(inputs, "rate");
        let start = p.spread(inputs, "start");
        let run = p.spread(inputs, "run");
        let reset = p.spread(inputs, "reset");
        let mode = Limit::parse(&choice(p, inputs, cx, "limit", "none"));
        let (lo, hi) = if mode == Limit::None {
            (&[][..], &[][..])
        } else {
            (p.spread(inputs, "min"), p.spread(inputs, "max"))
        };
        let max_dt = p.num(inputs, "maxDt", MAX_DT);
        let mut dt = cx.tick_frame.dt.max(0.0);
        if !dt.is_finite() {
            dt = 0.0;
        }
        if max_dt > 0.0 {
            dt = dt.min(max_dt);
        }
        let kind = match cx.param("valueType").and_then(|v| v.as_str()) {
            Some("vec2") => Kind::Vec2,
            Some("color") => Kind::Color,
            Some("number") => Kind::Number,
            _ => [rate.first(), start.first()]
                .into_iter()
                .flatten()
                .find_map(|v| kind_of(v).filter(|k| *k != Kind::Number))
                .unwrap_or(Kind::Number),
        };
        let nc = match kind {
            Kind::Number => 1,
            Kind::Vec2 => 2,
            Kind::Color => 4,
        };
        let mut sources: Vec<&[Value]> = vec![rate, start, run, reset];
        if mode != Limit::None {
            sources.push(lo);
            sources.push(hi);
        }
        let n = count(&sources);
        let st = &mut self.state;
        let old = st.value.len().min(n);
        st.value.truncate(n);
        st.dir.truncate(n);
        for i in old..n {
            st.value.push(comps(at(start, i), kind));
            st.dir.push([1.0; 4]);
        }
        let mut hits = Vec::with_capacity(n);
        for i in 0..n {
            if at(reset, i).truthy() {
                st.value[i] = comps(at(start, i), kind);
                st.dir[i] = [1.0; 4];
                hits.push(false);
                continue;
            }
            let prev = st.value[i];
            if at(run, i).truthy() {
                let r = comps(at(rate, i), kind);
                let (val, dir) = (&mut st.value[i], &st.dir[i]);
                for ((v, r), d) in val.iter_mut().zip(r).zip(dir).take(nc) {
                    *v += r * dt * d;
                }
            }
            let mut hit = false;
            if mode != Limit::None {
                let (a, b) = (comps(at(lo, i), kind), comps(at(hi, i), kind));
                for c in 0..nc {
                    hit |= limit(
                        &mut st.value[i][c],
                        &mut st.dir[i][c],
                        a[c],
                        b[c],
                        mode,
                        prev[c],
                    );
                }
            }
            hits.push(hit);
        }
        let values: Vec<Value> = st
            .value
            .iter()
            .map(|v| from_components(&v[..nc], kind))
            .collect();
        Ok(vec![Arc::from(values), bools(hits)])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(s) = state.downcast::<IntegrateState>() {
            self.state = *s;
        }
    }
}

// ─── Envelope ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// 1 at the event, falling exponentially (to 1 % at `time`).
    Decay,
    /// A quick ease-out rise over `attack`, `hold` at 1, then `decay`.
    AttackDecay,
    /// Linear ramps: up over `attack`, `hold`, down over `time`.
    Fade,
    /// A damped sine: starts at 0, wobbles about 0 at `frequency`, dies
    /// away over `time`.
    Boing,
    /// A spring's step response: 0 → 1 with overshoot at `frequency`,
    /// settling over `time`.
    Spring,
}

impl Shape {
    fn parse(s: &str) -> Shape {
        match s {
            "attack-decay" => Shape::AttackDecay,
            "fade" => Shape::Fade,
            "boing" => Shape::Boing,
            "spring" => Shape::Spring,
            _ => Shape::Decay,
        }
    }

    fn of(params: &Params) -> Shape {
        Shape::parse(param_str(params, "shape", "decay"))
    }
}

/// Inputs: `from` (age or trigger) and its pin, then the shape's timings.
fn envelope_inputs(params: &Params) -> Vec<PortSpec> {
    let mut pins = vec![choice_pin("from", "age")];
    if param_str(params, "from", "age") == "trigger" {
        pins.push(spread_pin("trigger", PortType::Boolean, Value::Bool(false)));
    } else {
        pins.push(spread_pin("age", PortType::Number, Value::Num(0.0)));
    }
    let num = |name: &str, d: f64| spread_pin(name, PortType::Number, Value::Num(d));
    match Shape::of(params) {
        Shape::Decay => pins.push(num("time", 0.5)),
        Shape::AttackDecay => {
            pins.extend([num("attack", 0.05), num("hold", 0.0), num("time", 0.5)]);
        }
        Shape::Fade => pins.extend([num("attack", 0.0), num("hold", 0.0), num("time", 1.0)]),
        Shape::Boing => pins.extend([num("frequency", 4.0), num("time", 1.0)]),
        Shape::Spring => pins.extend([num("frequency", 2.0), num("time", 1.0)]),
    }
    pins
}

/// The decay rate that brings `e^(−k·t)` to 1 % at `t = time`.
fn decay_rate(time: f64) -> f64 {
    2.0 * LN_10 / time.max(1e-6)
}

/// Timings for one element.
#[derive(Clone, Copy, Debug)]
struct Timing {
    attack: f64,
    hold: f64,
    time: f64,
    frequency: f64,
}

/// The envelope at `age` seconds after the event, and whether it is still
/// moving (`0 ≤ age <` its length). Before the event (or a NaN age) it
/// rests: 0 for every shape.
fn shape_at(shape: Shape, age: f64, tm: Timing) -> (f64, bool) {
    if age.is_nan() || age < 0.0 {
        return (0.0, false);
    }
    let (attack, hold, time) = (tm.attack.max(0.0), tm.hold.max(0.0), tm.time.max(0.0));
    let k = decay_rate(time);
    let w = TAU * tm.frequency;
    match shape {
        Shape::Decay => ((-k * age).exp(), age < time),
        Shape::AttackDecay | Shape::Fade => {
            let len = attack + hold + time;
            let v = if age < attack {
                let x = age / attack;
                if shape == Shape::Fade {
                    x
                } else {
                    1.0 - (1.0 - x) * (1.0 - x)
                }
            } else if age < attack + hold {
                1.0
            } else {
                let a = age - attack - hold;
                if shape == Shape::Fade {
                    if time > 0.0 {
                        (1.0 - a / time).max(0.0)
                    } else {
                        0.0
                    }
                } else {
                    (-k * a).exp()
                }
            };
            (v, age < len)
        }
        Shape::Boing => {
            if age.is_infinite() {
                return (0.0, false);
            }
            ((-k * age).exp() * (w * age).sin(), age < time)
        }
        Shape::Spring => {
            if age.is_infinite() {
                return (1.0, false);
            }
            // Under-damped step response, starting at rest.
            let e = (-k * age).exp();
            let v = if w > 0.0 {
                1.0 - e * ((w * age).cos() + k / w * (w * age).sin())
            } else {
                1.0 - e * (1.0 + k * age)
            };
            (v, age < time)
        }
    }
}

/// Trigger times per element (NaN = not triggered yet) and last tick's
/// trigger levels.
#[derive(Default)]
struct EnvelopeState {
    fired: Vec<f64>,
    prev: Vec<bool>,
}

struct Envelope {
    pins: Pins,
    shape: Shape,
    trigger: bool,
    state: EnvelopeState,
}

fn envelope(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Envelope {
        pins: Pins::new(init),
        shape: Shape::of(init.params),
        trigger: param_str(init.params, "from", "age") == "trigger",
        state: EnvelopeState::default(),
    })
}

impl NodeImpl for Envelope {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let p = &self.pins;
        let attack = p.spread(inputs, "attack");
        let hold = p.spread(inputs, "hold");
        let time = p.spread(inputs, "time");
        let freq = p.spread(inputs, "frequency");
        let source = p.spread(inputs, if self.trigger { "trigger" } else { "age" });
        let mut sources = vec![source, time];
        for s in [attack, hold, freq] {
            if !s.is_empty() {
                sources.push(s);
            }
        }
        let n = count(&sources);
        let now = cx.tick_frame.t;
        let ages: Vec<f64> = if self.trigger {
            let st = &mut self.state;
            st.fired.resize(n, f64::NAN);
            st.prev.resize(n, false);
            (0..n)
                .map(|i| {
                    let on = at(source, i).truthy();
                    if on && !st.prev[i] {
                        st.fired[i] = now;
                    }
                    st.prev[i] = on;
                    if st.fired[i].is_nan() {
                        f64::INFINITY
                    } else {
                        (now - st.fired[i]).max(0.0)
                    }
                })
                .collect()
        } else {
            (0..n)
                .map(|i| match at(source, i) {
                    Value::Num(x) => *x,
                    v => v.to_number(),
                })
                .collect()
        };
        let mut values = Vec::with_capacity(n);
        let mut active = Vec::with_capacity(n);
        for (i, age) in ages.iter().enumerate() {
            let tm = Timing {
                attack: num_at(attack, i, 0.0),
                hold: num_at(hold, i, 0.0),
                time: num_at(time, i, 0.5),
                frequency: num_at(freq, i, 0.0),
            };
            let (v, a) = shape_at(self.shape, *age, tm);
            values.push(v);
            active.push(a);
        }
        Ok(vec![nums(values), nums(ages), bools(active)])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(s) = state.downcast::<EnvelopeState>() {
            self.state = *s;
        }
    }
}

// ─── Beat ────────────────────────────────────────────────────────────────────

fn beat_inputs(_: &Params) -> Vec<PortSpec> {
    let int = |name: &str, d: f64| {
        let mut p = num_pin(name, d);
        p.integer = true;
        p
    };
    vec![
        num_pin("bpm", 120.0),
        int("steps", 16.0),
        int("beats", 4.0),
        num_pin("swing", 0.0),
        num_pin("width", 0.5).label("gate"),
        PortSpec::new("run", PortType::Boolean).default(Value::Bool(true)),
        PortSpec::new("reset", PortType::Boolean).default(Value::Bool(false)),
        choice_pin("sync", "free"),
    ]
}

/// The clock's position in (unswung) steps since it started.
#[derive(Default)]
struct BeatState {
    pos: Option<f64>,
    last: Option<f64>,
    was_reset: bool,
}

struct Beat {
    pins: Pins,
    state: BeatState,
}

fn beat(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Beat {
        pins: Pins::new(init),
        state: BeatState::default(),
    })
}

/// A step position a hair (1e-9 of a step, relative past one) short of or
/// past a whole step, moved onto it. Frame times summed up drift by
/// rounding, so a step due on a frame (6 steps a second at 60 fps starts
/// one every 10 frames) would otherwise now and then start a frame late;
/// `loop_phase` snaps loop boundaries the same way.
fn snap_to_step(pos: f64) -> f64 {
    let whole = pos.round();
    if (pos - whole).abs() < 1e-9 * whole.abs().max(1.0) {
        whole
    } else {
        pos
    }
}

/// Swing a step position: within each pair of steps the first runs
/// `1 + s` long and the second `1 − s` (`s = swing / 2`, `swing` 0..1).
/// Returns (whole step number, phase within it).
fn swung(pos: f64, swing: f64) -> (f64, f64) {
    let s = swing.clamp(0.0, 1.0) * 0.5;
    let pair = (pos / 2.0).floor();
    let u = pos - pair * 2.0;
    if u < 1.0 + s {
        (pair * 2.0, u / (1.0 + s))
    } else {
        (pair * 2.0 + 1.0, (u - 1.0 - s) / (1.0 - s))
    }
}

impl NodeImpl for Beat {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let p = &self.pins;
        let bpm = p.num(inputs, "bpm", 120.0).max(0.0);
        let steps = p.num(inputs, "steps", 16.0).round().clamp(1.0, 1024.0);
        let beats = p.num(inputs, "beats", 4.0).max(0.0);
        let swing = p.num(inputs, "swing", 0.0);
        let width = p.num(inputs, "width", 0.5);
        let run = p.bool(inputs, "run", true);
        let reset = p.bool(inputs, "reset", false);
        let frame = cx.tick_frame;
        // Seconds per bar and steps per second.
        let bar_len = if bpm > 0.0 { beats * 60.0 / bpm } else { 0.0 };
        let rate = if bar_len > 0.0 { steps / bar_len } else { 0.0 };
        let st = &mut self.state;
        let pos = if choice(p, inputs, cx, "sync", "free") == "loop" && frame.loop_length > 0.0 {
            // A whole number of bars fills the loop: the tempo bends a
            // little so a recording of one loop repeats seamlessly.
            let bars = if bar_len > 0.0 {
                (frame.loop_length / bar_len).round().max(1.0)
            } else {
                1.0
            };
            loop_phase(frame.t, frame.loop_length) * bars * steps
        } else {
            let pos = match st.pos {
                _ if reset && !st.was_reset => 0.0,
                Some(x) if run => x + frame.dt.max(0.0) * rate,
                Some(x) => x,
                None => frame.t.max(0.0) * rate,
            };
            if pos.is_finite() {
                pos
            } else {
                0.0
            }
        };
        let pos = snap_to_step(pos);
        st.pos = Some(pos);
        let restarted = reset && !st.was_reset;
        st.was_reset = reset;
        let (count, phase) = swung(pos, swing);
        let trigger = restarted || st.last != Some(count);
        st.last = Some(count);
        let step = count.rem_euclid(steps);
        let bar = (count / steps).floor();
        let gate = phase < width.clamp(0.0, 1.0);
        Ok(vec![
            one(Value::Num(step)),
            one(Value::Num(phase)),
            one(Value::Num(bar)),
            one(Value::Bool(gate)),
            one(Value::Bool(trigger)),
            one(Value::Num(count)),
        ])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(s) = state.downcast::<BeatState>() {
            self.state = *s;
        }
    }
}

#[cfg(test)]
mod tests;
