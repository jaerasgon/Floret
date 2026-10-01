//! Image node tests.
//!
//! `ts_cases.json` was recorded by calling the TS nodes directly
//! (`computeRaw` / `compute` from `nodeRegistry`) with a byte-accurate
//! canvas stand-in (1:1 `drawImage`, `getImageData` / `putImageData`) on
//! three seeded random images: `a` (9×6 opaque), `b` (7×5, random alpha),
//! `c` (12×4). It covers every byte-exact node — `pixel/*`, `image/convolve`,
//! `image/filter` (threshold / posterize / opaque), `image/sample*`,
//! `image/slicer`, `image/crop`, `image/dims` — including edge cases (NaN,
//! out-of-range rows, falsy pixels, unknown modes). Images are
//! `{"$image": {w, h, b64}}`; `{"$img": name}` in an input names a source
//! image; `{"$error": msg}` means the TS node threw.
//!
//! Resampling, filters and blend modes can't match Skia bit for bit; their
//! tests below check behaviour from the specs.

use super::super::value::harness::node;
use crate::host::{self, Host};
use crate::image::{base64_decode, Image};
use crate::scene::{RectF, SceneNode};
use crate::value::{Color, Value};
use serde_json::{json, Value as J};
use std::sync::Arc;

fn image_of(j: &J) -> Image {
    let w = j["w"].as_u64().unwrap() as u32;
    let h = j["h"].as_u64().unwrap() as u32;
    Image::new(w, h, base64_decode(j["b64"].as_str().unwrap()).unwrap())
}

fn decode_in(j: &J, images: &J) -> Value {
    if let Some(name) = j.get("$img").and_then(J::as_str) {
        return Value::Image(Arc::new(image_of(&images[name])));
    }
    Value::from_json(j)
}

fn num_eq(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a == b || (a - b).abs() <= 1e-12
}

fn cmp(v: &Value, j: &J, path: &str) -> Result<(), String> {
    let bad = || Err(format!("{path}: rust {v:?} vs ts {j}"));
    if let Some(im) = j.get("$image") {
        let Value::Image(img) = v else { return bad() };
        let want = image_of(im);
        if (img.width(), img.height()) != (want.width(), want.height()) {
            return bad();
        }
        if img.rgba() != want.rgba() {
            let i = img
                .rgba()
                .iter()
                .zip(want.rgba())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            return Err(format!(
                "{path}: byte {i} (pixel {}) rust {:?} vs ts {:?}",
                i / 4,
                &img.rgba()[i / 4 * 4..i / 4 * 4 + 4],
                &want.rgba()[i / 4 * 4..i / 4 * 4 + 4]
            ));
        }
        return Ok(());
    }
    match (v, j) {
        (Value::Undefined, J::Object(o)) if o.contains_key("$u") => Ok(()),
        (Value::Null, J::Null) => Ok(()),
        (Value::Bool(a), J::Bool(b)) if a == b => Ok(()),
        (Value::Num(a), _) => match Value::from_json(j) {
            Value::Num(b) if num_eq(*a, b) => Ok(()),
            _ => bad(),
        },
        (Value::Color(c), J::Object(o)) => {
            let ok = [("r", c.r), ("g", c.g), ("b", c.b), ("a", c.a)]
                .iter()
                .all(|(k, x)| {
                    o.get(*k)
                        .map(|y| Value::from_json(y).to_number())
                        .is_some_and(|y| num_eq(*x, y))
                });
            if ok && o.len() == 4 {
                Ok(())
            } else {
                bad()
            }
        }
        (Value::List(items), J::Array(arr)) => cmp_list(items, arr, path),
        _ => bad(),
    }
}

fn cmp_list(items: &[Value], arr: &[J], path: &str) -> Result<(), String> {
    if items.len() != arr.len() {
        return Err(format!(
            "{path}: length rust {} vs ts {}",
            items.len(),
            arr.len()
        ));
    }
    for (i, (x, y)) in items.iter().zip(arr).enumerate() {
        cmp(x, y, &format!("{path}[{i}]"))?;
    }
    Ok(())
}

