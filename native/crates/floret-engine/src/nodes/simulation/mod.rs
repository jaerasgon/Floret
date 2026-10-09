//! `particles/simulation`: one stateful node, four classic simulations.
//!
//! A native-only node (no TypeScript original). The `mode` param picks the
//! simulation and the input pins follow it (see [`inputs`]):
//!
//! - `flocking`: Reynolds boids (separation, alignment, cohesion) on a
//!   wrapping field ([`flock`]).
//! - `physarum`: slime-mould agents laying and following a trail map, which
//!   comes out as an image ([`physarum`]).
//! - `dla`: diffusion-limited aggregation, random walkers sticking to a
//!   growing cluster ([`dla`]).
//! - `cloth`: a Verlet spring mesh with pins, wind, tearing and a grab
//!   handle ([`cloth`]).
//! - `growth`: differential growth, a curve whose nodes repel and split
//!   into coral-like folds ([`growth`]).
//! - `reaction`: Gray–Scott reaction–diffusion on a grid, as an image
//!   ([`reaction`]).
//!
//! Time: the node steps in fixed ticks of [`STEP`] seconds, as many as the
//! frame's `dt` covers (at most [`MAX_STEPS`] a frame, so a slow frame
//! drops time instead of snowballing). The same seed and the same frame
//! times give the same picture. State lives in the node; a rising edge on
//! `reset` (or a new seed / size / count where that reshapes the
//! simulation) starts it over. The node is effectful (never memoised).
//! Growth and reaction–diffusion also survive edits of the patch (see
//! [`NodeImpl::carry`]): they take a while to develop.
//!
//! Outputs, in order: `layer` (the simulation drawn, ready for a renderer),
//! `points` (positions), `values` (one number per point: heading, growth
//! order or strain), `from` / `to` (line segments: velocity, branch or
//! spring) and `image` (the physarum trail map or the reaction–diffusion
//! picture; a 1×1 placeholder in the other modes).

mod cloth;
mod dla;
mod flock;
pub(crate) mod growth;
mod physarum;
pub(crate) mod reaction;
#[cfg(test)]
mod tests;

use super::shared::{first, to_color, to_vec2};
use super::style::style_arg;
use super::Registrar;
use crate::image::Image;
use crate::node::{CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::scene::{self, Paint, ResolvedStyle, SceneNode};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{empty, one, Color, Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("particles/simulation", simulation);
    r.dynamic("particles/simulation", sim_inputs);
}

/// One simulation step, in seconds.
pub(crate) const STEP: f64 = 1.0 / 60.0;
/// Most steps one frame may run.
const MAX_STEPS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Flocking,
    Physarum,
    Dla,
    Cloth,
    Growth,
    Reaction,
}

impl Mode {
    fn of(params: &Params) -> Mode {
        match params.get("mode").and_then(|v| v.as_str()) {
            Some("physarum") => Mode::Physarum,
            Some("dla") => Mode::Dla,
            Some("cloth") => Mode::Cloth,
            Some("growth") => Mode::Growth,
            Some("reaction") => Mode::Reaction,
            _ => Mode::Flocking,
        }
    }
}

// ─── Pins ────────────────────────────────────────────────────────────────────

fn num(name: &str, d: f64) -> PortSpec {
    PortSpec::new(name, PortType::Number).default(Value::Num(d))
}

fn int(name: &str, d: f64) -> PortSpec {
    let mut p = num(name, d);
    p.integer = true;
    p
}

fn vec2(name: &str, x: f64, y: f64) -> PortSpec {
    PortSpec::new(name, PortType::Vec2).default(Value::Vec2(Vec2 { x, y }))
}

fn spread(name: &str, ty: PortType) -> PortSpec {
    PortSpec::new(name, ty).kind(PortKind::Spread)
}

fn style() -> PortSpec {
    PortSpec::new("style", PortType::Style).default(Value::Null)
}

fn reset() -> PortSpec {
    PortSpec::new("reset", PortType::Boolean).default(Value::Bool(false))
}

