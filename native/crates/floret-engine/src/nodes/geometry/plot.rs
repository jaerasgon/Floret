//! `geometry/plot` (Plot): a list of numbers as a chart inside a box — a
//! line through the values, the area under them closed down to a baseline
//! (mountain ridges, filled waveforms) or one bar per value.
//!
//! Values run left to right across `size` (centred on `center`); the value
//! range maps bottom to top (the canvas's y grows downward, so larger
//! values are higher up). The range is the values' own (`auto`), the same
//! but always including 0 (`zero`), or `min` / `max` (`fixed`; values
//! outside it go past the box). `baseline` (a value; clamped into the
//! range) is where areas close and bars start. NaNs plot at the baseline.

use super::{num_pin, pin_style, style_pin, vec2_pin, Pins, MAX_POINTS};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::SceneNode;
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Spread, Value, Vec2};
use std::sync::Arc;

#[cfg(test)]
const MODES: [&str; 3] = ["line", "area", "bars"];
#[cfg(test)]
const RANGES: [&str; 3] = ["auto", "zero", "fixed"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Line,
    Area,
    Bars,
}

fn mode_of(params: &Params) -> Mode {
    match super::param_str(params, "mode", "line") {
        "area" => Mode::Area,
        "bars" => Mode::Bars,
        _ => Mode::Line,
    }
}

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mode = mode_of(params);
    let mut range = PortSpec::new("range", PortType::String).default(Value::str("auto"));
    range.param = Some("range".into());
    let mut v = vec![
        PortSpec::new("values", PortType::Number).kind(PortKind::Spread),
        vec2_pin("center", 0.0, 0.0),
        vec2_pin("size", 400.0, 200.0),
        range,
    ];
    if super::param_str(params, "range", "auto") == "fixed" {
        v.push(num_pin("min", 0.0));
        v.push(num_pin("max", 1.0));
    }
    if mode != Mode::Line {
        v.push(num_pin("baseline", 0.0));
    }
    if mode == Mode::Bars {
        v.push(num_pin("gap", 0.2));
    }
    v.push(style_pin());
    v
}

/// The value range (`lo`, `hi`) for the finite values.
fn range_of(values: &[f64], range: &str, min: f64, max: f64) -> (f64, f64) {
    if range == "fixed" {
        return (min, max);
    }
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let (mut lo, mut hi) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
        (a.min(v), b.max(v))
    });
    if lo > hi {
        (lo, hi) = (0.0, 1.0);
    }
    if range == "zero" {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    (lo, hi)
}

