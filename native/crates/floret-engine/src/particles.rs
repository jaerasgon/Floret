//! Particle runtime — a port of `src/runtime/particles.ts`.
//!
//! Struct-of-Arrays [`ParticleBuffer`] with a free-slot list, the force /
//! collider contracts, the two integrators, aging and the shared falloff
//! curves. The `particles/*`, `force/*`, `collider/*` and `physics/*` nodes
//! (in `nodes/`) are built on top of this module.
//!
//! # Values on `world` / `force` / `collider` / `particle` wires
//!
//! TS passes opaque JS objects on these wires; here they are
//! [`Value::Handle`] carrying a typed [`Handle`]:
//!
//! - `world`    → [`Handle::World`]: plain data ([`World`]), immutable.
//! - `force`    → [`Handle::Force`]: data + an `apply` function ([`Force`]).
//!   A TS force is a closure; turbulence / wind keep a private clock in it,
//!   so [`Force`] carries that clock as interior state. A force value is
//!   reused across ticks while its node's inputs are unchanged (the TS
//!   memo), so the clock keeps running exactly like the TS closure.
//! - `collider` → [`Handle::Collider`]: data + `apply` ([`Collider`]).
//! - `particle` → [`Handle::Particles`]: a shared, mutable buffer
//!   ([`ParticleHandle`] = `Arc<RwLock<ParticleBuffer>>`). Like TS, the
//!   emitter owns one buffer for its lifetime and mutates it in place each
//!   tick; every downstream value (render batches, trails) holds the SAME
//!   buffer, so a batch built on an earlier tick still draws the live
//!   particles. The renderer read-locks it while drawing.
//!
//! # Numerics
//!
//! TS stores particle state in `Float32Array`s and does all maths in f64:
//! a read widens f32 → f64 exactly, a write rounds f64 → f32 to nearest.
//! This port does the same operations in the same order, so the stored
//! f32 bits — and the trace's FNV hashes of them — match exactly.

use crate::nodes::noise::perlin_2d;
use crate::scene::SceneNode;
use crate::value::{Color, Spread, Value};
use indexmap::IndexMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

// ─── JS maths ────────────────────────────────────────────────────────────────

/// V8's two-argument `Math.hypot`: scale by the max, Kahan-sum the squares,
/// `sqrt(sum) * max`. Not always equal to libm's `hypot` in the last bit.
pub fn js_hypot(a: f64, b: f64) -> f64 {
    let (aa, ab) = (a.abs(), b.abs());
    if aa == f64::INFINITY || ab == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let max = if ab > aa { ab } else { aa };
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut comp = 0.0;
    for x in [aa, ab] {
        let n = x / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}

/// JS `a || b` on numbers: `b` when `a` is 0 or NaN.
fn or(a: f64, b: f64) -> f64 {
    if a == 0.0 || a.is_nan() {
        b
    } else {
        a
    }
}

// ─── Falloff curves ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FalloffCurve {
    Constant,
    Linear,
    Smooth,
    Inverse,
    InverseSquare,
}

impl FalloffCurve {
    /// `coerceFalloff` (force/_shared.ts): unknown → constant.
    pub fn coerce(v: &Value) -> FalloffCurve {
        match v {
            Value::Str(s) => match &**s {
                "linear" => FalloffCurve::Linear,
                "smooth" => FalloffCurve::Smooth,
                "inverse" => FalloffCurve::Inverse,
                "inverse-square" => FalloffCurve::InverseSquare,
                _ => FalloffCurve::Constant,
            },
            _ => FalloffCurve::Constant,
        }
    }
}

/// `falloffAt`: strength multiplier at distance ratio `t` (0 = centre,
/// 1 = radius).
pub fn falloff_at(curve: FalloffCurve, t: f64) -> f64 {
    if t <= 0.0 {
        return 1.0;
    }
    if t >= 1.0 {
        return 0.0;
    }
    match curve {
        FalloffCurve::Constant => 1.0,
        FalloffCurve::Linear => 1.0 - t,
        FalloffCurve::Smooth => {
            let s = 1.0 - t;
            s * s * (3.0 - 2.0 * s)
        }
        FalloffCurve::Inverse => 1.0 / (1.0 + 4.0 * t),
        FalloffCurve::InverseSquare => 1.0 / (1.0 + 16.0 * t * t),
    }
}

// ─── World ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Integrator {
    Euler,
    Verlet,
}

impl Integrator {
    pub fn name(self) -> &'static str {
        match self {
            Integrator::Euler => "euler",
            Integrator::Verlet => "verlet",
        }
    }
}

/// `World`: global physics settings plus the global force / collider lists.
/// `forces` / `colliders` are the raw input spreads (TS stores the wire's
/// array itself); entries that aren't forces / colliders make the emitter
/// fail the tick, as calling them throws in TS.
#[derive(Clone, Debug)]
pub struct World {
    pub gravity_x: f64,
    pub gravity_y: f64,
    /// Linear damping rate per second; per-tick factor is `exp(-drag·dt)`.
    pub drag: f64,
    pub time_scale: f64,
    /// `{ w, h }` or `null`. Carried along; nothing reads it.
    pub bounds: Option<(f64, f64)>,
    pub forces: Spread,
    pub colliders: Spread,
    pub integrator: Integrator,
}

impl World {
    /// `DEFAULT_WORLD`.
    pub fn default_world() -> World {
        World {
            gravity_x: 0.0,
            gravity_y: 200.0,
            drag: 0.05,
            time_scale: 1.0,
            bounds: None,
            forces: crate::value::empty(),
            colliders: crate::value::empty(),
            integrator: Integrator::Euler,
        }
    }

