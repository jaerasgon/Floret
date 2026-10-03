//! `image/*` and `pixel/*` nodes, the image draw nodes (`draw/image`,
//! `draw/image-region`, `draw/image-tile`), `transform/cache` and the image
//! loaders (`io/image-url`, `io/file`, `io/svg`). Ported from
//! `src/nodes/image/`, `src/nodes/pixel/`, `src/nodes/draw/image*.ts`,
//! `src/nodes/transform/cache.ts` and `src/nodes/io/{file,image-url,svg}.ts`.
//!
//! Image values are described in [`crate::image`]. Conventions shared by
//! every node here, all from TS:
//!
//! - An input image counts only when it's ready (larger than 1×1). A node
//!   whose source isn't ready outputs the 1×1 placeholder (TS: the
//!   derived-image registry's `ensure`), draw nodes an empty group.
//! - Canvas2D work (`drawImage`, filters, compositing) runs on the CPU in
//!   [`raster`]; pixel loops over `getImageData` bytes port byte for byte
//!   (`pixel`).
//! - Producing nodes memoise on their inputs (image ids + numbers), so an
//!   unchanged chain hands out the same image and the GPU uploads nothing.

mod draw;
mod load;
mod pixel;
pub mod raster;
#[cfg(test)]
mod tests;

use super::shared::first;
use super::Registrar;
use crate::image::{ready, Image, MAX_DIM};
use crate::js;
use crate::node::{raw, scalar, NodeImpl, NodeInit, NodeResult};
use crate::value::{one, Color, Spread, Value};
use raster::{apply_filters, Filter, Op, Surface};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("image/adjust", adjust);
    r.node("image/blend", blend);
    r.node("image/convolve", pixel::convolve);
    r.node("image/crop", crop);
    r.node("image/dims", dims);
    r.node("image/filter", filter);
    r.node("image/from-layer", draw::from_layer);
    r.node("image/pixelate", pixelate);
    r.node("image/resize", resize);
    r.node("image/sample", sample);
    r.node("image/sample-spread", sample_spread);
    r.node("image/slicer", slicer);
    r.node("pixel/get-pixels", pixel::get_pixels);
    r.node("pixel/set-pixels", pixel::set_pixels);
    r.node("pixel/get-row", pixel::get_row);
    r.node("pixel/get-col", pixel::get_col);
    r.node("pixel/set-row", pixel::set_row);
    r.node("pixel/set-col", pixel::set_col);
    r.node("pixel/sort", pixel::sort);
    r.node("draw/image", draw::draw_image);
    r.node("draw/image-region", draw::draw_image_region);
    r.node("draw/image-tile", draw::draw_image_tile);
    r.node("transform/cache", draw::cache);
    r.node("io/image-url", load::image_url);
    r.node("io/file", load::file);
    r.node("io/svg", load::svg);
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// `((inputs[i] ?? [])[0] as number | undefined) ?? fallback`.
pub(crate) fn num_in(inputs: &[Spread], i: usize, fallback: f64) -> f64 {
    let v = first(inputs, i);
    if v.is_nullish() {
        fallback
    } else {
        v.to_number()
    }
}

/// `params.param<string>(key, fallback)`: a non-string param matches no
/// mode name.
pub(crate) fn param_str(cx: &crate::node::EvalCtx, key: &str, fallback: &str) -> String {
    match cx.param(key) {
        None => fallback.to_owned(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(_) => String::new(),
    }
}

/// A canvas dimension: `Math.max(1, Math.floor(v))`, NaN as 1 (a canvas
/// can't be NaN wide), capped at [`MAX_DIM`].
pub(crate) fn dim(v: f64) -> u32 {
    if v.is_nan() {
        1
    } else {
        v.floor().clamp(1.0, f64::from(MAX_DIM)) as u32
    }
}

fn image_out(img: Arc<Image>) -> NodeResult {
    Ok(vec![one(Value::Image(img))])
}

fn placeholder() -> NodeResult {
    image_out(Image::placeholder())
}

/// One part of a memo key.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Key {
    Id(u64),
    Num(u64),
    Str(String),
}

pub(crate) fn kn(v: f64) -> Key {
    Key::Num(v.to_bits())
}

pub(crate) fn kid(img: &Image) -> Key {
    Key::Id(img.id())
}

