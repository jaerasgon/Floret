//! The ADAPTIVE arithmetic nodes (add, sub, mul, div, min, max, lerp, neg,
//! abs): one node per op, polymorphic over number / vec2 / color, computing
//! component-wise. Port of `src/nodes/math/_adaptive.ts`.

use crate::js;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::{Color, Value, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Number,
    Vec2,
    Color,
}

impl Kind {
    fn components(self) -> usize {
        match self {
            Kind::Number => 1,
            Kind::Vec2 => 2,
            Kind::Color => 4,
        }
    }
}

fn value_type(params: &Params) -> Option<&str> {
    params.get("valueType").and_then(serde_json::Value::as_str)
}

/// `kindForValueType`: `any` (or anything unknown) computes as number.
pub fn kind_for_value_type(params: &Params) -> Kind {
    match value_type(params) {
        Some("vec2") => Kind::Vec2,
        Some("color") => Kind::Color,
        _ => Kind::Number,
    }
}

/// `portTypeForValueType`: `any` stays neutral until latched.
pub fn port_type_for_value_type(params: &Params) -> PortType {
    match value_type(params) {
        Some("vec2") => PortType::Vec2,
        Some("color") => PortType::Color,
        Some("number") => PortType::Number,
        _ => PortType::Any,
    }
}