    /// The TS object (same keys, in order).
    pub fn to_value(&self) -> Value {
        let mut m: IndexMap<String, Value> = IndexMap::new();
        m.insert("gravityX".into(), Value::Num(self.gravity_x));
        m.insert("gravityY".into(), Value::Num(self.gravity_y));
        m.insert("drag".into(), Value::Num(self.drag));
        m.insert("timeScale".into(), Value::Num(self.time_scale));
        m.insert(
            "bounds".into(),
            match self.bounds {
                None => Value::Null,
                Some((w, h)) => {
                    let mut b = IndexMap::new();
                    b.insert("w".to_string(), Value::Num(w));
                    b.insert("h".to_string(), Value::Num(h));
                    Value::Object(Arc::new(b))
                }
            },
        );
        m.insert("forces".into(), Value::List(self.forces.clone()));
        m.insert("colliders".into(), Value::List(self.colliders.clone()));
        m.insert("integrator".into(), Value::str(self.integrator.name()));
        Value::Object(Arc::new(m))
    }
}

// ─── Forces ──────────────────────────────────────────────────────────────────

/// What a force does, with its constants baked at construction (the TS
/// closure's captured variables).
#[derive(Clone, Debug, PartialEq)]
pub enum ForceKind {
    /// `force/drag`: `f -= v · k`.
    Drag { k: f64 },
    /// `force/gravity`: `f += a · mass`.
    Gravity { ax: f64, ay: f64 },
    /// `force/attractor`: pull toward (cx, cy) within `r`.
    Attractor {
        cx: f64,
        cy: f64,
        strength: f64,
        r: f64,
        inv_r: f64,
        curve: FalloffCurve,
    },
    /// `force/repulsor`: push away from (cx, cy) within `r`.
    Repulsor {
        cx: f64,
        cy: f64,
        strength: f64,
        r: f64,
        inv_r: f64,
        curve: FalloffCurve,
    },
    /// `force/vortex`: tangential `axis` plus inward `pull`.
    Vortex {
        cx: f64,
        cy: f64,
        axis: f64,
        pull: f64,
        r: f64,
        inv_r: f64,
        curve: FalloffCurve,
    },
    /// `force/turbulence`: two Perlin samples per particle; the field drifts
    /// with the force's clock (`t += dt · speed` per application).
    Turbulence {
        scale: f64,
        speed: f64,
        strength: f64,
        seed: f64,
    },
    /// `force/wind`: direction (dx, dy) with Perlin magnitude noise; clock
    /// `t += dt` per application.
    Wind {
        dx: f64,
        dy: f64,
        strength: f64,
        noise_amp: f64,
        seed: f64,
    },
}

/// A force: `(buf, dt, world) => void` mutating `force_x` / `force_y`.
pub struct Force {
    pub kind: ForceKind,
    /// The closure's private clock (turbulence / wind), f64 bits.
    clock: AtomicU64,
}

impl Force {
    pub fn new(kind: ForceKind) -> Force {
        Force {
            kind,
            clock: AtomicU64::new(0f64.to_bits()),
        }
    }

    /// The closure clock's current value.
    pub fn clock(&self) -> f64 {
        f64::from_bits(self.clock.load(Ordering::Relaxed))
    }

    fn advance(&self, by: f64) -> f64 {
        let t = self.clock() + by;
        self.clock.store(t.to_bits(), Ordering::Relaxed);
        t
    }

    /// Apply to every alive particle.
    pub fn apply(&self, buf: &mut ParticleBuffer, dt: f64, _world: &World) {
        let cap = buf.capacity;
        let b = buf;
        match self.kind {
            ForceKind::Drag { k } => {
                for i in 0..cap {
                    if b.alive[i] == 0 {
                        continue;
                    }
                    b.force_x[i] = (f64::from(b.force_x[i]) - f64::from(b.vel_x[i]) * k) as f32;
                    b.force_y[i] = (f64::from(b.force_y[i]) - f64::from(b.vel_y[i]) * k) as f32;
                }
            }
            ForceKind::Gravity { ax, ay } => {
                for i in 0..cap {
                    if b.alive[i] == 0 {
                        continue;
                    }
                    let mi = or(f64::from(b.mass[i]), 1.0);
                    b.force_x[i] = (f64::from(b.force_x[i]) + ax * mi) as f32;
                    b.force_y[i] = (f64::from(b.force_y[i]) + ay * mi) as f32;
                }
            }
            ForceKind::Attractor {
                cx,
                cy,
                strength,
                r,
                inv_r,
                curve,
            }
            | ForceKind::Repulsor {
                cx,
                cy,
                strength,
                r,
                inv_r,
                curve,
            } => {
                let toward = matches!(self.kind, ForceKind::Attractor { .. });
                for i in 0..cap {
                    if b.alive[i] == 0 {
                        continue;
                    }
                    let (px, py) = (f64::from(b.pos_x[i]), f64::from(b.pos_y[i]));
                    let (dx, dy) = if toward {
                        (cx - px, cy - py)
                    } else {
                        (px - cx, py - cy)
                    };
                    let dist = js_hypot(dx, dy);
                    if dist == 0.0 || dist > r {
                        continue;
                    }
                    let t = dist * inv_r;
                    let mag = strength * falloff_at(curve, t);
                    let inv_d = 1.0 / dist;
                    b.force_x[i] = (f64::from(b.force_x[i]) + dx * inv_d * mag) as f32;
                    b.force_y[i] = (f64::from(b.force_y[i]) + dy * inv_d * mag) as f32;
                }
            }
            ForceKind::Vortex {
                cx,
                cy,
                axis,
                pull,
                r,
                inv_r,
                curve,
            } => {
                for i in 0..cap {
                    if b.alive[i] == 0 {
                        continue;
                    }
                    let dx = f64::from(b.pos_x[i]) - cx;
                    let dy = f64::from(b.pos_y[i]) - cy;
                    let dist = js_hypot(dx, dy);
                    if dist == 0.0 || dist > r {
                        continue;
                    }
                    let t = dist * inv_r;
                    let k = falloff_at(curve, t);
                    let inv_d = 1.0 / dist;
                    let (tx, ty) = (-dy * inv_d, dx * inv_d);
                    b.force_x[i] =
                        (f64::from(b.force_x[i]) + tx * axis * k - dx * inv_d * pull * k) as f32;
                    b.force_y[i] =
                        (f64::from(b.force_y[i]) + ty * axis * k - dy * inv_d * pull * k) as f32;
                }
            }
            ForceKind::Turbulence {
                scale,
                speed,
                strength,
                seed,
            } => {
                let t = self.advance(dt * speed);
                for i in 0..cap {
                    if b.alive[i] == 0 {
                        continue;
                    }
                    let x = f64::from(b.pos_x[i]) * scale;
                    let y = f64::from(b.pos_y[i]) * scale;
                    let ax = (perlin_2d(x, y + t, seed) - 0.5) * 2.0;
                    let ay = (perlin_2d(x + 50.0, y + t, seed + 1.0) - 0.5) * 2.0;
                    b.force_x[i] = (f64::from(b.force_x[i]) + ax * strength) as f32;
                    b.force_y[i] = (f64::from(b.force_y[i]) + ay * strength) as f32;
                }
            }
            ForceKind::Wind {
                dx,
                dy,
                strength,
                noise_amp,
                seed,
            } => {
                let t = self.advance(dt);
                for i in 0..cap {
                    if b.alive[i] == 0 {
                        continue;
                    }
                    let n = if noise_amp == 0.0 {
                        0.0
                    } else {
                        (perlin_2d(
                            f64::from(b.pos_x[i]) * 0.01,
                            f64::from(b.pos_y[i]) * 0.01 + t,
                            seed,
                        ) - 0.5)
                            * 2.0
                            * noise_amp
                    };
                    let mag = strength * (1.0 + n);
                    b.force_x[i] = (f64::from(b.force_x[i]) + dx * mag) as f32;
                    b.force_y[i] = (f64::from(b.force_y[i]) + dy * mag) as f32;
                }
            }
        }
    }
}

