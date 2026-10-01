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

pub mod audio;
pub mod compile;
pub mod devices;
pub mod graph;
pub mod host;
pub mod image;
pub mod js;
pub mod landmarks;
pub mod legacy;
pub mod node;
pub mod nodes;
pub mod particles;
pub mod patch;
pub mod scene;
pub mod shader;
pub mod spec;
pub mod text;
pub mod tick;
pub mod value;
pub mod video;

pub use compile::{compile, CompileResult, CompileWarning};
pub use node::{FrameCtx, ParamStore};
pub use patch::PatchFile;
pub use spec::Catalogue;
pub use tick::{Engine, TickResult};
pub use value::{Color, Spread, Value, Vec2};

/// Parse a `.flora` file the way the app opens one: version migrations,
/// then the web app's legacy fix-ups (old port names, draw x/y → vec2,
/// dead wires). Use this, not `PatchFile::load`, for user files.
pub fn load_patch(text: &str) -> Result<PatchFile, patch::LoadError> {
    let mut patch = PatchFile::load(text)?;
    legacy::normalize(&mut patch, Catalogue::builtin());
    Ok(patch)
}

impl Engine {
    /// Parse, migrate and compile a patch with the built-in catalogue.
    /// Returns the engine plus any compile warnings.
    pub fn load(text: &str) -> Result<Engine, String> {
        Self::load_with_warnings(text).map(|(e, _)| e)
    }

    pub fn load_with_warnings(text: &str) -> Result<(Engine, Vec<CompileWarning>), String> {
        let patch = load_patch(text).map_err(|e| e.to_string())?;
        Self::from_patch(&patch)
    }

    /// Compile an already-loaded patch. The param store is seeded from the
    /// patch's node params (what the editor's store holds after a load).
    pub fn from_patch(patch: &PatchFile) -> Result<(Engine, Vec<CompileWarning>), String> {
        let mut params = ParamStore::default();
        params.replace_all(patch.nodes.iter().map(|n| (n.id.as_str(), &n.params)));
        let result = compile(patch, Catalogue::builtin(), &params)?;
        Ok((Engine::new(result.graph, params), result.warnings))
    }
}
