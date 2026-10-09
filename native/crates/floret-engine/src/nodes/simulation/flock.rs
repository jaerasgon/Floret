//! `flocking`: Reynolds boids on a wrapping field.
//!
//! Each boid steers by its neighbours within `radius`: away from the ones
//! too close (separation, inside half the radius), toward their average
//! heading (alignment) and their centre (cohesion), and optionally toward
//! `target` (`attract`) — or, while `food` holds points, each toward the
//! food point nearest it (the shortest way round the field; every boid
//! against every point, so keep food to hundreds). Steering is limited to `maxSpeed` · 2 per second²,
//! speed to `maxSpeed` (and at least a third of it, so the flock keeps
//! moving). The field is `size`, centred on the origin; boids leaving one
//! side come back on the other. Neighbours come from a grid of
//! radius-sized cells, so the cost grows with the count, not its square.
//!
//! Changing `count` adds or drops boids without restarting; a new `seed`
//! starts over.

use super::{sim_rng, Ins, Out, Rng, Sim, Styles, STEP};
use crate::scene::SceneNode;
use crate::value::Vec2;
use std::f64::consts::TAU;
use std::sync::Arc;

const MAX_COUNT: usize = 5000;

pub(crate) struct Flock {
    seed: u64,
    rng: Rng,
    pos: Vec<Vec2>,
    vel: Vec<Vec2>,
    /// Cell-linked list, rebuilt every step.
    head: Vec<u32>,
    next: Vec<u32>,
}

const NONE: u32 = u32::MAX;

fn count(ins: &Ins) -> usize {
    ins.count("count", 250, 0, MAX_COUNT)
}

fn size(ins: &Ins) -> Vec2 {
    ins.size(Vec2 { x: 600.0, y: 600.0 })
}

fn max_speed(ins: &Ins) -> f64 {
    ins.num("maxSpeed", 140.0).max(0.0)
}

/// `v` scaled to length `len` (zero stays zero).
fn with_len(v: Vec2, len: f64) -> Vec2 {
    let m = v.x.hypot(v.y);
    if m > 1e-12 {
        Vec2 {
            x: v.x / m * len,
            y: v.y / m * len,
        }
    } else {
        Vec2::default()
    }
}

/// `v` shortened to at most `max`.
fn limit(v: Vec2, max: f64) -> Vec2 {
    if v.x.hypot(v.y) > max {
        with_len(v, max)
    } else {
        v
    }
}

/// Reynolds steering: the change from `vel` to `desired` at full speed.
fn steer(desired: Vec2, vel: Vec2, speed: f64, force: f64) -> Vec2 {
    let d = with_len(desired, speed);
    limit(
        Vec2 {
            x: d.x - vel.x,
            y: d.y - vel.y,
        },
        force,
    )
}

/// The shortest way from `p` to the nearest of `food` on a field of size
/// `s` that wraps.
pub(super) fn nearest_way(p: Vec2, food: &[Vec2], s: Vec2) -> Vec2 {
    let mut best = (f64::INFINITY, Vec2::default());
    for f in food {
        let d = Vec2 {
            x: wrap(f.x - p.x, s.x),
            y: wrap(f.y - p.y, s.y),
        };
        let d2 = d.x * d.x + d.y * d.y;
        if d2 < best.0 {
            best = (d2, d);
        }
    }
    best.1
}

/// `x` wrapped into `[-w/2, w/2)`.
fn wrap(x: f64, w: f64) -> f64 {
    (x + w / 2.0).rem_euclid(w) - w / 2.0
}

impl Flock {
    pub(crate) fn new(ins: &Ins) -> Flock {
        let seed = ins.seed();
        let mut f = Flock {
            seed,
            rng: sim_rng(seed),
            pos: Vec::new(),
            vel: Vec::new(),
            head: Vec::new(),
            next: Vec::new(),
        };
        f.resize(ins);
        f
    }

    /// Match `count`: new boids anywhere on the field, flying any way.
    fn resize(&mut self, ins: &Ins) {
        let n = count(ins);
        let s = size(ins);
        let speed = max_speed(ins);
        while self.pos.len() < n {
            let x = (self.rng.f64() - 0.5) * s.x;
            let y = (self.rng.f64() - 0.5) * s.y;
            let a = self.rng.f64() * TAU;
            let v = speed * (0.5 + 0.5 * self.rng.f64());
            self.pos.push(Vec2 { x, y });
            self.vel.push(Vec2 {
                x: a.cos() * v,
                y: a.sin() * v,
            });
        }
        self.pos.truncate(n);
        self.vel.truncate(n);
    }
}

impl Sim for Flock {
    fn stale(&self, ins: &Ins) -> bool {
        ins.seed() != self.seed
    }

