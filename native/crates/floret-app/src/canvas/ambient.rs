//! The ambient "living canvas" (`AmbientFlow.tsx`): a faint drift of ink
//! along a slow flow field behind the graph, so a quiet patch never feels
//! dead. Screen-space, fading out as the patch fills up. With reduced
//! motion it lays down one still field and stops.
//!
//! The web app paints onto a canvas it fades toward the background each
//! frame; here each particle keeps its recent path, and older segments are
//! drawn fainter by the same factor.

use crate::theme::GOLD;
use eframe::egui::{self, pos2, Color32, Mesh, Painter, Pos2, Rect, Shape, Vec2};
use std::collections::VecDeque;

const PARTICLES: usize = 150;
/// Per-frame fade toward the background, and the stroke's own alpha.
const FADE: f32 = 0.055;
const INK: f32 = 0.07;
/// Segments kept per particle (older ones are invisible).
const TRAIL: usize = 56;
const WIDTH: f32 = 1.1;
const SPEED: f32 = 1.15;
/// Steps laid down for the still field.
const STILL_STEPS: usize = 260;

/// The layered-sine flow field: an angle at (x, y) and time `t` (ms).
pub fn field(x: f32, y: f32, t: f32) -> f32 {
    std::f32::consts::TAU
        * ((x * 0.0009 + t * 0.00022).sin()
            + (y * 0.0011 - t * 0.00016).cos()
            + ((x + y) * 0.0006 + t * 0.00030).sin())
}

/// Backdrop opacity for a patch of `nodes` nodes: ~0.85 empty, ~0.17 busy.
pub fn presence(nodes: usize) -> f32 {
    let t = ((nodes as f32 - 3.0) / 26.0).clamp(0.0, 1.0);
    0.85 - t * 0.68
}

struct Particle {
    trail: VecDeque<Pos2>,
    life: f32,
}

#[derive(Default)]
pub struct Ambient {
    parts: Vec<Particle>,
    size: Vec2,
    seed: u32,
    /// The still field has been laid down (reduced motion).
    still: bool,
}

impl Ambient {
    fn rand(&mut self) -> f32 {
        // xorshift32
        let mut x = if self.seed == 0 {
            0x9e37_79b9
        } else {
            self.seed
        };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn spawn(&mut self, life: f32) -> Particle {
        let p = pos2(self.rand() * self.size.x, self.rand() * self.size.y);
        Particle {
            trail: VecDeque::from([p]),
            life,
        }
    }

    fn reset(&mut self, size: Vec2) {
        self.size = size;
        self.still = false;
        self.parts.clear();
        for _ in 0..PARTICLES {
            let life = self.rand() * 220.0;
            let p = self.spawn(life);
            self.parts.push(p);
        }
    }

    /// Advance every particle one step along the field at `t` (ms).
    fn step(&mut self, t: f32) {
        let (w, h) = (self.size.x, self.size.y);
        for i in 0..self.parts.len() {
            let head = *self.parts[i].trail.back().expect("never empty");
            let a = field(head.x, head.y, t);
            let next = head + Vec2::angled(a) * SPEED;
            let part = &mut self.parts[i];
            part.trail.push_back(next);
            if part.trail.len() > TRAIL + 1 {
                part.trail.pop_front();
            }
            part.life -= 1.0;
            let out = next.x < -2.0 || next.x > w + 2.0 || next.y < -2.0 || next.y > h + 2.0;
            if part.life < 0.0 || out {
                let life = 120.0 + self.rand() * 220.0;
                self.parts[i] = self.spawn(life);
            }
        }
    }

    /// Paint the backdrop over `area` (call once a frame, before the grid).
    pub fn paint(&mut self, painter: &Painter, area: Rect, nodes: usize, time: f64, still: bool) {
        if self.parts.is_empty() || (self.size - area.size()).length() > 0.5 {
            self.reset(area.size());
        }
        if still {
            if !self.still {
                for i in 0..STILL_STEPS {
                    self.step(i as f32 * 16.0);
                }
                self.still = true;
            }
        } else {
            self.still = false;
            self.step((time * 1000.0) as f32);
        }
        let ink = *GOLD;
        let presence = presence(nodes);
        let mut mesh = Mesh::default();
        for part in &self.parts {
            let n = part.trail.len();
            for (k, seg) in part.trail.iter().collect::<Vec<_>>().windows(2).enumerate() {
                // `age` frames since this segment was drawn.
                let age = (n - 2 - k) as i32;
                let alpha = INK * (1.0 - FADE).powi(age) * presence;
                if alpha < 0.002 {
                    continue;
                }
                let color = Color32::from_rgba_unmultiplied(
                    ink.r(),
                    ink.g(),
                    ink.b(),
                    (alpha * 255.0).round() as u8,
                );
                quad(
                    &mut mesh,
                    area.min + seg[0].to_vec2(),
                    area.min + seg[1].to_vec2(),
                    color,
                );
            }
        }
        painter.with_clip_rect(area).add(Shape::mesh(mesh));
    }
}

/// A thin line segment as two triangles.
fn quad(mesh: &mut Mesh, a: Pos2, b: Pos2, color: Color32) {
    let d = b - a;
    let len = d.length();
    if len < 1e-3 {
        return;
    }
    let n = egui::vec2(-d.y, d.x) / len * (WIDTH / 2.0);
    let i = mesh.vertices.len() as u32;
    for p in [a + n, a - n, b - n, b + n] {
        mesh.colored_vertex(p, color);
    }
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quieter_as_the_patch_fills() {
        assert!((presence(0) - 0.85).abs() < 1e-6);
        assert!((presence(29) - 0.17).abs() < 1e-6);
        assert!(presence(10) < presence(3));
    }

    #[test]
    fn particles_drift_and_stay_on_screen() {
        let mut a = Ambient::default();
        a.reset(egui::vec2(400.0, 300.0));
        for i in 0..500 {
            a.step(i as f32 * 16.0);
        }
        assert_eq!(a.parts.len(), PARTICLES);
        for p in &a.parts {
            assert!(p.trail.len() <= TRAIL + 1);
            let h = p.trail.back().unwrap();
            assert!(h.x >= -2.0 && h.x <= 402.0 && h.y >= -2.0 && h.y <= 302.0);
        }
        assert!(field(10.0, 20.0, 0.0).is_finite());
    }
}
