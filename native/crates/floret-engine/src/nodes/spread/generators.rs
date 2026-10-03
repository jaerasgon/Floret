//! Spread generators: `generators.ts` and `vec2-generators.ts`.

use super::{array_len, first, get, get_f, hash32, hash_unit, int_count, num_d, ok, sp};
use crate::js;
use crate::node::{EvalCtx, NodeResult};
use crate::nodes::shared::to_vec2;
use crate::value::{Spread, Value, Vec2};
use std::f64::consts::PI;

/// Evenly stepping from `from` to `to`, `count` values.
pub fn linear(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let from = num_d(&first(inputs, 0), 0.0);
    let to = num_d(&first(inputs, 1), 1.0);
    let count = int_count(&first(inputs, 2));
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    if count == 1.0 {
        return ok(vec![vec![Value::Num(from)]]);
    }
    let n = array_len(count)?;
    let step = (to - from) / (count - 1.0);
    ok(vec![(0..n)
        .map(|i| Value::Num(from + step * i as f64))
        .collect()])
}

/// 0, 1, …, count − 1.
pub fn range(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let count = int_count(&first(inputs, 0));
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    let n = array_len(count)?;
    ok(vec![(0..n).map(|i| Value::Num(i as f64)).collect()])
}

/// N copies of a scalar, or a spread tiled N times.
pub fn repeat(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let value = sp(inputs, 0);
    let count = int_count(&first(inputs, 1));
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    if value.len() <= 1 {
        let n = array_len(count)?;
        let v = get(value, 0);
        let v = if v.is_nullish() { Value::Num(0.0) } else { v };
        return ok(vec![vec![v; n]]);
    }
    let n = array_len(count * value.len() as f64)?;
    ok(vec![value.iter().cycle().take(n).cloned().collect()])
}

/// `source[indices[i] mod source.length]`.
pub fn pick(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let source = sp(inputs, 0);
    let indices = sp(inputs, 1);
    if source.is_empty() || indices.is_empty() {
        return ok(vec![vec![]]);
    }
    let n = source.len() as f64;
    ok(vec![indices
        .iter()
        .map(|v| {
            let idx = num_d(v, 0.0).floor();
            get_f(source, ((idx % n) + n) % n)
        })
        .collect()])
}

/// Seeded Fisher–Yates over `hashUnit(i, 0, seed)`.
pub fn randomize(inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
    let mut out = sp(inputs, 0).to_vec();
    let seed = f64::from(js::to_int32(cx.seed(num_d(&first(inputs, 1), 0.0))));
    for i in (1..out.len()).rev() {
        let fi = i as f64;
        let j = js::min(fi, (hash_unit(fi, 0.0, seed) * (fi + 1.0)).floor()) as usize;
        out.swap(i, j);
    }
    ok(vec![out])
}

/// `count` samples of a sine wave.
pub fn sine(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let count = int_count(&first(inputs, 0));
    let amp = num_d(&first(inputs, 1), 1.0);
    let freq = num_d(&first(inputs, 2), 1.0);
    let phase = num_d(&first(inputs, 3), 0.0);
    let off = num_d(&first(inputs, 4), 0.0);
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    let n = array_len(count)?;
    let k = (PI * 2.0 * freq) / count;
    ok(vec![(0..n)
        .map(|i| Value::Num(off + amp * (k * i as f64 + phase).sin()))
        .collect()])
}

/// start, start·ratio, start·ratio², …
pub fn geometric(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let start = num_d(&first(inputs, 0), 1.0);
    let ratio = num_d(&first(inputs, 1), 2.0);
    let count = int_count(&first(inputs, 2));
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    let n = array_len(count)?;
    let mut v = start;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(Value::Num(v));
        v *= ratio;
    }
    ok(vec![out])
}

/// 1-D Perlin noise in [0, 1].
pub fn noise(inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
    let count = int_count(&first(inputs, 0));
    let freq = num_d(&first(inputs, 1), 1.0);
    let phase = num_d(&first(inputs, 2), 0.0);
    let seed = f64::from(js::to_int32(cx.seed(num_d(&first(inputs, 3), 0.0))));
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    let n = array_len(count)?;
    ok(vec![(0..n)
        .map(|i| {
            let x = phase + (i as f64 / js::max(1.0, count)) * freq;
            Value::Num(perlin2d(x, 0.0, seed))
        })
        .collect()])
}

