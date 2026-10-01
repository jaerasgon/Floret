//! `input/*` nodes. Ported from `src/nodes/input/`.
//!
//! `input/mouse` and `input/keyboard` read the evaluation frame. MIDI,
//! camera and landmarks read the app's devices (`crate::devices`); with
//! none installed they emit what the TS nodes did without browser media.

use super::Registrar;
use crate::devices::{self, CameraRequest, LandmarkLayer, LandmarksRequest};
use crate::landmarks::Tracker;
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit};
use crate::scene::{line_default_style, resolve_style, Paint, ResolvedStyle, SceneNode};
use crate::value::{one, Color, Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("input/mouse", mouse);
    r.node("input/keyboard", keyboard);
    r.node("input/midi-cc", midi_cc);
    r.node("input/midi-note", midi_note);
    r.node("input/camera", camera);
    r.node("input/landmarks", landmarks);
}

fn mouse(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Num(f.mouse_x)),
            one(Value::Num(f.mouse_y)),
            one(Value::Bool(f.mouse_is_pressed)),
        ])
    })
}

fn keyboard(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Bool(f.key_is_pressed)),
            one(Value::str(&f.key)),
        ])
    })
}

/// `params.param<string>(key, '')` for a device id.
fn param_str(cx: &EvalCtx, key: &str) -> String {
    match cx.param_value(key) {
        Value::Str(s) => s.to_string(),
        Value::Undefined | Value::Null => String::new(),
        other => js_string(&other),
    }
}

fn js_string(v: &Value) -> String {
    match v {
        Value::Num(n) => crate::js::num_to_string(*n),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// `clampChannel` / `clampCC`: finite numbers floored into `0..=max`,
/// anything else 0.
fn param_clamped(cx: &EvalCtx, key: &str, max: f64) -> u8 {
    match cx.param_value(key) {
        Value::Num(n) if n.is_finite() => n.floor().clamp(0.0, max) as u8,
        _ => 0,
    }
}

/// `param === true || param === 'true'`.
fn param_flag(cx: &EvalCtx, key: &str, default: bool) -> bool {
    match cx.param_value(key) {
        Value::Undefined => default,
        Value::Bool(b) => b,
        Value::Str(s) => &*s == "true",
        _ => false,
    }
}

fn num(n: f64) -> Spread {
    one(Value::Num(n))
}

/// `getCC` → `[value 0..1, raw 0..127]`, `[0, 0]` until a CC arrives.
fn midi_cc(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let device = param_str(cx, "device");
        let channel = param_clamped(cx, "channel", 16.0);
        let cc = param_clamped(cx, "cc", 127.0);
        let raw = devices::installed().and_then(|d| d.midi_cc(&device, channel, cc));
        let v = raw.map_or(0.0, f64::from);
        Ok(vec![num(v / 127.0), num(v)])
    })
}

/// `[note, velocity 0..1, pressed, freq]`; `[0, 0, pressed, 0]` with no
/// held note.
fn midi_note(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let device = param_str(cx, "device");
        let channel = param_clamped(cx, "channel", 16.0);
        let state = devices::installed()
            .map(|d| d.midi_note(&device, channel))
            .unwrap_or_default();
        let pressed = one(Value::Bool(state.pressed));
        Ok(match state.note {
            None => vec![num(0.0), num(0.0), pressed, num(0.0)],
            Some(note) => {
                let note = f64::from(note);
                vec![
                    num(note),
                    num(f64::from(state.velocity) / 127.0),
                    pressed,
                    num(440.0 * 2f64.powf((note - 69.0) / 12.0)),
                ]
            }
        })
    })
}

/// `[image, ready, width, height]` from the app's camera; an error (no
/// camera support, no device, permission denied) empties the outputs.
fn camera(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let start = inputs
            .first()
            .and_then(|s| s.first())
            .is_some_and(Value::truthy);
        let device = param_str(cx, "device");
        let mirror = param_flag(cx, "mirror", false);
        let dev = devices::installed().ok_or("camera capture isn't available")?;
        let req = CameraRequest {
            device: &device,
            start,
            mirror,
        };
        let f = dev.camera(cx.node_id, &req)?;
        let (w, h) = if f.ready {
            (f64::from(f.image.width()), f64::from(f.image.height()))
        } else {
            (0.0, 0.0)
        };
        Ok(vec![
            one(Value::Image(f.image)),
            one(Value::Bool(f.ready)),
            num(w),
            num(h),
        ])
    })
}

