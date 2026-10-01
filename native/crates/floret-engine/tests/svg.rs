//! SVG exporter parity with the TypeScript reference (`src/runtime/scene-svg.ts`).
//!
//! 1. The TS unit tests (`src/runtime/__tests__/scene-svg.test.ts`), ported
//!    with the same inputs and the same substring assertions.
//! 2. Golden fixtures (`native/fixtures/svg/`, written by
//!    `npm run fixtures:svg`): scenes taken from every sample's recorded
//!    renderer output plus hand-built edge cases, each with the SVG the TS
//!    exporter produced. The Rust output must be byte-identical.

use floret_engine::scene::{
    default_style, line_default_style, Paint, RectF, ResolvedStyle, SceneNode,
};
use floret_engine::svg::{scene_to_svg, SvgExportOptions};
use floret_engine::Vec2;
use serde_json::Value as J;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ─── Ported TS unit tests ────────────────────────────────────────────────────

fn style(f: impl FnOnce(&mut ResolvedStyle)) -> Arc<ResolvedStyle> {
    let mut st = (**default_style()).clone();
    f(&mut st);
    Arc::new(st)
}

fn red() -> Arc<ResolvedStyle> {
    style(|s| s.fill = Some(Paint::css("#ff0000")))
}

fn red_stroke() -> Arc<ResolvedStyle> {
    style(|s| {
        s.fill = None;
        s.stroke = Some(Paint::css("#0000ff"));
        s.stroke_weight = 3.0;
    })
}

fn rect_at(x: f64, y: f64, w: f64, h: f64, mode: &str, st: Arc<ResolvedStyle>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Rect {
        x,
        y,
        w,
        h,
        mode: Arc::from(mode),
        style: st,
    })
}

fn rect() -> Arc<SceneNode> {
    rect_at(0.0, 0.0, 10.0, 20.0, "corner", red())
}

fn text(t: &str, x: f64, y: f64, st: Arc<ResolvedStyle>) -> Arc<SceneNode> {
    Arc::new(SceneNode::Text {
        text: Arc::from(t),
        x,
        y,
        style: st,
    })
}

fn group(
    children: Vec<Arc<SceneNode>>,
    transform: Option<[f64; 6]>,
    blend: Option<&str>,
    clip: Option<RectF>,
) -> Arc<SceneNode> {
    Arc::new(SceneNode::Group {
        children: Arc::from(children),
        transform,
        blend_mode: blend.map(Arc::from),
        clip,
    })
}

fn pts(xy: &[f64]) -> Arc<[Vec2]> {
    xy.chunks(2).map(|p| Vec2 { x: p[0], y: p[1] }).collect()
}

fn svg(layers: &[Arc<SceneNode>], width: f64, height: f64) -> String {
    scene_to_svg(
        layers,
        &SvgExportOptions {
            width,
            height,
            background: None,
        },
    )
}

