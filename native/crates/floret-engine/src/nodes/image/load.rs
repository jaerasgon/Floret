//! Image sources: `io/image-url`, `io/file`, `io/svg`.
//!
//! Loads run off the tick ([`crate::image::Loader`]); until they land a
//! node reports `loaded = false` and outputs the placeholder, like the TS
//! image registry. `http(s)` URLs go through the host's fetcher
//! (`crate::host`); `data:` URLs, `file://` URLs and local paths are read
//! directly.
//!
//! `io/svg` rasterises the SVG (resvg) into a texture and draws it as an
//! image (TS flattened it into polygons), re-rasterised at a power-of-two
//! multiple of its scale so it stays crisp. A wired style still replaces the SVG's own
//! paint: it becomes an `!important` stylesheet applied while rasterising.

use super::{param_str, Key, Memo};
use crate::image::{
    decode_data_url, parse_image, parse_text, rasterize_svg, resource_dir, Image, LoadSlot,
    LoadState,
};
use crate::js;
use crate::node::{always_eval, raw, EvalCtx, NodeImpl, NodeInit};
use crate::nodes::shared::{first, to_vec2};
use crate::nodes::style::style_arg;
use crate::nodes::value::param_or;
use crate::scene::{neutral_style, RectF, ResolvedStyle, SceneNode};
use crate::value::{one, Value};
use std::sync::Arc;

/// `params.param<string>(key, '').trim()` (a non-string param throws).
fn trimmed_param(cx: &EvalCtx, key: &str) -> Result<String, String> {
    match cx.param(key) {
        None => Ok(String::new()),
        Some(serde_json::Value::String(s)) => Ok(s.trim().to_owned()),
        Some(_) => Err(format!("{key}.trim is not a function")),
    }
}

fn num(v: f64) -> crate::value::Spread {
    one(Value::Num(v))
}

pub fn image_url(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut slot: LoadSlot<Arc<Image>> = LoadSlot::default();
    raw(move |_, cx| {
        let url = trimmed_param(cx, "url")?;
        if url.is_empty() {
            slot.clear();
            return Ok(vec![
                one(Value::Null),
                one(false.into()),
                num(0.0),
                num(0.0),
            ]);
        }
        Ok(match slot.ensure(&url, cx.host(), parse_image).state() {
            LoadState::Ready(img) => {
                let (w, h) = (f64::from(img.width()), f64::from(img.height()));
                vec![one(Value::Image(img)), one(true.into()), num(w), num(h)]
            }
            state => {
                if let LoadState::Failed(e) = state {
                    cx.warn(format!("Couldn't load the image: {e}"));
                }
                vec![
                    one(Value::Image(Image::placeholder())),
                    one(false.into()),
                    num(0.0),
                    num(0.0),
                ]
            }
        })
    })
}

