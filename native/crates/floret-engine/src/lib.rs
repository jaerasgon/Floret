//! Floret's dataflow engine.
//!
//! Load a `.flora` patch, compile it into a runtime graph, and tick it once
//! per frame. This crate has no GPU, UI or OS dependencies; rendering,
//! media and the app live in sibling crates.
//!
//! ```no_run
//! use floret_engine::{Engine, FrameCtx};
//! let text = std::fs::read_to_string("patch.flora").unwrap();
//! let mut engine = Engine::load(&text).unwrap();
//! let result = engine.tick(&FrameCtx::default());
//! ```

pub mod compile;
pub mod graph;
pub mod node;
pub mod nodes;
pub mod patch;
pub mod spec;
pub mod tick;
pub mod value;

pub use compile::{compile, CompileResult, CompileWarning};
pub use node::{FrameCtx, ParamStore};
pub use patch::PatchFile;
pub use spec::Catalogue;
pub use tick::{Engine, TickResult};
pub use value::{Color, Spread, Value, Vec2};

impl Engine {
    /// Parse, migrate and compile a patch with the built-in catalogue.
    /// Returns the engine plus any compile warnings.
    pub fn load(text: &str) -> Result<Engine, String> {
        Self::load_with_warnings(text).map(|(e, _)| e)
    }

    pub fn load_with_warnings(text: &str) -> Result<(Engine, Vec<CompileWarning>), String> {
        let patch = PatchFile::load(text).map_err(|e| e.to_string())?;
        let mut params = ParamStore::default();
        params.replace_all(patch.nodes.iter().map(|n| (n.id.as_str(), &n.params)));
        let result = compile(&patch, Catalogue::builtin(), &params)?;
        Ok((Engine::new(result.graph, params), result.warnings))
    }
}
