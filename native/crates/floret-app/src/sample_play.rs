//! The Interactive and Sound samples respond to being played.
//!
//! `sample_sweep` checks every sample with no input. Here each
//! interaction-first sample is also run with a scripted performance —
//! mouse moves, presses and drags, keys held, text typed — and the frame
//! it ends on must differ from the same frame played with no input, and
//! no node may fail on the way. Devices a test machine lacks (MIDI,
//! camera, microphone, speakers) are left to their fallbacks; the engine
//! tests (`floret-engine/tests/interactive_samples.rs`) and the audio
//! tests (`floret-media/tests/sample_sound.rs`) drive those.
//!
//! `FLORET_RENDER_OUT=<dir>` keeps both frames as PNGs
//! (`<sample>-idle.png`, `<sample>-played.png`). The ignored
//! `play_one` test runs a single `.flora` file (`FLORET_PLAY=<path>`,
//! optional `FLORET_PLAY_SCRIPT=<sample id>`) the same way, for
//! authoring.

use crate::sample_scripts::{frame_ctx, Script, SCRIPTS};
use crate::{thumbs, SAMPLES};
use floret_engine::scene::SceneNode;
use floret_engine::Engine;
use floret_render::RenderClock;
use std::sync::Arc;

/// Frames each performance lasts (at 60 fps).
const FRAMES: u32 = 150;

/// A finished run: the layers and background of the renderer's last frame
/// (and the few before it, for Echo's history), and every node error.
struct Run {
    size: (u32, u32),
    frames: Vec<(Vec<Arc<SceneNode>>, [f64; 4])>,
    errors: Vec<String>,
}

/// Errors that only mean this machine has no such device.
fn device_error(msg: &str) -> bool {
    ["no audio output", "camera", "microphone"]
        .iter()
        .any(|d| msg.contains(d))
}

fn run(flora: &str, script: Option<Script>) -> Run {
    let (mut engine, warnings) = Engine::load_with_warnings(flora).expect("sample loads");
    assert!(
        warnings.is_empty(),
        "compile warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    engine.set_host(Some(crate::host::shared()));
    let r = engine.graph.renderer().expect("a renderer");
    let id = engine.graph.nodes[r].id.clone();
    let size = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let mut frames = Vec::new();
    let mut errors = Vec::new();
    for f in 0..FRAMES {
        let input = script.map(|s| s(f)).unwrap_or_default();
        let res = engine.tick(&frame_ctx(f, size, &input));
        for (i, msg) in &res.errors {
            if !device_error(msg) {
                errors.push(format!("frame {f}: {}: {msg}", engine.graph.nodes[*i].id));
            }
        }
        if f + 12 >= FRAMES {
            let out = res.outputs.get(r).cloned().flatten().unwrap_or_default();
            let layers = out
                .first()
                .map(|s| floret_render::flatten_layers(s))
                .unwrap_or_default();
            let bg = out
                .get(1)
                .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
            frames.push((layers, bg));
        }
    }
    errors.dedup();
    Run {
        size,
        frames,
        errors,
    }
}

type Rgba = (u32, u32, Vec<u8>);

/// Render a run's last frames in order (so Echo and feedback effects see
/// their history) and read back the final one.
fn render(hl: &mut floret_render::Headless, run: &Run) -> Rgba {
    let (w, h) = run.size;
    let mut px = Vec::new();
    for (i, (layers, bg)) in run.frames.iter().enumerate() {
        let n = FRAMES as usize - run.frames.len() + i;
        hl.renderer.set_clock(RenderClock {
            time: n as f64 / 60.0,
            time_delta: 1.0 / 60.0,
            frame: n as f64,
            ..Default::default()
        });
        px = floret_render::render_and_read(
            &hl.device,
            &hl.queue,
            &mut hl.renderer,
            layers,
            (w, h),
            *bg,
        )
        .0;
    }
    (w, h, px)
}

/// Share of pixels that differ by more than a little between two frames.
fn changed(a: &Rgba, b: &Rgba) -> f64 {
    let (pa, _) = a.2.as_chunks::<4>();
    let (pb, _) = b.2.as_chunks::<4>();
    let n = pa
        .iter()
        .zip(pb)
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 24))
        .count();
    n as f64 / pa.len().max(1) as f64
}

fn keep(name: &str, img: &Rgba) {
    if let Some(dir) = std::env::var_os("FLORET_RENDER_OUT") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        thumbs::write_png(&dir.join(format!("{name}.png")), img).expect("png written");
    }
}

fn flora_of(id: &str) -> &'static str {
    SAMPLES
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("{id} is in the catalogue"))
        .flora
}

