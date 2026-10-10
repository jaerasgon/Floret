//! Rebuilt samples look, move and respond as before.
//!
//! When a Library sample is rebuilt from other nodes (the no-code rebuild
//! replaced every Expression node with visual nodes), its node ids change,
//! so `sample_sweep::rewritten_samples_draw_as_before` can't pair its
//! nodes up. This compares what comes out instead: both versions run the
//! same scripted performance (mouse, keys, a fake camera and hand) for
//! [`FRAMES`] frames, every frame is rendered at full size, each version
//! on its own renderer so Echo / Shader history stays its own. The
//! rebuild must behave as before, not match pixel for pixel: a frame or
//! two may look different (an event answered a frame sooner), a lasting
//! difference fails; the report lists the frames. A sample with a loop
//! length must also join seamlessly: the renderer's layers on the frame
//! one loop on are those of frame 0.
//!
//! `FLORET_SAMPLES_BEFORE=<dir> cargo test -p floret-app -- --ignored
//! rebuilt_samples_match_before --nocapture` (`<dir>`: copies of the
//! samples before the rebuild); `FLORET_SAMPLES_ONLY=a,b` limits it,
//! `FLORET_RENDER_OUT=<dir>` keeps the most different frame of each pair.

use crate::thumbs;
use floret_engine::devices::{
    self, CameraFrame, CameraRequest, Devices, LandmarkLayer, LandmarksRequest,
};
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx, Value};
use std::sync::{Arc, Mutex};

/// Frames each run lasts (at 60 fps).
const FRAMES: u32 = 180;

/// What the player does on one frame.
#[derive(Clone, Debug, Default)]
struct Input {
    mouse: (f64, f64),
    pressed: bool,
    keys: Vec<&'static str>,
    /// The fake hand's index fingertip, normalised camera coordinates.
    hand: Option<[f32; 2]>,
}

type Script = fn(u32) -> Input;

/// A mouse circling the canvas, pressed for a while, a key held.
fn wander(f: u32) -> Input {
    let a = f64::from(f) * 0.05;
    Input {
        mouse: (180.0 * a.cos(), 120.0 * a.sin()),
        pressed: (60..120).contains(&f),
        keys: if (90..100).contains(&f) {
            vec!["r"]
        } else {
            vec![]
        },
        hand: None,
    }
}

/// A melody, an octave up and down, a held chord, the filter swept.
fn keyboard(f: u32) -> Input {
    let held: &[&'static str] = match f {
        10..=24 => &["a"],
        25..=39 => &["d"],
        40..=44 => &["x"],
        45..=54 => &["g"],
        55..=59 => &["z"],
        60..=64 => &["z"],
        70..=140 => &["a", "d", "g"],
        150..=160 => &["k"],
        _ => &[],
    };
    Input {
        mouse: (0.0, -240.0 + f64::from(f) * 2.6),
        keys: held.to_vec(),
        ..Input::default()
    }
}

/// The mouse held and swept across, then a hand in front of the camera.
fn theremin(f: u32) -> Input {
    let x = -330.0 + f64::from(f) * 4.0;
    let y = -150.0 + 120.0 * (f64::from(f) * 0.07).sin();
    Input {
        mouse: (x, y),
        pressed: (15..100).contains(&f),
        hand: (110..170).contains(&f).then(|| {
            let s = f64::from(f - 110) / 60.0;
            [(0.85 - 0.7 * s) as f32, (0.2 + 0.5 * s) as f32]
        }),
        ..Input::default()
    }
}

/// (sample, the performance, text edits to both files first).
type Case = (&'static str, Script, &'static [(&'static str, &'static str)]);

/// What each rebuilt sample is run with. Toggles are switched by editing
/// both files the same way (`id`, `"value": false` → true).
const CASES: &[Case] = &[
    ("sound-keyboard-synth", keyboard, &[]),
    ("sound-keyboard-synth", keyboard, &[("ks_arpOn", "true")]),
    ("sound-hand-theremin", theremin, &[]),
    ("sound-hand-theremin", theremin, &[("th_camOn", "true")]),
    ("sound-hand-theremin", theremin, &[("th_snap", "false")]),
    ("sound-mic-bloom", wander, &[]),
    ("morph-tunnel", wander, &[]),
    ("number-counters", wander, &[]),
    ("echo-trails", wander, &[]),
    ("layout-bloom", wander, &[]),
    ("neon-orbits", wander, &[]),
    ("point-tracker", wander, &[]),
    ("shader-time-smear", wander, &[]),
    ("camera-slit-scan", wander, &[]),
    ("camera-slit-scan", wander, &[("cs_on", "false")]),
    ("kinetic-type", wander, &[]),
    ("plexus-colours", wander, &[]),
    ("topographic", wander, &[]),
    ("trim-write-on", wander, &[]),
];