/// `kindOf`: the value kind of a runtime value (`None` when unknown).
/// Colour wins over vec2 for objects carrying both shapes.
pub fn kind_of(v: &Value) -> Option<Kind> {
    match v {
        Value::Num(_) => Some(Kind::Number),
        Value::Color(_) => Some(Kind::Color),
        Value::Vec2(_) => Some(Kind::Vec2),
        Value::Object(_) => {
            let is_num = |k: &str| v.get(k).as_f64().is_some();
            if is_num("r") && is_num("g") && is_num("b") {
                Some(Kind::Color)
            } else if is_num("x") && is_num("y") {
                Some(Kind::Vec2)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `componentsOf`: break a value into numeric components of `kind`.
pub fn components_of(v: &Value, kind: Kind) -> Vec<f64> {
    let field = |k: &str, d: f64| v.get(k).as_f64().unwrap_or(d);
    match kind {
        // `typeof v === 'number' ? v : Number(v) || 0`
        Kind::Number => vec![v.as_number()],
        Kind::Vec2 => vec![field("x", 0.0), field("y", 0.0)],
        Kind::Color => vec![
            field("r", 0.0),
            field("g", 0.0),
            field("b", 0.0),
            field("a", 1.0),
        ],
    }
}

/// `fromComponents`.
pub fn from_components(c: &[f64], kind: Kind) -> Value {
    let at = |i: usize, d: f64| c.get(i).copied().unwrap_or(d);
    match kind {
        Kind::Number => Value::Num(at(0, 0.0)),
        Kind::Vec2 => Value::Vec2(Vec2 {
            x: at(0, 0.0),
            y: at(1, 0.0),
        }),
        Kind::Color => Value::Color(Color {
            r: at(0, 0.0),
            g: at(1, 0.0),
            b: at(2, 0.0),
            a: at(3, 1.0),
        }),
    }
}

/// `identityValue`: every component set to `scalar`.
pub fn identity_value(kind: Kind, scalar: f64) -> Value {
    from_components(&vec![scalar; kind.components()], kind)
}

// ─── Dynamic inputs ──────────────────────────────────────────────────────────

/// `adaptiveSlots(identity)`: `slotCount` pins a, b, c, … (1..=26).
pub fn adaptive_slots(params: &Params, identity: f64) -> Vec<PortSpec> {
    let kind = kind_for_value_type(params);
    let ty = port_type_for_value_type(params);
    let def = identity_value(kind, identity);
    // Math.max(1, Math.min(26, Math.floor(Number(params.slotCount) || 2)))
    let raw = params
        .get("slotCount")
        .map(Value::from_json)
        .unwrap_or(Value::Undefined)
        .to_number();
    let raw = if raw.is_nan() || raw == 0.0 { 2.0 } else { raw };
    let n = js::max(1.0, js::min(26.0, raw.floor())) as usize;
    (0..n)
        .map(|i| {
            let name = char::from(b'a' + i as u8).to_string();
            PortSpec::new(name.clone(), ty)
                .label(name)
                .default(def.clone())
        })
        .collect()
}

/// `adaptiveUnaryInputs(name, identity)` (neg / abs).
pub fn adaptive_unary_inputs(params: &Params, name: &str, identity: f64) -> Vec<PortSpec> {
    let kind = kind_for_value_type(params);
    vec![PortSpec::new(name, port_type_for_value_type(params))
        .label(name)
        .default(identity_value(kind, identity))]
}

/// `adaptiveLerpInputs`: `a` / `b` follow `valueType`; `t` is a number.
pub fn adaptive_lerp_inputs(params: &Params) -> Vec<PortSpec> {
    let kind = kind_for_value_type(params);
    let ty = port_type_for_value_type(params);
    vec![
        PortSpec::new("a", ty).default(identity_value(kind, 0.0)),
        PortSpec::new("b", ty).default(identity_value(kind, 1.0)),
        PortSpec::new("t", PortType::Number).default(Value::Num(0.0)),
    ]
}

// ─── Compute ─────────────────────────────────────────────────────────────────

/// `adaptiveReduce(perComponent)` over every operand.
pub fn reduce(vals: &[Value], per_component: fn(&[f64]) -> f64) -> Value {
    if vals.is_empty() {
        return Value::Num(0.0);
    }
    let kind = vals.iter().find_map(kind_of).unwrap_or(Kind::Number);
    let cols: Vec<Vec<f64>> = vals.iter().map(|v| components_of(v, kind)).collect();
    let out: Vec<f64> = (0..kind.components())
        .map(|ci| {
            let xs: Vec<f64> = cols
                .iter()
                .map(|c| c.get(ci).copied().unwrap_or(0.0))
                .collect();
            per_component(&xs)
        })
        .collect();
    from_components(&out, kind)
}

pub fn reduce_add(xs: &[f64]) -> f64 {
    xs.iter().fold(0.0, |a, b| a + b)
}

pub fn reduce_mul(xs: &[f64]) -> f64 {
    xs.iter().fold(1.0, |a, b| a * b)
}

pub fn reduce_sub(xs: &[f64]) -> f64 {
    match xs.split_first() {
        None => 0.0,
        Some((first, rest)) => rest.iter().fold(*first, |a, b| a - b),
    }
}

/// Division where any zero divisor makes the whole component 0.
pub fn reduce_div(xs: &[f64]) -> f64 {
    let Some((first, rest)) = xs.split_first() else {
        return 0.0;
    };
    let mut r = *first;
    for &d in rest {
        if d == 0.0 {
            return 0.0;
        }
        r /= d;
    }
    r
}

pub fn reduce_min(xs: &[f64]) -> f64 {
    js::min_of(xs.iter().copied())
}

pub fn reduce_max(xs: &[f64]) -> f64 {
    js::max_of(xs.iter().copied())
}

/// `adaptiveMap(perComponent)` (neg / abs).
pub fn map(v: &Value, per_component: fn(f64) -> f64) -> Value {
    let kind = kind_of(v).unwrap_or(Kind::Number);
    let c: Vec<f64> = components_of(v, kind)
        .into_iter()
        .map(per_component)
        .collect();
    from_components(&c, kind)
}

/// `adaptiveLerpCompute`: component-wise, scalar `t`.
pub fn lerp(a: &Value, b: &Value, t: &Value) -> Value {
    let kind = kind_of(a).or_else(|| kind_of(b)).unwrap_or(Kind::Number);
    let ca = components_of(a, kind);
    let cb = components_of(b, kind);
    let tv = t.as_number();
    let out: Vec<f64> = ca
        .iter()
        .enumerate()
        .map(|(i, av)| av + (cb.get(i).copied().unwrap_or(0.0) - av) * tv)
        .collect();
    from_components(&out, kind)
}

// ─── Factories ───────────────────────────────────────────────────────────────

pub fn reducer(init: &NodeInit, per_component: fn(&[f64]) -> f64) -> Box<dyn NodeImpl> {
    scalar(init, move |args| Ok(vec![reduce(args, per_component)]))
}

pub fn mapper(init: &NodeInit, per_component: fn(f64) -> f64) -> Box<dyn NodeImpl> {
    scalar(init, move |args| {
        Ok(vec![map(
            args.first().unwrap_or(&Value::Undefined),
            per_component,
        )])
    })
}

pub fn lerp_node(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let at = |i: usize| args.get(i).unwrap_or(&Value::Undefined);
        Ok(vec![lerp(at(0), at(1), at(2))])
    })
}