impl fmt::Debug for Force {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Force({:?}, clock {})", self.kind, self.clock())
    }
}

// ─── Colliders ───────────────────────────────────────────────────────────────

/// `ColliderMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColliderMode {
    Bounce,
    Kill,
    Absorb,
}

impl ColliderMode {
    /// `coerceMode` (collider/_shared.ts): unknown → bounce.
    pub fn coerce(v: &Value) -> ColliderMode {
        match v {
            Value::Str(s) if &**s == "kill" => ColliderMode::Kill,
            Value::Str(s) if &**s == "absorb" => ColliderMode::Absorb,
            _ => ColliderMode::Bounce,
        }
    }
}

/// `particles/bounds` mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsMode {
    Wrap,
    Kill,
    Bounce,
}

/// A polygon collider's precomputed edges (TS keeps them in
/// `Float32Array`s, so they are f32 here too).
#[derive(Clone, Debug, PartialEq)]
pub struct PolygonEdges {
    pub pts: Vec<(f64, f64)>,
    pub edge_x: Vec<f32>,
    pub edge_y: Vec<f32>,
    pub edge_len: Vec<f32>,
    pub norm_x: Vec<f32>,
    pub norm_y: Vec<f32>,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl PolygonEdges {
    pub fn new(pts: Vec<(f64, f64)>) -> PolygonEdges {
        let n = pts.len();
        let mut e = PolygonEdges {
            edge_x: vec![0.0; n],
            edge_y: vec![0.0; n],
            edge_len: vec![0.0; n],
            norm_x: vec![0.0; n],
            norm_y: vec![0.0; n],
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
            pts: Vec::new(),
        };
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let ex = b.0 - a.0;
            let ey = b.1 - a.1;
            let len = or(js_hypot(ex, ey), 1e-9);
            e.edge_x[i] = ex as f32;
            e.edge_y[i] = ey as f32;
            e.edge_len[i] = len as f32;
            e.norm_x[i] = (ey / len) as f32;
            e.norm_y[i] = (-ex / len) as f32;
        }
        for &(x, y) in &pts {
            if x < e.min_x {
                e.min_x = x;
            }
            if y < e.min_y {
                e.min_y = y;
            }
            if x > e.max_x {
                e.max_x = x;
            }
            if y > e.max_y {
                e.max_y = y;
            }
        }
        e.pts = pts;
        e
    }
}

/// What a collider does, with its constants baked at construction.
#[derive(Clone, Debug, PartialEq)]
pub enum ColliderShape {
    /// A collider that does nothing (`collider/from-layer` with no layer or
    /// an unsupported shape).
    None,
    /// `particles/bounds`.
    Bounds {
        mode: BoundsMode,
        rest: f64,
        min_x: f64,
        max_x: f64,
        min_y: f64,
        max_y: f64,
        w: f64,
        h: f64,
    },
    /// `collider/circle` and `collider/from-layer` ellipses.
    Circle {
        mode: ColliderMode,
        rest: f64,
        inverted: bool,
        cx: f64,
        cy: f64,
        r: f64,
    },
    /// `collider/rect` and `collider/from-layer` rects.
    Rect {
        mode: ColliderMode,
        rest: f64,
        inverted: bool,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    },
    /// `collider/from-layer` triangles and polygons.
    Polygon {
        mode: ColliderMode,
        rest: f64,
        inverted: bool,
        edges: Box<PolygonEdges>,
    },
}

/// A collider: `{ apply(buf, dt, world) }`, applied after integration.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    pub shape: ColliderShape,
    /// The TS `apply` function's `.name` ("apply", or "" → "anonymous" for
    /// the closures `collider/from-layer` builds). Only the traces see it.
    pub fn_name: &'static str,
}

impl Collider {
    pub fn new(shape: ColliderShape, fn_name: &'static str) -> Collider {
        Collider { shape, fn_name }
    }

    /// The TS object: `{ apply: <function> }`.
    pub fn to_value(&self) -> Value {
        let mut m = IndexMap::new();
        m.insert(
            "apply".to_string(),
            Value::Handle(Handle::Function(self.fn_name)),
        );
        Value::Object(Arc::new(m))
    }

