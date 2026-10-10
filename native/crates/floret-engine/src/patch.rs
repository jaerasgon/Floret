//! `.flora` patch files: the versioned JSON format shared with the
//! TypeScript app.

use crate::spec::Params;
use serde::{Deserialize, Serialize};

pub const CURRENT_PATCH_VERSION: u32 = 4;
pub const PATCH_KIND: &str = "floret.patch";
/// Loop length (seconds) of a patch that doesn't set one.
pub const DEFAULT_LOOP_LENGTH: f64 = 4.0;

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
    /// Loop length in seconds: the time node's `loop` phase runs 0..1 over
    /// it, and video / GIF export records whole loops. Absent in older
    /// files (and not written while unset): [`DEFAULT_LOOP_LENGTH`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_length: Option<f64>,
    /// Patch seed mixed into every random / noise node (`crate::seed`).
    /// 0 (the default, left out of the file) keeps each node's own seed.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seed: u32,
    /// Saved parameter sets (the Presets panel, `crate::preset`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<Preset>,
    /// Saved before v4: a spread reaching a single-value pin of a scalar
    /// node (Math, Compare, Vec2, Colour …) reads only its slot 0, and a
    /// spread *output* wired into one is dropped with a warning. From v4 on
    /// those nodes run once per element instead (`crate::nodes::lifts`).
    /// Set by the migration ladder and kept on save, so an older file
    /// computes exactly as it did; absent (false) in newer files.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legacy_slot0: bool,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// A named snapshot of the patch's editable values, to recall or morph
/// between (see `crate::preset` for what it holds).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// The patch seed when saved.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seed: u32,
    /// Node id → its values.
    #[serde(default)]
    pub values: std::collections::BTreeMap<String, PresetValues>,
}

