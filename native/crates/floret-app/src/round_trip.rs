//! Saving and loading through the real files (#82), for the beta:
//!
//! - every Library sample written with [`files::save`] and read back with
//!   [`files::read`] into a temporary folder: the same patch, the same
//!   compile warnings, and a second save that is the same file;
//! - a patch that uses what the samples don't (Sections, bypassed nodes,
//!   `legacy_slot0`, named Read / Write pairs, region and subpatch sizes,
//!   presets, the seed, the loop length, comments) saved and read back;
//! - fields from a newer build, kept on re-save;
//! - picked files (videos, fonts, LUTs …) saved relative to the patch when
//!   they sit beside it or below it, resolved on open, and rewritten by
//!   Save As.

use crate::editor::test_util::{edge, node, patch};
use crate::editor::{ops, pairs, regions, subpatch, wrap};
use crate::files::{self, Document, Origin};
use crate::sample_sweep::{content, same, warnings};
use crate::session::{self, How};
use crate::SAMPLES;
use floret_engine::patch::{PatchFile, Position};
use floret_engine::spec::Catalogue;
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("floret-rt-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A saved file without the line a save stamps with the time, so two
/// saves of the same patch compare equal.
fn stable(text: &str) -> String {
    text.lines()
        .filter(|l| !l.contains("\"modifiedAt\""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn json_of(path: &Path) -> Json {
    serde_json::from_str(&text(path)).unwrap()
}

/// Save, read back, and check the patch is the same, compiles the same and
/// saves to the same file. Returns what was read and a description of
/// anything that differed.
fn round_trip(p: &PatchFile, path: &Path) -> (PatchFile, Vec<String>) {
    let mut bad = Vec::new();
    files::save(p, path).unwrap();
    let first = text(path);
    assert!(
        !path.with_extension("flora.tmp").exists(),
        "no temporary file is left behind"
    );
    let back = match files::read(path) {
        Ok(b) => b,
        Err(e) => return (p.clone(), vec![format!("doesn't read back: {e}")]),
    };
    if !same(&content(p), &content(&back)) {
        bad.push("reads back as a different patch".to_owned());
    }
    match (warnings(p), warnings(&back)) {
        (Ok(a), Ok(b)) if a == b => {}
        (Ok(a), Ok(b)) => bad.push(format!("compiles differently: {a:?} → {b:?}")),
        (a, b) => bad.push(format!("doesn't compile: {:?} / {:?}", a.err(), b.err())),
    }
    files::save(&back, path).unwrap();
    if stable(&text(path)) != stable(&first) {
        bad.push("a second save is a different file".to_owned());
    }
    (back, bad)
}

#[test]
fn every_sample_round_trips_through_a_file_on_disk() {
    let dir = tmp("samples");
    let mut failures = Vec::new();
    for s in SAMPLES {
        let patch = match floret_engine::load_patch(s.flora) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: doesn't load: {e}", s.id));
                continue;
            }
        };
        let path = dir.join(format!("{}.flora", s.id));
        let (_, bad) = round_trip(&patch, &path);
        failures.extend(bad.into_iter().map(|b| format!("{}: {b}", s.id)));
        // The file on disk is the patch in the sample's own words.
        let saved = json_of(&path);
        let orig: Json = serde_json::from_str(s.flora).unwrap();
        if saved["nodes"].as_array().map(Vec::len) != orig["nodes"].as_array().map(Vec::len)
            || saved["edges"].as_array().map(Vec::len) != orig["edges"].as_array().map(Vec::len)
        {
            failures.push(format!("{}: nodes or wires lost on the way", s.id));
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Samples hold no references to files on a disk, so saving them never has
/// a path to rewrite: they write exactly what they always did.
#[test]
fn samples_name_no_local_files() {
    let mut found = Vec::new();
    for s in SAMPLES {
        let patch = floret_engine::load_patch(s.flora).unwrap();
        let mut seen = Vec::new();
        let mut probe = patch.clone();
        files::map_file_refs(&mut probe, |r| {
            seen.push(r.to_owned());
            r.to_owned()
        });
        for r in seen {
            for t in r.lines().map(str::trim) {
                // Remote and embedded sources are not files on this disk.
                if !t.is_empty() && files::is_local_ref(t) {
                    found.push(format!("{}: {t}", s.id));
                }
            }
        }
        let saved = files::to_flora(&patch, None, "2026-10-01T00:00:00.000Z");
        let mut rebased = patch.clone();
        files::resolve_paths(&mut rebased, Some(Path::new("/some/where")));
        assert_eq!(
            files::to_flora(&rebased, None, "2026-10-01T00:00:00.000Z"),
            saved,
            "{}: opening from a folder changes the patch",
            s.id
        );
    }
    assert!(
        found.is_empty(),
        "samples reference local files:\n{}",
        found.join("\n")
    );
}

// ── A patch with everything the samples leave out ───────────────────────────

/// Built with the editor's own operations: a Section holding a named
/// Write → Read pair, a ForEach with ports, a subpatch holding a spread
/// pair, a bypassed effect, a comment, presets, a seed and a loop length.
fn kitchen_sink() -> PatchFile {
    let cat = Catalogue::builtin();
    let at = |id: &str, ty: &str, x: f64, y: f64| {
        let mut n = node(id, ty);
        n.position = Position { x, y };
        n
    };
    let with = |mut n: floret_engine::patch::PatchNode, k: &str, v: Json| {
        n.params.insert(k.into(), v);
        n
    };
    let nodes = vec![
        with(
            with(at("rd", "state/read", 0.0, 0.0), "slot", json!(1)),
            "name",
            json!("ball pos"),
        ),
        with(at("num", "value/number", 0.0, 120.0), "value", json!(3.5)),
        at("add", "math/add", 240.0, 0.0),
        with(
            with(at("wr", "state/write", 480.0, 0.0), "slot", json!(1)),
            "name",
            json!("ball pos"),
        ),
        with(
            with(at("rds", "state/read-spread", 0.0, 300.0), "slot", json!(2)),
            "name",
            json!("trail ✿"),
        ),
        with(
            with(
                at("wrs", "state/write-spread", 480.0, 300.0),
                "slot",
                json!(2),
            ),
            "name",
            json!("trail ✿"),
        ),
        at("grid", "spread/grid", 0.0, 520.0),
        at("rect", "draw/rect", 240.0, 520.0),
        at("fx", "gpu/effect", 520.0, 520.0),
        at("out", "render/renderer", 800.0, 520.0),
        with(
            with(
                at("note", "meta/comment", 0.0, -160.0),
                "text",
                json!("Hello\nWörld ☃ \"quoted\""),
            ),
            "width",
            json!(240),
        ),
    ];
    let edges = vec![
        edge("rd", "out-current", "add", "in-a"),
        edge("num", "out-number", "add", "in-b"),
        edge("add", "out-result", "wr", "in-next"),
        edge("rds", "out-current", "wrs", "in-next"),
        edge("grid", "out-positions", "rect", "in-pos"),
        edge("rect", "out-shape", "fx", "in-input"),
        edge("fx", "out-out", "out", "in-layers"),
    ];
    let mut p = patch(nodes, edges);
    let size = |_: &floret_engine::patch::PatchNode| (180.0, 80.0);
    let ids = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    // A Section round the first pair, and a ForEach round the grid.
    let section = wrap::wrap(
        &mut p,
        cat,
        &ids(&["rd", "num", "add", "wr"]),
        wrap::SECTION,
        &size,
    )
    .expect("section");
    regions::set_size(
        p.nodes.iter_mut().find(|n| n.id == section).unwrap(),
        733.0,
        311.5,
    );
    p.nodes
        .iter_mut()
        .find(|n| n.id == section)
        .unwrap()
        .params
        .insert("label".into(), "Physics — ball".into());
    let each = wrap::wrap(
        &mut p,
        cat,
        &ids(&["grid", "rect"]),
        "region/foreach",
        &size,
    )
    .expect("foreach");
    regions::set_size(
        p.nodes.iter_mut().find(|n| n.id == each).unwrap(),
        523.0,
        297.25,
    );
    // The spread pair folded into a subpatch.
    let sub = subpatch::group(&mut p, cat, &ids(&["rds", "wrs"])).expect("subpatch");
    assert!(p.nodes.iter().any(|n| n.id == sub));
    // A bypassed effect.
    assert!(ops::toggle_bypass(&mut p, cat, &ids(&["fx"])));
    p.seed = 77;
    p.loop_length = Some(2.5);
    p.legacy_slot0 = true;
    p.meta = Some(floret_engine::patch::PatchMeta {
        author: Some("Tester ✿".into()),
        ..Default::default()
    });
    let preset = floret_engine::preset::capture(cat, &p, "Warm");
    p.presets.push(preset);
    p.presets
        .push(floret_engine::preset::capture(cat, &p, "Cool — 2"));
    p
}

#[test]
fn the_fixture_is_what_it_claims() {
    let p = kitchen_sink();
    let cat = Catalogue::builtin();
    let types = |t: &str| p.nodes.iter().filter(|n| n.def_type == t).count();
    assert_eq!(types("meta/section"), 1);
    assert_eq!(types("region/foreach"), 1);
    assert_eq!(types("meta/subpatch"), 1);
    assert_eq!(types("meta/comment"), 1);
    assert!(p.nodes.iter().any(|n| n.bypassed));
    assert!(p.legacy_slot0 && p.seed == 77 && p.loop_length == Some(2.5));
    assert_eq!(p.presets.len(), 2);
    // Both pairs are whole and named.
    assert_eq!(pairs::group(&p, "rd").len(), 2);
    assert_eq!(pairs::group(&p, "rds").len(), 2);
    // The flow rule holds, and it compiles.
    assert!(
        crate::editor::connect::backward_wires(&p, cat).is_empty(),
        "the fixture wires only to the right"
    );
    assert!(floret_engine::Engine::from_patch(&p).is_ok());
}

#[test]
fn a_patch_with_everything_round_trips_through_a_file() {
    let dir = tmp("sink");
    let p = kitchen_sink();
    let path = dir.join("Kitchen sink — ✿.flora");
    let (back, bad) = round_trip(&p, &path);
    assert!(bad.is_empty(), "{bad:?}");
    // The details a same-JSON check could only hide behind its own type.
    let file = json_of(&path);
    assert_eq!(file["version"], floret_engine::patch::CURRENT_PATCH_VERSION);
    assert_eq!(file["seed"], 77);
    assert_eq!(file["loopLength"], 2.5);
    assert_eq!(file["legacySlot0"], true);
    assert_eq!(file["meta"]["name"], "Kitchen sink — ✿");
    assert_eq!(file["meta"]["author"], "Tester ✿");
    assert_eq!(file["presets"].as_array().unwrap().len(), 2);
    assert_eq!(file["presets"][1]["name"], "Cool — 2");
    let node = |id: &str| back.nodes.iter().find(|n| n.id == id).unwrap();
    let section = back
        .nodes
        .iter()
        .find(|n| n.def_type == "meta/section")
        .unwrap();
    assert_eq!((section.width, section.height), (Some(733.0), Some(311.5)));
    assert_eq!(section.params["label"], "Physics — ball");
    assert_eq!(section.params["width"], 733);
    let each = back
        .nodes
        .iter()
        .find(|n| n.def_type == "region/foreach")
        .unwrap();
    assert_eq!((each.width, each.height), (Some(523.0), Some(297.25)));
    assert!(
        each.ports.as_ref().is_some_and(|p| !p.is_empty()),
        "frame ports"
    );
    let sub = back
        .nodes
        .iter()
        .find(|n| n.def_type == "meta/subpatch")
        .unwrap();
    assert_eq!(sub.parent_id, None);
    assert_eq!(
        back.nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(&sub.id))
            .count(),
        2,
        "the subpatch's contents"
    );
    assert!(node("fx").bypassed);
    assert!(!node("rect").bypassed);
    assert_eq!(node("rd").parent_id.as_deref(), Some(section.id.as_str()));
    // Named pairs: both halves, one slot, one name.
    assert_eq!(pairs::name(node("rd")), Some("ball pos"));
    assert_eq!(pairs::name(node("wr")), Some("ball pos"));
    assert_eq!(pairs::partners(&back, "wr"), ["rd"]);
    assert_eq!(node("rd").params["slot"], node("wr").params["slot"]);
    assert_eq!(pairs::name(node("rds")), Some("trail ✿"));
    assert_eq!(pairs::partners(&back, "rds"), ["wrs"]);
    assert_eq!(node("note").params["text"], "Hello\nWörld ☃ \"quoted\"");
    // Presets keep their values.
    assert!(same(
        &serde_json::to_value(&back.presets).unwrap(),
        &serde_json::to_value(&p.presets).unwrap()
    ));
    assert_eq!(back.seed, 77);
    assert_eq!(back.loop_seconds(), 2.5);
    assert!(back.legacy_slot0);
    let _ = std::fs::remove_dir_all(dir);
}

/// A file saved before scalar nodes ran per element: opening it, saving it
/// and opening it again keeps it reading slot 0, so it computes what it did.
#[test]
fn an_older_file_keeps_legacy_slot0_through_a_save() {
    let dir = tmp("legacy");
    let old = r#"{"kind":"floret.patch","version":3,"nodes":[
        {"id":"a","defType":"value/number","position":{"x":0,"y":0},"params":{"value":2}}],
        "edges":[]}"#;
    let path = dir.join("old.flora");
    std::fs::write(&path, old).unwrap();
    let opened = files::read(&path).unwrap();
    assert!(opened.legacy_slot0);
    files::save(&opened, &path).unwrap();
    let file = json_of(&path);
    assert_eq!(file["version"], floret_engine::patch::CURRENT_PATCH_VERSION);
    assert_eq!(file["legacySlot0"], true);
    let again = files::read(&path).unwrap();
    assert!(again.legacy_slot0);
    let (_, bad) = round_trip(&again, &path);
    assert!(bad.is_empty(), "{bad:?}");
    let _ = std::fs::remove_dir_all(dir);
}

// ── Fields from a newer build ───────────────────────────────────────────────

#[test]
fn fields_this_build_does_not_know_survive_a_save_to_disk() {
    let dir = tmp("extra");
    let path = dir.join("future.flora");
    let future = json!({
        "kind": "floret.patch", "version": 4,
        "appVersion": "9.9.9",
        "meta": { "name": "future", "createdAt": "2030-01-01T00:00:00.000Z", "palette": ["#fff"] },
        "nodes": [
            { "id": "a", "defType": "value/number", "position": { "x": 5, "y": 6 },
              "params": { "value": 1 }, "locked": true, "colour": { "r": 1, "g": 0.5 } },
            { "id": "b", "defType": "value/number", "position": { "x": 5, "y": 90 }, "params": {} }
        ],
        "edges": [],
        "viewport": { "x": 10, "y": -4, "zoom": 0.75 },
        "notes": ["one", "two"]
    });
    std::fs::write(&path, serde_json::to_string_pretty(&future).unwrap()).unwrap();
    let opened = files::read(&path).unwrap();
    files::save(&opened, &path).unwrap();
    let saved = json_of(&path);
    assert_eq!(saved["viewport"], future["viewport"]);
    assert_eq!(saved["notes"], future["notes"]);
    assert_eq!(saved["meta"]["palette"], future["meta"]["palette"]);
    assert_eq!(saved["nodes"][0]["locked"], true);
    assert_eq!(saved["nodes"][0]["colour"], future["nodes"][0]["colour"]);
    assert!(saved["nodes"][1].get("locked").is_none());
    // The ordinary fields are where they always are, not repeated.
    assert_eq!(saved["nodes"][0]["position"]["y"], 6);
    assert_eq!(saved["meta"]["createdAt"], "2030-01-01T00:00:00.000Z");
    // And the same on every save after.
    let (_, bad) = round_trip(&opened, &path);
    assert!(bad.is_empty(), "{bad:?}");
    let _ = std::fs::remove_dir_all(dir);
}

// ── Files a patch refers to ─────────────────────────────────────────────────

/// A folder with a patch's media in it: a video beside it, a font and a LUT
/// in sub-folders, a CSV, and a picture.
struct Project {
    dir: PathBuf,
}

impl Project {
    fn new(name: &str) -> Project {
        let dir = tmp(name);
        let p = Project { dir };
        for f in [
            "clip.mp4",
            "fonts/Face.ttf",
            "luts/warm.cube",
            "data/d.csv",
            "pic.png",
        ] {
            let path = p.dir.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, f).unwrap();
        }
        p
    }

    fn abs(&self, rel: &str) -> String {
        self.dir.join(rel).to_string_lossy().into_owned()
    }

    /// A patch whose nodes name those files by full path, plus two files
    /// that aren't in the folder (`elsewhere`) and sources that aren't
    /// files at all.
    fn patch(&self, elsewhere: &Path) -> PatchFile {
        let with = |id: &str, ty: &str, key: &str, v: Json| {
            let mut n = node(id, ty);
            n.params.insert(key.into(), v);
            n
        };
        let mut lut = node("lut", "gpu/effect");
        lut.params.insert("mode".into(), "lut".into());
        lut.port_values = Some(
            json!({ "lut": self.abs("luts/warm.cube"), "amount": 1 })
                .as_object()
                .unwrap()
                .clone(),
        );
        let mut seq = node("seq", "video/frame-at");
        seq.params.insert(
            "url".into(),
            format!("{}\n  {}\n", self.abs("clip.mp4"), self.abs("data/d.csv")).into(),
        );
        patch(
            vec![
                with("vid", "video/file", "url", self.abs("clip.mp4").into()),
                with(
                    "font",
                    "text/font",
                    "file",
                    self.abs("fonts/Face.ttf").into(),
                ),
                lut,
                with("csv", "data/csv-file", "url", self.abs("data/d.csv").into()),
                with("img", "io/image-url", "url", self.abs("pic.png").into()),
                with(
                    "far",
                    "video/file",
                    "url",
                    elsewhere
                        .join("far.mp4")
                        .to_string_lossy()
                        .into_owned()
                        .into(),
                ),
                with(
                    "web",
                    "io/image-url",
                    "url",
                    "https://example.com/a.png".into(),
                ),
                with(
                    "emb",
                    "video/file",
                    "url",
                    "data:video/mp4;base64,AAAA".into(),
                ),
                with("fileurl", "video/file", "url", "file:///opt/x/y.mp4".into()),
                seq,
            ],
            vec![],
        )
    }
}

fn param<'a>(file: &'a Json, node: &str, key: &str) -> &'a Json {
    let n = file["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == node)
        .unwrap();
    &n["params"][key]
}

