//! The Canvas2D operations the TS image nodes use, on the CPU.
//!
//! [`Surface`] is a canvas: premultiplied RGBA as `f32`, like the canvas
//! backing store (but without its 8-bit rounding between draws).
//! `drawImage` resampling and CSS filters can't match Skia bit for bit;
//! they follow the specs (Compositing & Blending, Filter Effects) and
//! round-trip exactly where the browser does: 1:1 integer-aligned copies
//! give back the same bytes.

use crate::image::Image;

/// Premultiplied RGBA in 0..1.
pub type Px = [f32; 4];

pub struct Surface {
    pub w: u32,
    pub h: u32,
    pub px: Vec<Px>,
}

impl Surface {
    pub fn new(w: u32, h: u32) -> Surface {
        Surface {
            w,
            h,
            px: vec![[0.0; 4]; w as usize * h as usize],
        }
    }

    pub fn from_image(img: &Image) -> Surface {
        let px = img
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let a = f32::from(p[3]) / 255.0;
                [
                    f32::from(p[0]) / 255.0 * a,
                    f32::from(p[1]) / 255.0 * a,
                    f32::from(p[2]) / 255.0 * a,
                    a,
                ]
            })
            .collect();
        Surface {
            w: img.width(),
            h: img.height(),
            px,
        }
    }

    /// `getImageData`: straight RGBA8.
    pub fn to_image(&self) -> Image {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            let a = p[3].clamp(0.0, 1.0);
            let a8 = (a * 255.0).round();
            if a8 <= 0.0 {
                out.extend_from_slice(&[0; 4]);
                continue;
            }
            for c in &p[..3] {
                out.push(((c / a).clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            out.push(a8 as u8);
        }
        Image::new(self.w, self.h, out)
    }

    /// `ctx.drawImage(src, sx, sy, sw, sh, dx, dy, dw, dh)` with the given
    /// smoothing and composite operation. Rects follow the canvas spec:
    /// negative sizes flip to positive, the source rect is clipped to the
    /// image and the destination shrinks with it.
    pub fn draw(&mut self, src: &Surface, s: [f64; 4], d: [f64; 4], smooth: bool, op: Op) {
        let [mut sx, mut sy, mut sw, mut sh] = s;
        let [mut dx, mut dy, mut dw, mut dh] = d;
        if !s.iter().chain(d.iter()).all(|v| v.is_finite()) {
            return;
        }
        for (p, l) in [
            (&mut sx, &mut sw),
            (&mut sy, &mut sh),
            (&mut dx, &mut dw),
            (&mut dy, &mut dh),
        ] {
            if *l < 0.0 {
                *p += *l;
                *l = -*l;
            }
        }
        if sw == 0.0 || sh == 0.0 || dw == 0.0 || dh == 0.0 {
            return;
        }
        let clip = |s0: &mut f64, sl: &mut f64, d0: &mut f64, dl: &mut f64, n: u32| -> bool {
            let (a, b) = (s0.max(0.0), (*s0 + *sl).min(f64::from(n)));
            if b <= a {
                return false;
            }
            let k = *dl / *sl;
            *d0 += (a - *s0) * k;
            *dl = (b - a) * k;
            *s0 = a;
            *sl = b - a;
            true
        };
        if !clip(&mut sx, &mut sw, &mut dx, &mut dw, src.w)
            || !clip(&mut sy, &mut sh, &mut dy, &mut dh, src.h)
        {
            return;
        }
        let cols = taps(dx, dw, sx, sw, src.w, self.w, smooth);
        let rows = taps(dy, dh, sy, sh, src.h, self.h, smooth);
        let sw_ = src.w as usize;
        for row in &rows {
            for col in &cols {
                let mut acc = [0.0f32; 4];
                for &(yk, wy) in &row.taps {
                    let line = yk * sw_;
                    for &(xk, wx) in &col.taps {
                        let p = src.px[line + xk];
                        let w = wx * wy;
                        for c in 0..4 {
                            acc[c] += p[c] * w;
                        }
                    }
                }
                let cov = row.cover * col.cover;
                let s = acc.map(|v| v * cov);
                let i = row.at * self.w as usize + col.at;
                self.px[i] = composite(self.px[i], s, op);
            }
        }
    }

    /// `drawImage(src, 0, 0)`.
    pub fn draw_at(&mut self, src: &Surface, x: f64, y: f64, op: Op) {
        let (w, h) = (f64::from(src.w), f64::from(src.h));
        self.draw(src, [0.0, 0.0, w, h], [x, y, w, h], true, op);
    }
}

/// One destination pixel along an axis: its index, how much of it the
/// destination rect covers, and the weighted source pixels it samples.
struct Tap {
    at: usize,
    cover: f32,
    taps: Vec<(usize, f32)>,
}

fn taps(d0: f64, dl: f64, s0: f64, sl: f64, src_n: u32, dst_n: u32, smooth: bool) -> Vec<Tap> {
    let d1 = d0 + dl;
    let first = d0.floor().max(0.0);
    let last = d1.ceil().min(f64::from(dst_n));
    let scale = sl / dl;
    let lo = s0.floor().max(0.0) as usize;
    let hi = ((s0 + sl).ceil() as usize).min(src_n as usize).max(lo + 1) - 1;
    let mut out = Vec::new();
    let mut i = first;
    while i < last {
        let (a, b) = (i.max(d0), (i + 1.0).min(d1));
        let cover = (b - a) as f32;
        if cover > 0.0 {
            let u = s0 + ((a + b) / 2.0 - d0) * scale;
            let mut t: Vec<(usize, f32)> = Vec::new();
            if !smooth {
                t.push(((u.floor().max(0.0) as usize).clamp(lo, hi), 1.0));
            } else if scale > 1.0 {
                // Box filter over the footprint of the destination pixel.
                let (fa, fb) = ((u - scale / 2.0).max(s0), (u + scale / 2.0).min(s0 + sl));
                let total = fb - fa;
                let mut k = fa.floor();
                while k < fb {
                    let w = (fb.min(k + 1.0) - fa.max(k)) / total;
                    if w > 0.0 {
                        t.push(((k as usize).clamp(lo, hi), w as f32));
                    }
                    k += 1.0;
                }
            } else {
                // Bilinear between pixel centres, clamped to the source rect.
                let c = u - 0.5;
                let k0 = c.floor();
                let f = (c - k0) as f32;
                let at = |k: f64| (k.max(0.0) as usize).clamp(lo, hi);
                t.push((at(k0), 1.0 - f));
                if f > 0.0 {
                    t.push((at(k0 + 1.0), f));
                }
            }
            out.push(Tap {
                at: i as usize,
                cover,
                taps: t,
            });
        }
        i += 1.0;
    }
    out
}

// ─── Compositing ─────────────────────────────────────────────────────────────

/// `globalCompositeOperation` values the image nodes use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    SourceOver,
    /// `lighter`: additive.
    Lighter,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

impl Op {
    /// `image/blend`'s mode names. Unknown modes leave the canvas at
    /// `source-over` (assigning `undefined` to `globalCompositeOperation`
    /// is ignored).
    pub fn from_blend_mode(mode: &str) -> Op {
        match mode {
            "add" => Op::Lighter,
            "darken" => Op::Darken,
            "lighten" => Op::Lighten,
            "difference" => Op::Difference,
            "exclusion" => Op::Exclusion,
            "multiply" => Op::Multiply,
            "screen" => Op::Screen,
            "overlay" => Op::Overlay,
            "hard-light" => Op::HardLight,
            "soft-light" => Op::SoftLight,
            "dodge" => Op::ColorDodge,
            "burn" => Op::ColorBurn,
            _ => Op::SourceOver,
        }
    }
}

fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb * 2.0 * cs
    } else {
        let s = 2.0 * cs - 1.0;
        cb + s - cb * s
    }
}