/// Fixed per-person palette (0..1), cycled by stable id: yellow, green,
/// blue, orange, magenta, cyan.
const PALETTE: [[f64; 3]; 6] = [
    [1.00, 0.84, 0.00],
    [0.24, 0.86, 0.35],
    [0.27, 0.51, 1.00],
    [1.00, 0.55, 0.16],
    [0.86, 0.31, 0.90],
    [0.16, 0.86, 0.90],
];

fn color_for_id(id: i64) -> Color {
    let [r, g, b] = PALETTE[id.rem_euclid(PALETTE.len() as i64) as usize];
    Color { r, g, b, a: 1.0 }
}

/// Hand / face / pose tracking. Outputs, in order: points, z, group,
/// color, connA, connB, connColor, guides, people, personColors,
/// detections, perGroup, detected.
fn landmarks(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let first = |i: usize| inputs.get(i).and_then(|s| s.first());
        let image = match first(0) {
            Some(Value::Image(img)) if img.is_ready() => Some(img.clone()),
            _ => None,
        };
        let start = first(1).is_some_and(Value::truthy);
        let size = match first(2) {
            None | Some(Value::Undefined | Value::Null) => 500.0,
            Some(v) => v.to_number(),
        };
        let size = if size.is_nan() || size == 0.0 {
            500.0
        } else {
            size
        };
        let guides_on = !matches!(first(3), Some(Value::Bool(false)));
        let style = match first(4) {
            Some(Value::Style(s)) => Some(s.clone()),
            _ => None,
        };

        let mode = param_str(cx, "tracker");
        let trackers = match mode.as_str() {
            "face" => vec![Tracker::Face],
            "pose" => vec![Tracker::Pose],
            "all" => vec![Tracker::Hands, Tracker::Face, Tracker::Pose],
            _ => vec![Tracker::Hands],
        };
        let mirror = param_flag(cx, "mirror", true);
        let count = {
            let n = cx.param_value("count");
            let n = if n.is_undefined() {
                2.0
            } else {
                n.to_number().floor()
            };
            if n.is_nan() || n == 0.0 {
                1.0
            } else {
                n.clamp(1.0, 6.0)
            }
        } as usize;
        let person = {
            let n = cx.param_value("person");
            let n = if n.is_undefined() {
                0.0
            } else {
                n.to_number().floor()
            };
            if n.is_nan() {
                0.0
            } else {
                n.max(0.0)
            }
        };
        let per_group = if trackers.len() > 1 {
            0
        } else {
            trackers[0].group()
        };

        let layers = match devices::installed() {
            Some(d) => {
                let req = LandmarksRequest {
                    image,
                    start,
                    trackers,
                    count,
                };
                d.landmarks(cx.node_id, &req)?.unwrap_or_default()
            }
            None => Vec::new(),
        };
        Ok(landmark_outputs(
            &layers,
            &LandmarkOptions {
                size,
                mirror,
                count,
                person,
                per_group,
                guides: guides_on,
                style: style.as_ref(),
            },
        ))
    })
}

struct LandmarkOptions<'a> {
    size: f64,
    mirror: bool,
    count: usize,
    person: f64,
    per_group: usize,
    guides: bool,
    style: Option<&'a crate::scene::Style>,
}

