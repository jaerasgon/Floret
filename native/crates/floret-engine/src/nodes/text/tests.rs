use crate::nodes::math::tests::eval;
use crate::scene::{neutral_style, resolve_style, SceneNode, Style, StyleOp};
use crate::spec::Params;
use crate::value::{one, Spread, Value, Vec2};

fn run(ty: &str, inputs: Vec<Value>) -> Vec<Spread> {
    let spreads: Vec<Spread> = inputs
        .into_iter()
        .map(|v| match v {
            Value::List(items) => items,
            other => one(other),
        })
        .collect();
    eval(ty, &Params::new(), &spreads).unwrap()
}

fn s(v: &str) -> Value {
    Value::str(v)
}

fn n(v: f64) -> Value {
    Value::Num(v)
}

fn v2(x: f64, y: f64) -> Value {
    Value::Vec2(Vec2 { x, y })
}

fn st(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => panic!("not a string: {other:?}"),
    }
}

fn nums(sp: &Spread) -> Vec<f64> {
    sp.iter().map(|v| v.to_number()).collect()
}

fn run_of(v: &Value) -> &crate::scene::GlyphRun {
    match v {
        Value::Scene(node) => match &**node {
            SceneNode::Glyphs(run) => run,
            other => panic!("not glyphs: {other:?}"),
        },
        other => panic!("not a layer: {other:?}"),
    }
}

fn variations(style: &Value) -> Vec<([u8; 4], f32)> {
    let Value::Style(st) = style else {
        panic!("not a style: {style:?}");
    };
    st.apply(neutral_style()).font_variations.to_vec()
}

#[test]
fn glyphs_split_text_like_draw_text() {
    let out = run(
        "text/glyphs",
        vec![
            s("Héllo"),
            v2(10.0, 20.0),
            n(40.0),
            s("left"),
            n(0.0),
            Value::Null,
        ],
    );
    let shape = run_of(&out[0][0]);
    assert_eq!(shape.glyphs.len(), 5);
    assert_eq!(shape.size, 40.0);
    assert_eq!(out[1].len(), 5);
    // Per-glyph layers sit at the origin.
    let first = run_of(&out[1][0]);
    assert_eq!((first.glyphs[0].x, first.glyphs[0].y), (0.0, 0.0));
    assert_eq!(first.glyphs[0].id, shape.glyphs[0].id);
    // Positions: on the baseline, 1 unit in and down from pos (where
    // draw/text puts its alphabetic baseline: the TS glyph canvas inset).
    let pos: Vec<Vec2> = out[2]
        .iter()
        .map(|v| match v {
            Value::Vec2(p) => *p,
            _ => panic!(),
        })
        .collect();
    assert!((pos[0].x - 11.0).abs() < 1e-9 && (pos[0].y - 21.0).abs() < 1e-9);
    let adv = nums(&out[3]);
    for i in 1..5 {
        assert!((pos[i].x - pos[i - 1].x - adv[i - 1]).abs() < 1e-9);
        assert_eq!(pos[i].x, shape.glyphs[i].x);
    }
    let chars: Vec<String> = out[4].iter().map(st).collect();
    assert_eq!(chars, ["H", "é", "l", "l", "o"]);
    assert_eq!(nums(&out[5]), [0.0, 1.0, 2.0, 3.0, 4.0]);

    // Spacing spreads the glyphs; right alignment ends them left of pos.
    let wide = run(
        "text/glyphs",
        vec![
            s("Héllo"),
            v2(10.0, 20.0),
            n(40.0),
            s("right"),
            n(8.0),
            Value::Null,
        ],
    );
    let wadv = nums(&wide[3]);
    assert!((wadv[0] - adv[0] - 8.0).abs() < 1e-9);
    let last = run_of(&wide[0][0]).glyphs[4];
    assert!(last.x + wadv[4] < 10.0);
}

#[test]
fn on_path_follows_points_and_layers() {
    let ring: Vec<Value> = (0..72)
        .map(|i| {
            let a = i as f64 / 72.0 * std::f64::consts::TAU;
            v2(a.cos() * 150.0, a.sin() * 150.0)
        })
        .collect();
    let out = run(
        "text/on-path",
        vec![
            s("ROUND AND ROUND"),
            Value::list(ring),
            Value::Null,
            Value::Bool(true),
            n(24.0),
            n(0.0),
            n(0.0),
            s("left"),
            n(0.0),
            Value::Null,
        ],
    );
    let shape = run_of(&out[0][0]);
    assert_eq!(shape.glyphs.len(), 15);
    assert_eq!(out[2].len(), 15);
    let angles = nums(&out[3]);
    // Angles turn steadily around the loop.
    assert!(angles.windows(2).filter(|w| w[1] < w[0]).count() <= 1);
    assert!(shape
        .glyphs
        .iter()
        .zip(&angles)
        .all(|(g, a)| (g.angle - a).abs() < 1e-12));

    // A wired path layer (draw/path's polygon) wins over the points.
    let path = run(
        "draw/path",
        vec![
            Value::list(vec![v2(-200.0, 0.0), v2(200.0, 0.0)]),
            Value::Null,
            n(2.0),
            n(0.0),
            n(0.0),
        ],
    );
    let out = run(
        "text/on-path",
        vec![
            s("flat"),
            Value::list(vec![v2(0.0, 0.0), v2(0.0, 100.0)]),
            path[0][0].clone(),
            Value::Bool(false),
            n(24.0),
            n(0.0),
            n(0.0),
            s("center"),
            n(0.0),
            Value::Null,
        ],
    );
    assert!(nums(&out[3]).iter().all(|a| a.abs() < 1e-12));
    let first = run_of(&out[0][0]).glyphs[0];
    assert!(first.x > -60.0 && first.x < 0.0 && first.y.abs() < 1e-9);
}