/// The last output of a pure computation and the inputs it came from.
pub(crate) struct Memo<T> {
    key: Option<Vec<Key>>,
    val: Option<T>,
}

impl<T> Default for Memo<T> {
    fn default() -> Self {
        Memo {
            key: None,
            val: None,
        }
    }
}

impl<T: Clone> Memo<T> {
    pub(crate) fn get(
        &mut self,
        key: Vec<Key>,
        f: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        if self.key.as_ref() == Some(&key) {
            if let Some(v) = &self.val {
                return Ok(v.clone());
            }
        }
        let v = f()?;
        self.key = Some(key);
        self.val = Some(v.clone());
        Ok(v)
    }
}

/// A raw image node: `f(inputs, cx, memo)`.
fn memo_node<F>(mut f: F) -> Box<dyn NodeImpl>
where
    F: FnMut(&[Spread], &crate::node::EvalCtx, &mut Memo<Arc<Image>>) -> NodeResult
        + Send
        + 'static,
{
    let mut memo = Memo::default();
    raw(move |inputs, cx| f(inputs, cx, &mut memo))
}

pub(crate) const TRANSPARENT: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

/// `colorFromBytes`.
pub(crate) fn color_from(px: [u8; 4]) -> Value {
    Value::Color(Color {
        r: f64::from(px[0]) / 255.0,
        g: f64::from(px[1]) / 255.0,
        b: f64::from(px[2]) / 255.0,
        a: f64::from(px[3]) / 255.0,
    })
}

// ─── adjust.ts ───────────────────────────────────────────────────────────────

/// CSS filters with a non-finite argument are invalid; assigning an invalid
/// `ctx.filter` string is ignored (the canvas keeps `none`).
fn valid(filters: Vec<Filter>) -> Vec<Filter> {
    let arg = |f: &Filter| match *f {
        Filter::Brightness(v)
        | Filter::Contrast(v)
        | Filter::Saturate(v)
        | Filter::HueRotate(v)
        | Filter::Grayscale(v)
        | Filter::Invert(v)
        | Filter::Blur(v) => v,
    };
    if filters.iter().all(|f| arg(f).is_finite()) {
        filters
    } else {
        Vec::new()
    }
}

fn adjust(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo_node(|inputs, _, memo| {
        let (br, ct) = (num_in(inputs, 1, 1.0), num_in(inputs, 2, 1.0));
        let (sa, hu) = (num_in(inputs, 3, 1.0), num_in(inputs, 4, 0.0));
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        let key = vec![kid(img), kn(br), kn(ct), kn(sa), kn(hu)];
        let out = memo.get(key, || {
            let mut fs = Vec::new();
            if br != 1.0 {
                fs.push(Filter::Brightness(js::max(0.0, br)));
            }
            if ct != 1.0 {
                fs.push(Filter::Contrast(js::max(0.0, ct)));
            }
            if sa != 1.0 {
                fs.push(Filter::Saturate(js::max(0.0, sa)));
            }
            if hu != 0.0 {
                fs.push(Filter::HueRotate(hu));
            }
            let fs = valid(fs);
            Ok(if fs.is_empty() {
                img.clone()
            } else {
                Arc::new(apply_filters(img, &fs))
            })
        })?;
        image_out(out)
    })
}

// ─── blend.ts ────────────────────────────────────────────────────────────────

fn blend(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo_node(|inputs, cx, memo| {
        let (a, b) = (first(inputs, 0), first(inputs, 1));
        let [sx, sy, mut sw, mut sh, dx, dy, mut dw, mut dh] =
            [2, 3, 4, 5, 6, 7, 8, 9].map(|i| num_in(inputs, i, 0.0));
        let mode = param_str(cx, "mode", "normal");
        let Some(a) = ready(&a) else {
            return placeholder();
        };
        let b = ready(&b);
        let mut key = vec![kid(a), Key::Str(mode.clone())];
        key.extend(b.map(|b| kid(b)));
        key.extend([sx, sy, sw, sh, dx, dy, dw, dh].map(kn));
        let out = memo.get(key, || {
            let mut canvas = Surface::from_image(a);
            if let Some(b) = b {
                if sw == 0.0 {
                    sw = f64::from(b.width()) - sx;
                }
                if sh == 0.0 {
                    sh = f64::from(b.height()) - sy;
                }
                if dw == 0.0 {
                    dw = sw;
                }
                if dh == 0.0 {
                    dh = sh;
                }
                if sw > 0.0 && sh > 0.0 && dw > 0.0 && dh > 0.0 {
                    canvas.draw(
                        &Surface::from_image(b),
                        [sx, sy, sw, sh],
                        [dx, dy, dw, dh],
                        true,
                        Op::from_blend_mode(&mode),
                    );
                }
            }
            Ok(Arc::new(canvas.to_image()))
        })?;
        image_out(out)
    })
}

