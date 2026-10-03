//! `physarum`: slime-mould agents on a trail map (after Jeff Jones, 2010).
//!
//! The map is a grid `resolution` cells along the longer side of `size`.
//! Every step each agent smells the trail `sensorDistance` cells ahead and
//! `sensorAngle`° to either side, turns `turnAngle`° toward the strongest
//! (randomly when both sides beat the middle), moves `stepSize` cells and
//! drops `deposit` where it lands (the trail saturates at 1). Then the map
//! blurs (3×3, mixed in by `diffuse`) and fades (`decay`, the fraction lost
//! per step). `food` points keep the trail full around them, so the
//! network grows between them. The grid wraps at its edges.
//!
//! The map comes out as an image, drawn over `size` on the layer: with no
//! `colors` white fading to transparent, with one colour that colour, with
//! several a ramp from the first (empty) to the last (dense).

use super::{ramp, sim_rng, Ins, Out, Rng, Sim};
use crate::image::Image;
use crate::scene::{RectF, SceneNode};
use crate::value::{Color, Vec2};
use std::f32::consts::TAU;
use std::sync::Arc;

const MAX_AGENTS: usize = 200_000;
const MAX_RES: usize = 1024;

pub(crate) struct Physarum {
    seed: u64,
    agents: usize,
    res: usize,
    size: Vec2,
    rng: Rng,
    w: usize,
    h: usize,
    x: Vec<f32>,
    y: Vec<f32>,
    /// Unit heading.
    dx: Vec<f32>,
    dy: Vec<f32>,
    trail: Vec<f32>,
    scratch: Vec<f32>,
}

fn agents(ins: &Ins) -> usize {
    ins.count("agents", 20000, 0, MAX_AGENTS)
}

fn res(ins: &Ins) -> usize {
    ins.count("resolution", 256, 8, MAX_RES)
}

fn size(ins: &Ins) -> Vec2 {
    ins.size(Vec2 { x: 600.0, y: 600.0 })
}

/// `v` wrapped into `[0, len)` (assumes it is at most one length out).
fn wrap(v: f32, len: f32) -> f32 {
    let v = if v < 0.0 {
        v + len
    } else if v >= len {
        v - len
    } else {
        v
    };
    // Rounding can land exactly on `len`.
    if v >= len || v < 0.0 {
        0.0
    } else {
        v
    }
}

/// The trail under a point in cells. (The hot loops index plain slices:
/// that stays cheap in unoptimised builds too.)
fn sense(trail: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let cx = wrap(x, w as f32) as usize;
    let cy = wrap(y, h as f32) as usize;
    trail[cy * w + cx]
}

/// Trail density lookup steps.
const LUT_LEN: usize = 256;

impl Physarum {
    pub(crate) fn new(ins: &Ins) -> Physarum {
        let (seed, n, res, size) = (ins.seed(), agents(ins), res(ins), size(ins));
        let long = size.x.max(size.y);
        let w = ((size.x / long * res as f64).round() as usize).max(8);
        let h = ((size.y / long * res as f64).round() as usize).max(8);
        let mut rng = sim_rng(seed);
        // A disc of agents in the middle, facing every way.
        let r = 0.35 * w.min(h) as f32;
        let (mut x, mut y) = (Vec::with_capacity(n), Vec::with_capacity(n));
        let (mut dx, mut dy) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for _ in 0..n {
            let d = r * rng.f32().sqrt();
            let a = rng.f32() * TAU;
            x.push(wrap(w as f32 / 2.0 + d * a.cos(), w as f32));
            y.push(wrap(h as f32 / 2.0 + d * a.sin(), h as f32));
            let h = rng.f32() * TAU;
            dx.push(h.cos());
            dy.push(h.sin());
        }
        Physarum {
            seed,
            agents: n,
            res,
            size,
            rng,
            w,
            h,
            x,
            y,
            dx,
            dy,
            trail: vec![0.0; w * h],
            scratch: vec![0.0; w * h],
        }
    }

    /// Fill the cells within `r` of a world-space point.
    fn feed(&mut self, p: Vec2, r: i64) {
        let gx = ((p.x / self.size.x + 0.5) * self.w as f64).floor() as i64;
        let gy = ((p.y / self.size.y + 0.5) * self.h as f64).floor() as i64;
        let (w, h) = (self.w as i64, self.h as i64);
        if gx < -r || gy < -r || gx >= w + r || gy >= h + r {
            return;
        }
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let (cx, cy) = ((gx + dx).rem_euclid(w), (gy + dy).rem_euclid(h));
                self.trail[(cy * w + cx) as usize] = 1.0;
            }
        }
    }
}

impl Sim for Physarum {
    fn stale(&self, ins: &Ins) -> bool {
        ins.seed() != self.seed
            || agents(ins) != self.agents
            || res(ins) != self.res
            || size(ins) != self.size
    }

