//! Images into the scene (`draw/image*`) and the scene into images
//! (`image/from-layer`, `transform/cache`).

use super::{dim, num_in};
use crate::image::{ready, Image};
use crate::js;
use crate::node::{raw, scalar, EvalCtx, NodeImpl, NodeInit};
use crate::nodes::draw::anchor_top_left;
use crate::nodes::shared::to_vec2;
use crate::nodes::style::paint_of;
use crate::scene::{Paint, RectF, SceneNode};
use crate::value::{one, Spread, Value};
use std::sync::Arc;

fn scene(node: SceneNode) -> Value {
    Value::Scene(Arc::new(node))
}

fn empty_group() -> Value {
    scene(SceneNode::group(Vec::new()))
}

/// `tint ? colorToCss(tint) : null`.
fn tint_of(v: &Value) -> Result<Option<Paint>, String> {
    if v.truthy() {
        paint_of(v).map(Some)
    } else {
        Ok(None)
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> RectF {
    RectF { x, y, w, h }
}

// ─── draw/image*.ts ──────────────────────────────────────────────────────────

/// `draw/image`: (image, pos, w, h, anchor, tint). A zero size draws at the
/// image's natural size.
pub fn draw_image(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let Some(img) = ready(&args[0]) else {
            return Ok(vec![empty_group()]);
        };
        let (w, h) = (args[2].to_number(), args[3].to_number());
        let dw = if w == 0.0 { f64::from(img.width()) } else { w };
        let dh = if h == 0.0 { f64::from(img.height()) } else { h };
        let (x, y) = anchor_top_left(to_vec2(&args[1]), dw, dh, &args[4]);
        Ok(vec![scene(SceneNode::Image {
            image: img.clone(),
            src: None,
            dst: rect(x, y, dw, dh),
            tint: tint_of(&args[5])?,
        })])
    })
}

/// `draw/image-region`: (image, sx, sy, sw, sh, dx, dy, dw, dh, tint).
/// Zero source size runs to the image edge; zero destination size matches
/// the source rect.
pub fn draw_image_region(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let Some(img) = ready(&args[0]) else {
            return Ok(vec![empty_group()]);
        };
        let [sx, sy, mut sw, mut sh, dx, dy, mut dw, mut dh] =
            [1, 2, 3, 4, 5, 6, 7, 8].map(|i| args[i].to_number());
        if sw == 0.0 {
            sw = f64::from(img.width()) - sx;
        }
        if sh == 0.0 {
            sh = f64::from(img.height()) - sy;
        }
        if dw == 0.0 {
            dw = sw;
        }
        if dh == 0.0 {
            dh = sh;
        }
        if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
            return Ok(vec![empty_group()]);
        }
        Ok(vec![scene(SceneNode::Image {
            image: img.clone(),
            src: Some(rect(sx, sy, sw, sh)),
            dst: rect(dx, dy, dw, dh),
            tint: tint_of(&args[9])?,
        })])
    })
}

/// `draw/image-tile`: (image, pos, w, h, cols, rows, anchor, tint) — a
/// group of `cols × rows` whole-image cells.
pub fn draw_image_tile(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let Some(img) = ready(&args[0]) else {
            return Ok(vec![empty_group()]);
        };
        let (w, h) = (args[2].to_number(), args[3].to_number());
        let (x, y) = anchor_top_left(to_vec2(&args[1]), w, h, &args[6]);
        if w <= 0.0 || h <= 0.0 {
            return Ok(vec![empty_group()]);
        }
        let cols = js::max(1.0, args[4].to_number().floor());
        let rows = js::max(1.0, args[5].to_number().floor());
        let (cw, ch) = (w / cols, h / rows);
        let tint = tint_of(&args[7])?;
        let mut children = Vec::new();
        let mut r = 0.0;
        while r < rows {
            let mut c = 0.0;
            while c < cols {
                children.push(Arc::new(SceneNode::Image {
                    image: img.clone(),
                    src: None,
                    dst: rect(x + c * cw, y + r * ch, cw, ch),
                    tint: tint.clone(),
                }));
                c += 1.0;
            }
            r += 1.0;
        }
        Ok(vec![scene(SceneNode::group(children))])
    })
}

