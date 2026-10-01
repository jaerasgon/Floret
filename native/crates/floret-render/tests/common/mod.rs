//! Test-only decoder: recorded TS scenes (`native/fixtures/traces`) →
//! `SceneNode`s, so the renderer can be exercised on real samples before the
//! draw nodes are ported. Format: `native/fixtures/README.md`.

#![allow(dead_code)]

use floret_engine::image::Image;
use floret_engine::particles::{ParticleBuffer, ParticleHandle, ParticleTemplate};
use floret_engine::scene::{Paint, RectF, ResolvedStyle, SceneNode};
use floret_engine::{Color, Vec2};
use serde_json::Value as J;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A decoded fixture value; `$id` / `$ref` sharing becomes `Arc` sharing.
#[derive(Clone, Debug)]
pub enum Fx {
    Null,
    Bool(bool),
    Num(f64),
    Str(Arc<str>),
    Arr(Arc<Vec<Fx>>),
    Obj(Arc<Vec<(String, Fx)>>),
    /// Any other tagged value (`$opaque`, `$ta`, `$fn`, …) or a cycle.
    Other,
}

impl Fx {
    pub fn get(&self, key: &str) -> Option<&Fx> {
        match self {
            Fx::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Fx::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Fx::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn arr(&self) -> Option<&[Fx]> {
        match self {
            Fx::Arr(a) => Some(a),
            _ => None,
        }
    }
}

fn decode(j: &J, refs: &mut Vec<Fx>) -> Fx {
    match j {
        J::Null => Fx::Null,
        J::Bool(b) => Fx::Bool(*b),
        J::Number(n) => Fx::Num(n.as_f64().unwrap_or(f64::NAN)),
        J::String(s) => Fx::Str(Arc::from(s.as_str())),
        J::Array(items) => Fx::Arr(Arc::new(items.iter().map(|x| decode(x, refs)).collect())),
        J::Object(map) => {
            if let Some(r) = map.get("$ref").and_then(J::as_u64) {
                return refs.get(r as usize).cloned().unwrap_or(Fx::Other);
            }
            if map.len() == 1 && map.contains_key("$u") {
                return Fx::Null;
            }
            if let Some(t) = map.get("$num").and_then(J::as_str) {
                return Fx::Num(match t {
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ => f64::NAN,
                });
            }
            let id = map.get("$id").and_then(J::as_u64).map(|i| i as usize);
            if let Some(i) = id {
                if refs.len() <= i {
                    refs.resize(i + 1, Fx::Other);
                }
            }
            let out = if let Some(arr) = map.get("$arr").and_then(J::as_array) {
                Fx::Arr(Arc::new(arr.iter().map(|x| decode(x, refs)).collect()))
            } else if map
                .keys()
                .any(|k| k.starts_with('$') && k != "$id" && k != "$class")
            {
                Fx::Other
            } else {
                Fx::Obj(Arc::new(
                    map.iter()
                        .filter(|(k, _)| !k.starts_with('$'))
                        .map(|(k, v)| (k.clone(), decode(v, refs)))
                        .collect(),
                ))
            };
            if let Some(i) = id {
                refs[i] = out.clone();
            }
            out
        }
    }
}

pub fn traces_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/traces")
}

/// One recorded frame of a sample's first renderer.
pub struct TraceScene {
    pub sample: String,
    pub frame: usize,
    pub width: u32,
    pub height: u32,
    pub background: [f64; 4],
    pub layers: Vec<Arc<SceneNode>>,
    /// Scene kinds the Rust `SceneNode` can't represent (image, shader,
    /// particle batches, …).
    pub unsupported: Vec<String>,
}

pub fn load_trace(sample: &str) -> Option<J> {
    let path = traces_dir().join(format!("{sample}.json.gz"));
    let f = std::fs::File::open(path).ok()?;
    let mut s = String::new();
    flate2::read::GzDecoder::new(f)
        .read_to_string(&mut s)
        .ok()?;
    serde_json::from_str(&s).ok()
}

pub fn sample_ids() -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(traces_dir())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| {
                    e.file_name()
                        .to_str()
                        .and_then(|n| n.strip_suffix(".json.gz"))
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

/// The scene at recorded frame index `fi` (0..6 ↔ frames 0, 1, 2, 10, 30, 60).
/// `None` when the trace has no renderer or no such frame.
pub fn scene(trace: &J, fi: usize) -> Option<TraceScene> {
    let sample = trace["sample"].as_str().unwrap_or("?").to_owned();
    let rid = trace["compile"]["rendererIds"].get(0)?.as_str()?;
    let fr = trace["frames"].get(fi)?;
    let width = fr["frame"]["width"].as_f64()? as u32;
    let height = fr["frame"]["height"].as_f64()? as u32;
    // Refs resolve in document order across all of the frame's outputs.
    let mut refs = Vec::new();
    let mut out = None;
    for (id, v) in fr["outputs"].as_object()? {
        let d = decode(v, &mut refs);
        if id == rid {
            out = Some(d);
        }
    }
    let out = out?;
    let ports = out.arr()?;
    let mut layers = Vec::new();
    let mut unsupported = Vec::new();
    if let Some(spread) = ports.first().and_then(Fx::arr) {
        for item in spread {
            flatten(item, &mut layers, &mut unsupported);
        }
    }
    let background = ports
        .get(1)
        .and_then(Fx::arr)
        .and_then(|s| s.first())
        .and_then(|c| {
            let ch = |k| c.get(k).and_then(Fx::num);
            Some(
                Paint::from_color(Color {
                    r: ch("r")?,
                    g: ch("g")?,
                    b: ch("b")?,
                    a: ch("a")?,
                })
                .rgba,
            )
        })
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    Some(TraceScene {
        sample,
        frame: fi,
        width,
        height,
        background,
        layers,
        unsupported,
    })
}

/// A synthetic reference scene (`reference/scenes/<name>.json`):
/// `{ width, height, background: [r, g, b], layers: [SceneNode JSON…] }`.
pub fn scene_from_json(name: &str, j: &J) -> TraceScene {
    let mut refs = Vec::new();
    let mut layers = Vec::new();
    let mut unsupported = Vec::new();
    flatten(
        &decode(&j["layers"], &mut refs),
        &mut layers,
        &mut unsupported,
    );
    let bg = |i: usize| j["background"][i].as_f64().unwrap_or(0.0);
    TraceScene {
        sample: name.to_owned(),
        frame: 0,
        width: j["width"].as_u64().unwrap_or(1) as u32,
        height: j["height"].as_u64().unwrap_or(1) as u32,
        background: [bg(0), bg(1), bg(2), 1.0],
        layers,
        unsupported,
    }
}

pub fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("reference")
}

/// Decode a PNG to RGBA8.
pub fn read_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return None,
    };
    Some((info.width, info.height, rgba))
}