/// `io/file`: a picked file stored in the patch as a data URL. Outputs
/// (url, image, name, loaded, type); `image` only for `image/*` types.
pub fn file(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut slot: LoadSlot<Arc<Image>> = LoadSlot::default();
    raw(move |_, cx| {
        let data_url = trimmed_param(cx, "dataUrl")?;
        let filename = param_or(cx, "filename", Value::str(""));
        let mime = match cx.param("mimeType") {
            None => String::new(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(_) => return Err("mimeType.startsWith is not a function".into()),
        };
        let mime_v = one(Value::str(&mime));
        if data_url.is_empty() {
            slot.clear();
            return Ok(vec![
                one(Value::str("")),
                one(Value::Null),
                one(filename),
                one(false.into()),
                mime_v,
            ]);
        }
        let url = one(Value::str(&data_url));
        if !mime.starts_with("image/") {
            slot.clear();
            return Ok(vec![
                url,
                one(Value::Null),
                one(filename),
                one(true.into()),
                mime_v,
            ]);
        }
        let (img, loaded) = match slot.ensure(&data_url, cx.host(), parse_image).state() {
            LoadState::Ready(img) => (img, true),
            LoadState::Failed(e) => {
                cx.warn(format!("Couldn't load the image: {e}"));
                (Image::placeholder(), false)
            }
            LoadState::Loading => (Image::placeholder(), false),
        };
        Ok(vec![
            url,
            one(Value::Image(img)),
            one(filename),
            one(loaded.into()),
            mime_v,
        ])
    })
}

// ─── svg.ts ──────────────────────────────────────────────────────────────────

/// A wired style as CSS that overrides every element's paint.
fn style_css(st: &ResolvedStyle) -> String {
    let paint = |p: &Option<crate::scene::Paint>| {
        p.as_ref()
            .map_or_else(|| "none".to_owned(), |p| p.css.to_string())
    };
    format!(
        "* {{ fill: {} !important; fill-opacity: 1 !important; \
         stroke: {} !important; stroke-opacity: 1 !important; \
         stroke-width: {} !important; stroke-linecap: {} !important; \
         stroke-linejoin: {} !important; }}",
        paint(&st.fill),
        paint(&st.stroke),
        js::num_to_string(st.stroke_weight),
        st.stroke_cap,
        st.stroke_join,
    )
}

/// Raster resolution for a drawn scale: the next power of two of 2×|scale|
/// (re-rasterising only when the scale crosses one).
fn raster_scale(scale: f64) -> f64 {
    let s = (scale.abs() * 2.0).clamp(0.25, 16.0);
    s.log2().ceil().exp2()
}

/// A rasterised SVG and its viewBox size, or why it couldn't be read.
type Raster = Result<(Arc<Image>, f64, f64), String>;

pub fn svg(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut remote: LoadSlot<Arc<str>> = LoadSlot::default();
    let mut inline: Option<(String, Option<Arc<str>>)> = None;
    let mut raster: Memo<Raster> = Memo::default();
    // Polls its load every tick (TS memoised it, freezing a pending URL).
    always_eval(raw(move |inputs, cx| {
        let none = || {
            Ok(vec![
                one(Value::Null),
                num(0.0),
                num(0.0),
                one(false.into()),
            ])
        };
        let mode = param_str(cx, "mode", "file");
        let (key, is_url) = match mode.as_str() {
            "markup" => (trimmed_param(cx, "markup")?, false),
            "url" => (trimmed_param(cx, "url")?, true),
            _ => (trimmed_param(cx, "dataUrl")?, false),
        };
        if key.is_empty() {
            return none();
        }
        let host = cx.host();
        let dir = if is_url {
            resource_dir(&key, host.as_deref())
        } else {
            host.as_ref().and_then(|h| h.base_dir())
        };
        let text = if is_url {
            match remote.ensure(&key, host, parse_text).state() {
                LoadState::Ready(t) => Some(t),
                LoadState::Failed(e) => {
                    cx.warn(format!("Couldn't load the SVG: {e}"));
                    None
                }
                LoadState::Loading => None,
            }
        } else {
            if inline.as_ref().is_none_or(|(k, _)| *k != key) {
                let text = if key.starts_with("data:") {
                    decode_data_url(&key).and_then(|b| parse_text(b, None)).ok()
                } else {
                    Some(Arc::from(key.as_str()))
                };
                inline = Some((key.clone(), text));
            }
            inline.as_ref().and_then(|(_, t)| t.clone())
        };
        let Some(text) = text else {
            return none();
        };

        let pos = to_vec2(&first(inputs, 0));
        let scale_v = first(inputs, 1);
        let scale = match if scale_v.is_nullish() {
            1.0
        } else {
            scale_v.to_number()
        } {
            s if s == 0.0 || s.is_nan() => 1.0,
            s => s,
        };
        let css = style_arg(&first(inputs, 2)).map(|st| style_css(&st.apply(neutral_style())));
        let rs = raster_scale(scale);
        let dir_key = dir.as_deref().map(|d| d.to_string_lossy().into_owned());
        let mem_key = vec![
            Key::Str(key.clone()),
            Key::Str(css.clone().unwrap_or_default()),
            super::kn(rs),
            Key::Str(dir_key.unwrap_or_default()),
        ];
        let svg = raster.get(mem_key, || {
            Ok(rasterize_svg(&text, css, rs, dir.as_deref())
                .map(|s| (Arc::new(s.image), s.width, s.height)))
        })?;
        let (image, w, h) = match svg {
            Ok(svg) => svg,
            Err(e) => {
                cx.warn(format!("Couldn't read the SVG: {e}"));
                return none();
            }
        };
        let mut layer = SceneNode::Image {
            image,
            src: None,
            dst: RectF {
                x: -w / 2.0,
                y: -h / 2.0,
                w,
                h,
            },
            tint: None,
        };
        if scale != 1.0 || pos.x != 0.0 || pos.y != 0.0 {
            layer = SceneNode::Group {
                children: Arc::from(vec![Arc::new(layer)]),
                transform: Some([scale, 0.0, 0.0, scale, pos.x, pos.y]),
                blend_mode: None,
                clip: None,
            };
        }
        Ok(vec![
            one(Value::Scene(Arc::new(layer))),
            num(w),
            num(h),
            one(true.into()),
        ])
    }))
}