/// The separable blend function `B(cb, cs)` (straight colours).
fn blend(op: Op, cb: f32, cs: f32) -> f32 {
    match op {
        Op::Multiply => cb * cs,
        Op::Screen => cb + cs - cb * cs,
        Op::Overlay => hard_light(cs, cb),
        Op::Darken => cb.min(cs),
        Op::Lighten => cb.max(cs),
        Op::ColorDodge => {
            if cb == 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        Op::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        Op::HardLight => hard_light(cb, cs),
        Op::SoftLight => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 {
                    ((16.0 * cb - 12.0) * cb + 4.0) * cb
                } else {
                    cb.sqrt()
                };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        Op::Difference => (cb - cs).abs(),
        Op::Exclusion => cb + cs - 2.0 * cb * cs,
        Op::SourceOver | Op::Lighter => cs,
    }
}

/// Composite premultiplied `s` onto premultiplied `d`.
pub fn composite(d: Px, s: Px, op: Op) -> Px {
    let (sa, da) = (s[3], d[3]);
    match op {
        Op::SourceOver => [0, 1, 2, 3].map(|c| s[c] + d[c] * (1.0 - sa)),
        Op::Lighter => [0, 1, 2, 3].map(|c| (s[c] + d[c]).min(1.0)),
        _ => {
            if sa <= 0.0 {
                return d;
            }
            let mut out = [0.0; 4];
            for c in 0..3 {
                let cs = s[c] / sa;
                let cb = if da > 0.0 { d[c] / da } else { 0.0 };
                out[c] = s[c] * (1.0 - da) + d[c] * (1.0 - sa) + sa * da * blend(op, cb, cs);
            }
            out[3] = sa + da - sa * da;
            out
        }
    }
}

// ─── CSS filters ─────────────────────────────────────────────────────────────

/// One CSS filter function (Filter Effects Module, shorthand functions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Brightness(f64),
    Contrast(f64),
    Saturate(f64),
    HueRotate(f64),
    Grayscale(f64),
    Invert(f64),
    Blur(f64),
}

