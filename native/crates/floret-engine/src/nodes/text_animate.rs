//! `text/animate` (Text Animate): per-glyph animation of text, without a
//! ForEach.
//!
//! Reads the text of a wired layer: `draw/text`, `text/glyphs` or
//! `text/on-path` runs (inside groups too). Every glyph gets a share of
//! `progress` (0..1), staggered across the text in `order`:
//!
//! - `in`: each glyph eases from its start state (moved by `offset`,
//!   turned by `rotate`, at `scale` and `opacity`) to where the text puts
//!   it. `stagger` 0 moves them all together, 1 one after another.
//!   Run `progress` from 1 to 0 to animate out.
//! - `wave`: `progress` is a phase; each glyph swings by `offset` /
//!   `rotate` (and towards `scale` / `opacity`) on a sine, `stagger`
//!   cycles apart from the first glyph to the last. Loop `progress` 0..1.
//!
//! Glyphs turn and scale about their own centre. `amounts` gives each
//! glyph's eased progress (`in`) or wave (`wave`, -1..1), for driving
//! anything else per glyph.

use super::geometry::fade_paint;
use super::random::hash32;
use super::shared::to_vec2;
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{self, Affine, GlyphRun, PlacedGlyph, ResolvedStyle, SceneNode};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("text/animate", animate);
    r.dynamic("text/animate", inputs);
}

pub const ORDERS: [&str; 5] = ["forward", "backward", "center", "edges", "random"];

fn param_str<'a>(params: &'a Params, key: &str, fallback: &'a str) -> &'a str {
    params.get(key).and_then(|v| v.as_str()).unwrap_or(fallback)
}

fn num(name: &str, default: f64) -> PortSpec {
    PortSpec::new(name, PortType::Number).default(Value::Num(default))
}

fn choice(name: &str, default: &str) -> PortSpec {
    let mut p = PortSpec::new(name, PortType::String).default(Value::str(default));
    p.param = Some(name.into());
    p
}

fn inputs(params: &Params) -> Vec<PortSpec> {
    let wave = param_str(params, "mode", "in") == "wave";
    let mut v = vec![
        PortSpec::new("text", PortType::Layer).kind(PortKind::Spread),
        num("progress", if wave { 0.0 } else { 0.5 }),
        num("stagger", if wave { 1.0 } else { 0.6 }),
        choice("order", "forward"),
    ];
    if !wave {
        v.push(choice("ease", "outCubic"));
    }
    v.push(
        PortSpec::new("offset", PortType::Vec2).default(Value::Vec2(Vec2 {
            x: 0.0,
            y: if wave { 12.0 } else { 40.0 },
        })),
    );
    v.push(num("rotate", 0.0));
    v.push(num("scale", 1.0));
    v.push(num("opacity", if wave { 1.0 } else { 0.0 }));
    let mut seed = num("seed", 1.0);
    seed.integer = true;
    v.push(seed);
    v
}

/// One glyph of the text, where the text puts it.
#[derive(Clone, Debug)]
pub(crate) struct Glyph {
    pub glyph: PlacedGlyph,
    pub size: f64,
    pub style: Arc<ResolvedStyle>,
    /// The enclosing groups' transform.
    pub at: Affine,
}

