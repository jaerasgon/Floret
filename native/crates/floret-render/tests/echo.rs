//! `gpu/echo` (trails, echo, delay, freeze, slit-scan) on the GPU, frame
//! by frame. Skips when no GPU adapter is available.

use floret_engine::scene::{default_style, ResolvedStyle, SceneNode};
use floret_engine::shader::ShaderShape;
use floret_engine::{Engine, FrameCtx, Value};
use floret_render::{Headless, RenderClock};
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};

fn gpu() -> Option<std::sync::MutexGuard<'static, Headless>> {
    static GPU: OnceLock<Option<Mutex<Headless>>> = OnceLock::new();
    GPU.get_or_init(|| Headless::new().map(Mutex::new))
        .as_ref()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

const W: u32 = 120;
const H: u32 = 80;
const BG: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

/// The `gpu/echo` shape for `mode` with `values` on its pins, as the
/// engine builds it (node id `fx` for every test: each test uses its own
/// renderer state by first resetting or by using a fresh node id).
fn echo(id: &str, mode: &str, values: serde_json::Value) -> ShaderShape {
    let patch = json!({
        "kind": "floret.patch",
        "version": 2,
        "nodes": [
            { "id": "r", "defType": "draw/rect", "position": {"x": 0, "y": 0} },
            {
                "id": id, "defType": "gpu/echo", "position": {"x": 200, "y": 0},
                "params": { "mode": mode },
                "portValues": values,
            },
            {
                "id": "out", "defType": "render/renderer", "position": {"x": 400, "y": 0},
                "params": { "width": W, "height": H },
            },
        ],
        "edges": [
            { "id": "e1", "source": "r", "target": id, "sourceHandle": "out-shape", "targetHandle": "in-input" },
            { "id": "e2", "source": id, "target": "out", "sourceHandle": "out-out", "targetHandle": "in-layers" },
        ],
    });
    let mut engine = Engine::load(&patch.to_string()).expect("patch loads");
    let renderer = engine.graph.renderer().expect("a renderer");
    let r = engine.tick(&FrameCtx::default());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    match r.layers(renderer).first() {
        Some(Value::Scene(n)) => match &**n {
            SceneNode::Shader(sh) => (**sh).clone(),
            other => panic!("not a shader: {other:?}"),
        },
        other => panic!("no layer: {other:?}"),
    }
}

fn solid(css: &str) -> Arc<ResolvedStyle> {
    let mut s = (**default_style()).clone();
    s.fill = Some(floret_engine::scene::Paint::css(css));
    Arc::new(s)
}

/// A 20 × 20 white square centred at (`x`, 0).
fn square(x: f64) -> Arc<SceneNode> {
    Arc::new(SceneNode::Rect {
        x,
        y: 0.0,
        w: 20.0,
        h: 20.0,
        mode: Arc::from("center"),
        style: solid("#ffffff"),
    })
}

fn full(css: &str) -> Arc<SceneNode> {
    Arc::new(SceneNode::Rect {
        x: 0.0,
        y: 0.0,
        w: f64::from(W),
        h: f64::from(H),
        mode: Arc::from("center"),
        style: solid(css),
    })
}

fn with_input(sh: &ShaderShape, input: Arc<SceneNode>) -> Arc<SceneNode> {
    let mut sh = sh.clone();
    sh.input = Some(input);
    Arc::new(SceneNode::Shader(Arc::new(sh)))
}

/// Draw frame `f` of `sh` over `input`.
fn frame(g: &mut Headless, f: u32, sh: &ShaderShape, input: Arc<SceneNode>) -> Vec<u8> {
    g.renderer.set_clock(RenderClock {
        frame: f64::from(f),
        time: f64::from(f) / 60.0,
        ..RenderClock::default()
    });
    g.render(&[with_input(sh, input)], W, H, BG)
}

/// Red at scene coordinates (centre origin).
fn red(px: &[u8], x: i32, y: i32) -> u8 {
    let (ix, iy) = ((x + W as i32 / 2) as usize, (y + H as i32 / 2) as usize);
    px[(iy * W as usize + ix) * 4]
}

