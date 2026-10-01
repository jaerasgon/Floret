//! `physics/world`. Ported from `src/nodes/physics/world.ts`.

use super::particles::{handle, num_or, vec2_or};
use super::shared::first;
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::particles::{Handle, Integrator, World};
use crate::value::{empty, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("physics/world", world);
}

/// `'x' in v` for the runtime's object shapes.
fn has_x(v: &Value) -> bool {
    match v {
        Value::Vec2(_) => true,
        Value::Object(m) => m.contains_key("x"),
        Value::Scene(n) => has_x(&n.to_value()),
        Value::Handle(h) => h.to_value().is_some_and(|o| has_x(&o)),
        _ => false,
    }
}

/// `physics/world`: gravity, drag, time scale, bounds and the global
/// force / collider lists, as one `World` handle.
fn world(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _cx| {
        let gravity = vec2_or(&first(inputs, 0), Vec2 { x: 0.0, y: 200.0 });
        let bounds_in = first(inputs, 3);
        let bounds = if bounds_in.truthy() && bounds_in.is_object() && has_x(&bounds_in) {
            Some((
                bounds_in.get("x").to_number(),
                bounds_in.get("y").to_number(),
            ))
        } else {
            None
        };
        let integrator = match first(inputs, 6) {
            Value::Str(s) if &*s == "verlet" => Integrator::Verlet,
            _ => Integrator::Euler,
        };
        let w = World {
            gravity_x: gravity.x,
            gravity_y: gravity.y,
            drag: num_or(&first(inputs, 1), 0.05),
            time_scale: num_or(&first(inputs, 2), 1.0),
            bounds,
            forces: inputs.get(4).cloned().unwrap_or_else(empty),
            colliders: inputs.get(5).cloned().unwrap_or_else(empty),
            integrator,
        };
        Ok(vec![handle(Handle::World(Arc::new(w)))])
    })
}
