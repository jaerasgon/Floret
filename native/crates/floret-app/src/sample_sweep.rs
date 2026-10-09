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

// ─── Review renders (docs/sample-review.md) ─────────────────────────────────

/// The frames [`review_samples`] saves (1-based: frame 1 is the first tick).
const REVIEW_FRAMES: &[u32] = &[1, 60, 240];

fn median_ms(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn save_png(path: &std::path::Path, w: u32, h: u32, px: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(px))
        .expect("write png");
}

/// What a frame shows, for spotting blank, clipped or off-centre pictures:
/// the box of pixels that differ from the background (x0, y0, x1, y1) and
/// the fraction of the canvas they cover.
fn ink(px: &[u8], w: u32, bg: [f64; 4]) -> (Option<[u32; 4]>, f64) {
    let bg = bg.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as i32);
    let (mut bb, mut n) = (None::<[u32; 4]>, 0usize);
    for (i, p) in px.chunks_exact(4).enumerate() {
        let d = (0..3)
            .map(|c| (i32::from(p[c]) - bg[c]).abs())
            .max()
            .unwrap_or(0);
        if d > 12 {
            n += 1;
            let (x, y) = (i as u32 % w, i as u32 / w);
            let b = bb.get_or_insert([x, y, x, y]);
            *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
    }
    (bb, n as f64 / (px.len() / 4).max(1) as f64)
}

/// Mean absolute channel difference between two frames, 0..255.
fn frame_diff(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::NAN;
    }
    let s: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    s as f64 / a.len() as f64
}

/// The app's host, but every http(s) URL loads one local picture
/// (`FLORET_REVIEW_IMAGE`): reviews photo samples without the network.
struct StandIn(std::sync::Arc<dyn floret_engine::host::Host>, Vec<u8>);

impl floret_engine::host::Host for StandIn {
    fn rasterize(
        &self,
        layers: &[std::sync::Arc<floret_engine::scene::SceneNode>],
        width: u32,
        height: u32,
    ) -> Option<floret_engine::image::Image> {
        self.0.rasterize(layers, width, height)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        if url.starts_with("http://") || url.starts_with("https://") {
            Ok(self.1.clone())
        } else {
            self.0.fetch(url)
        }
    }

    fn base_dir(&self) -> Option<std::path::PathBuf> {
        self.0.base_dir()
    }
}