/// The pins for the node's `mode` (`DynamicInputsFn`).
fn sim_inputs(params: &Params) -> Vec<PortSpec> {
    match Mode::of(params) {
        Mode::Flocking => vec![
            int("count", 250.0),
            vec2("size", 600.0, 600.0),
            num("maxSpeed", 140.0),
            num("separation", 1.6),
            num("alignment", 1.0),
            num("cohesion", 0.9),
            num("radius", 40.0),
            vec2("target", 0.0, 0.0),
            num("attract", 0.0),
            num("scale", 7.0),
            style(),
            spread("colors", PortType::Color),
            int("seed", 1.0),
            reset(),
        ],
        Mode::Physarum => vec![
            int("agents", 20000.0),
            vec2("size", 600.0, 600.0),
            int("resolution", 256.0),
            num("sensorAngle", 22.5),
            num("sensorDistance", 9.0),
            num("turnAngle", 45.0),
            num("stepSize", 1.0),
            num("deposit", 0.3),
            num("decay", 0.08),
            num("diffuse", 0.5),
            spread("food", PortType::Vec2),
            spread("colors", PortType::Color),
            int("seed", 1.0),
            reset(),
        ],
        Mode::Dla => vec![
            int("count", 3000.0),
            vec2("size", 600.0, 600.0),
            num("radius", 2.5),
            num("rate", 300.0),
            num("stickiness", 1.0),
            spread("seeds", PortType::Vec2),
            style(),
            spread("colors", PortType::Color),
            int("seed", 1.0),
            reset(),
        ],
        Mode::Growth => vec![
            PortSpec::new("shape", PortType::Layer).kind(PortKind::Spread),
            num("radius", 40.0),
            vec2("size", 600.0, 600.0),
            int("maxNodes", 2000.0),
            num("repel", 10.0),
            num("force", 1.0),
            num("align", 0.5),
            num("maxEdge", 5.0),
            num("grow", 40.0),
            num("speed", 1.0),
            style(),
            spread("colors", PortType::Color),
            int("seed", 1.0),
            reset(),
        ],
        Mode::Reaction => {
            let mut preset = PortSpec::new("preset", PortType::String).default(Value::str("coral"));
            preset.param = Some("preset".into());
            let custom = params.get("preset").and_then(|v| v.as_str()) == Some("custom");
            let mut v = vec![vec2("size", 600.0, 600.0), int("resolution", 200.0), preset];
            if custom {
                v.push(num("feed", 0.055));
                v.push(num("kill", 0.062));
            }
            v.extend([
                num("rateA", 1.0),
                num("rateB", 0.5),
                int("speed", 8.0),
                PortSpec::new("seedImage", PortType::Image)
                    .default(Value::Null)
                    .label("seed image"),
                spread("paint", PortType::Vec2),
                spread("colors", PortType::Color),
                int("seed", 1.0),
                reset(),
            ]);
            v
        }
        Mode::Cloth => {
            let mut pin = PortSpec::new("pin", PortType::String).default(Value::str("top"));
            pin.param = Some("pin".into());
            vec![
                int("cols", 28.0),
                int("rows", 18.0),
                vec2("size", 420.0, 270.0),
                vec2("pos", 0.0, 0.0),
                pin,
                vec2("gravity", 0.0, 600.0),
                vec2("wind", 0.0, 0.0),
                num("stiffness", 1.0),
                int("iterations", 8.0),
                num("damping", 0.01),
                num("tear", 0.0),
                vec2("pointer", 0.0, 0.0),
                PortSpec::new("grab", PortType::Boolean).default(Value::Bool(false)),
                style(),
                spread("colors", PortType::Color),
                reset(),
            ]
        }
    }
}

/// The node's inputs, read by pin name (the pins depend on the mode).
pub(crate) struct Ins<'a> {
    inputs: &'a [Spread],
    names: &'a [String],
}

