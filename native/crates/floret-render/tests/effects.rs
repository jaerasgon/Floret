//! Gradient fills, masks and post-effects (`style/gradient`,
//! `transform/mask`, `gpu/effect`) on the GPU, checked pixel by pixel, and
//! their samples drawn. `FLORET_RENDER_OUT=<dir>` keeps the sample frames.
//! Skips when no GPU adapter is available.

mod common;

use common::{out_dir, reference_dir, write_png};
use floret_engine::scene::{
    default_style, Gradient, GradientExtend, GradientShape, GradientStop, GradientUnits, Paint,
    ResolvedStyle, SceneNode,
};
use floret_engine::shader::ShaderShape;
use floret_engine::{Engine, FrameCtx, PatchFile, Value, Vec2};
use floret_render::{Headless, RenderClock};
use serde_json::json;
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

fn render(g: &mut Headless, layers: &[Arc<SceneNode>]) -> Vec<u8> {
    g.renderer.set_clock(RenderClock::default());
    g.render(layers, W, H, BG)
}

/// Pixel at scene coordinates (centre origin).
fn at(px: &[u8], x: i32, y: i32) -> [u8; 3] {
    let (ix, iy) = ((x + W as i32 / 2) as usize, (y + H as i32 / 2) as usize);
    let o = (iy * W as usize + ix) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

#[track_caller]
fn assert_px(px: &[u8], x: i32, y: i32, want: [u8; 3], tol: u8) {
    let got = at(px, x, y);
    assert!(
        (0..3).all(|i| got[i].abs_diff(want[i]) <= tol),
        "pixel ({x},{y}) = {got:?}, want {want:?}"
    );
}

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

fn ellipse(x: f64, y: f64, d: f64, st: Arc<ResolvedStyle>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Ellipse {
        x,
        y,
        w: d,
        h: d,
        mode: Arc::from("center"),
        style: st,
    })
}

fn solid(css: &str) -> Arc<ResolvedStyle> {
    style(|s| s.fill = Some(Paint::css(css)))
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

const RED: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f64; 4] = [0.0, 0.0, 1.0, 1.0];

fn gradient(shape: GradientShape, units: GradientUnits, stops: &[[f64; 4]]) -> Paint {
    let n = stops.len();
    Paint::gradient(Gradient {
        shape,
        stops: stops
            .iter()
            .enumerate()
            .map(|(i, &rgba)| GradientStop {
                pos: i as f64 / (n - 1).max(1) as f64,
                rgba,
            })
            .collect(),
        units,
        extend: GradientExtend::Pad,
    })
}

fn linear_box() -> Paint {
    let shape = GradientShape::Linear {
        start: v(-1.0, 0.0),
        end: v(1.0, 0.0),
    };
    gradient(shape, GradientUnits::Box, &[RED, BLUE])
}

// ─── Gradients ───────────────────────────────────────────────────────────────

#[test]
fn linear_gradient_spans_the_shape() {
    let Some(mut g) = gpu() else { return };
    let st = style(|s| s.fill = Some(linear_box()));
    // A rect from x = -80 to 80, offset down: shape units follow its box.
    let px = render(&mut g, &[rect(0.0, 40.0, 160.0, 40.0, st)]);
    assert_px(&px, -77, 40, [250, 0, 5], 6);
    assert_px(&px, 77, 40, [5, 0, 250], 6);
    assert_px(&px, 0, 40, [128, 0, 128], 4);
    // Outside the rect: background.
    assert_px(&px, 0, -40, [0, 0, 0], 0);
}