// ─── Perlin (`src/nodes/random/noise.ts`) ────────────────────────────────────

const GRADIENTS_2D: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
    (-1.0, -1.0),
];

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn grad_dot(ix: f64, iy: f64, fx: f64, fy: f64, seed: f64) -> f64 {
    let g = GRADIENTS_2D[(hash32(ix, iy, seed) & 7) as usize];
    g.0 * fx + g.1 * fy
}

fn perlin2d(x: f64, y: f64, seed: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    let u = fade(fx);
    let v = fade(fy);
    let n00 = grad_dot(ix, iy, fx, fy, seed);
    let n10 = grad_dot(ix + 1.0, iy, fx - 1.0, fy, seed);
    let n01 = grad_dot(ix, iy + 1.0, fx, fy - 1.0, seed);
    let n11 = grad_dot(ix + 1.0, iy + 1.0, fx - 1.0, fy - 1.0, seed);
    let nx0 = lerp(n00, n10, u);
    let nx1 = lerp(n01, n11, u);
    lerp(nx0, nx1, v) * std::f64::consts::FRAC_1_SQRT_2 + 0.5
}

// ─── vec2-generators.ts ──────────────────────────────────────────────────────

fn v2(x: f64, y: f64) -> Value {
    Value::Vec2(Vec2 { x, y })
}

/// A `cols × rows` grid of points centred on `center`.
pub fn grid(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let cols = int_count(&first(inputs, 0));
    let rows = int_count(&first(inputs, 1));
    let w = num_d(&first(inputs, 2), 0.0);
    let h = num_d(&first(inputs, 3), 0.0);
    let c = to_vec2(&first(inputs, 4));
    if cols == 0.0 || rows == 0.0 {
        return ok(vec![vec![]]);
    }
    let total = array_len(cols * rows)?;
    let dx = if cols == 1.0 { 0.0 } else { w / (cols - 1.0) };
    let dy = if rows == 1.0 { 0.0 } else { h / (rows - 1.0) };
    let ox = c.x - w / 2.0;
    let oy = c.y - h / 2.0;
    let mut out = Vec::with_capacity(total);
    for r in 0..rows as usize {
        let y = oy + r as f64 * dy;
        for col in 0..cols as usize {
            out.push(v2(ox + col as f64 * dx, y));
        }
    }
    ok(vec![out])
}

/// `count` points evenly around a circle, plus their angles.
pub fn ring(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let count = int_count(&first(inputs, 0));
    let radius = num_d(&first(inputs, 1), 0.0);
    let c = to_vec2(&first(inputs, 2));
    let phase = num_d(&first(inputs, 3), 0.0);
    if count == 0.0 {
        return ok(vec![vec![], vec![]]);
    }
    let n = array_len(count)?;
    let step = (PI * 2.0) / count;
    let mut positions = Vec::with_capacity(n);
    let mut angles = Vec::with_capacity(n);
    for i in 0..n {
        let a = phase + step * i as f64;
        angles.push(Value::Num(a));
        positions.push(v2(c.x + a.cos() * radius, c.y + a.sin() * radius));
    }
    ok(vec![positions, angles])
}

/// Points along an Archimedean spiral.
pub fn spiral(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let count = int_count(&first(inputs, 0));
    let turns = num_d(&first(inputs, 1), 0.0);
    let radius = num_d(&first(inputs, 2), 0.0);
    let phase = num_d(&first(inputs, 3), 0.0);
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    let n = array_len(count)?;
    let total = PI * 2.0 * turns;
    ok(vec![(0..n)
        .map(|i| {
            let t = if count == 1.0 {
                0.0
            } else {
                i as f64 / (count - 1.0)
            };
            let a = phase + total * t;
            let r = radius * t;
            v2(a.cos() * r, a.sin() * r)
        })
        .collect()])
}

/// Sunflower spiral using the golden angle.
pub fn fibonacci(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let golden_angle = PI * (3.0 - 5f64.sqrt());
    let count = int_count(&first(inputs, 0));
    let scale = num_d(&first(inputs, 1), 1.0);
    let phase = num_d(&first(inputs, 2), 0.0);
    if count == 0.0 {
        return ok(vec![vec![]]);
    }
    let n = array_len(count)?;
    ok(vec![(0..n)
        .map(|i| {
            let a = phase + i as f64 * golden_angle;
            let r = scale * (i as f64).sqrt();
            v2(a.cos() * r, a.sin() * r)
        })
        .collect()])
}