pub(super) fn plot(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    let mode = mode_of(init.params);
    raw(move |inputs, cx| {
        let mut values: Vec<f64> = pins
            .spread(inputs, "values")
            .iter()
            .map(|v| match v {
                Value::Num(x) => *x,
                v => v.to_number(),
            })
            .collect();
        if values.len() > MAX_POINTS {
            cx.warn(format!(
                "{} values, over {MAX_POINTS}: the rest are left out.",
                values.len()
            ));
            values.truncate(MAX_POINTS);
        }
        let range = match pins.value(inputs, "range") {
            Value::Str(s) => s.to_string(),
            _ => cx
                .param("range")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_owned(),
        };
        let center = pins.vec2(inputs, "center", Vec2::default());
        let size = pins.vec2(inputs, "size", Vec2 { x: 400.0, y: 200.0 });
        let (lo, hi) = range_of(
            &values,
            &range,
            pins.num(inputs, "min", 0.0),
            pins.num(inputs, "max", 1.0),
        );
        let (left, bottom) = (center.x - size.x / 2.0, center.y + size.y / 2.0);
        // Value → y; a flat range sits in the middle.
        let y_of = |v: f64| {
            if hi == lo {
                center.y
            } else {
                bottom - (v - lo) / (hi - lo) * size.y
            }
        };
        let (rlo, rhi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let base_v = pins.num(inputs, "baseline", 0.0).clamp(rlo, rhi);
        let base_y = y_of(base_v);
        let n = values.len();
        let gap = pins.num(inputs, "gap", 0.2).clamp(0.0, 1.0);
        let x_of = |i: usize| {
            if mode == Mode::Bars {
                left + (i as f64 + 0.5) / n as f64 * size.x
            } else if n > 1 {
                left + i as f64 / (n - 1) as f64 * size.x
            } else {
                center.x
            }
        };
        let tops: Vec<Vec2> = values
            .iter()
            .enumerate()
            .map(|(i, v)| Vec2 {
                x: x_of(i),
                y: if v.is_finite() { y_of(*v) } else { base_y },
            })
            .collect();
        let bases: Vec<Vec2> = tops.iter().map(|p| Vec2 { x: p.x, y: base_y }).collect();
        let style_in = pins.value(inputs, "style");
        let shape = match mode {
            Mode::Line => SceneNode::Polygon {
                points: Arc::from(tops.clone()),
                closed: false,
                style: super::as_line(&pin_style(&style_in, false)),
                tension: None,
            },
            Mode::Area => {
                let mut ring = tops.clone();
                if let (Some(first), Some(last)) = (tops.first(), tops.last()) {
                    ring.push(Vec2 {
                        x: last.x,
                        y: base_y,
                    });
                    ring.push(Vec2 {
                        x: first.x,
                        y: base_y,
                    });
                }
                SceneNode::Polygon {
                    points: Arc::from(ring),
                    closed: true,
                    style: pin_style(&style_in, true),
                    tension: None,
                }
            }
            Mode::Bars => {
                let style = pin_style(&style_in, true);
                let half = size.x / n.max(1) as f64 * (1.0 - gap) / 2.0;
                let bars = tops
                    .iter()
                    .map(|p| {
                        Arc::new(SceneNode::Polygon {
                            points: Arc::from(vec![
                                Vec2 {
                                    x: p.x - half,
                                    y: base_y,
                                },
                                Vec2 {
                                    x: p.x + half,
                                    y: base_y,
                                },
                                Vec2 {
                                    x: p.x + half,
                                    y: p.y,
                                },
                                Vec2 {
                                    x: p.x - half,
                                    y: p.y,
                                },
                            ]),
                            closed: true,
                            style: style.clone(),
                            tension: None,
                        })
                    })
                    .collect();
                SceneNode::group(bars)
            }
        };
        let pts = |v: Vec<Vec2>| -> Spread { v.into_iter().map(Value::Vec2).collect() };
        Ok(vec![
            one(Value::Scene(Arc::new(shape))),
            pts(tops),
            pts(bases),
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::{n, node};
    use serde_json::json;

    /// (shape, points, bases)
    fn run(params: serde_json::Value, given: &[(&str, Vec<Value>)]) -> Vec<Vec<Value>> {
        let p: Params = serde_json::from_value(params.clone()).unwrap();
        let ins: Vec<Vec<Value>> = inputs(&p)
            .iter()
            .map(|pin| {
                given
                    .iter()
                    .find(|(k, _)| *k == pin.name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| pin.default.clone().into_iter().collect())
            })
            .collect();
        node("geometry/plot", params).eval(ins).unwrap()
    }

    fn pts(v: &[Value]) -> Vec<(f64, f64)> {
        v.iter()
            .map(|v| match v {
                Value::Vec2(p) => (p.x, p.y),
                _ => panic!("point expected"),
            })
            .collect()
    }

    fn vals(xs: &[f64]) -> Vec<Value> {
        xs.iter().map(|x| n(*x)).collect()
    }

    #[test]
    fn line_spans_the_box_bottom_to_top() {
        // Box 400 × 200 at the origin: x −200..200, y 100 (bottom) .. −100.
        let out = run(json!({}), &[("values", vals(&[2.0, 4.0, 3.0]))]);
        assert_eq!(pts(&out[1]), [(-200.0, 100.0), (0.0, -100.0), (200.0, 0.0)]);
        let Value::Scene(s) = &out[0][0] else {
            panic!()
        };
        assert!(matches!(**s, SceneNode::Polygon { closed: false, .. }));
    }

    #[test]
    fn ranges() {
        let v = vals(&[2.0, 4.0]);
        let zero = run(
            json!({"range": "zero"}),
            &[("values", v.clone()), ("range", vec![Value::str("zero")])],
        );
        // 0..4: 2 is halfway up.
        assert_eq!(pts(&zero[1])[0].1, 0.0);
        let fixed = run(
            json!({"range": "fixed"}),
            &[
                ("values", v.clone()),
                ("range", vec![Value::str("fixed")]),
                ("min", vec![n(0.0)]),
                ("max", vec![n(2.0)]),
            ],
        );
        // Past the top of a fixed range: past the box.
        assert_eq!(pts(&fixed[1]), [(-200.0, -100.0), (200.0, -300.0)]);
        // A flat series sits in the middle.
        let flat = run(json!({}), &[("values", vals(&[5.0, 5.0]))]);
        assert_eq!(pts(&flat[1])[1].1, 0.0);
    }

    #[test]
    fn area_closes_to_the_baseline() {
        let out = run(
            json!({"mode": "area"}),
            &[("values", vals(&[-1.0, 1.0])), ("baseline", vec![n(0.0)])],
        );
        let Value::Scene(s) = &out[0][0] else {
            panic!()
        };
        let SceneNode::Polygon { points, closed, .. } = &**s else {
            panic!()
        };
        assert!(*closed);
        assert_eq!(points.len(), 4);
        assert_eq!((points[2].x, points[2].y), (200.0, 0.0));
        assert_eq!((points[3].x, points[3].y), (-200.0, 0.0));
        assert_eq!(pts(&out[2]), [(-200.0, 0.0), (200.0, 0.0)]);
        // A baseline outside the range clamps to the box edge.
        let out = run(
            json!({"mode": "area"}),
            &[("values", vals(&[5.0, 6.0])), ("baseline", vec![n(0.0)])],
        );
        assert_eq!(pts(&out[2])[0].1, 100.0);
    }

    #[test]
    fn bars_fill_slots() {
        let out = run(
            json!({"mode": "bars"}),
            &[
                ("values", vals(&[1.0, 2.0])),
                ("gap", vec![n(0.5)]),
                ("baseline", vec![n(0.0)]),
            ],
        );
        assert_eq!(pts(&out[1]), [(-100.0, 100.0), (100.0, -100.0)]);
        let Value::Scene(s) = &out[0][0] else {
            panic!()
        };
        let SceneNode::Group { children, .. } = &**s else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        let SceneNode::Polygon { points, .. } = &*children[1] else {
            panic!()
        };
        // Slot 200 wide, half gap: 100 wide.
        assert_eq!(points[1].x - points[0].x, 100.0);
    }

    #[test]
    fn empty_nan_and_one_value() {
        let out = run(json!({"mode": "area"}), &[("values", vec![])]);
        assert!(out[1].is_empty());
        let out = run(json!({}), &[("values", vals(&[f64::NAN, 1.0, 3.0]))]);
        // NaN sits at the baseline (clamped to the bottom here).
        assert_eq!(pts(&out[1])[0], (-200.0, 100.0));
        let out = run(json!({}), &[("values", vals(&[7.0]))]);
        assert_eq!(pts(&out[1]), [(0.0, 0.0)]);
        // Zero size: everything on the centre.
        let out = run(
            json!({"mode": "bars"}),
            &[
                ("values", vals(&[1.0, 2.0])),
                ("size", vec![Value::Vec2(Vec2::default())]),
            ],
        );
        assert!(pts(&out[1]).iter().all(|p| *p == (0.0, 0.0)));
    }

    #[test]
    fn static_spec_matches_default_mode() {
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("geometry/plot").unwrap();
        let st: Vec<String> = spec.inputs.iter().map(|p| p.name.clone()).collect();
        let dy: Vec<String> = inputs(&spec.default_params)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(st, dy);
        assert_eq!(spec.param_options["mode"], MODES);
        assert_eq!(spec.param_options["range"], RANGES);
    }
}
