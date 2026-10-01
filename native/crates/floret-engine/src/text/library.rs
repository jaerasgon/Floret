//! The process-wide font library: installed system fonts, the bundled
//! faces, and font files loaded by `text/font`. The renderer builds its
//! cosmic-text font system from a snapshot of the same database
//! ([`database`]), so a family name resolves to the same face on both
//! sides.

use fontdb::{Database, Family, Query, Source, Style, Weight, ID};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

/// A face in the library. Valid for the life of the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FaceId(pub(crate) ID);

/// One variation axis of a variable face (user-space values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisInfo {
    pub tag: [u8; 4],
    pub min: f32,
    pub default: f32,
    pub max: f32,
}

impl AxisInfo {
    pub fn tag_str(&self) -> String {
        String::from_utf8_lossy(&self.tag).into_owned()
    }
}

/// A face's bytes, kept in memory once the face is used.
pub struct FaceData {
    pub id: FaceId,
    bytes: Arc<dyn AsRef<[u8]> + Send + Sync>,
    index: u32,
    pub units_per_em: f32,
    pub axes: Vec<AxisInfo>,
}

impl FaceData {
    pub fn bytes(&self) -> &[u8] {
        (*self.bytes).as_ref()
    }

    /// Face index within a collection (`.ttc`).
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn font(&self) -> Option<skrifa::FontRef<'_>> {
        skrifa::FontRef::from_index(self.bytes(), self.index).ok()
    }

    pub fn has_char(&self, c: char) -> bool {
        use skrifa::MetadataProvider;
        self.font()
            .and_then(|f| f.charmap().map(c))
            .is_some_and(|g| g.to_u32() != 0)
    }

    pub fn axis(&self, tag: [u8; 4]) -> Option<&AxisInfo> {
        self.axes.iter().find(|a| a.tag == tag)
    }
}

impl std::fmt::Debug for FaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceData")
            .field("id", &self.id)
            .field("index", &self.index)
            .field("axes", &self.axes)
            .finish()
    }
}

/// A font file loaded with [`load_file`].
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedFont {
    /// The first face's family name: what `style/text-params` takes.
    pub family: String,
    pub faces: Vec<FaceId>,
}

/// The faces behind the generic families, bundled so `sans-serif`, `serif`
/// and `monospace` draw the same on every OS. They are what Chrome on Linux
/// picks (the reference images), metric-compatible with Arial / Times.
/// Roboto Flex is a variable font with many axes (`wght`, `wdth`, `slnt`,
/// `opsz`, `GRAD`, `XTRA` …), subset to Latin.
const BUNDLED: &[&[u8]] = &[
    include_bytes!("../../fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../../fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../../fonts/LiberationSans-Italic.ttf"),
    include_bytes!("../../fonts/LiberationSans-BoldItalic.ttf"),
    include_bytes!("../../fonts/LiberationSerif-Regular.ttf"),
    include_bytes!("../../fonts/LiberationSerif-Bold.ttf"),
    include_bytes!("../../fonts/LiberationSerif-Italic.ttf"),
    include_bytes!("../../fonts/LiberationSerif-BoldItalic.ttf"),
    include_bytes!("../../fonts/DejaVuSansMono.ttf"),
    include_bytes!("../../fonts/DejaVuSansMono-Bold.ttf"),
    include_bytes!("../../fonts/DejaVuSansMono-Oblique.ttf"),
    include_bytes!("../../fonts/DejaVuSansMono-BoldOblique.ttf"),
    include_bytes!("../../fonts/RobotoFlex-Variable.ttf"),
];

/// The bundled variable font's family.
pub const VARIABLE_FAMILY: &str = "Roboto Flex";

struct FileEntry {
    modified: Option<SystemTime>,
    result: Result<LoadedFont, String>,
}

struct Library {
    db: Database,
    data: HashMap<ID, Arc<FaceData>>,
    files: HashMap<PathBuf, FileEntry>,
    fallback: HashMap<(char, ID), Option<ID>>,
    queries: HashMap<(String, u16, bool), Option<ID>>,
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

fn lib() -> &'static RwLock<Library> {
    static LIB: OnceLock<RwLock<Library>> = OnceLock::new();
    LIB.get_or_init(|| RwLock::new(Library::new()))
}

impl Library {
    /// System fonts (for named families) plus the bundled faces, which back
    /// every generic family. `FLORET_BUNDLED_FONTS_ONLY` skips the system
    /// ones, so output doesn't depend on the machine (reference images).
    fn new() -> Library {
        let mut db = Database::new();
        if std::env::var_os("FLORET_BUNDLED_FONTS_ONLY").is_none() {
            db.load_system_fonts();
        }
        for face in BUNDLED {
            db.load_font_source(Source::Binary(Arc::new(*face)));
        }
        db.set_sans_serif_family("Liberation Sans");
        db.set_cursive_family("Liberation Sans");
        db.set_fantasy_family("Liberation Sans");
        db.set_serif_family("Liberation Serif");
        db.set_monospace_family("DejaVu Sans Mono");
        Library {
            db,
            data: HashMap::new(),
            files: HashMap::new(),
            fallback: HashMap::new(),
            queries: HashMap::new(),
        }
    }