#[test]
fn gradients_follow_transforms_and_polygons() {
    let Some(mut g) = gpu() else { return };
    // Canvas units, rotated 90° by the group: the gradient turns with it.
    let shape = GradientShape::Linear {
        start: v(-80.0, 0.0),
        end: v(80.0, 0.0),
    };
    let st = style(|s| s.fill = Some(gradient(shape, GradientUnits::Scene, &[RED, BLUE])));
    let poly = Arc::new(SceneNode::Polygon {
        points: Arc::from(
            [(-80.0, -30.0), (80.0, -30.0), (80.0, 30.0), (-80.0, 30.0)]
                .map(|(x, y)| v(x, y))
                .to_vec(),
        ),
        closed: true,
        style: style(|s| {
            s.fill = st.fill.clone();
            s.stroke = None;
        }),
        tension: None,
    });
    let group = Arc::new(SceneNode::Group {
        children: Arc::from(vec![poly]),
        transform: Some(floret_engine::scene::rotate_mat(
            std::f64::consts::FRAC_PI_2,
        )),
        blend_mode: None,
        clip: None,
    });
    let px = render(&mut g, &[group]);
    // Local -x is now up.
    assert_px(&px, 0, -78, [255, 0, 0], 8);
    assert_px(&px, 0, 78, [0, 0, 255], 8);
    assert_px(&px, 0, 0, [128, 0, 128], 4);
}

#[test]
fn radial_and_conic_gradients() {
    let Some(mut g) = gpu() else { return };
    let white = [1.0, 1.0, 1.0, 1.0];
    let black = [0.0, 0.0, 0.0, 1.0];
    let radial = GradientShape::Radial {
        center: v(0.0, 0.0),
        radius: 1.0,
    };
    let st = style(|s| s.fill = Some(gradient(radial, GradientUnits::Box, &[white, black])));
    let px = render(&mut g, &[ellipse(0.0, 0.0, 160.0, st)]);
    assert_px(&px, 0, 0, [255, 255, 255], 4);
    // Half the radius → mid grey, in every direction.
    assert_px(&px, 40, 0, [128, 128, 128], 4);
    assert_px(&px, 0, -40, [128, 128, 128], 4);
    assert_px(&px, 77, 0, [10, 10, 10], 10);

    // Conic from straight up (−π/2), clockwise: red → blue over the turn.
    let conic = GradientShape::Conic {
        center: v(0.0, 0.0),
        angle: -std::f64::consts::FRAC_PI_2,
    };
    let st = style(|s| s.fill = Some(gradient(conic, GradientUnits::Scene, &[RED, BLUE])));
    let px = render(&mut g, &[rect(0.0, 0.0, 200.0, 200.0, st)]);
    assert_px(&px, 2, -60, [255, 0, 0], 12);
    assert_px(&px, 60, 0, [191, 0, 64], 6);
    assert_px(&px, 0, 60, [128, 0, 128], 6);
    assert_px(&px, -60, 0, [64, 0, 191], 6);
}

#[test]
fn gradient_strokes_and_repeat() {
    let Some(mut g) = gpu() else { return };
    // A stroked line, coloured along its length.
    let line = Arc::new(SceneNode::Line {
        x1: -80.0,
        y1: 0.0,
        x2: 80.0,
        y2: 0.0,
        style: style(|s| {
            s.stroke = Some(linear_box());
            s.stroke_weight = 10.0;
        }),
    });
    let px = render(&mut g, &[line]);
    assert_px(&px, -78, 0, [255, 0, 0], 8);
    assert_px(&px, 78, 0, [0, 0, 255], 8);
    // Repeat: two cycles across the box.
    let mut p = linear_box();
    let mut gr = (**p.gradient.as_ref().unwrap()).clone();
    gr.shape = GradientShape::Linear {
        start: v(-1.0, 0.0),
        end: v(0.0, 0.0),
    };
    gr.extend = GradientExtend::Repeat;
    p = Paint::gradient(gr);
    let st = style(|s| s.fill = Some(p));
    let px = render(&mut g, &[rect(0.0, 0.0, 200.0, 40.0, st)]);
    assert_px(&px, -98, 0, [255, 0, 0], 8);
    assert_px(&px, 2, 0, [255, 0, 0], 8);
    assert_px(&px, -50, 0, [128, 0, 128], 6);
    assert_px(&px, 50, 0, [128, 0, 128], 6);
}

// ─── Masks ───────────────────────────────────────────────────────────────────

