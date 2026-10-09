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
        "flipbook-sequence",
        "draws the image sequence's frames (no video decoder installed in this test)",
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

// ─── Rewritten samples draw as before ───────────────────────────────────────

/// Frames ticked when comparing two versions of a sample, and the frames
/// rendered and compared pixel for pixel.
const COMPARE_FRAMES: u32 = 120;
const COMPARE_RENDERS: &[u32] = &[1, 30, 119];

/// A value as comparable text: layers, styles and handles in full (their
/// `Debug` is a summary), image ids (a global counter) left out.
fn canon_value(v: &floret_engine::Value) -> String {
    use floret_engine::Value;
    let s = match v {
        Value::Scene(n) => format!("{:?}", n.to_value()),
        Value::Style(st) => format!("{:?}", st.apply(floret_engine::scene::neutral_style())),
        Value::Handle(h) => format!("{:?}", h.to_value()),
        Value::List(items) => items.iter().map(canon_value).collect::<Vec<_>>().join(","),
        other => format!("{other:?}"),
    };
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find("Image#") {
        out.push_str(&rest[..i + 6]);
        rest = rest[i + 6..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// Each frame's outputs by node id, and the pictures rendered.
type Run = (Vec<std::collections::HashMap<String, String>>, Vec<Vec<u8>>);

/// Every node's outputs per frame (by node id) and the renderer's picture
/// at [`COMPARE_RENDERS`], full size.
fn run_for_compare(hl: &mut floret_render::Headless, flora: &str) -> Result<Run, String> {
    use floret_engine::{Engine, FrameCtx};
    let _wait = floret_engine::video::wait_for_frames();
    let (mut engine, _) = Engine::load_with_warnings(flora)?;
    engine.set_host(Some(crate::host::shared()));
    let r = engine.graph.renderer().ok_or("no renderer")?;
    let id = engine.graph.nodes[r].id.clone();
    let size = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let dt = 1.0 / 60.0;
    let (mut outputs, mut pictures) = (Vec::new(), Vec::new());
    for f in 0..COMPARE_FRAMES {
        let ctx = FrameCtx {
            t: f64::from(f) * dt,
            dt,
            frame_count: f64::from(f),
            width: f64::from(size.0),
            height: f64::from(size.1),
            ..Default::default()
        };
        let res = engine.tick(&ctx);
        let mut by_id = std::collections::HashMap::new();
        for (i, out) in res.outputs.iter().enumerate() {
            let text = match out {
                Some(spreads) => spreads
                    .iter()
                    .map(|s| s.iter().map(canon_value).collect::<Vec<_>>().join(";"))
                    .collect::<Vec<_>>()
                    .join(" | "),
                None => "-".into(),
            };
            by_id.insert(engine.graph.nodes[i].id.clone(), text);
        }
        outputs.push(by_id);
        if COMPARE_RENDERS.contains(&f) {
            let Some(out) = res.outputs.get(r).cloned().flatten() else {
                pictures.push(Vec::new());
                continue;
            };
            let layers = out
                .first()
                .map(|s| floret_render::flatten_layers(s))
                .unwrap_or_default();
            let bg = out
                .get(1)
                .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
            hl.renderer.set_clock(floret_render::RenderClock {
                time: ctx.t,
                time_delta: dt,
                frame: ctx.frame_count,
                ..Default::default()
            });
            let (px, _) = floret_render::render_and_read(
                &hl.device,
                &hl.queue,
                &mut hl.renderer,
                &layers,
                size,
                bg,
            );
            pictures.push(px);
        }
    }
    Ok((outputs, pictures))
}

/// `FLORET_SAMPLES_BEFORE=<dir> cargo test -p floret-app -- --ignored
/// rewritten_samples_draw_as_before`: every Library sample whose file
/// differs from its copy in `<dir>` (an earlier checkout's `samples/`)
/// must compute the same outputs on every node both versions share, over
/// [`COMPARE_FRAMES`] frames, and render the same pixels at full size.
/// For rewrites that mustn't change the picture (moving samples onto
/// newer nodes). `FLORET_SAMPLES_ONLY=a,b` limits it to files starting so.
#[test]
#[ignore = "needs FLORET_SAMPLES_BEFORE and a GPU adapter"]
fn rewritten_samples_draw_as_before() {
    let Some(before_dir) = std::env::var_os("FLORET_SAMPLES_BEFORE") else {
        eprintln!("FLORET_SAMPLES_BEFORE not set: skipping");
        return;
    };
    let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
        eprintln!("no GPU adapter: skipping");
        return;
    };
    let only = std::env::var("FLORET_SAMPLES_ONLY").ok();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "flora"))
        .collect();
    paths.sort();
    let (mut failures, mut compared) = (Vec::new(), 0);
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|s| name.starts_with(s)))
        {
            continue;
        }
        let after = std::fs::read_to_string(&path).unwrap();
        let Ok(before) = std::fs::read_to_string(std::path::Path::new(&before_dir).join(&name))
        else {
            continue;
        };
        if before == after {
            continue;
        }
        // The old version twice first: whatever differs between two runs of
        // one file (wall clock, devices, downloads) can't be compared.
        let (a, b, c) = match (
            run_for_compare(&mut hl, &before),
            run_for_compare(&mut hl, &before),
            run_for_compare(&mut hl, &after),
        ) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            (a, _, c) => {
                failures.push(format!("{name}: {:?} / {:?}", a.err(), c.err()));
                continue;
            }
        };
        compared += 1;
        let (mut node_frames, mut differing) = (0, Vec::new());
        for (f, ((oa, ob), oc)) in a.0.iter().zip(&b.0).zip(&c.0).enumerate() {
            for (id, va) in oa {
                let Some(vc) = oc.get(id).filter(|_| ob.get(id) == Some(va)) else {
                    continue;
                };
                node_frames += 1;
                if va != vc && differing.len() < 5 {
                    let cut = |s: &str| s.chars().take(200).collect::<String>();
                    differing.push(format!("frame {f} node {id}: {} → {}", cut(va), cut(vc)));
                }
            }
        }
        let deterministic = a.1 == b.1;
        let same_pixels = a.1 == c.1;
        eprintln!(
            "{name}: {node_frames} node-frames compared, {} differ; {} renders: pixels {}{}",
            differing.len(),
            a.1.len(),
            if same_pixels { "identical" } else { "DIFFER" },
            if deterministic {
                ""
            } else {
                " (not deterministic run to run)"
            }
        );
        if !differing.is_empty() {
            failures.push(format!("{name}:\n  {}", differing.join("\n  ")));
        }
        if deterministic && !same_pixels {
            failures.push(format!("{name}: pixels differ"));
        }
    }
    eprintln!("{compared} rewritten samples compared");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
