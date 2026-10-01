//! `particles/*` nodes. Ported from `src/nodes/particles/` (emitter.ts,
//! bounds.ts, render-as.ts, trail.ts, from-layer.ts). The runtime they share
//! (buffer, integrators, forces, colliders) is `crate::particles`.
//!
//! Also home to the helpers the `force/*`, `collider/*` and `physics/*`
//! modules share: JS-style input coercion and [`memo`], which reproduces the
//! TS runtime's pure-node memoisation for these node types (see there).

use super::shared::first;
use super::Registrar;
use crate::js::{self, Mulberry32};
use crate::node::{raw, NodeImpl, NodeInit, NodeResult};
use crate::particles::{
    age_and_cull, integrate_euler, integrate_verlet, BoundsMode, Collider, ColliderShape, Handle,
    Integrator, ParticleBuffer, ParticleHandle, ParticleTemplate, World,
};
use crate::scene::{self, Paint, ResolvedStyle, SceneNode, Style};
use crate::value::{empty, one, Color, Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("particles/emitter", emitter);
    r.node("particles/bounds", bounds);
    r.node("particles/render-as", render_as);
    r.node("particles/trail", trail);
    r.node("particles/from-layer", from_layer);
}

#[cfg(test)]
mod tests;

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// `Number(v ?? d)`.
pub(crate) fn num_or(v: &Value, d: f64) -> f64 {
    if v.is_nullish() {
        d
    } else {
        v.to_number()
    }
}

/// `(v as Vec2 | null) ?? d`, then `.x` / `.y` (a non-vec2 reads NaN).
pub(crate) fn vec2_or(v: &Value, d: Vec2) -> Vec2 {
    match v {
        Value::Undefined | Value::Null => d,
        Value::Vec2(p) => *p,
        other => Vec2 {
            x: other.get("x").to_number(),
            y: other.get("y").to_number(),
        },
    }
}

/// `(v as Color | null) ?? d`, then `.r` … `.a`.
fn color_or(v: &Value, d: Color) -> Color {
    match v {
        Value::Undefined | Value::Null => d,
        Value::Color(c) => *c,
        other => Color {
            r: other.get("r").to_number(),
            g: other.get("g").to_number(),
            b: other.get("b").to_number(),
            a: other.get("a").to_number(),
        },
    }
}

/// `Math.max(lo, Math.min(hi, x))` with JS NaN propagation.
pub(crate) fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    js::max(lo, js::min(hi, x))
}

/// `{ kind: 'group', children: [] }`.
pub(crate) fn empty_group() -> Value {
    Value::Scene(Arc::new(SceneNode::group(Vec::new())))
}

/// `resolveStyle(style ?? null, DEFAULT_STYLE)` for a style pin.
fn resolved_style(v: &Value) -> Arc<ResolvedStyle> {
    let st: Option<&Style> = match v {
        Value::Style(s) => Some(s),
        _ => None,
    };
    scene::resolve_style(st, scene::default_style())
}

/// `colorToCss(c)` for a truthy colour-ish value.
fn color_paint(v: &Value) -> Paint {
    Paint::from_color(super::shared::to_color(v))
}

/// Equality for the memo: JS `===` on the value TS would have seen. TS
/// memoises every pure node, so an unchanged upstream hands over the SAME
/// object; the Rust engine recomputes upstream nodes each tick, so values
/// that are plain data (scenes, styles, lists, objects) compare by
/// structure here, while opaque handles (worlds, forces, buffers) compare
/// by identity, as in JS.
fn memo_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            Arc::ptr_eq(x, y)
                || (x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| memo_eq(p, q)))
        }
        (Value::Object(x), Value::Object(y)) => {
            Arc::ptr_eq(x, y)
                || (x.len() == y.len()
                    && x.iter()
                        .zip(y.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && memo_eq(v1, v2)))
        }
        (Value::Scene(x), Value::Scene(y)) => Arc::ptr_eq(x, y) || **x == **y,
        (Value::Style(x), Value::Style(y)) => x == y,
        _ => a.same(b),
    }
}

fn inputs_eq(a: &[Spread], b: &[Spread]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            Arc::ptr_eq(x, y)
                || (x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| memo_eq(p, q)))
        })
}

