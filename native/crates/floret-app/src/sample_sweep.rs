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
//! - **Covers:** each sample's Library cover, made as the Library makes it,
//!   shows real content (not a blank or nearly blank frame).

use crate::{files, thumbs, SAMPLES};
use serde_json::Value as Json;

/// Fixed save time, so two saves compare equal.
const NOW: &str = "2026-10-01T00:00:00.000Z";

/// Samples whose Library cover may show next to nothing (as
/// [`every_sample_cover_shows_something`] measures it): (sample, why).
/// Only what no cover frame, scripted input or bundled cover can fix.
const MAY_BE_BLANK: &[(&str, &str)] = &[];

/// Samples that draw (almost) nothing at [`thumbs::FRAMES`] with no input
/// and no devices, as [`every_sample_renders_on_the_gpu`] runs them: their
/// covers are bundled or played instead.
fn needs_input(s: &crate::Sample) -> bool {
    s.cover.is_some() || s.cover_play || thumbs::downloads(s.flora)
}

/// Video files and image sequences play, as the app installs the decoder
/// at startup (once for the whole test binary).
fn install_video() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        floret_video::install();
    });
}

/// `a` and `b` hold the same JSON, numbers compared by value (a save
/// writes `1.0` as `1`, as JavaScript does).
pub(crate) fn same(a: &Json, b: &Json) -> bool {
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
pub(crate) fn content(p: &floret_engine::patch::PatchFile) -> Json {
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

pub(crate) fn warnings(
    p: &floret_engine::patch::PatchFile,
) -> Result<(usize, Vec<String>), String> {
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
    install_video();
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
        let excused = MAY_BE_BLANK.iter().any(|(id, _)| *id == s.id) || needs_input(s);
        if flat && !excused {
            failures.push(format!("{}: renders one flat colour", s.id));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A Library cover must ink at least this share of its pixels...
const COVER_MIN_INK: f64 = 0.005;
/// ...over a box whose longer side is at least this share of the cover's
/// shorter side (not one speck).
const COVER_MIN_SPAN: f64 = 0.06;

/// Samples whose whole picture is one or two small shapes, so their
/// covers can't reach [`COVER_MIN_INK`] without changing the sample:
/// (sample, what it draws). They're held to [`SMALL_MIN_INK`] and
/// [`SMALL_MIN_SPAN`] instead.
const SMALL_COVERS: &[(&str, &str)] = &[
    ("tut-44-accumulator", "one two-digit number"),
    ("noc-wind-gravity", "one ball"),
    ("noc-drag", "one ball in a tank of water"),
    ("noc-attraction", "two small bodies"),
    ("noc-seek", "a target and the vehicle that reaches it"),
];
const SMALL_MIN_INK: f64 = 0.002;
const SMALL_MIN_SPAN: f64 = 0.05;

/// The colour most of a picture's pixels have: a cover's background,
/// whether the Renderer clears to it or the patch paints it.
fn dominant(px: &[u8]) -> [f64; 4] {
    let mut counts = std::collections::HashMap::<[u8; 3], usize>::new();
    for p in px.as_chunks::<4>().0 {
        *counts.entry([p[0], p[1], p[2]]).or_default() += 1;
    }
    let c = counts
        .into_iter()
        .max_by_key(|(c, n)| (*n, *c))
        .map_or([0; 3], |(c, _)| c);
    let [r, g, b] = c.map(|v| f64::from(v) / 255.0);
    [r, g, b, 1.0]
}

/// What a cover shows: the share of pixels away from its dominant colour
/// and the inked box's longer side as a share of the cover's shorter side.
fn cover_content(w: u32, h: u32, px: &[u8]) -> (f64, f64) {
    let (bb, cov) = ink(px, w, dominant(px));
    let span = bb.map_or(0, |b| (b[2] - b[0] + 1).max(b[3] - b[1] + 1));
    (cov, f64::from(span) / f64::from(w.min(h).max(1)))
}

/// Every Library cover (as the Library makes it: bundled, or rendered at
/// the sample's cover frame, played when its catalogue says so) shows
/// real content, not a blank or nearly blank frame.
/// `FLORET_COVERS_OUT=<dir>` keeps the covers as `<id>.png`.
#[test]
fn every_sample_cover_shows_something() {
    let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
        eprintln!("no GPU adapter: skipping");
        return;
    };
    install_video();
    // No network here: downloads get a stand-in picture.
    let offline: std::sync::Arc<dyn floret_engine::host::Host> =
        std::sync::Arc::new(StandIn(crate::host::shared(), stand_in_picture()));
    let mut failures = Vec::new();
    for s in SAMPLES {
        if MAY_BE_BLANK.iter().any(|(id, _)| *id == s.id) {
            continue;
        }
        let cover = if thumbs::downloads(s.flora) {
            thumbs::render_cover(&hl.device, &hl.queue, &mut hl.renderer, s, offline.clone())
                .map(|(img, _)| img)
        } else {
            thumbs::cover(&hl.device, &hl.queue, &mut hl.renderer, s)
        };
        let Some((w, h, px)) = cover else {
            failures.push(format!("{}: no cover", s.id));
            continue;
        };
        if let Some(dir) = std::env::var_os("FLORET_COVERS_OUT") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            save_png(&dir.join(format!("{}.png", s.id)), w, h, &px);
        }
        if w.max(h) != thumbs::SIZE || px.len() != (w * h * 4) as usize {
            failures.push(format!("{}: {w}×{h} cover with {} bytes", s.id, px.len()));
            continue;
        }
        let (cov, span) = cover_content(w, h, &px);
        let (min_ink, min_span) = if SMALL_COVERS.iter().any(|(id, _)| *id == s.id) {
            (SMALL_MIN_INK, SMALL_MIN_SPAN)
        } else {
            (COVER_MIN_INK, COVER_MIN_SPAN)
        };
        if cov < min_ink || span < min_span {
            failures.push(format!(
                "{}: cover (frame {}) inks {:.2}% of its pixels over {:.0}% of its side",
                s.id,
                thumbs::cover_frame(s),
                cov * 100.0,
                span * 100.0
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A 500 × 500 PNG (the size the samples ask picsum.photos for) standing
/// in for a downloaded photo: a sky gradient over a dark hill, and a sun.
fn stand_in_picture() -> Vec<u8> {
    let (w, h) = (500u32, 500u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
            let hill = v > 0.62 - 0.18 * (u * 3.0).sin() * (1.0 - u);
            let sun = (u - 0.72).powi(2) + (v - 0.28).powi(2) < 0.008;
            // Flowers on the hill: a disc of colour every 40 px.
            let (cx, cy) = ((x % 40) as f64 - 20.0, (y % 40) as f64 - 20.0);
            let flower = hill && cx * cx + cy * cy < 90.0;
            let hue = f64::from((x / 40 * 7 + y / 40 * 3) % 5) / 5.0;
            let (r, g, b) = if sun {
                (1.0, 0.86, 0.45)
            } else if flower {
                (0.9, 0.3 + 0.6 * hue, 0.9 - 0.7 * hue)
            } else if hill {
                let grass = 0.06 * (u * 90.0).sin() * (v * 70.0).cos();
                (0.16 + 0.1 * u + grass, 0.36 - 0.1 * v + grass, 0.2)
            } else {
                (0.35 + 0.5 * v, 0.55 + 0.25 * v, 0.95 - 0.4 * v)
            };
            let c = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
            px.extend_from_slice(&[c(r), c(g), c(b), 255]);
        }
    }
    let mut buf = Vec::new();
    let mut enc = png::Encoder::new(&mut buf, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&px))
        .expect("png encodes");
    buf
}

#[test]
fn cover_lists_name_samples() {
    for (id, _) in MAY_BE_BLANK
        .iter()
        .chain(SMALL_COVERS)
        .chain(BUNDLED_COVERS)
    {
        assert!(
            SAMPLES.iter().any(|s| s.id == *id),
            "{id} is in the catalogue"
        );
    }
}

#[test]
fn played_covers_have_a_script() {
    for s in SAMPLES.iter().filter(|s| s.cover_play) {
        assert!(
            crate::sample_scripts::script_for(s.id).is_some(),
            "{} has coverPlay but no script in sample_scripts",
            s.id
        );
    }
}

// ─── Bundled covers ─────────────────────────────────────────────────────────

/// Samples whose Library cover is bundled (`samples/covers/<id>.png`)
/// because they draw what a device gives them, and the stand-in
/// [`update_covers`] feeds them.
const BUNDLED_COVERS: &[(&str, &str)] = &[
    (
        "body-tracking",
        "two open hands from a stand-in hand tracker",
    ),
    (
        "webcam-mosaic",
        "a stand-in camera frame: a figure against a bright wall",
    ),
    ("tut-49-fft", "a chord on a stand-in microphone"),
    (
        "hand-sparks",
        "two stand-in hands waving, sparks flying off them",
    ),
    (
        "camera-embers",
        "a stand-in camera frame: a lit figure in a dark room",
    ),
];

#[test]
fn bundled_covers_match_the_list() {
    for s in SAMPLES {
        let listed = BUNDLED_COVERS.iter().any(|(id, _)| *id == s.id);
        assert_eq!(
            s.cover.is_some(),
            listed,
            "{}: samples/covers/{}.png {} but the sample is {}in BUNDLED_COVERS \
             (FLORET_UPDATE_COVERS=1 cargo test -p floret-app -- --ignored update_covers)",
            s.id,
            s.id,
            if s.cover.is_some() {
                "exists"
            } else {
                "is missing"
            },
            if listed { "" } else { "not " }
        );
        if let Some(png) = s.cover {
            let (w, h, _) = thumbs::decode_png(png).expect("an 8-bit RGBA PNG");
            assert_eq!(w.max(h), thumbs::SIZE, "{}: cover size", s.id);
        }
    }
}

/// The devices [`update_covers`] lends the engine: a camera showing a
/// figure against a bright wall, and a hand tracker that finds two open
/// hands.
struct CoverDevices(std::sync::Arc<floret_engine::image::Image>);

impl CoverDevices {
    fn new() -> CoverDevices {
        let (w, h) = (640u32, 480u32);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
                // A warm wall, lighter towards the top right.
                let wall = 0.72 + 0.25 * (u * 0.6 + (1.0 - v) * 0.4);
                // Head and shoulders, dark, a little left of centre.
                let head = ((u - 0.45) / 0.13).powi(2) + ((v - 0.38) / 0.17).powi(2) < 1.0;
                let shoulders = ((u - 0.45) / 0.36).powi(2) + ((v - 1.05) / 0.42).powi(2) < 1.0;
                let neck = (u - 0.45).abs() < 0.06 && v > 0.5 && v < 0.7;
                let (r, g, b) = if head || shoulders || neck {
                    let shade = 0.18 + 0.12 * (1.0 - v);
                    (shade, shade * 0.9, shade * 1.2)
                } else {
                    (wall, wall * 0.86, wall * 0.62)
                };
                let c = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                px.extend_from_slice(&[c(r), c(g), c(b), 255]);
            }
        }
        CoverDevices(std::sync::Arc::new(floret_engine::image::Image::new(
            w, h, px,
        )))
    }
}

/// An open hand's 21 landmarks (MediaPipe's order: wrist, then thumb,
/// index, middle, ring and pinky from base to tip), wrist at (`x`, `y`)
/// in normalised image coordinates, fingers spread upwards; `side` -1
/// for a left hand.
fn open_hand(x: f32, y: f32, size: f32, side: f32) -> Vec<[f32; 3]> {
    let mut pts = vec![[x, y, 0.0]];
    let up = -std::f32::consts::FRAC_PI_2;
    // (angle from straight up, base distance, joint lengths)
    let fingers: [(f32, f32, [f32; 3]); 5] = [
        (-0.62, 0.3, [0.26, 0.22, 0.18]),
        (-0.32, 0.9, [0.38, 0.24, 0.2]),
        (-0.06, 0.92, [0.42, 0.27, 0.22]),
        (0.2, 0.86, [0.38, 0.25, 0.2]),
        (0.46, 0.76, [0.3, 0.2, 0.17]),
    ];
    for (a, base, joints) in fingers {
        let ang = up + a * side;
        let (dx, dy) = (ang.cos(), ang.sin());
        let (mut px, mut py) = (x + dx * base * size, y + dy * base * size);
        pts.push([px, py, 0.0]);
        // Each joint bends a little further out.
        for (i, len) in joints.iter().enumerate() {
            let bend = ang + a * side * 0.12 * (i as f32 + 1.0);
            px += bend.cos() * len * size;
            py += bend.sin() * len * size;
            pts.push([px, py, 0.0]);
        }
    }
    pts
}

impl floret_engine::devices::Devices for CoverDevices {
    fn camera(
        &self,
        _: &str,
        req: &floret_engine::devices::CameraRequest,
    ) -> Result<floret_engine::devices::CameraFrame, String> {
        Ok(floret_engine::devices::CameraFrame {
            image: if req.start {
                self.0.clone()
            } else {
                floret_engine::image::Image::placeholder()
            },
            ready: req.start,
        })
    }

    fn landmarks(
        &self,
        _: &str,
        req: &floret_engine::devices::LandmarksRequest,
    ) -> Result<Option<Vec<floret_engine::devices::LandmarkLayer>>, String> {
        use floret_engine::landmarks::Tracker;
        if !req.start || !req.trackers.contains(&Tracker::Hands) {
            return Ok(None);
        }
        Ok(Some(vec![floret_engine::devices::LandmarkLayer {
            tracker: Tracker::Hands,
            dets: vec![
                open_hand(0.27, 0.8, 0.28, -1.0),
                open_hand(0.73, 0.76, 0.28, 1.0),
            ],
            ids: vec![0, 1],
        }]))
    }
}

/// [`CoverDevices`] whose hands wave about: each call of the tracker moves
/// them on, so a sample that reacts to hand speed has something to react to.
struct WavingHands {
    inner: CoverDevices,
    calls: std::sync::atomic::AtomicU32,
}

impl floret_engine::devices::Devices for WavingHands {
    fn camera(
        &self,
        node: &str,
        req: &floret_engine::devices::CameraRequest,
    ) -> Result<floret_engine::devices::CameraFrame, String> {
        floret_engine::devices::Devices::camera(&self.inner, node, req)
    }

    fn landmarks(
        &self,
        _: &str,
        req: &floret_engine::devices::LandmarksRequest,
    ) -> Result<Option<Vec<floret_engine::devices::LandmarkLayer>>, String> {
        use floret_engine::landmarks::Tracker;
        if !req.start || !req.trackers.contains(&Tracker::Hands) {
            return Ok(None);
        }
        let t = self
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed) as f32;
        // Out of step with each other, sweeping a third of the picture.
        let (a, b) = ((t * 0.22).sin(), (t * 0.18 + 1.3).sin());
        Ok(Some(vec![floret_engine::devices::LandmarkLayer {
            tracker: Tracker::Hands,
            dets: vec![
                open_hand(0.3 + 0.11 * a, 0.78 + 0.06 * (t * 0.33).cos(), 0.26, -1.0),
                open_hand(0.7 + 0.11 * b, 0.74 + 0.06 * (t * 0.29).cos(), 0.26, 1.0),
            ],
            ids: vec![0, 1],
        }]))
    }
}

/// [`CoverDevices`]' camera with its light and dark swapped: a lit figure
/// in a dark room.
struct LitFigure(CoverDevices);

impl floret_engine::devices::Devices for LitFigure {
    fn camera(
        &self,
        node: &str,
        req: &floret_engine::devices::CameraRequest,
    ) -> Result<floret_engine::devices::CameraFrame, String> {
        let mut frame = floret_engine::devices::Devices::camera(&self.0, node, req)?;
        if frame.ready {
            let src = frame.image.clone();
            let px: Vec<u8> = src
                .rgba()
                .chunks(4)
                .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 255])
                .collect();
            frame.image = std::sync::Arc::new(floret_engine::image::Image::new(
                src.width(),
                src.height(),
                px,
            ));
        }
        Ok(frame)
    }
}