impl Ins<'_> {
    fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    fn spread(&self, name: &str) -> Spread {
        self.index(name)
            .and_then(|i| self.inputs.get(i).cloned())
            .unwrap_or_else(empty)
    }

    fn value(&self, name: &str) -> Value {
        self.index(name)
            .map_or(Value::Undefined, |i| first(self.inputs, i))
    }

    /// A finite number, else `d`.
    pub(crate) fn num(&self, name: &str, d: f64) -> f64 {
        let v = self.value(name).to_number();
        if v.is_finite() {
            v
        } else {
            d
        }
    }

    /// A whole number in `lo..=hi`, else `d`.
    pub(crate) fn count(&self, name: &str, d: usize, lo: usize, hi: usize) -> usize {
        (self.num(name, d as f64).round().max(0.0) as usize).clamp(lo, hi)
    }

    pub(crate) fn vec2(&self, name: &str, d: Vec2) -> Vec2 {
        match self.value(name) {
            Value::Undefined | Value::Null => d,
            v => {
                let p = to_vec2(&v);
                if p.x.is_finite() && p.y.is_finite() {
                    p
                } else {
                    d
                }
            }
        }
    }

    /// A `size` pin: both sides at least 1.
    pub(crate) fn size(&self, d: Vec2) -> Vec2 {
        let s = self.vec2("size", d);
        Vec2 {
            x: s.x.abs().max(1.0),
            y: s.y.abs().max(1.0),
        }
    }

    pub(crate) fn flag(&self, name: &str) -> bool {
        self.value(name).truthy()
    }

    pub(crate) fn str(&self, name: &str) -> String {
        match self.value(name) {
            Value::Str(s) => s.to_string(),
            _ => String::new(),
        }
    }

    pub(crate) fn points(&self, name: &str) -> Vec<Vec2> {
        self.spread(name)
            .iter()
            .filter(|v| !v.is_nullish())
            .map(to_vec2)
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect()
    }

    pub(crate) fn colors(&self) -> Vec<Color> {
        self.spread("colors")
            .iter()
            .filter(|v| !v.is_nullish())
            .map(to_color)
            .collect()
    }

    pub(crate) fn seed(&self) -> u64 {
        crate::seed::mix(self.num("seed", 1.0)) as i64 as u64
    }

    pub(crate) fn style(&self) -> Option<crate::scene::Style> {
        style_arg(&self.value("style")).cloned()
    }
}

// ─── Randomness ──────────────────────────────────────────────────────────────

/// Every random stream a simulation uses starts here, from the node's
/// `seed` pin with the patch seed already mixed in (see `seed()` above).
pub(crate) fn sim_rng(seed: u64) -> Rng {
    Rng::new(seed)
}

/// SplitMix64: small, fast and the same everywhere.
pub(crate) struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed ^ 0x5851_f42d_4c95_7f2d);
        r.next_u64();
        r
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub(crate) fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [0, 1), single precision.
    pub(crate) fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }
}

// ─── Drawing helpers ─────────────────────────────────────────────────────────

/// A colour on a 0..1 ramp through `stops` (evenly spaced).
pub(crate) fn ramp(stops: &[Color], t: f64) -> Color {
    match stops {
        [] => Color::WHITE,
        [c] => *c,
        _ => {
            let x = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
            let i = (x.floor() as usize).min(stops.len() - 2);
            let f = x - i as f64;
            let (a, b) = (stops[i], stops[i + 1]);
            Color {
                r: a.r + (b.r - a.r) * f,
                g: a.g + (b.g - a.g) * f,
                b: a.b + (b.b - a.b) * f,
                a: a.a + (b.a - a.a) * f,
            }
        }
    }
}

/// Shape styles: the wired `style` (or the mode's look when unwired), and
/// with `colors` wired, that ramp in [`LEVELS`] shared steps for the fill
/// (`fill: true`) or the stroke.
pub(crate) struct Styles {
    base: Arc<ResolvedStyle>,
    levels: Vec<Arc<ResolvedStyle>>,
}

const LEVELS: usize = 64;

impl Styles {
    pub(crate) fn new(ins: &Ins, fill: bool) -> Styles {
        let fallback = if fill {
            scene::default_style().clone()
        } else {
            Arc::new(ResolvedStyle {
                fill: None,
                stroke: Some(Paint::css("#ffffff")),
                ..(**scene::default_style()).clone()
            })
        };
        let base = scene::resolve_style(ins.style().as_ref(), &fallback);
        let stops = ins.colors();
        let levels = if stops.is_empty() {
            Vec::new()
        } else {
            (0..LEVELS)
                .map(|i| {
                    let paint = Some(Paint::from_color(ramp(
                        &stops,
                        i as f64 / (LEVELS - 1) as f64,
                    )));
                    let mut st = (*base).clone();
                    if fill {
                        st.fill = paint;
                    } else {
                        st.stroke = paint;
                    }
                    Arc::new(st)
                })
                .collect()
        };
        Styles { base, levels }
    }

    /// The style without the colour ramp.
    pub(crate) fn base(&self) -> &Arc<ResolvedStyle> {
        &self.base
    }

    /// The style for a shape at `t` (0..1) on the colour ramp.
    pub(crate) fn at(&self, t: f64) -> Arc<ResolvedStyle> {
        if self.levels.is_empty() {
            return self.base.clone();
        }
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.levels[(t * (LEVELS - 1) as f64).round() as usize].clone()
    }
}