/// The TS runtime's pure-compute memo (`tick.ts`): when a node's gathered
/// inputs equal the previous tick's, the previous OUTPUTS are reused and
/// compute is skipped. For these node types that is observable — a force
/// closure keeps its clock (turbulence / wind), `render-as` keeps the
/// `count` it saw first, `from-layer` doesn't re-draw random samples — so
/// the nodes reproduce it. Errors aren't memoised (TS retries next tick).
///
/// Difference: TS keys the memo per ForEach iteration; this keeps one
/// entry per node instance.
pub(crate) struct Memo {
    inner: Box<dyn NodeImpl>,
    last: Option<(Vec<Spread>, Vec<Spread>)>,
}

pub(crate) fn memo(inner: Box<dyn NodeImpl>) -> Box<dyn NodeImpl> {
    Box::new(Memo { inner, last: None })
}

impl NodeImpl for Memo {
    fn eval(&mut self, inputs: &[Spread], cx: &mut crate::node::EvalCtx) -> NodeResult {
        if let Some((prev_in, prev_out)) = &self.last {
            if inputs_eq(prev_in, inputs) {
                return Ok(prev_out.clone());
            }
        }
        let out = self.inner.eval(inputs, cx)?;
        self.last = Some((inputs.to_vec(), out.clone()));
        Ok(out)
    }
}

pub(crate) fn handle(h: Handle) -> Spread {
    one(Value::Handle(h))
}

// ─── particles/emitter ───────────────────────────────────────────────────────

struct EmitterState {
    buffer: ParticleHandle,
    /// Fractional spawns carried between ticks.
    spawn_acc: f64,
    /// Last tick's `time` input, for dt.
    last_t: f64,
    rng: Mulberry32,
}

/// `{x, y}` of a spawn position (`p !== null && typeof p === 'object' &&
/// 'x' in p`), or `None` when the element isn't one.
fn spawn_position(v: &Value) -> Option<(f64, f64)> {
    match v {
        Value::Vec2(p) => Some((p.x, p.y)),
        Value::Object(m) if m.contains_key("x") => {
            Some((v.get("x").to_number(), v.get("y").to_number()))
        }
        Value::Scene(n) => spawn_position(&n.to_value()),
        Value::Handle(h) => h.to_value().as_ref().and_then(spawn_position),
        _ => None,
    }
}

fn apply_force(v: &Value, buf: &mut ParticleBuffer, dt: f64, world: &World) -> Result<(), String> {
    match v {
        Value::Handle(Handle::Force(f)) => {
            f.apply(buf, dt, world);
            Ok(())
        }
        _ => Err("force is not a function".into()),
    }
}

fn apply_collider(
    v: &Value,
    buf: &mut ParticleBuffer,
    dt: f64,
    world: &World,
) -> Result<(), String> {
    match v {
        Value::Handle(Handle::Collider(c)) => {
            c.apply(buf, dt, world);
            Ok(())
        }
        _ => Err("collider.apply is not a function".into()),
    }
}