fn value<'a>(p: &'a PatchFile, node: &str, key: &str) -> &'a str {
    let n = p.nodes.iter().find(|n| n.id == node).unwrap();
    n.params
        .get(key)
        .or_else(|| n.port_values.as_ref().and_then(|v| v.get(key)))
        .and_then(Json::as_str)
        .unwrap()
}

#[test]
fn files_beside_or_below_the_patch_are_saved_relative() {
    let proj = Project::new("rel");
    let elsewhere = tmp("far-rel");
    let p = proj.patch(&elsewhere);
    let path = proj.dir.join("show.flora");
    files::save(&p, &path).unwrap();
    let f = json_of(&path);
    assert_eq!(param(&f, "vid", "url"), "clip.mp4");
    assert_eq!(param(&f, "font", "file"), "fonts/Face.ttf");
    assert_eq!(param(&f, "csv", "url"), "data/d.csv");
    assert_eq!(param(&f, "img", "url"), "pic.png");
    assert_eq!(
        f["nodes"][2]["portValues"]["lut"], "luts/warm.cube",
        "pin constants too"
    );
    // A list of frames: each line on its own, spacing kept.
    assert_eq!(param(&f, "seq", "url"), "clip.mp4\n  data/d.csv\n");
    // Not beside the patch: the full path, as before. Not files: untouched.
    assert_eq!(
        param(&f, "far", "url"),
        &json!(elsewhere.join("far.mp4").to_string_lossy())
    );
    assert_eq!(param(&f, "web", "url"), "https://example.com/a.png");
    assert_eq!(param(&f, "emb", "url"), "data:video/mp4;base64,AAAA");
    assert_eq!(param(&f, "fileurl", "url"), "file:///opt/x/y.mp4");
    assert!(
        !text(&path).contains(proj.dir.to_string_lossy().as_ref()),
        "nothing of this machine's folders is in the file"
    );
    // Reading it back gives the full paths again, and the app's copy of
    // the patch is the one it had.
    let back = files::read(&path).unwrap();
    assert_eq!(value(&back, "vid", "url"), proj.abs("clip.mp4"));
    assert_eq!(value(&back, "font", "file"), proj.abs("fonts/Face.ttf"));
    assert_eq!(value(&back, "lut", "lut"), proj.abs("luts/warm.cube"));
    assert_eq!(
        value(&back, "seq", "url"),
        format!("{}\n  {}\n", proj.abs("clip.mp4"), proj.abs("data/d.csv"))
    );
    assert_eq!(
        value(&back, "far", "url"),
        elsewhere.join("far.mp4").to_string_lossy()
    );
    assert_eq!(value(&back, "web", "url"), "https://example.com/a.png");
    assert!(same(&content(&p), &content(&back)));
    // The patch in memory is not rewritten by saving it.
    assert_eq!(value(&p, "vid", "url"), proj.abs("clip.mp4"));
    let (_, bad) = round_trip(&back, &path);
    assert!(bad.is_empty(), "{bad:?}");
    let _ = std::fs::remove_dir_all(proj.dir);
    let _ = std::fs::remove_dir_all(elsewhere);
}

