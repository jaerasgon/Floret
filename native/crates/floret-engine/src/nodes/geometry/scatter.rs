//! `geometry/scatter`: points spread over a rectangle or inside a shape.
//!
//! Modes: `poisson` (Bridson's Poisson-disc sampling: no two points closer
//! than `distance`), `random` (uniform), `jittered-grid` (one point per
//! grid cell, nudged by up to `jitter` of a cell), `halton` (the 2-3
//! Halton low-discrepancy sequence, shifted by the seed) and `relax` (the
//! wired `points`, spread out over the area).
//!
//! Every mode takes `relax`: that many passes of Lloyd relaxation (see
//! [`super::relax`]), which evens the points out over the area — random
//! points become an even, organic spread with an exact count.
//!
//! `stipple` draws `count` points whose density follows a `density` image
//! stretched over the area (darker = denser; `invert` for lighter; `gamma`
//! sharpens the contrast), then relaxes them by weighted Lloyd passes
//! (Secord's weighted Voronoi stippling): even, organic dots that shade
//! the picture.

use super::{
    int_pin, layer_contours, live_mode, num_pin, param_str, rng, vec2_pin, Pins, MAX_POINTS,
};
use crate::js::Mulberry32;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::text::outline::winding;
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

const MODE: &str = "poisson";
const SIZE: Vec2 = Vec2 { x: 400.0, y: 400.0 };
const DISTANCE: f64 = 24.0;
const COUNT: f64 = 200.0;
const SPACING: f64 = 24.0;
const STIPPLE_COUNT: f64 = 1500.0;
const STIPPLE_RELAX: f64 = 20.0;

/// Pins: the area (`center`, `size`, or a `shape` layer), then the mode's.
pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![
        vec2_pin("center", 0.0, 0.0),
        vec2_pin("size", SIZE.x, SIZE.y),
        PortSpec::new("shape", PortType::Layer).kind(PortKind::Spread),
    ];
    match param_str(params, "mode", MODE) {
        "random" | "halton" => v.push(int_pin("count", COUNT)),
        "jittered-grid" => {
            v.push(num_pin("spacing", SPACING));
            v.push(num_pin("jitter", 1.0));
        }
        "relax" => {
            v.push(PortSpec::new("points", PortType::Vec2).kind(PortKind::Spread));
            v.push(int_pin("relax", 10.0));
            return v;
        }
        "stipple" => {
            v.push(PortSpec::new("density", PortType::Image));
            v.push(int_pin("count", STIPPLE_COUNT));
            v.push(num_pin("gamma", 1.0));
            v.push(PortSpec::new("invert", PortType::Boolean).default(Value::Bool(false)));
            v.push(int_pin("relax", STIPPLE_RELAX));
            v.push(int_pin("seed", 1.0));
            return v;
        }
        _ => v.push(num_pin("distance", DISTANCE)),
    }
    v.push(int_pin("relax", 0.0));
    v.push(int_pin("seed", 1.0));
    v
}

/// Where points may land: a rectangle, or the filled area of a layer
/// (nonzero winding) inside its bounding box.
pub(super) struct Area {
    pub min: Vec2,
    pub max: Vec2,
    shape: Option<Vec<Vec<Vec2>>>,
}

impl Area {
    pub fn rect(center: Vec2, size: Vec2) -> Area {
        let (hw, hh) = (size.x.abs() / 2.0, size.y.abs() / 2.0);
        Area {
            min: Vec2 {
                x: center.x - hw,
                y: center.y - hh,
            },
            max: Vec2 {
                x: center.x + hw,
                y: center.y + hh,
            },
            shape: None,
        }
    }

    /// The closed contours of a layer; `None` when it has none.
    pub fn shape(layers: &[Value]) -> Option<Area> {
        let rings: Vec<Vec<Vec2>> = layer_contours(layers)
            .into_iter()
            .filter(|c| c.closed && c.pts.len() >= 3)
            .map(|c| c.pts)
            .collect();
        let (min, max) = crate::text::outline::bounds(&rings)?;
        Some(Area {
            min,
            max,
            shape: Some(rings),
        })
    }