/// `FLORET_UPDATE_COVERS=1 cargo test -p floret-app -- --ignored
/// update_covers`: renders the [`BUNDLED_COVERS`] to
/// `samples/covers/<id>.png` as the Library would at their cover frame,
/// with stand-in devices installed ([`CoverDevices`]) and the audio
/// engine running offline with a chord on its microphone. Rebuild after
/// running it: the covers are embedded.
#[test]
#[ignore = "writes samples/covers; needs FLORET_UPDATE_COVERS=1 and a GPU adapter"]
fn update_covers() {
    use std::f64::consts::TAU;
    if std::env::var("FLORET_UPDATE_COVERS").as_deref() != Ok("1") {
        eprintln!("FLORET_UPDATE_COVERS=1 not set: skipping");
        return;
    }
    let Some(mut hl) = floret_render::Headless::with_sample_count(4) else {
        eprintln!("no GPU adapter: skipping");
        return;
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/covers");
    std::fs::create_dir_all(&dir).expect("covers dir");
    const RATE: f32 = 48_000.0;
    for (id, _) in BUNDLED_COVERS {
        // Fresh stand-in devices for each (the waving hands start over).
        let devices: std::sync::Arc<dyn floret_engine::devices::Devices> = match *id {
            "hand-sparks" => std::sync::Arc::new(WavingHands {
                inner: CoverDevices::new(),
                calls: Default::default(),
            }),
            "camera-embers" => std::sync::Arc::new(LitFigure(CoverDevices::new())),
            _ => std::sync::Arc::new(CoverDevices::new()),
        };
        floret_engine::devices::install(Some(devices));
        let s = SAMPLES
            .iter()
            .find(|s| s.id == *id)
            .expect("in the catalogue");
        let (mut engine, _) = floret_engine::Engine::load_with_warnings(s.flora).expect("loads");
        engine.set_host(Some(crate::host::shared()));
        let audio = std::sync::Arc::new(floret_media::AudioSystem::offline(RATE));
        engine.set_audio(Some(audio.clone()));
        let mut mic = audio.attach_test_mic(RATE);
        let mut n = 0u64;
        // An A minor chord of bright tones (harmonics up to ~6 kHz).
        let mut chord = |_f: u32| {
            for _ in 0..800 {
                let t = n as f64 / f64::from(RATE);
                let v: f64 = [220.0, 261.63, 329.63]
                    .iter()
                    .flat_map(|hz| (1..=20).map(move |k| (hz * f64::from(k), 0.12 / f64::from(k))))
                    .map(|(hz, a)| a * (TAU * hz * t).sin())
                    .sum::<f64>();
                let _ = mic.push(v as f32);
                n += 1;
            }
        };
        let mut cover = thumbs::Cover::new(&mut engine, thumbs::cover_frame(s), None, Some(&audio));
        let img = cover
            .render(&hl.device, &hl.queue, &mut hl.renderer, &mut chord)
            .expect("rendered");
        let (cov, span) = cover_content(img.0, img.1, &img.2);
        eprintln!(
            "{id}: {:.2}% inked over {:.0}% of the side",
            cov * 100.0,
            span * 100.0
        );
        thumbs::write_png(&dir.join(format!("{id}.png")), &img).expect("cover written");
    }
    floret_engine::devices::install(None);
}

#[test]
fn cover_content_needs_more_than_a_speck() {
    let (w, h) = (100u32, 100u32);
    let mut px = vec![20u8; (w * h * 4) as usize];
    assert_eq!(cover_content(w, h, &px), (0.0, 0.0));
    let paint = |px: &mut [u8], x0: u32, y0: u32, x1: u32, y1: u32| {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 3].copy_from_slice(&[240, 200, 80]);
            }
        }
    };
    // A 3 × 3 speck: too little.
    paint(&mut px, 10, 10, 13, 13);
    assert!(cover_content(w, h, &px).0 < COVER_MIN_INK);
    // A 30 × 30 square: content.
    paint(&mut px, 40, 40, 70, 70);
    let (cov, span) = cover_content(w, h, &px);
    assert!(cov >= COVER_MIN_INK && span >= COVER_MIN_SPAN);
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
pub(crate) fn canon_value(v: &floret_engine::Value) -> String {
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

// ─── Review renders (docs/archive/sample-review.md) ─────────────────────────────────

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
    for (i, p) in px.as_chunks::<4>().0.iter().enumerate() {
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