/// Every glyph of the layers, in reading order.
pub(crate) fn glyphs(layers: &[Value]) -> Vec<Glyph> {
    fn walk(n: &SceneNode, m: &Affine, out: &mut Vec<Glyph>) {
        match n {
            SceneNode::Group {
                children,
                transform,
                ..
            } => {
                let gm = transform.map_or(*m, |t| scene::mul(m, &t));
                for c in children.iter() {
                    walk(c, &gm, out);
                }
            }
            SceneNode::Glyphs(run) => out.extend(run.glyphs.iter().map(|g| Glyph {
                glyph: *g,
                size: run.size,
                style: run.style.clone(),
                at: *m,
            })),
            SceneNode::Text { text, x, y, style } => {
                // Laid out as `draw/text` draws it (as `text/glyphs` does).
                if let Some(b) = crate::text::block(text, style, style.letter_spacing) {
                    let (ox, oy) = b.origin(style, *x, *y);
                    out.extend(b.glyphs.iter().map(|g| Glyph {
                        glyph: PlacedGlyph {
                            face: g.glyph.face,
                            id: g.glyph.id,
                            x: ox + g.glyph.x,
                            y: oy + g.glyph.y,
                            angle: 0.0,
                        },
                        size: style.font_size,
                        style: style.clone(),
                        at: *m,
                    }));
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for v in layers {
        if let Value::Scene(n) = v {
            walk(n, &scene::IDENTITY, &mut out);
        }
    }
    out
}

/// Each glyph's place in the order, 0 (first) to 1 (last).
pub(crate) fn ranks(n: usize, order: &str, seed: f64) -> Vec<f64> {
    if n <= 1 {
        return vec![0.0; n];
    }
    let last = (n - 1) as f64;
    let half = last / 2.0;
    match order {
        "backward" => (0..n).map(|i| 1.0 - i as f64 / last).collect(),
        "center" => (0..n).map(|i| (i as f64 - half).abs() / half).collect(),
        "edges" => (0..n)
            .map(|i| 1.0 - (i as f64 - half).abs() / half)
            .collect(),
        "random" => {
            let seed = crate::seed::mix(seed);
            let mut idx: Vec<usize> = (0..n).collect();
            idx.sort_by_key(|&i| (hash32(i as f64, 7.0, seed), i));
            let mut r = vec![0.0; n];
            for (k, &i) in idx.iter().enumerate() {
                r[i] = k as f64 / last;
            }
            r
        }
        _ => (0..n).map(|i| i as f64 / last).collect(),
    }
}

/// `in`: glyph progress for overall `p`, of `n` glyphs, `rank` 0..1
/// through the order. Each glyph takes a window of `1 / (1 + stagger ×
/// (n − 1))`, so stagger 0 runs them all at once and 1 one after another,
/// the last ending exactly at 1.
pub(crate) fn local(p: f64, rank: f64, stagger: f64, n: usize) -> f64 {
    let s = stagger.clamp(0.0, 1.0);
    let steps = n.saturating_sub(1) as f64;
    let window = 1.0 / (1.0 + s * steps);
    ((p - rank * steps * s * window) / window).clamp(0.0, 1.0)
}

/// The glyph's centre, relative to its origin (unturned).
fn pivot(g: &Glyph) -> Vec2 {
    let run = GlyphRun {
        glyphs: Arc::from([PlacedGlyph {
            x: 0.0,
            y: 0.0,
            angle: 0.0,
            ..g.glyph
        }]),
        size: g.size,
        style: g.style.clone(),
    };
    match crate::text::outline::bounds(&crate::text::outline::run_contours(&run)) {
        Some((lo, hi)) => Vec2 {
            x: (lo.x + hi.x) / 2.0,
            y: (lo.y + hi.y) / 2.0,
        },
        None => Vec2::default(),
    }
}

fn faded(st: &Arc<ResolvedStyle>, k: f64) -> Arc<ResolvedStyle> {
    if k >= 1.0 {
        return st.clone();
    }
    let mut s = (**st).clone();
    s.fill = s.fill.as_ref().map(|p| fade_paint(p, k));
    s.stroke = s.stroke.as_ref().map(|p| fade_paint(p, k));
    Arc::new(s)
}

fn animate(init: &NodeInit) -> Box<dyn NodeImpl> {
    let names: Vec<String> = init.inputs.iter().map(|p| p.name.clone()).collect();
    raw(move |inputs, cx| {
        let pin = |name: &str| -> Value {
            names
                .iter()
                .position(|n| n == name)
                .and_then(|i| inputs.get(i))
                .and_then(|s| s.first().cloned())
                .unwrap_or(Value::Undefined)
        };
        let n = |name: &str, fallback: f64| {
            let v = pin(name).to_number();
            if v.is_finite() {
                v
            } else {
                fallback
            }
        };
        let s = |name: &str, fallback: &str| match pin(name) {
            Value::Str(s) => s.to_string(),
            _ => fallback.to_owned(),
        };
        let wave = matches!(cx.param("mode"), Some(serde_json::Value::String(m)) if m == "wave");
        let text = names
            .iter()
            .position(|n| n == "text")
            .and_then(|i| inputs.get(i))
            .map_or(&[][..], |s| &s[..]);
        let all = glyphs(text);
        let progress = n("progress", if wave { 0.0 } else { 0.5 });
        let stagger = n("stagger", if wave { 1.0 } else { 0.6 });
        let ease = s("ease", "outCubic");
        let offset = match pin("offset") {
            v @ (Value::Vec2(_) | Value::Object(_)) => to_vec2(&v),
            _ => Vec2 {
                x: 0.0,
                y: if wave { 12.0 } else { 40.0 },
            },
        };
        let rotate = n("rotate", 0.0);
        let scale = n("scale", 1.0);
        let opacity = n("opacity", if wave { 1.0 } else { 0.0 }).clamp(0.0, 1.0);
        let count = all.len();
        let ranks = ranks(count, &s("order", "forward"), n("seed", 1.0));
        let mut layers = Vec::with_capacity(all.len());
        let mut amounts = Vec::with_capacity(all.len());
        for (g, rank) in all.iter().zip(ranks) {
            // `away`: how far from rest (0..1) for scale / opacity; `swing`
            // the signed amount for offset / rotate.
            let (amount, away, swing) = if wave {
                let th = std::f64::consts::TAU * (progress - rank * stagger);
                let s = th.sin();
                (s, (1.0 - th.cos()) / 2.0, s)
            } else {
                let e = super::motion::ease_fn(&ease, local(progress, rank, stagger, count));
                (e, 1.0 - e, 1.0 - e)
            };
            amounts.push(Value::Num(amount));
            let k = 1.0 + (scale - 1.0) * away;
            let alpha = 1.0 + (opacity - 1.0) * away;
            if alpha <= 0.0 || k == 0.0 {
                continue;
            }
            let c = pivot(g);
            let (gx, gy) = (g.glyph.x + offset.x * swing, g.glyph.y + offset.y * swing);
            // Group transforms, then to the glyph's origin and turn, then
            // about its centre.
            let m = [
                g.at,
                scene::translate_mat(gx, gy),
                scene::rotate_mat(g.glyph.angle),
                scene::translate_mat(c.x, c.y),
                scene::rotate_mat(rotate * swing),
                scene::scale_mat(k, k),
                scene::translate_mat(-c.x, -c.y),
            ]
            .iter()
            .fold(scene::IDENTITY, |acc, t| scene::mul(&acc, t));
            let run = GlyphRun {
                glyphs: Arc::from([PlacedGlyph {
                    x: 0.0,
                    y: 0.0,
                    angle: 0.0,
                    ..g.glyph
                }]),
                size: g.size,
                style: faded(&g.style, alpha),
            };
            layers.push(Arc::new(SceneNode::Group {
                children: Arc::from([Arc::new(SceneNode::Glyphs(Arc::new(run)))]),
                transform: Some(m),
                blend_mode: None,
                clip: None,
            }));
        }
        Ok(vec![
            crate::value::one(Value::Scene(Arc::new(SceneNode::group(layers.clone())))),
            Spread::from(layers.into_iter().map(Value::Scene).collect::<Vec<_>>()),
            Spread::from(amounts),
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::node;
    use crate::scene::default_style;
    use serde_json::json;

    fn text_layer(s: &str, x: f64, y: f64) -> Value {
        let style = ResolvedStyle {
            font_size: 40.0,
            text_align: Arc::from("center"),
            ..(**default_style()).clone()
        };
        Value::Scene(Arc::new(SceneNode::Text {
            text: Arc::from(s),
            x,
            y,
            style: Arc::new(style),
        }))
    }

    /// Each output glyph layer's transform (translation part) and alpha.
    fn placed(out: &[Value]) -> Vec<(f64, f64, f64)> {
        out.iter()
            .map(|v| match v {
                Value::Scene(n) => match &**n {
                    SceneNode::Group {
                        transform: Some(m),
                        children,
                        ..
                    } => {
                        let SceneNode::Glyphs(run) = &*children[0] else {
                            panic!("not glyphs")
                        };
                        let a = run.style.fill.as_ref().map_or(1.0, |p| p.rgba[3]);
                        (m[4], m[5], a)
                    }
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn ranks_follow_the_order() {
        assert_eq!(ranks(3, "forward", 1.0), [0.0, 0.5, 1.0]);
        assert_eq!(ranks(3, "backward", 1.0), [1.0, 0.5, 0.0]);
        assert_eq!(ranks(5, "center", 1.0), [1.0, 0.5, 0.0, 0.5, 1.0]);
        assert_eq!(ranks(5, "edges", 1.0), [0.0, 0.5, 1.0, 0.5, 0.0]);
        let mut r = ranks(6, "random", 3.0);
        assert_eq!(ranks(6, "random", 3.0), r);
        r.sort_by(f64::total_cmp);
        assert_eq!(r, [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        assert!(ranks(0, "forward", 1.0).is_empty());
        assert_eq!(ranks(1, "center", 1.0), [0.0]);
    }

    #[test]
    fn local_progress_staggers() {
        // Together: every glyph at p.
        assert_eq!(local(0.3, 1.0, 0.0, 5), 0.3);
        // Three glyphs, half staggered: windows of 1/2, starting 0, 1/4, 1/2.
        assert_eq!(local(0.5, 1.0, 0.5, 3), 0.0);
        assert_eq!(local(0.75, 1.0, 0.5, 3), 0.5);
        assert_eq!(local(1.0, 1.0, 0.5, 3), 1.0);
        assert_eq!(local(0.25, 0.0, 0.5, 3), 0.5);
        // Fully staggered: one after another, a third each.
        assert!((local(0.5, 0.5, 1.0, 3) - 0.5).abs() < 1e-12);
        assert_eq!(local(1.0, 1.0, 1.0, 3), 1.0);
        assert_eq!(local(0.6, 1.0, 1.0, 3), 0.0);
        // Out-of-range p clamps; one glyph or none is fine.
        assert_eq!(local(-3.0, 0.0, 0.2, 4), 0.0);
        assert_eq!(local(9.0, 0.7, 0.2, 4), 1.0);
        assert_eq!(local(0.4, 0.0, 1.0, 1), 0.4);
        assert_eq!(local(0.4, 0.0, 1.0, 0), 0.4);
    }

    /// Inputs (in): text, progress, stagger, order, ease, offset, rotate,
    /// scale, opacity, seed.
    fn run_in(text: Vec<Value>, progress: f64, stagger: f64) -> Vec<Vec<Value>> {
        let mut n = node("text/animate", json!({ "mode": "in" }));
        n.eval(vec![
            text,
            vec![Value::Num(progress)],
            vec![Value::Num(stagger)],
            vec![Value::str("forward")],
            vec![Value::str("linear")],
            vec![Value::Vec2(Vec2 { x: 0.0, y: 40.0 })],
            vec![],
            vec![],
            vec![Value::Num(0.0)],
            vec![],
        ])
        .unwrap()
    }

    #[test]
    fn glyphs_ease_in_one_after_another() {
        let text = vec![text_layer("abc", 0.0, 0.0)];
        let rest = placed(&run_in(text.clone(), 1.0, 0.5)[1]);
        assert_eq!(rest.len(), 3);
        assert!(rest.iter().all(|g| g.2 == 1.0));
        // Their baseline positions match the laid-out text.
        let laid = glyphs(&text);
        for (g, r) in laid.iter().zip(&rest) {
            assert!((g.glyph.x - r.0).abs() < 1e-9 && (g.glyph.y - r.1).abs() < 1e-9);
        }
        // Halfway with stagger 0.5: the first glyph is done, the last
        // hasn't started (fully transparent: left out).
        let out = run_in(text.clone(), 0.5, 0.5);
        let mid = placed(&out[1]);
        assert_eq!(mid.len(), 2);
        assert!((mid[0].1 - rest[0].1).abs() < 1e-9);
        assert!((mid[1].1 - rest[1].1 - 20.0).abs() < 1e-9, "{mid:?}");
        assert!((mid[1].2 - 0.5).abs() < 0.01);
        let amounts: Vec<f64> = out[2].iter().map(Value::to_number).collect();
        assert_eq!(amounts, [1.0, 0.5, 0.0]);
        // At 0 nothing shows.
        assert!(run_in(text, 0.0, 0.5)[1].is_empty());
    }

    #[test]
    fn wave_swings_and_edge_cases() {
        // Nothing wired / no text: empty outputs, no panic.
        let out = run_in(vec![], 0.5, 0.5);
        assert!(out[1].is_empty() && out[2].is_empty());
        let out = run_in(vec![text_layer("", 0.0, 0.0), Value::Num(3.0)], 0.5, 0.5);
        assert!(out[1].is_empty());
        // Wave: text, progress, stagger, order, offset, rotate, scale,
        // opacity, seed.
        let mut n = node("text/animate", json!({ "mode": "wave" }));
        let out = n
            .eval(vec![
                vec![text_layer("ab", 0.0, 0.0)],
                vec![Value::Num(0.25)],
                vec![Value::Num(0.5)],
                vec![],
                vec![Value::Vec2(Vec2 { x: 0.0, y: 10.0 })],
                vec![],
                vec![],
                vec![],
                vec![],
            ])
            .unwrap();
        let amounts: Vec<f64> = out[2].iter().map(Value::to_number).collect();
        // First glyph at the top of its swing, the second half a cycle on.
        assert!((amounts[0] - 1.0).abs() < 1e-9 && (amounts[1] + 1.0).abs() < 1e-9);
        let rest = glyphs(&[text_layer("ab", 0.0, 0.0)]);
        let g = placed(&out[1]);
        assert!((g[0].1 - rest[0].glyph.y - 10.0).abs() < 1e-9);
        assert!((g[1].1 - rest[1].glyph.y + 10.0).abs() < 1e-9);
        // Pins: no ease in wave mode.
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("text/animate").unwrap();
        let p: Params = serde_json::from_value(json!({ "mode": "wave" })).unwrap();
        assert!(!cat
            .resolved_inputs(spec, &p)
            .iter()
            .any(|p| p.name == "ease"));
        let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
        let ins: Vec<String> = cat
            .resolved_inputs(spec, &Params::new())
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(statics, ins);
    }

    #[test]
    fn glyph_runs_and_groups_are_read_too() {
        let run = GlyphRun {
            glyphs: Arc::from(
                glyphs(&[text_layer("xy", 0.0, 0.0)])
                    .iter()
                    .map(|g| g.glyph)
                    .collect::<Vec<_>>(),
            ),
            size: 40.0,
            style: default_style().clone(),
        };
        let moved = Value::Scene(Arc::new(SceneNode::Group {
            children: Arc::from([Arc::new(SceneNode::Glyphs(Arc::new(run)))]),
            transform: Some(scene::translate_mat(100.0, 0.0)),
            blend_mode: None,
            clip: None,
        }));
        let found = glyphs(&[moved]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].at[4], 100.0);
    }
}
