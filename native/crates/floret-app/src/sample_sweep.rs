//! QA sweep over every Library sample (#82), from the app's side.
//!
//! `floret-engine/tests/all_samples.rs` already loads every sample in the
//! catalogue, checks it compiles without warnings, ticks a second of frames
//! without node errors and draws something; `canvas/samples.rs` checks the
//! layouts. This adds what needs the app:
//!
//! - **Save → reload:** each sample written the way File → Save writes it
//!   reads back to the same patch, compiles to the same graph with the
//!   same warnings, and saving again gives the same file byte for byte.
//! - **Rendering:** where a GPU adapter exists (lavapipe in CI / Xvfb), each
//!   sample renders a thumbnail-sized frame on the GPU without validation
//!   errors or panics, and the frame isn't one flat colour.

use crate::{files, thumbs, SAMPLES};
use serde_json::Value as Json;

/// Fixed save time, so two saves compare equal.
const NOW: &str = "2026-10-01T00:00:00.000Z";

/// Samples whose frame may be one flat colour after
/// [`thumbs::FRAMES`] frames here: (sample, why).
const MAY_BE_BLANK: &[(&str, &str)] = &[
    (
        "body-tracking",
        "draws the landmarks the camera finds (no camera here)",
    ),
    ("webcam-mosaic", "draws the camera image (no camera here)"),
    (
        "video-pixelate",
        "draws the video's frames (no video decoder installed in this test)",
    ),
    (
        "tut-49-fft",
        "draws the microphone's spectrum (no microphone here)",
    ),
    (
        "fft-bloom",
        "draws the spectrum the audio engine analyses (no audio engine here)",
    ),
    ("tut-30-keyboard", "draws only while R or B is pressed"),
    (
        "shader-reaction-diffusion",
        "its feedback buffer blooms over seconds of rendered frames; this renders one",
    ),
];

/// Samples that download a picture in the background: their first frames
/// can be empty whether or not the network is reachable.
fn loads_from_network(flora: &str) -> bool {
    flora.contains("\"io/image-url\"")
}

/// `a` and `b` hold the same JSON, numbers compared by value (a save
/// writes `1.0` as `1`, as JavaScript does).
fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(x), Json::Number(y)) => x.as_f64() == y.as_f64(),
        (Json::Array(x), Json::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Json::Object(x), Json::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// The patch as JSON without what a save stamps on it (version, app
/// version, name and times) and without the empty lists it leaves out.
fn content(p: &floret_engine::patch::PatchFile) -> Json {
    let mut p = p.clone();
    p.meta = None;
    p.app_version = None;
    p.version = 0;
    for n in &mut p.nodes {
        if n.port_values.as_ref().is_some_and(|v| v.is_empty()) {
            n.port_values = None;
        }
        if n.ports.as_ref().is_some_and(|v| v.is_empty()) {
            n.ports = None;
        }
    }
    serde_json::to_value(&p).expect("patch serialises")
}

fn warnings(p: &floret_engine::patch::PatchFile) -> Result<(usize, Vec<String>), String> {
    let (engine, w) = floret_engine::Engine::from_patch(p)?;
    Ok((
        engine.graph.nodes.len(),
        w.into_iter().map(|w| w.message).collect(),
    ))
}

#[test]
fn every_sample_survives_save_and_reload() {
    let mut failures = Vec::new();
    for s in SAMPLES {
        let patch = match floret_engine::load_patch(s.flora) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: doesn't load: {e}", s.id));
                continue;
            }
        };
        let saved = files::to_flora(&patch, None, NOW);
        let back = match floret_engine::load_patch(&saved) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: saved file doesn't load: {e}", s.id));
                continue;
            }
        };
        if !same(&content(&patch), &content(&back)) {
            failures.push(format!("{}: reloads as a different patch", s.id));
        }
        if files::to_flora(&back, None, NOW) != saved {
            failures.push(format!(
                "{}: saving the reloaded patch changes the file",
                s.id
            ));
        }
        match (warnings(&patch), warnings(&back)) {
            (Ok(a), Ok(b)) if a == b => {}
            (Ok(a), Ok(b)) => failures.push(format!(
                "{}: compiles differently after reload: {a:?} → {b:?}",
                s.id
            )),
            (a, b) => failures.push(format!(
                "{}: doesn't compile: {:?} / {:?}",
                s.id,
                a.err(),
                b.err()
            )),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn every_sample_renders_on_the_gpu() {
    let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
        eprintln!("no GPU adapter: skipping");
        return;
    };
    let mut failures = Vec::new();
    for s in SAMPLES {
        let Some((w, h, px)) =
            thumbs::render_sample(&hl.device, &hl.queue, &mut hl.renderer, s.flora)
        else {
            failures.push(format!(
                "{}: didn't render (no renderer, a panic or a GPU error)",
                s.id
            ));
            continue;
        };
        if px.len() != (w * h * 4) as usize || w.max(h) != thumbs::SIZE {
            failures.push(format!("{}: {w}×{h} frame with {} bytes", s.id, px.len()));
            continue;
        }
        let (pixels, _) = px.as_chunks::<4>();
        let flat = pixels.iter().all(|p| p == &pixels[0]);
        let excused = MAY_BE_BLANK.iter().any(|(id, _)| *id == s.id) || loads_from_network(s.flora);
        if flat && !excused {
            failures.push(format!("{}: renders one flat colour", s.id));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn json_comparison_is_by_value() {
    let a: Json = serde_json::from_str(r#"{"x": 1.0, "l": [2, {"y": 0.5}]}"#).unwrap();
    let b: Json = serde_json::from_str(r#"{"x": 1, "l": [2.0, {"y": 0.5}]}"#).unwrap();
    let c: Json = serde_json::from_str(r#"{"x": 1, "l": [2.0, {"y": 0.25}]}"#).unwrap();
    assert!(same(&a, &b));
    assert!(!same(&a, &c));
    assert!(!same(&a, &serde_json::json!({"x": 1})));
}