#[test]
fn outline_points_trace_and_fill_glyphs() {
    let args = |mode: &str| {
        vec![
            s("Io"),
            v2(0.0, 0.0),
            n(100.0),
            s("center"),
            n(4.0),
            s(mode),
            Value::Null,
        ]
    };
    let out = run("text/outline", args("outline"));
    let pts = &out[0];
    assert!(pts.len() > 60);
    assert_eq!(out[1].len(), pts.len());
    // "I" is one contour, "o" two: contour ids 0..3, glyph ids 0..2.
    let contours = nums(&out[1]);
    assert_eq!(contours.first(), Some(&0.0));
    assert_eq!(contours.last(), Some(&2.0));
    assert!(contours.windows(2).all(|w| w[1] >= w[0]));
    assert_eq!(nums(&out[2]).last(), Some(&1.0));
    // Centred on x = 0, above the baseline.
    let xs: Vec<f64> = pts.iter().map(|v| v.get("x").to_number()).collect();
    let ys: Vec<f64> = pts.iter().map(|v| v.get("y").to_number()).collect();
    let (lo, hi) = xs
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
    assert!(lo < 0.0 && hi > 0.0 && (lo + hi).abs() < 30.0, "{lo} {hi}");
    assert!(ys.iter().all(|y| *y <= 4.0 && *y > -100.0));

    let fill = run("text/outline", args("fill"));
    assert!(fill[0].len() > pts.len() / 2);
    assert_eq!(nums(&fill[1]).last(), Some(&1.0));
}

#[test]
fn font_variations_build_a_style_chain() {
    let out = run(
        "style/font-variations",
        vec![
            n(650.0),
            n(0.0),
            n(-8.0),
            n(0.0),
            s("GRAD"),
            n(-50.0),
            Value::Null,
        ],
    );
    assert_eq!(
        variations(&out[0][0]),
        vec![(*b"wght", 650.0), (*b"slnt", -8.0), (*b"GRAD", -50.0)]
    );
    // Chained: a later node overrides an axis and keeps the others.
    let next = run(
        "style/font-variations",
        vec![
            n(300.0),
            n(75.0),
            n(0.0),
            n(0.0),
            s(""),
            n(0.0),
            out[0][0].clone(),
        ],
    );
    let v = variations(&next[0][0]);
    assert_eq!(
        v,
        vec![
            (*b"slnt", -8.0),
            (*b"GRAD", -50.0),
            (*b"wght", 300.0),
            (*b"wdth", 75.0)
        ]
    );
    // Untouched styles keep no variations (and no `fontVariations` key).
    let plain = Style::chain(None, [StyleOp::FontSize(12.0)]);
    let st = resolve_style(Some(&plain), crate::scene::default_style());
    assert!(st.font_variations.is_empty());
    assert!(crate::scene::style_value(&st)
        .get("fontVariations")
        .is_undefined());
}

#[test]
fn font_node_reports_families_and_axes() {
    let out = run("text/font", vec![s("Roboto Flex"), s("")]);
    assert_eq!(st(&out[0][0]), "Roboto Flex");
    assert!(out[1][0].truthy());
    let axes: Vec<String> = out[2].iter().map(st).collect();
    let w = axes.iter().position(|a| a == "wght").unwrap();
    assert_eq!(nums(&out[3])[w], 100.0);
    assert_eq!(nums(&out[4])[w], 400.0);
    assert_eq!(nums(&out[5])[w], 1000.0);

    let missing = run("text/font", vec![s("No Such Family"), s("")]);
    assert!(!missing[1][0].truthy());
    let generic = run("text/font", vec![s("serif"), s("")]);
    assert!(generic[1][0].truthy());
    assert!(generic[2].is_empty());
    let bad_file = run("text/font", vec![s("serif"), s("/no/such/font.ttf")]);
    assert_eq!(st(&bad_file[0][0]), "serif");
    assert!(!bad_file[1][0].truthy());
    // A loaded file's family is what styles take.
    let path = std::env::temp_dir().join(format!("floret-node-font-{}.woff2", std::process::id()));
    std::fs::write(&path, include_bytes!("../../text/test-subset.woff2")).unwrap();
    let file = run("text/font", vec![s(""), s(path.to_str().unwrap())]);
    assert_eq!(st(&file[0][0]), "Floret Test Subset");
    assert!(file[1][0].truthy());
    let _ = std::fs::remove_file(path);
}
