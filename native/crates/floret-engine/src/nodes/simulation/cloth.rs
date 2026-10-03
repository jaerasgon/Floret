//! `cloth`: a Verlet spring mesh.
//!
//! `cols × rows` points span `size`, centred on `pos`, joined to their
//! right and lower neighbours by springs at their rest length. `pin` holds
//! the whole top row (`top`), its two ends (`corners`), the left column
//! (`left`) or nothing (`none`); pinned points follow `pos`. Every step the
//! free points keep their velocity (less `damping`), fall with `gravity`
//! and are pushed by `wind` (rippling over the cloth and in time), then
//! the springs are relaxed `iterations` times toward their rest length by
//! `stiffness` (0..1). With `tear` above 0 a spring stretched past `tear`
//! × its rest length breaks. While `grab` is held, the point nearest
//! `pointer` (when the grab starts, within 60) follows the pointer.
//!
//! A one-column or one-row cloth is a rope. `values` is each point's
//! stretch (its springs' mean length / rest − 1), which also picks the
//! `colors` ramp for the springs (0 at rest, 1 at a quarter longer).

use super::{line, Ins, Out, Sim, Styles, STEP};
use crate::value::Vec2;

const MAX_SIDE: usize = 200;
const GRAB_RADIUS: f64 = 60.0;

struct Link {
    a: usize,
    b: usize,
    rest: f64,
    alive: bool,
}

pub(crate) struct Cloth {
    cols: usize,
    rows: usize,
    size: Vec2,
    pin: String,
    pos: Vec<Vec2>,
    prev: Vec<Vec2>,
    /// Rest offset from `pos` of each point.
    rest: Vec<Vec2>,
    pinned: Vec<bool>,
    links: Vec<Link>,
    grabbed: Option<usize>,
    was_grab: bool,
    time: f64,
}

fn dims(ins: &Ins) -> (usize, usize) {
    (
        ins.count("cols", 28, 1, MAX_SIDE),
        ins.count("rows", 18, 1, MAX_SIDE),
    )
}

fn size(ins: &Ins) -> Vec2 {
    ins.size(Vec2 { x: 420.0, y: 270.0 })
}

fn pin(ins: &Ins) -> String {
    match ins.str("pin").as_str() {
        p @ ("corners" | "left" | "none") => p.to_owned(),
        _ => "top".to_owned(),
    }
}

fn origin(ins: &Ins) -> Vec2 {
    ins.vec2("pos", Vec2::default())
}

fn sub(a: Vec2, b: Vec2) -> Vec2 {
    Vec2 {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn len(v: Vec2) -> f64 {
    v.x.hypot(v.y)
}

impl Cloth {
    pub(crate) fn new(ins: &Ins) -> Cloth {
        let (cols, rows) = dims(ins);
        let (size, pin, o) = (size(ins), pin(ins), origin(ins));
        let at = |c: usize, r: usize| Vec2 {
            x: if cols > 1 {
                (c as f64 / (cols - 1) as f64 - 0.5) * size.x
            } else {
                0.0
            },
            y: if rows > 1 {
                (r as f64 / (rows - 1) as f64 - 0.5) * size.y
            } else {
                -size.y / 2.0
            },
        };
        let mut rest = Vec::with_capacity(cols * rows);
        let mut pinned = Vec::with_capacity(cols * rows);
        let mut links = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                rest.push(at(c, r));
                pinned.push(match pin.as_str() {
                    "top" => r == 0,
                    "corners" => r == 0 && (c == 0 || c == cols - 1),
                    "left" => c == 0,
                    _ => false,
                });
                let i = r * cols + c;
                let mut link = |b: usize, rc: (usize, usize)| {
                    links.push(Link {
                        a: i,
                        b,
                        rest: len(sub(at(rc.0, rc.1), at(c, r))),
                        alive: true,
                    })
                };
                if c + 1 < cols {
                    link(i + 1, (c + 1, r));
                }
                if r + 1 < rows {
                    link(i + cols, (c, r + 1));
                }
            }
        }
        let pos: Vec<Vec2> = rest
            .iter()
            .map(|p| Vec2 {
                x: o.x + p.x,
                y: o.y + p.y,
            })
            .collect();
        Cloth {
            cols,
            rows,
            size,
            pin,
            prev: pos.clone(),
            pos,
            rest,
            pinned,
            links,
            grabbed: None,
            was_grab: false,
            time: 0.0,
        }
    }

    fn nearest(&self, p: Vec2) -> Option<usize> {
        self.pos
            .iter()
            .enumerate()
            .map(|(i, q)| (len(sub(*q, p)), i))
            .filter(|(d, _)| *d <= GRAB_RADIUS)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i)| i)
    }

    /// Points held in place this step: pins at `pos`, the grabbed point
    /// at the pointer.
    fn hold(&mut self, o: Vec2, pointer: Vec2) {
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                self.pos[i] = Vec2 {
                    x: o.x + self.rest[i].x,
                    y: o.y + self.rest[i].y,
                };
            }
        }
        if let Some(g) = self.grabbed {
            // Let go, it keeps the pointer's speed.
            self.prev[g] = self.pos[g];
            self.pos[g] = pointer;
        }
    }

    fn fixed(&self, i: usize) -> bool {
        self.pinned[i] || self.grabbed == Some(i)
    }
}