    fn installed(&self, name: &str) -> Option<String> {
        self.db
            .faces()
            .flat_map(|f| f.families.iter())
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(n, _)| n.clone())
    }

    /// Bytes for a face, read once (files are read, not mapped).
    fn face_data(&mut self, id: ID) -> Option<Arc<FaceData>> {
        if let Some(d) = self.data.get(&id) {
            return Some(d.clone());
        }
        let (source, index) = self.db.face_source(id)?;
        let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = match source {
            Source::Binary(b) => b,
            Source::SharedFile(_, b) => b,
            Source::File(path) => {
                // Share one copy between the faces of a collection.
                let other = self.data.values().find(|d| {
                    self.db
                        .face_source(d.id.0)
                        .is_some_and(|(s, _)| matches!(&s, Source::File(p) if *p == path))
                });
                match other {
                    Some(d) => d.bytes.clone(),
                    None => Arc::new(std::fs::read(&path).ok()?),
                }
            }
        };
        let font = skrifa::FontRef::from_index((*bytes).as_ref(), index).ok()?;
        let data = Arc::new(FaceData {
            id: FaceId(id),
            units_per_em: font_upem(&font),
            axes: font_axes(&font),
            bytes,
            index,
        });
        self.data.insert(id, data.clone());
        Some(data)
    }
}

fn font_upem(font: &skrifa::FontRef) -> f32 {
    use skrifa::raw::TableProvider;
    font.head()
        .map(|h| f32::from(h.units_per_em()))
        .ok()
        .filter(|u| *u > 0.0)
        .unwrap_or(1000.0)
}

fn font_axes(font: &skrifa::FontRef) -> Vec<AxisInfo> {
    use skrifa::MetadataProvider;
    font.axes()
        .iter()
        .map(|a| AxisInfo {
            tag: a.tag().to_be_bytes(),
            min: a.min_value(),
            default: a.default_value(),
            max: a.max_value(),
        })
        .collect()
}

/// Bumped whenever faces are added; the renderer re-snapshots on change.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// A copy of the font database (for cosmic-text).
pub fn database() -> Database {
    lib().read().unwrap_or_else(|e| e.into_inner()).db.clone()
}

/// Every installed family name, sorted, without duplicates.
pub fn families() -> Vec<String> {
    let lib = lib().read().unwrap_or_else(|e| e.into_inner());
    let mut names: Vec<String> = lib
        .db
        .faces()
        .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// Metric-compatible stand-ins (what fontconfig aliases them to).
const ALIASES: &[(&str, &[&str])] = &[
    ("arial", &["Liberation Sans", "Arimo"]),
    ("helvetica", &["Liberation Sans", "Arimo"]),
    ("times new roman", &["Liberation Serif", "Tinos"]),
    ("times", &["Liberation Serif", "Tinos"]),
    ("courier new", &["Liberation Mono", "Cousine"]),
    ("courier", &["Liberation Mono", "Cousine"]),
];

/// CSS `font-family` fallback: the first family in the list that is
/// installed (or has an installed metric-compatible alias), else the first
/// generic keyword, else `sans-serif`. Generic names come back lower-case.
pub fn resolve_family(css: &str) -> String {
    let lib = lib().read().unwrap_or_else(|e| e.into_inner());
    for raw in css.split(',') {
        let name = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "" => continue,
            "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" => return lower,
            "system-ui" | "ui-sans-serif" | "-apple-system" => return "sans-serif".into(),
            "ui-serif" => return "serif".into(),
            "ui-monospace" => return "monospace".into(),
            _ => {}
        }
        if let Some(n) = lib.installed(name) {
            return n;
        }
        if let Some((_, alts)) = ALIASES.iter().find(|(k, _)| *k == lower) {
            if let Some(n) = alts.iter().find_map(|a| lib.installed(a)) {
                return n;
            }
        }
    }
    "sans-serif".into()
}

/// CSS `font-weight` as a number (`bold` → 700; unknown → 400).
pub fn css_weight(w: &str) -> u16 {
    match w.trim() {
        "bold" | "bolder" => 700,
        "lighter" => 300,
        "normal" | "" => 400,
        n => n
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map_or(400, |v| v.clamp(1.0, 1000.0) as u16),
    }
}

/// The face a CSS family list, weight and italic flag select.
pub fn resolve(css_family: &str, weight: u16, italic: bool) -> Option<FaceId> {
    let key = (css_family.to_owned(), weight, italic);
    if let Some(hit) = lib()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .queries
        .get(&key)
    {
        return hit.map(FaceId);
    }
    let name = resolve_family(css_family);
    let mut lib = lib().write().unwrap_or_else(|e| e.into_inner());
    let family = match name.as_str() {
        "serif" => Family::Serif,
        "monospace" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        "sans-serif" => Family::SansSerif,
        n => Family::Name(n),
    };
    let id = lib.db.query(&Query {
        families: &[family],
        weight: Weight(weight),
        style: if italic { Style::Italic } else { Style::Normal },
        ..Query::default()
    });
    lib.queries.insert(key, id);
    id.map(FaceId)
}

