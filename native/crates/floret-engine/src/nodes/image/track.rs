//! `image/track-points` (Point Tracker): follow points from frame to frame
//! of a moving image (a camera, a video, a rendered layer) by pyramidal
//! Lucas–Kanade optical flow.
//!
//! Each new frame (a new image; repeats of the same frame are skipped) is
//! turned into a grey pyramid. Every point is tracked from the previous
//! frame to this one over a 15 × 15 window, coarse level first, so it can
//! move up to about `window` pixels a frame; then tracked back again. The
//! `confidence` (0..1) is the normalised cross-correlation of the two
//! windows, cut down when the way back doesn't return to the start or the
//! window has too little texture to lock onto. A point whose track fails
//! (off the image, nothing to lock onto) stays where it was, at
//! confidence 0. Each point also remembers how it looked when its track
//! started and pulls every frame's result onto that look, so small
//! frame-to-frame errors don't add up into drift; when the look changes
//! for good (it turns, the light moves) it takes a new one.
//!
//! Points are in canvas units like the other image nodes: (0, 0) is the
//! image's centre, y down, one unit a pixel. The tracks start at the
//! `points` wired in, and start there again while `reset` is on, when the
//! number of points changes, when the image changes size, or when the
//! points are edited (the tracks survive other edits of the patch, see
//! [`NodeImpl::carry`]).