    pub fn apply(&self, buf: &mut ParticleBuffer, _dt: f64, _world: &World) {
        let cap = buf.capacity;
        match &self.shape {
            ColliderShape::None => {}
            &ColliderShape::Bounds {
                mode,
                rest,
                min_x,
                max_x,
                min_y,
                max_y,
                w,
                h,
            } => {
                for i in 0..cap {
                    if buf.alive[i] == 0 {
                        continue;
                    }
                    let mut x = f64::from(buf.pos_x[i]);
                    let mut y = f64::from(buf.pos_y[i]);
                    let out_x = x < min_x || x > max_x;
                    let out_y = y < min_y || y > max_y;
                    if !out_x && !out_y {
                        continue;
                    }
                    match mode {
                        BoundsMode::Kill => buf.kill(i),
                        BoundsMode::Wrap => {
                            if x < min_x {
                                x += w;
                            } else if x > max_x {
                                x -= w;
                            }
                            if y < min_y {
                                y += h;
                            } else if y > max_y {
                                y -= h;
                            }
                            buf.pos_x[i] = x as f32;
                            buf.pos_y[i] = y as f32;
                        }
                        BoundsMode::Bounce => {
                            if out_x {
                                buf.vel_x[i] = (-(f64::from(buf.vel_x[i]) * rest)) as f32;
                                buf.pos_x[i] =
                                    if x < min_x { min_x + 0.1 } else { max_x - 0.1 } as f32;
                            }
                            if out_y {
                                buf.vel_y[i] = (-(f64::from(buf.vel_y[i]) * rest)) as f32;
                                buf.pos_y[i] =
                                    if y < min_y { min_y + 0.1 } else { max_y - 0.1 } as f32;
                            }
                        }
                    }
                }
            }
            &ColliderShape::Circle {
                mode,
                rest,
                inverted,
                cx,
                cy,
                r,
            } => {
                let r2 = r * r;
                for i in 0..cap {
                    if buf.alive[i] == 0 {
                        continue;
                    }
                    let dx = f64::from(buf.pos_x[i]) - cx;
                    let dy = f64::from(buf.pos_y[i]) - cy;
                    let d2 = dx * dx + dy * dy;
                    let hit = if inverted { d2 > r2 } else { d2 < r2 };
                    if !hit {
                        continue;
                    }
                    match mode {
                        ColliderMode::Kill => {
                            buf.kill(i);
                            continue;
                        }
                        ColliderMode::Absorb => {
                            buf.vel_x[i] = 0.0;
                            buf.vel_y[i] = 0.0;
                            continue;
                        }
                        ColliderMode::Bounce => {}
                    }
                    let dist = or(d2.sqrt(), 1e-6);
                    let nx = (if inverted { -dx } else { dx }) / dist;
                    let ny = (if inverted { -dy } else { dy }) / dist;
                    let v_dot_n = f64::from(buf.vel_x[i]) * nx + f64::from(buf.vel_y[i]) * ny;
                    if v_dot_n < 0.0 {
                        buf.vel_x[i] =
                            ((f64::from(buf.vel_x[i]) - 2.0 * v_dot_n * nx) * rest) as f32;
                        buf.vel_y[i] =
                            ((f64::from(buf.vel_y[i]) - 2.0 * v_dot_n * ny) * rest) as f32;
                    }
                    let overlap = if inverted { dist - r } else { r - dist };
                    buf.pos_x[i] = (f64::from(buf.pos_x[i]) + nx * overlap) as f32;
                    buf.pos_y[i] = (f64::from(buf.pos_y[i]) + ny * overlap) as f32;
                }
            }
            &ColliderShape::Rect {
                mode,
                rest,
                inverted,
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                for i in 0..cap {
                    if buf.alive[i] == 0 {
                        continue;
                    }
                    let x = f64::from(buf.pos_x[i]);
                    let y = f64::from(buf.pos_y[i]);
                    let inside = x > min_x && x < max_x && y > min_y && y < max_y;
                    let hit = if inverted { !inside } else { inside };
                    if !hit {
                        continue;
                    }
                    match mode {
                        ColliderMode::Kill => {
                            buf.kill(i);
                            continue;
                        }
                        ColliderMode::Absorb => {
                            buf.vel_x[i] = 0.0;
                            buf.vel_y[i] = 0.0;
                            continue;
                        }
                        ColliderMode::Bounce => {}
                    }
                    let (dx_l, dx_r) = (x - min_x, max_x - x);
                    let (dy_t, dy_b) = (y - min_y, max_y - y);
                    let min_dx = crate::js::min(dx_l, dx_r);
                    let min_dy = crate::js::min(dy_t, dy_b);
                    if min_dx < min_dy {
                        buf.vel_x[i] = (-(f64::from(buf.vel_x[i]) * rest)) as f32;
                        buf.pos_x[i] = if dx_l < dx_r {
                            min_x - 0.1
                        } else {
                            max_x + 0.1
                        } as f32;
                    } else {
                        buf.vel_y[i] = (-(f64::from(buf.vel_y[i]) * rest)) as f32;
                        buf.pos_y[i] = if dy_t < dy_b {
                            min_y - 0.1
                        } else {
                            max_y + 0.1
                        } as f32;
                    }
                }
            }
            ColliderShape::Polygon {
                mode,
                rest,
                inverted,
                edges: e,
            } => {
                let (mode, rest, inverted) = (*mode, *rest, *inverted);
                let n = e.pts.len();
                for i in 0..cap {
                    if buf.alive[i] == 0 {
                        continue;
                    }
                    let x = f64::from(buf.pos_x[i]);
                    let y = f64::from(buf.pos_y[i]);
                    if !inverted && (x < e.min_x || x > e.max_x || y < e.min_y || y > e.max_y) {
                        continue;
                    }
                    let in_poly = point_in_polygon(x, y, &e.pts);
                    let hit = if inverted { !in_poly } else { in_poly };
                    if !hit {
                        continue;
                    }
                    match mode {
                        ColliderMode::Kill => {
                            buf.kill(i);
                            continue;
                        }
                        ColliderMode::Absorb => {
                            buf.vel_x[i] = 0.0;
                            buf.vel_y[i] = 0.0;
                            continue;
                        }
                        ColliderMode::Bounce => {}
                    }
                    let mut best_dist = f64::INFINITY;
                    let (mut best_nx, mut best_ny) = (0.0, -1.0);
                    let (mut best_px, mut best_py) = (x, y);
                    for k in 0..n {
                        let a = e.pts[k];
                        let ex = f64::from(e.edge_x[k]);
                        let ey = f64::from(e.edge_y[k]);
                        let el = f64::from(e.edge_len[k]);
                        let t_raw = ((x - a.0) * ex + (y - a.1) * ey) / (el * el);
                        // NaN passes through, as with the TS ternary.
                        let t = t_raw.clamp(0.0, 1.0);
                        let (cx2, cy2) = (a.0 + ex * t, a.1 + ey * t);
                        let (ddx, ddy) = (x - cx2, y - cy2);
                        let d2 = ddx * ddx + ddy * ddy;
                        if d2 < best_dist {
                            best_dist = d2;
                            best_nx = f64::from(e.norm_x[k]);
                            best_ny = f64::from(e.norm_y[k]);
                            best_px = cx2;
                            best_py = cy2;
                        }
                    }
                    let nx = if inverted { -best_nx } else { best_nx };
                    let ny = if inverted { -best_ny } else { best_ny };
                    let v_dot_n = f64::from(buf.vel_x[i]) * nx + f64::from(buf.vel_y[i]) * ny;
                    if v_dot_n < 0.0 {
                        buf.vel_x[i] =
                            ((f64::from(buf.vel_x[i]) - 2.0 * v_dot_n * nx) * rest) as f32;
                        buf.vel_y[i] =
                            ((f64::from(buf.vel_y[i]) - 2.0 * v_dot_n * ny) * rest) as f32;
                    }
                    buf.pos_x[i] = (best_px + nx * 0.5) as f32;
                    buf.pos_y[i] = (best_py + ny * 0.5) as f32;
                }
            }
        }
    }
}

