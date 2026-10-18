//! `.flora` documents (#73): reading and writing patch files the way the
//! web app does (`src/persistence/fs.ts`, `serialize.ts`), the open
//! document's identity and dirty state, recent files, and native dialogs.

use floret_engine::patch::{PatchFile, PatchMeta, CURRENT_PATCH_VERSION, PATCH_KIND};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "flora";
/// Extensions the Open dialog accepts (TS keeps the legacy two too).
const OPEN_EXTENSIONS: [&str; 2] = [EXTENSION, "json"];
const MAX_RECENT: usize = 10;

/// Floret's per-user data directory: autosaves, recovery copies,
/// thumbnails. `FLORET_DATA_DIR` overrides (like `FLORET_CONFIG_DIR`).
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("FLORET_DATA_DIR") {
        return PathBuf::from(d);
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let dir = if cfg!(target_os = "windows") {
        env("LOCALAPPDATA")
            .or_else(|| env("APPDATA"))
            .map(|d| d.join("Floret"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/Floret"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("floret"))
    };
    dir.unwrap_or_else(|| std::env::temp_dir().join("floret"))
}

/// Read and migrate a patch file (versions, then the web app's legacy
/// fix-ups), like opening one in the web app. Paths to picked files that
/// the file holds relative to its own folder come back absolute, so the
/// rest of the app only ever sees full paths ([`resolve_paths`]).
#[cfg(test)]
pub fn read(path: &Path) -> Result<PatchFile, String> {
    read_noting(path).map(|(patch, _)| patch)
}

/// [`read`], plus a line for each change the load made that the user
/// should hear about (a node type Floret no longer has, dropped).
pub fn read_noting(path: &Path) -> Result<(PatchFile, Vec<String>), String> {
    let (mut patch, notes) = read_unresolved(path)?;
    resolve_paths(&mut patch, path.parent());
    Ok((patch, notes))
}

/// [`read_noting`] without resolving relative file references (for copies
/// the app wrote itself, which don't live next to the files they name).
pub fn read_unresolved(path: &Path) -> Result<(PatchFile, Vec<String>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    floret_engine::load_patch_noting(&text).map_err(|e| e.to_string())
}

/// The file's JSON, as the web app writes it (`serializePatch` +
/// `JSON.stringify(patch, null, 2)`): current version, `appVersion`, `meta`
/// with name and ISO timestamps, empty `portValues` / `ports` omitted, and
/// integral numbers written without a fraction. The creation time is kept
/// when the patch has one.
pub fn to_flora(patch: &PatchFile, name: Option<&str>, now: &str) -> String {
    flora_text(patch, name, now, None)
}

/// [`to_flora`], with the patch's file references written for a patch
/// saved in `rebase.to` ([`portable_paths`]).
fn flora_text(patch: &PatchFile, name: Option<&str>, now: &str, rebase: Option<Rebase>) -> String {
    let mut p = patch.clone();
    if let Some(r) = rebase {
        portable_paths(&mut p, r);
    }
    p.kind = PATCH_KIND.to_owned();
    p.version = CURRENT_PATCH_VERSION;
    p.app_version = Some(env!("CARGO_PKG_VERSION").to_owned());
    let meta = p.meta.get_or_insert_with(PatchMeta::default);
    if let Some(n) = name {
        meta.name = Some(n.to_owned());
    }
    meta.created_at.get_or_insert_with(|| now.to_owned());
    meta.modified_at = Some(now.to_owned());
    for n in &mut p.nodes {
        if n.port_values.as_ref().is_some_and(|v| v.is_empty()) {
            n.port_values = None;
        }
        if n.ports.as_ref().is_some_and(|v| v.is_empty()) {
            n.ports = None;
        }
    }
    let mut json = serde_json::to_value(&p).expect("patch serialises");
    integral_numbers(&mut json);
    serde_json::to_string_pretty(&json).expect("json serialises")
}

/// `1.0` → `1`, as JavaScript prints numbers.
fn integral_numbers(v: &mut Json) {
    match v {
        Json::Number(n) => {
            if let Some(f) = n.as_f64().filter(|_| n.is_f64()) {
                if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
                    *v = Json::from(f as i64);
                }
            }
        }
        Json::Array(a) => a.iter_mut().for_each(integral_numbers),
        Json::Object(o) => o.values_mut().for_each(integral_numbers),
        _ => {}
    }
}

// ── Files a patch refers to ─────────────────────────────────────────────────
//
// Images, SVGs and audio picked on a node are embedded in the patch; videos,
// image sequences, fonts, LUTs and CSV files are referenced by path (they can
// be large). In the open patch every such path is absolute. On disk, one
// that sits beside the patch file or below it is written relative to it, with
// `/` for separators on every system, so a folder holding a patch and its
// media can be moved, zipped or shared and still work. Opening resolves
// relative paths against the patch's folder; absolute paths load as they
// are; paths outside the folder stay absolute.

/// Node params and input-pin constants that name a file, as (node type,
/// key). The sweep in the tests checks this against the catalogue.
pub const FILE_REFS: &[(&str, &str)] = &[
    ("video/file", "url"),
    ("video/frame-at", "url"),
    ("text/font", "file"),
    ("io/image-url", "url"),
    ("io/svg", "url"),
    ("data/csv-file", "url"),
    ("sound/sound-file", "url"),
    ("gpu/effect", "lut"),
];

/// Where a patch is being saved: the folder it goes in, and the folder it
/// was open from (`None` for a new patch).
#[derive(Clone, Copy)]
struct Rebase<'a> {
    from: Option<&'a Path>,
    to: &'a Path,
}