fn matrix(m: &[f32; 9], c: [f32; 3]) -> [f32; 3] {
    [
        m[0] * c[0] + m[1] * c[1] + m[2] * c[2],
        m[3] * c[0] + m[4] * c[1] + m[5] * c[2],
        m[6] * c[0] + m[7] * c[1] + m[8] * c[2],
    ]
}

impl Filter {
    /// The colour transform (straight colours), `None` for blur.
    fn color(self, c: [f32; 3]) -> Option<[f32; 3]> {
        Some(match self {
            Filter::Brightness(b) => c.map(|v| v * b as f32),
            Filter::Contrast(k) => {
                let k = k as f32;
                c.map(|v| v * k + 0.5 - 0.5 * k)
            }
            Filter::Saturate(s) => {
                let s = s as f32;
                matrix(
                    &[
                        0.213 + 0.787 * s,
                        0.715 - 0.715 * s,
                        0.072 - 0.072 * s,
                        0.213 - 0.213 * s,
                        0.715 + 0.285 * s,
                        0.072 - 0.072 * s,
                        0.213 - 0.213 * s,
                        0.715 - 0.715 * s,
                        0.072 + 0.928 * s,
                    ],
                    c,
                )
            }
            Filter::HueRotate(deg) => {
                let (sn, cs) = (deg.to_radians() as f32).sin_cos();
                matrix(
                    &[
                        0.213 + cs * 0.787 - sn * 0.213,
                        0.715 - cs * 0.715 - sn * 0.715,
                        0.072 - cs * 0.072 + sn * 0.928,
                        0.213 - cs * 0.213 + sn * 0.143,
                        0.715 + cs * 0.285 + sn * 0.140,
                        0.072 - cs * 0.072 - sn * 0.283,
                        0.213 - cs * 0.213 - sn * 0.787,
                        0.715 - cs * 0.715 + sn * 0.715,
                        0.072 + cs * 0.928 + sn * 0.072,
                    ],
                    c,
                )
            }
            Filter::Grayscale(a) => {
                let g = 1.0 - (a as f32).clamp(0.0, 1.0);
                matrix(
                    &[
                        0.2126 + 0.7874 * g,
                        0.7152 - 0.7152 * g,
                        0.0722 - 0.0722 * g,
                        0.2126 - 0.2126 * g,
                        0.7152 + 0.2848 * g,
                        0.0722 - 0.0722 * g,
                        0.2126 - 0.2126 * g,
                        0.7152 - 0.7152 * g,
                        0.0722 + 0.9278 * g,
                    ],
                    c,
                )
            }
            Filter::Invert(a) => {
                let a = (a as f32).clamp(0.0, 1.0);
                c.map(|v| a + v * (1.0 - 2.0 * a))
            }
            Filter::Blur(_) => return None,
        })
    }
}