#[test]
fn matches_typescript_cases() {
    let all: J = serde_json::from_str(include_str!("ts_cases.json")).expect("ts_cases.json");
    let images = &all["images"];
    let cases = all["cases"].as_array().unwrap();
    assert!(cases.len() > 300);
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let ty = case["type"].as_str().unwrap();
        let mut h = node(ty, case["params"].clone());
        // TS `compute` ignores extra arguments; Rust checks arity.
        let arity = crate::spec::Catalogue::builtin()
            .get(ty)
            .unwrap()
            .inputs
            .len();
        let inputs: Vec<Vec<Value>> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .take(arity)
            .map(|s| {
                s.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| decode_in(x, images))
                    .collect()
            })
            .collect();
        let got = h.eval(inputs);
        let want = &case["outputs"];
        let res = match (&got, want.get("$error")) {
            (Err(_), Some(_)) => Ok(()),
            (Ok(_), Some(e)) => Err(format!("ts threw {e}, rust didn't")),
            (Err(e), None) => Err(format!("rust error {e}")),
            (Ok(out), None) => {
                let arr = want.as_array().unwrap();
                if out.len() != arr.len() {
                    Err(format!("port count {} vs {}", out.len(), arr.len()))
                } else {
                    out.iter().zip(arr).enumerate().try_for_each(|(p, (o, a))| {
                        cmp_list(o, a.as_array().unwrap(), &format!("out[{p}]"))
                    })
                }
            }
        };
        if let Err(e) = res {
            failures.push(format!("#{i} {ty} {}: {e}", case["params"]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn solid(w: u32, h: u32, px: [u8; 4]) -> Arc<Image> {
    Arc::new(Image::new(w, h, px.repeat((w * h) as usize)))
}

fn img(v: &Value) -> &Arc<Image> {
    match v {
        Value::Image(i) => i,
        other => panic!("not an image: {other:?}"),
    }
}

fn n(x: f64) -> Value {
    Value::Num(x)
}

fn im(i: &Arc<Image>) -> Value {
    Value::Image(i.clone())
}

// ─── Canvas-op nodes ─────────────────────────────────────────────────────────

#[test]
fn unready_sources_give_placeholders() {
    for ty in [
        "image/adjust",
        "image/blend",
        "image/resize",
        "image/pixelate",
    ] {
        let mut h = node(ty, json!({}));
        let out = h.eval(vec![vec![Value::Null]]).unwrap();
        assert!(!img(&out[0][0]).is_ready(), "{ty}");
    }
}

#[test]
fn aligned_crops_match_the_canvas_path() {
    use super::raster::{Op, Surface};
    let (w, h) = (13u32, 7u32);
    let mut rng = crate::js::Mulberry32::new(7);
    let bytes: Vec<u8> = (0..w * h * 4)
        .map(|i| {
            let b = (rng.next_f64() * 256.0) as u8;
            // Plenty of fully transparent pixels.
            if i % 4 == 3 && b < 64 {
                0
            } else {
                b
            }
        })
        .collect();
    let src = Arc::new(Image::new(w, h, bytes));
    for [sx, sy, sw, sh] in [[0, 0, 13, 7], [2, 1, 5, 4], [0, 3, 13, 4]] {
        let mut node = node("image/crop", json!({}));
        let rect = [sx, sy, sw, sh].map(|v| n(f64::from(v)));
        let out = node.eval1(&[
            im(&src),
            rect[0].clone(),
            rect[1].clone(),
            rect[2].clone(),
            rect[3].clone(),
        ]);
        let mut c = Surface::new(sw, sh);
        c.draw(
            &Surface::from_image(&src),
            [sx, sy, sw, sh].map(f64::from),
            [0.0, 0.0, f64::from(sw), f64::from(sh)],
            true,
            Op::SourceOver,
        );
        assert_eq!(
            img(&out[0]).rgba(),
            c.to_image().rgba(),
            "{sx},{sy} {sw}×{sh}"
        );
    }
}

#[test]
fn adjust_identity_passes_through_and_memoises() {
    let src = solid(4, 4, [10, 20, 30, 255]);
    let mut h = node("image/adjust", json!({}));
    let out = h.eval1(&[im(&src), n(1.0), n(1.0), n(1.0), n(0.0)]);
    assert!(Arc::ptr_eq(img(&out[0]), &src));
    let a = h.eval1(&[im(&src), n(0.5), n(1.0), n(1.0), n(0.0)]);
    let b = h.eval1(&[im(&src), n(0.5), n(1.0), n(1.0), n(0.0)]);
    assert!(
        Arc::ptr_eq(img(&a[0]), img(&b[0])),
        "same inputs → same image"
    );
    assert_eq!(img(&a[0]).pixel(0, 0), [5, 10, 15, 255]);
    // An invalid filter (NaN) is ignored, like an invalid ctx.filter.
    let c = h.eval1(&[im(&src), n(f64::NAN), n(2.0), n(1.0), n(0.0)]);
    assert_eq!(img(&c[0]).pixel(0, 0), [10, 20, 30, 255]);
}

#[test]
fn blend_composites_b_onto_a() {
    let a = solid(4, 4, [200, 100, 50, 255]);
    let b = solid(2, 2, [128, 255, 0, 255]);
    let mut h = node("image/blend", json!({ "mode": "multiply" }));
    let mut args = vec![im(&a), im(&b)];
    args.extend([0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0].map(n));
    let out = img(&h.eval1(&args)[0]).clone();
    assert_eq!((out.width(), out.height()), (4, 4));
    assert_eq!(out.pixel(0, 0), [200, 100, 50, 255]);
    assert_eq!(out.pixel(1, 1), [100, 100, 0, 255]);
    assert_eq!(out.pixel(2, 2), [100, 100, 0, 255]);
    assert_eq!(out.pixel(3, 3), [200, 100, 50, 255]);
    // Without B, A passes through (as a copy).
    let out = h.eval1(&[
        im(&a),
        Value::Null,
        n(0.0),
        n(0.0),
        n(0.0),
        n(0.0),
        n(0.0),
        n(0.0),
        n(0.0),
        n(0.0),
    ]);
    assert_eq!(img(&out[0]).rgba(), a.rgba());
}

#[test]
fn resize_and_pixelate() {
    let mut px = Vec::new();
    for y in 0..4u8 {
        for x in 0..4u8 {
            px.extend_from_slice(&[x * 60, y * 60, 0, 255]);
        }
    }
    let src = Arc::new(Image::new(4, 4, px));
    let mut h = node("image/resize", json!({}));
    let out = img(&h.eval1(&[im(&src), n(2.0), n(0.0)])[0]).clone();
    assert_eq!((out.width(), out.height()), (2, 4));
    assert_eq!(out.pixel(0, 0), [30, 0, 0, 255]);
    // Zero / negative sizes keep the source size; that's a pass-through.
    let same = h.eval1(&[im(&src), n(0.0), n(-1.0)]);
    assert!(Arc::ptr_eq(img(&same[0]), &src));

    let mut h = node("image/pixelate", json!({}));
    let out = img(&h.eval1(&[im(&src), n(2.0)])[0]).clone();
    assert_eq!((out.width(), out.height()), (4, 4));
    // Each 2×2 block is the block's average.
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        assert_eq!(out.pixel(x, y), [30, 30, 0, 255]);
    }
    assert_eq!(out.pixel(3, 3), [150, 150, 0, 255]);
}

#[test]
fn css_filter_modes() {
    let src = solid(15, 15, [200, 100, 50, 255]);
    let run = |mode: &str, level: f64| {
        let mut h = node("image/filter", json!({ "mode": mode }));
        img(&h.eval1(&[im(&src), n(level)])[0]).pixel(7, 7)
    };
    assert_eq!(run("invert", 0.5), [55, 155, 205, 255]);
    let g = run("gray", 0.5);
    assert!(g[0] == g[1] && g[1] == g[2]);
    assert_eq!(run("dilate", 1.0), [255, 150, 75, 255]);
    assert_eq!(run("erode", 1.0), [100, 50, 25, 255]);
    // Blurring a flat image keeps the middle; edges fade into transparency.
    assert_eq!(run("blur", 1.0), [200, 100, 50, 255]);
    let mut h = node("image/filter", json!({ "mode": "blur" }));
    let edge = img(&h.eval1(&[im(&src), n(1.0)])[0]).pixel(0, 0);
    assert!(edge[3] < 255);
}

// ─── Draw nodes ──────────────────────────────────────────────────────────────

fn scene_of(v: &Value) -> &SceneNode {
    match v {
        Value::Scene(s) => s,
        other => panic!("not a scene: {other:?}"),
    }
}

fn is_empty_group(v: &Value) -> bool {
    matches!(scene_of(v), SceneNode::Group { children, .. } if children.is_empty())
}

#[test]
fn draw_image_shapes() {
    let src = solid(40, 20, [1, 2, 3, 255]);
    let pos = Value::Vec2(crate::Vec2 { x: 10.0, y: 5.0 });
    let mut h = node("draw/image", json!({}));
    let out = h.eval1(&[
        im(&src),
        pos.clone(),
        n(0.0),
        n(0.0),
        Value::str("center"),
        Value::Null,
    ]);
    let SceneNode::Image {
        image,
        src: s,
        dst,
        tint,
    } = scene_of(&out[0])
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(image, &src));
    assert_eq!(*s, None);
    assert_eq!(
        *dst,
        RectF {
            x: -10.0,
            y: -5.0,
            w: 40.0,
            h: 20.0
        }
    );
    assert_eq!(*tint, None);
    let red = Value::Color(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 0.5,
    });
    let out = h.eval1(&[im(&src), pos, n(8.0), n(4.0), Value::str("br"), red]);
    let SceneNode::Image { dst, tint, .. } = scene_of(&out[0]) else {
        panic!()
    };
    assert_eq!(
        *dst,
        RectF {
            x: 2.0,
            y: 1.0,
            w: 8.0,
            h: 4.0
        }
    );
    assert_eq!(tint.as_ref().unwrap().css.as_ref(), "rgba(255,0,0,0.500)");
    // TS object shape.
    let v = scene_of(&out[0]).to_value();
    let keys: Vec<&String> = match &v {
        Value::Object(m) => m.keys().collect(),
        _ => panic!(),
    };
    assert_eq!(keys, ["kind", "source", "src", "dst", "tint"]);
    // Not ready → an empty group.
    let out = h.eval1(&[
        Value::Null,
        Value::Null,
        n(0.0),
        n(0.0),
        Value::Null,
        Value::Null,
    ]);
    assert!(is_empty_group(&out[0]));
}

