//! Pixel-diff floret-render against the web app's WebGL2 renderer.
//!
//! `reference/png/*.png` were rendered by the TS renderer in headless
//! Chromium (`reference/generate.mjs`) from the same scenes: synthetic ones
//! in `reference/scenes/` and recorded sample frames from
//! `fixtures/traces`. Skips when no GPU adapter is available.

mod common;

use common::*;
use floret_render::Headless;

struct Diff {
    /// Mean of per-pixel max channel difference (0..255).
    mean: f64,
    /// Fraction of pixels differing by more than 64 in some channel.
    bad: f64,
}

fn diff(a: &[u8], b: &[u8]) -> (Diff, Vec<u8>) {
    let mut sum = 0u64;
    let mut bad = 0usize;
    let mut img = Vec::with_capacity(a.len());
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        let d = (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0);
        sum += u64::from(d);
        if d > 64 {
            bad += 1;
        }
        let v = d.saturating_mul(4);
        img.extend_from_slice(&[v, v, v, 255]);
    }
    let n = (a.len() / 4).max(1) as f64;
    (
        Diff {
            mean: sum as f64 / n,
            bad: bad as f64 / n,
        },
        img,
    )
}

/// What a reference may differ by. Edges are anti-aliased differently
/// (SwiftShader MSAA-less layers vs our MSAA everywhere) and text goes
/// through different rasterisers (Chromium/Skia vs swash), so text-heavy
/// scenes get looser bounds.
fn tolerance(name: &str) -> (f64, f64) {
    if name == "clip-rotated" {
        // TS over-draws the rotated clip's AABB; we clip exactly.
        (f64::INFINITY, f64::INFINITY)
    } else if name == "text"
        || name.starts_with("type-")
        || name.starts_with("tut-34")
        || name.starts_with("tut-44")
    {
        (6.0, 0.03)
    } else {
        // Mean headroom for anti-aliasing differences between GPU drivers
        // (thousands of small shapes, e.g. webcam-mosaic on Windows WARP);
        // the bad-pixel limit is the real guard.
        (2.5, 0.005)
    }
}

#[test]
fn matches_the_web_renderer() {
    // The references were drawn with the bundled faces (Chrome on Linux);
    // a machine's own fonts (Consolas on Windows) would legitimately differ.
    std::env::set_var("FLORET_BUNDLED_FONTS_ONLY", "1");
    let samples = std::env::var("FLORET_MSAA")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let Some(mut gpu) = Headless::with_sample_count(samples) else {
        eprintln!("floret-render: no GPU adapter — skipping reference diffs");
        return;
    };
    let mut names: Vec<String> = std::fs::read_dir(reference_dir().join("png"))
        .expect("reference/png")
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_suffix(".png")
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty());
    let only = std::env::var("REFERENCE_ONLY").ok();
    let mut failures = Vec::new();
    for name in names {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == name))
        {
            continue;
        }
        let scene_path = reference_dir().join("scenes").join(format!("{name}.json"));
        let scene = if scene_path.exists() {
            let j: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&scene_path).unwrap()).unwrap();
            scene_from_json(&name, &j)
        } else {
            let (sample, frame) = name.rsplit_once("-f").expect("<sample>-f<i>");
            let trace = load_trace(sample).expect("trace");
            scene(&trace, frame.parse().unwrap()).expect("scene")
        };
        let (rw, rh, reference) =
            read_png(&reference_dir().join("png").join(format!("{name}.png")))
                .expect("reference png");
        assert_eq!((rw, rh), (scene.width, scene.height), "{name}: size");
        let ours = gpu.render(&scene.layers, scene.width, scene.height, scene.background);
        let (d, img) = diff(&ours, &reference);
        let (max_mean, max_bad) = tolerance(&name);
        let ok = d.mean <= max_mean && d.bad <= max_bad;
        eprintln!(
            "{} {name:32} mean {:6.3}  bad {:7.4}%",
            if ok { "ok  " } else { "FAIL" },
            d.mean,
            d.bad * 100.0
        );
        if let Some(dir) = out_dir() {
            write_png(&dir.join(format!("{name}.ours.png")), rw, rh, &ours);
            write_png(&dir.join(format!("{name}.diff.png")), rw, rh, &img);
        }
        if !ok {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "differs from the web renderer: {failures:?}"
    );
}