// ─── Rasterising (from-layer.ts, cache.ts) ───────────────────────────────────

/// The scene nodes on a layer wire, lists flattened (like the renderer).
fn layers_of(spread: &[Value]) -> Vec<Arc<SceneNode>> {
    fn walk(v: &Value, out: &mut Vec<Arc<SceneNode>>) {
        match v {
            Value::Scene(n) => out.push(n.clone()),
            Value::List(items) => items.iter().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    spread.iter().for_each(|v| walk(v, &mut out));
    out
}

/// Particle batches read buffers that change in place: never reuse a
/// raster of them.
fn has_live(n: &SceneNode) -> bool {
    match n {
        SceneNode::ParticleBatch { .. } | SceneNode::ParticleTrailBatch { .. } => true,
        SceneNode::Group { children, .. } => children.iter().any(|c| has_live(c)),
        _ => false,
    }
}

/// Rasterises through the host, reusing the last image while the layers
/// (by value), the size and the host stay the same. Without a host the
/// image is transparent — what the TS fixtures recorded, where the canvas
/// was a no-op mock.
#[derive(Default)]
struct Raster {
    last: Option<(RasterKey, Arc<Image>)>,
}

#[derive(PartialEq)]
struct RasterKey {
    layers: Vec<Arc<SceneNode>>,
    size: (u32, u32),
    host: bool,
}

impl Raster {
    fn get(&mut self, cx: &EvalCtx, layers: Vec<Arc<SceneNode>>, w: u32, h: u32) -> Arc<Image> {
        let host = cx.host();
        let key = RasterKey {
            layers,
            size: (w, h),
            host: host.is_some(),
        };
        if let Some((prev, img)) = &self.last {
            // `Vec<Arc<_>>` equality compares the nodes by value.
            if *prev == key {
                return img.clone();
            }
        }
        let img = Arc::new(
            host.as_ref()
                .and_then(|hst| hst.rasterize(&key.layers, w, h))
                .filter(|i| (i.width(), i.height()) == (w, h))
                .unwrap_or_else(|| Image::transparent(w, h)),
        );
        self.last = if key.layers.iter().any(|l| has_live(l)) {
            None
        } else {
            Some((key, img.clone()))
        };
        img
    }
}

/// `image/from-layer`: (layer spread, w, h) → a `w × h` image of the
/// layers, centre origin, transparent background.
pub fn from_layer(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut raster = Raster::default();
    raw(move |inputs, cx| {
        let layers = layers_of(inputs.first().map_or(&[][..], |s| &s[..]));
        let w = dim(num_in(inputs, 1, 256.0));
        let h = dim(num_in(inputs, 2, 256.0));
        Ok(vec![one(Value::Image(raster.get(cx, layers, w, h)))])
    })
}

/// `cache.ts`'s default raster size when `w` / `h` aren't positive.
const DEFAULT_CACHE_SIZE: f64 = 720.0;

/// `transform/cache`: rasterise the layers into a bitmap and draw that,
/// centred.
pub fn cache(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut raster = Raster::default();
    raw(move |inputs: &[Spread], cx| {
        let layers = layers_of(inputs.first().map_or(&[][..], |s| &s[..]));
        let size = |i: usize| {
            let v = num_in(inputs, i, 0.0);
            js::max(1.0, (if v > 0.0 { v } else { DEFAULT_CACHE_SIZE }).floor())
        };
        let (w, h) = (size(1), size(2));
        let img = raster.get(cx, layers, dim(w), dim(h));
        Ok(vec![one(scene(SceneNode::Image {
            image: img,
            src: None,
            dst: rect(-w / 2.0, -h / 2.0, w, h),
            tint: None,
        }))])
    })
}
