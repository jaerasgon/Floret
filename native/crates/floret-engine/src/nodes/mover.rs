//! `particles/move` (Move): lightweight per-element kinematics on plain
//! vec2 spreads — positions, velocities, acceleration, drag and a speed
//! limit inside a box whose edges wrap, bounce, clamp or do nothing.
//!
//! A native-only node, the spread-based companion of the particle system:
//! no buffers or worlds, just lists in and lists out, so its results feed
//! straight into Hit Test, Switch, Math or a draw node. In `run` mode it
//! keeps its own positions and velocities (seeded from the inputs, re-read
//! per element while `reset` holds); in `step` mode it takes one step from
//! the inputs (for a State / feedback loop that owns the state).

use super::geometry::{num_pin, vec2_pin, Pins};
use super::shared::to_vec2;
use super::Registrar;
use crate::node::{CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("particles/move", mover);
    r.dynamic("particles/move", inputs);
}

/// The longest frame step taken (a stall doesn't fling things away).
const MAX_DT: f64 = 0.1;

fn param_str<'a>(params: &'a Params, key: &str, fallback: &'a str) -> &'a str {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    None,
    Wrap,
    Bounce,
    Clamp,
}

impl Edge {
    fn parse(s: &str) -> Edge {
        match s {
            "wrap" => Edge::Wrap,
            "bounce" => Edge::Bounce,
            "clamp" => Edge::Clamp,
            _ => Edge::None,
        }
    }
}

fn spread(name: &str, ty: PortType, default: Value) -> PortSpec {
    PortSpec::new(name, ty)
        .kind(PortKind::Spread)
        .default(default)
}

fn vec_spread(name: &str) -> PortSpec {
    spread(name, PortType::Vec2, Value::Vec2(Vec2::default()))
}

fn num_spread(name: &str, d: f64) -> PortSpec {
    spread(name, PortType::Number, Value::Num(d))
}

/// Pins: the motion per element, the box (`size` 0 × 0 = the canvas),
/// `restitution` in bounce mode, `reset` in run mode.
pub(crate) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut edge = PortSpec::new("edge", PortType::String).default(Value::str("bounce"));
    edge.param = Some("edge".into());
    let mut pins = vec![
        vec_spread("position"),
        vec_spread("velocity"),
        vec_spread("acceleration"),
        num_spread("drag", 0.0),
        num_spread("maxSpeed", 0.0).label("max speed"),
        num_spread("radius", 0.0),
        edge,
        vec2_pin("center", 0.0, 0.0),
        vec2_pin("size", 0.0, 0.0),
    ];
    if Edge::parse(param_str(params, "edge", "bounce")) == Edge::Bounce {
        pins.push(num_spread("restitution", 1.0));
    }
    pins.push(num_pin("timeScale", 1.0).label("time scale"));
    if param_str(params, "mode", "run") != "step" {
        pins.push(spread("reset", PortType::Boolean, Value::Bool(false)));
    }
    pins
}

/// Element `i` of a spread, cycling (`Undefined` when empty).
fn at(s: &[Value], i: usize) -> &Value {
    static UNDEF: Value = Value::Undefined;
    if s.is_empty() {
        &UNDEF
    } else {
        &s[i % s.len()]
    }
}

fn num_at(s: &[Value], i: usize, fallback: f64) -> f64 {
    match at(s, i) {
        Value::Num(x) if x.is_finite() => *x,
        _ => fallback,
    }
}

/// A finite point at element `i` ((0, 0) for anything else).
fn vec_at(s: &[Value], i: usize) -> Vec2 {
    let p = to_vec2(at(s, i));
    Vec2 {
        x: if p.x.is_finite() { p.x } else { 0.0 },
        y: if p.y.is_finite() { p.y } else { 0.0 },
    }
}

/// One axis against `[lo, hi]` (already shrunk by the radius). Returns the
/// wall normal component hit (−1, +1) or 0.
fn edge_axis(p: &mut f64, v: &mut f64, lo: f64, hi: f64, r: f64, edge: Edge, rest: f64) -> f64 {
    match edge {
        Edge::None => 0.0,
        Edge::Wrap => {
            // The span widens by the radius so things leave whole before
            // they come back round.
            let (a, b) = (lo - r, hi + r);
            let w = b - a;
            if w <= 0.0 {
                return 0.0;
            }
            if *p < a {
                *p = b - (a - *p).rem_euclid(w);
                1.0
            } else if *p >= b {
                *p = a + (*p - b).rem_euclid(w);
                -1.0
            } else {
                0.0
            }
        }
        Edge::Bounce | Edge::Clamp => {
            let (a, b) = (lo + r, hi - r);
            if a > b {
                *p = (lo + hi) / 2.0;
                return 0.0;
            }
            let bounce = edge == Edge::Bounce;
            if *p < a {
                *p = if bounce {
                    (a + (a - *p) * rest).min(b)
                } else {
                    a
                };
                if *v < 0.0 {
                    *v = if bounce { -*v * rest } else { 0.0 };
                    return 1.0;
                }
                0.0
            } else if *p > b {
                *p = if bounce {
                    (b - (*p - b) * rest).max(a)
                } else {
                    b
                };
                if *v > 0.0 {
                    *v = if bounce { -*v * rest } else { 0.0 };
                    return -1.0;
                }
                0.0
            } else {
                0.0
            }
        }
    }
}