#[test]
fn draw_image_region_and_tile() {
    let src = solid(10, 8, [1, 2, 3, 255]);
    let mut h = node("draw/image-region", json!({}));
    let mut args = vec![im(&src)];
    args.extend([2.0, 1.0, 0.0, 0.0, -5.0, -5.0, 0.0, 20.0].map(n));
    args.push(Value::Null);
    let out = h.eval1(&args);
    let SceneNode::Image { src: s, dst, .. } = scene_of(&out[0]) else {
        panic!()
    };
    assert_eq!(
        *s,
        Some(RectF {
            x: 2.0,
            y: 1.0,
            w: 8.0,
            h: 7.0
        })
    );
    assert_eq!(
        *dst,
        RectF {
            x: -5.0,
            y: -5.0,
            w: 8.0,
            h: 20.0
        }
    );
    args[3] = n(-1.0);
    assert!(is_empty_group(&h.eval1(&args)[0]));

    let mut h = node("draw/image-tile", json!({}));
    let out = h.eval1(&[
        im(&src),
        Value::Vec2(crate::Vec2 { x: 0.0, y: 0.0 }),
        n(100.0),
        n(50.0),
        n(4.0),
        n(2.5),
        Value::str("tl"),
        Value::Null,
    ]);
    let SceneNode::Group { children, .. } = scene_of(&out[0]) else {
        panic!()
    };
    assert_eq!(children.len(), 8);
    let SceneNode::Image { dst, .. } = &*children[5] else {
        panic!()
    };
    assert_eq!(
        *dst,
        RectF {
            x: 25.0,
            y: 25.0,
            w: 25.0,
            h: 25.0
        }
    );
}

