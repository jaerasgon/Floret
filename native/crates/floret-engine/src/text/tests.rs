use super::path::{on_path, Path};
use super::*;
use crate::scene::{default_style, ResolvedStyle};
use crate::value::Vec2;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

fn spec(family: &str, size: f64, vars: &[AxisSetting]) -> FontSpec {
    FontSpec {
        family: Arc::from(family),
        weight: 400,
        italic: false,
        variations: Arc::from(vars),
        size,
    }
}

fn width(text: &str, f: &FontSpec) -> f64 {
    shape_line(text, f, 0.0).width
}

#[test]
fn bundled_families_resolve() {
    assert_eq!(resolve_family("'NoSuchFont', Serif"), "serif");
    assert_eq!(resolve_family("NoSuchFont"), "sans-serif");
    assert_eq!(resolve_family("roboto flex"), VARIABLE_FAMILY);
    assert!(families().iter().any(|f| f == VARIABLE_FAMILY));
    let rf = resolve(VARIABLE_FAMILY, 400, false).and_then(face).unwrap();
    let tags: Vec<String> = rf.axes.iter().map(AxisInfo::tag_str).collect();
    for t in ["wght", "wdth", "slnt", "opsz", "GRAD", "XTRA"] {
        assert!(tags.iter().any(|x| x == t), "{t} missing from {tags:?}");
    }
    assert_eq!(css_weight("bold"), 700);
    assert_eq!(css_weight("350"), 350);
    assert_eq!(parse_tag("wght"), Some(*b"wght"));
    assert_eq!(parse_tag("ab"), Some(*b"ab  "));
    assert_eq!(parse_tag("toolong"), None);
}

#[test]
fn shaping_kerns_and_tracks() {
    let f = spec("sans-serif", 100.0, &[]);
    // "AV" kerns tighter than its letters apart.
    assert!(width("AV", &f) < width("A", &f) + width("V", &f) - 1.0);
    let line = shape_line("Hello", &f, 0.0);
    assert_eq!(line.glyphs.len(), 5);
    let clusters: Vec<usize> = line.glyphs.iter().map(|g| g.cluster).collect();
    assert_eq!(clusters, [0, 1, 2, 3, 4]);
    // Pen positions accumulate advances.
    for w in line.glyphs.windows(2) {
        assert!((w[1].x - (w[0].x + w[0].advance)).abs() < 1e-9);
    }
    // Tracking adds to every advance.
    let tracked = shape_line("Hello", &f, 10.0);
    assert!((tracked.width - line.width - 50.0).abs() < 1e-6);
}

#[test]
fn variation_axes_change_advances_and_outlines() {
    let thin = spec(VARIABLE_FAMILY, 100.0, &[(*b"wght", 100.0)]);
    let black = spec(VARIABLE_FAMILY, 100.0, &[(*b"wght", 1000.0)]);
    let narrow = spec(VARIABLE_FAMILY, 100.0, &[(*b"wdth", 25.0)]);
    let wide = spec(VARIABLE_FAMILY, 100.0, &[(*b"wdth", 151.0)]);
    let (wt, wb) = (width("Hamburg", &thin), width("Hamburg", &black));
    let (wn, ww) = (width("Hamburg", &narrow), width("Hamburg", &wide));
    assert!(wb > wt * 1.05, "wght: {wt} → {wb}");
    assert!(ww > wn * 1.25, "wdth: {wn} → {ww}");

    let face = resolve(VARIABLE_FAMILY, 400, false).and_then(face).unwrap();
    let h = shape_line("H", &thin, 0.0).glyphs[0].id;
    let outline_w = |f: &FontSpec| {
        let cs = outline::glyph_contours(&face, h, 100.0, &face_settings(&face, f));
        let (lo, hi) = outline::bounds(&cs).unwrap();
        hi.x - lo.x
    };
    assert!(outline_w(&wide) > outline_w(&narrow) * 1.25);
    assert!(outline_w(&black) > outline_w(&thin));
    // Rasteriser coordinates follow the same settings.
    let c_thin = normalized_coords(&face, &face_settings(&face, &thin));
    let c_black = normalized_coords(&face, &face_settings(&face, &black));
    assert_eq!(c_thin.len(), face.axes.len());
    assert_ne!(c_thin, c_black);
}

