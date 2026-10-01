//! `.flora` patch files: the versioned JSON format shared with the
//! TypeScript app.

use crate::spec::Params;
use serde::{Deserialize, Serialize};

pub const CURRENT_PATCH_VERSION: u32 = 2;
pub const PATCH_KIND: &str = "floret.patch";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchFile {
    pub kind: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<PatchMeta>,
    pub nodes: Vec<PatchNode>,
    pub edges: Vec<PatchEdge>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchNode {
    pub id: String,
    pub def_type: String,
    pub position: Position,
    #[serde(default)]
    pub params: Params,
    /// Enclosing region (absent for root-scope nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Per-input constant overrides, keyed by input port name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_values: Option<Params>,
    /// Dynamic frame ports on region nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<RegionPort>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegionPort {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub direction: PortDirection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_handle: Option<String>,
    pub target_handle: Option<String>,
}

/// A handle id (`in-<name>` / `out-<name>`) split into its side and name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle<'a> {
    In(&'a str),
    Out(&'a str),
}

pub fn parse_handle(h: Option<&str>) -> Option<Handle<'_>> {
    let h = h?;
    if let Some(n) = h.strip_prefix("in-") {
        Some(Handle::In(n))
    } else {
        h.strip_prefix("out-").map(Handle::Out)
    }
}

#[derive(Debug)]
pub enum LoadError {
    Json(serde_json::Error),
    NotAPatch,
    TooNew(u32),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Json(e) => write!(f, "not valid JSON: {e}"),
            LoadError::NotAPatch => write!(f, "not a Floret patch file"),
            LoadError::TooNew(v) => write!(
                f,
                "patch version {v} is newer than this app (max {CURRENT_PATCH_VERSION}). Please update Floret."
            ),
        }
    }
}

impl std::error::Error for LoadError {}

impl PatchFile {
    /// Parse a `.flora` file and bring it up to the current version.
    pub fn load(text: &str) -> Result<PatchFile, LoadError> {
        let patch: PatchFile = serde_json::from_str(text).map_err(LoadError::Json)?;
        if patch.kind != PATCH_KIND {
            return Err(LoadError::NotAPatch);
        }
        migrate(patch)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("patch serialises")
    }
}

/// Run the migration ladder up to `CURRENT_PATCH_VERSION`.
pub fn migrate(mut patch: PatchFile) -> Result<PatchFile, LoadError> {
    if patch.version > CURRENT_PATCH_VERSION {
        return Err(LoadError::TooNew(patch.version));
    }
    while patch.version < CURRENT_PATCH_VERSION {
        patch = match patch.version {
            0 | 1 => v1_to_v2(patch),
            v => unreachable!("no migration from v{v}"),
        };
    }
    Ok(patch)
}

/// v1 → v2: the vec2 arithmetic nodes folded into the adaptive math nodes.
fn v1_to_v2(mut patch: PatchFile) -> PatchFile {
    let mut migrated = std::collections::HashSet::new();
    for n in &mut patch.nodes {
        let mapped = match n.def_type.as_str() {
            "vec2/add" => "math/add",
            "vec2/sub" => "math/sub",
            "vec2/mul" => "math/mul",
            "vec2/lerp" => "math/lerp",
            _ => continue,
        };
        n.def_type = mapped.to_owned();
        n.params.insert("valueType".into(), "vec2".into());
        migrated.insert(n.id.clone());
    }
    for e in &mut patch.edges {
        if migrated.contains(&e.source) && e.source_handle.as_deref() == Some("out-vector") {
            e.source_handle = Some("out-result".into());
        }
    }
    patch.version = 2;
    patch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_vec2_math() {
        let text = r#"{"kind":"floret.patch","version":1,"nodes":[
            {"id":"a","defType":"vec2/add","position":{"x":0,"y":0},"params":{}}],
            "edges":[{"id":"e","source":"a","target":"b","sourceHandle":"out-vector","targetHandle":"in-pos"}]}"#;
        let p = PatchFile::load(text).unwrap();
        assert_eq!(p.version, 2);
        assert_eq!(p.nodes[0].def_type, "math/add");
        assert_eq!(p.nodes[0].params["valueType"], "vec2");
        assert_eq!(p.edges[0].source_handle.as_deref(), Some("out-result"));
    }

    #[test]
    fn rejects_newer_versions() {
        let text = r#"{"kind":"floret.patch","version":99,"nodes":[],"edges":[]}"#;
        assert!(matches!(PatchFile::load(text), Err(LoadError::TooNew(99))));
    }
}
