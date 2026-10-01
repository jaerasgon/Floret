//! Feature checks on hand-built scenes with pixel values we can reason
//! about. Skips when no GPU adapter is available.

use floret_engine::scene::{default_style, Paint, RectF, ResolvedStyle, SceneNode};
use floret_engine::Vec2;
use floret_render::Headless;
use std::sync::{Arc, Mutex, OnceLock};

fn gpu() -> Option<std::sync::MutexGuard<'static, Headless>> {
    static GPU: OnceLock<Option<Mutex<Headless>>> = OnceLock::new();
    GPU.get_or_init(|| Headless::new().map(Mutex::new))
        .as_ref()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

const W: u32 = 200;
const H: u32 = 200;
const BG: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

fn style(f: impl FnOnce(&mut ResolvedStyle)) -> Arc<ResolvedStyle> {
    let mut s = (**default_style()).clone();
    f(&mut s);
    Arc::new(s)
}

fn rect(x: f64, y: f64, w: f64, h: f64, st: Arc<ResolvedStyle>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Rect {
        x,
        y,
        w,
        h,
        mode: Arc::from("center"),
        style: st,
    })
}

fn fill(css: &str) -> Arc<ResolvedStyle> {
    style(|s| s.fill = Some(Paint::css(css)))
}

/// Pixel at scene coordinates (centre origin).
fn at(px: &[u8], x: i32, y: i32) -> [u8; 3] {
    let (ix, iy) = ((x + W as i32 / 2) as usize, (y + H as i32 / 2) as usize);
    let o = (iy * W as usize + ix) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

fn close(a: [u8; 3], b: [u8; 3], tol: u8) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) <= tol)
}

#[track_caller]
fn assert_px(px: &[u8], x: i32, y: i32, want: [u8; 3]) {
    let got = at(px, x, y);
    assert!(
        close(got, want, 3),
        "pixel ({x},{y}) = {got:?}, want {want:?}"
    );
}

#[test]
fn empty_scene_is_the_opaque_background() {
    let Some(mut g) = gpu() else { return };
    // Alpha is ignored, like the web app.
    let px = g.render(&[], 64, 48, [0.2, 0.4, 0.6, 0.0]);
    assert_eq!(px.len(), 64 * 48 * 4);
    assert!(px.chunks(4).all(|p| p == [51, 102, 153, 255]));
}

#[test]
fn centre_origin_y_down() {
    let Some(mut g) = gpu() else { return };
    let px = g.render(&[rect(-50.0, -50.0, 20.0, 20.0, fill("#ff0000"))], W, H, BG);
    // Top-left quadrant of the image.
    assert_px(&px, -50, -50, [255, 0, 0]);
    assert_px(&px, 50, 50, [0, 0, 0]);
    assert_px(&px, -50, 50, [0, 0, 0]);
}

#[test]
fn premultiplied_alpha_blends_like_canvas() {
    let Some(mut g) = gpu() else { return };
    let px = g.render(
        &[
            rect(0.0, 0.0, 100.0, 100.0, fill("#ffffff")),
            rect(0.0, 0.0, 50.0, 50.0, fill("rgba(255,0,0,0.5)")),
        ],
        W,
        H,
        BG,
    );
    assert_px(&px, 0, 0, [255, 128, 128]);
    assert_px(&px, 40, 40, [255, 255, 255]);
}

#[test]
fn stroke_align_inside_outside_centre() {
    let Some(mut g) = gpu() else { return };
    let mk = |x: f64, align: &str| {
        let a: Arc<str> = Arc::from(align);
        rect(
            x,
            0.0,
            40.0,
            40.0,
            style(|s| {
                s.fill = Some(Paint::css("#0000ff"));
                s.stroke = Some(Paint::css("#ffffff"));
                s.stroke_weight = 8.0;
                s.stroke_align = a;
            }),
        )
    };
    let px = g.render(
        &[mk(-60.0, "inside"), mk(0.0, "center"), mk(60.0, "outside")],
        W,
        H,
        BG,
    );
    // Edges at ±20 from each centre.
    // inside: stroke covers 12..20 from centre; outside it's background.
    assert_px(&px, -60 + 16, 0, [255, 255, 255]);
    assert_px(&px, -60 + 23, 0, [0, 0, 0]);
    assert_px(&px, -60 + 8, 0, [0, 0, 255]);
    // centre: 16..24
    assert_px(&px, 18, 0, [255, 255, 255]);
    assert_px(&px, 22, 0, [255, 255, 255]);
    assert_px(&px, 13, 0, [0, 0, 255]);
    assert_px(&px, 26, 0, [0, 0, 0]);
    // outside: 20..28; the fill reaches the edge.
    assert_px(&px, 60 + 18, 0, [0, 0, 255]);
    assert_px(&px, 60 + 24, 0, [255, 255, 255]);
    assert_px(&px, 60 + 30, 0, [0, 0, 0]);
}