fn landmark_outputs(layers: &[LandmarkLayer], o: &LandmarkOptions) -> Vec<Spread> {
    let to_canvas = |p: &[f32; 3]| {
        let x = f64::from(p[0]);
        Vec2 {
            x: ((if o.mirror { 1.0 - x } else { x }) - 0.5) * o.size,
            y: (f64::from(p[1]) - 0.5) * o.size,
        }
    };
    let mut points: Vec<Vec2> = Vec::new();
    let mut zs = Vec::new();
    let mut group = Vec::new();
    let mut color: Vec<Color> = Vec::new();
    let mut conns: Vec<(Vec2, Vec2, Color)> = Vec::new();
    let mut people: Vec<Vec<Value>> = vec![Vec::new(); o.count];
    let mut detections = 0;
    for layer in layers {
        for (di, det) in layer.dets.iter().enumerate() {
            let id = layer.ids.get(di).map_or(di as i64, |&i| i64::from(i));
            if o.person > 0.0 && id as f64 != o.person - 1.0 {
                continue;
            }
            detections += 1;
            let col = color_for_id(id);
            let bucket = usize::try_from(id).ok().filter(|&i| i < o.count);
            for lm in det {
                let p = to_canvas(lm);
                points.push(p);
                zs.push(Value::Num(f64::from(lm[2]) * o.size));
                group.push(Value::Num(id as f64));
                color.push(col);
                if let Some(b) = bucket {
                    people[b].push(Value::Vec2(p));
                }
            }
            for &[a, b] in layer.tracker.connections() {
                if let (Some(a), Some(b)) = (det.get(usize::from(a)), det.get(usize::from(b))) {
                    conns.push((to_canvas(a), to_canvas(b), col));
                }
            }
        }
    }

    let mut guides = Vec::new();
    if o.guides {
        let fallback = Arc::new(ResolvedStyle {
            stroke_weight: 2.0,
            ..(**line_default_style()).clone()
        });
        let base = resolve_style(o.style, &fallback);
        let styled = |c: Color| {
            Arc::new(ResolvedStyle {
                stroke: Some(Paint::from_color(c)),
                ..(*base).clone()
            })
        };
        for &(a, b, c) in &conns {
            guides.push(Value::Scene(Arc::new(SceneNode::Line {
                x1: a.x,
                y1: a.y,
                x2: b.x,
                y2: b.y,
                style: styled(c),
            })));
        }
        for (p, &c) in points.iter().zip(&color) {
            guides.push(Value::Scene(Arc::new(SceneNode::Point {
                x: p.x,
                y: p.y,
                style: styled(c),
            })));
        }
    }

    let detected = !points.is_empty();
    let spread = |v: Vec<Value>| -> Spread { Arc::from(v) };
    vec![
        spread(points.into_iter().map(Value::Vec2).collect()),
        spread(zs),
        spread(group),
        spread(color.into_iter().map(Value::Color).collect()),
        spread(conns.iter().map(|c| Value::Vec2(c.0)).collect()),
        spread(conns.iter().map(|c| Value::Vec2(c.1)).collect()),
        spread(conns.iter().map(|c| Value::Color(c.2)).collect()),
        spread(guides),
        spread(people.into_iter().map(Value::list).collect()),
        spread(
            (0..o.count as i64)
                .map(|i| Value::Color(color_for_id(i)))
                .collect(),
        ),
        num(f64::from(detections)),
        num(o.per_group as f64),
        one(Value::Bool(detected)),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{node, num};
    use super::*;

    #[test]
    fn mouse_and_keyboard() {
        let mut m = node("input/mouse", serde_json::json!({}));
        m.frame.mouse_x = -12.0;
        m.frame.mouse_y = 30.0;
        m.frame.mouse_is_pressed = true;
        let out = m.eval1(&[]);
        assert_eq!(num(&out[0]), -12.0);
        assert_eq!(num(&out[1]), 30.0);
        assert!(matches!(out[2], Value::Bool(true)));

        let mut k = node("input/keyboard", serde_json::json!({}));
        k.frame.key = "a".into();
        let out = k.eval1(&[]);
        assert!(matches!(out[0], Value::Bool(false)));
        assert!(matches!(&out[1], Value::Str(s) if &**s == "a"));
    }

    // Unit tests run without a device provider (the fake-provider test is
    // `tests/devices.rs`, its own process, since the provider is global).

    #[test]
    fn midi_defaults() {
        let mut n = node("input/midi-note", serde_json::json!({}));
        let out = n.eval1(&[]);
        assert_eq!(out.len(), 4);
        assert!(matches!(out[2], Value::Bool(false)));
        let mut c = node("input/midi-cc", serde_json::json!({ "cc": 7 }));
        let out = c.eval1(&[]);
        assert_eq!((num(&out[0]), num(&out[1])), (0.0, 0.0));
    }

    #[test]
    fn camera_without_devices_errors_like_the_traces() {
        let mut n = node(
            "input/camera",
            serde_json::json!({ "device": "", "mirror": true }),
        );
        assert!(n.eval(vec![vec![Value::Bool(true)]]).is_err());
    }

    #[test]
    fn landmarks_without_devices_match_the_traces() {
        // body-tracking's bt_lm: [[]×8, [[], []], 2 colours, [0], [21], [false]].
        let mut n = node(
            "input/landmarks",
            serde_json::json!({ "tracker": "hands", "mirror": true, "count": 2, "person": 0 }),
        );
        let out = n
            .eval(vec![
                vec![],
                vec![Value::Bool(true)],
                vec![],
                vec![],
                vec![],
            ])
            .unwrap();
        assert_eq!(out.len(), 13);
        assert!(out[..8].iter().all(Vec::is_empty));
        assert!(
            matches!(&out[8][..], [Value::List(a), Value::List(b)] if a.is_empty() && b.is_empty())
        );
        assert!(matches!(out[9][1], Value::Color(c) if c == color_for_id(1)));
        assert_eq!(color_for_id(1).g, 0.86);
        assert_eq!((num(&out[10][0]), num(&out[11][0])), (0.0, 21.0));
        assert!(matches!(out[12][0], Value::Bool(false)));
        let mut all = node(
            "input/landmarks",
            serde_json::json!({ "tracker": "all", "count": "0" }),
        );
        let out = all.eval(vec![vec![]; 5]).unwrap();
        assert_eq!((out[8].len(), num(&out[11][0])), (1, 0.0));
    }

    #[test]
    fn landmark_layers_become_canvas_points_edges_and_people() {
        let layer = LandmarkLayer {
            tracker: Tracker::Hands,
            dets: vec![(0..21).map(|i| [i as f32 / 20.0, 0.25, 0.1]).collect()],
            ids: vec![1],
        };
        let opts = LandmarkOptions {
            size: 100.0,
            mirror: true,
            count: 2,
            person: 0.0,
            per_group: 21,
            guides: true,
            style: None,
        };
        let out = landmark_outputs(std::slice::from_ref(&layer), &opts);
        assert_eq!(out[0].len(), 21);
        // Mirrored: x = 0 → +50, x = 1 → -50; y = 0.25 → -25.
        assert!(matches!(out[0][0], Value::Vec2(Vec2 { x: 50.0, y: -25.0 })));
        assert!(matches!(
            out[0][20],
            Value::Vec2(Vec2 { x: -50.0, y: -25.0 })
        ));
        assert!((num(&out[1][0]) - 10.0).abs() < 1e-5);
        assert_eq!(num(&out[2][0]), 1.0);
        assert!(matches!(out[3][0], Value::Color(c) if c == color_for_id(1)));
        assert_eq!((out[4].len(), out[5].len(), out[6].len()), (21, 21, 21));
        // 21 edges + 21 dots, stroked in the person's colour, weight 2.
        assert_eq!(out[7].len(), 42);
        let Value::Scene(s) = &out[7][0] else {
            panic!()
        };
        let SceneNode::Line { style, .. } = &**s else {
            panic!()
        };
        assert_eq!(style.stroke_weight, 2.0);
        assert_eq!(&*style.stroke.as_ref().unwrap().css, "rgb(61,219,89)");
        // Person id 1 fills slot 1 of `people`.
        assert!(
            matches!(&out[8][..], [Value::List(a), Value::List(b)] if a.is_empty() && b.len() == 21)
        );
        assert_eq!(num(&out[10][0]), 1.0);
        assert!(matches!(out[12][0], Value::Bool(true)));

        // `person` filters to one id; `guides: false` drops the overlay.
        let only_first = LandmarkOptions {
            person: 1.0,
            guides: false,
            ..opts
        };
        let out = landmark_outputs(&[layer], &only_first);
        assert!(out[0].is_empty() && out[7].is_empty());
        assert_eq!(num(&out[10][0]), 0.0);
    }
}