// ─── Rasterising ─────────────────────────────────────────────────────────────

/// A host that "rasterises" to solid green and counts calls.
#[derive(Default)]
struct Green(std::sync::atomic::AtomicUsize);

impl Host for Green {
    fn rasterize(&self, layers: &[Arc<SceneNode>], w: u32, h: u32) -> Option<Image> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let px = if layers.is_empty() {
            [0; 4]
        } else {
            [0, 255, 0, 255]
        };
        Some(Image::new(w, h, px.repeat((w * h) as usize)))
    }
}

fn a_layer() -> Value {
    Value::Scene(Arc::new(SceneNode::Point {
        x: 0.0,
        y: 0.0,
        style: crate::scene::default_style().clone(),
    }))
}

#[test]
fn cache_and_from_layer_use_the_host() {
    // No host: a transparent raster of the requested size.
    let mut h = node("transform/cache", json!({}));
    let out = h
        .eval(vec![vec![a_layer()], vec![n(0.0)], vec![n(-3.0)]])
        .unwrap();
    let SceneNode::Image {
        image,
        dst,
        src,
        tint,
    } = scene_of(&out[0][0])
    else {
        panic!()
    };
    assert_eq!(
        *dst,
        RectF {
            x: -360.0,
            y: -360.0,
            w: 720.0,
            h: 720.0
        }
    );
    assert_eq!(
        (image.width(), image.height(), *src, tint.clone()),
        (720, 720, None, None)
    );
    assert!(image.rgba().iter().all(|&b| b == 0));

    let host = Arc::new(Green::default());
    let _g = host::install(Some(host.clone()));
    let mut h = node("image/from-layer", json!({}));
    let layer = a_layer();
    let a = h
        .eval(vec![vec![layer.clone()], vec![n(30.5)], vec![n(20.0)]])
        .unwrap();
    let a = img(&a[0][0]).clone();
    assert_eq!((a.width(), a.height()), (30, 20));
    assert_eq!(a.pixel(5, 5), [0, 255, 0, 255]);
    // Equal layers → the same raster, no second render.
    let b = h
        .eval(vec![vec![a_layer()], vec![n(30.0)], vec![n(20.0)]])
        .unwrap();
    assert!(Arc::ptr_eq(&a, img(&b[0][0])));
    assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 1);
    let c = h.eval(vec![vec![], vec![n(30.0)], vec![n(20.0)]]).unwrap();
    assert_eq!(img(&c[0][0]).pixel(0, 0), [0, 0, 0, 0]);
}