// ─── crop.ts / resize.ts / pixelate.ts ───────────────────────────────────────

fn crop(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo_node(|inputs, _, memo| {
        let [sx, sy, mut sw, mut sh] = [1, 2, 3, 4].map(|i| num_in(inputs, i, 0.0));
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        if sw == 0.0 {
            sw = f64::from(img.width()) - sx;
        }
        if sh == 0.0 {
            sh = f64::from(img.height()) - sy;
        }
        let (w, h) = (dim(sw), dim(sh));
        let out = memo.get(vec![kid(img), kn(sx), kn(sy), kn(sw), kn(sh)], || {
            let inside = sx >= 0.0
                && sy >= 0.0
                && sx + sw <= f64::from(img.width())
                && sy + sh <= f64::from(img.height());
            if inside
                && [sx, sy].iter().all(|v| v.fract() == 0.0)
                && (sw, sh) == (w.into(), h.into())
            {
                // A 1:1 copy round-trips exactly through the canvas, except
                // that fully transparent pixels come back as 0,0,0,0.
                let mut out = crop_exact(img, sx as u32, sy as u32, w, h).into_rgba();
                for p in out.as_chunks_mut::<4>().0 {
                    if p[3] == 0 {
                        p.copy_from_slice(&[0; 4]);
                    }
                }
                return Ok(Arc::new(Image::new(w, h, out)));
            }
            let mut c = Surface::new(w, h);
            c.draw(
                &Surface::from_image(img),
                [sx, sy, sw, sh],
                [0.0, 0.0, f64::from(w), f64::from(h)],
                true,
                Op::SourceOver,
            );
            Ok(Arc::new(c.to_image()))
        })?;
        image_out(out)
    })
}

fn resize(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo_node(|inputs, _, memo| {
        let (mut w_in, mut h_in) = (num_in(inputs, 1, 0.0), num_in(inputs, 2, 0.0));
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        if w_in <= 0.0 {
            w_in = f64::from(img.width());
        }
        if h_in <= 0.0 {
            h_in = f64::from(img.height());
        }
        let (w, h) = (dim(w_in), dim(h_in));
        let out = memo.get(vec![kid(img), kn(f64::from(w)), kn(f64::from(h))], || {
            if (w, h) == (img.width(), img.height()) {
                return Ok(img.clone());
            }
            let mut c = Surface::new(w, h);
            c.draw(
                &Surface::from_image(img),
                [0.0, 0.0, f64::from(img.width()), f64::from(img.height())],
                [0.0, 0.0, f64::from(w), f64::from(h)],
                true,
                Op::SourceOver,
            );
            Ok(Arc::new(c.to_image()))
        })?;
        image_out(out)
    })
}

fn pixelate(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo_node(|inputs, _, memo| {
        let size = js::max(1.0, num_in(inputs, 1, 8.0).floor());
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        let out = memo.get(vec![kid(img), kn(size)], || {
            let (w, h) = (f64::from(img.width()), f64::from(img.height()));
            let small_w = js::max(1.0, (w / size).floor());
            let small_h = js::max(1.0, (h / size).floor());
            let mut c = Surface::new(img.width(), img.height());
            c.draw(
                &Surface::from_image(img),
                [0.0, 0.0, w, h],
                [0.0, 0.0, small_w, small_h],
                true,
                Op::SourceOver,
            );
            // Upscale the canvas's own top-left corner back over itself.
            let snapshot = Surface {
                w: c.w,
                h: c.h,
                px: c.px.clone(),
            };
            c.draw(
                &snapshot,
                [0.0, 0.0, small_w, small_h],
                [0.0, 0.0, w, h],
                false,
                Op::SourceOver,
            );
            Ok(Arc::new(c.to_image()))
        })?;
        image_out(out)
    })
}