#[test]
fn css_weight_drives_wght_unless_set() {
    let face = resolve(VARIABLE_FAMILY, 400, false).and_then(face).unwrap();
    let mut f = spec(VARIABLE_FAMILY, 50.0, &[]);
    f.weight = 700;
    assert_eq!(face_settings(&face, &f), vec![(*b"wght", 700.0)]);
    f.variations = Arc::from([(*b"wght", 250.0), (*b"wdth", 80.0), (*b"nope", 1.0)]);
    assert_eq!(
        face_settings(&face, &f),
        vec![(*b"wght", 250.0), (*b"wdth", 80.0)]
    );
    // A static face takes no settings.
    let lib = resolve("Liberation Sans", 400, false)
        .and_then(super::face)
        .unwrap();
    assert!(face_settings(&lib, &f).is_empty());
}

#[test]
fn outlines_flatten_and_sample() {
    let face = resolve("sans-serif", 400, false).and_then(face).unwrap();
    let f = spec("sans-serif", 100.0, &[]);
    let o = shape_line("O", &f, 0.0).glyphs[0].id;
    let cs = outline::glyph_contours(&face, o, 100.0, &[]);
    // An "O" is two closed rings, above the baseline (y up = negative).
    assert_eq!(cs.len(), 2);
    let (lo, hi) = outline::bounds(&cs).unwrap();
    assert!(lo.y < -60.0 && hi.y > -5.0 && hi.y < 5.0, "{lo:?} {hi:?}");
    let pts = outline::sample_contour(&cs[0], 5.0);
    for w in pts.windows(2) {
        let d = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        assert!(d < 5.5, "gap {d}");
    }
    // Inside the ring, not in the counter.
    let mid = Vec2 {
        x: (lo.x + hi.x) / 2.0,
        y: (lo.y + hi.y) / 2.0,
    };
    assert_eq!(outline::winding(&cs, mid), 0);
    let fill = outline::fill_points(&cs, 3.0);
    assert!(fill.len() > 50);
    assert!(fill.iter().all(|p| outline::winding(&cs, *p) != 0));
}

#[test]
fn layout_along_a_straight_path() {
    let f = spec("sans-serif", 40.0, &[]);
    let line = shape_line("Path", &f, 0.0);
    let flat = Path::new([Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 1000.0, y: 0.0 }], false).unwrap();
    let placed = on_path(&line, &flat, 10.0, "left", 0.0);
    assert_eq!(placed.len(), 4);
    for (p, g) in placed.iter().zip(&line.glyphs) {
        assert!(p.angle.abs() < 1e-12);
        assert!((p.pos.x - (10.0 + g.x)).abs() < 1e-9);
        assert!(p.pos.y.abs() < 1e-9);
    }
    // Centred; a shift lifts the baseline off the path (y up).
    let c = on_path(&line, &flat, 0.0, "center", 5.0);
    assert!((c[0].pos.x - (500.0 - line.width / 2.0)).abs() < 1e-9);
    assert!((c[0].pos.y + 5.0).abs() < 1e-9);
    // Down a vertical path the glyphs turn a quarter and advance in y.
    let down = Path::new([Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 0.0, y: 500.0 }], false).unwrap();
    let d = on_path(&line, &down, 0.0, "left", 0.0);
    assert!((d[1].angle - FRAC_PI_2).abs() < 1e-12);
    assert!(d[1].pos.x.abs() < 1e-9 && (d[1].pos.y - line.glyphs[1].x).abs() < 1e-9);
    // Glyphs past the end of an open path are dropped.
    let short = Path::new([Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 30.0, y: 0.0 }], false).unwrap();
    assert!(on_path(&line, &short, 0.0, "left", 0.0).len() < 4);
}

#[test]
fn layout_around_a_circle() {
    let r = 200.0;
    let ring: Vec<Vec2> = (0..360)
        .map(|i| {
            let a = i as f64 / 360.0 * TAU;
            Vec2 {
                x: a.cos() * r,
                y: a.sin() * r,
            }
        })
        .collect();
    let path = Path::new(ring, true).unwrap();
    assert!((path.length() - TAU * r).abs() < 1.0);
    let f = spec("sans-serif", 30.0, &[]);
    let line = shape_line("AROUND THE RING", &f, 0.0);
    // Starting past the end wraps around a closed path.
    let placed = on_path(&line, &path, path.length() - 20.0, "left", 0.0);
    assert_eq!(placed.len(), line.glyphs.len());
    for p in &placed {
        let (s, c) = p.angle.sin_cos();
        let adv = p.glyph.advance;
        // The middle of each glyph's advance lies on the circle, and the
        // glyph is turned to the tangent there (perpendicular to the radius).
        let mid = Vec2 {
            x: p.pos.x + c * adv / 2.0,
            y: p.pos.y + s * adv / 2.0,
        };
        assert!((mid.x.hypot(mid.y) - r).abs() < 0.5, "{mid:?}");
        let radial = mid.y.atan2(mid.x);
        let turn = (p.angle - radial).rem_euclid(TAU);
        assert!(
            (turn - FRAC_PI_2).abs() < 0.03,
            "angle {} at {radial}",
            p.angle
        );
    }
}

