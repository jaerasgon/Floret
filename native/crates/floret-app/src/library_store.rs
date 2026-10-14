//! Patch libraries as files (`src/persistence/library.ts`): the
//! `floret.library` JSON bundle the web app imports and downloads — a
//! name, an optional folder and a list of complete patches. Imported and
//! user-made libraries live as `<slug>.library.json` in the app's
//! library folder; built-in sample categories and "My patches" are
//! exported in the same format.

use floret_engine::patch::PatchFile;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

pub const KIND: &str = "floret.library";
pub const VERSION: u32 = 1;
const SUFFIX: &str = ".library.json";

/// What a library entry holds: a whole patch to open, or a subpatch
/// saved from the Subpatches panel, which inserts into the open patch.
/// Saved as `"kind": "subpatch"`; a missing `kind` is a patch, so older
/// files load unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    Patch,
    Subpatch,
}

impl EntryKind {
    pub fn is_patch(&self) -> bool {
        *self == EntryKind::Patch
    }
}

/// One patch in a library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibEntry {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "EntryKind::is_patch")]
    pub kind: EntryKind,
    /// A complete `.flora` patch, kept as JSON until it's opened.
    pub patch: Json,
}

impl LibEntry {
    pub fn load(&self) -> Result<PatchFile, String> {
        PatchFile::load(&self.patch.to_string()).map_err(|e| e.to_string())
    }
}

/// `PatchLibrary`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PatchLibrary {
    pub kind: String,
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub patches: Vec<LibEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

impl PatchLibrary {
    pub fn new(name: &str, folder: Option<&str>) -> PatchLibrary {
        PatchLibrary {
            kind: KIND.into(),
            version: VERSION,
            name: name.trim().to_owned(),
            description: None,
            author: None,
            patches: Vec::new(),
            folder: folder.map(str::to_owned),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// `upsertLibraryEntry`: add, or replace the entry with the same id.
    pub fn upsert(&mut self, entry: LibEntry) {
        match self.patches.iter_mut().find(|e| e.id == entry.id) {
            Some(e) => *e = entry,
            None => self.patches.push(entry),
        }
    }
}

/// `slugify`: lower-case, runs of anything else become `-`.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "library".into()
    } else {
        out
    }
}

/// `parseLibrary`: validate a `floret.library` JSON text, with the web
/// app's messages.
pub fn parse_library(text: &str) -> Result<PatchLibrary, String> {
    let raw: Json = serde_json::from_str(text).map_err(|e| format!("Could not parse JSON: {e}"))?;
    let obj = raw
        .as_object()
        .ok_or("Not a valid library: expected a JSON object")?;
    let bad = |m: &str| Err(format!("Not a valid library: {m}"));
    if obj.get("kind").and_then(Json::as_str) != Some(KIND) {
        let got = obj.get("kind").cloned().unwrap_or(Json::Null);
        return bad(&format!("kind must be \"{KIND}\", got {got}"));
    }
    if obj
        .get("name")
        .and_then(Json::as_str)
        .is_none_or(|n| n.trim().is_empty())
    {
        return bad("name is required");
    }
    let Some(patches) = obj.get("patches").and_then(Json::as_array) else {
        return bad("patches must be an array");
    };
    for p in patches {
        let Some(e) = p.as_object() else {
            return bad("each patch must be an object");
        };
        if !e.get("id").is_some_and(Json::is_string) {
            return bad("each patch needs an id");
        }
        if !e.get("name").is_some_and(Json::is_string) {
            return bad("each patch needs a name");
        }
        if !e.get("patch").is_some_and(Json::is_object) {
            return bad("each entry needs a patch (PatchFile)");
        }
    }
    if obj.get("builtIn").and_then(Json::as_bool) == Some(true) {
        return Err("Cannot import a library marked as built-in.".into());
    }
    let mut lib: PatchLibrary =
        serde_json::from_value(raw.clone()).map_err(|e| format!("Not a valid library: {e}"))?;
    if !obj.contains_key("version") {
        lib.version = VERSION;
    }
    Ok(lib)
}

/// A library entry for `patch` (`SaveToLibraryModal`): the id is the
/// slug of its name, and the patch carries the name.
pub fn entry_for(patch: &PatchFile, name: &str, description: &str, kind: EntryKind) -> LibEntry {
    let mut patch = patch.clone();
    let mut meta = patch.meta.take().unwrap_or_default();
    meta.name = Some(name.trim().to_owned());
    patch.meta = Some(meta);
    LibEntry {
        id: slugify(name),
        name: name.trim().to_owned(),
        description: Some(description.trim().to_owned()).filter(|d| !d.is_empty()),
        kind,
        patch: serde_json::from_str(&patch.to_json()).unwrap_or(Json::Null),
    }
}

/// The file a library is stored in, and the name it downloads as.
pub fn file_name(name: &str) -> String {
    format!("{}{SUFFIX}", slugify(name))
}

/// A stored (imported or user-made) library.
#[derive(Clone, Debug, PartialEq)]
pub struct Stored {
    pub slug: String,
    pub library: PatchLibrary,
}