/// Run `f` on every file reference in `patch`, the saved presets' too.
pub(crate) fn map_file_refs(patch: &mut PatchFile, mut f: impl FnMut(&str) -> String) {
    let mut change = |params: &mut floret_engine::spec::Params, ty: &str| {
        for (t, key) in FILE_REFS {
            if *t != ty {
                continue;
            }
            if let Some(Json::String(s)) = params.get_mut(*key) {
                let new = f(s);
                if new != *s {
                    *s = new;
                }
            }
        }
    };
    let mut types = std::collections::HashMap::new();
    for n in &mut patch.nodes {
        types.insert(n.id.clone(), n.def_type.clone());
        change(&mut n.params, &n.def_type);
        if let Some(pv) = &mut n.port_values {
            change(pv, &n.def_type);
        }
    }
    for preset in &mut patch.presets {
        for (id, v) in &mut preset.values {
            if let Some(ty) = types.get(id) {
                change(&mut v.params, ty);
                change(&mut v.ports, ty);
            }
        }
    }
}

/// Make the relative file references in `patch` absolute, against the
/// folder the patch file is in (`None`: nothing to resolve against, so
/// they stay as they are). Done when a file is opened.
pub fn resolve_paths(patch: &mut PatchFile, dir: Option<&Path>) {
    let Some(dir) = dir.filter(|d| !d.as_os_str().is_empty()) else {
        return;
    };
    map_file_refs(patch, |s| map_lines(s, |r| resolve(r, dir)));
}

/// Write the file references in `patch` the way they are saved in
/// `rebase.to`: relative when beside or below it, else absolute.
fn portable_paths(patch: &mut PatchFile, rebase: Rebase) {
    map_file_refs(patch, |s| map_lines(s, |r| portable(r, rebase)));
}

/// Apply `f` to each non-empty line of a reference (a video's `url` may
/// list an image sequence's frames, one per line), keeping the lines'
/// own spacing. Only local paths are given to `f`: not `data:` URLs,
/// `http(s)` or `file://` URLs.
fn map_lines(s: &str, f: impl Fn(&str) -> String) -> String {
    if !s.contains('\n') {
        return map_line(s, &f);
    }
    s.split('\n')
        .map(|l| map_line(l, &f))
        .collect::<Vec<_>>()
        .join("\n")
}

