//! `gpu/shader` draws on the GPU: orientation, the input channel, feedback,
//! uniforms and compile-error fallback. Skips when no GPU adapter is
//! available.

use floret_engine::scene::{default_style, Paint, ResolvedStyle, SceneNode};
use floret_engine::shader::{
    ShaderBuffer, ShaderChannel, ShaderPass, ShaderShape, UniformDecl, UniformValue,
};
use floret_render::{Headless, RenderClock};
use std::sync::{Arc, Mutex, OnceLock};

fn gpu() -> Option<std::sync::MutexGuard<'static, Headless>> {
    static GPU: OnceLock<Option<Mutex<Headless>>> = OnceLock::new();
    GPU.get_or_init(|| Headless::new().map(Mutex::new))
        .as_ref()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

const W: u32 = 64;
const H: u32 = 48;
const BG: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

fn shader(
    id: &str,
    passes: Vec<(ShaderBuffer, &str, [ShaderChannel; 4])>,
    uniforms: Vec<(&str, &str, UniformValue)>,
    input: Option<Arc<SceneNode>>,
) -> Arc<SceneNode> {
    Arc::new(SceneNode::Shader(Arc::new(ShaderShape {
        node_id: Arc::from(id),
        passes: passes
            .into_iter()
            .map(|(buf, source, channels)| ShaderPass {
                buf,
                source: Arc::from(source),
                channels,
            })
            .collect(),
        uniform_decls: uniforms
            .iter()
            .map(|(n, t, _)| UniformDecl {
                name: Arc::from(*n),
                glsl_type: Arc::from(*t),
            })
            .collect(),
        uniforms: uniforms.iter().map(|(_, _, v)| *v).collect(),
        input,
        map: None,
        history: None,
    })))
}

const NONE: [ShaderChannel; 4] = [ShaderChannel::None; 4];

/// Pixel at image coordinates (top-left origin).
fn px(p: &[u8], x: u32, y: u32) -> [u8; 4] {
    let o = ((y * W + x) * 4) as usize;
    [p[o], p[o + 1], p[o + 2], p[o + 3]]
}

#[track_caller]
fn near(got: [u8; 4], want: [u8; 4], tol: u8) {
    assert!(
        (0..4).all(|i| got[i].abs_diff(want[i]) <= tol),
        "got {got:?}, want {want:?}"
    );
}

#[test]
fn frag_coord_is_bottom_left_like_shadertoy() {
    let Some(mut g) = gpu() else { return };
    let src = "void mainImage(out vec4 c, in vec2 f) {
  c = vec4(f / iResolution.xy, iResolution.x == 64.0 ? 1.0 : 0.0, 1.0);
}";
    let s = shader("a", vec![(ShaderBuffer::Image, src, NONE)], vec![], None);
    let p = g.render(&[s], W, H, BG);
    // Top-left of the image is fragCoord (0.5, H - 0.5).
    near(px(&p, 0, 0), [2, 252, 255, 255], 2);
    near(px(&p, W - 1, H - 1), [253, 3, 255, 255], 2);
    assert!(g.renderer.shader_errors().is_empty());
}

#[test]
fn input_channel_keeps_the_scene_upright() {
    let Some(mut g) = gpu() else { return };
    let mut st: ResolvedStyle = (**default_style()).clone();
    st.fill = Some(Paint::css("#ff0000"));
    // A red rect in the top-left quadrant (scene coords: centre origin).
    let rect = Arc::new(SceneNode::Rect {
        x: -32.0,
        y: -24.0,
        w: 32.0,
        h: 24.0,
        mode: Arc::from("corner"),
        style: Arc::new(st),
    });
    let src = "void mainImage(out vec4 c, in vec2 f) {
  vec4 s = texture(iChannel0, f / iResolution.xy);
  c = vec4(s.rgb, 1.0) + vec4(0.0, 0.0, 1.0 - s.a, 0.0);
}";
    let s = shader(
        "b",
        vec![(
            ShaderBuffer::Image,
            src,
            [
                ShaderChannel::Input,
                ShaderChannel::None,
                ShaderChannel::None,
                ShaderChannel::None,
            ],
        )],
        vec![],
        Some(rect),
    );
    let p = g.render(&[s], W, H, BG);
    near(px(&p, 4, 4), [255, 0, 0, 255], 2);
    near(px(&p, W - 4, H - 4), [0, 0, 255, 255], 2);
}

#[test]
fn feedback_reads_the_previous_frame() {
    let Some(mut g) = gpu() else { return };
    let src = "void mainImage(out vec4 c, in vec2 f) {
  c = texture(iChannel1, f / iResolution.xy) + vec4(0.25, 0.0, 0.0, 0.25);
}";
    let channels = [
        ShaderChannel::Input,
        ShaderChannel::Buffer(ShaderBuffer::Image),
        ShaderChannel::None,
        ShaderChannel::None,
    ];
    let s = shader(
        "fb",
        vec![(ShaderBuffer::Image, src, channels)],
        vec![],
        None,
    );
    let mut reds = Vec::new();
    for _ in 0..3 {
        let p = g.render(std::slice::from_ref(&s), W, H, BG);
        reds.push(px(&p, 10, 10)[0]);
    }
    // Straight alpha out, premultiplied over black: r·a = 0.25², 0.5², 0.75².
    assert!(
        reds[0].abs_diff(16) <= 2 && reds[1].abs_diff(64) <= 2 && reds[2].abs_diff(143) <= 2,
        "{reds:?}"
    );
}

#[test]
fn buffers_chain_and_uniforms_arrive() {
    let Some(mut g) = gpu() else { return };
    let buf_a = "uniform float k;
void mainImage(out vec4 c, in vec2 f) { c = vec4(k, iFrame == 7 ? 1.0 : 0.0, 0.0, 1.0); }";
    let image = "uniform vec4 tint;
void mainImage(out vec4 c, in vec2 f) { c = texture(iChannel0, f / iResolution.xy) + tint; }";
    let s = shader(
        "chain",
        vec![
            (ShaderBuffer::A, buf_a, NONE),
            (
                ShaderBuffer::Image,
                image,
                [
                    ShaderChannel::Buffer(ShaderBuffer::A),
                    ShaderChannel::None,
                    ShaderChannel::None,
                    ShaderChannel::None,
                ],
            ),
        ],
        vec![
            ("k", "float", UniformValue::Number(0.5)),
            ("tint", "vec4", UniformValue::Vec2([0.0, 0.0])),
        ],
        None,
    );
    g.renderer.set_clock(RenderClock {
        frame: 7.0,
        ..Default::default()
    });
    let p = g.render(&[s], W, H, BG);
    g.renderer.set_clock(RenderClock::default());
    // tint = (0, 0, 0, 1) via the vec4 setter's w default.
    near(px(&p, 20, 20), [128, 255, 0, 255], 2);
}

#[test]
fn compile_errors_fall_back_and_are_reported() {
    let Some(mut g) = gpu() else { return };
    let bad = "void mainImage(out vec4 c, in vec2 f) {\n  c = vec4(oops);\n}";
    let s = shader("bad", vec![(ShaderBuffer::Image, bad, NONE)], vec![], None);
    let p = g.render(&[s], W, H, BG);
    // Magenta: the fallback with nothing on iChannel0.
    near(px(&p, 5, 5), [217, 26, 115, 255], 2);
    let errs = g.renderer.shader_errors();
    assert_eq!(errs.len(), 1);
    assert_eq!(&*errs[0].0, "bad");
    assert_eq!(errs[0].2[0].line, 2);
}