/// Even-odd point-in-polygon (`pointInPolygon` / `pointInPoly` in TS).
pub fn point_in_polygon(x: f64, y: f64, pts: &[(f64, f64)]) -> bool {
    let n = pts.len();
    let mut inside = false;
    if n == 0 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        let intersect =
            ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / or(yj - yi, 1e-9) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ─── ParticleBuffer ──────────────────────────────────────────────────────────

/// Capacity-fixed pool of particles in Struct-of-Arrays layout
/// (`ParticleBuffer` in TS).
///
/// Slot `i` is live iff `alive[i] == 1`; dead slots keep stale data, so
/// readers must check `alive`. All per-particle arrays have `capacity`
/// entries. Colours are 0..1 floats.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleBuffer {
    pub capacity: usize,
    /// Number of alive particles.
    pub count: i64,
    pub pos_x: Vec<f32>,
    pub pos_y: Vec<f32>,
    pub vel_x: Vec<f32>,
    pub vel_y: Vec<f32>,
    pub age: Vec<f32>,
    pub lifetime: Vec<f32>,
    pub size: Vec<f32>,
    pub rotation: Vec<f32>,
    pub ang_vel: Vec<f32>,
    pub mass: Vec<f32>,
    pub color_r: Vec<f32>,
    pub color_g: Vec<f32>,
    pub color_b: Vec<f32>,
    pub color_a: Vec<f32>,
    pub id: Vec<u32>,
    /// 0 = dead, 1 = alive.
    pub alive: Vec<u8>,
    /// Force accumulators, zeroed before each force pass.
    pub force_x: Vec<f32>,
    pub force_y: Vec<f32>,
    /// Next slot a spawn takes, or -1 when full.
    pub free_head: i32,
    /// Free-slot chain.
    pub free_next: Vec<i32>,
    /// Next particle id (a JS number; ids are stored mod 2³²).
    pub next_id: u64,
}

impl ParticleBuffer {
    /// `new ParticleBuffer(capacity)`: non-finite or < 1 → 1, then floor.
    pub fn new(capacity: f64) -> ParticleBuffer {
        let cap = if !capacity.is_finite() || capacity < 1.0 {
            1
        } else {
            capacity.floor() as usize
        };
        let f = || vec![0f32; cap];
        let mut free_next: Vec<i32> = (1..=cap as i32).collect();
        free_next[cap - 1] = -1;
        ParticleBuffer {
            capacity: cap,
            count: 0,
            pos_x: f(),
            pos_y: f(),
            vel_x: f(),
            vel_y: f(),
            age: f(),
            lifetime: f(),
            size: f(),
            rotation: f(),
            ang_vel: f(),
            mass: f(),
            color_r: f(),
            color_g: f(),
            color_b: f(),
            color_a: f(),
            id: vec![0; cap],
            alive: vec![0; cap],
            force_x: f(),
            force_y: f(),
            free_head: 0,
            free_next,
            next_id: 1,
        }
    }

    /// Pop a free slot, mark it alive with a fresh id and age 0. `None`
    /// when full.
    pub fn spawn(&mut self) -> Option<usize> {
        if self.free_head == -1 {
            return None;
        }
        let i = self.free_head as usize;
        self.free_head = self.free_next[i];
        self.alive[i] = 1;
        self.id[i] = self.next_id as u32;
        self.next_id += 1;
        self.age[i] = 0.0;
        self.count += 1;
        Some(i)
    }

    /// Reuse the live slot with the smallest id (O(capacity)).
    pub fn spawn_replacing_oldest(&mut self) -> Option<usize> {
        let mut oldest: Option<usize> = None;
        let mut oldest_id = f64::INFINITY;
        for i in 0..self.capacity {
            if self.alive[i] != 0 && f64::from(self.id[i]) < oldest_id {
                oldest_id = f64::from(self.id[i]);
                oldest = Some(i);
            }
        }
        let Some(i) = oldest else {
            return self.spawn();
        };
        self.id[i] = self.next_id as u32;
        self.next_id += 1;
        self.age[i] = 0.0;
        Some(i)
    }