use super::super::geometry::Pins;
use crate::image::{ready, Image};
use crate::node::{CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

/// Half the tracking window (15 × 15 pixels).
const HALF: i32 = 7;
/// Most Lucas–Kanade iterations per level.
const ITERATIONS: usize = 20;
/// Most pyramid levels (a reach of about 7 × 2⁵ px).
const MAX_LEVELS: usize = 5;
/// Most points tracked.
pub const MAX_POINTS: usize = 256;
const WINDOW: f64 = 40.0;

/// A grey image (0..1), row-major.
#[derive(Clone, Debug)]
pub(crate) struct Grey {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Grey {
    pub fn of(img: &Image) -> Grey {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let px = img
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let l =
                    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
                l * f32::from(p[3]) / (255.0 * 255.0)
            })
            .collect();
        Grey { w, h, px }
    }

    /// Half the size, each pixel the mean of a 2 × 2 block.
    fn half(&self) -> Grey {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let at = |x: usize, y: usize| self.px[y.min(self.h - 1) * self.w + x.min(self.w - 1)];
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (x2, y2) = (2 * x, 2 * y);
                px.push((at(x2, y2) + at(x2 + 1, y2) + at(x2, y2 + 1) + at(x2 + 1, y2 + 1)) / 4.0);
            }
        }
        Grey { w, h, px }
    }

    /// Bilinear sample at pixel coordinates (centres at integers), edges
    /// clamped.
    fn at(&self, x: f64, y: f64) -> f64 {
        let x = x.clamp(0.0, (self.w - 1) as f64);
        let y = y.clamp(0.0, (self.h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let p = |x: usize, y: usize| f64::from(self.px[y * self.w + x]);
        let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * fx;
        let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * fx;
        top + (bottom - top) * fy
    }

    fn inside(&self, x: f64, y: f64) -> bool {
        x >= 0.0 && y >= 0.0 && x <= (self.w - 1) as f64 && y <= (self.h - 1) as f64
    }
}

/// A frame as a pyramid, finest first.
#[derive(Clone, Debug)]
pub(crate) struct Pyramid(Vec<Grey>);

impl Pyramid {
    pub fn new(base: Grey, levels: usize) -> Pyramid {
        let mut v = vec![base];
        while v.len() < levels {
            let last = &v[v.len() - 1];
            if last.w < 2 * (HALF as usize + 2) || last.h < 2 * (HALF as usize + 2) {
                break;
            }
            let next = last.half();
            v.push(next);
        }
        Pyramid(v)
    }

    fn size(&self) -> (usize, usize) {
        (self.0[0].w, self.0[0].h)
    }
}

/// Levels for a reach of `window` pixels a frame.
fn levels_for(window: f64) -> usize {
    let reach = f64::from(HALF);
    if window.is_nan() || window <= reach {
        return 1;
    }
    ((window / reach).log2().ceil() as usize + 1).clamp(1, MAX_LEVELS)
}

/// A window of a picture round a point, ready to look for: its pixels'
/// offsets, values and gradients, and their structure tensor.
#[derive(Clone, Debug)]
pub(crate) struct Template {
    win: Vec<(f64, f64, f64, f64, f64)>,
    gxx: f64,
    gxy: f64,
    gyy: f64,
    det: f64,
    /// The tensor's smaller eigenvalue per pixel: how much texture there
    /// is to lock onto.
    pub min_eig: f64,
}

impl Template {
    /// The window round `c` (pixel coordinates) of `img`; `None` when it
    /// has no texture at all.
    pub fn new(img: &Grey, c: Vec2) -> Option<Template> {
        let n = ((2 * HALF + 1) * (2 * HALF + 1)) as usize;
        let mut win = Vec::with_capacity(n);
        let (mut gxx, mut gxy, mut gyy) = (0.0, 0.0, 0.0);
        for dy in -HALF..=HALF {
            for dx in -HALF..=HALF {
                let (dx, dy) = (f64::from(dx), f64::from(dy));
                let (x, y) = (c.x + dx, c.y + dy);
                let ix = (img.at(x + 1.0, y) - img.at(x - 1.0, y)) / 2.0;
                let iy = (img.at(x, y + 1.0) - img.at(x, y - 1.0)) / 2.0;
                gxx += ix * ix;
                gxy += ix * iy;
                gyy += iy * iy;
                win.push((dx, dy, img.at(x, y), ix, iy));
            }
        }
        let det = gxx * gyy - gxy * gxy;
        if det.abs() < 1e-12 {
            return None;
        }
        let tr = gxx + gyy;
        let min_eig = (tr - (tr * tr - 4.0 * det).max(0.0).sqrt()) / 2.0 / n as f64;
        Some(Template {
            win,
            gxx,
            gxy,
            gyy,
            det,
            min_eig,
        })
    }

    /// Normalised cross-correlation of the window with `img` round `q`
    /// (−1..1; 0 for flat windows).
    pub fn ncc(&self, img: &Grey, q: Vec2) -> f64 {
        let n = self.win.len() as f64;
        let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(dx, dy, u, _, _) in &self.win {
            let w = img.at(q.x + dx, q.y + dy);
            sa += u;
            sb += w;
            saa += u * u;
            sbb += w * w;
            sab += u * w;
        }
        let va = saa - sa * sa / n;
        let vb = sbb - sb * sb / n;
        if va <= 1e-9 || vb <= 1e-9 {
            return 0.0;
        }
        (sab - sa * sb / n) / (va * vb).sqrt()
    }

    /// Lucas–Kanade on one level: where in `img` the window lies, looking
    /// from `start` (pixel coordinates of its centre).
    pub fn find(&self, img: &Grey, start: Vec2) -> Option<Vec2> {
        let mut q = start;
        for _ in 0..ITERATIONS {
            let (mut bx, mut by) = (0.0, 0.0);
            for &(dx, dy, i0, ix, iy) in &self.win {
                let d = i0 - img.at(q.x + dx, q.y + dy);
                bx += d * ix;
                by += d * iy;
            }
            let ex = (self.gyy * bx - self.gxy * by) / self.det;
            let ey = (self.gxx * by - self.gxy * bx) / self.det;
            if !(ex.is_finite() && ey.is_finite()) {
                return None;
            }
            q.x += ex;
            q.y += ey;
            if ex * ex + ey * ey < 1e-4 {
                break;
            }
        }
        Some(q)
    }
}

/// Track `p` (pixel coordinates) from `a` to `b`, coarse level first:
/// where it went, and how much texture its window had (see
/// [`Template::min_eig`]). `None` when it can't be tracked.
fn lucas_kanade(a: &Pyramid, b: &Pyramid, p: Vec2) -> Option<(Vec2, f64)> {
    let levels = a.0.len().min(b.0.len());
    // The motion so far, in this level's pixels.
    let mut g = Vec2::default();
    let mut texture = 0.0;
    for l in (0..levels).rev() {
        let k = f64::from(1u32 << l);
        let c = Vec2 {
            x: p.x / k,
            y: p.y / k,
        };
        let t = Template::new(&a.0[l], c)?;
        texture = t.min_eig;
        let q = t.find(
            &b.0[l],
            Vec2 {
                x: c.x + g.x,
                y: c.y + g.y,
            },
        )?;
        let f = if l > 0 { 2.0 } else { 1.0 };
        g = Vec2 {
            x: (q.x - c.x) * f,
            y: (q.y - c.y) * f,
        };
    }
    let q = Vec2 {
        x: p.x + g.x,
        y: p.y + g.y,
    };
    (q.x.is_finite() && q.y.is_finite() && b.0[0].inside(q.x, q.y)).then_some((q, texture))
}

/// One point from frame `a` to `b`: its new place and the confidence.
pub(crate) fn track(a: &Pyramid, b: &Pyramid, p: Vec2) -> Option<(Vec2, f64)> {
    if !a.0[0].inside(p.x, p.y) {
        return None;
    }
    let (q, texture) = lucas_kanade(a, b, p)?;
    // Forward–backward: the way back should come home.
    let fb =
        lucas_kanade(b, a, q).map_or(f64::INFINITY, |(r, _)| super::super::geometry::dist(p, r));
    let corr = Template::new(&a.0[0], p).map_or(0.0, |t| t.ncc(&b.0[0], q).max(0.0));
    let home = 1.0 / (1.0 + (fb / 2.0).powi(2));
    // Below ~1e-5 per pixel (grey 0..1) there's little to lock onto.
    let texture = (texture / 1e-5).min(1.0);
    Some((q, (corr * home * texture).clamp(0.0, 1.0)))
}

/// How far (pixels) the anchored look may move a frame-to-frame track.
const ANCHOR_PULL: f64 = 2.5;
/// Below this match with its anchor, a well-tracked point takes a new one
/// (its look has changed: it turned, the light moved).
const ANCHOR_KEEP: f64 = 0.75;

/// A tracked point: where it is, how sure, and the look it started with
/// (re-taken when the look changes), which stops slow drift: each
/// frame's track is pulled onto that look.
#[derive(Clone, Debug)]
pub(crate) struct Track {
    pub at: Vec2,
    pub confidence: f64,
    anchor: Option<Template>,
}

impl Track {
    fn new(at: Vec2, frame: &Arc<Pyramid>) -> Track {
        let mut t = Track {
            at,
            confidence: 1.0,
            anchor: None,
        };
        t.anchor_on(frame);
        t
    }

    fn anchor_on(&mut self, frame: &Arc<Pyramid>) {
        let base = &frame.0[0];
        self.anchor = (base.inside(self.at.x, self.at.y))
            .then(|| Template::new(base, self.at))
            .flatten();
    }

    /// Follow the point from `prev` into `next`.
    fn step(&mut self, prev: &Arc<Pyramid>, next: &Arc<Pyramid>) {
        let Some((q, conf)) = track(prev, next, self.at) else {
            self.confidence = 0.0;
            return;
        };
        let cur = &next.0[0];
        let mut at = q;
        let mut stale = true;
        if let Some(tpl) = &self.anchor {
            if let Some(r) = tpl.find(cur, q) {
                let close = super::super::geometry::dist(q, r) <= ANCHOR_PULL;
                if close && cur.inside(r.x, r.y) && tpl.ncc(cur, r) >= ANCHOR_KEEP {
                    at = r;
                    stale = false;
                }
            }
        }
        self.at = at;
        self.confidence = conf;
        if stale && conf >= ANCHOR_KEEP {
            self.anchor_on(next);
        }
    }
}

/// What a Point Tracker keeps between frames (and across recompiles).
#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    /// The last frame's pyramid and its image id.
    frame: Option<(u64, Arc<Pyramid>)>,
    /// Tracked points (pixel coordinates).
    tracks: Vec<Track>,
    /// The seeds the tracks started from (canvas units).
    seeds: Vec<Vec2>,
    /// Rebuilt after an edit: start again if the seeds changed.
    edited: bool,
}