#[test]
fn blend_modes_composite_groups() {
    let Some(mut g) = gpu() else { return };
    let grey = rect(0.0, 0.0, 200.0, 200.0, fill("#808080"));
    let group = |x: f64, mode: Option<&str>| {
        Arc::new(SceneNode::Group {
            children: Arc::from(vec![rect(x, 0.0, 30.0, 30.0, fill("rgb(255,51,51)"))]),
            transform: None,
            blend_mode: mode.map(Arc::from),
            clip: None,
        })
    };
    let px = g.render(
        &[
            grey,
            group(-75.0, Some("source-over")),
            group(-25.0, Some("multiply")),
            group(25.0, Some("screen")),
            group(75.0, Some("lighter")),
        ],
        W,
        H,
        BG,
    );
    let (r, b) = (255.0 / 255.0, 51.0 / 255.0);
    let gr = 128.0 / 255.0;
    let q = |v: f64| (v.min(1.0) * 255.0).round() as u8;
    assert_px(&px, -75, 0, [255, 51, 51]);
    assert_px(&px, -25, 0, [q(r * gr), q(b * gr), q(b * gr)]);
    let screen = |a: f64| q(1.0 - (1.0 - a) * (1.0 - gr));
    assert_px(&px, 25, 0, [screen(r), screen(b), screen(b)]);
    assert_px(&px, 75, 0, [255, q(b + gr), q(b + gr)]);
    // Outside the shapes the backdrop is untouched.
    assert_px(&px, -25, 50, [128, 128, 128]);
}

#[test]
fn rotated_clips_are_exact() {
    let Some(mut g) = gpu() else { return };
    let a: f64 = 0.5;
    let (s, c) = a.sin_cos();
    let clip = RectF {
        x: -30.0,
        y: -40.0,
        w: 60.0,
        h: 80.0,
    };
    let node = Arc::new(SceneNode::Group {
        children: Arc::from(vec![rect(0.0, 0.0, 400.0, 400.0, fill("#ffffff"))]),
        transform: Some([c, s, -s, c, 0.0, 0.0]),
        blend_mode: None,
        clip: Some(clip),
    });
    let px = g.render(&[node], W, H, BG);
    let mut checked = 0;
    for iy in 0..H as i32 {
        for ix in 0..W as i32 {
            // Pixel centre → clip-local (inverse rotation).
            let (wx, wy) = (ix as f64 + 0.5 - 100.0, iy as f64 + 0.5 - 100.0);
            let (lx, ly) = (c * wx + s * wy, -s * wx + c * wy);
            let dist = (lx.abs() - 30.0).max(ly.abs() - 40.0);
            let v = at(&px, ix - 100, iy - 100)[0];
            if dist < -1.5 {
                assert_eq!(v, 255, "inside at ({ix},{iy})");
                checked += 1;
            } else if dist > 1.5 {
                assert_eq!(v, 0, "outside at ({ix},{iy})");
                checked += 1;
            }
        }
    }
    assert!(checked > 30_000);
}

#[test]
fn nonzero_winding_fills_self_intersections() {
    let Some(mut g) = gpu() else { return };
    let pts: Vec<Vec2> = (0..5)
        .map(|i| {
            let a = f64::from(i) * 4.0 / 5.0 * std::f64::consts::PI - std::f64::consts::FRAC_PI_2;
            Vec2 {
                x: a.cos() * 80.0,
                y: a.sin() * 80.0,
            }
        })
        .collect();
    let node = Arc::new(SceneNode::Polygon {
        points: Arc::from(pts),
        closed: true,
        style: style(|s| {
            s.fill = Some(Paint::css("#00ff00"));
            s.stroke = None;
        }),
        tension: None,
    });
    let px = g.render(&[node], W, H, BG);
    // The pentagram's centre has winding 2: filled under nonzero.
    assert_px(&px, 0, 0, [0, 255, 0]);
    // A star point and the gap between two points.
    assert_px(&px, 0, -70, [0, 255, 0]);
    assert_px(&px, 0, 75, [0, 0, 0]);
}

#[test]
fn blur_spreads_but_keeps_mass() {
    let Some(mut g) = gpu() else { return };
    let sharp = g.render(&[rect(0.0, 0.0, 40.0, 40.0, fill("#ffffff"))], W, H, BG);
    let blurred = g.render(
        &[rect(
            0.0,
            0.0,
            40.0,
            40.0,
            style(|s| {
                s.fill = Some(Paint::css("#ffffff"));
                s.blur = 6.0;
            }),
        )],
        W,
        H,
        BG,
    );
    let mass = |p: &[u8]| p.chunks(4).map(|c| u64::from(c[0])).sum::<u64>();
    let (m0, m1) = (mass(&sharp) as f64, mass(&blurred) as f64);
    assert!((m1 / m0 - 1.0).abs() < 0.03, "mass {m0} → {m1}");
    // Light leaks past the edge; the corner darkens.
    assert_eq!(at(&sharp, 26, 0)[0], 0);
    assert!(at(&blurred, 26, 0)[0] > 20);
    assert!(at(&blurred, 19, 19)[0] < 200);
    assert_px(&blurred, 0, 0, [255, 255, 255]);
}

