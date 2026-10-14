//! Rebuilt samples behave as before.
//!
//! When a Library sample is rebuilt with different nodes (formulas turned
//! into visual nodes, say), its node ids change, so
//! `sample_sweep::rewritten_samples_draw_as_before` can only compare the
//! nodes both versions share. Here the old and the new file are played
//! side by side with the same scripted input — the mouse circling and
//! dragging, keys held, every slider, toggle, XY pad and bang moved — and
//! what reaches the Renderer is compared every frame: the flattened layers
//! number by number (within a small tolerance) and the rendered pictures
//! (every frame is rendered, in order, so trails and feedback see their
//! history). The wall clock is frozen for both runs. It also reports the
//! median tick time of each version.
//!
//! `FLORET_SAMPLES_BEFORE=<dir of old .flora files> FLORET_SAMPLES_ONLY=a,b
//! cargo test -p floret-app -- --ignored rebuilt_samples_match --nocapture`
//! (`FLORET_COMPARE_FRAMES`, default 180; `FLORET_COMPARE_OUT=<dir>` keeps
//! the last frame of both as PNGs).

use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx};
use serde_json::Value as Json;
use std::sync::Arc;
use std::time::Instant;

/// Relative / absolute tolerance for numbers in the layers.
const TOLERANCE: f64 = 1e-6;

/// A fixed wall-clock time for `time/clock` (2026-03-14 15:09:26 UTC).
const FROZEN_CLOCK: f64 = 1_773_500_966.0;

/// The input on frame `f` of a canvas `w × h`: the mouse circles, presses
/// from frame 40 to 70 and again from 130 to 150 (a drag), keys are held
/// now and then.
fn frame_ctx(f: u32, size: (u32, u32)) -> FrameCtx {
    let dt = 1.0 / 60.0;
    let t = f64::from(f) * dt;
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let r = 0.3 * w.min(h);
    let keys: &[&str] = match f {
        20..=24 => &["r"],
        50..=52 => &[" "],
        80..=83 => &["ArrowLeft"],
        100..=110 => &["a", "s"],
        140..=142 => &["1"],
        _ => &[],
    };
    FrameCtx {
        t,
        dt,
        frame_count: f64::from(f),
        width: w,
        height: h,
        mouse_x: r * (t * 1.3).cos(),
        mouse_y: r * 0.8 * (t * 1.7).sin(),
        mouse_is_pressed: (40..70).contains(&f) || (130..150).contains(&f),
        key: keys.last().map(|k| (*k).to_owned()).unwrap_or_default(),
        key_is_pressed: !keys.is_empty(),
        keys_down: keys.iter().map(|k| (*k).to_owned()).collect(),
        typed: if f == 100 { "a".into() } else { String::new() },
        loop_length: 0.0,
    }
}

/// The controls of a patch and what happens to them: (frame, node, param,
/// value). Each slider goes to a quarter of its range above its minimum at
/// frame 60 and to its top at 120; toggles flip at 90 and back at 150; XY
/// pads move at 75; bangs fire at 110.
fn control_moves(flora: &str) -> Vec<(u32, String, String, Json)> {
    let patch = floret_engine::load_patch(flora).expect("loads");
    let mut moves = Vec::new();
    for n in &patch.nodes {
        let num = |k: &str, d: f64| n.params.get(k).and_then(Json::as_f64).unwrap_or(d);
        match n.def_type.as_str() {
            "io/slider" => {
                let (lo, hi) = (num("min", 0.0), num("max", 1.0));
                moves.push((
                    60,
                    n.id.clone(),
                    "value".into(),
                    Json::from(lo + 0.25 * (hi - lo)),
                ));
                moves.push((120, n.id.clone(), "value".into(), Json::from(hi)));
            }
            "io/toggle" => {
                let on = n
                    .params
                    .get("value")
                    .and_then(Json::as_bool)
                    .unwrap_or(false);
                moves.push((90, n.id.clone(), "value".into(), Json::Bool(!on)));
                moves.push((150, n.id.clone(), "value".into(), Json::Bool(on)));
            }
            "io/xy-pad" => {
                moves.push((75, n.id.clone(), "x".into(), Json::from(0.7)));
                moves.push((75, n.id.clone(), "y".into(), Json::from(0.25)));
            }
            "io/bang" => moves.push((110, n.id.clone(), "bang".into(), Json::Bool(true))),
            _ => {}
        }
    }
    moves
}