#[test]
fn document_wrapper() {
    let out = svg(&[], 320.0, 240.0);
    assert!(out.starts_with("<svg"));
    assert!(out.contains(r#"viewBox="0 0 320 240""#));
    assert!(out.contains(r#"width="320""#));
    assert!(out.contains(r#"height="240""#));
    assert!(out.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
    assert!(out.ends_with("</svg>"));

    let bg = scene_to_svg(
        &[],
        &SvgExportOptions {
            width: 100.0,
            height: 100.0,
            background: Some("#abcdef"),
        },
    );
    assert!(bg.contains(r##"<rect width="100" height="100" fill="#abcdef""##));
    assert!(!svg(&[], 100.0, 100.0).contains("<rect "));

    assert!(svg(&[], 0.0, -50.0).contains(r#"viewBox="0 0 1 1""#));

    // Centre-origin, like the canvas renderer.
    let out = svg(&[rect()], 400.0, 300.0);
    let at = out.find(r#"<g transform="translate(200,150)">"#).unwrap();
    assert!(out[at..].contains("<rect"));
    assert!(out.ends_with("</g></svg>"));
}

#[test]
fn rgba_paint_is_split() {
    let st = style(|s| s.fill = Some(Paint::css("rgba(244,71,61,0.5)")));
    let out = svg(&[rect_at(0.0, 0.0, 10.0, 20.0, "corner", st)], 100.0, 100.0);
    assert!(out.contains(r#"fill="rgb(244,71,61)""#));
    assert!(out.contains(r#"fill-opacity="0.5""#));
    assert!(!out.contains("rgba("));

    let out = svg(&[rect()], 100.0, 100.0);
    assert!(out.contains(r##"fill="#ff0000""##));
    assert!(!out.contains("fill-opacity"));
}

#[test]
fn coordinate_robustness() {
    let r = rect_at(
        f64::NAN,
        f64::INFINITY,
        10.0,
        f64::NEG_INFINITY,
        "corner",
        red(),
    );
    let out = svg(&[r], 100.0, 100.0);
    assert!(!out.contains("NaN") && !out.contains("Infinity"));
    assert!(out.contains(r#"x="0""#));
    assert!(out.contains(r#"y="0""#));

    let r = rect_at(1.347e-14, 12.3456789, 10.0, 10.0, "corner", red());
    let out = svg(&[r], 100.0, 100.0);
    assert!(out.contains(r#"x="0""#));
    assert!(out.contains(r#"y="12.346""#));
    assert!(!out.contains("e-") && !out.contains("e+"));
}

#[test]
fn text_fonts() {
    let st = style(|s| {
        s.font_family = Arc::from("Inter, sans-serif");
        s.font_size = 24.0;
        s.font_weight = Arc::from("bold");
    });
    let out = svg(&[text("hi", 0.0, 0.0, st)], 100.0, 100.0);
    assert!(out.contains(r#"font-family="Inter, sans-serif""#));
    assert!(out.contains(r#"font-size="24""#));
    assert!(out.contains(r#"font-weight="bold""#));
    assert!(!out.contains(r#"style="font:"#));
}

#[test]
fn shape_emission() {
    let out = svg(
        &[rect_at(5.0, 10.0, 100.0, 50.0, "corner", red())],
        200.0,
        200.0,
    );
    assert!(out.contains(r#"<rect x="5" y="10" width="100" height="50""#));
    assert!(out.contains(r##"fill="#ff0000""##));

    let mode = |x, y, w, h, m| svg(&[rect_at(x, y, w, h, m, red())], 200.0, 200.0);
    assert!(mode(100.0, 100.0, 40.0, 20.0, "center")
        .contains(r#"x="80" y="90" width="40" height="20""#));
    assert!(
        mode(10.0, 20.0, 50.0, 80.0, "corners").contains(r#"x="10" y="20" width="40" height="60""#)
    );
    assert!(mode(100.0, 100.0, 30.0, 40.0, "radius")
        .contains(r#"x="70" y="60" width="60" height="80""#));

    let e = Arc::new(SceneNode::Ellipse {
        x: 100.0,
        y: 100.0,
        w: 40.0,
        h: 20.0,
        mode: Arc::from("corner"),
        style: red(),
    });
    assert!(svg(&[e], 200.0, 200.0).contains(r#"<ellipse cx="120" cy="110" rx="20" ry="10""#));

    let line = |st: Arc<ResolvedStyle>| {
        Arc::new(SceneNode::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 100.0,
            y2: 100.0,
            style: st,
        })
    };
    let out = svg(&[line(line_default_style().clone())], 200.0, 200.0);
    assert!(out.contains(r#"<line x1="0" y1="0" x2="100" y2="100""#));
    assert!(out.contains(r##"stroke="#000000""##));
    assert!(out.contains(r#"fill="none""#));
    let out = svg(&[line(style(|s| s.stroke = None))], 200.0, 200.0);
    assert!(!out.contains("<line"));

    let tri = Arc::new(SceneNode::Triangle {
        pts: [0.0, 0.0, 10.0, 0.0, 5.0, 10.0],
        style: red(),
    });
    assert!(svg(&[tri], 100.0, 100.0).contains(r#"<polygon points="0,0 10,0 5,10""#));

    let poly = |closed, st| {
        Arc::new(SceneNode::Polygon {
            points: pts(&[0.0, 0.0, 10.0, 0.0, 10.0, 10.0]),
            closed,
            style: st,
            tension: None,
        })
    };
    assert!(svg(&[poly(true, red())], 100.0, 100.0).contains(r#"<polygon points="0,0 10,0 10,10""#));
    let open = svg(&[poly(false, red_stroke())], 100.0, 100.0);
    assert!(open.contains(r#"<polyline points="0,0 10,0 10,10""#));
    assert!(open.contains(r#"fill="none""#));

    let st = style(|s| {
        s.text_align = Arc::from("center");
        s.text_baseline = Arc::from("middle");
    });
    let out = svg(&[text("hello", 50.0, 50.0, st)], 100.0, 100.0);
    assert!(out.contains(r#"text-anchor="middle""#));
    assert!(out.contains(r#"dominant-baseline="middle""#));
    assert!(out.contains(">hello</text>"));

    let out = svg(
        &[text(
            r#"<bold> & "quoted""#,
            0.0,
            0.0,
            default_style().clone(),
        )],
        100.0,
        100.0,
    );
    assert!(out.contains(r#"&lt;bold&gt; &amp; "quoted"</text>"#));
}

#[test]
fn groups() {
    let out = svg(&[group(vec![rect()], None, None, None)], 100.0, 100.0);
    assert!(out.contains("<g>") && out.contains("<rect ") && out.contains("</g>"));

    let out = svg(
        &[group(
            vec![rect()],
            Some([2.0, 0.0, 0.0, 2.0, 10.0, 20.0]),
            None,
            None,
        )],
        100.0,
        100.0,
    );
    assert!(out.contains(r#"transform="matrix(2 0 0 2 10 20)""#));

    let blend = |b| svg(&[group(vec![rect()], None, Some(b), None)], 100.0, 100.0);
    assert!(blend("multiply").contains("mix-blend-mode:multiply"));
    assert!(!blend("destination-in").contains("mix-blend-mode"));

    let clip = |w| {
        Some(RectF {
            x: if w == 50.0 { 10.0 } else { 0.0 },
            y: if w == 50.0 { 20.0 } else { 0.0 },
            w,
            h: if w == 50.0 { 60.0 } else { w },
        })
    };
    let out = svg(&[group(vec![rect()], None, None, clip(50.0))], 100.0, 100.0);
    assert!(out.contains(r#"<clipPath id="clip0">"#));
    assert!(out.contains(r#"<rect x="10" y="20" width="50" height="60""#));
    assert!(out.contains(r#"clip-path="url(#clip0)""#));

    let out = svg(
        &[
            group(vec![rect()], None, None, clip(10.0)),
            group(vec![rect()], None, None, clip(20.0)),
        ],
        100.0,
        100.0,
    );
    for s in [
        r#"id="clip0""#,
        r#"id="clip1""#,
        "url(#clip0)",
        "url(#clip1)",
    ] {
        assert!(out.contains(s), "{s}");
    }
}

#[test]
fn style_attributes() {
    let open = Arc::new(SceneNode::Polygon {
        points: pts(&[0.0, 0.0, 10.0, 0.0]),
        closed: false,
        style: red_stroke(),
        tension: None,
    });
    let out = svg(&[open], 100.0, 100.0);
    assert!(out.contains(r#"fill="none""#));
    assert!(out.contains(r##"stroke="#0000ff""##));
    assert!(out.contains(r#"stroke-width="3""#));

    let stroked = |cap: &str, join: &str| {
        let st = style(|s| {
            s.fill = Some(Paint::css("#ff0000"));
            s.stroke = Some(Paint::css("#000"));
            s.stroke_weight = 1.0;
            s.stroke_cap = Arc::from(cap);
            s.stroke_join = Arc::from(join);
        });
        svg(&[rect_at(0.0, 0.0, 10.0, 20.0, "corner", st)], 100.0, 100.0)
    };
    let out = stroked("butt", "miter");
    assert!(!out.contains(r#"stroke-linecap="butt""#));
    assert!(!out.contains(r#"stroke-linejoin="miter""#));
    let out = stroked("round", "bevel");
    assert!(out.contains(r#"stroke-linecap="round""#));
    assert!(out.contains(r#"stroke-linejoin="bevel""#));
}

// ─── Golden fixtures ─────────────────────────────────────────────────────────

/// Decodes the fixture scene encoding (TS object shape; `$num`, `$u`,
/// `$id`/`$ref`/`$arr` for shared objects) into scene nodes, keeping shared
/// subtrees shared.
struct Decoder<'a> {
    defs: HashMap<u64, &'a J>,
    nodes: HashMap<u64, Arc<SceneNode>>,
    styles: HashMap<u64, Arc<ResolvedStyle>>,
}

impl<'a> Decoder<'a> {
    fn new(root: &'a J) -> Self {
        let mut defs = HashMap::new();
        fn walk<'a>(j: &'a J, defs: &mut HashMap<u64, &'a J>) {
            match j {
                J::Array(a) => a.iter().for_each(|x| walk(x, defs)),
                J::Object(o) => {
                    if let Some(id) = o.get("$id").and_then(J::as_u64) {
                        defs.insert(id, j);
                    }
                    o.values().for_each(|x| walk(x, defs));
                }
                _ => {}
            }
        }
        walk(root, &mut defs);
        Decoder {
            defs,
            nodes: HashMap::new(),
            styles: HashMap::new(),
        }
    }

    /// Follow a `$ref`; returns the value and its id when shared.
    fn deref(&self, j: &'a J) -> (&'a J, Option<u64>) {
        let j = match j.get("$ref").and_then(J::as_u64) {
            Some(r) => self.defs[&r],
            None => j,
        };
        (j, j.get("$id").and_then(J::as_u64))
    }

    /// Undefined, absent and null all mean "unset" to the exporter.
    fn present(&self, j: Option<&'a J>) -> Option<&'a J> {
        let j = self.deref(j?).0;
        (!j.is_null() && j.get("$u").is_none()).then_some(j)
    }

    fn array(&self, j: &'a J) -> &'a [J] {
        let j = self.deref(j).0;
        j.get("$arr")
            .unwrap_or(j)
            .as_array()
            .unwrap_or_else(|| panic!("expected array: {j}"))
    }

    fn num(&self, j: &J) -> f64 {
        match j {
            J::Number(n) => n.as_f64().unwrap(),
            _ => match j.get("$num").and_then(J::as_str) {
                Some("NaN") => f64::NAN,
                Some("Infinity") => f64::INFINITY,
                Some("-Infinity") => f64::NEG_INFINITY,
                Some("-0") => -0.0,
                _ => panic!("expected number: {j}"),
            },
        }
    }

    fn field(&self, o: &'a J, k: &str) -> &'a J {
        o.get(k).unwrap_or_else(|| panic!("missing {k:?} in {o}"))
    }

    fn f(&self, o: &J, k: &str) -> f64 {
        self.num(o.get(k).unwrap_or_else(|| panic!("missing {k:?} in {o}")))
    }

    fn s(&self, o: &J, k: &str) -> Arc<str> {
        Arc::from(
            o.get(k)
                .and_then(J::as_str)
                .unwrap_or_else(|| panic!("missing string {k:?} in {o}")),
        )
    }

    fn style(&mut self, j: &'a J) -> Arc<ResolvedStyle> {
        let (o, id) = self.deref(j);
        if let Some(st) = id.and_then(|i| self.styles.get(&i)) {
            return st.clone();
        }
        let paint = |k: &str| {
            self.present(o.get(k))
                .map(|p| Paint::css(p.as_str().unwrap_or_else(|| panic!("{k}: {p}"))))
        };
        let st = Arc::new(ResolvedStyle {
            fill: paint("fill"),
            stroke: paint("stroke"),
            stroke_weight: self.f(o, "strokeWeight"),
            stroke_cap: self.s(o, "strokeCap"),
            stroke_join: self.s(o, "strokeJoin"),
            stroke_align: self.s(o, "strokeAlign"),
            blur: self.f(o, "blur"),
            font_size: self.f(o, "fontSize"),
            font_family: self.s(o, "fontFamily"),
            font_weight: self.s(o, "fontWeight"),
            font_style: self.s(o, "fontStyle"),
            letter_spacing: self.f(o, "letterSpacing"),
            line_height: self.f(o, "lineHeight"),
            text_align: self.s(o, "textAlign"),
            text_baseline: self.s(o, "textBaseline"),
            rect_mode: self.s(o, "rectMode"),
            ellipse_mode: self.s(o, "ellipseMode"),
        });
        if let Some(i) = id {
            self.styles.insert(i, st.clone());
        }
        st
    }

    fn node(&mut self, j: &'a J) -> Arc<SceneNode> {
        let (o, id) = self.deref(j);
        if let Some(n) = id.and_then(|i| self.nodes.get(&i)) {
            return n.clone();
        }
        let kind = o.get("kind").and_then(J::as_str).expect("kind");
        let f = |k: &str| self.f(o, k);
        let node = match kind {
            "rect" | "ellipse" => {
                let (x, y, w, h, mode) = (f("x"), f("y"), f("w"), f("h"), self.s(o, "mode"));
                let style = self.style(self.field(o, "style"));
                if kind == "rect" {
                    SceneNode::Rect {
                        x,
                        y,
                        w,
                        h,
                        mode,
                        style,
                    }
                } else {
                    SceneNode::Ellipse {
                        x,
                        y,
                        w,
                        h,
                        mode,
                        style,
                    }
                }
            }
            "line" => SceneNode::Line {
                x1: f("x1"),
                y1: f("y1"),
                x2: f("x2"),
                y2: f("y2"),
                style: self.style(self.field(o, "style")),
            },
            "arc" => SceneNode::Arc {
                x: f("x"),
                y: f("y"),
                w: f("w"),
                h: f("h"),
                start: f("start"),
                stop: f("stop"),
                mode: self.s(o, "mode"),
                style: self.style(self.field(o, "style")),
            },
            "triangle" => SceneNode::Triangle {
                pts: ["x1", "y1", "x2", "y2", "x3", "y3"].map(f),
                style: self.style(self.field(o, "style")),
            },
            "quad" => SceneNode::Quad {
                pts: ["x1", "y1", "x2", "y2", "x3", "y3", "x4", "y4"].map(f),
                style: self.style(self.field(o, "style")),
            },
            "point" => SceneNode::Point {
                x: f("x"),
                y: f("y"),
                style: self.style(self.field(o, "style")),
            },
            "polygon" => {
                let points = self
                    .array(self.field(o, "points"))
                    .iter()
                    .map(|p| {
                        let p = self.deref(p).0;
                        Vec2 {
                            x: self.f(p, "x"),
                            y: self.f(p, "y"),
                        }
                    })
                    .collect();
                SceneNode::Polygon {
                    points,
                    closed: o.get("closed").and_then(J::as_bool).expect("closed"),
                    tension: self.present(o.get("tension")).map(|t| self.num(t)),
                    style: self.style(self.field(o, "style")),
                }
            }
            "text" => SceneNode::Text {
                text: self.s(o, "text"),
                x: f("x"),
                y: f("y"),
                style: self.style(self.field(o, "style")),
            },
            "group" => {
                let transform = self.present(o.get("transform")).map(|t| {
                    let a = self.array(t);
                    assert_eq!(a.len(), 6, "transform: {t}");
                    std::array::from_fn(|i| self.num(&a[i]))
                });
                let blend_mode = self
                    .present(o.get("blendMode"))
                    .map(|b| Arc::from(b.as_str().expect("blendMode")));
                let clip = self.present(o.get("clip")).map(|c| RectF {
                    x: self.f(c, "x"),
                    y: self.f(c, "y"),
                    w: self.f(c, "w"),
                    h: self.f(c, "h"),
                });
                let children = self
                    .array(self.field(o, "children"))
                    .iter()
                    .map(|c| self.node(c))
                    .collect();
                SceneNode::Group {
                    children,
                    transform,
                    blend_mode,
                    clip,
                }
            }
            other => panic!("unsupported scene kind {other:?}"),
        };
        let node = Arc::new(node);
        if let Some(i) = id {
            self.nodes.insert(i, node.clone());
        }
        node
    }
}

fn svg_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/svg")
}

fn read_gz_json(path: &Path) -> J {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut text = String::new();
    flate2::read::GzDecoder::new(file)
        .read_to_string(&mut text)
        .expect("gzip");
    serde_json::from_str(&text).expect("fixture json")
}

/// Run every case in a fixture file; returns (cases, failures).
fn check_file(path: &Path, failures: &mut Vec<String>) -> usize {
    let root = read_gz_json(path);
    assert_eq!(root["format"], 1, "{}: fixture format", path.display());
    let cases = root["cases"].as_array().expect("cases");
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let mut dec = Decoder::new(case);
        let layers: Vec<Arc<SceneNode>> = dec
            .array(&case["layers"])
            .iter()
            .map(|l| dec.node(l))
            .collect();
        let background = dec
            .present(case.get("background"))
            .map(|b| b.as_str().unwrap());
        let opts = SvgExportOptions {
            width: dec.num(&case["width"]),
            height: dec.num(&case["height"]),
            background,
        };
        let got = scene_to_svg(&layers, &opts);
        let want = case["svg"].as_str().expect("svg");
        if got != want {
            let at = got
                .bytes()
                .zip(want.bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(got.len().min(want.len()));
            let ctx = |s: &str| {
                let (mut lo, mut hi) = (at.saturating_sub(80), (at + 80).min(s.len()));
                while !s.is_char_boundary(lo) {
                    lo -= 1;
                }
                while !s.is_char_boundary(hi) {
                    hi += 1;
                }
                s[lo..hi].to_string()
            };
            failures.push(format!(
                "{name}: differs at byte {at}\n  rust: …{}…\n  ts:   …{}…",
                ctx(&got),
                ctx(want)
            ));
        }
    }
    cases.len()
}

#[test]
fn svg_matches_typescript_goldens() {
    let dir = svg_fixture_dir();
    let mut files = vec![dir.join("edge-cases.json.gz")];
    let mut samples: Vec<PathBuf> = std::fs::read_dir(dir.join("samples"))
        .expect("native/fixtures/svg/samples")
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".json.gz"))
        .collect();
    samples.sort();
    assert!(!samples.is_empty(), "no sample fixtures");
    files.extend(samples);

    let mut failures = Vec::new();
    let total: usize = files.iter().map(|f| check_file(f, &mut failures)).sum();
    println!(
        "svg goldens: {}/{total} byte-identical across {} files",
        total - failures.len(),
        files.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {total} SVG goldens differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
