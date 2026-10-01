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

pub mod color;
pub mod convert;
pub mod data;
pub mod input;
pub mod io;
pub mod logic;
pub mod math;
pub mod meta;
pub mod motion;
pub mod noise;
pub mod random;
pub mod render;
pub mod shared;
pub mod spread;
pub mod string;
pub mod time;
pub mod value;
pub mod vec2;

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
        color::register(&mut r);
        convert::register(&mut r);
        data::register(&mut r);
        input::register(&mut r);
        io::register(&mut r);
        logic::register(&mut r);
        math::register(&mut r);
        meta::register(&mut r);
        motion::register(&mut r);
        noise::register(&mut r);
        random::register(&mut r);
        render::register(&mut r);
        spread::register(&mut r);
        string::register(&mut r);
        time::register(&mut r);
        value::register(&mut r);
        vec2::register(&mut r);
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