fn flatten(v: &Fx, out: &mut Vec<Arc<SceneNode>>, unsupported: &mut Vec<String>) {
    match v {
        Fx::Arr(items) => items.iter().for_each(|i| flatten(i, out, unsupported)),
        Fx::Obj(_) => {
            if let Some(n) = node(v, unsupported) {
                out.push(Arc::new(n));
            }
        }
        _ => {}
    }
}

fn num(o: &Fx, k: &str) -> f64 {
    o.get(k).and_then(Fx::num).unwrap_or(f64::NAN)
}

fn paint(v: Option<&Fx>) -> Option<Paint> {
    v.and_then(Fx::str).map(Paint::css)
}

fn style(o: &Fx) -> Arc<ResolvedStyle> {
    let d = floret_engine::scene::default_style();
    let Some(s) = o.get("style") else {
        return d.clone();
    };
    let st = |k: &str, def: &Arc<str>| -> Arc<str> {
        s.get(k).and_then(Fx::str).map_or(def.clone(), Arc::from)
    };
    let n = |k: &str, def: f64| s.get(k).and_then(Fx::num).unwrap_or(def);
    Arc::new(ResolvedStyle {
        fill: paint(s.get("fill")),
        stroke: paint(s.get("stroke")),
        stroke_weight: n("strokeWeight", d.stroke_weight),
        stroke_cap: st("strokeCap", &d.stroke_cap),
        stroke_join: st("strokeJoin", &d.stroke_join),
        stroke_align: st("strokeAlign", &d.stroke_align),
        blur: n("blur", 0.0),
        font_size: n("fontSize", d.font_size),
        font_family: st("fontFamily", &d.font_family),
        font_weight: st("fontWeight", &d.font_weight),
        font_style: st("fontStyle", &d.font_style),
        letter_spacing: n("letterSpacing", 0.0),
        line_height: n("lineHeight", d.line_height),
        text_align: st("textAlign", &d.text_align),
        text_baseline: st("textBaseline", &d.text_baseline),
        rect_mode: st("rectMode", &d.rect_mode),
        ellipse_mode: st("ellipseMode", &d.ellipse_mode),
    })
}