fn mask(content: Arc<SceneNode>, mask: Arc<SceneNode>, lum: bool, inv: bool) -> Arc<SceneNode> {
    Arc::new(SceneNode::Mask {
        content,
        mask,
        luminance: lum,
        invert: inv,
    })
}

#[test]
fn masks_by_alpha_with_invert() {
    let Some(mut g) = gpu() else { return };
    let content = rect(0.0, 0.0, 180.0, 180.0, solid("#ff0000"));
    let hole = ellipse(0.0, 0.0, 80.0, solid("#ffffff"));
    let px = render(&mut g, &[mask(content.clone(), hole.clone(), false, false)]);
    assert_px(&px, 0, 0, [255, 0, 0], 0);
    assert_px(&px, 70, 70, [0, 0, 0], 0);
    let px = render(&mut g, &[mask(content.clone(), hole.clone(), false, true)]);
    assert_px(&px, 0, 0, [0, 0, 0], 0);
    assert_px(&px, 70, 70, [255, 0, 0], 0);
    // Outside the content nothing shows either way.
    assert_px(&px, 95, 95, [0, 0, 0], 0);
    // A half-transparent mask lets half through.
    let half = ellipse(0.0, 0.0, 80.0, solid("rgba(0,0,0,0.5)"));
    let px = render(&mut g, &[mask(content, half, false, false)]);
    assert_px(&px, 0, 0, [128, 0, 0], 2);
}

#[test]
fn masks_by_luminance() {
    let Some(mut g) = gpu() else { return };
    let content = rect(0.0, 0.0, 180.0, 180.0, solid("#00ff00"));
    // Opaque mask: white on the left, mid grey top-right, black bottom-right.
    let m = Arc::new(SceneNode::group(vec![
        rect(-50.0, 0.0, 100.0, 200.0, solid("#ffffff")),
        rect(50.0, -50.0, 100.0, 100.0, solid("#808080")),
        rect(50.0, 50.0, 100.0, 100.0, solid("#000000")),
    ]));
    let px = render(&mut g, &[mask(content.clone(), m.clone(), true, false)]);
    assert_px(&px, -50, 0, [0, 255, 0], 1);
    assert_px(&px, 50, -50, [0, 128, 0], 2);
    assert_px(&px, 50, 50, [0, 0, 0], 0);
    // By alpha the same opaque mask shows everything.
    let px = render(&mut g, &[mask(content.clone(), m.clone(), false, false)]);
    assert_px(&px, 50, 50, [0, 255, 0], 0);
    let px = render(&mut g, &[mask(content, m, true, true)]);
    assert_px(&px, -50, 0, [0, 0, 0], 0);
    assert_px(&px, 50, 50, [0, 255, 0], 1);
}

// ─── Effects ─────────────────────────────────────────────────────────────────

/// The `gpu/effect` shape for `mode` with `values` on its pins, built by
/// the engine from a patch (input: a placeholder rect, replaced by tests).
fn effect(mode: &str, values: serde_json::Value) -> ShaderShape {
    let patch = json!({
        "kind": "floret.patch",
        "version": 2,
        "nodes": [
            { "id": "r", "defType": "draw/rect", "position": {"x": 0, "y": 0} },
            { "id": "m", "defType": "draw/rect", "position": {"x": 0, "y": 100} },
            {
                "id": "fx", "defType": "gpu/effect", "position": {"x": 200, "y": 0},
                "params": { "mode": mode },
                "portValues": values,
            },
            {
                "id": "out", "defType": "render/renderer", "position": {"x": 400, "y": 0},
                "params": { "width": W, "height": H },
            },
        ],
        "edges": [
            { "id": "e1", "source": "r", "target": "fx", "sourceHandle": "out-shape", "targetHandle": "in-input" },
            { "id": "e2", "source": "fx", "target": "out", "sourceHandle": "out-out", "targetHandle": "in-layers" },
        ],
    });
    let mut engine = Engine::load(&patch.to_string()).expect("patch loads");
    let renderer = engine.graph.renderer().expect("a renderer");
    let r = engine.tick(&FrameCtx::default());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let layers = r.layers(renderer);
    match layers.first() {
        Some(Value::Scene(n)) => match &**n {
            SceneNode::Shader(sh) => (**sh).clone(),
            other => panic!("not a shader: {other:?}"),
        },
        other => panic!("no layer: {other:?}"),
    }
}