// ─── filter.ts ───────────────────────────────────────────────────────────────

fn filter(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo_node(|inputs, cx, memo| {
        let level = num_in(inputs, 1, 0.5);
        let mode = param_str(cx, "mode", "gray");
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            return placeholder();
        };
        let key = vec![kid(img), kn(level), Key::Str(mode.clone())];
        let out = memo.get(key, || {
            let css = match mode.as_str() {
                "gray" => Some(Filter::Grayscale(1.0)),
                "invert" => Some(Filter::Invert(1.0)),
                "blur" => Some(Filter::Blur(js::max(0.0, level))),
                "dilate" => Some(Filter::Brightness(1.0 + 0.5 * level)),
                "erode" => Some(Filter::Brightness(js::max(0.0, 1.0 - 0.5 * level))),
                _ => None,
            };
            if let Some(f) = css {
                let fs = valid(vec![f]);
                return Ok(if fs.is_empty() {
                    img.clone()
                } else {
                    Arc::new(apply_filters(img, &fs))
                });
            }
            let mut d = img.rgba().to_vec();
            match mode.as_str() {
                "threshold" => {
                    let t = js::max(0.0, js::min(1.0, level)) * 255.0;
                    for p in d.as_chunks_mut::<4>().0 {
                        let lum = 0.299 * f64::from(p[0])
                            + 0.587 * f64::from(p[1])
                            + 0.114 * f64::from(p[2]);
                        let v = if lum < t { 0 } else { 255 };
                        p[..3].fill(v);
                    }
                }
                "posterize" => {
                    let levels = js::max(2.0, js::min(255.0, js::round(level)));
                    let step = 255.0 / (levels - 1.0);
                    for p in d.as_chunks_mut::<4>().0 {
                        for c in &mut p[..3] {
                            *c = pixel::clamp_u8(js::round(f64::from(*c) / step) * step);
                        }
                    }
                }
                "opaque" => d.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255),
                _ => return Ok(img.clone()),
            }
            Ok(Arc::new(Image::new(img.width(), img.height(), d)))
        })?;
        image_out(out)
    })
}

// ─── dims.ts / sample.ts / sample-spread.ts ──────────────────────────────────

fn dims(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(match ready(&args[0]) {
            Some(img) => {
                let (w, h) = (f64::from(img.width()), f64::from(img.height()));
                vec![w.into(), h.into(), (w / h).into()]
            }
            None => vec![0.0.into(), 0.0.into(), 0.0.into()],
        })
    })
}

/// `Math.max(0, Math.min(n - 1, Math.floor(v)))`, with a NaN read at 0
/// (`getImageData` converts its coordinates to integers, NaN → 0).
fn clamp_index(v: f64, n: u32) -> u32 {
    let i = js::max(0.0, js::min(f64::from(n) - 1.0, v.floor()));
    if i.is_nan() {
        0
    } else {
        i as u32
    }
}

/// Where `image/sample*` measure positions from: the image's centre, like
/// the canvas ((0, 0) is the middle, y down), so (x, y) reads pixel
/// (x + w/2, y + h/2). Nodes from patches saved before v3 carry
/// `origin: "top-left"` (see `patch::v2_to_v3`) and read pixel (x, y),
/// as the web app did.
#[derive(Clone, Copy)]
struct Origin {
    top_left: bool,
}

impl Origin {
    fn of(params: &crate::spec::Params) -> Origin {
        Origin {
            top_left: params
                .get(crate::patch::ORIGIN_PARAM)
                .and_then(|v| v.as_str())
                == Some(crate::patch::ORIGIN_TOP_LEFT),
        }
    }

    /// Pixel-space offset added to a position on `img`.
    fn offset(self, img: &Image) -> (f64, f64) {
        if self.top_left {
            (0.0, 0.0)
        } else {
            (f64::from(img.width()) / 2.0, f64::from(img.height()) / 2.0)
        }
    }
}

fn sample(init: &NodeInit) -> Box<dyn NodeImpl> {
    let origin = Origin::of(init.params);
    scalar(init, move |args| {
        let Some(img) = ready(&args[0]) else {
            return Ok(vec![Value::Color(TRANSPARENT)]);
        };
        let (ox, oy) = origin.offset(img);
        let x = clamp_index(args[1].to_number() + ox, img.width());
        let y = clamp_index(args[2].to_number() + oy, img.height());
        Ok(vec![color_from(img.pixel(x, y))])
    })
}

