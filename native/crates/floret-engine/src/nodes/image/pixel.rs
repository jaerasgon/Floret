//! `pixel/*` and `image/convolve`: loops over `getImageData` bytes, ported
//! byte for byte (`Uint8ClampedArray` stores included).

use super::{color_from, first, kid, kn, num_in, param_str, placeholder, Key, Memo};
use crate::image::{ready, Image};
use crate::js;
use crate::node::{raw, NodeImpl, NodeInit, NodeResult};
use crate::value::{empty, one, Spread, Value};
use std::cmp::Ordering::Equal;
use std::sync::Arc;

/// A store into a `Uint8ClampedArray`: NaN → 0, clamp to 0..255, round
/// half to even.
pub fn clamp_u8(v: f64) -> u8 {
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    if v >= 255.0 {
        return 255;
    }
    let f = v.floor();
    let r = v - f;
    let n = if r > 0.5 || (r == 0.5 && f % 2.0 != 0.0) {
        f + 1.0
    } else {
        f
    };
    n as u8
}

/// `colorToBytes`: `Math.round(c.r * 255)` etc. into the clamped array.
fn color_to_bytes(c: &Value, out: &mut [u8]) {
    for (i, k) in ["r", "g", "b", "a"].iter().enumerate() {
        out[i] = clamp_u8(js::round(c.get(k).to_number() * 255.0));
    }
}

fn spread_at(inputs: &[Spread], i: usize) -> Spread {
    inputs.get(i).cloned().unwrap_or_else(empty)
}

fn image_out(img: Image) -> NodeResult {
    Ok(vec![one(Value::Image(Arc::new(img)))])
}

// ─── pixels.ts ───────────────────────────────────────────────────────────────

pub fn get_pixels(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return Ok(vec![empty(), one(0.0.into()), one(0.0.into())]);
        };
        let px: Vec<Value> = img
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| color_from([p[0], p[1], p[2], p[3]]))
            .collect();
        Ok(vec![
            Arc::from(px),
            one(f64::from(img.width()).into()),
            one(f64::from(img.height()).into()),
        ])
    })
}

pub fn set_pixels(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = first(inputs, 0);
        let pixels = spread_at(inputs, 1);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        let mut d = img.rgba().to_vec();
        let n = pixels
            .len()
            .min(img.width() as usize * img.height() as usize);
        for (i, c) in pixels.iter().take(n).enumerate() {
            if c.truthy() {
                color_to_bytes(c, &mut d[i * 4..i * 4 + 4]);
            }
        }
        image_out(Image::new(img.width(), img.height(), d))
    })
}

// ─── row-col.ts ──────────────────────────────────────────────────────────────

/// `clampInt(v, lo, hi)`: `Math.floor(Number(v ?? 0))` clamped; NaN passes
/// the clamp and reads as 0 (`getImageData` / `putImageData` coordinates).
fn clamp_int(v: &Value, lo: f64, hi: f64) -> usize {
    let n = if v.is_nullish() { 0.0 } else { v.to_number() }.floor();
    let n = if n < lo {
        lo
    } else if n > hi {
        hi
    } else {
        n
    };
    if n.is_nan() {
        0
    } else {
        n as usize
    }
}

pub fn get_row(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return Ok(vec![empty()]);
        };
        let y = clamp_int(&first(inputs, 1), 0.0, f64::from(img.height()) - 1.0) as u32;
        let px: Vec<Value> = (0..img.width())
            .map(|x| color_from(img.pixel(x, y)))
            .collect();
        Ok(vec![Arc::from(px)])
    })
}

pub fn get_col(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return Ok(vec![empty()]);
        };
        let x = clamp_int(&first(inputs, 1), 0.0, f64::from(img.width()) - 1.0) as u32;
        let px: Vec<Value> = (0..img.height())
            .map(|y| color_from(img.pixel(x, y)))
            .collect();
        Ok(vec![Arc::from(px)])
    })
}

/// `writeAxisLine`: the source with `pixels` written along one row / column.
fn write_line(img: &Image, pixels: &[Value], row: bool, index: &Value) -> Image {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut d = img.rgba().to_vec();
    let (len, at) = if row {
        (w, clamp_int(index, 0.0, h as f64 - 1.0))
    } else {
        (h, clamp_int(index, 0.0, w as f64 - 1.0))
    };
    for (i, c) in pixels.iter().take(len).enumerate() {
        if c.truthy() {
            let o = if row { at * w + i } else { i * w + at } * 4;
            color_to_bytes(c, &mut d[o..o + 4]);
        }
    }
    Image::new(img.width(), img.height(), d)
}