#[test]
fn text_draws_inside_its_box() {
    let Some(mut g) = gpu() else { return };
    let node = Arc::new(SceneNode::Text {
        text: Arc::from("Hello"),
        x: -40.0,
        y: 0.0,
        style: style(|s| {
            s.fill = Some(Paint::css("#ffffff"));
            s.font_size = 20.0;
        }),
    });
    let (px, stats) = g.render_with_stats(&[node], W, H, BG);
    assert_eq!(stats.text_quads, 1);
    let mut ink = 0;
    for (i, p) in px.chunks(4).enumerate() {
        if p[0] > 128 {
            ink += 1;
            let (x, y) = (i as i32 % W as i32 - 100, i as i32 / W as i32 - 100);
            // Alphabetic baseline at y = 0: glyphs sit above it (no descenders).
            assert!(
                (-40..20).contains(&x) && (-17..=1).contains(&y),
                "ink at ({x},{y})"
            );
        }
    }
    assert!(ink > 80, "ink {ink}");
}

#[test]
fn bad_geometry_is_skipped_not_fatal() {
    let Some(mut g) = gpu() else { return };
    let nan = rect(f64::NAN, 0.0, 10.0, 10.0, fill("#ffffff"));
    let inf = Arc::new(SceneNode::Line {
        x1: 0.0,
        y1: 0.0,
        x2: f64::INFINITY,
        y2: 0.0,
        style: floret_engine::scene::line_default_style().clone(),
    });
    let ok = rect(0.0, 0.0, 10.0, 10.0, fill("#ffffff"));
    let (px, stats) = g.render_with_stats(&[nan, inf, ok], W, H, BG);
    assert_eq!(stats.skipped, 2);
    assert_px(&px, 0, 0, [255, 255, 255]);
}

// ─── Images ──────────────────────────────────────────────────────────────────

use floret_engine::image::Image;

/// 2×2: red, green / blue, half-transparent white.
fn quad_image() -> Arc<Image> {
    Arc::new(Image::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 128,
        ],
    ))
}

fn image(img: &Arc<Image>, src: Option<RectF>, dst: RectF, tint: Option<&str>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Image {
        image: img.clone(),
        src,
        dst,
        tint: tint.map(Paint::css),
    })
}

const BOX: RectF = RectF {
    x: -50.0,
    y: -50.0,
    w: 100.0,
    h: 100.0,
};

#[test]
fn images_draw_as_textured_quads() {
    let Some(mut g) = gpu() else { return };
    let img = quad_image();
    let (px, stats) = g.render_with_stats(&[image(&img, None, BOX, None)], W, H, BG);
    assert_eq!(stats.image_quads, 1);
    // Near the corners the linear sampler clamps to one texel.
    assert_px(&px, -40, -40, [255, 0, 0]);
    assert_px(&px, 40, -40, [0, 255, 0]);
    assert_px(&px, -40, 40, [0, 0, 255]);
    // Straight alpha 128 over black (premultiplied on upload).
    assert_px(&px, 40, 40, [128, 128, 128]);
    assert_px(&px, 70, 0, [0, 0, 0]);
    // A source rect picks the green texel; a tint multiplies.
    let src = Some(RectF {
        x: 1.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    });
    let px = g.render(
        &[image(&img, src, BOX, Some("rgba(255,255,255,0.5)"))],
        W,
        H,
        BG,
    );
    assert_px(&px, 0, 0, [0, 128, 0]);
    // Group transforms apply; the cached texture is reused.
    let g2 = Arc::new(SceneNode::Group {
        children: Arc::from(vec![image(&img, None, BOX, None)]),
        transform: Some([0.5, 0.0, 0.0, 0.5, 40.0, 0.0]),
        blend_mode: None,
        clip: None,
    });
    let px = g.render(&[g2], W, H, BG);
    assert_px(&px, 18, -22, [255, 0, 0]);
    assert_px(&px, -25, -25, [0, 0, 0]);
}

#[test]
fn changing_images_reupload() {
    let Some(mut g) = gpu() else { return };
    for c in [[255u8, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]] {
        let img = Arc::new(Image::new(4, 4, c.repeat(16)));
        let px = g.render(&[image(&img, None, BOX, None)], W, H, BG);
        assert_px(&px, 0, 0, [c[0], c[1], c[2]]);
    }
}

#[test]
fn layer_rasterizer_renders_transparent_images() {
    let r = floret_render::LayerRasterizer::new();
    let Some(img) = r.rasterize(&[rect(0.0, 0.0, 10.0, 10.0, fill("#ff0000"))], 20, 20) else {
        return;
    };
    assert_eq!((img.width(), img.height()), (20, 20));
    assert_eq!(img.pixel(10, 10), [255, 0, 0, 255]);
    assert_eq!(img.pixel(1, 1)[3], 0);
}