/// `FLORET_REVIEW_OUT=<dir> cargo test -p floret-app -- --ignored
/// review_samples --nocapture`: runs every Library sample to its last saved
/// frame
/// at 60 fps (a mouse circling the canvas, pressed from frame 90 to 150;
/// R held from frame 100), renders every frame at full size as the app's
/// preview does, saves frames [`REVIEW_FRAMES`] (or `FLORET_REVIEW_FRAMES=1,60,…`;
/// the TSV compares the first three) as `<id>-fNNN.png` and
/// writes `review.tsv`: median tick / render ms, the inked box and
/// coverage, and how much the picture changes between the saved frames.
/// `FLORET_SAMPLES_ONLY=a,b` limits it to ids starting so.
#[test]
#[ignore = "writes review frames; needs FLORET_REVIEW_OUT and a GPU adapter"]
fn review_samples() {
    use floret_engine::{Engine, FrameCtx};
    use std::fmt::Write as _;
    use std::time::Instant;
    let Some(out_dir) = std::env::var_os("FLORET_REVIEW_OUT") else {
        eprintln!("FLORET_REVIEW_OUT not set: skipping");
        return;
    };
    let out_dir = std::path::PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir).expect("output dir");
    let only = std::env::var("FLORET_SAMPLES_ONLY").ok();
    let frames: Vec<u32> = std::env::var("FLORET_REVIEW_FRAMES")
        .ok()
        .map(|s| s.split(',').filter_map(|f| f.trim().parse().ok()).collect())
        .unwrap_or_else(|| REVIEW_FRAMES.to_vec());
    let last = frames.iter().copied().max().unwrap_or(1);
    // As the app does at startup: video files and image sequences play.
    floret_video::install();
    let host: std::sync::Arc<dyn floret_engine::host::Host> =
        match std::env::var_os("FLORET_REVIEW_IMAGE") {
            Some(p) => std::sync::Arc::new(StandIn(
                crate::host::shared(),
                std::fs::read(p).expect("FLORET_REVIEW_IMAGE"),
            )),
            None => crate::host::shared(),
        };
    let mut tsv = String::from(
        "id\tcategory\tw\th\ttick_ms\trender_ms\tframe_ms\tbox\tcoverage\tdiff_1_60\tdiff_60_240\n",
    );
    for s in SAMPLES {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|p| s.id.starts_with(p)))
        {
            continue;
        }
        // A fresh device per sample: no feedback buffers carried over.
        let Some(mut hl) = floret_render::Headless::with_sample_count(4) else {
            eprintln!("no GPU adapter: skipping");
            return;
        };
        let _wait = floret_engine::video::wait_for_frames();
        let (mut engine, _) = Engine::load_with_warnings(s.flora).expect("sample loads");
        engine.set_host(Some(host.clone()));
        let Some(r) = engine.graph.renderer() else {
            eprintln!("{}: no renderer", s.id);
            continue;
        };
        let id = engine.graph.nodes[r].id.clone();
        let size = floret_render::canvas_size(
            engine.params.get(&id, "width"),
            engine.params.get(&id, "height"),
        );
        let (cw, ch) = (f64::from(size.0), f64::from(size.1));
        let dt = 1.0 / 60.0;
        let (mut tick_ms, mut render_ms, mut frame_ms) = (Vec::new(), Vec::new(), Vec::new());
        let mut saved: Vec<(Vec<u8>, [f64; 4])> = Vec::new();
        for f in 0..last {
            let t = f64::from(f) * dt;
            let rad = 0.3 * cw.min(ch);
            let ctx = FrameCtx {
                t,
                dt,
                frame_count: f64::from(f),
                width: cw,
                height: ch,
                mouse_x: rad * (t * 1.5).cos(),
                mouse_y: rad * (t * 1.5).sin(),
                mouse_is_pressed: (90..150).contains(&f),
                key: if f >= 100 { "r".into() } else { String::new() },
                key_is_pressed: f >= 100,
                ..Default::default()
            };
            let t0 = Instant::now();
            let res = engine.tick(&ctx);
            let t1 = Instant::now();
            let out = res.outputs.get(r).cloned().flatten();
            let layers = out
                .as_ref()
                .and_then(|o| o.first())
                .map(|s| floret_render::flatten_layers(s))
                .unwrap_or_default();
            let bg = out
                .as_ref()
                .and_then(|o| o.get(1))
                .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
            hl.renderer.set_clock(floret_render::RenderClock {
                time: t,
                time_delta: dt,
                frame: f64::from(f),
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
            let t2 = Instant::now();
            if f > 0 {
                tick_ms.push((t1 - t0).as_secs_f64() * 1e3);
                render_ms.push((t2 - t1).as_secs_f64() * 1e3);
                frame_ms.push((t2 - t0).as_secs_f64() * 1e3);
            }
            if frames.contains(&(f + 1)) {
                let path = out_dir.join(format!("{}-f{:03}.png", s.id, f + 1));
                save_png(&path, size.0, size.1, &px);
                saved.push((px, bg));
            }
        }
        let (bb, cov) = saved
            .get(1)
            .map_or((None, 0.0), |(px, bg)| ink(px, size.0, *bg));
        let bb = bb.map_or("-".into(), |b| {
            format!("{},{},{},{}", b[0], b[1], b[2], b[3])
        });
        let d = |a: usize, b: usize| match (saved.get(a), saved.get(b)) {
            (Some(x), Some(y)) => frame_diff(&x.0, &y.0),
            _ => f64::NAN,
        };
        let line = format!(
            "{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{bb}\t{cov:.3}\t{:.2}\t{:.2}",
            s.id,
            s.category,
            size.0,
            size.1,
            median_ms(tick_ms),
            median_ms(render_ms),
            median_ms(frame_ms),
            d(0, 1),
            d(1, 2),
        );
        eprintln!("{line}");
        let _ = writeln!(tsv, "{line}");
    }
    std::fs::write(out_dir.join("review.tsv"), tsv).expect("write review.tsv");
}