    pub fn w(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn h(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// No shape: the whole rectangle.
    pub fn is_rect(&self) -> bool {
        self.shape.is_none()
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && self.shape.as_ref().is_none_or(|s| winding(s, p) != 0)
    }

    fn random_point(&self, r: &mut Mulberry32) -> Vec2 {
        Vec2 {
            x: self.min.x + r.next_f64() * self.w(),
            y: self.min.y + r.next_f64() * self.h(),
        }
    }
}

/// What `stipple` mode's points depend on: the image (by identity), the
/// shape layers (by identity, as the memo hands them on unchanged), the
/// numbers and the (patch-mixed) seed.
fn stipple_key(pins: &Pins, inputs: &[Spread], seed: f64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match crate::image::ready(&pins.value(inputs, "density")) {
        Some(img) => img.id().hash(&mut h),
        None => u64::MAX.hash(&mut h),
    }
    for v in pins.spread(inputs, "shape") {
        if let Value::Scene(n) = v {
            (Arc::as_ptr(n) as usize).hash(&mut h);
        }
    }
    let c = pins.vec2(inputs, "center", Vec2::default());
    let z = pins.vec2(inputs, "size", SIZE);
    for n in [
        c.x,
        c.y,
        z.x,
        z.y,
        pins.num(inputs, "count", STIPPLE_COUNT),
        pins.num(inputs, "gamma", 1.0),
        pins.num(inputs, "relax", STIPPLE_RELAX),
        crate::seed::mix(seed),
    ] {
        n.to_bits().hash(&mut h);
    }
    pins.bool(inputs, "invert", false).hash(&mut h);
    h.finish()
}

pub(super) fn scatter(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    let mut stippled: Option<(u64, Spread)> = None;
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let shape = pins.spread(inputs, "shape");
        let area = if shape.iter().any(|v| matches!(v, Value::Scene(_))) {
            match Area::shape(shape) {
                Some(a) => a,
                None => return Ok(vec![crate::value::empty()]),
            }
        } else {
            Area::rect(
                pins.vec2(inputs, "center", Vec2::default()),
                pins.vec2(inputs, "size", SIZE),
            )
        };
        let seed = pins.num(inputs, "seed", 1.0);
        let count = || {
            let n = pins.num(inputs, "count", COUNT).max(0.0) as usize;
            (n.min(MAX_POINTS), n > MAX_POINTS)
        };
        let (pts, capped) = match mode.as_str() {
            "random" => {
                let (n, capped) = count();
                (random(&area, n, seed), capped)
            }
            "halton" => {
                let (n, capped) = count();
                (halton(&area, n, seed), capped)
            }
            "jittered-grid" => {
                let spacing = pins.num(inputs, "spacing", SPACING);
                let jitter = pins.num(inputs, "jitter", 1.0).clamp(0.0, 1.0);
                jittered_grid(&area, spacing, jitter, seed)
            }
            "relax" => (pins.points(inputs, "points"), false),
            "stipple" => {
                // An image pin keeps the node out of the memo: reuse the
                // last points while nothing they depend on changed.
                let key = stipple_key(&pins, inputs, seed);
                if let Some((k, out)) = &stippled {
                    if *k == key {
                        return Ok(vec![out.clone()]);
                    }
                }
                let n = pins.num(inputs, "count", STIPPLE_COUNT).max(0.0) as usize;
                let density = crate::image::ready(&pins.value(inputs, "density")).map(|img| {
                    Density::new(
                        img,
                        &area,
                        pins.num(inputs, "gamma", 1.0),
                        pins.bool(inputs, "invert", false),
                    )
                });
                let n = n.min(MAX_POINTS);
                let pts = match &density {
                    Some(d) => d.sample(&area, n, seed),
                    None => random(&area, n, seed),
                };
                let passes = pins
                    .num(inputs, "relax", STIPPLE_RELAX)
                    .clamp(0.0, super::relax::MAX_ITERATIONS) as usize;
                let weight = density.as_ref().map(|d| move |p: Vec2| d.at(p));
                let pts = super::relax::weighted_lloyd(
                    &area,
                    pts,
                    passes,
                    weight.as_ref().map(|w| w as &dyn Fn(Vec2) -> f64),
                );
                if pins.num(inputs, "count", STIPPLE_COUNT) > MAX_POINTS as f64 {
                    cx.warn(format!("Stopped at {MAX_POINTS} points."));
                }
                let out: Spread = Arc::from(pts.into_iter().map(Value::Vec2).collect::<Vec<_>>());
                stippled = Some((key, out.clone()));
                return Ok(vec![out]);
            }
            _ => poisson(&area, pins.num(inputs, "distance", DISTANCE), seed),
        };
        let fallback = if mode == "relax" { 10.0 } else { 0.0 };
        let passes = pins
            .num(inputs, "relax", fallback)
            .clamp(0.0, super::relax::MAX_ITERATIONS) as usize;
        let pts = super::relax::lloyd(&area, pts, passes);
        if capped {
            cx.warn(format!("Stopped at {MAX_POINTS} points."));
        }
        Ok(vec![Arc::from(
            pts.into_iter().map(Value::Vec2).collect::<Vec<_>>(),
        )])
    })
}