/// A camera that films a slowly moving gradient, and a hand tracker that
/// sees the scripted hand.
#[derive(Default)]
struct Fake(Mutex<(u32, Option<[f32; 2]>)>);

impl Devices for Fake {
    fn camera(&self, _: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        if !req.start {
            return Ok(CameraFrame {
                image: Image::placeholder(),
                ready: false,
            });
        }
        let f = self.0.lock().unwrap().0;
        let (w, h) = (64u32, 48u32);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend([
                    ((x * 4 + f * 3) % 256) as u8,
                    ((y * 5) % 256) as u8,
                    ((x + y + f) % 256) as u8,
                    255,
                ]);
            }
        }
        Ok(CameraFrame {
            image: Arc::new(Image::new(w, h, px)),
            ready: true,
        })
    }

    fn landmarks(
        &self,
        _: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        let tip = self.0.lock().unwrap().1;
        Ok(match (req.start, tip) {
            (true, Some([x, y])) => Some(vec![LandmarkLayer {
                tracker: Tracker::Hands,
                dets: vec![(0..21)
                    .map(|i| if i == 8 { [x, y, 0.0] } else { [0.5, 0.8, 0.0] })
                    .collect()],
                ids: vec![0],
            }]),
            _ => Some(Vec::new()),
        })
    }
}

/// `text` with the toggles in `edits` switched.
fn edited(text: &str, edits: &[(&str, &str)]) -> String {
    let mut t = text.to_owned();
    for (id, value) in edits {
        let at = t
            .find(&format!("\"id\": \"{id}\""))
            .unwrap_or_else(|| panic!("{id} in the file"));
        let from = if *value == "true" { "false" } else { "true" };
        let rel = t[at..]
            .find(&format!("\"value\": {from}"))
            .unwrap_or_else(|| panic!("{id}'s value"));
        let on = at + rel;
        t = format!(
            "{}\"value\": {value}{}",
            &t[..on],
            &t[on + 9 + from.len()..]
        );
    }
    t
}

/// One run: per frame the renderer's layers (as comparable text) and the
/// picture, and every node error.
struct Run {
    layers: Vec<String>,
    pictures: Vec<Vec<u8>>,
    size: (u32, u32),
    errors: Vec<String>,
}

/// A value as text, scene nodes in full (their `Debug` is just a kind),
/// groups' children included.
fn canon(v: &Value) -> String {
    match v {
        Value::Scene(n) => canon(&n.to_value()),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(canon).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map.iter().map(|(k, v)| format!("{k}: {}", canon(v))).collect();
            format!("{{{}}}", parts.join(", "))
        }
        other => format!("{other:?}"),
    }
}