fn with_input(mut sh: ShaderShape, input: Arc<SceneNode>) -> Arc<SceneNode> {
    sh.input = Some(input);
    Arc::new(SceneNode::Shader(Arc::new(sh)))
}

fn mean(px: &[u8]) -> f64 {
    px.chunks(4)
        .map(|p| f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2]))
        .sum::<f64>()
        / (px.len() / 4 * 3) as f64
}

#[test]
fn every_effect_compiles() {
    let Some(mut g) = gpu() else { return };
    for mode in floret_engine::nodes::effect::MODES {
        let sh = effect(mode, json!({}));
        for p in sh.passes.iter() {
            if let Err(e) = floret_render::shader::check(&p.source) {
                panic!("{mode} {:?}: {e:?}", p.buf);
            }
        }
        let node = with_input(sh, rect(0.0, 0.0, 100.0, 100.0, solid("#ffffff")));
        render(&mut g, &[node]);
        assert!(
            g.renderer.shader_errors().is_empty(),
            "{mode}: {:?}",
            g.renderer.shader_errors()
        );
    }
}

#[test]
fn vignette_darkens_the_corners() {
    let Some(mut g) = gpu() else { return };
    let sh = effect(
        "vignette",
        json!({ "amount": 1.0, "radius": 0.5, "softness": 0.6 }),
    );
    let node = with_input(sh, rect(0.0, 0.0, 200.0, 200.0, solid("#ffffff")));
    let px = render(&mut g, &[node]);
    assert_px(&px, 0, 0, [255, 255, 255], 2);
    let corner = at(&px, -99, -99);
    assert!(corner[0] < 40, "corner {corner:?}");
    // Darker towards the corners all the way.
    assert!(at(&px, 60, 60)[0] < at(&px, 30, 30)[0]);
    // Over a transparent layer the vignette still darkens the background
    // it composites onto: here, a white backdrop.
    let sh = effect(
        "vignette",
        json!({ "amount": 1.0, "radius": 0.5, "softness": 0.6 }),
    );
    let node = with_input(sh, rect(0.0, 0.0, 10.0, 10.0, solid("#ffffff")));
    let px = g.render(&[node], W, H, [1.0, 1.0, 1.0, 1.0]);
    assert!(at(&px, -99, -99)[0] < 40);
}

#[test]
fn grain_animates_by_itself() {
    let Some(mut g) = gpu() else { return };
    let sh = effect("grain", json!({ "amount": 0.3 }));
    let node = with_input(sh, rect(0.0, 0.0, 200.0, 200.0, solid("#808080")));
    let frame = |g: &mut Headless, f: f64| {
        g.renderer.set_clock(RenderClock {
            frame: f,
            time: f / 60.0,
            ..RenderClock::default()
        });
        g.render(std::slice::from_ref(&node), W, H, BG)
    };
    let a = frame(&mut g, 1.0);
    let b = frame(&mut g, 2.0);
    let again = frame(&mut g, 1.0);
    assert_ne!(a, b, "grain changes every frame");
    assert_eq!(a, again, "and is deterministic per frame");
    // Noisy around the input's grey, without shifting it much.
    let spread =
        a.chunks(4).map(|p| p[0]).max().unwrap() - a.chunks(4).map(|p| p[0]).min().unwrap();
    assert!(spread > 40, "spread {spread}");
    assert!((mean(&a) - 128.0).abs() < 8.0, "mean {}", mean(&a));
}