/// `particles/emitter`: owns the buffer; spawn → forces → integrate →
/// colliders → age + cull, every tick (effectful).
fn emitter(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut state: Option<EmitterState> = None;
    raw(move |inputs, _cx| {
        let max_count = js::max(1.0, num_or(&first(inputs, 8), 1000.0).floor());
        let replace_oldest = matches!(first(inputs, 9), Value::Str(s) if &*s == "replace-oldest");
        let seed = num_or(&first(inputs, 10), 1.0).floor();

        let stale = match &state {
            None => true,
            Some(s) => s.buffer.read().capacity as f64 != max_count,
        };
        if stale {
            state = Some(EmitterState {
                buffer: ParticleHandle::new(ParticleBuffer::new(max_count)),
                spawn_acc: 0.0,
                last_t: 0.0,
                rng: Mulberry32::new(js::to_uint32(seed)),
            });
        }
        let st = state.as_mut().expect("emitter state");

        // A non-world value would only fail once its force list is read
        // (after spawning), as in TS.
        let world_in = first(inputs, 0);
        let default_world;
        let world: Option<&World> = match &world_in {
            Value::Handle(Handle::World(w)) => Some(w),
            Value::Undefined | Value::Null => {
                default_world = World::default_world();
                Some(&default_world)
            }
            _ => None,
        };
        let time_scale = world.map_or(f64::NAN, |w| w.time_scale);

        let positions = inputs.get(1).cloned().unwrap_or_else(empty);
        let rate = js::max(0.0, num_or(&first(inputs, 2), 0.0));
        let lifetime = js::max(0.001, num_or(&first(inputs, 3), 2.0));
        let init_vel = vec2_or(&first(inputs, 4), Vec2 { x: 0.0, y: 0.0 });
        let vel_spread = js::max(0.0, num_or(&first(inputs, 5), 0.0));
        let color = color_or(&first(inputs, 6), Color::WHITE);
        let size = js::max(0.0, num_or(&first(inputs, 7), 4.0));
        let local_forces = inputs.get(11).cloned().unwrap_or_else(empty);
        let t_now = num_or(&first(inputs, 12), 0.0);

        // dt from the wired clock; 1/60 on the first ticks, clamped to 0.1.
        let mut dt = if t_now > 0.0 && st.last_t > 0.0 {
            t_now - st.last_t
        } else {
            1.0 / 60.0
        };
        if !dt.is_finite() || dt <= 0.0 {
            dt = 1.0 / 60.0;
        }
        if dt > 0.1 {
            dt = 0.1;
        }
        dt *= if time_scale == 0.0 || time_scale.is_nan() {
            1.0
        } else {
            time_scale
        };
        st.last_t = t_now;

        // 1. Spawn.
        st.spawn_acc += rate * dt;
        let spawn_now = st.spawn_acc.floor();
        st.spawn_acc -= spawn_now;
        let valid: Vec<(f64, f64)> = positions.iter().filter_map(spawn_position).collect();
        let mut buf = st.buffer.write();
        // TS loops forever on an infinite count with replace-oldest; stop
        // after one buffer's worth instead.
        let limit = if spawn_now.is_finite() {
            spawn_now
        } else {
            buf.capacity as f64
        };
        let mut n = 0.0;
        while n < limit {
            n += 1.0;
            let slot = match buf.spawn() {
                Some(s) => s,
                None if replace_oldest => match buf.spawn_replacing_oldest() {
                    Some(s) => s,
                    None => break,
                },
                None => break,
            };
            let p = if valid.is_empty() {
                (0.0, 0.0)
            } else {
                valid[(st.rng.next_f64() * valid.len() as f64).floor() as usize]
            };
            let jx = (st.rng.next_f64() * 2.0 - 1.0) * vel_spread * 100.0;
            let jy = (st.rng.next_f64() * 2.0 - 1.0) * vel_spread * 100.0;
            buf.init_slot(
                slot,
                p.0,
                p.1,
                init_vel.x + jx,
                init_vel.y + jy,
                lifetime,
                size,
                color,
                1.0,
            );
        }

        let Some(world) = world else {
            return Err("Cannot read properties of world (forces)".into());
        };

        // 2 + 3. Forces (re-run at the half step by Verlet).
        let apply_all = |b: &mut ParticleBuffer| -> Result<(), String> {
            b.zero_forces();
            for f in world.forces.iter() {
                apply_force(f, b, dt, world)?;
            }
            for f in local_forces.iter() {
                apply_force(f, b, dt, world)?;
            }
            Ok(())
        };
        apply_all(&mut buf)?;

        // 4. Integrate.
        match world.integrator {
            Integrator::Verlet => integrate_verlet(&mut buf, dt, world, apply_all)?,
            Integrator::Euler => integrate_euler(&mut buf, dt, world),
        }

        // 5. Colliders.
        for c in world.colliders.iter() {
            apply_collider(c, &mut buf, dt, world)?;
        }

        // 6. Age + cull.
        age_and_cull(&mut buf, dt);
        drop(buf);

        Ok(vec![handle(Handle::Particles(st.buffer.clone()))])
    })
}

// ─── particles/bounds ────────────────────────────────────────────────────────

/// `particles/bounds`: wrap / kill / bounce at a rectangle's edges.
fn bounds(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let size = vec2_or(&first(inputs, 0), Vec2 { x: 600.0, y: 400.0 });
        let center = vec2_or(&first(inputs, 1), Vec2 { x: 0.0, y: 0.0 });
        let mode = match first(inputs, 2) {
            Value::Str(s) if &*s == "kill" => BoundsMode::Kill,
            Value::Str(s) if &*s == "bounce" => BoundsMode::Bounce,
            _ => BoundsMode::Wrap,
        };
        let rest = clamp(num_or(&first(inputs, 3), 0.6), 0.0, 1.0);
        let hw = size.x.abs() * 0.5;
        let hh = size.y.abs() * 0.5;
        let (min_x, max_x) = (center.x - hw, center.x + hw);
        let (min_y, max_y) = (center.y - hh, center.y + hh);
        let shape = ColliderShape::Bounds {
            mode,
            rest,
            min_x,
            max_x,
            min_y,
            max_y,
            w: max_x - min_x,
            h: max_y - min_y,
        };
        Ok(vec![handle(Handle::Collider(Arc::new(Collider::new(
            shape, "apply",
        ))))])
    }))
}