fn map_line(line: &str, f: &impl Fn(&str) -> String) -> String {
    let t = line.trim();
    if t.is_empty() || !is_local_ref(t) {
        return line.to_owned();
    }
    let at = line.find(t).unwrap_or(0);
    format!("{}{}{}", &line[..at], f(t), &line[at + t.len()..])
}

/// Not a URL of any kind (`data:`, `http:`, `file:`, …): a path. A
/// Windows drive (`C:\…`) is not mistaken for a URL scheme.
pub(crate) fn is_local_ref(s: &str) -> bool {
    if is_absolute_ref(s) {
        return true;
    }
    match s.split_once(':') {
        Some((scheme, _)) => {
            !(scheme.len() > 1
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)))
        }
        None => true,
    }
}

/// An absolute path on any system: `/…`, `C:\…` or `C:/…`, `\\server\…`.
/// (A patch saved on Windows and opened on a Mac still has its drive
/// paths, which `Path::is_absolute` there would call relative.)
fn is_absolute_ref(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/')
        || s.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'/' || b[2] == b'\\'))
}

/// A relative reference joined onto `dir` (its `/` become the system's
/// separator); absolute ones as they are.
fn resolve(r: &str, dir: &Path) -> String {
    if is_absolute_ref(r) {
        return r.to_owned();
    }
    let mut p = dir.to_path_buf();
    for part in r.split('/').filter(|c| !c.is_empty() && *c != ".") {
        p.push(part);
    }
    p.to_string_lossy().into_owned()
}

/// One reference for a file saved in `rebase.to`.
fn portable(r: &str, rebase: Rebase) -> String {
    let absolute = if is_absolute_ref(r) {
        PathBuf::from(r)
    } else if let Some(from) = rebase.from {
        PathBuf::from(resolve(r, from))
    } else {
        // A relative path typed in the editor, with nothing to anchor it.
        return r.to_owned();
    };
    if let Some(rel) = below(&absolute, rebase.to) {
        return rel;
    }
    if !is_absolute_ref(r) && rebase.from == Some(rebase.to) {
        // Saved in place: the user's own relative path is left alone.
        return r.to_owned();
    }
    absolute.to_string_lossy().into_owned()
}

/// `path` relative to `dir` with `/` separators, when it is inside it.
fn below(path: &Path, dir: &Path) -> Option<String> {
    let clean = |rel: &Path| -> Option<String> {
        let parts: Option<Vec<String>> = rel
            .components()
            .map(|c| match c {
                std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        parts.filter(|p| !p.is_empty()).map(|p| p.join("/"))
    };
    if let Some(rel) = path.strip_prefix(dir).ok().and_then(clean) {
        return Some(rel);
    }
    // The same place under another name (a symlinked folder, `/var` and
    // `/private/var`, a different capitalisation on a Mac).
    let dir = dir.canonicalize().ok()?;
    let path = canonical_or_parent(path)?;
    path.strip_prefix(&dir).ok().and_then(clean)
}

/// The canonical path, or for one that doesn't exist (yet) its nearest
/// existing folder's with the rest appended.
fn canonical_or_parent(path: &Path) -> Option<PathBuf> {
    if let Ok(p) = path.canonicalize() {
        return Some(p);
    }
    let name = path.file_name()?;
    Some(canonical_or_parent(path.parent()?)?.join(name))
}

/// Write `text` to `path` through a temporary file and a rename, so a
/// failed or interrupted save never leaves a half-written patch.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Save `patch` to `path`: the file is named after its stem. Picked files
/// that sit beside the patch or below it are written relative to it.
pub fn save(patch: &PatchFile, path: &Path) -> Result<(), String> {
    save_from(patch, path, None)
}

/// [`save`] for a patch that was open from the folder `from` (`None`:
/// untitled, or a built-in sample): file references still relative to it
/// are made to point at the same files from the new place.
pub fn save_from(patch: &PatchFile, path: &Path, from: Option<&Path>) -> Result<(), String> {
    let to = path.parent().filter(|d| !d.as_os_str().is_empty());
    let rebase = to.map(|to| Rebase { from, to });
    let text = flora_text(patch, Some(&file_stem(path)), &now_iso(), rebase);
    write_atomic(path, &text).map_err(|e| e.to_string())
}

/// `sketch.flora` → `sketch` (also `x.floret.json` → `x`).
pub fn file_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    for ext in [".floret.json", ".flora", ".json"] {
        if let Some(s) = name.strip_suffix(ext) {
            return s.to_owned();
        }
    }
    name
}

/// The web app's `suggestedFilename` slug.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || "._-".contains(c) {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    if out.is_empty() {
        "untitled".to_owned()
    } else {
        out
    }
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `Date.toISOString()` for now: `2026-10-01T09:30:00.000Z`.
pub fn now_iso() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    iso(d.as_secs(), d.subsec_millis())
}