#[test]
fn bloom_spreads_light_around_bright_shapes() {
    let Some(mut g) = gpu() else { return };
    let dot = ellipse(0.0, 0.0, 20.0, solid("#ffffff"));
    let plain = render(&mut g, std::slice::from_ref(&dot));
    let sh = effect(
        "bloom",
        json!({ "threshold": 0.5, "radius": 30.0, "intensity": 2.0 }),
    );
    let px = render(&mut g, &[with_input(sh, dot.clone())]);
    assert_px(&plain, 16, 0, [0, 0, 0], 0);
    assert!(at(&px, 16, 0)[0] > 30, "glow {:?}", at(&px, 16, 0));
    assert_px(&px, 0, 0, [255, 255, 255], 0);
    // Below the threshold nothing glows.
    let dim = ellipse(0.0, 0.0, 20.0, solid("#404040"));
    let sh = effect(
        "bloom",
        json!({ "threshold": 0.5, "radius": 30.0, "intensity": 2.0 }),
    );
    let px = render(&mut g, &[with_input(sh, dim)]);
    assert_px(&px, 20, 0, [0, 0, 0], 1);
}

#[test]
fn chromatic_aberration_splits_channels_outwards() {
    let Some(mut g) = gpu() else { return };
    let sh = effect("chromatic", json!({ "amount": 10.0 }));
    let px = render(
        &mut g,
        &[with_input(
            sh,
            rect(0.0, 0.0, 120.0, 120.0, solid("#ffffff")),
        )],
    );
    // Right edge (x = 60): red reaches outwards, blue pulls in.
    let out = at(&px, 63, 0);
    assert!(out[0] > 200 && out[2] < 30, "outside the edge {out:?}");
    let inside = at(&px, 57, 0);
    assert!(
        inside[0] > 200 && inside[2] < 30,
        "inside the edge {inside:?}"
    );
    assert_px(&px, 0, 0, [255, 255, 255], 1);
}

#[test]
fn dither_quantises_to_levels() {
    let Some(mut g) = gpu() else { return };
    let sh = effect("dither", json!({ "levels": 2, "size": 1 }));
    let node = with_input(sh, rect(0.0, 0.0, 200.0, 200.0, solid("#808080")));
    let px = render(&mut g, &[node]);
    assert!(px.chunks(4).all(|p| p[0] == 0 || p[0] == 255));
    // Half the pixels lit, give or take.
    let lit = px.chunks(4).filter(|p| p[0] == 255).count() as f64 / (W * H) as f64;
    assert!((lit - 0.5).abs() < 0.05, "lit {lit}");
}

#[test]
fn halftone_makes_dots() {
    let Some(mut g) = gpu() else { return };
    let sh = effect("halftone", json!({ "size": 10.0, "angle": 0.0 }));
    let node = with_input(sh, rect(0.0, 0.0, 200.0, 200.0, solid("#ff8000")));
    let px = render(&mut g, &[node]);
    // Cell corners stay dark, cell centres take the colour.
    let lit = px.chunks(4).filter(|p| p[0] > 200).count() as f64 / (W * H) as f64;
    assert!(lit > 0.15 && lit < 0.8, "lit {lit}");
    assert!(px.chunks(4).any(|p| p[0] < 10));
    assert!(px
        .chunks(4)
        .any(|p| p[0] > 250 && (110..150).contains(&p[1])));
}

#[test]
fn displacement_moves_pixels() {
    let Some(mut g) = gpu() else { return };
    let stripe = rect(0.0, 0.0, 20.0, 200.0, solid("#ffffff"));
    // A map whose red is 1 everywhere: push right by `amount`.
    let mut sh = effect("displace", json!({ "amount": 30.0 }));
    sh.map = Some(rect(0.0, 0.0, 200.0, 200.0, solid("#ff8080")));
    sh.passes = Arc::from(floret_engine::nodes::effect::passes("displace", true));
    let px = render(&mut g, &[with_input(sh, stripe.clone())]);
    assert_px(&px, 30, 0, [255, 255, 255], 2);
    assert_px(&px, 0, 0, [0, 0, 0], 2);
    // Noise displacement changes the picture.
    let sh = effect("displace", json!({ "amount": 20.0, "scale": 20.0 }));
    let px = render(&mut g, &[with_input(sh, stripe.clone())]);
    let plain = render(&mut g, &[stripe]);
    assert_ne!(px, plain);
}