/// Per-element positions and velocities (run mode).
#[derive(Default)]
struct MoveState {
    pos: Vec<Vec2>,
    vel: Vec<Vec2>,
}

struct Mover {
    pins: Pins,
    step_mode: bool,
    state: MoveState,
}

fn mover(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Mover {
        pins: Pins::new(init),
        step_mode: param_str(init.params, "mode", "run") == "step",
        state: MoveState::default(),
    })
}

impl NodeImpl for Mover {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let p = &self.pins;
        let pos_in = p.spread(inputs, "position");
        let vel_in = p.spread(inputs, "velocity");
        let acc = p.spread(inputs, "acceleration");
        let drag = p.spread(inputs, "drag");
        let max_speed = p.spread(inputs, "maxSpeed");
        let radius = p.spread(inputs, "radius");
        let rest = p.spread(inputs, "restitution");
        let reset = p.spread(inputs, "reset");
        let edge = Edge::parse(&match p.value(inputs, "edge") {
            Value::Str(s) => s.to_string(),
            _ => cx
                .param("edge")
                .and_then(|v| v.as_str())
                .unwrap_or("bounce")
                .to_owned(),
        });
        let frame = cx.tick_frame;
        let center = p.vec2(inputs, "center", Vec2::default());
        let mut size = p.vec2(inputs, "size", Vec2::default());
        if size.x <= 0.0 && size.y <= 0.0 {
            size = Vec2 {
                x: frame.width,
                y: frame.height,
            };
        }
        let (lo, hi) = (
            Vec2 {
                x: center.x - size.x.abs() / 2.0,
                y: center.y - size.y.abs() / 2.0,
            },
            Vec2 {
                x: center.x + size.x.abs() / 2.0,
                y: center.y + size.y.abs() / 2.0,
            },
        );
        let mut dt = frame.dt;
        if !dt.is_finite() {
            dt = 0.0;
        }
        dt = dt.clamp(0.0, MAX_DT) * p.num(inputs, "timeScale", 1.0);