struct Played {
    size: (u32, u32),
    /// Per frame: the layers as text, and the layers and background.
    frames: Vec<(String, Vec<Arc<SceneNode>>, [f64; 4])>,
    tick_ms: f64,
    errors: Vec<String>,
}

fn play(flora: &str, frames: u32, moves: &[(u32, String, String, Json)]) -> Result<Played, String> {
    let _wait = floret_engine::video::wait_for_frames();
    let (mut engine, warnings) = Engine::load_with_warnings(flora)?;
    let mut errors: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    engine.set_host(Some(crate::host::shared()));
    engine.set_profiling(std::env::var_os("FLORET_COMPARE_PROFILE").is_some());
    let r = engine.graph.renderer().ok_or("no renderer")?;
    let id = engine.graph.nodes[r].id.clone();
    let size = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let mut out_frames = Vec::new();
    let mut ticks = Vec::new();
    for f in 0..frames {
        for (at, node, key, value) in moves {
            if *at == f {
                engine.params.set(node, key, value.clone());
            }
            // A bang is a one-frame pulse.
            if *at + 1 == f && key == "bang" {
                engine.params.set(node, key, Json::Bool(false));
            }
        }
        let t0 = Instant::now();
        let res = engine.tick(&frame_ctx(f, size));
        ticks.push(t0.elapsed().as_secs_f64() * 1e3);
        for (i, msg) in &res.errors {
            // Devices this machine lacks.
            if ["no audio output", "camera", "microphone"]
                .iter()
                .any(|d| msg.contains(d))
            {
                continue;
            }
            let line = format!("{}: {msg}", engine.graph.nodes[*i].id);
            if !errors.contains(&line) {
                errors.push(line);
            }
        }
        let out = res.outputs.get(r).cloned().flatten().unwrap_or_default();
        let layers = out
            .first()
            .map(|s| floret_render::flatten_layers(s))
            .unwrap_or_default();
        let bg = out
            .get(1)
            .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
        out_frames.push((without_image_ids(&format!("{layers:?}")), layers, bg));
    }
    ticks.sort_by(f64::total_cmp);
    if std::env::var_os("FLORET_COMPARE_PROFILE").is_some() {
        let p = engine.profile();
        let mut slow: Vec<(f64, String)> = p
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.avg_ms, engine.graph.nodes[i].id.clone()))
            .collect();
        slow.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (ms, id) in slow.iter().take(12) {
            eprintln!("    {ms:.3} ms  {id}");
        }
    }
    Ok(Played {
        size,
        frames: out_frames,
        tick_ms: ticks.get(ticks.len() / 2).copied().unwrap_or(0.0),
        errors,
    })
}