fn node(o: &Fx, unsupported: &mut Vec<String>) -> Option<SceneNode> {
    let kind = o.get("kind").and_then(Fx::str)?;
    let mode = || -> Arc<str> { Arc::from(o.get("mode").and_then(Fx::str).unwrap_or("corner")) };
    let pts = |keys: &[&str]| -> Vec<f64> { keys.iter().map(|k| num(o, k)).collect() };
    Some(match kind {
        "rect" | "ellipse" => {
            let (x, y, w, h) = (num(o, "x"), num(o, "y"), num(o, "w"), num(o, "h"));
            if kind == "rect" {
                SceneNode::Rect {
                    x,
                    y,
                    w,
                    h,
                    mode: mode(),
                    style: style(o),
                }
            } else {
                SceneNode::Ellipse {
                    x,
                    y,
                    w,
                    h,
                    mode: mode(),
                    style: style(o),
                }
            }
        }
        "line" => SceneNode::Line {
            x1: num(o, "x1"),
            y1: num(o, "y1"),
            x2: num(o, "x2"),
            y2: num(o, "y2"),
            style: style(o),
        },
        "arc" => SceneNode::Arc {
            x: num(o, "x"),
            y: num(o, "y"),
            w: num(o, "w"),
            h: num(o, "h"),
            start: num(o, "start"),
            stop: num(o, "stop"),
            mode: mode(),
            style: style(o),
        },
        "triangle" => {
            let p = pts(&["x1", "y1", "x2", "y2", "x3", "y3"]);
            SceneNode::Triangle {
                pts: p.try_into().ok()?,
                style: style(o),
            }
        }
        "quad" => {
            let p = pts(&["x1", "y1", "x2", "y2", "x3", "y3", "x4", "y4"]);
            SceneNode::Quad {
                pts: p.try_into().ok()?,
                style: style(o),
            }
        }
        "point" => SceneNode::Point {
            x: num(o, "x"),
            y: num(o, "y"),
            style: style(o),
        },
        "polygon" => {
            let points: Vec<Vec2> = o
                .get("points")
                .and_then(Fx::arr)
                .unwrap_or(&[])
                .iter()
                .map(|p| Vec2 {
                    x: num(p, "x"),
                    y: num(p, "y"),
                })
                .collect();
            SceneNode::Polygon {
                points: Arc::from(points),
                closed: matches!(o.get("closed"), Some(Fx::Bool(true))),
                style: style(o),
                tension: o.get("tension").and_then(Fx::num),
            }
        }
        "text" => SceneNode::Text {
            text: Arc::from(o.get("text").and_then(Fx::str).unwrap_or("")),
            x: num(o, "x"),
            y: num(o, "y"),
            style: style(o),
        },
        "group" => {
            let mut children = Vec::new();
            for c in o.get("children").and_then(Fx::arr).unwrap_or(&[]) {
                flatten(c, &mut children, unsupported);
            }
            let transform = o.get("transform").and_then(Fx::arr).and_then(|t| {
                let v: Vec<f64> = t.iter().filter_map(Fx::num).collect();
                v.try_into().ok()
            });
            let clip = o.get("clip").map(|c| RectF {
                x: num(c, "x"),
                y: num(c, "y"),
                w: num(c, "w"),
                h: num(c, "h"),
            });
            SceneNode::Group {
                children: Arc::from(children),
                transform,
                blend_mode: o.get("blendMode").and_then(Fx::str).map(Arc::from),
                clip,
            }
        }
        "particle-batch" => SceneNode::ParticleBatch {
            buffer: particle_buffer(o.get("buffer")?)?,
            count: num(o, "count"),
            template: match o.get("template").and_then(|t| t.get("kind")?.str()) {
                Some("rect") => ParticleTemplate::Rect,
                _ => ParticleTemplate::Circle,
            },
            style: style(o),
            use_buffer_color: matches!(o.get("useBufferColor"), Some(Fx::Bool(true))),
        },
        "particle-trail-batch" => SceneNode::ParticleTrailBatch {
            buffer: particle_buffer(o.get("buffer")?)?,
            history: Arc::new(f32s(o.get("history")?)?),
            segments: num(o, "segments") as usize,
            head_idx: num(o, "headIdx") as usize,
            style: style(o),
            fade_out: matches!(o.get("fadeOut"), Some(Fx::Bool(true))),
            use_buffer_color: matches!(o.get("useBufferColor"), Some(Fx::Bool(true))),
            tapered: matches!(o.get("tapered"), Some(Fx::Bool(true))),
            tail_width: num(o, "tailWidth"),
        },
        // Synthetic scenes carry inline pixels; recorded canvases are opaque.
        "image" => {
            let rect = |r: &Fx| RectF {
                x: num(r, "x"),
                y: num(r, "y"),
                w: num(r, "w"),
                h: num(r, "h"),
            };
            let Some((src, dst)) = o.get("source").zip(o.get("dst")) else {
                unsupported.push(kind.to_owned());
                return None;
            };
            let Some(rgba) = src.get("rgba").and_then(Fx::arr) else {
                unsupported.push(kind.to_owned());
                return None;
            };
            let bytes = rgba.iter().map(|v| v.num().unwrap_or(0.0) as u8).collect();
            SceneNode::Image {
                image: Arc::new(Image::new(
                    num(src, "w") as u32,
                    num(src, "h") as u32,
                    bytes,
                )),
                src: o.get("src").filter(|s| s.get("x").is_some()).map(rect),
                dst: rect(dst),
                tint: paint(o.get("tint")),
            }
        }
        other => {
            unsupported.push(other.to_owned());
            return None;
        }
    })
}