impl Sim for Cloth {
    fn stale(&self, ins: &Ins) -> bool {
        dims(ins) != (self.cols, self.rows) || size(ins) != self.size || pin(ins) != self.pin
    }

    fn step(&mut self, ins: &Ins) {
        let o = origin(ins);
        let gravity = ins.vec2("gravity", Vec2 { x: 0.0, y: 600.0 });
        let wind = ins.vec2("wind", Vec2::default());
        let stiffness = ins.num("stiffness", 1.0).clamp(0.0, 1.0);
        let iterations = ins.count("iterations", 8, 1, 64);
        let keep = 1.0 - ins.num("damping", 0.01).clamp(0.0, 1.0);
        let tear = ins.num("tear", 0.0);
        let pointer = ins.vec2("pointer", Vec2::default());
        let grab = ins.flag("grab");
        if grab && !self.was_grab {
            self.grabbed = self.nearest(pointer);
        } else if !grab {
            self.grabbed = None;
        }
        self.was_grab = grab;
        self.time += STEP;

        let h2 = STEP * STEP;
        for i in 0..self.pos.len() {
            if self.fixed(i) {
                continue;
            }
            let (p, q) = (self.pos[i], self.prev[i]);
            // Gusts rolling across the cloth.
            let (c, r) = ((i % self.cols) as f64, (i / self.cols) as f64);
            let gust = 0.6 + 0.4 * (self.time * 2.3 + c * 0.35 + r * 0.2).sin();
            self.prev[i] = p;
            self.pos[i] = Vec2 {
                x: p.x + (p.x - q.x) * keep + (gravity.x + wind.x * gust) * h2,
                y: p.y + (p.y - q.y) * keep + (gravity.y + wind.y * gust) * h2,
            };
        }
        self.hold(o, pointer);

        for _ in 0..iterations {
            for k in 0..self.links.len() {
                let Link { a, b, rest, alive } = self.links[k];
                if !alive {
                    continue;
                }
                let d = sub(self.pos[b], self.pos[a]);
                let l = len(d);
                if l < 1e-9 {
                    continue;
                }
                if tear > 0.0 && l > rest * tear {
                    self.links[k].alive = false;
                    continue;
                }
                let (fa, fb) = (self.fixed(a), self.fixed(b));
                let share = match (fa, fb) {
                    (true, true) => continue,
                    (true, false) | (false, true) => 1.0,
                    (false, false) => 0.5,
                };
                let f = (l - rest) / l * stiffness * share;
                if !fa {
                    self.pos[a].x += d.x * f;
                    self.pos[a].y += d.y * f;
                }
                if !fb {
                    self.pos[b].x -= d.x * f;
                    self.pos[b].y -= d.y * f;
                }
            }
        }
        // Pins move with `pos` without gaining speed.
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                self.prev[i] = self.pos[i];
            }
        }
    }

    fn output(&self, ins: &Ins) -> Out {
        let styles = Styles::new(ins, false);
        let mut stretch = vec![0.0; self.pos.len()];
        let mut links = vec![0u32; self.pos.len()];
        let mut out = Out::default();
        for l in self.links.iter().filter(|l| l.alive) {
            let (a, b) = (self.pos[l.a], self.pos[l.b]);
            let s = if l.rest > 0.0 {
                len(sub(b, a)) / l.rest - 1.0
            } else {
                0.0
            };
            for i in [l.a, l.b] {
                stretch[i] += s;
                links[i] += 1;
            }
            out.layer.push(line(a, b, styles.at(s * 4.0)));
            out.segments.push((a, b));
        }
        out.points = self.pos.clone();
        out.values = stretch
            .iter()
            .zip(&links)
            .map(|(s, n)| if *n > 0 { s / f64::from(*n) } else { 0.0 })
            .collect();
        out
    }
}