fn iso(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// Where the open patch came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// New, unsaved.
    Untitled,
    /// A built-in sample (saving asks where).
    Sample(&'static str),
    /// A file on disk.
    File(PathBuf),
}

/// The open document: where it lives and whether it has unsaved changes.
/// Changes are tracked by the editor's undo revision, so undoing back to
/// the saved state makes it clean again.
#[derive(Clone, Debug)]
pub struct Document {
    pub origin: Origin,
    pub name: String,
    saved_rev: u64,
    /// Forced dirty (a restored autosave) until the next save.
    force_dirty: bool,
}

impl Document {
    pub fn untitled(rev: u64) -> Document {
        Document {
            origin: Origin::Untitled,
            name: "Untitled".into(),
            saved_rev: rev,
            force_dirty: false,
        }
    }

    pub fn sample(id: &'static str, name: &str, rev: u64) -> Document {
        Document {
            origin: Origin::Sample(id),
            name: name.to_owned(),
            saved_rev: rev,
            force_dirty: false,
        }
    }

    pub fn file(path: PathBuf, rev: u64) -> Document {
        Document {
            name: file_stem(&path),
            origin: Origin::File(path),
            saved_rev: rev,
            force_dirty: false,
        }
    }

    /// A restored autosave: unsaved until written somewhere.
    pub fn recovered(origin: Origin, name: String, rev: u64) -> Document {
        Document {
            origin,
            name,
            saved_rev: rev,
            force_dirty: true,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.origin {
            Origin::File(p) => Some(p),
            _ => None,
        }
    }

    pub fn is_dirty(&self, rev: u64) -> bool {
        self.force_dirty || rev != self.saved_rev
    }

    /// Saved to `path` at revision `rev`.
    pub fn saved_as(&mut self, path: PathBuf, rev: u64) {
        *self = Document::file(path, rev);
    }

    /// Window title: `name — Floret`, with `*` while there are unsaved
    /// changes.
    pub fn title(&self, rev: u64) -> String {
        let star = if self.is_dirty(rev) { "*" } else { "" };
        format!("{star}{} — Floret", self.name)
    }

    /// Suggested file name for Save As.
    pub fn suggested_file_name(&self) -> String {
        format!("{}.{EXTENSION}", slug(&self.name))
    }
}

/// Recently opened / saved files, newest first, persisted as a JSON array
/// of paths in the config dir.
#[derive(Default)]
pub struct Recent {
    paths: Vec<PathBuf>,
    store: Option<PathBuf>,
}

impl Recent {
    pub fn load() -> Recent {
        Self::load_from(crate::editor::favorites::config_dir().map(|d| d.join("recent.json")))
    }

    pub fn load_from(store: Option<PathBuf>) -> Recent {
        let paths = store
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Vec<PathBuf>>(&t).ok())
            .unwrap_or_default();
        Recent { paths, store }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Move `path` to the front.
    pub fn push(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT);
        self.persist();
    }

    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.persist();
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.persist();
    }

    fn persist(&self) {
        if let Some(p) = &self.store {
            // Best effort: a read-only config dir just loses persistence.
            if let Ok(text) = serde_json::to_string_pretty(&self.paths) {
                let _ = write_atomic(p, &text);
            }
        }
    }
}