/// One node's values in a [`Preset`]: params by key, unwired input pins'
/// constants (`portValues`) by port name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetValues {
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub params: Params,
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub ports: Params,
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
    /// Switched off in the editor: outputs pass inputs through (see
    /// `crate::bypass`). Absent when off; builds without bypass ignore it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypassed: bool,
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
    /// Editor-only: where the port's pill sits on the frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
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

    /// The loop length in effect: the patch's own if it's a usable
    /// positive number, else [`DEFAULT_LOOP_LENGTH`].
    pub fn loop_seconds(&self) -> f64 {
        self.loop_length
            .filter(|l| l.is_finite() && *l > 0.0)
            .unwrap_or(DEFAULT_LOOP_LENGTH)
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
            2 => v2_to_v3(patch),
            3 => v3_to_v4(patch),
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

/// Nodes that read a position on an image: centre-origin from v3 on.
pub const IMAGE_POSITION_NODES: &[&str] = &["image/sample", "image/sample-spread"];
/// Hidden param on [`IMAGE_POSITION_NODES`]: `"top-left"` keeps the
/// pre-v3 pixel coordinates; absent (or anything else) is centre-origin.
pub const ORIGIN_PARAM: &str = "origin";
pub const ORIGIN_TOP_LEFT: &str = "top-left";

/// v2 → v3: image sampling became centre-origin like drawing (position
/// (0, 0) reads the image's centre, y down). Nodes saved before that read
/// top-left pixel coordinates, so they get `origin: "top-left"` and keep
/// sampling the same pixels. New nodes don't carry the param.
fn v2_to_v3(mut patch: PatchFile) -> PatchFile {
    for n in &mut patch.nodes {
        if IMAGE_POSITION_NODES.contains(&n.def_type.as_str())
            && !n.params.contains_key(ORIGIN_PARAM)
        {
            n.params.insert(ORIGIN_PARAM.into(), ORIGIN_TOP_LEFT.into());
        }
    }
    patch.version = 3;
    patch
}

/// v3 → v4: scalar nodes run per element over spreads. Older files keep
/// reading slot 0 (`legacy_slot0`), so they compute what they did.
fn v3_to_v4(mut patch: PatchFile) -> PatchFile {
    patch.legacy_slot0 = true;
    patch.version = 4;
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
        assert_eq!(p.version, CURRENT_PATCH_VERSION);
        assert_eq!(p.nodes[0].def_type, "math/add");
        assert_eq!(p.nodes[0].params["valueType"], "vec2");
        assert_eq!(p.edges[0].source_handle.as_deref(), Some("out-result"));
    }

    #[test]
    fn v2_image_sampling_keeps_its_top_left_origin() {
        let nodes = r#"[
            {"id":"s","defType":"image/sample","position":{"x":0,"y":0},"params":{}},
            {"id":"p","defType":"image/sample-spread","position":{"x":0,"y":0}},
            {"id":"c","defType":"image/crop","position":{"x":0,"y":0},"params":{}}]"#;
        let file = |v: u32| {
            format!(r#"{{"kind":"floret.patch","version":{v},"nodes":{nodes},"edges":[]}}"#)
        };
        // Saved before v3: the sampling nodes keep reading top-left pixels.
        let old = PatchFile::load(&file(2)).unwrap();
        assert_eq!(old.version, CURRENT_PATCH_VERSION);
        assert_eq!(old.nodes[0].params[ORIGIN_PARAM], ORIGIN_TOP_LEFT);
        assert_eq!(old.nodes[1].params[ORIGIN_PARAM], ORIGIN_TOP_LEFT);
        assert!(!old.nodes[2].params.contains_key(ORIGIN_PARAM));
        // Older still: the whole ladder runs.
        let v1 = PatchFile::load(&file(1)).unwrap();
        assert_eq!(v1.nodes[0].params[ORIGIN_PARAM], ORIGIN_TOP_LEFT);
        // Saved and reopened: migrated once, the param round-trips.
        let again = PatchFile::load(&old.to_json()).unwrap();
        assert_eq!(again.nodes[0].params[ORIGIN_PARAM], ORIGIN_TOP_LEFT);
        // Current files are centre-origin: nothing added.
        let new = PatchFile::load(&file(3)).unwrap();
        assert!(new
            .nodes
            .iter()
            .all(|n| !n.params.contains_key(ORIGIN_PARAM)));
    }

    #[test]
    fn library_samples_sample_images_centre_origin() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "flora") {
                continue;
            }
            let p = PatchFile::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for n in &p.nodes {
                if IMAGE_POSITION_NODES.contains(&n.def_type.as_str()) {
                    checked += 1;
                    assert!(
                        !n.params.contains_key(ORIGIN_PARAM),
                        "{}: {} samples top-left (save it as v{CURRENT_PATCH_VERSION})",
                        path.display(),
                        n.id
                    );
                }
            }
        }
        assert!(checked > 0, "no sample uses image sampling");
    }

    #[test]
    fn loop_length_is_optional_and_round_trips() {
        // Older files load unchanged, and an unset loop isn't written.
        let old = r#"{"kind":"floret.patch","version":2,"nodes":[],"edges":[]}"#;
        let mut p = PatchFile::load(old).unwrap();
        assert_eq!(p.loop_length, None);
        assert_eq!(p.loop_seconds(), DEFAULT_LOOP_LENGTH);
        assert!(!p.to_json().contains("loopLength"));
        // Set: written, read back, and in effect.
        p.loop_length = Some(2.5);
        let json = p.to_json();
        assert!(json.contains("\"loopLength\": 2.5"), "{json}");
        let back = PatchFile::load(&json).unwrap();
        assert_eq!(back.loop_length, Some(2.5));
        assert_eq!(back.loop_seconds(), 2.5);
        // Unusable values fall back to the default.
        for bad in [0.0, -1.0, f64::NAN] {
            p.loop_length = Some(bad);
            assert_eq!(p.loop_seconds(), DEFAULT_LOOP_LENGTH);
        }
    }

    #[test]
    fn older_files_keep_slot0_scalars() {
        let file =
            |v: u32| format!(r#"{{"kind":"floret.patch","version":{v},"nodes":[],"edges":[]}}"#);
        for v in 0..CURRENT_PATCH_VERSION {
            let p = PatchFile::load(&file(v)).unwrap();
            assert!(p.legacy_slot0, "v{v}");
            // Saved at the current version, it stays legacy.
            let json = p.to_json();
            assert!(json.contains("\"legacySlot0\": true"), "{json}");
            assert!(PatchFile::load(&json).unwrap().legacy_slot0);
        }
        // Current files lift, and don't write the flag.
        let p = PatchFile::load(&file(CURRENT_PATCH_VERSION)).unwrap();
        assert!(!p.legacy_slot0);
        assert!(!p.to_json().contains("legacySlot0"));
    }

    #[test]
    fn rejects_newer_versions() {
        let text = r#"{"kind":"floret.patch","version":99,"nodes":[],"edges":[]}"#;
        assert!(matches!(PatchFile::load(text), Err(LoadError::TooNew(99))));
    }
}