    fn step(&mut self, ins: &Ins) {
        let sa = ins.num("sensorAngle", 22.5).to_radians() as f32;
        let (w, h) = (self.w as f32, self.h as f32);
        let so = (ins.num("sensorDistance", 9.0) as f32).clamp(-w.min(h) / 2.0, w.min(h) / 2.0);
        let ra = ins.num("turnAngle", 45.0).to_radians() as f32;
        let ss = (ins.num("stepSize", 1.0) as f32).clamp(-8.0, 8.0);
        let deposit = ins.num("deposit", 0.3) as f32;
        let decay = ins.num("decay", 0.08).clamp(0.0, 1.0) as f32;
        let diffuse = ins.num("diffuse", 0.5).clamp(0.0, 1.0) as f32;

        // Sense, turn, move, deposit. Turns rotate the heading vector.
        let (cs, ss_) = (sa.cos(), sa.sin());
        let (cr, sr) = (ra.cos(), ra.sin());
        let (gw, gh) = (self.w, self.h);
        let trail: &mut [f32] = &mut self.trail;
        let (xs, ys): (&mut [f32], &mut [f32]) = (&mut self.x, &mut self.y);
        let (dxs, dys): (&mut [f32], &mut [f32]) = (&mut self.dx, &mut self.dy);
        for i in 0..xs.len() {
            let (x, y) = (xs[i], ys[i]);
            let (dx, dy) = (dxs[i], dys[i]);
            let f = sense(trail, gw, gh, x + dx * so, y + dy * so);
            let l = sense(
                trail,
                gw,
                gh,
                x + (dx * cs + dy * ss_) * so,
                y + (dy * cs - dx * ss_) * so,
            );
            let r = sense(
                trail,
                gw,
                gh,
                x + (dx * cs - dy * ss_) * so,
                y + (dy * cs + dx * ss_) * so,
            );
            let turn = if f > l && f > r {
                0.0
            } else if f < l && f < r {
                if self.rng.f32() < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            } else if l < r {
                1.0
            } else if r < l {
                -1.0
            } else {
                0.0
            };
            let (mut nx, mut ny) = (dx, dy);
            if turn != 0.0 {
                let s = sr * turn;
                (nx, ny) = (dx * cr - dy * s, dy * cr + dx * s);
                // Keep it a unit vector.
                let m = (nx * nx + ny * ny).sqrt();
                (nx, ny) = (nx / m, ny / m);
            }
            let px = wrap(x + nx * ss, w);
            let py = wrap(y + ny * ss, h);
            xs[i] = px;
            ys[i] = py;
            dxs[i] = nx;
            dys[i] = ny;
            // The trail saturates at 1, so crowded veins stop pulling in more.
            let t = &mut trail[py as usize * gw + px as usize];
            *t = (*t + deposit).clamp(0.0, 1.0);
        }
        for p in ins.points("food") {
            self.feed(p, 3);
        }

        // Blur (two 3-tap passes, wrapping), mix it in, fade.
        let trail: &mut [f32] = &mut self.trail;
        let tmp: &mut [f32] = &mut self.scratch;
        for row in 0..gh {
            let o = row * gw;
            tmp[o] = trail[o + gw - 1] + trail[o] + trail[o + 1];
            for c in 1..gw - 1 {
                tmp[o + c] = trail[o + c - 1] + trail[o + c] + trail[o + c + 1];
            }
            tmp[o + gw - 1] = trail[o + gw - 2] + trail[o + gw - 1] + trail[o];
        }
        let keep = 1.0 - decay;
        for row in 0..gh {
            let up = (row + gh - 1) % gh * gw;
            let down = (row + 1) % gh * gw;
            let o = row * gw;
            for c in 0..gw {
                let blur = (tmp[up + c] + tmp[o + c] + tmp[down + c]) / 9.0;
                let t = trail[o + c];
                trail[o + c] = (t + (blur - t) * diffuse) * keep;
            }
        }
    }

    fn output(&self, ins: &Ins) -> Out {
        let stops = ins.colors();
        // Trail (0..1) → RGBA.
        let lut: Vec<[u8; 4]> = (0..LUT_LEN)
            .map(|i| {
                let t = i as f64 / (LUT_LEN - 1) as f64;
                let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                let c = match stops.as_slice() {
                    [] => Color {
                        a: t,
                        ..Color::WHITE
                    },
                    [c] => Color { a: c.a * t, ..*c },
                    _ => ramp(&stops, t),
                };
                [byte(c.r), byte(c.g), byte(c.b), byte(c.a)]
            })
            .collect();
        let lut: &[[u8; 4]] = &lut;
        let trail: &[f32] = &self.trail;
        let mut rgba = vec![0u8; trail.len() * 4];
        let px: &mut [u8] = &mut rgba;
        for i in 0..trail.len() {
            // `as` saturates: NaN and negatives land on 0.
            let c = lut[((trail[i] * (LUT_LEN - 1) as f32 + 0.5) as usize).min(LUT_LEN - 1)];
            let o = i * 4;
            px[o] = c[0];
            px[o + 1] = c[1];
            px[o + 2] = c[2];
            px[o + 3] = c[3];
        }
        let image = Arc::new(Image::new(self.w as u32, self.h as u32, rgba));
        Out {
            layer: vec![Arc::new(SceneNode::Image {
                image: image.clone(),
                src: None,
                dst: RectF {
                    x: -self.size.x / 2.0,
                    y: -self.size.y / 2.0,
                    w: self.size.x,
                    h: self.size.y,
                },
                tint: None,
            })],
            image: Some(image),
            ..Out::default()
        }
    }
}