/// Where points gather in `stipple` mode: an image's darkness (or
/// lightness, `invert`) × alpha, raised to `gamma`, stretched over the
/// area's bounds. Kept per pixel, read nearest.
pub(super) struct Density {
    w: Vec<f32>,
    cols: usize,
    rows: usize,
    min: Vec2,
    /// Pixels per unit.
    sx: f64,
    sy: f64,
    max: f32,
}

impl Density {
    pub fn new(img: &crate::image::Image, area: &Area, gamma: f64, invert: bool) -> Density {
        let (cols, rows) = (img.width() as usize, img.height() as usize);
        let gamma = if gamma.is_finite() && gamma > 0.0 {
            gamma
        } else {
            1.0
        };
        let w: Vec<f32> = img
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let l = (0.2126 * f64::from(p[0])
                    + 0.7152 * f64::from(p[1])
                    + 0.0722 * f64::from(p[2]))
                    / 255.0;
                let ink = if invert { l } else { 1.0 - l };
                (ink * f64::from(p[3]) / 255.0).clamp(0.0, 1.0).powf(gamma) as f32
            })
            .collect();
        let max = w.iter().copied().fold(0.0_f32, f32::max);
        Density {
            w,
            cols,
            rows,
            min: area.min,
            sx: cols as f64 / area.w().max(1e-9),
            sy: rows as f64 / area.h().max(1e-9),
            max,
        }
    }

    /// The weight at `p` (0 outside the image).
    pub fn at(&self, p: Vec2) -> f64 {
        let i = ((p.x - self.min.x) * self.sx).floor();
        let j = ((p.y - self.min.y) * self.sy).floor();
        if !(i >= 0.0 && j >= 0.0 && (i as usize) < self.cols && (j as usize) < self.rows) {
            return 0.0;
        }
        f64::from(self.w[j as usize * self.cols + i as usize])
    }

    /// `count` points drawn with probability ∝ the weight (rejection
    /// sampled; uniform when the image is blank).
    pub fn sample(&self, area: &Area, count: usize, seed: f64) -> Vec<Vec2> {
        if self.max <= 0.0 {
            return random(area, count, seed);
        }
        let mut r = rng(seed);
        let mut out = Vec::with_capacity(count);
        let mut tries = 0usize;
        let limit = count.saturating_mul(400).max(10_000);
        while out.len() < count && tries < limit {
            tries += 1;
            let p = area.random_point(&mut r);
            if area.contains(p) && r.next_f64() * f64::from(self.max) < self.at(p) {
                out.push(p);
            }
        }
        out
    }
}

/// Uniform points (rejection sampled inside a shape).
pub(super) fn random(area: &Area, count: usize, seed: f64) -> Vec<Vec2> {
    let mut r = rng(seed);
    let mut out = Vec::with_capacity(count);
    let mut tries = 0usize;
    while out.len() < count && tries < count.saturating_mul(100).max(1000) {
        tries += 1;
        let p = area.random_point(&mut r);
        if area.contains(p) {
            out.push(p);
        }
    }
    out
}

/// The radical inverse of `i` in `base`.
fn radical_inverse(mut i: u64, base: u64) -> f64 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= base as f64;
        r += f * (i % base) as f64;
        i /= base;
    }
    r
}

/// Halton (2, 3) points, toroidally shifted by the seed (Cranley–Patterson
/// rotation, which keeps them evenly spread).
pub(super) fn halton(area: &Area, count: usize, seed: f64) -> Vec<Vec2> {
    let mut r = rng(seed);
    let (ox, oy) = (r.next_f64(), r.next_f64());
    let mut out = Vec::with_capacity(count);
    let mut i = 1u64;
    let limit = (count as u64).saturating_mul(100).max(1000);
    while out.len() < count && i <= limit {
        let u = (radical_inverse(i, 2) + ox).fract();
        let v = (radical_inverse(i, 3) + oy).fract();
        i += 1;
        let p = Vec2 {
            x: area.min.x + u * area.w(),
            y: area.min.y + v * area.h(),
        };
        if area.contains(p) {
            out.push(p);
        }
    }
    out
}