#[test]
fn interactive_samples_have_a_performance() {
    for s in SAMPLES {
        if s.category == "Interactive" || s.id.starts_with("play-") {
            assert!(
                SCRIPTS.iter().any(|(id, _)| *id == s.id),
                "{} has no scripted performance here",
                s.id
            );
        }
    }
}

#[test]
fn interactive_samples_respond_to_play() {
    let mut failures = Vec::new();
    let mut hl = floret_render::Headless::with_sample_count(1);
    for (id, script) in SCRIPTS {
        let flora = flora_of(id);
        let idle = run(flora, None);
        let played = run(flora, Some(*script));
        for e in idle.errors.iter().chain(&played.errors) {
            failures.push(format!("{id}: {e}"));
        }
        let Some(hl) = hl.as_mut() else { continue };
        let (a, b) = (render(hl, &idle), render(hl, &played));
        keep(&format!("{id}-idle"), &a);
        keep(&format!("{id}-played"), &b);
        let share = changed(&a, &b);
        if share < 0.01 {
            failures.push(format!(
                "{id}: playing it changes only {:.2}% of the frame",
                share * 100.0
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
#[ignore = "authoring aid: FLORET_PLAY=<file.flora> [FLORET_PLAY_SCRIPT=<id>]"]
fn play_one() {
    let path = std::env::var("FLORET_PLAY").expect("FLORET_PLAY=<file.flora>");
    let flora = std::fs::read_to_string(&path).expect("readable");
    let name = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let script_id = std::env::var("FLORET_PLAY_SCRIPT").unwrap_or_else(|_| name.clone());
    let script = SCRIPTS.iter().find(|(id, _)| *id == script_id).map(|s| s.1);
    let idle = run(&flora, None);
    let played = run(&flora, script);
    for e in idle.errors.iter().chain(&played.errors).take(30) {
        eprintln!("error: {e}");
    }
    if let Some(mut hl) = floret_render::Headless::with_sample_count(1) {
        let (a, b) = (render(&mut hl, &idle), render(&mut hl, &played));
        keep(&format!("{name}-idle"), &a);
        keep(&format!("{name}-played"), &b);
        eprintln!("changed: {:.2}%", changed(&a, &b) * 100.0);
    }
}

// ─── Rebuilt samples play as before ─────────────────────────────────────────

/// Numbers in two texts closer than this (relative, at least 1 absolute)
/// count as equal: rebuilding a formula from nodes can change the order of
/// floating-point operations.
const SAME_WITHIN: f64 = 1e-6;

/// Rebuilds that decide a corner case differently on purpose, so their
/// scripted runs may draw differently: (sample, why). Their differences
/// are reported, not failed; errors and warnings still fail.
const DECIDED_DIFFERENTLY: &[(&str, &str)] = &[
    (
        "sound-step-sequencer",
        "Hit Test gives a mouse exactly on a cell border to the cell before it; the formula \
         gave it to the one after (the script drags along borders)",
    ),
    (
        "play-physics-sandbox",
        "Edge doesn't count a button already down on the very first frame as a press (the \
         script presses on frame 0)",
    ),
];

/// The renderer's output on every frame of a performance, as text, and the
/// run (its last frames, for rendering).
fn traced(flora: &str, script: Option<Script>) -> (Vec<String>, Run) {
    let (mut engine, _) = Engine::load_with_warnings(flora).expect("sample loads");
    engine.set_host(Some(crate::host::shared()));
    let r = engine.graph.renderer().expect("a renderer");
    let id = engine.graph.nodes[r].id.clone();
    let size = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let (mut texts, mut frames, mut errors) = (Vec::new(), Vec::new(), Vec::new());
    for f in 0..FRAMES {
        let input = script.map(|s| s(f)).unwrap_or_default();
        let res = engine.tick(&frame_ctx(f, size, &input));
        for (i, msg) in &res.errors {
            if !device_error(msg) {
                errors.push(format!("frame {f}: {}: {msg}", engine.graph.nodes[*i].id));
            }
        }
        let out = res.outputs.get(r).cloned().flatten().unwrap_or_default();
        texts.push(
            out.iter()
                .map(|s| s.iter().map(scene_text).collect::<Vec<_>>().join(";"))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        if f + 12 >= FRAMES {
            let layers = out
                .first()
                .map(|s| floret_render::flatten_layers(s))
                .unwrap_or_default();
            let bg = out
                .get(1)
                .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
            frames.push((layers, bg));
        }
    }
    errors.dedup();
    (
        texts,
        Run {
            size,
            frames,
            errors,
        },
    )
}

/// A value as comparable text: drawn layers in full (every shape, style
/// and transform), image ids (a global counter) left out.
fn scene_text(v: &floret_engine::Value) -> String {
    let s = match v {
        floret_engine::Value::Scene(n) => format!("{n:?}"),
        other => return crate::sample_sweep::canon_value(other),
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

/// Split text into runs of non-number text and numbers.
fn tokens(s: &str) -> Vec<Result<f64, &str>> {
    let b = s.as_bytes();
    let digit = |k: usize| b.get(k).is_some_and(u8::is_ascii_digit);
    // A number starts a word: digits inside names (`s1`) are text.
    let number_at = |i: usize| {
        let word = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        word && (digit(i)
            || (b[i] == b'-' && (digit(i + 1) || (b.get(i + 1) == Some(&b'.') && digit(i + 2)))))
    };
    let (mut out, mut i, mut start) = (Vec::new(), 0, 0);
    while i < b.len() {
        if !number_at(i) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() {
            if b[j].is_ascii_digit() || b[j] == b'.' {
                j += 1;
            } else if (b[j] == b'e' || b[j] == b'E')
                && (digit(j + 1) || (b.get(j + 1) == Some(&b'-') && digit(j + 2)))
            {
                j += 2;
            } else {
                break;
            }
        }
        if let Ok(x) = s[i..j].parse::<f64>() {
            if start < i {
                out.push(Err(&s[start..i]));
            }
            out.push(Ok(x));
            start = j;
        }
        i = j;
    }
    if start < b.len() {
        out.push(Err(&s[start..]));
    }
    out
}

/// The largest relative difference between the numbers of two texts that
/// are otherwise the same, or `None` when the texts differ in shape.
fn deviation(a: &str, b: &str) -> Option<f64> {
    if a == b {
        return Some(0.0);
    }
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.len() != tb.len() {
        return None;
    }
    let mut worst = 0.0f64;
    for (x, y) in ta.iter().zip(&tb) {
        match (x, y) {
            (Ok(p), Ok(q)) => {
                if p.is_nan() && q.is_nan() {
                    continue;
                }
                let d = (p - q).abs() / p.abs().max(q.abs()).max(1.0);
                worst = worst.max(if d.is_nan() { f64::INFINITY } else { d });
            }
            (Err(p), Err(q)) if p == q => {}
            _ => return None,
        }
    }
    Some(worst)
}

/// Where two layer texts first differ (by more than [`SAME_WITHIN`]), with
/// some text either side.
fn first_difference(a: &str, b: &str) -> (String, String) {
    let at = |s: &str, i: usize| {
        let lo = s.floor_char_boundary(i.saturating_sub(160));
        let hi = s.ceil_char_boundary((i + 160).min(s.len()));
        format!("…{}…", &s[lo..hi])
    };
    let (ta, tb) = (tokens(a), tokens(b));
    let (mut ia, mut ib) = (0, 0);
    for (x, y) in ta.iter().zip(&tb) {
        let same = match (x, y) {
            (Ok(p), Ok(q)) => {
                (p.is_nan() && q.is_nan())
                    || (p - q).abs() / p.abs().max(q.abs()).max(1.0) <= SAME_WITHIN
            }
            (Err(p), Err(q)) => p == q,
            _ => false,
        };
        if !same {
            return (at(a, ia), at(b, ib));
        }
        let len = |t: &Result<f64, &str>, s: &str, i: usize| match t {
            Err(text) => text.len(),
            // A number's text: up to the next non-number character.
            Ok(_) => s[i..]
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E')))
                .unwrap_or(s.len() - i),
        };
        ia += len(x, a, ia);
        ib += len(y, b, ib);
    }
    (at(a, ia), at(b, ib))
}

/// Share of pixels that differ at all between two frames, and the largest
/// channel difference.
fn pixel_difference(a: &Rgba, b: &Rgba) -> (f64, u8) {
    let (pa, _) = a.2.as_chunks::<4>();
    let (pb, _) = b.2.as_chunks::<4>();
    let (mut n, mut most) = (0usize, 0u8);
    for (x, y) in pa.iter().zip(pb) {
        let d = x
            .iter()
            .zip(y)
            .map(|(p, q)| p.abs_diff(*q))
            .max()
            .unwrap_or(0);
        if d > 0 {
            n += 1;
        }
        most = most.max(d);
    }
    (n as f64 / pa.len().max(1) as f64, most)
}

/// `FLORET_SAMPLES_BEFORE=<dir> cargo test -p floret-app -- --ignored
/// rebuilt_samples_play_as_before`: every scripted sample whose file differs
/// from its copy in `<dir>` must give the renderer the same layers on every
/// frame — idle and played by its script — up to [`SAME_WITHIN`], and its
/// last frames must render to the same pixels. For rebuilds that mustn't
/// change what a sample does (Expression nodes replaced by visual nodes).
/// `FLORET_SAMPLES_ONLY=a,b` limits it to ids starting so;
/// `FLORET_RENDER_OUT=<dir>` keeps the frames.
#[test]
#[ignore = "needs FLORET_SAMPLES_BEFORE"]
fn rebuilt_samples_play_as_before() {
    let Some(before_dir) = std::env::var_os("FLORET_SAMPLES_BEFORE") else {
        eprintln!("FLORET_SAMPLES_BEFORE not set: skipping");
        return;
    };
    let only = std::env::var("FLORET_SAMPLES_ONLY").ok();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let (mut failures, mut compared) = (Vec::new(), 0);
    for (id, script) in SCRIPTS {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|s| id.starts_with(s)))
        {
            continue;
        }
        let name = format!("{id}.flora");
        let after = std::fs::read_to_string(dir.join(&name)).expect("sample file");
        let Ok(before) = std::fs::read_to_string(std::path::Path::new(&before_dir).join(&name))
        else {
            continue;
        };
        if before == after {
            continue;
        }
        compared += 1;
        match Engine::load_with_warnings(&after) {
            Ok((_, warnings)) => failures.extend(
                warnings
                    .iter()
                    .map(|w| format!("{id}: compile warning: {}", w.message)),
            ),
            Err(e) => {
                failures.push(format!("{id}: doesn't load: {e}"));
                continue;
            }
        }
        let decided = DECIDED_DIFFERENTLY.iter().find(|(s, _)| s == id);
        for (how, script) in [("idle", None), ("played", Some(*script))] {
            let (ta, ra) = traced(&before, script);
            let (tb, rb) = traced(&after, script);
            for e in rb.errors.iter().take(5) {
                failures.push(format!("{id} {how}: {e}"));
            }
            let mut differences = Vec::new();
            let mut worst = 0.0f64;
            for (f, (a, b)) in ta.iter().zip(&tb).enumerate() {
                let d = deviation(a, b);
                if let Some(d) = d {
                    worst = worst.max(d);
                }
                if d.is_none_or(|d| d > SAME_WITHIN) {
                    let (x, y) = first_difference(a, b);
                    differences.push(format!(
                        "{id} {how}: frame {f} draws differently:\n    {x}\n  → {y}"
                    ));
                    break;
                }
            }
            if worst > SAME_WITHIN {
                differences.push(format!("{id} {how}: numbers differ by up to {worst:e}"));
            }
            let mut pixels = String::new();
            // A fresh renderer for each run: Echo and feedback effects keep
            // past frames, which mustn't leak from one run into the other.
            let fresh = || floret_render::Headless::with_sample_count(1);
            if let (Some(mut ha), Some(mut hb)) = (fresh(), fresh()) {
                let (a, b) = (render(&mut ha, &ra), render(&mut hb, &rb));
                keep(&format!("{id}-{how}-before"), &a);
                keep(&format!("{id}-{how}-after"), &b);
                let (share, most) = pixel_difference(&a, &b);
                pixels = format!(
                    "; last frame: {:.3}% of pixels differ (by at most {most})",
                    share * 100.0
                );
                if share > 0.001 || most > 8 {
                    differences.push(format!("{id} {how}: pixels differ{pixels}"));
                }
            }
            eprintln!("{id} {how}: layers within {worst:e} over {FRAMES} frames{pixels}");
            match decided {
                Some((_, why)) if !differences.is_empty() => {
                    eprintln!("  (allowed: {why})");
                }
                _ => failures.extend(differences),
            }
        }
    }
    eprintln!("{compared} rebuilt samples compared");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn layer_texts_compare_numbers_within_a_tolerance() {
    assert_eq!(
        deviation("Rect(x: 1.5, y: -2)", "Rect(x: 1.5, y: -2)"),
        Some(0.0)
    );
    let d = deviation("x: 100.0, y: 2e-3", "x: 100.00000001, y: 0.002").unwrap();
    assert!(d > 0.0 && d < 1e-9, "{d}");
    assert_eq!(deviation("Rect(x: 1)", "Ellipse(x: 1)"), None);
    assert_eq!(deviation("a: [1, 2]", "a: [1, 2, 3]"), None);
    assert!(deviation("v: NaN", "v: NaN").is_some());
    // Digits inside names are text, not numbers.
    assert_eq!(deviation("s1: 2", "s2: 2"), None);
}