// ─── Samples ─────────────────────────────────────────────────────────────────

/// A sample after `frames` ticks at 60 fps, drawn: (pixels, width, height).
fn draw_sample(g: &mut Headless, name: &str, frames: u32) -> (Vec<u8>, u32, u32) {
    let path = reference_dir().join(format!("../../../samples/{name}.flora"));
    let patch = PatchFile::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let (mut engine, _) = Engine::from_patch(&patch).expect("compiles");
    let r = engine.graph.renderer().expect("a renderer");
    let id = engine.graph.nodes[r].id.clone();
    let (w, h) = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let dt = 1.0 / 60.0;
    let frame = |f: u32| FrameCtx {
        t: f64::from(f) * dt,
        dt,
        frame_count: f64::from(f),
        width: f64::from(w),
        height: f64::from(h),
        ..FrameCtx::default()
    };
    let mut outputs = Vec::new();
    for f in 0..frames {
        let t = engine.tick(&frame(f));
        assert!(t.errors.is_empty(), "{name}: {:?}", t.errors);
        outputs = t.outputs;
    }
    let out = outputs[r].as_ref().expect("renderer output");
    let layers = floret_render::flatten_layers(&out[0]);
    let bg = out
        .get(1)
        .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
    let f = frame(frames);
    g.renderer.set_clock(RenderClock {
        time: f.t,
        frame: f.frame_count,
        ..RenderClock::default()
    });
    let (px, stats) = g.render_with_stats(&layers, w, h, bg);
    assert_eq!(stats.skipped, 0, "{name}: skipped shapes");
    assert!(
        g.renderer.shader_errors().is_empty(),
        "{name}: shader errors"
    );
    if let Some(dir) = out_dir() {
        write_png(&dir.join(format!("{name}-f{frames}.png")), w, h, &px);
    }
    (px, w, h)
}

fn px_at(px: &[u8], w: u32, h: u32, x: i32, y: i32) -> [u8; 3] {
    let (ix, iy) = ((x + w as i32 / 2) as usize, (y + h as i32 / 2) as usize);
    let o = (iy * w as usize + ix) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

#[test]
fn gradient_sunset_sample_draws() {
    let Some(mut g) = gpu() else { return };
    let (px, w, h) = draw_sample(&mut g, "gradient-sunset", 30);
    // The sky runs from indigo at the top to orange at the bottom.
    let (top, bottom) = (px_at(&px, w, h, -250, -290), px_at(&px, w, h, -250, 290));
    assert!(
        top[2] > top[0] && bottom[0] > 200,
        "sky {top:?} → {bottom:?}"
    );
    // The sun's top half is whole and yellow-ish.
    let sun = px_at(&px, w, h, 0, -100);
    assert!(sun[0] > 230 && sun[1] > 150, "sun {sun:?}");
    // Its lower half is cut into stripes: a column through it alternates.
    let column: Vec<u8> = (40..150).map(|y| px_at(&px, w, h, 0, y)[1]).collect();
    let flips = column
        .windows(2)
        .filter(|p| p[0].abs_diff(p[1]) > 40)
        .count();
    assert!(flips >= 6, "stripes {flips}");
}

#[test]
fn neon_bloom_sample_draws() {
    let Some(mut g) = gpu() else { return };
    let (a, w, h) = draw_sample(&mut g, "neon-bloom", 30);
    // Vignetted corners, a glowing centre.
    let corner = px_at(&a, w, h, -295, -295);
    assert!(corner.iter().all(|c| *c < 12), "corner {corner:?}");
    let core = px_at(&a, w, h, 0, 0);
    assert!(core.iter().all(|c| *c > 200), "core {core:?}");
    // Glow spills off the core well past its 18 px radius.
    let halo = px_at(&a, w, h, 30, 0);
    assert!(halo.iter().any(|c| *c > 40), "halo {halo:?}");
}