// ─── particles/render-as ─────────────────────────────────────────────────────

/// `particles/render-as`: wrap a buffer in a `ParticleBatch` scene node.
fn render_as(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let Value::Handle(Handle::Particles(buf)) = first(inputs, 0) else {
            return Ok(vec![one(empty_group())]);
        };
        let template_kind = first(inputs, 1);
        let template_layer = first(inputs, 2);
        let tint = first(inputs, 4);
        let use_buffer_color = match first(inputs, 5) {
            Value::Undefined | Value::Null => true,
            v => v.truthy(),
        };
        let template = match (&template_kind, &template_layer) {
            (Value::Str(s), _) if &**s == "rect" => ParticleTemplate::Rect,
            (Value::Str(s), Value::Scene(n)) if &**s == "layer" => {
                ParticleTemplate::Layer(n.clone())
            }
            _ => ParticleTemplate::Circle,
        };
        let mut style = resolved_style(&first(inputs, 3));
        if tint.truthy() && !use_buffer_color {
            let mut st = (*style).clone();
            st.fill = Some(color_paint(&tint));
            style = Arc::new(st);
        }
        let count = buf.read().count as f64;
        Ok(vec![one(Value::Scene(Arc::new(
            SceneNode::ParticleBatch {
                buffer: buf,
                count,
                template,
                style,
                use_buffer_color,
            },
        )))])
    }))
}

// ─── particles/trail ─────────────────────────────────────────────────────────

struct TrailState {
    buffer: ParticleHandle,
    history: Arc<Vec<f32>>,
    segments: usize,
    head_idx: usize,
    capacity: usize,
}

/// `particles/trail`: a ring buffer of past positions per slot (effectful).
fn trail(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut state: Option<TrailState> = None;
    raw(move |inputs, _cx| {
        let segments = clamp(num_or(&first(inputs, 1), 8.0).floor(), 2.0, 64.0);
        let Value::Handle(Handle::Particles(buf)) = first(inputs, 0) else {
            return Ok(vec![one(empty_group())]);
        };
        // A NaN segment count (non-numeric constant) breaks the TS ring
        // buffer; fall back to the minimum.
        let segs = if segments.is_nan() {
            2
        } else {
            segments as usize
        };
        let b = buf.read();
        let cap = b.capacity;
        let stale = match &state {
            None => true,
            Some(s) => s.capacity != cap || s.segments != segs || !s.buffer.ptr_eq(&buf),
        };
        if stale {
            state = Some(TrailState {
                buffer: buf.clone(),
                history: Arc::new(vec![f32::NAN; cap * segs * 2]),
                segments: segs,
                head_idx: 0,
                capacity: cap,
            });
        }
        let st = state.as_mut().expect("trail state");
        st.head_idx = (st.head_idx + 1) % segs;
        let head = st.head_idx;
        let hist = Arc::make_mut(&mut st.history);
        for i in 0..cap {
            let base = i * segs * 2 + head * 2;
            if b.alive[i] != 0 {
                hist[base] = b.pos_x[i];
                hist[base + 1] = b.pos_y[i];
            } else {
                hist[base] = f32::NAN;
                hist[base + 1] = f32::NAN;
            }
        }
        drop(b);

        let tint = first(inputs, 3);
        let flag = |i: usize, d: bool| match first(inputs, i) {
            Value::Undefined | Value::Null => d,
            v => v.truthy(),
        };
        let fade_out = flag(4, true);
        let use_buffer_color = flag(5, true);
        let tapered = flag(6, false);
        let tail_width = clamp(num_or(&first(inputs, 7), 0.1), 0.0, 1.0);
        let base = resolved_style(&first(inputs, 2));
        let weight_or_1 = |w: f64| if w == 0.0 || w.is_nan() { 1.0 } else { w };
        let mut style = (*base).clone();
        if tint.truthy() {
            style.stroke = Some(color_paint(&tint));
            style.stroke_weight = weight_or_1(style.stroke_weight);
        }
        if style.stroke.as_ref().is_none_or(|p| p.css.is_empty()) {
            style.stroke = Some(Paint::css("#ffffff"));
            style.stroke_weight = weight_or_1(style.stroke_weight);
        }
        let style = if style == *base {
            base
        } else {
            Arc::new(style)
        };

        Ok(vec![one(Value::Scene(Arc::new(
            SceneNode::ParticleTrailBatch {
                buffer: buf,
                history: st.history.clone(),
                segments: segs,
                head_idx: head,
                style,
                fade_out,
                use_buffer_color,
                tapered,
                tail_width,
            },
        )))])
    })
}