/// Controls [`review_controls`] can't see change anything here: (node,
/// why).
const QUIET_CONTROLS: &[(&str, &str)] = &[
    ("wm_start", "starts the camera (none here)"),
    ("bt_tog", "starts the camera and tracker (none here)"),
    ("cs_on", "starts the camera; the stand-in shows without one"),
    ("fb_amp", "scales the analysed sound (no audio engine here)"),
    ("cl_tear", "matters only while the mouse pulls the cloth"),
    (
        "t39_sz",
        "changes the image's pixels, which the scene doesn't show",
    ),
    (
        "ps_lo",
        "changes the image's pixels, which the scene doesn't show",
    ),
    (
        "ps_hi",
        "changes the image's pixels, which the scene doesn't show",
    ),
];

/// `cargo test -p floret-app -- --ignored review_controls --nocapture`:
/// every Slider, Toggle and Bang in a Library sample changes what the
/// Renderer gets. Two copies of the sample run in step; at frame 30 one
/// has the control moved (a slider to the end of its range further from
/// its value, a toggle flipped, a bang fired) as the editor does it, and
/// the scenes are compared at frames 60 and 120.
#[test]
#[ignore = "slow: runs every sample with a control twice"]
fn review_controls() {
    use floret_engine::{Engine, FrameCtx};
    let mut dead = Vec::new();
    let mut checked = 0;
    let only = std::env::var("FLORET_SAMPLES_ONLY").ok();
    floret_video::install();
    let host: std::sync::Arc<dyn floret_engine::host::Host> =
        match std::env::var_os("FLORET_REVIEW_IMAGE") {
            Some(p) => std::sync::Arc::new(StandIn(
                crate::host::shared(),
                std::fs::read(p).expect("FLORET_REVIEW_IMAGE"),
            )),
            None => crate::host::shared(),
        };
    for s in SAMPLES {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|p| s.id.starts_with(p)))
        {
            continue;
        }
        let patch = floret_engine::load_patch(s.flora).expect("sample loads");
        let controls: Vec<_> = patch
            .nodes
            .iter()
            .filter(|n| matches!(n.def_type.as_str(), "io/slider" | "io/toggle" | "io/bang"))
            .map(|n| (n.id.clone(), n.def_type.clone(), n.params.clone()))
            .collect();
        for (id, ty, params) in controls {
            let run = |moved: bool| -> Vec<String> {
                let _wait = floret_engine::video::wait_for_frames();
                let (mut e, _) = Engine::from_patch(&patch).expect("compiles");
                e.set_host(Some(host.clone()));
                let r = e.graph.renderer().expect("renderer");
                let mut seen = Vec::new();
                for f in 0..121u32 {
                    if moved && f == 30 {
                        let num =
                            |k: &str, d: f64| params.get(k).and_then(Json::as_f64).unwrap_or(d);
                        match ty.as_str() {
                            "io/slider" => {
                                let (v, lo, hi) =
                                    (num("value", 0.5), num("min", 0.0), num("max", 1.0));
                                let to = if v - lo > hi - v { lo } else { hi };
                                e.params.set(&id, "value", Json::from(to));
                            }
                            "io/toggle" => {
                                let on =
                                    params.get("value").and_then(Json::as_bool).unwrap_or(false);
                                e.params.set(&id, "value", Json::Bool(!on));
                            }
                            _ => e.params.set(&id, "bang", Json::Bool(true)),
                        }
                    }
                    let ctx = FrameCtx {
                        t: f64::from(f) / 60.0,
                        dt: 1.0 / 60.0,
                        frame_count: f64::from(f),
                        width: 600.0,
                        height: 600.0,
                        ..Default::default()
                    };
                    let res = e.tick(&ctx);
                    if f == 60 || f == 120 {
                        let out = res.outputs.get(r).cloned().flatten().unwrap_or_default();
                        // The layers in full (a group's value is a summary).
                        let layers = out.first().map(|sp| floret_render::flatten_layers(sp));
                        seen.push(format!("{layers:?}"));
                    }
                }
                seen
            };
            checked += 1;
            let (a, b) = (run(false), run(true));
            let effect = a != b;
            eprintln!(
                "{}\t{id}\t{ty}\t{}",
                s.id,
                if effect { "changes" } else { "NO EFFECT" }
            );
            if !effect && !QUIET_CONTROLS.iter().any(|(c, _)| *c == id) {
                dead.push(format!("{}: {id} ({ty})", s.id));
            }
        }
    }
    eprintln!("{checked} controls checked");
    assert!(
        dead.is_empty(),
        "controls with no effect:\n{}",
        dead.join("\n")
    );
}