// ─── Loaders ─────────────────────────────────────────────────────────────────

fn png_data_url(w: u32, h: u32, px: [u8; 4]) -> String {
    let mut png = Vec::new();
    ::image::RgbaImage::from_raw(w, h, px.repeat((w * h) as usize))
        .unwrap()
        .write_to(
            &mut std::io::Cursor::new(&mut png),
            ::image::ImageFormat::Png,
        )
        .unwrap();
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::from("data:image/png;base64,");
    for c in png.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(v >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Evaluate a source node until `loaded` (output `port`) is true.
fn until_loaded(h: &mut super::super::value::harness::Harness, port: usize) -> Vec<Value> {
    for _ in 0..2000 {
        let out = h.eval1(&[]);
        if matches!(out[port], Value::Bool(true)) {
            return out;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("never loaded");
}

/// `Value::same` over two lists.
fn same(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same(y))
}

#[test]
fn image_url_loads_off_thread() {
    let mut h = node("io/image-url", json!({}));
    assert!(same(
        &h.eval1(&[]),
        &[Value::Null, Value::Bool(false), n(0.0), n(0.0)]
    ));
    let mut h = node(
        "io/image-url",
        json!({ "url": png_data_url(3, 2, [9, 8, 7, 255]) }),
    );
    let first = h.eval1(&[]);
    if matches!(first[1], Value::Bool(false)) {
        assert!(!img(&first[0]).is_ready());
    }
    let out = until_loaded(&mut h, 1);
    assert!(same(&out[2..], &[n(3.0), n(2.0)]));
    assert_eq!(img(&out[0]).pixel(2, 1), [9, 8, 7, 255]);
    // Stable across ticks (same image, nothing to re-upload).
    assert!(Arc::ptr_eq(img(&out[0]), img(&h.eval1(&[])[0])));
    // A network URL without a host fails: never loaded.
    let mut h = node(
        "io/image-url",
        json!({ "url": "https://example.com/a.png" }),
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(matches!(h.eval1(&[])[1], Value::Bool(false)));
}

#[test]
fn file_node_outputs() {
    let mut h = node("io/file", json!({}));
    assert!(same(
        &h.eval1(&[]),
        &[
            Value::str(""),
            Value::Null,
            Value::str(""),
            Value::Bool(false),
            Value::str("")
        ]
    ));
    let url = png_data_url(2, 2, [1, 2, 3, 4]);
    let mut h = node(
        "io/file",
        json!({ "dataUrl": url, "filename": "a.png", "mimeType": "image/png" }),
    );
    let out = until_loaded(&mut h, 3);
    assert!(out[0].same(&Value::str(&url)));
    assert_eq!(img(&out[1]).pixel(1, 1), [1, 2, 3, 4]);
    assert!(out[2].same(&Value::str("a.png")));
    let mut h = node(
        "io/file",
        json!({ "dataUrl": "data:audio/wav;base64,AAAA", "mimeType": "audio/wav" }),
    );
    let out = h.eval1(&[]);
    assert!(same(
        &[out[1].clone(), out[3].clone()],
        &[Value::Null, Value::Bool(true)]
    ));
}

#[test]
fn svg_rasterises_to_an_image_layer() {
    let markup = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><rect width="40" height="20" fill="#0000ff"/></svg>"##;
    let mut h = node("io/svg", json!({ "mode": "markup", "markup": markup }));
    let origin = Value::Vec2(crate::Vec2 { x: 0.0, y: 0.0 });
    let out = h.eval1(&[origin.clone(), n(1.0), Value::Null]);
    assert!(same(&out[1..], &[n(40.0), n(20.0), Value::Bool(true)]));
    let SceneNode::Image { image, dst, .. } = scene_of(&out[0]) else {
        panic!()
    };
    assert_eq!(
        *dst,
        RectF {
            x: -20.0,
            y: -10.0,
            w: 40.0,
            h: 20.0
        }
    );
    // Rasterised at 2× for crispness.
    assert_eq!((image.width(), image.height()), (80, 40));
    assert_eq!(image.pixel(10, 10), [0, 0, 255, 255]);
    // Scale / position wrap it in a transformed group.
    let out = h.eval1(&[
        Value::Vec2(crate::Vec2 { x: 5.0, y: 6.0 }),
        n(3.0),
        Value::Null,
    ]);
    let SceneNode::Group {
        transform,
        children,
        ..
    } = scene_of(&out[0])
    else {
        panic!()
    };
    assert_eq!(*transform, Some([3.0, 0.0, 0.0, 3.0, 5.0, 6.0]));
    let SceneNode::Image { image, .. } = &*children[0] else {
        panic!()
    };
    assert_eq!(image.width(), 320);
    // A wired style replaces the SVG's paint.
    let red = crate::nodes::style::paint_of(&Value::Color(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }))
    .unwrap();
    let style = crate::scene::Style::chain(None, [crate::scene::StyleOp::Fill(Some(red))]);
    let out = h.eval1(&[origin, n(1.0), Value::Style(style)]);
    let SceneNode::Image { image, .. } = scene_of(&out[0]) else {
        panic!()
    };
    assert_eq!(image.pixel(10, 10), [255, 0, 0, 255]);
    // Nothing to load.
    let mut h = node("io/svg", json!({ "mode": "url" }));
    assert!(matches!(
        h.eval1(&[Value::Null, Value::Null, Value::Null])[3],
        Value::Bool(false)
    ));
}