// ─── particles/from-layer ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sampling {
    Outline,
    Fill,
}

/// `particles/from-layer`: sample spawn positions from a layer's shapes.
///
/// TS draws from the global, unseeded `Math.random`; the fixtures replace
/// it with `mulberry32(0x5eed)`, so this node draws from its own
/// `Mulberry32(0x5eed)` stream — identical to the fixtures whenever this
/// node is the patch's only `Math.random` consumer.
///
/// Text: TS rasterises glyphs on an `OffscreenCanvas` and samples inside /
/// outline pixels. TODO(#61): port that once text rasterisation exists in
/// the engine; until then text uses the TS bounding-box fallback (what TS
/// itself does without `OffscreenCanvas`, and what the fixtures recorded).
fn from_layer(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut rng = Mulberry32::new(0x5eed);
    memo(raw(move |inputs, _cx| {
        let layer = first(inputs, 0);
        let mode = match first(inputs, 1) {
            Value::Str(s) if &*s == "fill" => Sampling::Fill,
            _ => Sampling::Outline,
        };
        let density = js::max(1.0, num_or(&first(inputs, 2), 64.0).floor());
        if !layer.truthy() {
            return Ok(vec![empty()]);
        }
        let mut out: Vec<(f64, f64)> = Vec::new();
        if let Value::Scene(n) = &layer {
            let mut random = || rng.next_f64();
            sample_positions(n, mode, density, &mut out, &mut random);
        }
        Ok(vec![Arc::from(
            out.into_iter()
                .map(|(x, y)| Value::Vec2(Vec2 { x, y }))
                .collect::<Vec<_>>(),
        )])
    }))
}

type Random<'a> = dyn FnMut() -> f64 + 'a;

fn sample_positions(
    node: &SceneNode,
    mode: Sampling,
    density: f64,
    out: &mut Vec<(f64, f64)>,
    random: &mut Random,
) {
    match node {
        SceneNode::Group { children, .. } => {
            for c in children.iter() {
                sample_positions(c, mode, density, out, random);
            }
        }
        SceneNode::Rect {
            x,
            y,
            w,
            h,
            mode: m,
            ..
        } => {
            let (mut x, mut y, mut w, mut h) = (*x, *y, *w, *h);
            match &**m {
                "center" => {
                    x -= w / 2.0;
                    y -= h / 2.0;
                }
                "corners" => {
                    w -= x;
                    h -= y;
                }
                "radius" => {
                    x -= w;
                    y -= h;
                    w *= 2.0;
                    h *= 2.0;
                }
                _ => {}
            }
            sample_box(x, y, w, h, mode, density, out, random);
        }
        SceneNode::Ellipse {
            x,
            y,
            w,
            h,
            mode: m,
            ..
        } => {
            let (mut cx, mut cy, mut rx, mut ry) = (*x, *y, w / 2.0, h / 2.0);
            match &**m {
                "corner" => {
                    cx += rx;
                    cy += ry;
                }
                "radius" => {
                    rx = *w;
                    ry = *h;
                }
                _ => {}
            }
            let tau = std::f64::consts::PI * 2.0;
            let mut i = 0.0;
            while i < density {
                if mode == Sampling::Fill {
                    let r = random().sqrt();
                    let a = random() * tau;
                    out.push((cx + a.cos() * r * rx, cy + a.sin() * r * ry));
                } else {
                    let a = (i / density) * tau;
                    out.push((cx + a.cos() * rx, cy + a.sin() * ry));
                }
                i += 1.0;
            }
        }
        SceneNode::Line { x1, y1, x2, y2, .. } => {
            let d = if density - 1.0 == 0.0 {
                1.0
            } else {
                density - 1.0
            };
            let mut i = 0.0;
            while i < density {
                let t = i / d;
                out.push((x1 + (x2 - x1) * t, y1 + (y2 - y1) * t));
                i += 1.0;
            }
        }
        SceneNode::Polygon { points, closed, .. } => {
            let pts: Vec<(f64, f64)> = points.iter().map(|p| (p.x, p.y)).collect();
            sample_polygon(&pts, *closed, mode, density, out, random);
        }
        SceneNode::Text { text, x, y, style } => {
            if text.is_empty() {
                return;
            }
            // TODO(#61): glyph rasterisation; this is the TS bbox fallback.
            let sz = style.font_size;
            let w = text.encode_utf16().count() as f64 * sz * 0.6;
            let h = sz;
            let mut bx = *x;
            let by = y - h * 0.7;
            match &*style.text_align {
                "center" => bx -= w / 2.0,
                "right" => bx -= w,
                _ => {}
            }
            sample_box(bx, by, w, h, mode, density, out, random);
        }
        SceneNode::Triangle { pts: p, .. } => {
            if mode == Sampling::Fill {
                let mut i = 0.0;
                while i < density {
                    let (mut r1, mut r2) = (random(), random());
                    if r1 + r2 > 1.0 {
                        r1 = 1.0 - r1;
                        r2 = 1.0 - r2;
                    }
                    let x = p[0] + r1 * (p[2] - p[0]) + r2 * (p[4] - p[0]);
                    let y = p[1] + r1 * (p[3] - p[1]) + r2 * (p[5] - p[1]);
                    out.push((x, y));
                    i += 1.0;
                }
            } else {
                let edges = [
                    ((p[0], p[1]), (p[2], p[3])),
                    ((p[2], p[3]), (p[4], p[5])),
                    ((p[4], p[5]), (p[0], p[1])),
                ];
                let per = (density / 3.0).ceil();
                for (a, b) in edges {
                    let mut i = 0.0;
                    while i < per && (out.len() as f64) < density {
                        let f = i / per;
                        out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
                        i += 1.0;
                    }
                }
            }
        }
        SceneNode::Point { x, y, .. } => out.push((*x, *y)),
        // arc, quad, particle batches: skipped (as in TS v1).
        _ => {}
    }
}

