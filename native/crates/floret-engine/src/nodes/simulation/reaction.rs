//! `reaction`: Gray–Scott reaction–diffusion, the chemistry behind coral,
//! zebra stripes and fingerprints.
//!
//! Two chemicals on a grid `resolution` cells along the longer side of
//! `size` (wrapping at the edges): A is fed in at rate `feed`, B is
//! removed at `kill`, and where they meet A turns into B (A + 2B → 3B).
//! Both spread out, A at `rateA` and B at `rateB` (Karl Sims's
//! formulation: a 3×3 Laplacian, one unit of time per iteration). The
//! `preset` picks feed and kill for a known pattern; `custom` takes the
//! `feed` / `kill` pins. Each step runs `speed` iterations.
//!
//! B starts where the wired `seed image` is bright (luminance × alpha
//! over ½, the image stretched over `size`; read when the simulation
//! starts, or when it first arrives), or in a few small random spots; `paint` points keep adding B
//! round them (a mouse, a particle).
//!
//! It runs on the CPU at a modest resolution (a few hundred cells a side)
//! rather than on the GPU: the Simulation node hands its picture on as an
//! image, so it can be sampled, traced into contours or fed to effects.
//! The Shader node's feedback buffers (see the reaction–diffusion shader
//! sample) are the way to full-screen resolution.
//!
//! The image is B's concentration: with no `colors` white fading to
//! transparent, with one colour that colour, with several a ramp from the
//! first (no B) to the last.

use super::{ramp, sim_rng, Ins, Out, Sim};
use crate::image::Image;
use crate::scene::{RectF, SceneNode};
use crate::value::{Color, Vec2};
use std::sync::Arc;

const MAX_RES: usize = 512;
/// Most iterations a step.
const MAX_SPEED: usize = 40;
/// The presets, in dropdown order, with their feed and kill.
pub const PRESETS: [(&str, f64, f64); 8] = [
    ("coral", 0.0545, 0.062),
    ("mitosis", 0.0367, 0.0649),
    ("fingerprints", 0.037, 0.06),
    ("worms", 0.078, 0.061),
    ("maze", 0.029, 0.057),
    ("spots", 0.03, 0.062),
    ("waves", 0.014, 0.045),
    ("custom", 0.055, 0.062),
];

pub(crate) struct Reaction {
    seed: u64,
    res: usize,
    size: Vec2,
    w: usize,
    h: usize,
    a: Vec<f32>,
    b: Vec<f32>,
    na: Vec<f32>,
    nb: Vec<f32>,
    /// Whether a seed image was there at the start (one that arrives
    /// later, like a rasterised layer a frame on, starts it over).
    had_image: bool,
}

fn seed_image(ins: &Ins) -> Option<Arc<Image>> {
    crate::image::ready(&ins.value("seedImage"))
        .filter(|i| i.width() > 0 && i.height() > 0)
        .cloned()
}

fn res(ins: &Ins) -> usize {
    ins.count("resolution", 200, 16, MAX_RES)
}

fn size(ins: &Ins) -> Vec2 {
    ins.size(Vec2 { x: 600.0, y: 600.0 })
}

/// Feed and kill: the preset's, or the pins' for `custom`.
pub(crate) fn feed_kill(ins: &Ins) -> (f64, f64) {
    let preset = ins.str("preset");
    let p = PRESETS
        .iter()
        .find(|p| p.0 == preset)
        .unwrap_or(&PRESETS[0]);
    if p.0 == "custom" {
        (
            ins.num("feed", p.1).clamp(0.0, 0.2),
            ins.num("kill", p.2).clamp(0.0, 0.2),
        )
    } else {
        (p.1, p.2)
    }
}

impl Reaction {
    pub(crate) fn new(ins: &Ins) -> Reaction {
        let (seed, res, size) = (ins.seed(), res(ins), size(ins));
        let long = size.x.max(size.y);
        let w = ((size.x / long * res as f64).round() as usize).max(8);
        let h = ((size.y / long * res as f64).round() as usize).max(8);
        let mut r = Reaction {
            seed,
            res,
            size,
            w,
            h,
            a: vec![1.0; w * h],
            b: vec![0.0; w * h],
            na: vec![0.0; w * h],
            nb: vec![0.0; w * h],
            had_image: false,
        };
        let mut seeded = false;
        if let Some(img) = seed_image(ins) {
            r.had_image = true;
            let (iw, ih) = (img.width() as usize, img.height() as usize);
            for y in 0..h {
                for x in 0..w {
                    let px = ((x as f64 + 0.5) / w as f64 * iw as f64) as usize;
                    let py = ((y as f64 + 0.5) / h as f64 * ih as f64) as usize;
                    let [cr, cg, cb, ca] = img.pixel(px.min(iw - 1) as u32, py.min(ih - 1) as u32);
                    let l =
                        (0.2126 * f64::from(cr) + 0.7152 * f64::from(cg) + 0.0722 * f64::from(cb))
                            / 255.0
                            * f64::from(ca)
                            / 255.0;
                    if l > 0.5 {
                        r.b[y * w + x] = 1.0;
                        r.a[y * w + x] = 0.0;
                        seeded = true;
                    }
                }
            }
        }
        if !seeded {
            // A few small square spots, like a drop of B here and there.
            let mut rng = sim_rng(seed);
            let spots = 6 + (rng.next_u64() % 6) as usize;
            let half = (w.min(h) / 40).max(2) as isize;
            for _ in 0..spots {
                let cx = (w as f64 * (0.2 + 0.6 * rng.f64())) as isize;
                let cy = (h as f64 * (0.2 + 0.6 * rng.f64())) as isize;
                r.drop_b(cx, cy, half);
            }
        }
        r
    }