pub(crate) fn line(a: Vec2, b: Vec2, style: Arc<ResolvedStyle>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Line {
        x1: a.x,
        y1: a.y,
        x2: b.x,
        y2: b.y,
        style,
    })
}

// ─── The node ────────────────────────────────────────────────────────────────

/// What a mode hands back each frame.
#[derive(Default)]
pub(crate) struct Out {
    pub layer: Vec<Arc<SceneNode>>,
    pub points: Vec<Vec2>,
    pub values: Vec<f64>,
    pub segments: Vec<(Vec2, Vec2)>,
    pub image: Option<Arc<Image>>,
}

/// One running simulation.
pub(crate) trait Sim: Send {
    /// Whether the inputs reshape the simulation (it then starts over).
    fn stale(&self, ins: &Ins) -> bool;
    /// Advance one [`STEP`].
    fn step(&mut self, ins: &Ins);
    fn output(&self, ins: &Ins) -> Out;
}

fn start(mode: Mode, ins: &Ins) -> Box<dyn Sim> {
    match mode {
        Mode::Flocking => Box::new(flock::Flock::new(ins)),
        Mode::Physarum => Box::new(physarum::Physarum::new(ins)),
        Mode::Dla => Box::new(dla::Dla::new(ins)),
        Mode::Cloth => Box::new(cloth::Cloth::new(ins)),
        Mode::Growth => Box::new(growth::Growth::new(ins)),
        Mode::Reaction => Box::new(reaction::Reaction::new(ins)),
    }
}

/// The node: the running simulation and its clock.
struct Simulation {
    mode: Mode,
    names: Vec<String>,
    sim: Option<Box<dyn Sim>>,
    was_reset: bool,
    /// Unstepped time carried to the next frame.
    carry: f64,
}

/// What a growth or reaction–diffusion simulation hands to the node
/// rebuilt by an edit, so a long-grown picture survives a tweak.
struct Carried {
    mode: Mode,
    sim: Box<dyn Sim>,
    was_reset: bool,
    carry: f64,
}

fn simulation(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Simulation {
        mode: Mode::of(init.params),
        names: init.inputs.iter().map(|p| p.name.clone()).collect(),
        sim: None,
        was_reset: false,
        carry: 0.0,
    })
}

impl NodeImpl for Simulation {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let ins = Ins {
            inputs,
            names: &self.names,
        };
        let reset = ins.flag("reset");
        if (reset && !self.was_reset) || self.sim.as_ref().is_some_and(|s| s.stale(&ins)) {
            self.sim = None;
        }
        self.was_reset = reset;
        let s = match &mut self.sim {
            Some(s) => {
                let dt = cx.tick_frame.dt;
                self.carry += if dt.is_finite() {
                    dt.clamp(0.0, 0.25)
                } else {
                    0.0
                };
                let mut steps = 0;
                while self.carry >= STEP * 0.999 && steps < MAX_STEPS {
                    s.step(&ins);
                    self.carry -= STEP;
                    steps += 1;
                }
                self.carry = self.carry.clamp(0.0, STEP);
                s
            }
            None => {
                self.carry = 0.0;
                self.sim.insert(start(self.mode, &ins))
            }
        };
        let out = s.output(&ins);
        let (from, to): (Vec<Value>, Vec<Value>) = out
            .segments
            .iter()
            .map(|(a, b)| (Value::Vec2(*a), Value::Vec2(*b)))
            .unzip();
        Ok(vec![
            one(Value::Scene(Arc::new(SceneNode::group(out.layer)))),
            out.points.into_iter().map(Value::Vec2).collect(),
            out.values.into_iter().map(Value::Num).collect(),
            Arc::from(from),
            Arc::from(to),
            one(Value::Image(out.image.unwrap_or_else(Image::placeholder))),
        ])
    }

    /// Growth and reaction–diffusion take minutes to develop, so they
    /// survive edits (the older modes start over, as they always have).
    fn carry(&mut self) -> Option<CarriedState> {
        if !matches!(self.mode, Mode::Growth | Mode::Reaction) {
            return None;
        }
        let sim = self.sim.take()?;
        Some(Box::new(Carried {
            mode: self.mode,
            sim,
            was_reset: self.was_reset,
            carry: self.carry,
        }))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(c) = state.downcast::<Carried>() {
            if c.mode == self.mode {
                self.sim = Some(c.sim);
                self.was_reset = c.was_reset;
                self.carry = c.carry;
            }
        }
    }
}