fn set_line(row: bool) -> Box<dyn NodeImpl> {
    raw(move |inputs, _| {
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        image_out(write_line(
            img,
            &spread_at(inputs, 2),
            row,
            &first(inputs, 1),
        ))
    })
}

pub fn set_row(_: &NodeInit) -> Box<dyn NodeImpl> {
    set_line(true)
}

pub fn set_col(_: &NodeInit) -> Box<dyn NodeImpl> {
    set_line(false)
}

// ─── sort.ts ─────────────────────────────────────────────────────────────────

type KeyFn = fn(f64, f64, f64) -> f64;

fn criterion(name: &str) -> Option<KeyFn> {
    Some(match name {
        "brightness" => |r, g, b| 0.2126 * r + 0.7152 * g + 0.0722 * b,
        "lightness" => |r, g, b| (js::max_of([r, g, b]) + js::min_of([r, g, b])) * 0.5,
        "saturation" => |r, g, b| js::max_of([r, g, b]) - js::min_of([r, g, b]),
        "hue" => |r, g, b| {
            let (mx, mn) = (js::max_of([r, g, b]), js::min_of([r, g, b]));
            if mx == mn {
                return 0.0;
            }
            let d = mx - mn;
            if mx == r {
                (g - b) / d + if g < b { 6.0 } else { 0.0 }
            } else if mx == g {
                (b - r) / d + 2.0
            } else {
                (r - g) / d + 4.0
            }
        },
        "red" => |r, _, _| r,
        "green" => |_, g, _| g,
        "blue" => |_, _, b| b,
        _ => return None,
    })
}

fn key_at(d: &[u8], o: usize, key: KeyFn) -> f64 {
    key(f64::from(d[o]), f64::from(d[o + 1]), f64::from(d[o + 2]))
}

/// `sortSegment`: stable sort of `len` pixels from byte `start`, `stride`
/// bytes apart.
fn sort_segment(d: &mut [u8], start: usize, len: usize, stride: usize, key: KeyFn, desc: bool) {
    if len < 2 {
        return;
    }
    let mut buf: Vec<(f64, [u8; 4])> = (0..len)
        .map(|i| {
            let o = start + i * stride;
            (key_at(d, o, key), [d[o], d[o + 1], d[o + 2], d[o + 3]])
        })
        .collect();
    if desc {
        buf.sort_by(|p, q| q.0.partial_cmp(&p.0).unwrap_or(Equal));
    } else {
        buf.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(Equal));
    }
    for (i, (_, px)) in buf.iter().enumerate() {
        let o = start + i * stride;
        d[o..o + 4].copy_from_slice(px);
    }
}

/// `sortLine`: sort every maximal run whose key is within `[lo, hi]`.
#[allow(clippy::too_many_arguments)]
fn sort_line(
    d: &mut [u8],
    start: usize,
    len: usize,
    stride: usize,
    key: KeyFn,
    desc: bool,
    lo: f64,
    hi: f64,
) {
    let mut run: Option<(usize, usize)> = None;
    for i in 0..len {
        let o = start + i * stride;
        let k = key_at(d, o, key);
        if k >= lo && k <= hi {
            run = Some(match run {
                None => (o, 1),
                Some((s, n)) => (s, n + 1),
            });
        } else if let Some((s, n)) = run.take() {
            sort_segment(d, s, n, stride, key, desc);
        }
    }
    if let Some((s, n)) = run {
        sort_segment(d, s, n, stride, key, desc);
    }
}

pub fn sort(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut memo: Memo<Arc<Image>> = Memo::default();
    raw(move |inputs, cx| {
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        let lo = num_in(inputs, 1, 0.0);
        let hi = num_in(inputs, 2, 1.0);
        let axis = param_str(cx, "axis", "rows");
        let crit = param_str(cx, "criterion", "brightness");
        let desc = param_str(cx, "order", "ascending") == "descending";
        let key = vec![
            kid(img),
            kn(lo),
            kn(hi),
            Key::Str(format!("{axis}/{crit}/{desc}")),
        ];
        let out = memo.get(key, || {
            let key = criterion(&crit).ok_or("key is not a function")?;
            let scale = if crit == "hue" { 6.0 } else { 255.0 };
            let (lo, hi) = (lo * scale, hi * scale);
            let (w, h) = (img.width() as usize, img.height() as usize);
            let mut d = img.rgba().to_vec();
            if axis == "rows" {
                for y in 0..h {
                    sort_line(&mut d, y * w * 4, w, 4, key, desc, lo, hi);
                }
            } else {
                for x in 0..w {
                    sort_line(&mut d, x * 4, h, w * 4, key, desc, lo, hi);
                }
            }
            Ok(Arc::new(Image::new(img.width(), img.height(), d)))
        })?;
        Ok(vec![one(Value::Image(out))])
    })
}