/// `sampleRect` / `sampleTextBBox` body: uniform fill or perimeter walk.
#[allow(clippy::too_many_arguments)]
fn sample_box(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    mode: Sampling,
    density: f64,
    out: &mut Vec<(f64, f64)>,
    random: &mut Random,
) {
    if mode == Sampling::Fill {
        let mut i = 0.0;
        while i < density {
            let px = x + random() * w;
            let py = y + random() * h;
            out.push((px, py));
            i += 1.0;
        }
    } else {
        let perim = 2.0 * (w + h);
        let mut i = 0.0;
        while i < density {
            let t = (i / density) * perim;
            if t < w {
                out.push((x + t, y));
            } else if t < w + h {
                out.push((x + w, y + (t - w)));
            } else if t < 2.0 * w + h {
                out.push((x + (2.0 * w + h - t), y + h));
            } else {
                out.push((x, y + (perim - t)));
            }
            i += 1.0;
        }
    }
}

fn sample_polygon(
    pts: &[(f64, f64)],
    closed: bool,
    mode: Sampling,
    density: f64,
    out: &mut Vec<(f64, f64)>,
    random: &mut Random,
) {
    if pts.len() < 2 {
        return;
    }
    if mode == Sampling::Outline || !closed {
        let mut lens: Vec<f64> = Vec::new();
        let mut total = 0.0;
        for i in 0..pts.len() - 1 {
            let l = crate::particles::js_hypot(pts[i + 1].0 - pts[i].0, pts[i + 1].1 - pts[i].1);
            lens.push(l);
            total += l;
        }
        if closed {
            let (first, last) = (pts[0], pts[pts.len() - 1]);
            let l = crate::particles::js_hypot(first.0 - last.0, first.1 - last.1);
            lens.push(l);
            total += l;
        }
        let mut i = 0.0;
        while i < density {
            let mut t = (i / density) * total;
            for (j, &len) in lens.iter().enumerate() {
                if t <= len {
                    let a = pts[j];
                    let b = pts[(j + 1) % pts.len()];
                    let f = if len > 0.0 { t / len } else { 0.0 };
                    out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
                    break;
                }
                t -= len;
            }
            i += 1.0;
        }
        return;
    }
    // Fill: bounding-box rejection sampling.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in pts {
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x > max_x {
            max_x = x;
        }
        if y > max_y {
            max_y = y;
        }
    }
    let mut tries = 0.0;
    while (out.len() as f64) < density && tries < density * 40.0 {
        let x = min_x + random() * (max_x - min_x);
        let y = min_y + random() * (max_y - min_y);
        if crate::particles::point_in_polygon(x, y, pts) {
            out.push((x, y));
        }
        tries += 1.0;
    }
}