#[test]
fn arabic_shapes_right_to_left_with_joining() {
    let f = spec("sans-serif", 40.0, &[]);
    let sans = resolve("sans-serif", 400, false).unwrap();
    if fallback('س', sans).is_none() {
        eprintln!("no Arabic font installed; skipping");
        return;
    }
    let word = "سلام";
    let line = shape_line(word, &f, 0.0);
    assert!(!line.glyphs.is_empty());
    assert!(line.glyphs.iter().all(|g| g.id != 0), "{line:?}");
    // Visual order: the first glyph drawn is the last character.
    assert!(line.glyphs[0].cluster > line.glyphs[line.glyphs.len() - 1].cluster);
    // Joined forms differ from the isolated letter.
    // In a proportional sans like the primary, not a monospaced face.
    let used = database().face(line.glyphs[0].face.0).unwrap().monospaced;
    assert!(!used);
    let alone = shape_line("س", &f, 0.0).glyphs[0].id;
    let initial = line.glyphs.last().unwrap();
    assert_eq!(initial.cluster, 0);
    assert_ne!(initial.id, alone);
    // Mixed with Latin: the Latin word keeps its place on the left.
    let mixed = shape_line("Hi سلام", &f, 0.0);
    assert_eq!(mixed.glyphs[0].cluster, 0);
    assert!(mixed.width > line.width);
}

#[test]
fn combining_marks_stay_in_their_cluster() {
    let f = spec("sans-serif", 40.0, &[]);
    let line = shape_line("e\u{301}x", &f, 0.0);
    let pre = shape_line("\u{e9}x", &f, 0.0);
    assert!((line.width - pre.width).abs() < 0.5, "{line:?} vs {pre:?}");
    assert!(line.glyphs.iter().all(|g| g.id != 0));
    let ranges = line.cluster_ranges("e\u{301}x".len());
    assert_eq!(ranges[0], 0..3);
}

#[test]
fn block_matches_draw_text_metrics() {
    let st = ResolvedStyle {
        font_size: 20.0,
        ..(**default_style()).clone()
    };
    let b = block("ab\ncd", &st, 0.0).unwrap();
    assert_eq!(b.glyphs.len(), 4);
    assert!((b.asc - 16.0).abs() < 1e-12);
    // Glyphs start 1 unit in; the second line sits a line height lower.
    assert!((b.glyphs[0].glyph.x - 1.0).abs() < 1e-9);
    assert!((b.glyphs[2].glyph.y - b.glyphs[0].glyph.y - 24.0).abs() < 1e-9);
    assert_eq!(b.glyphs[2].source, 3..4);
    assert!((b.h - (48.0 + 6.0)).abs() < 1e-9);
    let mut centred = st.clone();
    centred.text_align = Arc::from("center");
    let (ox, oy) = b.origin(&centred, 100.0, 50.0);
    assert!((ox - (100.0 - b.w / 2.0)).abs() < 1e-9 && (oy - 34.0).abs() < 1e-9);
}

#[test]
fn loads_woff2_and_ttf_files() {
    let woff2 = include_bytes!("test-subset.woff2").to_vec();
    let f = load_bytes(woff2).unwrap();
    assert_eq!(f.faces.len(), 1);
    assert!(!f.family.is_empty());
    let face = super::face(f.faces[0]).unwrap();
    assert!(face.has_char('A'));
    assert!(load_bytes(b"not a font".to_vec()).is_err());
    let gen = generation();
    let dir = std::env::temp_dir().join(format!("floret-font-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("subset.woff2");
    std::fs::write(&path, include_bytes!("test-subset.woff2")).unwrap();
    let a = load_file(&path).unwrap();
    assert!(generation() > gen);
    // Loading the same unchanged file again is cached.
    let g2 = generation();
    assert_eq!(load_file(&path).unwrap(), a);
    assert_eq!(generation(), g2);
    assert!(load_file(&dir.join("missing.ttf")).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