// ─── convolve.ts ─────────────────────────────────────────────────────────────

fn kernel(mode: &str) -> Option<([f64; 9], f64)> {
    Some(match mode {
        "edge" => ([0., -1., 0., -1., 4., -1., 0., -1., 0.], 0.0),
        "sharpen" => ([0., -1., 0., -1., 5., -1., 0., -1., 0.], 0.0),
        "emboss" => ([-2., -1., 0., -1., 1., 1., 0., 1., 2.], 128.0),
        _ => return None,
    })
}

/// The 3×3 neighbourhood's byte offsets (edges clamp), row-major.
fn neighbours(x: usize, y: usize, w: usize, h: usize) -> [usize; 9] {
    let (ym, yp) = (y.saturating_sub(1), (y + 1).min(h - 1));
    let (xm, xp) = (x.saturating_sub(1), (x + 1).min(w - 1));
    let at = |yy: usize, xx: usize| (yy * w + xx) * 4;
    [
        at(ym, xm),
        at(ym, x),
        at(ym, xp),
        at(y, xm),
        at(y, x),
        at(y, xp),
        at(yp, xm),
        at(yp, x),
        at(yp, xp),
    ]
}

fn convolve_bytes(s: &[u8], w: usize, h: usize, mode: &str) -> Result<Vec<u8>, String> {
    let mut d = vec![0u8; s.len()];
    let sobel = mode == "edge-sobel";
    let k = if sobel {
        None
    } else {
        Some(kernel(mode).ok_or("Cannot read properties of undefined (reading 'data')")?)
    };
    for y in 0..h {
        for x in 0..w {
            let n = neighbours(x, y, w, h);
            let mc = n[4];
            for ch in 0..3 {
                let v = |i: usize| f64::from(s[n[i] + ch]);
                d[mc + ch] = match k {
                    Some((k, bias)) => {
                        let sum = (0..9).map(|i| v(i) * k[i]).sum::<f64>() + bias;
                        clamp_u8(sum.clamp(0.0, 255.0))
                    }
                    None => {
                        let gx = -v(0) + v(2) - 2.0 * v(3) + 2.0 * v(5) - v(6) + v(8);
                        let gy = -v(0) - 2.0 * v(1) - v(2) + v(6) + 2.0 * v(7) + v(8);
                        let mag = (gx * gx + gy * gy).sqrt();
                        clamp_u8(if mag > 255.0 { 255.0 } else { mag })
                    }
                };
            }
            d[mc + 3] = s[mc + 3];
        }
    }
    Ok(d)
}

pub fn convolve(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut memo: Memo<Arc<Image>> = Memo::default();
    raw(move |inputs, cx| {
        let src = first(inputs, 0);
        let amount = js::max(0.0, js::min(1.0, num_in(inputs, 1, 1.0)));
        let mode = param_str(cx, "mode", "edge");
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        let key = vec![kid(img), kn(amount), Key::Str(mode.clone())];
        let out = memo.get(key, || {
            let s = img.rgba();
            let (w, h) = (img.width() as usize, img.height() as usize);
            let mut d = convolve_bytes(s, w, h, &mode)?;
            if amount < 1.0 {
                let inv = 1.0 - amount;
                for (dp, sp) in d
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(s.as_chunks::<4>().0)
                {
                    for c in 0..3 {
                        dp[c] = clamp_u8(f64::from(sp[c]) * inv + f64::from(dp[c]) * amount);
                    }
                    dp[3] = sp[3];
                }
            }
            Ok(Arc::new(Image::new(img.width(), img.height(), d)))
        })?;
        Ok(vec![one(Value::Image(out))])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamped_stores_round_half_to_even() {
        assert_eq!(clamp_u8(0.5), 0);
        assert_eq!(clamp_u8(1.5), 2);
        assert_eq!(clamp_u8(2.5), 2);
        assert_eq!(clamp_u8(2.51), 3);
        assert_eq!(clamp_u8(-3.0), 0);
        assert_eq!(clamp_u8(300.0), 255);
        assert_eq!(clamp_u8(f64::NAN), 0);
        assert_eq!(clamp_u8(254.5), 254);
    }
}