    /// Mark slot `i` dead and push it onto the free list (no-op if dead).
    pub fn kill(&mut self, i: usize) {
        if self.alive[i] == 0 {
            return;
        }
        self.alive[i] = 0;
        self.free_next[i] = self.free_head;
        self.free_head = i as i32;
        self.count -= 1;
    }

    pub fn zero_forces(&mut self) {
        self.force_x.fill(0.0);
        self.force_y.fill(0.0);
    }

    /// `initSlot`: write a fresh particle's fields.
    #[allow(clippy::too_many_arguments)]
    pub fn init_slot(
        &mut self,
        i: usize,
        x: f64,
        y: f64,
        vx: f64,
        vy: f64,
        lifetime: f64,
        size: f64,
        color: Color,
        mass: f64,
    ) {
        self.pos_x[i] = x as f32;
        self.pos_y[i] = y as f32;
        self.vel_x[i] = vx as f32;
        self.vel_y[i] = vy as f32;
        self.lifetime[i] = lifetime as f32;
        self.size[i] = size as f32;
        self.color_r[i] = color.r as f32;
        self.color_g[i] = color.g as f32;
        self.color_b[i] = color.b as f32;
        self.color_a[i] = color.a as f32;
        self.mass[i] = mass as f32;
        self.rotation[i] = 0.0;
        self.ang_vel[i] = 0.0;
    }

    /// The TS object's enumerable fields, in declaration order, with the
    /// typed arrays copied out. For parity checks / JSON / debugging.
    pub fn to_value(&self) -> Value {
        let f =
            |v: &Vec<f32>| Value::Handle(Handle::TypedArray(TypedArray::F32(Arc::new(v.clone()))));
        let mut m: IndexMap<String, Value> = IndexMap::new();
        m.insert("capacity".into(), Value::Num(self.capacity as f64));
        m.insert("count".into(), Value::Num(self.count as f64));
        for (k, v) in [
            ("posX", &self.pos_x),
            ("posY", &self.pos_y),
            ("velX", &self.vel_x),
            ("velY", &self.vel_y),
            ("age", &self.age),
            ("lifetime", &self.lifetime),
            ("size", &self.size),
            ("rotation", &self.rotation),
            ("angVel", &self.ang_vel),
            ("mass", &self.mass),
            ("colorR", &self.color_r),
            ("colorG", &self.color_g),
            ("colorB", &self.color_b),
            ("colorA", &self.color_a),
        ] {
            m.insert(k.into(), f(v));
        }
        m.insert(
            "id".into(),
            Value::Handle(Handle::TypedArray(TypedArray::U32(Arc::new(
                self.id.clone(),
            )))),
        );
        m.insert(
            "alive".into(),
            Value::Handle(Handle::TypedArray(TypedArray::U8(Arc::new(
                self.alive.clone(),
            )))),
        );
        m.insert("forceX".into(), f(&self.force_x));
        m.insert("forceY".into(), f(&self.force_y));
        m.insert("freeHead".into(), Value::Num(f64::from(self.free_head)));
        m.insert(
            "freeNext".into(),
            Value::Handle(Handle::TypedArray(TypedArray::I32(Arc::new(
                self.free_next.clone(),
            )))),
        );
        m.insert("nextId".into(), Value::Num(self.next_id as f64));
        Value::Object(Arc::new(m))
    }
}

/// A shared, mutable particle buffer — what a `particle` wire carries.
/// Identity (`Arc::ptr_eq`) is the buffer's identity, like a JS reference.
#[derive(Clone)]
pub struct ParticleHandle(pub Arc<RwLock<ParticleBuffer>>);

impl ParticleHandle {
    pub fn new(buf: ParticleBuffer) -> ParticleHandle {
        ParticleHandle(Arc::new(RwLock::new(buf)))
    }

    /// Read access (renderers, trails). Ignores lock poisoning.
    pub fn read(&self) -> RwLockReadGuard<'_, ParticleBuffer> {
        self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Write access (the owning emitter).
    pub fn write(&self) -> RwLockWriteGuard<'_, ParticleBuffer> {
        self.0.write().unwrap_or_else(|e| e.into_inner())
    }

    pub fn ptr_eq(&self, other: &ParticleHandle) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for ParticleHandle {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

impl fmt::Debug for ParticleHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.read();
        write!(f, "ParticleBuffer({}/{})", b.count, b.capacity)
    }
}

// ─── Integration / aging ─────────────────────────────────────────────────────

/// Symplectic Euler: v += (f/m + g)·dt, v *= exp(-drag·dt), p += v·dt.
pub fn integrate_euler(buf: &mut ParticleBuffer, dt: f64, world: &World) {
    let drag_factor = (-world.drag * dt).exp();
    let (gx, gy) = (world.gravity_x, world.gravity_y);
    for i in 0..buf.capacity {
        if buf.alive[i] == 0 {
            continue;
        }
        let mi = or(f64::from(buf.mass[i]), 1.0);
        let inv_m = 1.0 / mi;
        let fxi = f64::from(buf.force_x[i]);
        let fyi = f64::from(buf.force_y[i]);
        let mut vxi = f64::from(buf.vel_x[i]) + (fxi * inv_m + gx) * dt;
        let mut vyi = f64::from(buf.vel_y[i]) + (fyi * inv_m + gy) * dt;
        vxi *= drag_factor;
        vyi *= drag_factor;
        buf.vel_x[i] = vxi as f32;
        buf.vel_y[i] = vyi as f32;
        // Position uses the unrounded f64 velocity, as in TS.
        buf.pos_x[i] = (f64::from(buf.pos_x[i]) + vxi * dt) as f32;
        buf.pos_y[i] = (f64::from(buf.pos_y[i]) + vyi * dt) as f32;
    }
}