    /// B = 1 in a square of half-side `half` round cell (cx, cy).
    fn drop_b(&mut self, cx: isize, cy: isize, half: isize) {
        let (w, h) = (self.w as isize, self.h as isize);
        for dy in -half..=half {
            for dx in -half..=half {
                let x = (cx + dx).rem_euclid(w) as usize;
                let y = (cy + dy).rem_euclid(h) as usize;
                self.b[y * self.w + x] = 1.0;
            }
        }
    }

    /// One iteration (Karl Sims: Laplacian weights −1 centre, 0.2 sides,
    /// 0.05 corners; dt = 1).
    fn iterate(&mut self, f: f32, k: f32, da: f32, db: f32) {
        let (w, h) = (self.w, self.h);
        let (a, b): (&[f32], &[f32]) = (&self.a, &self.b);
        let (na, nb): (&mut [f32], &mut [f32]) = (&mut self.na, &mut self.nb);
        for y in 0..h {
            let up = (y + h - 1) % h * w;
            let row = y * w;
            let down = (y + 1) % h * w;
            for x in 0..w {
                let l = if x == 0 { w - 1 } else { x - 1 };
                let r = if x + 1 == w { 0 } else { x + 1 };
                let lap = |v: &[f32]| {
                    (v[up + x] + v[down + x] + v[row + l] + v[row + r]) * 0.2
                        + (v[up + l] + v[up + r] + v[down + l] + v[down + r]) * 0.05
                        - v[row + x]
                };
                let (av, bv) = (a[row + x], b[row + x]);
                let abb = av * bv * bv;
                na[row + x] = (av + da * lap(a) - abb + f * (1.0 - av)).clamp(0.0, 1.0);
                nb[row + x] = (bv + db * lap(b) + abb - (k + f) * bv).clamp(0.0, 1.0);
            }
        }
        std::mem::swap(&mut self.a, &mut self.na);
        std::mem::swap(&mut self.b, &mut self.nb);
    }
}

impl Sim for Reaction {
    fn stale(&self, ins: &Ins) -> bool {
        ins.seed() != self.seed
            || res(ins) != self.res
            || size(ins) != self.size
            || (!self.had_image && seed_image(ins).is_some())
    }

    fn step(&mut self, ins: &Ins) {
        let (f, k) = feed_kill(ins);
        let da = ins.num("rateA", 1.0).clamp(0.0, 1.0) as f32;
        let db = ins.num("rateB", 0.5).clamp(0.0, 1.0) as f32;
        let speed = ins.count("speed", 8, 0, MAX_SPEED);
        // Painting: B round each point (world units → cells).
        let half = (self.w.min(self.h) / 60).max(1) as isize;
        for p in ins.points("paint") {
            let x = ((p.x / self.size.x + 0.5) * self.w as f64).floor();
            let y = ((p.y / self.size.y + 0.5) * self.h as f64).floor();
            if x >= 0.0 && y >= 0.0 && (x as usize) < self.w && (y as usize) < self.h {
                self.drop_b(x as isize, y as isize, half);
            }
        }
        for _ in 0..speed {
            self.iterate(f as f32, k as f32, da, db);
        }
    }

    fn output(&self, ins: &Ins) -> Out {
        let stops = ins.colors();
        const LUT: usize = 256;
        let lut: Vec<[u8; 4]> = (0..LUT)
            .map(|i| {
                let t = i as f64 / (LUT - 1) as f64;
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
        let mut rgba = vec![0u8; self.b.len() * 4];
        for (i, &bv) in self.b.iter().enumerate() {
            // B rarely tops 0.4: stretch 0.05..0.35 over the ramp.
            let t = ((bv - 0.05) / 0.3).clamp(0.0, 1.0);
            let c = lut[((t * (LUT - 1) as f32).round() as usize).min(LUT - 1)];
            rgba[i * 4..i * 4 + 4].copy_from_slice(&c);
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