/// A plain-array number list (`null` → NaN, as JSON writes it).
fn f32s(v: &Fx) -> Option<Vec<f32>> {
    Some(
        v.arr()?
            .iter()
            .map(|x| x.num().map_or(f32::NAN, |n| n as f32))
            .collect(),
    )
}

/// A synthetic scene's particle buffer: `{ capacity, alive: [0|1…],
/// posX, posY, size, colorR, colorG, colorB, colorA }` as plain arrays
/// (recorded traces only keep typed-array digests, so they can't be drawn).
fn particle_buffer(o: &Fx) -> Option<ParticleHandle> {
    let mut b = ParticleBuffer::new(num(o, "capacity"));
    let cap = b.capacity;
    let field = |k: &str| -> Option<Vec<f32>> {
        let mut v = f32s(o.get(k)?)?;
        v.resize(cap, 0.0);
        Some(v)
    };
    let alive = field("alive")?;
    b.alive = alive.iter().map(|&a| u8::from(a != 0.0)).collect();
    b.count = b.alive.iter().map(|&a| i64::from(a)).sum();
    b.pos_x = field("posX")?;
    b.pos_y = field("posY")?;
    b.size = field("size")?;
    b.color_r = field("colorR")?;
    b.color_g = field("colorG")?;
    b.color_b = field("colorB")?;
    b.color_a = field("colorA")?;
    Some(ParticleHandle::new(b))
}

/// Write RGBA8 as a PNG (for eyeballing renders).
pub fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .expect("write png");
}

/// `FLORET_RENDER_OUT=<dir>`: where tests drop PNGs of what they rendered.
pub fn out_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("FLORET_RENDER_OUT")?);
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}