/// A face's bytes and axes.
pub fn face(id: FaceId) -> Option<Arc<FaceData>> {
    if let Some(d) = lib()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .data
        .get(&id.0)
    {
        return Some(d.clone());
    }
    lib()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .face_data(id.0)
}

/// Widely installed faces with good coverage, best first (prefix match).
const PREFERRED: &[&str] = &[
    "Noto Sans",
    "Noto Naskh Arabic",
    "Geeza Pro",
    "Segoe UI",
    "Tahoma",
    "Arial Unicode MS",
    "Arial",
    "Helvetica Neue",
    "DejaVu Sans",
    "PingFang",
    "Hiragino Sans",
    "Microsoft YaHei",
    "Apple Color Emoji",
    "Segoe UI Emoji",
    "Noto Color Emoji",
    "Droid Sans Fallback",
];

/// A face that has `c`, for characters the `primary` face lacks: one like
/// it (monospaced or not, serif or not), upright and regular, preferring
/// well-known fallback families. Cached per character and primary.
pub fn fallback(c: char, primary: FaceId) -> Option<FaceId> {
    let key = (c, primary.0);
    if let Some(hit) = lib()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .fallback
        .get(&key)
    {
        return hit.map(FaceId);
    }
    let mut lib = lib().write().unwrap_or_else(|e| e.into_inner());
    let serif = |name: &str| {
        let n = name.to_ascii_lowercase();
        n.contains("serif") && !n.contains("sans")
    };
    let (mono, ser) = lib.db.face(primary.0).map_or((false, false), |f| {
        (
            f.monospaced,
            f.families.first().is_some_and(|(n, _)| serif(n)),
        )
    });
    let mut ranked: Vec<((bool, bool, bool, usize), ID)> = lib
        .db
        .faces()
        .map(|f| {
            let name = f.families.first().map_or("", |(n, _)| n.as_str());
            let pref = PREFERRED
                .iter()
                .position(|p| name.starts_with(p))
                .unwrap_or(PREFERRED.len());
            let odd = f.style != Style::Normal || f.weight != Weight::NORMAL;
            ((f.monospaced != mono, serif(name) != ser, odd, pref), f.id)
        })
        .collect();
    // Stable: database order among equals.
    ranked.sort_by_key(|(k, _)| *k);
    let has = |db: &Database, id: ID| {
        db.with_face_data(id, |data, index| {
            use skrifa::MetadataProvider;
            skrifa::FontRef::from_index(data, index)
                .ok()
                .and_then(|f| f.charmap().map(c))
                .is_some_and(|g| g.to_u32() != 0)
        })
        .unwrap_or(false)
    };
    let found = ranked
        .into_iter()
        .map(|(_, id)| id)
        .find(|id| has(&lib.db, *id));
    lib.fallback.insert(key, found);
    found.map(FaceId)
}

/// Load a font file (`.ttf`, `.otf`, `.ttc`, `.woff`, `.woff2`) into the
/// library. Loading the same unchanged file again is free.
pub fn load_file(path: &Path) -> Result<LoadedFont, String> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some(e) = lib()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .files
        .get(path)
    {
        if e.modified == modified {
            return e.result.clone();
        }
    }
    let result = std::fs::read(path)
        .map_err(|e| format!("{}: {e}", path.display()))
        .and_then(|bytes| load_bytes(bytes).map_err(|e| format!("{}: {e}", path.display())));
    lib()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .files
        .insert(
            path.to_owned(),
            FileEntry {
                modified,
                result: result.clone(),
            },
        );
    result
}

/// Add font bytes (any supported format) to the library.
pub fn load_bytes(bytes: Vec<u8>) -> Result<LoadedFont, String> {
    let bytes = decode_webfont(bytes)?;
    let mut lib = lib().write().unwrap_or_else(|e| e.into_inner());
    let ids = lib.db.load_font_source(Source::Binary(Arc::new(bytes)));
    let family = ids
        .first()
        .and_then(|id| lib.db.face(*id))
        .and_then(|f| f.families.first())
        .map(|(n, _)| n.clone())
        .ok_or_else(|| "not a font file".to_owned())?;
    // A new family can change how earlier queries resolve.
    lib.queries.clear();
    lib.fallback.clear();
    GENERATION.fetch_add(1, Ordering::AcqRel);
    Ok(LoadedFont {
        family,
        faces: ids.into_iter().map(FaceId).collect(),
    })
}

/// WOFF / WOFF2 → plain OpenType; anything else passes through.
fn decode_webfont(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    match bytes.get(..4) {
        Some(b"wOF2") => wuff::decompress_woff2(&bytes).map_err(|e| format!("WOFF2: {e:?}")),
        Some(b"wOFF") => wuff::decompress_woff1(&bytes).map_err(|e| format!("WOFF: {e:?}")),
        _ => Ok(bytes),
    }
}