/// `ctx.filter = <filters>; ctx.drawImage(src, 0, 0)` onto a cleared
/// canvas of the source's size. Each step's result is clamped to 0..1.
pub fn apply_filters(src: &Image, filters: &[Filter]) -> Image {
    let mut s = Surface::from_image(src);
    for f in filters {
        if let Filter::Blur(sigma) = *f {
            gaussian_blur(&mut s, sigma);
            continue;
        }
        for p in &mut s.px {
            let a = p[3];
            if a <= 0.0 {
                continue;
            }
            let c = [p[0] / a, p[1] / a, p[2] / a];
            if let Some(out) = f.color(c) {
                for k in 0..3 {
                    p[k] = out[k].clamp(0.0, 1.0) * a;
                }
            }
        }
    }
    s.to_image()
}

/// Separable Gaussian (`blur(σpx)`) over premultiplied pixels; outside the
/// canvas counts as transparent.
pub fn gaussian_blur(s: &mut Surface, sigma: f64) {
    if sigma.is_nan() || sigma <= 0.0 || s.px.is_empty() {
        return;
    }
    let r = (sigma * 3.0).ceil().min(1024.0) as isize;
    let mut k: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let sum: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    let (w, h) = (s.w as isize, s.h as isize);
    let pass = |src: &[Px], horizontal: bool| -> Vec<Px> {
        let mut out = vec![[0.0f32; 4]; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                for (j, kv) in k.iter().enumerate() {
                    let o = j as isize - r;
                    let (sx, sy) = if horizontal { (x + o, y) } else { (x, y + o) };
                    if sx < 0 || sy < 0 || sx >= w || sy >= h {
                        continue;
                    }
                    let p = src[(sy * w + sx) as usize];
                    for c in 0..4 {
                        acc[c] += p[c] * kv;
                    }
                }
                out[(y * w + x) as usize] = acc;
            }
        }
        out
    };
    let tmp = pass(&s.px, true);
    s.px = pass(&tmp, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Image {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        Image::new(w, h, v)
    }

    #[test]
    fn integer_copies_are_exact() {
        let src = img(5, 4, |x, y| [x as u8 * 50, y as u8 * 60, 7, 128 + x as u8]);
        let s = Surface::from_image(&src);
        let mut d = Surface::new(5, 4);
        d.draw_at(&s, 0.0, 0.0, Op::SourceOver);
        assert_eq!(d.to_image().rgba(), src.rgba());
        // A crop: source (1, 1, 3, 2) → a 3×2 canvas.
        let mut c = Surface::new(3, 2);
        c.draw(
            &s,
            [1.0, 1.0, 3.0, 2.0],
            [0.0, 0.0, 3.0, 2.0],
            true,
            Op::SourceOver,
        );
        let out = c.to_image();
        assert_eq!(out.pixel(0, 0), src.pixel(1, 1));
        assert_eq!(out.pixel(2, 1), src.pixel(3, 2));
    }

    #[test]
    fn source_rect_clips_to_the_image() {
        let src = Surface::from_image(&img(2, 2, |_, _| [255, 0, 0, 255]));
        let mut d = Surface::new(4, 4);
        // Source runs past the image: only the overlap draws, scaled 1:1.
        d.draw(
            &src,
            [0.0, 0.0, 4.0, 4.0],
            [0.0, 0.0, 4.0, 4.0],
            true,
            Op::SourceOver,
        );
        let out = d.to_image();
        assert_eq!(out.pixel(1, 1), [255, 0, 0, 255]);
        assert_eq!(out.pixel(2, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn downscale_averages_and_nearest_upscale_repeats() {
        let src = Surface::from_image(&img(4, 1, |x, _| [x as u8 * 60, 0, 0, 255]));
        let mut d = Surface::new(2, 1);
        d.draw(
            &src,
            [0.0, 0.0, 4.0, 1.0],
            [0.0, 0.0, 2.0, 1.0],
            true,
            Op::SourceOver,
        );
        let out = d.to_image();
        assert_eq!(out.pixel(0, 0), [30, 0, 0, 255]);
        assert_eq!(out.pixel(1, 0), [150, 0, 0, 255]);
        let small = Surface::from_image(&out);
        let mut up = Surface::new(4, 1);
        up.draw(
            &small,
            [0.0, 0.0, 2.0, 1.0],
            [0.0, 0.0, 4.0, 1.0],
            false,
            Op::SourceOver,
        );
        let up = up.to_image();
        assert_eq!(up.pixel(1, 0), [30, 0, 0, 255]);
        assert_eq!(up.pixel(2, 0), [150, 0, 0, 255]);
    }

    #[test]
    fn blend_modes_follow_the_spec() {
        let d = [0.5, 0.25, 1.0, 1.0];
        let s = [1.0, 0.5, 0.0, 1.0];
        assert_eq!(composite(d, s, Op::Multiply), [0.5, 0.125, 0.0, 1.0]);
        assert_eq!(composite(d, s, Op::Screen), [1.0, 0.625, 1.0, 1.0]);
        assert_eq!(composite(d, s, Op::Difference), [0.5, 0.25, 1.0, 1.0]);
        assert_eq!(composite(d, s, Op::Lighter), [1.0, 0.75, 1.0, 1.0]);
        // Half-transparent source over opaque: lerp.
        assert_eq!(
            composite([0.0, 0.0, 0.0, 1.0], [0.5, 0.0, 0.0, 0.5], Op::SourceOver),
            [0.5, 0.0, 0.0, 1.0]
        );
        // Blending onto nothing is just the source.
        assert_eq!(composite([0.0; 4], s, Op::Multiply), s);
    }

    #[test]
    fn color_filters() {
        let src = img(1, 1, |_, _| [200, 100, 50, 255]);
        let f = |fs: &[Filter]| apply_filters(&src, fs).pixel(0, 0);
        assert_eq!(f(&[Filter::Invert(1.0)]), [55, 155, 205, 255]);
        assert_eq!(f(&[Filter::Brightness(0.5)]), [100, 50, 25, 255]);
        assert_eq!(f(&[Filter::Contrast(0.0)]), [128, 128, 128, 255]);
        let g = f(&[Filter::Grayscale(1.0)]);
        assert!(g[0] == g[1] && g[1] == g[2]);
        assert_eq!(
            g[0],
            (0.2126f32 * 200.0 + 0.7152 * 100.0 + 0.0722 * 50.0).round() as u8
        );
        assert_eq!(f(&[Filter::HueRotate(0.0)]), [200, 100, 50, 255]);
        assert_eq!(f(&[Filter::Saturate(1.0)]), [200, 100, 50, 255]);
    }

    #[test]
    fn blur_spreads_and_keeps_energy() {
        let src = img(9, 9, |x, y| {
            if x == 4 && y == 4 {
                [255, 255, 255, 255]
            } else {
                [0; 4]
            }
        });
        let mut s = Surface::from_image(&src);
        gaussian_blur(&mut s, 1.0);
        let total: f32 = s.px.iter().map(|p| p[3]).sum();
        assert!((total - 1.0).abs() < 1e-4);
        assert!(s.px[4 * 9 + 5][3] > 0.0);
    }
}