/// Every `*.library.json` in `dir`, by name.
pub fn load_all(dir: &Path) -> Vec<Stored> {
    let mut out: Vec<Stored> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let slug = name.strip_suffix(SUFFIX)?.to_owned();
            let text = std::fs::read_to_string(e.path()).ok()?;
            let library = parse_library(&text).ok()?;
            Some(Stored { slug, library })
        })
        .collect();
    out.sort_by(|a, b| {
        a.library
            .name
            .to_lowercase()
            .cmp(&b.library.name.to_lowercase())
    });
    out
}

fn path_of(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("{slug}{SUFFIX}"))
}

/// `importLibrary`: store `library`, replacing one with the same slug.
pub fn store(dir: &Path, all: &mut Vec<Stored>, library: PatchLibrary) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let slug = slugify(&library.name);
    crate::files::write_atomic(&path_of(dir, &slug), library.to_json())?;
    let s = Stored {
        slug: slug.clone(),
        library,
    };
    match all.iter_mut().find(|x| x.slug == slug) {
        Some(x) => *x = s,
        None => all.push(s),
    }
    Ok(slug)
}

/// `removeLibrary`.
pub fn remove(dir: &Path, all: &mut Vec<Stored>, slug: &str) -> std::io::Result<()> {
    all.retain(|s| s.slug != slug);
    match std::fs::remove_file(path_of(dir, slug)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib_json(extra: &str) -> String {
        format!(
            r#"{{"kind":"floret.library","version":1,"name":"Night Sky",{extra}"patches":[
                {{"id":"stars","name":"Stars","patch":{{"kind":"floret.patch","version":2,"nodes":[],"edges":[]}}}}
            ]}}"#
        )
    }

    #[test]
    fn parses_and_rejects_like_the_web_app() {
        let lib = parse_library(&lib_json("")).unwrap();
        assert_eq!((lib.name.as_str(), lib.patches.len()), ("Night Sky", 1));
        assert!(lib.patches[0].load().is_ok());
        let err = |t: &str| parse_library(t).unwrap_err();
        assert!(err("[1]").contains("expected a JSON object"));
        assert!(err(r#"{"kind":"x"}"#).contains("kind must be \"floret.library\""));
        assert!(err(r#"{"kind":"floret.library","name":" ","patches":[]}"#)
            .contains("name is required"));
        assert!(
            err(r#"{"kind":"floret.library","name":"a","patches":[{"id":"x","name":"y"}]}"#)
                .contains("needs a patch")
        );
        assert!(err("{").starts_with("Could not parse JSON"));
        assert!(err(&lib_json(r#""builtIn":true,"#)).contains("built-in"));
    }

    #[test]
    fn an_entry_without_a_kind_is_a_patch() {
        // `lib_json`'s entry has no `kind`, as in files saved before
        // subpatches could be saved.
        let lib = parse_library(&lib_json("")).unwrap();
        assert_eq!(lib.patches[0].kind, EntryKind::Patch);
        assert!(!lib.to_json().contains("\"kind\": \"patch\""));
        let sub =
            lib_json("").replace(r#""name":"Stars","#, r#""name":"Stars","kind":"subpatch","#);
        let lib = parse_library(&sub).unwrap();
        assert_eq!(lib.patches[0].kind, EntryKind::Subpatch);
    }

    #[test]
    fn stores_round_trips_and_removes() {
        let dir = std::env::temp_dir().join(format!("floret-libstore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut all = Vec::new();
        let mut lib = parse_library(&lib_json(r#""folder":"Samples","#)).unwrap();
        let slug = store(&dir, &mut all, lib.clone()).unwrap();
        assert_eq!(slug, "night-sky");
        assert_eq!(load_all(&dir), all);
        // Re-import replaces by slug; entries upsert by id.
        let patch = crate::editor::test_util::patch(vec![], vec![]);
        lib.upsert(entry_for(&patch, "Stars", "twinkle", EntryKind::Patch));
        lib.upsert(entry_for(&patch, "Moon!", "", EntryKind::Subpatch));
        store(&dir, &mut all, lib).unwrap();
        let back = load_all(&dir);
        assert_eq!(back.len(), 1);
        let ids: Vec<_> = back[0]
            .library
            .patches
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, ["stars", "moon"]);
        assert_eq!(back[0].library.folder.as_deref(), Some("Samples"));
        let moon = back[0].library.patches[1].load().unwrap();
        assert_eq!(moon.meta.and_then(|m| m.name).as_deref(), Some("Moon!"));
        // What's exported re-imports unchanged.
        let text = back[0].library.to_json();
        assert_eq!(parse_library(&text).unwrap(), back[0].library);
        assert!(!text.contains("description\": null"));
        // Only the subpatch entry carries a `kind`, and it survives.
        assert_eq!(back[0].library.patches[0].kind, EntryKind::Patch);
        assert_eq!(back[0].library.patches[1].kind, EntryKind::Subpatch);
        assert_eq!(text.matches("\"kind\": \"subpatch\"").count(), 1);
        remove(&dir, &mut all, "night-sky").unwrap();
        assert!(load_all(&dir).is_empty() && all.is_empty());
        assert_eq!(file_name("Tutorials"), "tutorials.library.json");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