/// The native Open dialog.
pub fn open_dialog(start: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .set_title("Open patch")
        .add_filter("Floret patch", &OPEN_EXTENSIONS);
    if let Some(dir) = start.and_then(Path::parent) {
        d = d.set_directory(dir);
    }
    d.pick_file()
}

/// The native Save dialog; adds `.flora` when the name has no extension.
pub fn save_dialog(doc: &Document) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .set_title("Save patch")
        .add_filter("Floret patch", &[EXTENSION])
        .set_file_name(doc.suggested_file_name());
    if let Some(dir) = doc.path().and_then(Path::parent) {
        d = d.set_directory(dir);
    }
    let path = d.save_file()?;
    Some(if path.extension().is_none() {
        path.with_extension(EXTENSION)
    } else {
        path
    })
}

/// A command-line argument naming a patch file (the first one that isn't
/// a flag). Finder / Explorer "Open with" pass the file this way.
pub fn path_from_args(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<PathBuf> {
    args.into_iter()
        .skip(1)
        .map(PathBuf::from)
        .find(|a| !a.to_string_lossy().starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use floret_engine::patch::{PatchEdge, PatchNode, Position, RegionPort};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("floret-files-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_patch() -> PatchFile {
        let text = include_str!("../../../samples/tut-03-animate.flora");
        floret_engine::load_patch(text).unwrap()
    }

    #[test]
    fn iso_matches_javascript() {
        assert_eq!(iso(0, 0), "1970-01-01T00:00:00.000Z");
        // new Date(1790812800123).toISOString()
        assert_eq!(iso(1_790_812_800, 123), "2026-10-01T00:00:00.123Z");
        assert_eq!(iso(951_782_400, 5), "2000-02-29T00:00:00.005Z");
    }

    #[test]
    fn writes_the_web_apps_shape() {
        let mut p = sample_patch();
        p.meta = None;
        p.nodes[0].position = Position { x: 120.0, y: -40.5 };
        p.nodes[0].port_values = Some(Default::default());
        let text = to_flora(&p, Some("My Sketch"), "2026-10-01T00:00:00.000Z");
        let v: Json = serde_json::from_str(&text).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["kind", "version", "appVersion", "meta", "nodes", "edges"]
        );
        assert_eq!(v["kind"], "floret.patch");
        assert_eq!(v["version"], CURRENT_PATCH_VERSION);
        assert_eq!(v["meta"]["name"], "My Sketch");
        assert_eq!(v["meta"]["createdAt"], "2026-10-01T00:00:00.000Z");
        assert_eq!(v["meta"]["modifiedAt"], "2026-10-01T00:00:00.000Z");
        let n0 = &v["nodes"][0];
        let keys: Vec<&str> = n0.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(&keys[..4], ["id", "defType", "position", "params"]);
        assert!(
            n0.get("portValues").is_none(),
            "empty portValues are omitted"
        );
        assert!(
            text.contains("\"x\": 120,"),
            "integral numbers print like JS"
        );
        assert!(text.contains("\"y\": -40.5"));
        assert!(v["edges"][0].get("sourceHandle").is_some());
        assert!(text.starts_with("{\n  \"kind\""), "2-space pretty print");
    }

    #[test]
    fn keeps_created_at_and_round_trips() {
        let mut p = sample_patch();
        p.meta = Some(PatchMeta {
            created_at: Some("2020-01-01T00:00:00.000Z".into()),
            ..Default::default()
        });
        let text = to_flora(&p, None, "2026-10-01T00:00:00.000Z");
        let back = floret_engine::load_patch(&text).unwrap();
        let m = back.meta.as_ref().unwrap();
        assert_eq!(m.created_at.as_deref(), Some("2020-01-01T00:00:00.000Z"));
        assert_eq!(back.nodes.len(), p.nodes.len());
        assert_eq!(back.edges.len(), p.edges.len());
        for (a, b) in p.nodes.iter().zip(&back.nodes) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.def_type, b.def_type);
            assert_eq!(a.position, b.position);
            for (k, v) in &a.params {
                assert_eq!(v.as_f64(), b.params[k].as_f64(), "{}.{k}", a.id);
            }
        }
    }

    #[test]
    fn saves_the_seed_and_presets() {
        let mut p = sample_patch();
        p.seed = 1234;
        p.presets.push(floret_engine::preset::capture(
            floret_engine::spec::Catalogue::builtin(),
            &p,
            "Mine",
        ));
        let text = to_flora(&p, None, "2026-10-01T00:00:00.000Z");
        let v: Json = serde_json::from_str(&text).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "kind",
                "version",
                "appVersion",
                "meta",
                "nodes",
                "edges",
                "seed",
                "presets"
            ]
        );
        let back = floret_engine::load_patch(&text).unwrap();
        assert_eq!(back.seed, 1234);
        assert_eq!(back.presets.len(), 1);
        assert_eq!(back.presets[0].name, "Mine");
        assert_eq!(back.presets[0].seed, 1234);
        assert_eq!(back.presets[0].values.len(), p.presets[0].values.len());
    }

    /// A file the web app wrote (`serializePatch` output) opens, and saving
    /// it again keeps every field the web app reads.
    #[test]
    fn opens_web_app_files() {
        let web = r#"{
  "kind": "floret.patch",
  "version": 2,
  "appVersion": "0.0.1",
  "meta": { "name": "web", "createdAt": "2026-01-01T00:00:00.000Z", "modifiedAt": "2026-01-01T00:00:00.000Z" },
  "nodes": [
    { "id": "fe", "defType": "region/foreach", "position": { "x": 0, "y": 0 }, "params": {},
      "ports": [{ "id": "p1", "name": "item", "type": "number", "direction": "input" }],
      "width": 300, "height": 200 },
    { "id": "n", "defType": "value/number", "position": { "x": 10, "y": 20 }, "params": { "value": 3 },
      "parentId": "fe", "portValues": { "value": 3 } }
  ],
  "edges": [{ "id": "e1", "source": "n", "target": "fe", "sourceHandle": "out-value", "targetHandle": null }]
}"#;
        let p = floret_engine::load_patch(web).unwrap();
        assert_eq!(p.nodes[1].parent_id.as_deref(), Some("fe"));
        let out = to_flora(&p, None, "2026-10-01T00:00:00.000Z");
        let v: Json = serde_json::from_str(&out).unwrap();
        assert_eq!(v["nodes"][0]["ports"][0]["direction"], "input");
        assert_eq!(v["nodes"][0]["width"], 300);
        assert_eq!(v["nodes"][1]["parentId"], "fe");
        assert_eq!(v["nodes"][1]["portValues"]["value"], 3);
        assert_eq!(v["edges"][0]["targetHandle"], Json::Null);
        assert_eq!(v["meta"]["createdAt"], "2026-01-01T00:00:00.000Z");
    }

    #[test]
    fn save_writes_atomically_and_reads_back() {
        let dir = tmp("save");
        let path = dir.join("Sketch One.flora");
        let mut p = sample_patch();
        p.nodes.push(PatchNode {
            id: "r".into(),
            def_type: "region/foreach".into(),
            position: Position::default(),
            params: Default::default(),
            parent_id: None,
            port_values: None,
            ports: Some(vec![RegionPort {
                id: "p".into(),
                name: "x".into(),
                ty: "number".into(),
                direction: floret_engine::patch::PortDirection::Input,
                y: Some(40.0),
            }]),
            width: Some(200.0),
            height: Some(100.0),
            bypassed: false,
            extra: Default::default(),
        });
        p.edges.push(PatchEdge {
            id: "dangling".into(),
            source: "a".into(),
            target: "b".into(),
            source_handle: None,
            target_handle: None,
        });
        save(&p, &path).unwrap();
        assert!(!dir.join("Sketch One.flora.tmp").exists());
        let back = read(&path).unwrap();
        assert_eq!(back.meta.unwrap().name.as_deref(), Some("Sketch One"));
        let r = back.nodes.iter().find(|n| n.id == "r").unwrap();
        assert_eq!(r.ports.as_ref().unwrap()[0].y, Some(40.0));
        // Overwrite in place.
        save(&sample_patch(), &path).unwrap();
        assert!(read(&path).unwrap().nodes.iter().all(|n| n.id != "r"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn large_patches_save_and_load() {
        let dir = tmp("large");
        let mut p = sample_patch();
        let blob = "data:image/png;base64,".to_owned() + &"A".repeat(4 << 20);
        for i in 0..5000 {
            let mut n = p.nodes[0].clone();
            n.id = format!("big_{i}");
            n.position = Position {
                x: f64::from(i % 100) * 180.0,
                y: f64::from(i / 100) * 90.0,
            };
            p.nodes.push(n);
        }
        p.nodes[0].params.insert("src".into(), blob.clone().into());
        let path = dir.join("big.flora");
        save(&p, &path).unwrap();
        let back = read(&path).unwrap();
        assert_eq!(back.nodes.len(), p.nodes.len());
        assert_eq!(
            back.nodes[0].params["src"].as_str().unwrap().len(),
            blob.len()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn read_reports_bad_files() {
        let dir = tmp("bad");
        let path = dir.join("x.flora");
        std::fs::write(&path, "{nope").unwrap();
        assert!(read(&path).unwrap_err().contains("not valid JSON"));
        std::fs::write(
            &path,
            r#"{"kind":"other","version":1,"nodes":[],"edges":[]}"#,
        )
        .unwrap();
        assert!(read(&path).unwrap_err().contains("not a Floret patch"));
        assert!(read(&dir.join("missing.flora")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn document_dirty_and_titles() {
        let mut d = Document::untitled(1);
        assert!(!d.is_dirty(1));
        assert!(d.is_dirty(2));
        assert_eq!(d.title(1), "Untitled — Floret");
        assert_eq!(d.title(2), "*Untitled — Floret");
        assert_eq!(d.suggested_file_name(), "untitled.flora");
        d.saved_as(PathBuf::from("/x/My Patch.flora"), 2);
        assert!(!d.is_dirty(2));
        assert_eq!(d.name, "My Patch");
        assert_eq!(d.suggested_file_name(), "my-patch.flora");
        let r = Document::recovered(Origin::Untitled, "a".into(), 5);
        assert!(r.is_dirty(5), "a restored autosave stays unsaved");
        assert_eq!(file_stem(Path::new("a/b.floret.json")), "b");
    }

    #[test]
    fn recent_files_are_ordered_capped_and_persist() {
        let dir = tmp("recent");
        let store = dir.join("recent.json");
        let mut r = Recent::load_from(Some(store.clone()));
        for i in 0..12 {
            r.push(&dir.join(format!("{i}.flora")));
        }
        assert_eq!(r.paths().len(), MAX_RECENT);
        assert!(r.paths()[0].ends_with("11.flora"));
        r.push(&dir.join("5.flora"));
        assert!(r.paths()[0].ends_with("5.flora"));
        assert_eq!(
            r.paths().iter().filter(|p| p.ends_with("5.flora")).count(),
            1
        );
        let again = Recent::load_from(Some(store));
        assert_eq!(again.paths(), r.paths());
        let first = r.paths()[0].clone();
        r.remove(&first);
        assert!(!r.paths().contains(&first));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn command_line_path() {
        let args = ["floret", "--verbose", "a b.flora"].map(std::ffi::OsString::from);
        assert_eq!(path_from_args(args), Some(PathBuf::from("a b.flora")));
        assert_eq!(
            path_from_args(["floret"].map(std::ffi::OsString::from)),
            None
        );
    }
}