#[test]
fn a_patch_moved_with_its_media_still_finds_it() {
    let proj = Project::new("move-a");
    let elsewhere = tmp("far-move");
    let path = proj.dir.join("show.flora");
    files::save(&proj.patch(&elsewhere), &path).unwrap();
    // The whole folder goes somewhere else (another Mac, a zip).
    let moved = tmp("move-b");
    for f in [
        "show.flora",
        "clip.mp4",
        "fonts/Face.ttf",
        "luts/warm.cube",
        "data/d.csv",
        "pic.png",
    ] {
        let to = moved.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(proj.dir.join(f), to).unwrap();
    }
    std::fs::remove_dir_all(&proj.dir).unwrap();
    let opened = files::read(&moved.join("show.flora")).unwrap();
    for (node, key, rel) in [
        ("vid", "url", "clip.mp4"),
        ("font", "file", "fonts/Face.ttf"),
        ("lut", "lut", "luts/warm.cube"),
        ("csv", "url", "data/d.csv"),
        ("img", "url", "pic.png"),
    ] {
        let at = value(&opened, node, key);
        assert_eq!(at, moved.join(rel).to_string_lossy(), "{node}");
        assert!(Path::new(at).exists(), "{node}: {at} is there");
    }
    // The engine reads it from there.
    let bytes = floret_engine::image::read_source(value(&opened, "img", "url"), None).unwrap();
    assert_eq!(bytes, b"pic.png");
    // Saved again in the new place it stays relative, byte for byte.
    let before = stable(&text(&moved.join("show.flora")));
    files::save(&opened, &moved.join("show.flora")).unwrap();
    assert_eq!(stable(&text(&moved.join("show.flora"))), before);
    let _ = std::fs::remove_dir_all(moved);
    let _ = std::fs::remove_dir_all(elsewhere);
}