/// One point per `spacing` cell, centred on the area, each moved up to
/// `jitter` × half a cell. Returns whether the point cap was hit.
pub(super) fn jittered_grid(
    area: &Area,
    spacing: f64,
    jitter: f64,
    seed: f64,
) -> (Vec<Vec2>, bool) {
    if spacing.is_nan() || spacing <= 0.0 {
        return (Vec::new(), false);
    }
    let cols = (area.w() / spacing).floor().max(1.0);
    let rows = (area.h() / spacing).floor().max(1.0);
    let capped = cols * rows > MAX_POINTS as f64;
    let (cols, rows) = if capped {
        let k = (MAX_POINTS as f64 / (cols * rows)).sqrt();
        ((cols * k).floor().max(1.0), (rows * k).floor().max(1.0))
    } else {
        (cols, rows)
    };
    let x0 = (area.min.x + area.max.x) / 2.0 - (cols - 1.0) * spacing / 2.0;
    let y0 = (area.min.y + area.max.y) / 2.0 - (rows - 1.0) * spacing / 2.0;
    let mut r = rng(seed);
    let mut out = Vec::new();
    for j in 0..rows as usize {
        for i in 0..cols as usize {
            let p = Vec2 {
                x: x0 + i as f64 * spacing + (r.next_f64() - 0.5) * jitter * spacing,
                y: y0 + j as f64 * spacing + (r.next_f64() - 0.5) * jitter * spacing,
            };
            if area.contains(p) {
                out.push(p);
            }
        }
    }
    (out, capped)
}

/// Bridson's Poisson-disc sampling: no two points closer than `d`. Starts
/// again from a fresh random point until those stop fitting, so every
/// separate part of a shape fills. Returns whether the point cap was hit.
pub(super) fn poisson(area: &Area, d: f64, seed: f64) -> (Vec<Vec2>, bool) {
    const K: usize = 30;
    const RESTARTS: usize = 200;
    if d.is_nan() || d <= 0.0 {
        return (Vec::new(), false);
    }
    // Keep the background grid (and the output) bounded.
    let fits = area.w() * area.h() / (d * d);
    let d = if fits > MAX_POINTS as f64 * 2.0 {
        (area.w() * area.h() / (MAX_POINTS as f64 * 2.0)).sqrt()
    } else {
        d
    };
    let cell = d / std::f64::consts::SQRT_2;
    let gw = ((area.w() / cell).ceil() as usize).max(1);
    let gh = ((area.h() / cell).ceil() as usize).max(1);
    let mut grid: Vec<u32> = vec![u32::MAX; gw * gh];
    let at = |p: Vec2| {
        let i = (((p.x - area.min.x) / cell) as usize).min(gw - 1);
        let j = (((p.y - area.min.y) / cell) as usize).min(gh - 1);
        (i, j)
    };
    let mut r = rng(seed);
    let mut out: Vec<Vec2> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let far_enough = |out: &[Vec2], grid: &[u32], p: Vec2| {
        let (i, j) = at(p);
        for gj in j.saturating_sub(2)..(j + 3).min(gh) {
            for gi in i.saturating_sub(2)..(i + 3).min(gw) {
                let k = grid[gj * gw + gi];
                if k != u32::MAX && super::dist(out[k as usize], p) < d {
                    return false;
                }
            }
        }
        true
    };
    let mut restarts = 0;
    while restarts < RESTARTS && out.len() < MAX_POINTS {
        restarts += 1;
        let p = area.random_point(&mut r);
        if !area.contains(p) || !far_enough(&out, &grid, p) {
            continue;
        }
        let (i, j) = at(p);
        grid[j * gw + i] = out.len() as u32;
        active.push(out.len());
        out.push(p);
        while !active.is_empty() {
            let slot = ((r.next_f64() * active.len() as f64) as usize).min(active.len() - 1);
            let base = out[active[slot]];
            let mut placed = false;
            for _ in 0..K {
                let a = r.next_f64() * std::f64::consts::TAU;
                let rad = d * (1.0 + r.next_f64());
                let q = Vec2 {
                    x: base.x + a.cos() * rad,
                    y: base.y + a.sin() * rad,
                };
                if area.contains(q) && far_enough(&out, &grid, q) {
                    let (i, j) = at(q);
                    grid[j * gw + i] = out.len() as u32;
                    active.push(out.len());
                    out.push(q);
                    placed = true;
                    if out.len() >= MAX_POINTS {
                        return (out, true);
                    }
                    break;
                }
            }
            if !placed {
                active.swap_remove(slot);
            }
        }
    }
    (out, fits > MAX_POINTS as f64 * 2.0)
}
