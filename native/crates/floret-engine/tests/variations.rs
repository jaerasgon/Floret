//! The patch seed and presets (item 5, #26): patch seed 0 draws exactly
//! what the patch drew before seeds existed, other seeds draw something
//! else, and `.flora` files round-trip with and without the new fields.

use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::{preset, Engine, FrameCtx};
use std::path::Path;

fn sample(id: &str) -> String {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples"))
        .join(format!("{id}.flora"));
    std::fs::read_to_string(path).unwrap()
}

fn frame(f: usize) -> FrameCtx {
    FrameCtx {
        t: f as f64 / 60.0,
        dt: 1.0 / 60.0,
        frame_count: f as f64,
        width: 640.0,
        height: 480.0,
        ..FrameCtx::default()
    }
}

/// Everything the patch computed over a few frames, as text.
fn run(patch: &PatchFile) -> String {
    let (mut engine, _) = Engine::from_patch(patch).unwrap();
    let mut out = String::new();
    for f in 0..20 {
        let r = engine.tick(&frame(f));
        out = format!("{:?}", r.outputs);
    }
    out
}

/// Samples built on every kind of seeded node: random / noise values,
/// spreads (noise, shuffle, Game of Life), forces, walkers and LFOs;
/// geometry (scatter, L-system jitter), simulations and the Noise node.
const SEEDED: &[&str] = &[
    "tut-17-perlin",
    "tut-18-random-spread",
    "tut-35-wanderer",
    "tut-36-gaussian",
    "tut-43-gol",
    "noise-wave",
    "curl-flow",
    "force-field",
    "variations",
    "voronoi-mosaic",
    "lsystem-plant",
    "sim-flocking",
    "sim-dla-coral",
    "noise-cellular-warp",
];

#[test]
fn seed_zero_draws_what_the_patch_always_drew() {
    for id in SEEDED {
        let text = sample(id);
        let mut plain = floret_engine::load_patch(&text).unwrap();
        plain.seed = 0;
        // The same file as written before patch seeds existed.
        let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("seed");
        obj.remove("presets");
        let old = floret_engine::load_patch(&json.to_string()).unwrap();
        assert_eq!(old.seed, 0);
        assert_eq!(run(&plain), run(&old), "{id}");
    }
}

#[test]
fn other_seeds_draw_something_else() {
    for id in SEEDED {
        let mut p = floret_engine::load_patch(&sample(id)).unwrap();
        p.seed = 0;
        let base = run(&p);
        p.seed = 4242;
        let a = run(&p);
        assert_ne!(base, a, "{id}: seed 4242 drew the same as seed 0");
        p.seed = 4243;
        assert_ne!(a, run(&p), "{id}: neighbouring seeds drew the same");
        // Deterministic: the same seed draws the same again.
        p.seed = 4242;
        assert_eq!(a, run(&p), "{id}");
    }
}

#[test]
fn set_patch_seed_forgets_memoised_outputs() {
    let p = floret_engine::load_patch(&sample("tut-36-gaussian")).unwrap();
    let (mut engine, _) = Engine::from_patch(&p).unwrap();
    let before = format!("{:?}", engine.tick(&frame(0)).outputs);
    engine.set_patch_seed(99);
    assert_eq!(engine.patch_seed(), 99);
    let after = format!("{:?}", engine.tick(&frame(0)).outputs);
    assert_ne!(before, after);
}

#[test]
fn files_without_seed_or_presets_are_written_as_before() {
    for id in ["tut-17-perlin", "grid-ripple"] {
        let text = sample(id);
        let p = PatchFile::load(&text).unwrap();
        assert_eq!(p.seed, 0);
        assert!(p.presets.is_empty());
        let json: serde_json::Value = serde_json::from_str(&p.to_json()).unwrap();
        let obj = json.as_object().unwrap();
        assert!(
            !obj.contains_key("seed") && !obj.contains_key("presets"),
            "{id}"
        );
        // Exactly the original keys, in the original order (plus the
        // migration's `legacySlot0` for a file saved before v4).
        let original: serde_json::Value = serde_json::from_str(&text).unwrap();
        let keys = |v: &serde_json::Value| {
            v.as_object()
                .unwrap()
                .keys()
                .filter(|k| *k != "legacySlot0")
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&json), keys(&original), "{id}");
    }
}

#[test]
fn seed_and_presets_round_trip() {
    let cat = Catalogue::builtin();
    let mut p = floret_engine::load_patch(&sample("tut-36-gaussian")).unwrap();
    p.seed = 31337;
    let a = preset::capture(cat, &p, "Calm");
    p.seed = 7;
    let b = preset::capture(cat, &p, "Wild");
    p.presets = vec![a, b];
    let text = p.to_json();
    let back = PatchFile::load(&text).unwrap();
    assert_eq!(back.seed, 7);
    assert_eq!(back.presets, p.presets);
    assert_eq!(back.presets[0].seed, 31337);
    assert!(!back.presets[0].values.is_empty());
    assert_eq!(back.to_json(), text);
}

#[test]
fn the_variations_sample_has_presets_to_morph() {
    let p = floret_engine::load_patch(&sample("variations")).unwrap();
    assert!(p.presets.len() >= 2);
    let cat = Catalogue::builtin();
    let (a, b) = (&p.presets[0], &p.presets[1]);
    // Halfway between them, something numeric differs from both ends.
    let (mid, _) = preset::morph(cat, &p, a, b, 0.5);
    let (start, _) = preset::morph(cat, &p, a, b, 0.0);
    let (end, _) = preset::morph(cat, &p, a, b, 1.0);
    assert!(mid
        .iter()
        .zip(&start)
        .zip(&end)
        .any(|((m, s), e)| m.value != s.value && m.value != e.value));
}
