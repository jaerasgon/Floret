//! Canvas motion: the animated camera fit (React Flow's `fitView` with
//! `duration: 300`) and the spring a node settles in with when it appears
//! (`.node { animation: node-appear var(--dur-slow) var(--ease-spring) }`).
//! Plain functions of time so they test without a window.

use eframe::egui::{vec2, Vec2};

/// `fitView({ duration: 300 })`.
pub const FIT_SECS: f64 = 0.3;
/// `--dur-slow`.
pub const APPEAR_SECS: f64 = 0.26;

/// CSS `cubic-bezier(x1, y1, x2, y2)` at progress `t` (0–1).
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let coord = |a: f64, b: f64, s: f64| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    // Solve x(s) = t for s by bisection (x is monotonic for x1, x2 in 0–1).
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if coord(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    coord(y1, y2, (lo + hi) / 2.0)
}

/// `--ease-spring`: a small overshoot.
pub fn spring(t: f64) -> f64 {
    cubic_bezier(0.34, 1.4, 0.5, 1.0, t)
}

/// d3's default transition ease (`easeCubicInOut`).
pub fn cubic_in_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0) * 2.0;
    if t <= 1.0 {
        t * t * t / 2.0
    } else {
        let t = t - 2.0;
        (t * t * t + 2.0) / 2.0
    }
}

/// A camera move from one (pan, zoom) to another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAnim {
    pub from: (Vec2, f32),
    pub to: (Vec2, f32),
    pub start: f64,
    /// The canvas size the move was planned for.
    pub size: Vec2,
}

impl CameraAnim {
    /// The camera at time `now`, and whether the move is finished. The
    /// point at the centre of the view travels in a straight line while
    /// the zoom changes geometrically, so nothing swings sideways.
    pub fn at(&self, now: f64) -> ((Vec2, f32), bool) {
        let t = ((now - self.start) / FIT_SECS).clamp(0.0, 1.0);
        if t >= 1.0 {
            return (self.to, true);
        }
        let e = cubic_in_out(t) as f32;
        let half = self.size * 0.5;
        let centre = |(pan, zoom): (Vec2, f32)| half / zoom - pan;
        let (c0, c1) = (centre(self.from), centre(self.to));
        let z0 = self.from.1.max(1e-3).ln();
        let z1 = self.to.1.max(1e-3).ln();
        let zoom = (z0 + (z1 - z0) * e).exp();
        let c = c0 + (c1 - c0) * e;
        ((half / zoom - c, zoom), false)
    }
}

/// How a node `age` seconds old is drawn while it settles in: (opacity,
/// downward offset in canvas units, scale). `None` once it has landed.
pub fn appear(age: f64) -> Option<(f32, f32, f32)> {
    if !(0.0..APPEAR_SECS).contains(&age) {
        return None;
    }
    // node-appear: from { opacity 0; translateY(5px) scale(0.985) }.
    let e = spring(age / APPEAR_SECS) as f32;
    Some((e.clamp(0.0, 1.0), 5.0 * (1.0 - e), 0.985 + 0.015 * e))
}

/// The selected node's slow gold breath (`node-breathe`, 3.6 s): 0 at
/// rest, 1 at the peak.
pub fn breathe(time: f64) -> f32 {
    let phase = (time / 3.6).fract();
    // ease-std in and out of the 50% keyframe.
    let half = if phase < 0.5 {
        phase * 2.0
    } else {
        (1.0 - phase) * 2.0
    };
    cubic_bezier(0.4, 0.0, 0.2, 1.0, half) as f32
}

/// Translation for scaling by `s` around `centre`, then moving down `dy`
/// (screen units): `p' = s·p + t`.
pub fn settle_offset(centre: eframe::egui::Pos2, s: f32, dy: f32) -> Vec2 {
    centre.to_vec2() * (1.0 - s) + vec2(0.0, dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_endpoints_and_overshoot() {
        assert!(cubic_bezier(0.4, 0.0, 0.2, 1.0, 0.0).abs() < 1e-9);
        assert!((cubic_bezier(0.4, 0.0, 0.2, 1.0, 1.0) - 1.0).abs() < 1e-9);
        // Linear control points give back t.
        assert!((cubic_bezier(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 0.3) - 0.3).abs() < 1e-6);
        // The spring passes 1 before settling.
        let peak = (1..100)
            .map(|i| spring(f64::from(i) / 100.0))
            .fold(0.0, f64::max);
        assert!(peak > 1.02, "{peak}");
        assert!((cubic_in_out(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn camera_moves_between_the_two_views() {
        let a = CameraAnim {
            from: (vec2(0.0, 0.0), 1.0),
            to: (vec2(-100.0, 50.0), 0.5),
            start: 10.0,
            size: vec2(800.0, 600.0),
        };
        assert_eq!(a.at(10.0).0, a.from);
        assert!(!a.at(10.1).1);
        assert_eq!(a.at(10.0 + FIT_SECS), (a.to, true));
        let ((_, z), _) = a.at(10.15);
        assert!(z < 1.0 && z > 0.5);
    }

    #[test]
    fn nodes_settle_in() {
        let (o, dy, s) = appear(0.0).unwrap();
        assert!(o < 1e-6 && (dy - 5.0).abs() < 1e-5 && (s - 0.985).abs() < 1e-6);
        assert!(appear(APPEAR_SECS).is_none());
        let (o, _, _) = appear(APPEAR_SECS * 0.6).unwrap();
        assert!(o > 0.9);
        assert!(breathe(0.0) < 1e-3 && (breathe(1.8) - 1.0).abs() < 1e-3);
        let t = settle_offset(eframe::egui::pos2(100.0, 0.0), 0.5, 2.0);
        assert_eq!(t, vec2(50.0, 2.0));
    }
}
