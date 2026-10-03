//! Node implementations, one module per category.
//!
//! Each module exposes `register(&mut Registrar)` and adds a factory per
//! node type it implements (plus a dynamic-inputs function for node types
//! whose pins depend on params). A node type in the catalogue with no
//! factory still compiles — it just outputs empty spreads and is reported
//! as unimplemented.

use crate::node::Factory;
use crate::spec::DynamicInputsFn;
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod collider;
pub mod color;
pub mod convert;
pub mod data;
pub mod draw;
pub mod force;
pub mod gpu;
pub mod image;
pub mod input;
pub mod io;
pub mod lfo;
pub mod logic;
pub mod math;
pub mod meta;
pub mod motion;
pub mod net;
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
pub mod text;
pub mod time;
pub mod transform;
pub mod value;
pub mod vec2;
pub mod video;

#[derive(Default)]
pub struct Registrar {
    factories: HashMap<&'static str, Factory>,
    dynamic: Vec<(&'static str, DynamicInputsFn)>,
}

impl Registrar {
    pub fn node(&mut self, ty: &'static str, f: Factory) {
        let prev = self.factories.insert(ty, f);
        assert!(prev.is_none(), "node type {ty} registered twice");
    }

    pub fn dynamic(&mut self, ty: &'static str, f: DynamicInputsFn) {
        self.dynamic.push((ty, f));
    }
}

fn registry() -> &'static Registrar {
    static REG: OnceLock<Registrar> = OnceLock::new();
    REG.get_or_init(|| {
        let mut r = Registrar::default();
        collider::register(&mut r);
        color::register(&mut r);
        convert::register(&mut r);
        data::register(&mut r);
        draw::register(&mut r);
        force::register(&mut r);
        image::register(&mut r);
        gpu::register(&mut r);
        input::register(&mut r);
        io::register(&mut r);
        lfo::register(&mut r);
        logic::register(&mut r);
        math::register(&mut r);
        meta::register(&mut r);
        motion::register(&mut r);
        net::register(&mut r);
        noise::register(&mut r);
        particles::register(&mut r);
        physics::register(&mut r);
        random::register(&mut r);
        render::register(&mut r);
        simulation::register(&mut r);
        sound::register(&mut r);
        spread::register(&mut r);
        string::register(&mut r);
        style::register(&mut r);
        text::register(&mut r);
        time::register(&mut r);
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

/// Number of node types with an implementation.
pub fn implemented_count() -> usize {
    registry().factories.len()
}