fn layers_text(layers: &[Arc<SceneNode>], bg: [f64; 4]) -> String {
    let mut s = format!("{bg:?}");
    for l in layers {
        s.push_str(&canon(&l.to_value()));
    }
    // Image ids come from a global counter.
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find("Image#") {
        out.push_str(&rest[..i + 6]);
        rest = rest[i + 6..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

fn run(fake: &Fake, flora: &str, script: Script, frames: u32, loop_len: f64) -> Run {
    let (mut engine, warnings) = Engine::load_with_warnings(flora).expect("sample loads");
    let mut errors: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    engine.set_host(Some(crate::host::shared()));
    let r = engine.graph.renderer().expect("a renderer");
    let id = engine.graph.nodes[r].id.clone();
    let size = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let mut hl = floret_render::Headless::with_sample_count(1).expect("a GPU adapter");
    let dt = 1.0 / 60.0;
    let (mut layers, mut pictures) = (Vec::new(), Vec::new());
    for f in 0..frames {
        let input = script(f);
        *fake.0.lock().unwrap() = (f, input.hand);
        let ctx = FrameCtx {
            t: f64::from(f) * dt,
            dt,
            frame_count: f64::from(f),
            width: f64::from(size.0),
            height: f64::from(size.1),
            mouse_x: input.mouse.0,
            mouse_y: input.mouse.1,
            mouse_is_pressed: input.pressed,
            key: input
                .keys
                .last()
                .map(|k| (*k).to_owned())
                .unwrap_or_default(),
            key_is_pressed: !input.keys.is_empty(),
            keys_down: input.keys.iter().map(|k| (*k).to_owned()).collect(),
            loop_length: loop_len,
            ..FrameCtx::default()
        };
        let res = engine.tick(&ctx);
        for (i, msg) in &res.errors {
            if !msg.contains("no audio output") && !msg.contains("microphone") {
                errors.push(format!("frame {f}: {}: {msg}", engine.graph.nodes[*i].id));
            }
        }
        let out = res.outputs.get(r).cloned().flatten().unwrap_or_default();
        let ls = out
            .first()
            .map(|s| floret_render::flatten_layers(s))
            .unwrap_or_default();
        let bg = out
            .get(1)
            .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
        layers.push(layers_text(&ls, bg));
        hl.renderer.set_clock(floret_render::RenderClock {
            time: ctx.t,
            time_delta: dt,
            frame: ctx.frame_count,
            ..Default::default()
        });
        let (px, _) =
            floret_render::render_and_read(&hl.device, &hl.queue, &mut hl.renderer, &ls, size, bg);
        pictures.push(px);
    }
    errors.dedup();
    Run {
        layers,
        pictures,
        size,
        errors,
    }
}

/// Numbers in `a` and `b` within `tol` (relative past 1), the rest equal.
fn near_text(a: &str, b: &str, tol: f64) -> bool {
    fn tokens(s: &str) -> Vec<Result<f64, &str>> {
        let mut out = Vec::new();
        let bytes = s.as_bytes();
        let (mut i, mut start) = (0, 0);
        while i < bytes.len() {
            let c = bytes[i] as char;
            let starts_num = c.is_ascii_digit()
                || (c == '-'
                    && bytes.get(i + 1).is_some_and(|d| d.is_ascii_digit())
                    && (i == 0 || !(bytes[i - 1] as char).is_ascii_alphanumeric()));
            if starts_num && (i == 0 || !(bytes[i - 1] as char).is_ascii_alphabetic()) {
                if start < i {
                    out.push(Err(&s[start..i]));
                }
                let mut j = i + 1;
                while j < bytes.len() {
                    let d = bytes[j] as char;
                    let exp_sign =
                        (d == '-' || d == '+') && matches!(bytes[j - 1] as char, 'e' | 'E');
                    if d.is_ascii_digit() || d == '.' || d == 'e' || d == 'E' || exp_sign {
                        j += 1;
                    } else {
                        break;
                    }
                }
                match s[i..j].parse::<f64>() {
                    Ok(v) => out.push(Ok(v)),
                    Err(_) => out.push(Err(&s[i..j])),
                }
                i = j;
                start = j;
            } else {
                i += 1;
            }
        }
        if start < s.len() {
            out.push(Err(&s[start..]));
        }
        out
    }
    let (ta, tb) = (tokens(a), tokens(b));
    ta.len() == tb.len()
        && ta.iter().zip(&tb).all(|(x, y)| match (x, y) {
            (Ok(x), Ok(y)) => (x - y).abs() <= tol * x.abs().max(1.0) || (x.is_nan() && y.is_nan()),
            (Err(x), Err(y)) => x == y,
            _ => false,
        })
}

/// (pixels with a channel off by more than 2, by more than 24, the
/// largest difference).
fn pixel_diff(a: &[u8], b: &[u8]) -> (usize, usize, u8) {
    let (mut small, mut big, mut max) = (0, 0, 0u8);
    for (p, q) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let d = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        max = max.max(d);
        if d > 2 {
            small += 1;
        }
        if d > 24 {
            big += 1;
        }
    }
    (small, big, max)
}

fn loop_length(flora: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(flora).ok()?;
    v.get("loopLength")?.as_f64()
}

#[test]
#[ignore = "needs FLORET_SAMPLES_BEFORE and a GPU adapter"]
fn rebuilt_samples_match_before() {
    let Some(before_dir) = std::env::var_os("FLORET_SAMPLES_BEFORE") else {
        eprintln!("FLORET_SAMPLES_BEFORE not set: skipping");
        return;
    };
    if floret_render::Headless::with_sample_count(1).is_none() {
        eprintln!("no GPU adapter: skipping");
        return;
    }
    let fake = Arc::new(Fake::default());
    devices::install(Some(fake.clone()));
    let only = std::env::var("FLORET_SAMPLES_ONLY").ok();
    let out_dir = std::env::var_os("FLORET_RENDER_OUT").map(std::path::PathBuf::from);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut failures = Vec::new();
    for (k, (id, script, edits)) in CASES.iter().enumerate() {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|s| id.starts_with(s)))
        {
            continue;
        }
        let read = |d: &std::path::Path| {
            std::fs::read_to_string(d.join(format!("{id}.flora")))
                .map(|t| edited(&t, edits))
                .unwrap_or_else(|e| panic!("{id}: {e}"))
        };
        let (before, after) = (read(std::path::Path::new(&before_dir)), read(&dir));
        let name = if edits.is_empty() {
            (*id).to_owned()
        } else {
            format!("{id} ({})", edits.iter().map(|e| e.0).collect::<Vec<_>>().join(", "))
        };
        let a = run(&fake, &before, *script, FRAMES, 0.0);
        let b = run(&fake, &after, *script, FRAMES, 0.0);
        for e in b.errors.iter().take(5) {
            failures.push(format!("{name}: {e}"));
        }
        let total = (b.size.0 * b.size.1) as usize;
        let (mut worst, mut worst_f, mut layer_frames) = ((0, 0, 0u8), 0, 0);
        let mut visible = Vec::new();
        for (f, (pa, pb)) in a.pictures.iter().zip(&b.pictures).enumerate() {
            let d = pixel_diff(pa, pb);
            if (d.1, d.0, d.2) > (worst.1, worst.0, worst.2) {
                (worst, worst_f) = (d, f);
            }
            if d.1 * 1000 > total {
                visible.push(f);
            }
            if near_text(&a.layers[f], &b.layers[f], 1e-6) {
                layer_frames += 1;
            }
        }
        eprintln!(
            "{name}: layers equal (±1e-6) on {layer_frames}/{FRAMES} frames; visibly different \
             frames {visible:?}; worst frame {worst_f}: {} px off by > 2, {} by > 24 (of {total}), max {}",
            worst.0, worst.1, worst.2
        );
        if let Some(dir) = &out_dir {
            let _ = std::fs::create_dir_all(dir);
            let (w, h) = b.size;
            for (tag, run) in [("before", &a), ("after", &b)] {
                let px = run.pictures[worst_f].clone();
                let _ = thumbs::write_png(&dir.join(format!("{id}-{k}-{tag}.png")), &(w, h, px));
            }
        }
        // The rebuild must behave the same, not match pixel for pixel: a
        // frame or two may differ (Keyboard Synth's octave Counter answers
        // Z / X a frame sooner than the old State loop), a lasting
        // difference may not.
        if visible.len() * 20 > FRAMES as usize {
            failures.push(format!(
                "{name}: {} of {FRAMES} frames look different, from frame {}",
                visible.len(),
                visible[0]
            ));
        }
        // Loops join: one loop on, the same layers as frame 0.
        if let Some(len) = loop_length(&after).filter(|_| edits.is_empty()) {
            let n = (len * 60.0).round() as u32;
            let still: Script = |_| Input::default();
            let r = run(&fake, &after, still, n + 1, len);
            let joins = near_text(&r.layers[0], &r.layers[n as usize], 1e-9);
            eprintln!("{name}: loop of {len} s joins: {joins}");
            if !joins {
                failures.push(format!("{name}: frame {n} isn't frame 0 (loop {len} s)"));
            }
        }
    }
    devices::install(None);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn near_text_compares_numbers_within_tolerance() {
    assert!(near_text("Circle { x: 1.0, y: -2.5 }", "Circle { x: 1.0000000001, y: -2.5 }", 1e-6));
    assert!(!near_text("Circle { x: 1.0 }", "Circle { x: 1.1 }", 1e-6));
    assert!(!near_text("Circle { x: 1.0 }", "Rect { x: 1.0 }", 1e-6));
    assert!(near_text("p2 [1e-20, 3]", "p2 [-1e-20, 3]", 1e-6));
    assert_eq!(pixel_diff(&[0, 0, 0, 255], &[3, 0, 0, 255]), (1, 0, 3));
}