impl State {
    /// One tick on `img` (`None`: no picture yet) with `seeds` in canvas
    /// units. Returns the points in canvas units and their confidence.
    pub fn step(
        &mut self,
        img: Option<&Arc<Image>>,
        seeds: &[Vec2],
        reset: bool,
        window: f64,
    ) -> (Vec<Vec2>, Vec<f64>) {
        let edited = std::mem::take(&mut self.edited);
        let Some(img) = img.filter(|i| i.width() > 1 && i.height() > 1) else {
            // Nothing to track on: the seeds, unconfirmed.
            return (seeds.to_vec(), vec![0.0; seeds.len()]);
        };
        let (w, h) = (f64::from(img.width()), f64::from(img.height()));
        let to_px = |p: &Vec2| Vec2 {
            x: p.x + w / 2.0,
            y: p.y + h / 2.0,
        };
        let size_changed = self
            .frame
            .as_ref()
            .is_some_and(|(_, f)| f.size() != (img.width() as usize, img.height() as usize));
        let reseed = reset
            || size_changed
            || self.frame.is_none()
            || seeds.len() != self.seeds.len()
            || (edited && seeds != self.seeds.as_slice());
        let levels = levels_for(window);
        let new_frame = self.frame.as_ref().is_none_or(|(id, _)| *id != img.id());
        if reseed {
            let frame = Arc::new(Pyramid::new(Grey::of(img), levels));
            self.seeds = seeds.to_vec();
            self.tracks = seeds.iter().map(|s| Track::new(to_px(s), &frame)).collect();
            self.frame = Some((img.id(), frame));
        } else if new_frame {
            let next = Arc::new(Pyramid::new(Grey::of(img), levels));
            if let Some((_, prev)) = &self.frame {
                for t in &mut self.tracks {
                    t.step(prev, &next);
                }
            }
            self.frame = Some((img.id(), next));
        }
        let out = self
            .tracks
            .iter()
            .map(|t| Vec2 {
                x: t.at.x - w / 2.0,
                y: t.at.y - h / 2.0,
            })
            .collect();
        (out, self.tracks.iter().map(|t| t.confidence).collect())
    }
}