/// Velocity Verlet: half-step velocity, move, re-run forces at the new
/// positions (`apply_forces`), complete velocity, then drag.
pub fn integrate_verlet<E>(
    buf: &mut ParticleBuffer,
    dt: f64,
    world: &World,
    mut apply_forces: impl FnMut(&mut ParticleBuffer) -> Result<(), E>,
) -> Result<(), E> {
    let half_dt = dt * 0.5;
    let (gx, gy) = (world.gravity_x, world.gravity_y);
    for i in 0..buf.capacity {
        if buf.alive[i] == 0 {
            continue;
        }
        let inv_m = 1.0 / or(f64::from(buf.mass[i]), 1.0);
        let ax = f64::from(buf.force_x[i]) * inv_m + gx;
        let ay = f64::from(buf.force_y[i]) * inv_m + gy;
        let vhx = f64::from(buf.vel_x[i]) + ax * half_dt;
        let vhy = f64::from(buf.vel_y[i]) + ay * half_dt;
        buf.vel_x[i] = vhx as f32;
        buf.vel_y[i] = vhy as f32;
        buf.pos_x[i] = (f64::from(buf.pos_x[i]) + vhx * dt) as f32;
        buf.pos_y[i] = (f64::from(buf.pos_y[i]) + vhy * dt) as f32;
    }
    apply_forces(buf)?;
    let drag_factor = (-world.drag * dt).exp();
    for i in 0..buf.capacity {
        if buf.alive[i] == 0 {
            continue;
        }
        let inv_m = 1.0 / or(f64::from(buf.mass[i]), 1.0);
        let ax = f64::from(buf.force_x[i]) * inv_m + gx;
        let ay = f64::from(buf.force_y[i]) * inv_m + gy;
        buf.vel_x[i] = ((f64::from(buf.vel_x[i]) + ax * half_dt) * drag_factor) as f32;
        buf.vel_y[i] = ((f64::from(buf.vel_y[i]) + ay * half_dt) * drag_factor) as f32;
    }
    Ok(())
}

/// Advance every alive particle's age; kill those past their lifetime.
/// Returns the number killed.
pub fn age_and_cull(buf: &mut ParticleBuffer, dt: f64) -> usize {
    let mut killed = 0;
    for i in 0..buf.capacity {
        if buf.alive[i] == 0 {
            continue;
        }
        let a = f64::from(buf.age[i]) + dt;
        if a >= f64::from(buf.lifetime[i]) {
            buf.kill(i);
            killed += 1;
        } else {
            buf.age[i] = a as f32;
        }
    }
    killed
}

// ─── Render template ─────────────────────────────────────────────────────────

/// How the renderer draws each live particle of a `ParticleBatch`.
#[derive(Clone, Debug, PartialEq)]
pub enum ParticleTemplate {
    /// A filled circle of diameter `size[i]` centred at (posX, posY).
    Circle,
    /// A filled square of side `size[i]` centred at (posX, posY).
    Rect,
    /// The layer, translated to each particle's position.
    Layer(Arc<SceneNode>),
}

impl ParticleTemplate {
    pub fn to_value(&self) -> Value {
        let mut m = IndexMap::new();
        let kind = match self {
            ParticleTemplate::Circle => "circle",
            ParticleTemplate::Rect => "rect",
            ParticleTemplate::Layer(_) => "layer",
        };
        m.insert("kind".to_string(), Value::str(kind));
        if let ParticleTemplate::Layer(n) = self {
            m.insert("node".to_string(), Value::Scene(n.clone()));
        }
        Value::Object(Arc::new(m))
    }
}

// ─── Handles: opaque JS objects on wires ─────────────────────────────────────

/// A JS typed array (only produced for trail history and for the JS-object
/// views of a [`ParticleBuffer`]).
#[derive(Clone, Debug, PartialEq)]
pub enum TypedArray {
    F32(Arc<Vec<f32>>),
    U32(Arc<Vec<u32>>),
    I32(Arc<Vec<i32>>),
    U8(Arc<Vec<u8>>),
}