fn rgb(px: &[u8], x: i32, y: i32) -> [u8; 3] {
    let (ix, iy) = ((x + W as i32 / 2) as usize, (y + H as i32 / 2) as usize);
    let o = (iy * W as usize + ix) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

/// Within `tol` per channel (slit-scan blends neighbouring frames).
#[track_caller]
fn near(got: [u8; 3], want: [u8; 3], tol: u8) {
    assert!(
        (0..3).all(|i| got[i].abs_diff(want[i]) <= tol),
        "{got:?}, want {want:?}"
    );
}

#[test]
fn every_mode_compiles() {
    let Some(mut g) = gpu() else { return };
    for mode in floret_engine::nodes::echo::MODES {
        let sh = echo("compile", mode, json!({}));
        for p in sh.passes.iter() {
            if let Err(e) = floret_render::shader::check(&p.source) {
                panic!("{mode} {:?}: {e:?}", p.buf);
            }
        }
        frame(&mut g, 0, &sh, square(0.0));
        assert!(
            g.renderer.shader_errors().is_empty(),
            "{mode}: {:?}",
            g.renderer.shader_errors()
        );
    }
}

#[test]
fn trails_fade_and_reset() {
    let Some(mut g) = gpu() else { return };
    let sh = echo("trails", "trails", json!({ "decay": 0.5 }));
    frame(&mut g, 0, &sh, square(-40.0));
    let px = frame(&mut g, 1, &sh, square(40.0));
    // The square where it is now, and half as bright where it was.
    assert_eq!(red(&px, 40, 0), 255);
    let ghost = red(&px, -40, 0);
    assert!((120..=130).contains(&ghost), "ghost {ghost}");
    // Trails die out completely (8-bit buffers included).
    let mut px = px;
    for f in 2..14 {
        px = frame(&mut g, f, &sh, square(40.0));
    }
    assert_eq!(red(&px, -40, 0), 0);
    // `reset` draws only the live frame.
    frame(&mut g, 14, &sh, square(-40.0));
    let reset = echo("trails", "trails", json!({ "decay": 0.5, "reset": true }));
    let px = frame(&mut g, 15, &reset, square(40.0));
    assert_eq!(red(&px, -40, 0), 0);
    assert_eq!(red(&px, 40, 0), 255);
}

#[test]
fn delay_shows_an_earlier_frame() {
    let Some(mut g) = gpu() else { return };
    let sh = echo("delay", "delay", json!({ "frames": 2 }));
    let xs = [-45.0, -15.0, 15.0, 45.0];
    // Before two frames are held it shows the oldest it has.
    let first = frame(&mut g, 0, &sh, square(xs[0]));
    assert_eq!(red(&first, -45, 0), 255);
    frame(&mut g, 1, &sh, square(xs[1]));
    frame(&mut g, 2, &sh, square(xs[2]));
    let px = frame(&mut g, 3, &sh, square(xs[3]));
    assert_eq!(red(&px, -15, 0), 255, "two frames back");
    assert_eq!(red(&px, 45, 0), 0);
    assert_eq!(red(&px, 15, 0), 0);
    // Reset: the history refills from this frame.
    let reset = echo("delay", "delay", json!({ "frames": 2, "reset": true }));
    let px = frame(&mut g, 4, &reset, square(-45.0));
    assert_eq!(red(&px, -45, 0), 255);
    assert_eq!(red(&px, 15, 0), 0);
}

#[test]
fn echo_stacks_fainter_copies() {
    let Some(mut g) = gpu() else { return };
    let sh = echo(
        "echo",
        "echo",
        json!({ "copies": 2, "spacing": 1, "decay": 0.5 }),
    );
    frame(&mut g, 0, &sh, square(-40.0));
    frame(&mut g, 1, &sh, square(0.0));
    let px = frame(&mut g, 2, &sh, square(40.0));
    assert_eq!(red(&px, 40, 0), 255);
    let one = red(&px, 0, 0);
    let two = red(&px, -40, 0);
    assert!((120..=135).contains(&one), "one back {one}");
    assert!((55..=70).contains(&two), "two back {two}");
}

#[test]
fn freeze_holds_the_last_frame() {
    let Some(mut g) = gpu() else { return };
    let live = echo("freeze", "freeze", json!({ "freeze": false }));
    let held = echo("freeze", "freeze", json!({ "freeze": true }));
    frame(&mut g, 0, &live, square(-40.0));
    let px = frame(&mut g, 1, &held, square(40.0));
    assert_eq!(red(&px, -40, 0), 255);
    assert_eq!(red(&px, 40, 0), 0);
    let px = frame(&mut g, 2, &held, square(0.0));
    assert_eq!(red(&px, -40, 0), 255);
    let px = frame(&mut g, 3, &live, square(0.0));
    assert_eq!(red(&px, 0, 0), 255);
    assert_eq!(red(&px, -40, 0), 0);
}

#[test]
fn slit_scan_spreads_time_down_the_rows() {
    let Some(mut g) = gpu() else { return };
    let sh = echo(
        "slit",
        "slit-scan",
        json!({ "frames": 3, "direction": "down" }),
    );
    frame(&mut g, 0, &sh, full("#ff0000"));
    frame(&mut g, 1, &sh, full("#00ff00"));
    let px = frame(&mut g, 2, &sh, full("#0000ff"));
    let h = H as i32 / 2;
    // Top row: now; middle: one frame back; bottom: two.
    near(rgb(&px, 0, -h), [0, 0, 255], 48);
    near(rgb(&px, 0, 0), [0, 255, 0], 48);
    near(rgb(&px, 0, h - 1), [255, 0, 0], 48);
    // Sideways: the left column is now.
    let side = echo(
        "slit2",
        "slit-scan",
        json!({ "frames": 3, "direction": "right" }),
    );
    frame(&mut g, 0, &side, full("#ff0000"));
    frame(&mut g, 1, &side, full("#00ff00"));
    let px = frame(&mut g, 2, &side, full("#0000ff"));
    let w = W as i32 / 2;
    near(rgb(&px, -w, 0), [0, 0, 255], 48);
    near(rgb(&px, w - 1, 0), [255, 0, 0], 48);
}

// ─── Frame history in user shaders (`gpu/shader`'s `history`) ───────────────

/// A `gpu/shader` (node id `id`) whose Image pass shows the input `ago`
/// frames back through `iHistoryFrame`, keeping `history` frames.
fn history_shader(id: &str, history: u32, ago: u32) -> ShaderShape {
    let source = format!(
        "void mainImage(out vec4 o, in vec2 fc) {{\n  vec4 c = iHistoryFrame({ago}, fc / iResolution.xy);\n  o = vec4(c.a > 0.0 ? c.rgb / c.a : vec3(0.0), c.a);\n}}"
    );
    let patch = json!({
        "kind": "floret.patch",
        "version": 2,
        "nodes": [
            { "id": "r", "defType": "draw/rect", "position": {"x": 0, "y": 0} },
            {
                "id": id, "defType": "gpu/shader", "position": {"x": 200, "y": 0},
                "params": { "source": source, "history": history },
            },
            {
                "id": "out", "defType": "render/renderer", "position": {"x": 400, "y": 0},
                "params": { "width": W, "height": H },
            },
        ],
        "edges": [
            { "id": "e1", "source": "r", "target": id, "sourceHandle": "out-shape", "targetHandle": "in-input" },
            { "id": "e2", "source": id, "target": "out", "sourceHandle": "out-out", "targetHandle": "in-layers" },
        ],
    });
    let mut engine = Engine::load(&patch.to_string()).expect("patch loads");
    let renderer = engine.graph.renderer().expect("a renderer");
    let r = engine.tick(&FrameCtx::default());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    match r.layers(renderer).first() {
        Some(Value::Scene(n)) => match &**n {
            SceneNode::Shader(sh) => (**sh).clone(),
            other => panic!("not a shader: {other:?}"),
        },
        other => panic!("no layer: {other:?}"),
    }
}

#[test]
fn user_shaders_read_past_frames_with_history() {
    let Some(mut g) = gpu() else { return };
    // Three frames back: the square from frame f − 3.
    let sh = history_shader("hist", 8, 3);
    for p in sh.passes.iter() {
        floret_render::shader::check(&p.source).unwrap();
    }
    let xs = [-40.0, -20.0, 0.0, 20.0, 40.0];
    let mut last = Vec::new();
    for (f, x) in xs.iter().enumerate() {
        last = frame(&mut g, f as u32, &sh, square(*x));
    }
    assert!(g.renderer.shader_errors().is_empty());
    // Frame 4 shows frame 1's square (x = −20), not the live one (40).
    assert_eq!(red(&last, -20, 0), 255);
    assert_eq!(red(&last, 40, 0), 0);
    // Before the ring fills, older-than-held reads the oldest frame.
    let sh = history_shader("young", 8, 3);
    let px = frame(&mut g, 0, &sh, square(30.0));
    assert_eq!(red(&px, 30, 0), 255);
    // history 0 (the default): nothing kept, iHistoryFrame reads clear.
    let sh = history_shader("none", 0, 0);
    assert!(sh.history.is_none());
    let px = frame(&mut g, 0, &sh, square(0.0));
    assert_eq!(rgb(&px, 0, 0), [0, 0, 0]);
}