struct Tracker {
    pins: Pins,
    state: State,
}

pub(super) fn track_points(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Tracker {
        pins: Pins::new(init),
        state: State::default(),
    })
}

impl NodeImpl for Tracker {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let pins = &self.pins;
        let mut seeds = pins.points(inputs, "points");
        if seeds.len() > MAX_POINTS {
            cx.warn(format!("Tracking the first {MAX_POINTS} points."));
            seeds.truncate(MAX_POINTS);
        }
        if cx.tick_frame.frame_count == 0.0 {
            self.state = State::default();
        }
        let image = pins.value(inputs, "image");
        let (pts, conf) = self.state.step(
            ready(&image),
            &seeds,
            pins.bool(inputs, "reset", false),
            pins.num(inputs, "window", WINDOW),
        );
        Ok(vec![
            Arc::from(pts.into_iter().map(Value::Vec2).collect::<Vec<_>>()),
            Arc::from(conf.into_iter().map(Value::Num).collect::<Vec<_>>()),
        ])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(s) = state.downcast::<State>() {
            self.state = *s;
            self.state.edited = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w × h` frame of soft blobs (a texture LK can lock onto) moved
    /// by `(ox, oy)` pixels.
    /// A grey texture of scattered blobs of a few sizes (nothing repeats
    /// to confuse a match), `BASE` pixels square.
    const BASE: usize = 1000;

    fn base() -> &'static Grey {
        static B: std::sync::OnceLock<Grey> = std::sync::OnceLock::new();
        B.get_or_init(|| {
            let mut r = crate::js::Mulberry32::new(7);
            let mut px = vec![0.5f32; BASE * BASE];
            for _ in 0..2500 {
                let (bx, by) = (r.next_f64() * BASE as f64, r.next_f64() * BASE as f64);
                let s = 3.0 + r.next_f64() * 9.0;
                let a = r.next_f64() - 0.5;
                let reach = (3.0 * s) as isize;
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let (x, y) = (bx as isize + dx, by as isize + dy);
                        if x < 0 || y < 0 || x as usize >= BASE || y as usize >= BASE {
                            continue;
                        }
                        let d2 = (x as f64 - bx).powi(2) + (y as f64 - by).powi(2);
                        px[y as usize * BASE + x as usize] +=
                            (a * (-d2 / (2.0 * s * s)).exp()) as f32;
                    }
                }
            }
            Grey {
                w: BASE,
                h: BASE,
                px,
            }
        })
    }

    /// A `w × h` view of the texture moved by `(ox, oy)` pixels.
    fn frame(w: u32, h: u32, ox: f64, oy: f64) -> Arc<Image> {
        noisy(w, h, ox, oy, 0.0, 0)
    }

    /// [`frame`] with ± `noise` (0..1) of sensor noise, drawn from `seed`.
    fn noisy(w: u32, h: u32, ox: f64, oy: f64, noise: f64, seed: u32) -> Arc<Image> {
        let b = base();
        let mut r = crate::js::Mulberry32::new(seed);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = b.at(f64::from(x) - ox + 150.0, f64::from(y) - oy + 150.0)
                    + noise * (2.0 * r.next_f64() - 1.0);
                let c = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                px.extend([c, c, c, 255]);
            }
        }
        Arc::new(Image::new(w, h, px))
    }

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    #[test]
    fn follows_a_moving_pattern() {
        let mut s = State::default();
        let seeds = [v(0.0, 0.0), v(-40.0, 30.0)];
        let (p, c) = s.step(Some(&frame(160, 120, 0.0, 0.0)), &seeds, false, 40.0);
        assert_eq!(p, seeds);
        assert_eq!(c, [1.0, 1.0]);
        // Drift right and down, a few pixels a frame (sub-pixel too).
        let mut at = (0.0, 0.0);
        for k in 1..=12 {
            at = (k as f64 * 3.5, k as f64 * 1.25);
            let (p, c) = s.step(Some(&frame(160, 120, at.0, at.1)), &seeds, false, 40.0);
            for (q, seed) in p.iter().zip(&seeds) {
                assert!(
                    (q.x - seed.x - at.0).abs() < 0.3 && (q.y - seed.y - at.1).abs() < 0.3,
                    "frame {k}: {q:?} from {seed:?}"
                );
            }
            assert!(c.iter().all(|c| *c > 0.6), "frame {k}: {c:?}");
        }
        // A big jump (25 px in one frame) is still within reach.
        let (p, _) = s.step(
            Some(&frame(160, 120, at.0 + 25.0, at.1)),
            &seeds,
            false,
            40.0,
        );
        assert!((p[0].x - (at.0 + 25.0)).abs() < 0.5, "{:?}", p[0]);
    }

    #[test]
    fn repeats_reset_and_lost_points() {
        let mut s = State::default();
        let img = frame(100, 80, 0.0, 0.0);
        let seeds = [v(0.0, 0.0)];
        s.step(Some(&img), &seeds, false, 40.0);
        // The same frame again: nothing moves.
        let (p, _) = s.step(Some(&img), &seeds, false, 40.0);
        assert_eq!(p, seeds);
        let moved = frame(100, 80, 4.0, 0.0);
        let (p, _) = s.step(Some(&moved), &seeds, false, 40.0);
        assert!((p[0].x - 4.0).abs() < 0.3);
        // Reset: back to the seeds.
        let (p, c) = s.step(Some(&moved), &seeds, true, 40.0);
        assert_eq!((p, c), (seeds.to_vec(), vec![1.0]));
        // New seeds while running are only taken on a reset or an edit...
        let other = [v(10.0, 0.0)];
        let (p, _) = s.step(Some(&moved), &other, false, 40.0);
        assert_eq!(p, seeds);
        s.edited = true;
        let (p, _) = s.step(Some(&moved), &other, false, 40.0);
        assert_eq!(p, other);
        // ...or when the count changes.
        let two = [v(0.0, 0.0), v(5.0, 5.0)];
        let (p, _) = s.step(Some(&moved), &two, false, 40.0);
        assert_eq!(p, two);
        // Off the image: held, confidence 0.
        let far = [v(500.0, 0.0)];
        s.step(Some(&moved), &far, true, 40.0);
        let (p, c) = s.step(Some(&frame(100, 80, 1.0, 0.0)), &far, false, 40.0);
        assert_eq!((p, c), (far.to_vec(), vec![0.0]));
        // A flat picture: nothing to lock onto.
        let flat = Arc::new(Image::new(50, 50, vec![128; 50 * 50 * 4]));
        s.step(Some(&flat), &seeds, true, 40.0);
        let flat2 = Arc::new(Image::new(50, 50, vec![128; 50 * 50 * 4]));
        let (p, c) = s.step(Some(&flat2), &seeds, false, 40.0);
        assert_eq!(p, seeds);
        assert_eq!(c, [0.0]);
        // No picture: the seeds at confidence 0; no seeds: nothing.
        assert_eq!(
            s.step(None, &seeds, false, 40.0),
            (seeds.to_vec(), vec![0.0])
        );
        assert_eq!(s.step(Some(&img), &[], false, 40.0), (vec![], vec![]));
    }

    #[test]
    fn long_tracks_dont_drift() {
        // Twice round a circle in sub-pixel steps through noisy frames:
        // frame-to-frame errors would pile up; the anchor holds them.
        let mut s = State::default();
        let seeds = [v(0.0, 0.0), v(-40.0, 30.0)];
        s.step(
            Some(&noisy(160, 140, 0.0, 0.0, 0.04, 0)),
            &seeds,
            false,
            40.0,
        );
        let n = 160;
        let (mut worst, mut late) = (0.0f64, 0.0f64);
        for k in 1..=n {
            let a = std::f64::consts::TAU * 2.0 * f64::from(k) / f64::from(n);
            let (ox, oy) = (18.0 * a.sin(), 12.0 * (1.0 - a.cos()));
            let img = noisy(160, 140, ox, oy, 0.04, k);
            let (p, c) = s.step(Some(&img), &seeds, false, 40.0);
            for (q, seed) in p.iter().zip(&seeds) {
                let err = (q.x - seed.x - ox).hypot(q.y - seed.y - oy);
                worst = worst.max(err);
                if k > n - 20 {
                    late = late.max(err);
                }
            }
            assert!(c.iter().all(|c| *c > 0.5), "frame {k}: {c:?} at {p:?}");
        }
        // Bounded by the noise (without the anchor: 1.7 px, still growing).
        assert!(
            worst < 1.0 && late < 0.8,
            "worst {worst}, last frames {late}"
        );
    }

    #[test]
    fn levels_cover_the_window() {
        assert_eq!(levels_for(0.0), 1);
        assert_eq!(levels_for(f64::NAN), 1);
        assert_eq!(levels_for(7.0), 1);
        assert_eq!(levels_for(14.0), 2);
        assert_eq!(levels_for(40.0), 4);
        assert_eq!(levels_for(1e9), MAX_LEVELS);
    }

    #[test]
    fn fast_enough_for_video() {
        // 640 × 480 at 30 fps, a handful of points: well under a frame.
        let mut s = State::default();
        let seeds: Vec<Vec2> = (0..8)
            .map(|i| v(-140.0 + 40.0 * f64::from(i), 0.0))
            .collect();
        let frames: Vec<Arc<Image>> = (0..6)
            .map(|k| frame(640, 480, f64::from(k) * 3.0, 0.0))
            .collect();
        s.step(Some(&frames[0]), &seeds, false, 40.0);
        let t = std::time::Instant::now();
        for f in &frames[1..] {
            s.step(Some(f), &seeds, false, 40.0);
        }
        let per = t.elapsed().as_secs_f64() / 5.0;
        // Generous for unoptimised test builds; release is far faster.
        assert!(per < 0.25, "{per} s a frame");
    }

    #[test]
    fn node_outputs_points_and_confidence() {
        let mut h = crate::nodes::value::harness::node("image/track-points", serde_json::json!({}));
        // image, points, reset, window
        let out = h
            .eval(vec![
                vec![Value::Image(frame(80, 60, 0.0, 0.0))],
                vec![Value::Vec2(v(1.0, 2.0))],
            ])
            .unwrap();
        assert!(matches!(out[0][..], [Value::Vec2(p)] if p == v(1.0, 2.0)));
        assert!(matches!(out[1][..], [Value::Num(c)] if c == 1.0));
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("image/track-points").unwrap();
        assert!(spec.effectful);
    }
}