#[test]
fn absolute_paths_in_a_file_still_load_as_they_are() {
    let dir = tmp("abs");
    let path = dir.join("old.flora");
    let abs = dir.join("m.mp4").to_string_lossy().into_owned();
    let file = json!({
        "kind": "floret.patch", "version": 4,
        "nodes": [
            { "id": "a", "defType": "video/file", "position": {"x":0,"y":0}, "params": { "url": abs } },
            { "id": "b", "defType": "video/file", "position": {"x":0,"y":0}, "params": { "url": "sub/b.mp4" } },
            { "id": "c", "defType": "video/file", "position": {"x":0,"y":0}, "params": { "url": "C:\\media\\c.mp4" } },
            { "id": "d", "defType": "video/file", "position": {"x":0,"y":0}, "params": { "url": "\\\\server\\share\\d.mp4" } },
            { "id": "e", "defType": "video/file", "position": {"x":0,"y":0}, "params": { "url": "D:/media/e.mp4" } },
            { "id": "f", "defType": "text/font", "position": {"x":0,"y":0}, "params": { "file": "./f.ttf" } },
            { "id": "g", "defType": "video/file", "position": {"x":0,"y":0}, "params": { "url": "" } }
        ],
        "edges": []
    });
    std::fs::write(&path, file.to_string()).unwrap();
    let p = files::read(&path).unwrap();
    assert_eq!(value(&p, "a", "url"), abs, "absolute: as it was");
    assert_eq!(
        value(&p, "b", "url"),
        dir.join("sub").join("b.mp4").to_string_lossy()
    );
    // Windows paths opened on any system are left alone (not joined on).
    assert_eq!(value(&p, "c", "url"), "C:\\media\\c.mp4");
    assert_eq!(value(&p, "d", "url"), "\\\\server\\share\\d.mp4");
    assert_eq!(value(&p, "e", "url"), "D:/media/e.mp4");
    assert_eq!(value(&p, "f", "file"), dir.join("f.ttf").to_string_lossy());
    assert_eq!(value(&p, "g", "url"), "");
    // Saved back: the absolute ones in this folder become relative, the
    // foreign ones stay as they were.
    files::save(&p, &path).unwrap();
    let f = json_of(&path);
    assert_eq!(param(&f, "a", "url"), "m.mp4");
    assert_eq!(param(&f, "b", "url"), "sub/b.mp4");
    assert_eq!(param(&f, "c", "url"), "C:\\media\\c.mp4");
    assert_eq!(param(&f, "d", "url"), "\\\\server\\share\\d.mp4");
    assert_eq!(param(&f, "e", "url"), "D:/media/e.mp4");
    assert_eq!(param(&f, "f", "file"), "f.ttf");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn save_as_to_another_folder_keeps_every_reference_pointing_at_the_same_file() {
    let proj = Project::new("saveas");
    let elsewhere = tmp("far-saveas");
    let a = proj.dir.join("show.flora");
    files::save(&proj.patch(&elsewhere), &a).unwrap();
    // Open it (paths become full), then Save As in a folder beside the
    // media, one with the media below it, and one unrelated.
    let mut opened = files::read(&a).unwrap();
    let mut doc = Document::file(a.clone(), 1);
    let resolved = |p: &PatchFile| -> Vec<(&'static str, &'static str, String)> {
        [
            ("vid", "url"),
            ("font", "file"),
            ("lut", "lut"),
            ("csv", "url"),
            ("img", "url"),
            ("far", "url"),
        ]
        .into_iter()
        .map(|(n, k)| (n, k, value(p, n, k).to_owned()))
        .collect()
    };
    let before = resolved(&opened);

    // 1. Into a sub-folder of the project: the media is above it, so the
    // full paths are written; they still reach the files.
    let sub = proj.dir.join("copies");
    let b = sub.join("copy.flora");
    let out = session::save(&mut doc, &mut opened, 2, How::SaveAs, |_| Some(b.clone()));
    assert!(out.ok(), "{out:?}");
    assert_eq!(doc.origin, Origin::File(b.clone()));
    let f = json_of(&b);
    assert_eq!(param(&f, "vid", "url"), &json!(proj.abs("clip.mp4")));
    assert_eq!(param(&f, "csv", "url"), &json!(proj.abs("data/d.csv")));
    assert_eq!(resolved(&files::read(&b).unwrap()), before);

    // 2. Into a folder that has some of the media below it: those are
    // relative from there, the rest full.
    let fonts = proj.dir.join("fonts");
    let c = fonts.join("fontcopy.flora");
    let out = session::save(&mut doc, &mut opened, 3, How::SaveAs, |_| Some(c.clone()));
    assert!(out.ok());
    let f = json_of(&c);
    assert_eq!(param(&f, "font", "file"), "Face.ttf");
    assert_eq!(param(&f, "vid", "url"), &json!(proj.abs("clip.mp4")));
    assert_eq!(resolved(&files::read(&c).unwrap()), before);

    // 3. Into the project folder again: back to beside.
    let d = proj.dir.join("again.flora");
    let out = session::save(&mut doc, &mut opened, 4, How::SaveAs, |_| Some(d.clone()));
    assert!(out.ok());
    let f = json_of(&d);
    assert_eq!(param(&f, "vid", "url"), "clip.mp4");
    assert_eq!(param(&f, "font", "file"), "fonts/Face.ttf");
    assert_eq!(resolved(&files::read(&d).unwrap()), before);

    // 4. A folder with none of it: full paths, still the same files.
    let lone = tmp("saveas-lone");
    let e = lone.join("lone.flora");
    let out = session::save(&mut doc, &mut opened, 5, How::SaveAs, |_| Some(e.clone()));
    assert!(out.ok());
    assert_eq!(resolved(&files::read(&e).unwrap()), before);
    for (_, _, at) in &before[..5] {
        assert!(Path::new(at).exists(), "{at}");
    }
    let _ = std::fs::remove_dir_all(proj.dir);
    let _ = std::fs::remove_dir_all(elsewhere);
    let _ = std::fs::remove_dir_all(lone);
}

/// A path typed relative into a field (nothing makes the editor absolute
/// it) still reaches the same file when the patch is saved somewhere else.
#[test]
fn a_relative_path_typed_into_a_patch_follows_save_as() {
    let proj = Project::new("typed");
    let a = proj.dir.join("show.flora");
    let mut typed = node("vid", "video/file");
    typed.params.insert("url".into(), "clip.mp4".into());
    let mut p = patch(vec![typed], vec![]);
    files::save(&p, &a).unwrap();
    assert_eq!(
        param(&json_of(&a), "vid", "url"),
        "clip.mp4",
        "in place: as typed"
    );
    let mut doc = Document::file(a.clone(), 1);
    let other = tmp("typed-other");
    let b = other.join("b.flora");
    let out = session::save(&mut doc, &mut p, 2, How::SaveAs, |_| Some(b.clone()));
    assert!(out.ok());
    assert_eq!(
        param(&json_of(&b), "vid", "url"),
        &json!(proj.abs("clip.mp4"))
    );
    // Saved untitled → no folder to anchor it: left as typed.
    let mut fresh = Document::untitled(1);
    let c = other.join("c.flora");
    let mut q = patch(
        vec![{
            let mut n = node("vid", "video/file");
            n.params.insert("url".into(), "clip.mp4".into());
            n
        }],
        vec![],
    );
    assert!(session::save(&mut fresh, &mut q, 2, How::Save, |_| Some(c.clone())).ok());
    assert_eq!(param(&json_of(&c), "vid", "url"), "clip.mp4");
    let _ = std::fs::remove_dir_all(proj.dir);
    let _ = std::fs::remove_dir_all(other);
}

#[test]
fn a_missing_file_opens_and_warns_as_before() {
    let dir = tmp("missing");
    let path = dir.join("p.flora");
    let mut n = node("img", "io/image-url");
    n.params.insert("url".into(), "gone/pic.png".into());
    let mut v = node("vid", "video/file");
    v.params.insert("url".into(), "gone.mp4".into());
    let file = patch(vec![n, v], vec![]);
    std::fs::write(&path, file.to_json()).unwrap();
    // It opens; the path is the full one the file would be at.
    let p = files::read(&path).unwrap();
    let at = value(&p, "img", "url").to_owned();
    assert_eq!(at, dir.join("gone").join("pic.png").to_string_lossy());
    assert!(!Path::new(&at).exists());
    // Loading it reports the full path and why, as a full path always did.
    let err = floret_engine::image::read_source(&at, None).unwrap_err();
    assert!(
        err.contains("file not found") && err.contains("pic.png"),
        "{err}"
    );
    // It compiles the same as with the path typed in full.
    let (engine, w) = floret_engine::Engine::from_patch(&p).unwrap();
    assert!(w.is_empty());
    assert_eq!(engine.graph.nodes.len(), 2);
    // And saving it again keeps the reference (relative, though missing).
    files::save(&p, &path).unwrap();
    let f = json_of(&path);
    assert_eq!(param(&f, "img", "url"), "gone/pic.png");
    assert_eq!(param(&f, "vid", "url"), "gone.mp4");
    let _ = std::fs::remove_dir_all(dir);
}

/// Every pin that takes a path is in the table that rewrites them: if a
/// node gains one, this says so.
#[test]
fn every_node_input_that_names_a_file_is_handled() {
    use floret_engine::spec::{NodeSpec, Params, PortType};
    let cat = Catalogue::builtin();
    let pathish = [
        "url", "file", "path", "lut", "src", "filename", "folder", "dir",
    ];
    // Names that look like paths but aren't read from disk: `filename` is
    // the label of a file embedded in the patch.
    let not_files: &[(&str, &str)] = &[("io/file", "filename"), ("io/svg", "filename")];
    // The pins a node can show: its own, and those of each `mode`.
    let pins = |spec: &NodeSpec| {
        let mut all = cat.resolved_inputs(spec, &Params::new());
        for mode in spec.param_options.get("mode").into_iter().flatten() {
            let mut params = Params::new();
            params.insert("mode".into(), mode.clone().into());
            all.extend(cat.resolved_inputs(spec, &params));
        }
        all
    };
    let mut unhandled = Vec::new();
    for spec in cat.iter() {
        let mut keys: Vec<String> = pins(spec)
            .iter()
            .filter(|p| p.ty == PortType::String)
            .map(|p| p.param.clone().unwrap_or_else(|| p.name.clone()))
            .collect();
        keys.extend(spec.default_params.keys().cloned());
        keys.sort();
        keys.dedup();
        for k in keys {
            if !pathish.contains(&k.as_str()) {
                continue;
            }
            let key = (spec.ty.as_str(), k.as_str());
            let handled = files::FILE_REFS.contains(&key) || not_files.contains(&key);
            if !handled {
                unhandled.push(format!("{}.{k}", spec.ty));
            }
        }
    }
    assert!(
        unhandled.is_empty(),
        "path-like pins missing from files::FILE_REFS (or the exceptions here): {unhandled:?}"
    );
    // And the table names things that exist.
    for (ty, key) in files::FILE_REFS {
        let spec = cat.get(ty).unwrap_or_else(|| panic!("{ty} is not a node"));
        let has = pins(spec)
            .iter()
            .any(|p| p.name == *key || p.param.as_deref() == Some(key))
            || spec.default_params.contains_key(*key);
        assert!(has, "{ty} has no `{key}`");
    }
}