fn sample_spread(init: &NodeInit) -> Box<dyn NodeImpl> {
    let origin = Origin::of(init.params);
    raw(move |inputs, _| {
        let src = first(inputs, 0);
        let points = inputs.get(1).cloned().unwrap_or_else(crate::value::empty);
        let Some(img) = ready(&src).filter(|_| !points.is_empty()) else {
            let out: Vec<Value> = points.iter().map(|_| Value::Color(TRANSPARENT)).collect();
            return Ok(vec![Arc::from(out)]);
        };
        let (w, h) = (f64::from(img.width()), f64::from(img.height()));
        let (ox, oy) = origin.offset(img);
        let mut out = Vec::with_capacity(points.len());
        for p in points.iter() {
            if p.is_nullish() {
                return Err(format!("Cannot read properties of {p:?} (reading 'x')"));
            }
            let ix = js::max(0.0, js::min(w - 1.0, (p.get("x").to_number() + ox).floor()));
            let iy = js::max(0.0, js::min(h - 1.0, (p.get("y").to_number() + oy).floor()));
            out.push(if ix.is_nan() || iy.is_nan() {
                // `data[NaN]` is undefined → NaN channels.
                Value::Color(Color {
                    r: f64::NAN,
                    g: f64::NAN,
                    b: f64::NAN,
                    a: f64::NAN,
                })
            } else {
                color_from(img.pixel(ix as u32, iy as u32))
            });
        }
        Ok(vec![Arc::from(out)])
    })
}

// ─── slicer.ts ───────────────────────────────────────────────────────────────

/// `image/slicer`'s cell cap.
const MAX_CELLS: f64 = 256.0;

/// A 1:1 integer copy of a rect (`drawImage` with matching source and
/// destination sizes); outside the image is transparent.
pub(crate) fn crop_exact(img: &Image, x: u32, y: u32, w: u32, h: u32) -> Image {
    let mut out = vec![0u8; w as usize * h as usize * 4];
    let (iw, ih) = (img.width(), img.height());
    let src = img.rgba();
    for row in 0..h {
        let sy = y + row;
        if sy >= ih || x >= iw {
            continue;
        }
        let n = (w.min(iw - x)) as usize * 4;
        let so = (sy as usize * iw as usize + x as usize) * 4;
        let o = row as usize * w as usize * 4;
        out[o..o + n].copy_from_slice(&src[so..so + n]);
    }
    Image::new(w, h, out)
}

fn slicer(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut memo: Memo<Spread> = Memo::default();
    raw(move |inputs, _| {
        let count_of = |i: usize| {
            let v = first(inputs, i);
            let n = if v.is_nullish() { 1.0 } else { v.to_number() };
            // NaN stays NaN (no cells); huge counts are capped before the
            // shrink loop below.
            let n = js::max(1.0, n.floor());
            if n > MAX_CELLS {
                MAX_CELLS
            } else {
                n
            }
        };
        let (mut cols, mut rows) = (count_of(1), count_of(2));
        if cols.is_nan() || rows.is_nan() {
            return Ok(vec![crate::value::empty()]);
        }
        while cols * rows > MAX_CELLS {
            if cols >= rows {
                cols -= 1.0;
            } else {
                rows -= 1.0;
            }
        }
        let (cols, rows) = (cols as u32, rows as u32);
        let src = first(inputs, 0);
        let Some(img) = ready(&src) else {
            let out: Vec<Value> = (0..cols * rows)
                .map(|_| Value::Image(Image::placeholder()))
                .collect();
            return Ok(vec![Arc::from(out)]);
        };
        let key = vec![kid(img), kn(f64::from(cols)), kn(f64::from(rows))];
        let out = memo.get(key, || {
            let cw = (img.width() / cols).max(1);
            let ch = (img.height() / rows).max(1);
            let mut cells = Vec::with_capacity((cols * rows) as usize);
            for r in 0..rows {
                for c in 0..cols {
                    cells.push(Value::Image(Arc::new(crop_exact(
                        img,
                        c * cw,
                        r * ch,
                        cw,
                        ch,
                    ))));
                }
            }
            Ok(Arc::from(cells))
        })?;
        Ok(vec![out])
    })
}
