//! Every built-in sample, traced or not: it's in the catalogue, compiles
//! without warnings, runs a second of frames without node errors and draws
//! something.
//!
//! The engine runs with a stand-in network (every URL serves a small
//! picture) and audio engine (silent), a moving mouse and a held key. The
//! devices a test machine lacks are the only allowed errors.

use floret_engine::audio::{AudioBackend, AudioFeedback, AudioGraph};
use floret_engine::host::Host;
use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn samples_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples"))
}

/// Errors that only mean the test machine lacks a device: (error text,
/// why it's expected).
const DEVICE_ERRORS: &[(&str, &str)] = &[
    (
        "camera capture isn't available",
        "no camera here (the app's floret-input provides one; tests/devices.rs fakes it)",
    ),
    (
        "video playback isn't available",
        "video needs the floret-video decoder (floret-video tests its sample end to end)",
    ),
    (
        "microphone",
        "no microphone here (the app's floret-media opens one)",
    ),
];

/// Samples that draw only once a device feeds them: (sample, why).
const DRAWS_WITH_DEVICE: &[(&str, &str)] = &[
    ("body-tracking", "draws the landmarks the camera finds"),
    ("video-pixelate", "draws the video's frames"),
    ("tut-49-fft", "draws the microphone's spectrum"),
    ("fft-bloom", "draws the spectrum the audio engine analyses"),
];

/// Every URL loads a 64×64 gradient.
struct FakeNet;

impl Host for FakeNet {
    fn fetch(&self, _: &str) -> Result<Vec<u8>, String> {
        let img = image::RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255])
        });
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(png)
    }
}

/// An audio engine that plays nothing and hears silence.
struct Silence;

impl AudioBackend for Silence {
    fn feedback(&self) -> Arc<AudioFeedback> {
        Arc::new(AudioFeedback {
            sample_rate: 48_000.0,
            ..AudioFeedback::default()
        })
    }

    fn submit(&self, _: AudioGraph) {}
}

/// Drawn leaves (shapes, characters, particle batches, pictures).
fn leaves(n: &SceneNode) -> usize {
    match n {
        SceneNode::Group { children, .. } => children.iter().map(|c| leaves(c)).sum(),
        SceneNode::Glyphs(run) => run.glyphs.len(),
        SceneNode::Text { text, .. } => text.chars().count(),
        _ => 1,
    }
}

fn drawn(v: &Value) -> usize {
    match v {
        Value::Scene(n) => leaves(n),
        Value::Image(_) => 1,
        Value::List(items) => items.iter().map(drawn).sum(),
        _ => 0,
    }
}

fn frame(f: usize) -> FrameCtx {
    let t = f as f64 / 60.0;
    FrameCtx {
        t,
        dt: 1.0 / 60.0,
        frame_count: f as f64,
        width: 640.0,
        height: 480.0,
        mouse_x: 120.0 * (t * 3.0).sin(),
        mouse_y: 80.0 * (t * 2.0).cos(),
        mouse_is_pressed: f % 20 < 10,
        key: "r".into(),
        key_is_pressed: f >= 30,
        ..FrameCtx::default()
    }
}

#[test]
fn every_sample_compiles_runs_and_draws() {
    let cat: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(samples_dir().join("catalogue.json")).unwrap(),
    )
    .unwrap();
    let listed: Vec<&str> = cat["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["file"].as_str().unwrap())
        .collect();
    for entry in std::fs::read_dir(samples_dir()).unwrap() {
        let file = entry.unwrap().file_name().into_string().unwrap();
        if file.ends_with(".flora") {
            assert!(
                listed.contains(&file.as_str()),
                "{file} isn't in catalogue.json"
            );
        }
    }

    let mut failures = Vec::new();
    for file in &listed {
        let id = file.strip_suffix(".flora").unwrap();
        let text = std::fs::read_to_string(samples_dir().join(file)).unwrap();
        let (mut engine, warnings) = match Engine::load_with_warnings(&text) {
            Ok(x) => x,
            Err(e) => {
                failures.push(format!("{id}: doesn't load: {e}"));
                continue;
            }
        };
        for w in &warnings {
            failures.push(format!("{id}: compile warning: {}", w.message));
        }
        engine.set_host(Some(Arc::new(FakeNet)));
        engine.set_audio(Some(Arc::new(Silence)));
        let Some(renderer) = engine.graph.renderer() else {
            failures.push(format!("{id}: no renderer"));
            continue;
        };
        let (mut most, mut errors) = (0, Vec::new());
        // A second of frames; then, while nothing is drawn, a little more
        // (pictures load on a background thread).
        let until = Instant::now() + Duration::from_secs(3);
        let mut f = 0;
        while f < 60 || (most == 0 && Instant::now() < until) {
            let r = engine.tick(&frame(f));
            for (node, msg) in &r.errors {
                let expected = DEVICE_ERRORS.iter().any(|(e, _)| msg.contains(e));
                let line = format!("{} — {msg}", engine.graph.nodes[*node].id);
                if !expected && !errors.contains(&line) {
                    errors.push(line);
                }
            }
            most = most.max(r.layers(renderer).iter().map(drawn).sum::<usize>());
            if f >= 60 {
                std::thread::sleep(Duration::from_millis(10));
            }
            f += 1;
        }
        for e in errors {
            failures.push(format!("{id}: tick error: {e}"));
        }
        if most == 0 && !DRAWS_WITH_DEVICE.iter().any(|(s, _)| *s == id) {
            failures.push(format!("{id}: draws nothing"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Samples rebuilt from visual nodes, without the Expression node (the
/// owner's rule: Library patches are built from visual nodes, not code).
/// Each rebuild lists its own here; the final merge switches this check
/// to every sample in the catalogue.
const NO_EXPRESSION: &[&str] = &[
    // No-code rebuild (agent B3).
    "sound-keyboard-synth",
    "sound-hand-theremin",
    "sound-mic-bloom",
    "rose-curve",
    "morph-tunnel",
    "number-counters",
    "echo-trails",
    "layout-bloom",
    "neon-orbits",
    "point-tracker",
    "shader-time-smear",
    "camera-slit-scan",
    "kinetic-type",
    "plexus-colours",
    "topographic",
    "trim-write-on",
];

#[test]
fn rebuilt_samples_have_no_expression_nodes() {
    let mut failures = Vec::new();
    for id in NO_EXPRESSION {
        let text = std::fs::read_to_string(samples_dir().join(format!("{id}.flora")))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let patch: serde_json::Value = serde_json::from_str(&text).unwrap();
        for n in patch["nodes"].as_array().unwrap() {
            if n["defType"] == "math/expression" {
                failures.push(format!("{id}: Expression node {}", n["id"]));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