        let n = if pos_in.is_empty() || vel_in.is_empty() {
            0
        } else {
            pos_in.len().max(vel_in.len())
        };
        let st = &mut self.state;
        if self.step_mode {
            st.pos.clear();
            st.vel.clear();
        }
        let old = st.pos.len().min(n);
        st.pos.truncate(n);
        st.vel.truncate(n);
        for i in old..n {
            st.pos.push(vec_at(pos_in, i));
            st.vel.push(vec_at(vel_in, i));
        }
        let mut hits = Vec::with_capacity(n);
        let mut normals = Vec::with_capacity(n);
        for i in 0..n {
            if !self.step_mode && at(reset, i).truthy() {
                st.pos[i] = vec_at(pos_in, i);
                st.vel[i] = vec_at(vel_in, i);
                hits.push(Value::Bool(false));
                normals.push(Value::Vec2(Vec2::default()));
                continue;
            }
            let (mut q, mut v) = (st.pos[i], st.vel[i]);
            // Semi-implicit Euler: velocity first, then position.
            let a = vec_at(acc, i);
            v.x += a.x * dt;
            v.y += a.y * dt;
            let k = num_at(drag, i, 0.0);
            if k > 0.0 {
                let f = (-k * dt).exp();
                v.x *= f;
                v.y *= f;
            }
            let m = num_at(max_speed, i, 0.0);
            let speed = v.x.hypot(v.y);
            if m > 0.0 && speed > m {
                v.x *= m / speed;
                v.y *= m / speed;
            }
            q.x += v.x * dt;
            q.y += v.y * dt;
            let r = num_at(radius, i, 0.0).max(0.0);
            let e = num_at(rest, i, 1.0).max(0.0);
            let nx = edge_axis(&mut q.x, &mut v.x, lo.x, hi.x, r, edge, e);
            let ny = edge_axis(&mut q.y, &mut v.y, lo.y, hi.y, r, edge, e);
            st.pos[i] = q;
            st.vel[i] = v;
            hits.push(Value::Bool(nx != 0.0 || ny != 0.0));
            normals.push(Value::Vec2(Vec2 { x: nx, y: ny }));
        }
        let out = |v: &[Vec2]| -> Spread { v.iter().map(|p| Value::Vec2(*p)).collect() };
        Ok(vec![
            out(&st.pos),
            out(&st.vel),
            Arc::from(hits),
            Arc::from(normals),
        ])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        if self.step_mode {
            return None;
        }
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if self.step_mode {
            return;
        }
        if let Ok(s) = state.downcast::<MoveState>() {
            self.state = *s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::{n, node, Harness};
    use serde_json::json;

    fn v2(x: f64, y: f64) -> Value {
        Value::Vec2(Vec2 { x, y })
    }

    fn pt(v: &Value) -> (f64, f64) {
        let p = to_vec2(v);
        (p.x, p.y)
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    fn ins(params: &serde_json::Value, given: &[(&str, Vec<Value>)]) -> Vec<Vec<Value>> {
        let p: Params = serde_json::from_value(params.clone()).unwrap();
        inputs(&p)
            .iter()
            .map(|pin| {
                given
                    .iter()
                    .find(|(k, _)| *k == pin.name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| pin.default.clone().into_iter().collect())
            })
            .collect()
    }

    fn step(
        h: &mut Harness,
        params: &serde_json::Value,
        dt: f64,
        given: &[(&str, Vec<Value>)],
    ) -> Vec<Vec<Value>> {
        h.frame.dt = dt;
        h.frame.width = 200.0;
        h.frame.height = 100.0;
        h.eval(ins(params, given)).unwrap()
    }

    #[test]
    fn moves_with_velocity_and_acceleration() {
        let p = json!({"edge": "none"});
        let mut h = node("particles/move", p.clone());
        let given = [
            ("position", vec![v2(0.0, 0.0), v2(10.0, 0.0)]),
            ("velocity", vec![v2(10.0, 0.0)]),
            ("acceleration", vec![v2(0.0, 100.0)]),
        ];
        let out = step(&mut h, &p, 0.1, &given);
        assert_eq!(out[0].len(), 2);
        // v = (10, 10), p += v·dt.
        assert!(near(pt(&out[0][0]), (1.0, 1.0)));
        assert!(near(pt(&out[0][1]), (11.0, 1.0)));
        assert!(near(pt(&out[1][0]), (10.0, 10.0)));
        // The state carries on (inputs are only the start).
        let out = step(&mut h, &p, 0.1, &given);
        assert!(near(pt(&out[0][0]), (2.0, 3.0)));
        // A stall counts as 0.1 s.
        let out = step(&mut h, &p, 9.0, &given);
        assert!(near(pt(&out[0][0]), (3.0, 6.0)));
    }

    #[test]
    fn edges_bounce_wrap_clamp() {
        // Canvas 200 × 100: x in [-100, 100], y in [-50, 50].
        let one = |edge: &str, pos: Value, vel: Value, r: f64| {
            let p = json!({ "edge": edge });
            let mut h = node("particles/move", p.clone());
            let out = step(
                &mut h,
                &p,
                0.1,
                &[
                    ("position", vec![pos]),
                    ("velocity", vec![vel]),
                    ("radius", vec![n(r)]),
                    ("edge", vec![Value::str(edge)]),
                ],
            );
            (
                pt(&out[0][0]),
                pt(&out[1][0]),
                out[2][0].truthy(),
                pt(&out[3][0]),
            )
        };
        let (q, v, hit, nrm) = one("bounce", v2(95.0, 0.0), v2(100.0, 0.0), 0.0);
        assert!(near(q, (95.0, 0.0)) && near(v, (-100.0, 0.0)) && hit);
        assert!(near(nrm, (-1.0, 0.0)));
        // With a radius it bounces off its edge.
        let (q, _, hit, _) = one("bounce", v2(80.0, 0.0), v2(100.0, 0.0), 15.0);
        assert!(near(q, (80.0, 0.0)) && hit);
        let (q, v, hit, _) = one("clamp", v2(0.0, 45.0), v2(0.0, 100.0), 0.0);
        assert!(near(q, (0.0, 50.0)) && near(v, (0.0, 0.0)) && hit);
        let (q, _, hit, nrm) = one("wrap", v2(-95.0, 0.0), v2(-100.0, 0.0), 0.0);
        assert!(near(q, (95.0, 0.0)) && hit && near(nrm, (1.0, 0.0)));
        let (q, _, hit, _) = one("none", v2(95.0, 0.0), v2(100.0, 0.0), 0.0);
        assert!(near(q, (105.0, 0.0)) && !hit);
        // A box smaller than the thing: it sits in the middle.
        let (q, _, _, _) = one("clamp", v2(30.0, 0.0), v2(0.0, 0.0), 500.0);
        assert!(near(q, (0.0, 0.0)));
    }

    #[test]
    fn drag_speed_limit_restitution() {
        let p = json!({"edge": "bounce"});
        let mut h = node("particles/move", p.clone());
        let out = step(
            &mut h,
            &p,
            0.1,
            &[
                ("position", vec![v2(0.0, 0.0)]),
                ("velocity", vec![v2(1000.0, 0.0)]),
                ("maxSpeed", vec![n(100.0)]),
            ],
        );
        assert!(near(pt(&out[1][0]), (100.0, 0.0)));
        let mut h = node("particles/move", p.clone());
        let out = step(
            &mut h,
            &p,
            0.1,
            &[
                ("position", vec![v2(0.0, 0.0)]),
                ("velocity", vec![v2(100.0, 0.0)]),
                ("drag", vec![n(10.0)]),
            ],
        );
        assert!((pt(&out[1][0]).0 - 100.0 * (-1.0f64).exp()).abs() < 1e-9);
        let mut h = node("particles/move", p.clone());
        let out = step(
            &mut h,
            &p,
            0.1,
            &[
                ("position", vec![v2(95.0, 0.0)]),
                ("velocity", vec![v2(100.0, 0.0)]),
                ("restitution", vec![n(0.5)]),
            ],
        );
        assert!(near(pt(&out[1][0]), (-50.0, 0.0)));
    }

    #[test]
    fn reset_per_element_and_step_mode() {
        let p = json!({"edge": "none"});
        let mut h = node("particles/move", p.clone());
        let base = |reset: Vec<Value>| {
            vec![
                ("position", vec![v2(0.0, 0.0), v2(5.0, 5.0)]),
                ("velocity", vec![v2(10.0, 0.0)]),
                ("reset", reset),
            ]
        };
        step(&mut h, &p, 0.1, &base(vec![Value::Bool(false)]));
        let out = step(
            &mut h,
            &p,
            0.1,
            &base(vec![Value::Bool(true), Value::Bool(false)]),
        );
        assert!(near(pt(&out[0][0]), (0.0, 0.0)));
        assert!(near(pt(&out[0][1]), (7.0, 5.0)));
        // Step mode: one step from the inputs each time.
        let p = json!({"mode": "step", "edge": "none"});
        let mut h = node("particles/move", p.clone());
        let given = [
            ("position", vec![v2(1.0, 1.0)]),
            ("velocity", vec![v2(10.0, 0.0)]),
        ];
        step(&mut h, &p, 0.1, &given);
        let out = step(&mut h, &p, 0.1, &given);
        assert!(near(pt(&out[0][0]), (2.0, 1.0)));
        // Empty positions: nothing.
        let out = step(&mut h, &p, 0.1, &[("position", vec![])]);
        assert!(out[0].is_empty());
    }

    #[test]
    fn state_survives_an_edit_and_nan_is_safe() {
        let p = json!({"edge": "none"});
        let mut h = node("particles/move", p.clone());
        let given = [
            ("position", vec![v2(0.0, 0.0)]),
            ("velocity", vec![v2(10.0, 0.0)]),
        ];
        step(&mut h, &p, 0.1, &given);
        let mut h2 = node("particles/move", p.clone());
        h2.adopt(h.carry().unwrap());
        let out = step(&mut h2, &p, 0.1, &given);
        assert!(near(pt(&out[0][0]), (2.0, 0.0)));
        let mut h = node("particles/move", p.clone());
        let out = step(
            &mut h,
            &p,
            0.1,
            &[
                ("position", vec![v2(f64::NAN, 0.0)]),
                ("velocity", vec![v2(1.0, f64::INFINITY)]),
                ("drag", vec![n(f64::NAN)]),
            ],
        );
        let (x, y) = pt(&out[0][0]);
        assert!(x.is_finite() && y.is_finite());
    }

    #[test]
    fn pins_follow_mode_and_edge() {
        let names = |p: serde_json::Value| -> Vec<String> {
            inputs(&serde_json::from_value(p).unwrap())
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        let all = names(json!({}));
        assert!(all.contains(&"restitution".into()) && all.contains(&"reset".into()));
        let st = names(json!({"mode": "step", "edge": "wrap"}));
        assert!(!st.contains(&"restitution".into()) && !st.contains(&"reset".into()));
        let spec = crate::spec::Catalogue::builtin()
            .get("particles/move")
            .unwrap();
        let s: Vec<String> = spec.inputs.iter().map(|p| p.name.clone()).collect();
        assert_eq!(s, all);
    }
}
