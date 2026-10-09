//! The built-in samples on the canvas: every one opens fitted and centred
//! in a laptop and a desktop window, clear of the minimap, and its layout
//! is tidy. `relayout_samples` (ignored) re-arranges `native/samples/` in
//! place with the auto-arrange.

use super::*;
use crate::SAMPLES;
use serde_json::json;
use std::path::PathBuf;

/// A 1440×900 and a 1920×1080 window: the canvas left of the 400-wide
/// dock (Nodes panel hidden), between the toolbar and the status bar.
const CANVASES: [(f32, f32); 2] = [(1040.0, 838.0), (1520.0, 1018.0)];

/// Measure text with real fonts, as the app does. Every test here sets
/// the same context, so none depends on which ran first.
fn real_fonts() {
    static CTX: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();
    geom::set_measure(CTX.get_or_init(|| {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        ctx
    }));
}

fn loaded(flora: &str) -> PatchFile {
    let mut p = floret_engine::load_patch(flora).expect("sample loads");
    crate::editor::settle(&mut p, Catalogue::builtin());
    p
}

#[test]
fn samples_open_fitted_and_centred() {
    real_fonts();
    let cat = Catalogue::builtin();
    let painter = geom::measuring_painter();
    let mut failures = Vec::new();
    for s in SAMPLES {
        let mut patch = loaded(s.flora);
        for (w, h) in CANVASES {
            let area = Rect::from_min_size(pos2(0.0, 38.0), vec2(w, h));
            let mut view = View::default();
            view.on_load(&mut patch);
            let visible = subpatch::visible(&patch, cat, &view.stack);
            view.fit_nodes(&patch, area, |id| visible.contains(id));
            let decor = geom::Decor::default();
            let cx = geom::Ctx {
                patch: &patch,
                cat,
                painter: &painter,
                origin: area.min,
                pan: view.pan,
                zoom: view.zoom,
                scope: None,
                expanded: &view.expanded,
                decor: &decor,
            };
            // Node boxes, and with the pins sticking out of them.
            let (mut boxes, mut shown) = (Rect::NOTHING, Rect::NOTHING);
            for b in geom::layout(&cx, &visible, &|_| false).values() {
                boxes = boxes.union(b.rect).union(b.header);
                for p in &b.pins {
                    shown = shown.union(Rect::from_center_size(
                        p.pos,
                        Vec2::splat(2.0 * geom::PIN_R * view.zoom),
                    ));
                }
            }
            shown = shown.union(boxes);
            let at = format!("{} at {w}×{h}", s.id);
            if !area.shrink(16.0).contains_rect(shown) {
                failures.push(format!("{at}: {shown:?} spills out of {area:?}"));
            }
            let off = boxes.center() - area.center();
            if off.length() > 1.0 {
                failures.push(format!("{at}: off centre by {off:?}"));
            }
            if shown.intersects(minimap::map_rect(area)) {
                failures.push(format!("{at}: under the minimap"));
            }
            // Fitting a big patch zooms out a long way; this catches a
            // stray node far from the rest.
            if view.zoom < 0.12 {
                failures.push(format!("{at}: zoomed out to {:.2}", view.zoom));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Rects of a scope's members, relative to the scope: frames include the
/// label strip above them.
fn scope_rects(patch: &PatchFile, m: &geom::Metrics, scope: Option<&str>) -> Vec<(String, Rect)> {
    use crate::editor::layout::Measure;
    patch
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == scope)
        .map(|n| {
            let (w, h) = m.size(n);
            let strip = if m.is_frame(n) {
                regions::HEADER_H
            } else {
                0.0
            };
            let min = pos2(n.position.x as f32, (n.position.y - strip) as f32);
            (
                n.id.clone(),
                Rect::from_min_size(min, vec2(w as f32, (h + strip) as f32)),
            )
        })
        .collect()
}

#[test]
fn sample_layouts_are_tidy() {
    real_fonts();
    let cat = Catalogue::builtin();
    let mut failures = Vec::new();
    for s in SAMPLES {
        let patch = loaded(s.flora);
        let m = geom::Metrics::of(&patch, cat);
        let mut scopes: Vec<Option<&str>> = vec![None];
        scopes.extend(
            patch
                .nodes
                .iter()
                .filter(|n| {
                    patch
                        .nodes
                        .iter()
                        .any(|c| c.parent_id.as_ref() == Some(&n.id))
                })
                .map(|n| Some(n.id.as_str())),
        );
        for scope in scopes {
            let rects = scope_rects(&patch, &m, scope);
            for (i, (a, ra)) in rects.iter().enumerate() {
                for (b, rb) in &rects[i + 1..] {
                    // At least a pin's reach apart.
                    if ra.expand(8.0).intersects(*rb) {
                        failures.push(format!("{}: {a} and {b} overlap", s.id));
                    }
                }
            }
            let Some(region) = scope.and_then(|r| patch.nodes.iter().find(|n| n.id == r)) else {
                continue;
            };
            let (w, h) = ops::region_size(region);
            // Clear of the pills down both sides.
            let inner = Rect::from_min_max(
                pos2((regions::PORT_TAB_W + 8.0) as f32, 0.0),
                pos2((w - regions::PORT_TAB_W - 8.0) as f32, h as f32),
            );
            for (c, r) in &rects {
                if !inner.contains_rect(*r) {
                    failures.push(format!("{}: {c} isn't clear inside {}", s.id, region.id));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Every row's label, value and output name, and every header, fit their
/// node without running into each other (zoom 1, real fonts).
#[test]
fn sample_nodes_fit_their_text() {
    real_fonts();
    let cat = Catalogue::builtin();
    let painter = geom::measuring_painter();
    let w = |s: &str, font: egui::FontId| {
        painter
            .layout_no_wrap(s.to_owned(), font, egui::Color32::WHITE)
            .size()
            .x
    };
    let label = egui::FontId::proportional(geom::LABEL_FONT);
    let mono = egui::FontId::monospace(10.0);
    let mut failures = Vec::new();
    for s in SAMPLES {
        let patch = loaded(s.flora);
        let all: HashSet<String> = patch.nodes.iter().map(|n| n.id.clone()).collect();
        let (decor, expanded) = (geom::Decor::default(), HashSet::new());
        let cx = geom::Ctx {
            patch: &patch,
            cat,
            painter: &painter,
            origin: Pos2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0,
            scope: None,
            expanded: &expanded,
            decor: &decor,
        };
        for (id, b) in geom::layout(&cx, &all, &|_| false) {
            if b.look != Look::Node {
                continue;
            }
            let at = format!("{}: {id}", s.id);
            let title = if b.math {
                egui::FontId::monospace(13.0)
            } else {
                egui::FontId::proportional(12.5)
            };
            let mut head = draw::TITLE_X + w(&b.label, title);
            if let Some(slot) = &b.slot {
                head += 6.0 + w(slot, mono.clone()) + 8.0;
            }
            if head > b.rect.width() - 8.0 {
                failures.push(format!("{at}: title runs past the header"));
            }
            for row in &b.rows {
                let fields: Vec<&Field> = b
                    .fields
                    .iter()
                    .filter(|f| (f.rect.center().y - row.y).abs() < 1.0)
                    .collect();
                // Where the left column's content ends.
                let mut end = row.label_rect.right();
                if let Some(ro) = &row.readout {
                    end = row.label_rect.right() + 4.0 + w(ro, mono.clone());
                }
                for f in &fields {
                    if f.rect.left() < row.label_rect.right() - 4.0 {
                        failures.push(format!("{at}: a field covers its label"));
                    }
                    end = end.max(f.rect.right());
                    if let FieldKind::Select(_) = &f.kind {
                        let text = values::option_label(&values::as_text(&f.value)).to_owned();
                        if w(&text, mono.clone()) + 4.0 > f.rect.width() - 12.0 {
                            failures.push(format!("{at}: dropdown text '{text}' overflows"));
                        }
                    }
                }
                let right = b.rect.right() - 10.0;
                let out = row
                    .output
                    .as_ref()
                    .map_or(0.0, |(l, _)| w(l, label.clone()) + 4.0);
                if end > right - out + 1.0 {
                    failures.push(format!(
                        "{at}: row '{}' runs into {}",
                        row.label.clone().unwrap_or_default(),
                        if out > 0.0 { "its output" } else { "the edge" }
                    ));
                }
            }
        }
    }
    failures.dedup();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The Palette's mode dropdown is wide enough for every mode, in every
/// mode: a generating palette (standard rows, the output on the mode's
/// row) grows to fit it; the stops body keeps the custom-node width.
#[test]
fn palette_mode_dropdown_fits_every_option() {
    real_fonts();
    let cat = Catalogue::builtin();
    let painter = geom::measuring_painter();
    let mono = egui::FontId::monospace(10.0);
    let w = |s: &str| {
        painter
            .layout_no_wrap(s.to_owned(), mono.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    let modes = cat.get("color/palette").unwrap().param_options["mode"].clone();
    let longest = modes.iter().map(|m| w(m)).fold(0.0, f32::max);
    for mode in &modes {
        let flora = json!({
            "kind": "floret.patch", "version": 2, "edges": [],
            "nodes": [{"id": "p", "defType": "color/palette", "position": {"x": 0, "y": 0},
                       "params": {"mode": mode, "stopCount": 3}}],
        });
        let patch = loaded(&flora.to_string());
        let all: HashSet<String> = ["p".to_owned()].into();
        let (decor, expanded) = (geom::Decor::default(), HashSet::new());
        let cx = geom::Ctx {
            patch: &patch,
            cat,
            painter: &painter,
            origin: Pos2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0,
            scope: None,
            expanded: &expanded,
            decor: &decor,
        };
        let boxes = geom::layout(&cx, &all, &|_| false);
        let b = &boxes["p"];
        let field = b
            .fields
            .iter()
            .find(|f| f.key == EditKey::Param("mode".into()))
            .unwrap_or_else(|| panic!("{mode}: no mode dropdown"));
        // The same fit rule as `sample_nodes_fit_their_text`.
        assert!(
            longest + 4.0 <= field.rect.width() - 12.0,
            "{mode}: dropdown {} wide, longest option {longest}",
            field.rect.width()
        );
        assert!(field.rect.right() <= b.rect.right() - 10.0, "{mode}");
        if mode == "stops" {
            assert_eq!(b.rect.width(), geom::NODE_W, "stops keeps its width");
        } else {
            assert!(b.rect.width() >= geom::NODE_W, "{mode}");
        }
    }
}

fn samples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples")
}

fn num(v: f64) -> Json {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

/// Copy the arranged positions and region sizes into the file's JSON,
/// leaving everything else as it was.
fn write_back(raw: &mut Json, patch: &PatchFile) {
    let nodes = raw["nodes"].as_array_mut().expect("nodes");
    assert_eq!(nodes.len(), patch.nodes.len(), "the loader added nodes");
    for (j, n) in nodes.iter_mut().zip(&patch.nodes) {
        assert_eq!(j["id"].as_str(), Some(n.id.as_str()));
        j["position"] = json!({ "x": num(n.position.x), "y": num(n.position.y) });
        if let (Some(ports), Some(js)) = (&n.ports, j.get_mut("ports").and_then(Json::as_array_mut))
        {
            for (p, jp) in ports.iter().zip(js) {
                assert_eq!(jp["id"].as_str(), Some(p.id.as_str()));
                if let Some(y) = p.y {
                    jp["y"] = num(y);
                }
            }
        }
        for (k, v) in [("width", n.width), ("height", n.height)] {
            let Some(v) = v.filter(|_| is_region(&n.def_type)) else {
                continue;
            };
            if j.get(k).is_some() {
                j[k] = num(v);
            }
            if let Some(p) = j.get_mut("params").filter(|p| p.get(k).is_some()) {
                p[k] = num(v);
            }
        }
    }
}

fn is_region(ty: &str) -> bool {
    Catalogue::builtin()
        .get(ty)
        .is_some_and(|s| s.region_kind.is_some())
}

#[test]
#[ignore = "rewrites native/samples; run on its own with --ignored"]
fn relayout_samples() {
    real_fonts();
    let cat = Catalogue::builtin();
    for s in SAMPLES {
        let path = samples_dir().join(format!("{}.flora", s.id));
        let text = std::fs::read_to_string(&path).expect("sample file");
        let mut patch = loaded(&text);
        let metrics = geom::Metrics::of(&patch, cat);
        layout::auto_layout(&mut patch, &metrics);
        let mut raw: Json = serde_json::from_str(&text).unwrap();
        write_back(&mut raw, &patch);
        let out = serde_json::to_string_pretty(&raw).unwrap() + "\n";
        if out != text {
            std::fs::write(&path, out).unwrap();
        }
    }
}