    fn step(&mut self, ins: &Ins) {
        self.resize(ins);
        let s = size(ins);
        let speed = max_speed(ins);
        let force = speed * 2.0;
        let radius = ins.num("radius", 40.0).max(1.0);
        let (ws, wa, wc) = (
            ins.num("separation", 1.6),
            ins.num("alignment", 1.0),
            ins.num("cohesion", 0.9),
        );
        let attract = ins.num("attract", 0.0);
        let target = ins.vec2("target", Vec2::default());
        let food = ins.points("food");
        let n = self.pos.len();
        for p in &mut self.pos {
            p.x = wrap(p.x, s.x);
            p.y = wrap(p.y, s.y);
        }

        // Grid of cells at least `radius` wide; small grids search every
        // cell (each once).
        let cols = ((s.x / radius).floor() as usize).clamp(1, 256);
        let rows = ((s.y / radius).floor() as usize).clamp(1, 256);
        let (cw, ch) = (s.x / cols as f64, s.y / rows as f64);
        let cell = |p: Vec2| {
            let cx = (((p.x + s.x / 2.0) / cw) as usize).min(cols - 1);
            let cy = (((p.y + s.y / 2.0) / ch) as usize).min(rows - 1);
            (cx, cy)
        };
        self.head.clear();
        self.head.resize(cols * rows, NONE);
        self.next.clear();
        self.next.resize(n, NONE);
        for (i, p) in self.pos.iter().enumerate() {
            let (cx, cy) = cell(*p);
            let c = cy * cols + cx;
            self.next[i] = self.head[c];
            self.head[c] = i as u32;
        }
        let span = |c: usize, len: usize| -> ([usize; 3], usize) {
            match len {
                1 => ([0; 3], 1),
                2 => ([0, 1, 0], 2),
                _ => ([(c + len - 1) % len, c, (c + 1) % len], 3),
            }
        };

        let (r2, sep2) = (radius * radius, radius * radius * 0.25);
        let mut acc = vec![Vec2::default(); n];
        for (i, a) in acc.iter_mut().enumerate() {
            let p = self.pos[i];
            let (cx, cy) = cell(p);
            let mut sep = Vec2::default();
            let mut ali = Vec2::default();
            let mut coh = Vec2::default();
            let (mut n_sep, mut n_near) = (0, 0);
            let ((ys, ny), (xs, nx)) = (span(cy, rows), span(cx, cols));
            for &gy in &ys[..ny] {
                for &gx in &xs[..nx] {
                    let mut j = self.head[gy * cols + gx];
                    while j != NONE {
                        let k = j as usize;
                        j = self.next[k];
                        if k == i {
                            continue;
                        }
                        let q = self.pos[k];
                        // Shortest way round the wrapping field.
                        let dx = wrap(q.x - p.x, s.x);
                        let dy = wrap(q.y - p.y, s.y);
                        let d2 = dx * dx + dy * dy;
                        if d2 >= r2 {
                            continue;
                        }
                        n_near += 1;
                        ali.x += self.vel[k].x;
                        ali.y += self.vel[k].y;
                        coh.x += dx;
                        coh.y += dy;
                        if d2 < sep2 && d2 > 1e-12 {
                            // Away, weighted by 1 / distance.
                            sep.x -= dx / d2;
                            sep.y -= dy / d2;
                            n_sep += 1;
                        }
                    }
                }
            }
            let v = self.vel[i];
            let mut add = |f: Vec2, w: f64| {
                a.x += f.x * w;
                a.y += f.y * w;
            };
            if n_sep > 0 {
                add(steer(sep, v, speed, force), ws);
            }
            if n_near > 0 {
                add(steer(ali, v, speed, force), wa);
                add(steer(coh, v, speed, force), wc);
            }
            if attract != 0.0 {
                let to = if food.is_empty() {
                    Vec2 {
                        x: target.x - p.x,
                        y: target.y - p.y,
                    }
                } else {
                    nearest_way(p, &food, s)
                };
                add(steer(to, v, speed, force), attract);
            }
        }

        for ((v, p), a) in self.vel.iter_mut().zip(&mut self.pos).zip(&acc) {
            v.x += a.x * STEP;
            v.y += a.y * STEP;
            let m = v.x.hypot(v.y);
            if m > speed {
                *v = with_len(*v, speed);
            } else if m < speed / 3.0 {
                *v = if m > 1e-9 {
                    with_len(*v, speed / 3.0)
                } else {
                    Vec2 {
                        x: speed / 3.0,
                        y: 0.0,
                    }
                };
            }
            p.x = wrap(p.x + v.x * STEP, s.x);
            p.y = wrap(p.y + v.y * STEP, s.y);
        }
    }

    fn output(&self, ins: &Ins) -> Out {
        let styles = Styles::new(ins, true);
        let scale = ins.num("scale", 7.0).max(0.0);
        let n = self.pos.len();
        let mut out = Out::default();
        for (i, (p, v)) in self.pos.iter().zip(&self.vel).enumerate() {
            let a = v.y.atan2(v.x);
            let (c, s) = (a.cos(), a.sin());
            // A dart pointing along the velocity, centred on the boid.
            let tip = (p.x + c * scale, p.y + s * scale);
            let back = (p.x - c * scale * 0.6, p.y - s * scale * 0.6);
            let half = scale * 0.45;
            if scale > 0.0 {
                out.layer.push(Arc::new(SceneNode::Triangle {
                    pts: [
                        tip.0,
                        tip.1,
                        back.0 - s * half,
                        back.1 + c * half,
                        back.0 + s * half,
                        back.1 - c * half,
                    ],
                    style: styles.at(if n > 1 {
                        i as f64 / (n - 1) as f64
                    } else {
                        0.0
                    }),
                }));
            }
            out.points.push(*p);
            out.values.push(a);
            out.segments.push((
                *p,
                Vec2 {
                    x: p.x + v.x * 0.1,
                    y: p.y + v.y * 0.1,
                },
            ));
        }
        out
    }
}
