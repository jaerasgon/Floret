//! Node implementations, one module per category.
//!
//! Each module exposes `register(&mut Registrar)` and adds a factory per
//! node type it implements (plus a dynamic-inputs function for node types
//! whose pins depend on params). A node type in the catalogue with no
//! factory still compiles — it just outputs empty spreads and is reported
//! as unimplemented.

use crate::node::Factory;
use crate::spec::{AdaptiveFn, DynamicInputsFn};
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod collider;
pub mod color;
pub mod control;
pub mod convert;
pub mod data;
pub mod draw;
pub mod echo;
pub mod effect;
pub mod expr;
pub mod family;
pub mod force;
pub mod format;
pub mod geometry;
pub mod gpu;
pub mod image;
pub mod input;
pub mod io;
pub mod lfo;
pub mod logic;
pub mod math;
pub mod meta;
pub mod motion;
pub mod mover;
pub mod noise;
pub mod particles;
pub mod physics;
pub mod random;
pub mod render;
pub mod shared;
pub mod simulation;
pub mod sound;
pub mod spread;
pub mod string;
pub mod style;
pub mod symmetry;
pub mod text;
pub mod text_animate;
pub mod time;
pub mod timing;
pub mod transform;
pub mod value;

#[cfg(test)]
mod lift_tests;
pub mod vec2;
pub mod video;

#[derive(Default)]
pub struct Registrar {
    factories: HashMap<&'static str, Factory>,
    dynamic: Vec<(&'static str, DynamicInputsFn)>,
    dynamic_outputs: Vec<(&'static str, DynamicInputsFn)>,
    adaptive: Vec<(&'static str, AdaptiveFn)>,
    lifted: std::collections::HashSet<&'static str>,
}

impl Registrar {
    pub fn node(&mut self, ty: &'static str, f: Factory) {
        let prev = self.factories.insert(ty, f);
        assert!(prev.is_none(), "node type {ty} registered twice");
    }

    pub fn dynamic(&mut self, ty: &'static str, f: DynamicInputsFn) {
        self.dynamic.push((ty, f));
    }

    /// Output ports whose types depend on params (names stay static).
    pub fn dynamic_outputs(&mut self, ty: &'static str, f: DynamicInputsFn) {
        self.dynamic_outputs.push((ty, f));
    }

    /// For an `adaptiveType` node that only adapts in some modes.
    pub fn adaptive_when(&mut self, ty: &'static str, f: AdaptiveFn) {
        self.adaptive.push((ty, f));
    }

    /// Node types that run once per element when a spread reaches a
    /// single-value pin (in patches from v4 on, see [`lifts`]). Their
    /// factories build nodes whose [`NodeImpl::lift_spreads`] says yes.
    ///
    /// [`NodeImpl::lift_spreads`]: crate::node::NodeImpl::lift_spreads
    pub fn lifted(&mut self, types: &[&'static str]) {
        self.lifted.extend(types.iter().copied());
    }
}

fn registry() -> &'static Registrar {
    static REG: OnceLock<Registrar> = OnceLock::new();
    REG.get_or_init(|| {
        let mut r = Registrar::default();
        collider::register(&mut r);
        color::register(&mut r);
        control::register(&mut r);
        convert::register(&mut r);
        data::register(&mut r);
        draw::register(&mut r);
        echo::register(&mut r);
        effect::register(&mut r);
        expr::register(&mut r);
        family::register(&mut r);
        format::register(&mut r);
        force::register(&mut r);
        geometry::register(&mut r);
        image::register(&mut r);
        gpu::register(&mut r);
        input::register(&mut r);
        io::register(&mut r);
        lfo::register(&mut r);
        logic::register(&mut r);
        math::register(&mut r);
        meta::register(&mut r);
        motion::register(&mut r);
        mover::register(&mut r);
        noise::register(&mut r);
        particles::register(&mut r);
        physics::register(&mut r);
        random::register(&mut r);
        render::register(&mut r);
        simulation::register(&mut r);
        sound::register(&mut r);
        spread::register(&mut r);
        symmetry::register(&mut r);
        string::register(&mut r);
        style::register(&mut r);
        text::register(&mut r);
        text_animate::register(&mut r);
        time::register(&mut r);
        timing::register(&mut r);
        transform::register(&mut r);
        value::register(&mut r);
        vec2::register(&mut r);
        video::register(&mut r);
        r
    })
}

/// Factory for a node type, if implemented.
pub fn factory(ty: &str) -> Option<Factory> {
    registry().factories.get(ty).copied()
}

/// Every registered dynamic-inputs function.
pub fn dynamic_inputs() -> Vec<(&'static str, DynamicInputsFn)> {
    registry().dynamic.clone()
}

/// Every registered dynamic-outputs function.
pub fn dynamic_outputs() -> Vec<(&'static str, DynamicInputsFn)> {
    registry().dynamic_outputs.clone()
}

/// Every registered adaptive-mode predicate.
pub fn adaptive_when() -> Vec<(&'static str, AdaptiveFn)> {
    registry().adaptive.clone()
}

/// Whether nodes of this type run once per element when a spread reaches a
/// single-value pin, in patches saved from v4 on (older files read slot 0;
/// see `PatchFile::legacy_slot0`). The editor lets a spread wire into such
/// a pin; the compiler keeps the wire instead of dropping it.
pub fn lifts(ty: &str) -> bool {
    registry().lifted.contains(ty)
}

/// Every lifting node type (see [`lifts`]), sorted.
pub fn lifted_types() -> Vec<&'static str> {
    let mut v: Vec<_> = registry().lifted.iter().copied().collect();
    v.sort_unstable();
    v
}

/// Number of node types with an implementation.
pub fn implemented_count() -> usize {
    registry().factories.len()
}