/// Layer text without image ids (a global counter, different every run).
fn without_image_ids(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("Image#") {
        out.push_str(&rest[..i + 6]);
        rest = rest[i + 6..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// Split text into numbers and the text between them.
fn tokens(s: &str) -> Vec<Result<f64, &str>> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let (mut i, mut start) = (0, 0);
    while i < b.len() {
        let c = b[i];
        let starts_num = c.is_ascii_digit()
            || (c == b'-' && i + 1 < b.len() && b[i + 1].is_ascii_digit())
            || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit());
        let in_word = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if starts_num && !in_word {
            let mut j = i + 1;
            while j < b.len()
                && (b[j].is_ascii_digit()
                    || b[j] == b'.'
                    || b[j] == b'e'
                    || ((b[j] == b'-' || b[j] == b'+') && b[j - 1] == b'e'))
            {
                j += 1;
            }
            if let Ok(v) = s[i..j].parse::<f64>() {
                if start < i {
                    out.push(Err(&s[start..i]));
                }
                out.push(Ok(v));
                start = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    if start < s.len() {
        out.push(Err(&s[start..]));
    }
    out
}

/// Where two layer texts differ beyond the tolerance (the worst number,
/// or the first structural difference).
fn layer_diff(a: &str, b: &str) -> Option<String> {
    if a == b {
        return None;
    }
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.len() != tb.len() {
        let at = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
        let cut = |s: &str| {
            s.chars()
                .skip(at.saturating_sub(80))
                .take(200)
                .collect::<String>()
        };
        return Some(format!(
            "structure differs ({} vs {} tokens) near: {} ⟶ {}",
            ta.len(),
            tb.len(),
            cut(a),
            cut(b)
        ));
    }
    let mut worst: Option<(f64, f64, f64)> = None;
    for (x, y) in ta.iter().zip(&tb) {
        match (x, y) {
            (Ok(p), Ok(q)) => {
                let d = (p - q).abs();
                let tol = TOLERANCE * p.abs().max(q.abs()).max(1.0);
                if (d > tol || p.is_nan() != q.is_nan()) && worst.is_none_or(|w| d > w.0) {
                    worst = Some((d, *p, *q));
                }
            }
            (Err(p), Err(q)) if p == q => {}
            (p, q) => return Some(format!("text differs: {p:?} vs {q:?}")),
        }
    }
    worst.map(|(d, p, q)| format!("number {p} vs {q} (Δ {d:e})"))
}

/// The largest relative difference between the numbers of two layer texts
/// of the same structure (0 when identical, NaN when the structures
/// differ).
fn largest_difference(a: &str, b: &str) -> f64 {
    if a == b {
        return 0.0;
    }
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.len() != tb.len() {
        return f64::NAN;
    }
    ta.iter()
        .zip(&tb)
        .map(|(x, y)| match (x, y) {
            (Ok(p), Ok(q)) if p != q => (p - q).abs() / p.abs().max(q.abs()).max(1.0),
            (Err(p), Err(q)) if p != q => f64::NAN,
            _ => 0.0,
        })
        .fold(0.0, f64::max)
}

/// Render every frame in order; returns the pictures at `keep`.
fn render_all(
    hl: &mut floret_render::Headless,
    run: &Played,
    keep: &[usize],
) -> Vec<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    for (f, (_, layers, bg)) in run.frames.iter().enumerate() {
        hl.renderer.set_clock(floret_render::RenderClock {
            time: f as f64 / 60.0,
            time_delta: 1.0 / 60.0,
            frame: f as f64,
            ..Default::default()
        });
        let (px, _) = floret_render::render_and_read(
            &hl.device,
            &hl.queue,
            &mut hl.renderer,
            layers,
            run.size,
            *bg,
        );
        if keep.contains(&f) {
            out.push((f, px));
        }
    }
    out
}

/// (pixels differing by more than 2 of 255 in a channel, the largest
/// difference).
fn pixel_diff(a: &[u8], b: &[u8]) -> (usize, u8) {
    let (pa, _) = a.as_chunks::<4>();
    let (pb, _) = b.as_chunks::<4>();
    let mut n = 0;
    let mut most = 0;
    for (x, y) in pa.iter().zip(pb) {
        let d = x
            .iter()
            .zip(y)
            .map(|(p, q)| p.abs_diff(*q))
            .max()
            .unwrap_or(0);
        most = most.max(d);
        if d > 2 {
            n += 1;
        }
    }
    (n, most)
}

#[test]
#[ignore = "needs FLORET_SAMPLES_BEFORE and FLORET_SAMPLES_ONLY (and a GPU adapter for pictures)"]
fn rebuilt_samples_match() {
    let Some(before_dir) = std::env::var_os("FLORET_SAMPLES_BEFORE") else {
        eprintln!("FLORET_SAMPLES_BEFORE not set: skipping");
        return;
    };
    let only = std::env::var("FLORET_SAMPLES_ONLY").expect("FLORET_SAMPLES_ONLY=a,b");
    let frames: u32 = std::env::var("FLORET_COMPARE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(180);
    let out_dir = std::env::var_os("FLORET_COMPARE_OUT").map(std::path::PathBuf::from);
    floret_engine::nodes::time::freeze_clock(Some(FROZEN_CLOCK));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut failures = Vec::new();
    for id in only.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let name = format!("{id}.flora");
        let after = std::fs::read_to_string(dir.join(&name)).expect("new sample");
        let before = std::fs::read_to_string(std::path::Path::new(&before_dir).join(&name))
            .expect("old sample");
        let moves = control_moves(&before);
        let (a, b) = match (play(&before, frames, &moves), play(&after, frames, &moves)) {
            (Ok(a), Ok(b)) => (a, b),
            (a, b) => {
                failures.push(format!("{id}: {:?} / {:?}", a.err(), b.err()));
                continue;
            }
        };
        for e in &b.errors {
            failures.push(format!("{id}: new version: {e}"));
        }
        let mut differing = 0;
        let mut first = None;
        let (mut identical, mut largest) = (0, 0.0f64);
        for (x, y) in a.frames.iter().zip(&b.frames) {
            identical += usize::from(x.0 == y.0);
            largest = largest.max(largest_difference(&x.0, &y.0));
        }
        eprintln!(
            "{id}: layers identical on {identical} of {frames} frames; largest relative difference {largest:e}"
        );
        for (f, (x, y)) in a.frames.iter().zip(&b.frames).enumerate() {
            if let Some(d) = layer_diff(&x.0, &y.0) {
                differing += 1;
                if first.is_none() {
                    first = Some(format!("frame {f}: {d}"));
                }
            }
        }
        eprintln!(
            "{id}: {frames} frames, layers differ on {differing}{}; tick {:.2} ms → {:.2} ms",
            first
                .as_deref()
                .map(|s| format!(" (first: {s})"))
                .unwrap_or_default(),
            a.tick_ms,
            b.tick_ms
        );
        if let Some(first) = first {
            failures.push(format!(
                "{id}: layers differ on {differing} frames, first {first}"
            ));
        }
        let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
            eprintln!("no GPU adapter: pictures not compared");
            continue;
        };
        let keep: Vec<usize> = [1, 30, 60, 90, 120, 150, frames as usize - 1]
            .into_iter()
            .filter(|f| *f < frames as usize)
            .collect();
        let pa = render_all(&mut hl, &a, &keep);
        let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
            continue;
        };
        let pb = render_all(&mut hl, &b, &keep);
        for ((f, x), (_, y)) in pa.iter().zip(&pb) {
            let (n, most) = pixel_diff(x, y);
            eprintln!("  frame {f}: {n} pixels differ (most {most})");
            if n > 0 {
                failures.push(format!("{id}: frame {f}: {n} pixels differ (most {most})"));
            }
        }
        if let (Some(dir), Some((_, x)), Some((_, y))) = (&out_dir, pa.last(), pb.last()) {
            let _ = std::fs::create_dir_all(dir);
            let (w, h) = a.size;
            for (tag, px) in [("before", x), ("after", y)] {
                crate::thumbs::write_png(&dir.join(format!("{id}-{tag}.png")), &(w, h, px.clone()))
                    .expect("png written");
            }
        }
    }
    floret_engine::nodes::time::freeze_clock(None);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn layer_texts_compare_by_number() {
    assert_eq!(
        layer_diff("Rect { x: 1.0, y: -2.5 }", "Rect { x: 1.0, y: -2.5 }"),
        None
    );
    assert_eq!(
        layer_diff(
            "Rect { x: 1.0, y: -2.5 }",
            "Rect { x: 1.0000000001, y: -2.5 }"
        ),
        None
    );
    assert!(layer_diff("Rect { x: 1.0 }", "Rect { x: 1.5 }").is_some());
    assert!(layer_diff("Rect { x: 1.0 }", "Ellipse { x: 1.0 }").is_some());
    assert!(layer_diff("[a1, 2]", "[a1, 2, 3]").is_some());
    // Digits inside names are text.
    assert_eq!(tokens("s1: 2e-3"), vec![Err("s1: "), Ok(0.002)]);
}