impl TypedArray {
    /// The JS constructor name.
    pub fn name(&self) -> &'static str {
        match self {
            TypedArray::F32(_) => "Float32Array",
            TypedArray::U32(_) => "Uint32Array",
            TypedArray::I32(_) => "Int32Array",
            TypedArray::U8(_) => "Uint8Array",
        }
    }

    pub fn len(&self) -> usize {
        match self {
            TypedArray::F32(v) => v.len(),
            TypedArray::U32(v) => v.len(),
            TypedArray::I32(v) => v.len(),
            TypedArray::U8(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Element `i` as a JS number.
    pub fn get(&self, i: usize) -> f64 {
        match self {
            TypedArray::F32(v) => f64::from(v[i]),
            TypedArray::U32(v) => f64::from(v[i]),
            TypedArray::I32(v) => f64::from(v[i]),
            TypedArray::U8(v) => f64::from(v[i]),
        }
    }

    /// The raw little-endian bytes (what the trace exporter hashes).
    pub fn le_bytes(&self) -> Vec<u8> {
        match self {
            TypedArray::F32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            TypedArray::U32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            TypedArray::I32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            TypedArray::U8(v) => v.to_vec(),
        }
    }

    /// FNV-1a (32-bit) of the raw bytes, as 8 lowercase hex digits.
    pub fn fnv1a(&self) -> String {
        format!("{:08x}", fnv1a(&self.le_bytes()))
    }
}

/// FNV-1a, 32-bit.
pub fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// An opaque JS object on a wire (see the module docs).
#[derive(Clone, Debug)]
pub enum Handle {
    World(Arc<World>),
    Force(Arc<Force>),
    Collider(Arc<Collider>),
    Particles(ParticleHandle),
    TypedArray(TypedArray),
    /// A bare JS function with this `.name` (a collider's `apply` in the
    /// object view; never on a wire by itself).
    Function(&'static str),
}

impl Handle {
    /// Short name for debugging / UI.
    pub fn type_name(&self) -> &'static str {
        match self {
            Handle::World(_) => "world",
            Handle::Force(_) => "force",
            Handle::Collider(_) => "collider",
            Handle::Particles(_) => "particles",
            Handle::TypedArray(t) => t.name(),
            Handle::Function(_) => "function",
        }
    }

    /// `typeof v === 'function'`.
    pub fn is_function(&self) -> bool {
        matches!(self, Handle::Force(_) | Handle::Function(_))
    }

    /// The function's `.name` (`name || 'anonymous'`, as the traces record).
    pub fn fn_name(&self) -> Option<&'static str> {
        match self {
            Handle::Force(_) => Some("force"),
            Handle::Function(n) => Some(if n.is_empty() { "anonymous" } else { n }),
            _ => None,
        }
    }

    /// The constructor name of a class instance (`$class` in traces).
    pub fn class_name(&self) -> Option<&'static str> {
        match self {
            Handle::Particles(_) => Some("ParticleBuffer"),
            Handle::TypedArray(t) => Some(t.name()),
            _ => None,
        }
    }

    /// The JS object as plain data (`None` for functions). Typed arrays
    /// become `{"0": …, "1": …}`, like `JSON.stringify` sees them.
    pub fn to_value(&self) -> Option<Value> {
        match self {
            Handle::World(w) => Some(w.to_value()),
            Handle::Collider(c) => Some(c.to_value()),
            Handle::Particles(p) => Some(p.read().to_value()),
            Handle::TypedArray(t) => {
                let m: IndexMap<String, Value> = (0..t.len())
                    .map(|i| (i.to_string(), Value::Num(t.get(i))))
                    .collect();
                Some(Value::Object(Arc::new(m)))
            }
            Handle::Force(_) | Handle::Function(_) => None,
        }
    }

    /// JS `String(v)`.
    pub fn js_string(&self) -> String {
        match self {
            // JS prints the function's source; no equivalent here.
            Handle::Force(_) | Handle::Function(_) => "function".into(),
            Handle::TypedArray(t) => (0..t.len())
                .map(|i| crate::js::num_to_string(t.get(i)))
                .collect::<Vec<_>>()
                .join(","),
            _ => "[object Object]".into(),
        }
    }

    /// JS `a === b` (reference identity).
    pub fn ptr_eq(&self, other: &Handle) -> bool {
        match (self, other) {
            (Handle::World(a), Handle::World(b)) => Arc::ptr_eq(a, b),
            (Handle::Force(a), Handle::Force(b)) => Arc::ptr_eq(a, b),
            (Handle::Collider(a), Handle::Collider(b)) => Arc::ptr_eq(a, b),
            (Handle::Particles(a), Handle::Particles(b)) => a.ptr_eq(b),
            (Handle::TypedArray(a), Handle::TypedArray(b)) => match (a, b) {
                (TypedArray::F32(x), TypedArray::F32(y)) => Arc::ptr_eq(x, y),
                (TypedArray::U32(x), TypedArray::U32(y)) => Arc::ptr_eq(x, y),
                (TypedArray::I32(x), TypedArray::I32(y)) => Arc::ptr_eq(x, y),
                (TypedArray::U8(x), TypedArray::U8(y)) => Arc::ptr_eq(x, y),
                _ => false,
            },
            (Handle::Function(a), Handle::Function(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypot_matches_v8() {
        // Recorded with node 22 (`Math.hypot`).
        let cases = [
            (179.79697632409128, -0.35272097965937166, 179.7973223031292),
            (327.8364867747682, 0.6049955620742367, 327.8370450093352),
            (417.9703735955846, -0.672063769219698, 417.9709139083035),
            (434.278325481691, 0.49848448718660265, 434.2786115732221),
            (373.87987560835813, -0.06790680354458875, 373.8798817752238),
            (77.30069250161634, 1.4126984146005077, 77.31360021393428),
            (-106.49538177456863, -2.288414097390026, 106.5199661020987),
            (-436.9263432499926, 3.959656734037288, 436.94428512942216),
            (29.84404625586952, 0.6100632943127647, 29.850280972639894),
            (429.6078727824839, -6.318863421206705, 429.65434059442106),
        ];
        for (a, b, want) in cases {
            assert_eq!(js_hypot(a, b), want, "hypot({a}, {b})");
        }
        assert_eq!(js_hypot(0.0, 0.0), 0.0);
        assert!(js_hypot(f64::NAN, 1.0).is_nan());
        assert_eq!(js_hypot(f64::NAN, f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn exp_matches_v8() {
        // Recorded with node 22 (`Math.exp`): the integrators' drag factors.
        assert_eq!((-0.05f64 / 60.0).exp(), 0.9991670137924583);
        assert_eq!((-0.02f64 / 60.0).exp(), 0.9996667222160499);
        assert_eq!((-0.5f64 / 60.0).exp(), 0.991701292638876);
    }

    #[test]
    fn free_list_spawn_and_kill() {
        let mut b = ParticleBuffer::new(3.0);
        assert_eq!(b.spawn(), Some(0));
        assert_eq!(b.spawn(), Some(1));
        b.kill(0);
        assert_eq!(b.count, 1);
        assert_eq!(b.spawn(), Some(0));
        assert_eq!(b.id[0], 3);
        assert_eq!(b.spawn(), Some(2));
        assert_eq!(b.spawn(), None);
        // Oldest live id is slot 1 (id 2).
        assert_eq!(b.spawn_replacing_oldest(), Some(1));
        assert_eq!(b.id[1], 5);
        assert_eq!(ParticleBuffer::new(f64::NAN).capacity, 1);
        assert_eq!(ParticleBuffer::new(2.7).capacity, 2);
    }

    #[test]
    fn fnv_of_typed_arrays() {
        // A fresh Float32Array(1500) / Int32Array, hashed like the exporter.
        let z = TypedArray::F32(Arc::new(vec![0.0; 1500]));
        assert_eq!(z.fnv1a(), "a6cc3885");
        // particles-fountain frame 1: mass = 1 for the 8 live slots.
        let mut mass = vec![0.0; 1500];
        mass[..8].fill(1.0);
        assert_eq!(TypedArray::F32(Arc::new(mass)).fnv1a(), "e4326e45");
    }
}
