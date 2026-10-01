//! Landmark smoothing: MediaPipe's `LandmarksSmoothingCalculator` with a
//! one-euro filter, values scaled by the object's size so small and large
//! (near / far) people filter alike.

use super::geom::Point;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct OneEuroParams {
    pub min_cutoff: f64,
    pub beta: f64,
    pub derivate_cutoff: f64,
}

/// MediaPipe's pose and face landmark filter (the Tasks graphs configure
/// both alike): ~0.01 alpha when still, ~0.94 when moving fast.
pub const LANDMARKS: OneEuroParams = OneEuroParams {
    min_cutoff: 0.05,
    beta: 80.0,
    derivate_cutoff: 1.0,
};

#[derive(Clone, Copy, Default)]
struct LowPass {
    raw: Option<f64>,
    stored: f64,
}

impl LowPass {
    fn apply(&mut self, v: f64, alpha: f64) -> f64 {
        self.stored = if self.raw.is_some() {
            alpha * v + (1.0 - alpha) * self.stored
        } else {
            v
        };
        self.raw = Some(v);
        self.stored
    }
}

#[derive(Clone, Copy, Default)]
struct OneEuro {
    x: LowPass,
    dx: LowPass,
}

fn alpha(cutoff: f64, freq: f64) -> f64 {
    let te = 1.0 / freq;
    let tau = 1.0 / (2.0 * PI * cutoff);
    1.0 / (1.0 + tau / te)
}

impl OneEuro {
    fn apply(&mut self, p: &OneEuroParams, freq: f64, scale: f64, v: f64) -> f64 {
        let d = self.x.raw.map_or(0.0, |raw| (v - raw) * scale * freq);
        let ed = self.dx.apply(d, alpha(p.derivate_cutoff, freq));
        let cutoff = p.min_cutoff + p.beta * ed.abs();
        self.x.apply(v, alpha(cutoff, freq))
    }
}

/// Smooths one tracked object's landmarks over time.
pub struct Smoother {
    params: OneEuroParams,
    filters: Vec<[OneEuro; 3]>,
    last_t: Option<f64>,
    freq: f64,
}

impl Smoother {
    pub fn new(params: OneEuroParams) -> Smoother {
        Smoother {
            params,
            filters: Vec::new(),
            last_t: None,
            freq: 30.0,
        }
    }

    /// Filter `pts` (normalised coords) in place; `t` in seconds, `img` the
    /// frame size in pixels (filtering happens in pixel units). The object
    /// size is the landmarks' bounding box.
    pub fn apply(&mut self, t: f64, img: (f32, f32), pts: &mut [Point]) {
        self.apply_scaled(t, img, pts, None);
    }

    /// [`apply`](Self::apply) with the object size given in pixels (pose
    /// uses its ROI).
    pub fn apply_scaled(
        &mut self,
        t: f64,
        img: (f32, f32),
        pts: &mut [Point],
        object: Option<f32>,
    ) {
        if let Some(last) = self.last_t {
            if t <= last {
                return;
            }
            self.freq = 1.0 / (t - last);
        }
        self.last_t = Some(t);
        if self.filters.len() != pts.len() {
            self.filters = vec![Default::default(); pts.len()];
        }
        let (w, h) = (f64::from(img.0), f64::from(img.1));
        let object = object.map(f64::from).unwrap_or_else(|| {
            let b = super::geom::landmark_bounds(pts);
            ((f64::from(b.2 - b.0) * w) + (f64::from(b.3 - b.1) * h)) / 2.0
        });
        let scale = 1.0 / object.max(1e-6);
        for (p, f) in pts.iter_mut().zip(&mut self.filters) {
            // z is in x's units.
            let px = [
                f64::from(p[0]) * w,
                f64::from(p[1]) * h,
                f64::from(p[2]) * w,
            ];
            let out: Vec<f64> = (0..3)
                .map(|k| f[k].apply(&self.params, self.freq, scale, px[k]))
                .collect();
            *p = [
                (out[0] / w) as f32,
                (out[1] / h) as f32,
                (out[2] / w) as f32,
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_points_stay_put_and_jitter_is_damped() {
        let mut s = Smoother::new(LANDMARKS);
        let img = (1000.0, 1000.0);
        let base = [[0.1, 0.1, 0.0], [0.9, 0.9, 0.0]];
        for k in 0..10 {
            let mut p = base;
            s.apply(f64::from(k) / 30.0, img, &mut p);
            assert_eq!(p, base);
        }
        // A 1 px jitter on an 800 px object is mostly filtered out.
        let mut jitter = [[0.101, 0.1, 0.0], [0.9, 0.9, 0.0]];
        s.apply(10.0 / 30.0, img, &mut jitter);
        assert!(
            jitter[0][0] > 0.1 && jitter[0][0] < 0.1002,
            "{}",
            jitter[0][0]
        );
        // A big fast move mostly passes through.
        let mut moved = [[0.3, 0.1, 0.0], [1.1, 0.9, 0.0]];
        s.apply(11.0 / 30.0, img, &mut moved);
        assert!(moved[0][0] > 0.25, "{}", moved[0][0]);
        // Out-of-order timestamps pass values through untouched.
        let mut late = [[0.5, 0.5, 0.0], [0.6, 0.6, 0.0]];
        s.apply(0.0, img, &mut late);
        assert_eq!(late[0], [0.5, 0.5, 0.0]);
    }
}
