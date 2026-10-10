//! Node-editor model: every editing operation is a plain function on a
//! `PatchFile`, so it unit-tests without a window. The canvas (`canvas.rs`)
//! maps pointer / keyboard input onto these.
//!
//! Behaviour reference: the web editor (`src/editor/`).

pub mod clipboard;
pub mod connect;
pub mod favorites;
pub mod history;
pub mod latch;
pub mod layout;
pub mod ops;
pub mod popups;
pub mod ports;
pub mod regions;
pub mod search;
pub mod subpatch;
pub mod values;
pub mod widgets;

use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;

/// The compile pre-pass run after every structural edit (the TS
/// `compile()` head): drop orphaned wires, then re-run the adaptive
/// `valueType` latch.
pub fn settle(patch: &mut PatchFile, cat: &Catalogue) {
    ops::prune_orphan_edges(patch);
    latch::reconcile_adaptive_types(patch, cat);
}

#[cfg(test)]
pub(crate) mod test_util {
    use floret_engine::patch::{PatchEdge, PatchFile, PatchNode, Position};

    pub fn node(id: &str, ty: &str) -> PatchNode {
        PatchNode {
            id: id.into(),
            def_type: ty.into(),
            position: Position::default(),
            params: Default::default(),
            parent_id: None,
            port_values: None,
            ports: None,
            width: None,
            height: None,
            bypassed: false,
        }
    }

    pub fn edge(s: &str, sh: &str, t: &str, th: &str) -> PatchEdge {
        PatchEdge {
            id: format!("{s}.{sh}->{t}.{th}"),
            source: s.into(),
            target: t.into(),
            source_handle: Some(sh.into()),
            target_handle: Some(th.into()),
        }
    }

    pub fn patch(nodes: Vec<PatchNode>, edges: Vec<PatchEdge>) -> PatchFile {
        PatchFile {
            kind: floret_engine::patch::PATCH_KIND.into(),
            version: floret_engine::patch::CURRENT_PATCH_VERSION,
            app_version: None,
            meta: None,
            nodes,
            edges,
            loop_length: None,
            seed: 0,
            presets: Vec::new(),
            legacy_slot0: false,
        }
    }
}
