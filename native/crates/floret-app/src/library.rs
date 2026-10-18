//! Library panel (`LibraryPanel.tsx`): the built-in samples by category,
//! the user's own saved patches and any imported libraries, as a
//! collapsible list, a tile grid or Finder-style columns, with one search
//! across all of them. "Save to library" stores the current patch (name +
//! description) in "My patches" (a `.flora` file in the app data dir, next
//! to a small JSON index) or in another library. Libraries download and
//! import as the web app's `floret.library` JSON (see `library_store`).
//! A subpatch saved from the Subpatches panel is an entry of kind
//! "subpatch": it carries a small mark, and clicking it inserts it into
//! the open patch instead of opening it.

use crate::library_store::{self, EntryKind, LibEntry, PatchLibrary};
use crate::theme;
use eframe::egui::{self, vec2, Color32, RichText};
use floret_engine::patch::PatchFile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A saved patch in the user library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub file: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// A whole patch, or a saved subpatch (missing: a patch).
    #[serde(default, skip_serializing_if = "EntryKind::is_patch")]
    pub kind: EntryKind,
}

/// What the user picked.
pub enum Pick {
    Sample(usize),
    /// Open this patch in place of the current one.
    Patch(Box<PatchFile>),
    /// Insert this saved subpatch into the current patch.
    Insert(Box<PatchFile>),
}

/// How a library entry was used: a click (a subpatch inserts, a patch
/// opens), or one of a subpatch's right-click items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Use {
    Click,
    Open,
    Insert,
}

impl Use {
    fn pick(self, patch: PatchFile, kind: EntryKind) -> Pick {
        let insert = match self {
            Use::Click => kind == EntryKind::Subpatch,
            Use::Open => false,
            Use::Insert => true,
        };
        if insert {
            Pick::Insert(Box::new(patch))
        } else {
            Pick::Patch(Box::new(patch))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    List,
    Grid,
    Column,
}

/// A file-name-safe slug (`my-patch`), never empty.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "patch".into()
    } else {
        out
    }
}

/// `<slug>.flora`, numbered if taken.
pub fn unique_file(entries: &[Entry], name: &str) -> String {
    let base = slug(name);
    let taken = |f: &str| entries.iter().any(|e| e.file == f);
    let mut file = format!("{base}.flora");
    let mut n = 2;
    while taken(&file) {
        file = format!("{base}-{n}.flora");
        n += 1;
    }
    file
}

/// Case-insensitive match on any of `fields`.
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&q))
}

/// A stable hue per name (`hueFor`), for tile accents.
pub fn hue_for(s: &str) -> f32 {
    let mut h: u32 = 0;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(c));
    }
    (h % 360) as f32
}

fn index_path(dir: &Path) -> PathBuf {
    dir.join("library.json")
}

pub fn load_index(dir: &Path) -> Vec<Entry> {
    std::fs::read_to_string(index_path(dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_index(dir: &Path, entries: &[Entry]) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(entries).map_err(std::io::Error::other)?;
    crate::files::write_atomic(&index_path(dir), text)
}

/// Save `patch` (a whole patch, or a subpatch snippet) into the library
/// at `dir` and return its entry.
pub fn save_patch(
    dir: &Path,
    entries: &mut Vec<Entry>,
    patch: &PatchFile,
    name: &str,
    description: &str,
    kind: EntryKind,
) -> std::io::Result<Entry> {
    std::fs::create_dir_all(dir)?;
    let mut patch = patch.clone();
    let mut meta = patch.meta.take().unwrap_or_default();
    meta.name = Some(name.trim().to_owned());
    patch.meta = Some(meta);
    let entry = Entry {
        file: unique_file(entries, name),
        name: name.trim().to_owned(),
        description: description.trim().to_owned(),
        kind,
    };
    crate::files::write_atomic(&dir.join(&entry.file), patch.to_json())?;
    entries.push(entry.clone());
    save_index(dir, entries)?;
    Ok(entry)
}

pub fn remove_entry(dir: &Path, entries: &mut Vec<Entry>, file: &str) -> std::io::Result<()> {
    entries.retain(|e| e.file != file);
    let _ = std::fs::remove_file(dir.join(file));
    save_index(dir, entries)
}

/// Which library a row or tile belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LibRef {
    /// A built-in sample category (in the "Samples" folder).
    Category(String),
    /// The user's "My patches".
    Mine,
    /// An imported or user-made library, by slug.
    User(String),
}

/// Where "Save to library" puts the patch.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Mine,
    User(String),
    New,
}

struct SaveForm {
    name: String,
    description: String,
    target: Target,
    new_library: String,
    /// A subpatch to save instead of the open patch.
    snippet: Option<PatchFile>,
}

/// The built-in samples' folder.
const SAMPLES_FOLDER: &str = "Samples";
const MINE: &str = "My patches";

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeaderAct {
    Download,
    Remove,
}

/// The column view's rows.
#[derive(Default)]
struct ColumnRows {
    /// (folder, patch count).
    folders: Vec<(String, usize)>,
    /// Libraries outside any folder: (library, name, patch count).
    root: Vec<(LibRef, String, usize)>,
    /// Libraries in folders: (folder, library, name, patch count).
    in_folder: Vec<(String, LibRef, String, usize)>,
}

/// The library's own state.
pub struct Library {
    mode: Mode,
    query: String,
    save: Option<SaveForm>,
    dir: Option<PathBuf>,
    entries: Vec<Entry>,
    /// Imported and user-made libraries.
    libs: Vec<library_store::Stored>,
    status: Option<String>,
    /// Column view: the selected folder and library.
    folder: Option<String>,
    library: Option<LibRef>,
    /// Sample thumbnails (rendered in the background, cached).
    thumbs: crate::thumbs::Thumbnails,
    /// Tiles view: the tile width ([`TILE_MIN`]..=[`TILE_MAX`]), kept in
    /// the preferences (`libraryTileWidth`).
    pub tile_width: f32,
}

impl Default for Library {
    fn default() -> Self {
        let dir = crate::editor::favorites::config_dir().map(|d| d.join("library"));
        let entries = dir.as_deref().map(load_index).unwrap_or_default();
        let libs = dir
            .as_deref()
            .map(|d| library_store::load_all(&d.join("libraries")))
            .unwrap_or_default();
        Library {
            mode: Mode::Grid,
            thumbs: crate::thumbs::Thumbnails::default(),
            tile_width: TILE_DEFAULT,
            query: String::new(),
            save: None,
            dir,
            entries,
            libs,
            status: None,
            folder: None,
            library: None,
        }
    }
}

/// A built-in category as a `floret.library` (for download).
fn category_library(samples: &[crate::Sample], cat: &str) -> PatchLibrary {
    let mut lib = PatchLibrary::new(cat, Some(SAMPLES_FOLDER));
    for s in samples.iter().filter(|s| s.category == cat) {
        lib.patches.push(LibEntry {
            id: s.id.to_owned(),
            name: s.name.to_owned(),
            description: Some(s.description.to_owned()).filter(|d| !d.is_empty()),
            kind: EntryKind::Patch,
            patch: serde_json::from_str(s.flora).unwrap_or_default(),
        });
    }
    lib
}

fn categories(samples: &[crate::Sample]) -> Vec<&'static str> {
    let mut cats: Vec<&'static str> = Vec::new();
    for s in samples {
        if !cats.contains(&s.category) {
            cats.push(s.category);
        }
    }
    cats
}

/// A compact selectable row for the column view and search results. A
/// saved subpatch gets its mark in front of the name.
fn pick_row(ui: &mut egui::Ui, on: bool, text: &str, tip: &str, kind: EntryKind) -> egui::Response {
    let sub = kind == EntryKind::Subpatch;
    let text = if sub {
        format!("{MARK_ROOM}{text}")
    } else {
        text.to_owned()
    };
    let b = egui::Button::selectable(on, RichText::new(text).small()).truncate();
    let r = ui.add(b);
    if sub {
        row_mark(ui, &r);
    }
    let tip = if sub {
        subpatch_tip(tip)
    } else {
        tip.to_owned()
    };
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// Spaces left in front of a row's text for the subpatch mark.
const MARK_ROOM: &str = "     ";

/// A subpatch entry's hover text: its description and what a click does.
fn subpatch_tip(desc: &str) -> String {
    let how = "Subpatch: click to insert it into the patch, right-click for more";
    if desc.is_empty() {
        how.to_owned()
    } else {
        format!("{desc}\n{how}")
    }
}

/// The subpatch mark, a small frame (title bar over a body) in the
/// subpatch accent, centred on `c`.
fn paint_mark(p: &egui::Painter, c: egui::Pos2) {
    let r = egui::Rect::from_center_size(c, vec2(11.0, 9.0));
    let col = *theme::REGION_SUBPATCH;
    p.rect_stroke(
        r,
        2.0,
        egui::Stroke::new(1.2, col),
        egui::StrokeKind::Inside,
    );
    let bar = egui::Rect::from_min_size(r.min, vec2(r.width(), 3.0));
    p.rect_filled(
        bar,
        egui::CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        col,
    );
}

/// The mark at the start of a row (in the room `MARK_ROOM` left).
fn row_mark(ui: &egui::Ui, r: &egui::Response) {
    let x = r.rect.left() + ui.spacing().button_padding.x + 6.0;
    paint_mark(ui.painter(), egui::pos2(x, r.rect.center().y));
}

/// How an entry's row or tile was used this frame: clicked, or (for a
/// saved subpatch) an item of its right-click menu, which inserts it
/// (what a click does) or opens it as a patch of its own.
fn entry_use(r: &egui::Response, kind: EntryKind) -> Option<Use> {
    let mut how = None;
    if kind == EntryKind::Subpatch {
        r.context_menu(|ui| {
            if ui.button("Insert into patch").clicked() {
                how = Some(Use::Insert);
                ui.close();
            }
            if ui.button("Open on its own").clicked() {
                how = Some(Use::Open);
                ui.close();
            }
        });
    }
    how.or(r.clicked().then_some(Use::Click))
}

fn hint(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).color(theme::FG_2).small());
}

impl Library {
    fn libs_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("libraries"))
    }

    /// "My patches" as a `floret.library` (for download).
    fn mine_library(&self) -> PatchLibrary {
        let mut lib = PatchLibrary::new(MINE, None);
        for e in &self.entries {
            let patch = self
                .dir
                .as_ref()
                .and_then(|d| std::fs::read_to_string(d.join(&e.file)).ok())
                .and_then(|t| serde_json::from_str(&t).ok());
            if let Some(patch) = patch {
                lib.patches.push(LibEntry {
                    id: library_store::slugify(&e.name),
                    name: e.name.clone(),
                    description: Some(e.description.clone()).filter(|d| !d.is_empty()),
                    kind: e.kind,
                    patch,
                });
            }
        }
        lib
    }

    fn library_of(&self, r: &LibRef, samples: &[crate::Sample]) -> Option<PatchLibrary> {
        match r {
            LibRef::Category(c) => Some(category_library(samples, c)),
            LibRef::Mine => Some(self.mine_library()),
            LibRef::User(slug) => self
                .libs
                .iter()
                .find(|l| &l.slug == slug)
                .map(|l| l.library.clone()),
        }
    }

    /// Save a library as `<slug>.library.json` where the user picks
    /// (`downloadLibrary`).
    fn download(&mut self, r: &LibRef, samples: &[crate::Sample]) {
        let Some(lib) = self.library_of(r, samples) else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Download library")
            .add_filter("Floret library", &["json"])
            .set_file_name(library_store::file_name(&lib.name))
            .save_file()
        else {
            return;
        };
        self.status = Some(match crate::files::write_atomic(&path, lib.to_json()) {
            Ok(()) => format!("Saved “{}” to {}.", lib.name, path.display()),
            Err(e) => format!("Couldn't save the library: {e}"),
        });
    }

    /// Pick a `floret.library` JSON file and add it (`importLibrary`).
    fn import(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import library")
            .add_filter("Floret library", &["json"])
            .pick_file()
        else {
            return;
        };
        self.status = Some(self.import_file(&path));
    }

    fn import_file(&mut self, path: &Path) -> String {
        let Some(dir) = self.libs_dir() else {
            return "No app data directory to import into.".into();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return "Could not read file.".into();
        };
        match library_store::parse_library(&text) {
            Err(e) => e,
            Ok(lib) => {
                let name = lib.name.clone();
                match library_store::store(&dir, &mut self.libs, lib) {
                    Ok(_) => format!("Imported “{name}”."),
                    Err(e) => format!("Couldn't import: {e}"),
                }
            }
        }
    }

    fn remove_library(&mut self, slug: &str) {
        if let Some(dir) = self.libs_dir() {
            if let Err(e) = library_store::remove(&dir, &mut self.libs, slug) {
                self.status = Some(format!("Couldn't remove the library: {e}"));
            }
        }
        if self.library == Some(LibRef::User(slug.to_owned())) {
            self.library = None;
        }
    }

    fn remove_user_entry(&mut self, slug: &str, id: &str) {
        let (Some(dir), Some(i)) = (
            self.libs_dir(),
            self.libs.iter().position(|l| l.slug == slug),
        ) else {
            return;
        };
        let mut lib = self.libs[i].library.clone();
        lib.patches.retain(|e| e.id != id);
        if let Err(e) = library_store::store(&dir, &mut self.libs, lib) {
            self.status = Some(format!("Couldn't update the library: {e}"));
        }
    }

    fn open_user_entry(&mut self, slug: &str, id: &str, how: Use) -> Option<Pick> {
        let entry = self
            .libs
            .iter()
            .find(|l| l.slug == slug)?
            .library
            .patches
            .iter()
            .find(|e| e.id == id)?;
        match entry.load() {
            Ok(p) => Some(how.pick(p, entry.kind)),
            Err(e) => {
                self.status = Some(format!("Couldn't open {}: {e}", entry.name));
                None
            }
        }
    }

    fn open_mine(&mut self, file: &str, how: Use) -> Option<Pick> {
        let dir = self.dir.clone()?;
        let kind = self
            .entries
            .iter()
            .find(|e| e.file == file)
            .map(|e| e.kind)
            .unwrap_or_default();
        match std::fs::read_to_string(dir.join(file))
            .map_err(|e| e.to_string())
            .and_then(|t| PatchFile::load(&t).map_err(|e| e.to_string()))
        {
            Ok(p) => Some(how.pick(p, kind)),
            Err(e) => {
                self.status = Some(format!("Couldn't open {file}: {e}"));
                None
            }
        }
    }

    /// Open the save form for `snippet` (a subpatch from the Subpatches
    /// panel) instead of the open patch, named `name`.
    pub fn open_save_form(&mut self, snippet: PatchFile, name: &str) {
        self.save = Some(SaveForm {
            name: name.to_owned(),
            description: String::new(),
            target: Target::Mine,
            new_library: String::new(),
            snippet: Some(snippet),
        });
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        samples: &[crate::Sample],
        current: Option<usize>,
        patch: &PatchFile,
        rs: Option<&eframe::egui_wgpu::RenderState>,
    ) -> Option<Pick> {
        let mut pick = None;
        if self.mode == Mode::Grid {
            self.thumbs.update(ui.ctx(), rs);
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search all libraries…")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            for (m, label, tip) in [
                (Mode::List, "List", "List view"),
                (Mode::Grid, "Tiles", "Icon view"),
                (Mode::Column, "Columns", "Column view"),
            ] {
                if ui
                    .add(egui::Button::new(RichText::new(label).small()).selected(self.mode == m))
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.mode = m;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Save to library…").small())
                    .on_hover_text("Save the current patch to a library")
                    .clicked()
                {
                    let name = patch
                        .meta
                        .as_ref()
                        .and_then(|m| m.name.clone())
                        .unwrap_or_default();
                    self.save = Some(SaveForm {
                        name,
                        description: String::new(),
                        target: Target::Mine,
                        new_library: String::new(),
                        snippet: None,
                    });
                }
                if ui
                    .button(RichText::new("Import…").small())
                    .on_hover_text("Import a floret.library JSON file")
                    .clicked()
                {
                    self.import();
                }
            });
        });
        if self.mode == Mode::Grid {
            ui.horizontal(|ui| self.size_control(ui));
        }
        self.save_form(ui, patch);
        if let Some(s) = &self.status {
            ui.label(RichText::new(s).color(theme::FG_2).small());
        }
        if self.mode == Mode::Column {
            return if self.query.trim().is_empty() {
                self.columns(ui, samples, current)
            } else {
                self.search_results(ui, samples, current)
            };
        }
        let scrolled = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(p) = self.user_section(ui, samples) {
                    pick = Some(p);
                }
                if let Some(p) = self.user_libraries(ui, samples) {
                    pick = Some(p);
                }
                for cat in categories(samples) {
                    let items: Vec<(usize, &crate::Sample)> = samples
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| {
                            s.category == cat
                                && matches(&self.query, &[s.name, s.category, s.description, s.id])
                        })
                        .collect();
                    if items.is_empty() {
                        continue;
                    }
                    let (chosen, act) = self.folder(ui, cat, items.len(), false, |this, ui| {
                        this.items(ui, cat, &items, current)
                    });
                    if let Some(i) = chosen.flatten() {
                        pick = Some(Pick::Sample(i));
                    }
                    if act == Some(HeaderAct::Download) {
                        self.download(&LibRef::Category(cat.to_owned()), samples);
                    }
                }
            });
        // Cmd / Ctrl + scroll (or a pinch) over the tiles resizes them.
        if self.mode == Mode::Grid && ui.rect_contains_pointer(scrolled.inner_rect) {
            let zoom = ui.input(|i| i.zoom_delta());
            if zoom != 1.0 {
                self.tile_width = clamp_tile(self.tile_width * zoom);
            }
        }
        pick
    }

    /// The Tiles view's size control: − / + and a small slider, on its own
    /// row under the view buttons (the side panel is too narrow for one).
    fn size_control(&mut self, ui: &mut egui::Ui) {
        let tip = "Tile size (or Cmd / Ctrl + scroll over the tiles)";
        ui.label(RichText::new("Size").small().color(theme::FG_2))
            .on_hover_text(tip);
        let w = &mut self.tile_width;
        if ui
            .add_enabled(*w > TILE_MIN, egui::Button::new(RichText::new("−").small()))
            .on_hover_text("Smaller tiles")
            .clicked()
        {
            *w = clamp_tile(*w - TILE_STEP);
        }
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = 96.0;
            ui.add(egui::Slider::new(w, TILE_MIN..=TILE_MAX).show_value(false))
                .on_hover_text(tip);
        });
        if ui
            .add_enabled(*w < TILE_MAX, egui::Button::new(RichText::new("+").small()))
            .on_hover_text("Larger tiles")
            .clicked()
        {
            *w = clamp_tile(*w + TILE_STEP);
        }
    }

    /// A collapsible library (searching opens everything), with download
    /// and, for user libraries, remove buttons in its header.
    fn folder<R>(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        count: usize,
        removable: bool,
        body: impl FnOnce(&mut Self, &mut egui::Ui) -> R,
    ) -> (Option<R>, Option<HeaderAct>) {
        let searching = !self.query.trim().is_empty();
        let id = ui.make_persistent_id(("floret-library", name));
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
        if searching {
            state.set_open(true);
        }
        let mut act = None;
        let header = state.show_header(ui, |ui| {
            ui.label(
                RichText::new(format!("{name}  ·  {count}"))
                    .color(theme::FG_1)
                    .small(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if removable
                    && ui
                        .small_button("✕")
                        .on_hover_text(format!("Remove library “{name}”"))
                        .clicked()
                {
                    act = Some(HeaderAct::Remove);
                }
                if ui
                    .small_button("⬇")
                    .on_hover_text("Download this library as a .json file")
                    .clicked()
                {
                    act = Some(HeaderAct::Download);
                }
            });
        });
        let (_, _, body) = header.body(|ui| body(self, ui));
        (body.map(|r| r.inner), act)
    }

    fn items(
        &self,
        ui: &mut egui::Ui,
        cat: &str,
        items: &[(usize, &crate::Sample)],
        current: Option<usize>,
    ) -> Option<usize> {
        let mut picked = None;
        match self.mode {
            Mode::List | Mode::Column => {
                for (i, s) in items {
                    let on = current == Some(*i);
                    if ui
                        .selectable_label(on, format!("   {}", s.name))
                        .on_hover_text(s.description)
                        .clicked()
                    {
                        picked = Some(*i);
                    }
                }
            }
            Mode::Grid => {
                ui.horizontal_wrapped(|ui| {
                    for (i, s) in items {
                        let thumb = self.thumbs.get(s.id).map(|t| t.id());
                        if tile(
                            ui,
                            self.tile_width,
                            s.name,
                            cat,
                            current == Some(*i),
                            thumb,
                            false,
                        )
                        .on_hover_text(s.description)
                        .clicked()
                        {
                            picked = Some(*i);
                        }
                    }
                });
            }
        }
        ui.add_space(4.0);
        picked
    }

    /// Patches of a user library as rows (with remove buttons) or tiles.
    /// Returns the index used (and how) and the index removed.
    fn entries_ui(
        &self,
        ui: &mut egui::Ui,
        list: &[(String, String, EntryKind)],
        group: &str,
    ) -> (Option<(usize, Use)>, Option<usize>) {
        let (mut open, mut remove) = (None, None);
        // A subpatch's hover text says what a click does; its right-click
        // menu inserts or opens it.
        let used = |r: &egui::Response, kind: EntryKind, desc: &str| {
            let tip = if kind == EntryKind::Subpatch {
                subpatch_tip(desc)
            } else {
                desc.to_owned()
            };
            entry_use(&r.clone().on_hover_text(tip), kind)
        };
        if self.mode == Mode::Grid {
            ui.horizontal_wrapped(|ui| {
                for (i, (name, desc, kind)) in list.iter().enumerate() {
                    let r = tile(
                        ui,
                        self.tile_width,
                        name,
                        group,
                        false,
                        None,
                        *kind == EntryKind::Subpatch,
                    );
                    if let Some(how) = used(&r, *kind, desc) {
                        open = Some((i, how));
                    }
                }
            });
        } else {
            for (i, (name, desc, kind)) in list.iter().enumerate() {
                ui.horizontal(|ui| {
                    let sub = *kind == EntryKind::Subpatch;
                    let text = if sub {
                        format!("   {MARK_ROOM}{name}")
                    } else {
                        format!("   {name}")
                    };
                    let r = ui.selectable_label(false, text);
                    if sub {
                        // After the row's three-space indent.
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        let indent = ui.fonts_mut(|f| {
                            f.layout_no_wrap("   ".into(), font, Color32::WHITE)
                                .size()
                                .x
                        });
                        let x = r.rect.left() + ui.spacing().button_padding.x + indent + 6.0;
                        paint_mark(ui.painter(), egui::pos2(x, r.rect.center().y));
                    }
                    if let Some(how) = used(&r, *kind, desc) {
                        open = Some((i, how));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text("Remove from library")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                });
            }
        }
        ui.add_space(4.0);
        (open, remove)
    }

    fn user_section(&mut self, ui: &mut egui::Ui, samples: &[crate::Sample]) -> Option<Pick> {
        let shown: Vec<Entry> = self
            .entries
            .iter()
            .filter(|e| matches(&self.query, &[&e.name, &e.description, MINE]))
            .cloned()
            .collect();
        if shown.is_empty() {
            return None;
        }
        let list: Vec<(String, String, EntryKind)> = shown
            .iter()
            .map(|e| (e.name.clone(), e.description.clone(), e.kind))
            .collect();
        let (body, act) = self.folder(ui, MINE, shown.len(), false, |this, ui| {
            this.entries_ui(ui, &list, MINE)
        });
        let (open, remove) = body.unwrap_or_default();
        if act == Some(HeaderAct::Download) {
            self.download(&LibRef::Mine, samples);
        }
        if let (Some(i), Some(dir)) = (remove, self.dir.clone()) {
            if let Err(e) = remove_entry(&dir, &mut self.entries, &shown[i].file) {
                self.status = Some(format!("Couldn't update the library: {e}"));
            }
        }
        let (i, how) = open?;
        self.open_mine(&shown[i].file, how)
    }

    /// Imported and user-made libraries, one section each.
    fn user_libraries(&mut self, ui: &mut egui::Ui, samples: &[crate::Sample]) -> Option<Pick> {
        let mut pick = None;
        let searching = !self.query.trim().is_empty();
        for lib in self.libs.clone() {
            let shown: Vec<&LibEntry> = lib
                .library
                .patches
                .iter()
                .filter(|e| {
                    let desc = e.description.as_deref().unwrap_or("");
                    matches(&self.query, &[&e.name, desc, &lib.library.name])
                })
                .collect();
            if shown.is_empty() && searching {
                continue;
            }
            let list: Vec<(String, String, EntryKind)> = shown
                .iter()
                .map(|e| {
                    let desc = e.description.clone().unwrap_or_default();
                    (e.name.clone(), desc, e.kind)
                })
                .collect();
            let name = &lib.library.name;
            let (body, act) = self.folder(ui, name, shown.len(), true, |this, ui| {
                if list.is_empty() {
                    hint(ui, "Empty — use “Save to library…” to add a patch.");
                }
                this.entries_ui(ui, &list, name)
            });
            let (open, remove) = body.unwrap_or_default();
            match act {
                Some(HeaderAct::Download) => {
                    self.download(&LibRef::User(lib.slug.clone()), samples)
                }
                Some(HeaderAct::Remove) => self.remove_library(&lib.slug),
                None => {}
            }
            if let Some(i) = remove {
                self.remove_user_entry(&lib.slug, &shown[i].id);
            }
            if let Some((i, how)) = open {
                pick = self.open_user_entry(&lib.slug, &shown[i].id, how).or(pick);
            }
        }
        pick
    }

    /// The column view's rows.
    fn column_rows(&self, samples: &[crate::Sample]) -> ColumnRows {
        let mut rows = ColumnRows::default();
        let add_folder = |rows: &mut ColumnRows, folder: &str, n: usize| match rows
            .folders
            .iter_mut()
            .find(|(f, _)| f == folder)
        {
            Some(f) => f.1 += n,
            None => rows.folders.push((folder.to_owned(), n)),
        };
        for cat in categories(samples) {
            let n = samples.iter().filter(|s| s.category == cat).count();
            add_folder(&mut rows, SAMPLES_FOLDER, n);
            let r = LibRef::Category(cat.to_owned());
            rows.in_folder
                .push((SAMPLES_FOLDER.to_owned(), r, cat.to_owned(), n));
        }
        rows.root
            .push((LibRef::Mine, MINE.to_owned(), self.entries.len()));
        for l in &self.libs {
            let r = LibRef::User(l.slug.clone());
            let n = l.library.patches.len();
            match &l.library.folder {
                Some(f) => {
                    add_folder(&mut rows, f, n);
                    rows.in_folder
                        .push((f.clone(), r, l.library.name.clone(), n));
                }
                None => rows.root.push((r, l.library.name.clone(), n)),
            }
        }
        rows
    }

    /// Column view (Finder's three panes): folders and top-level
    /// libraries → libraries in the folder → patches in the library.
    /// Right-click a library to download it.
    fn columns(
        &mut self,
        ui: &mut egui::Ui,
        samples: &[crate::Sample],
        current: Option<usize>,
    ) -> Option<Pick> {
        let rows = self.column_rows(samples);
        let mut pick: Option<Pick> = None;
        let mut download = None;
        let mut open: Option<(LibOpen, Use)> = None;
        let height = ui.available_height();
        let lib_row = |ui: &mut egui::Ui, on: bool, icon: &str, name: &str, n: usize| {
            pick_row(
                ui,
                on,
                &format!("{icon} {name}  ·  {n}  ›"),
                "",
                EntryKind::Patch,
            )
        };
        ui.columns(3, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("lib-col-1")
                .max_height(height)
                .auto_shrink([false, false])
                .show(&mut cols[0], |ui| {
                    for (folder, n) in &rows.folders {
                        let on = self.folder.as_deref() == Some(folder.as_str());
                        if lib_row(ui, on, "🗀", folder, *n).clicked() {
                            self.folder = Some(folder.clone());
                            self.library = None;
                        }
                    }
                    for (r, name, n) in &rows.root {
                        let on = self.folder.is_none() && self.library.as_ref() == Some(r);
                        let resp = lib_row(ui, on, "📖", name, *n);
                        if resp.clicked() {
                            self.folder = None;
                            self.library = Some(r.clone());
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Download…").clicked() {
                                download = Some(r.clone());
                                ui.close();
                            }
                        });
                    }
                });
            egui::ScrollArea::vertical()
                .id_salt("lib-col-2")
                .max_height(height)
                .auto_shrink([false, false])
                .show(&mut cols[1], |ui| {
                    let Some(f) = self.folder.clone() else {
                        theme::empty_state(ui, "No folder picked", "Pick one on the left");
                        return;
                    };
                    for (_, r, name, n) in rows.in_folder.iter().filter(|x| x.0 == f) {
                        let on = self.library.as_ref() == Some(r);
                        let resp = lib_row(ui, on, "📖", name, *n);
                        if resp.clicked() {
                            self.library = Some(r.clone());
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Download…").clicked() {
                                download = Some(r.clone());
                                ui.close();
                            }
                        });
                    }
                });
            egui::ScrollArea::vertical()
                .id_salt("lib-col-3")
                .max_height(height)
                .auto_shrink([false, false])
                .show(&mut cols[2], |ui| match &self.library {
                    None => theme::empty_state(ui, "No library picked", "Pick one on the left"),
                    Some(LibRef::Category(c)) => {
                        for (i, s) in samples.iter().enumerate().filter(|(_, s)| s.category == c) {
                            let on = current == Some(i);
                            if pick_row(ui, on, s.name, s.description, EntryKind::Patch).clicked() {
                                pick = Some(Pick::Sample(i));
                            }
                        }
                    }
                    Some(LibRef::Mine) => {
                        if self.entries.is_empty() {
                            theme::empty_state(
                                ui,
                                "Empty library",
                                "Use “Save to library…” to add a patch",
                            );
                        }
                        for e in &self.entries {
                            let r = pick_row(ui, false, &e.name, &e.description, e.kind);
                            if let Some(how) = entry_use(&r, e.kind) {
                                open = Some((LibOpen::Mine(e.file.clone()), how));
                            }
                        }
                    }
                    Some(LibRef::User(slug)) => {
                        let lib = self.libs.iter().find(|l| &l.slug == slug);
                        let entries = lib.map(|l| l.library.patches.as_slice()).unwrap_or(&[]);
                        if entries.is_empty() {
                            theme::empty_state(
                                ui,
                                "Empty library",
                                "Use “Save to library…” to add a patch",
                            );
                        }
                        for e in entries {
                            let desc = e.description.as_deref().unwrap_or("");
                            let r = pick_row(ui, false, &e.name, desc, e.kind);
                            if let Some(how) = entry_use(&r, e.kind) {
                                open = Some((LibOpen::User(slug.clone(), e.id.clone()), how));
                            }
                        }
                    }
                });
        });
        if let Some(r) = download {
            self.download(&r, samples);
        }
        match open {
            Some((what, how)) => self.use_entry(what, how),
            None => pick,
        }
    }

    /// Open a sample, or open / insert a saved entry.
    fn use_entry(&mut self, what: LibOpen, how: Use) -> Option<Pick> {
        match what {
            LibOpen::Sample(i) => Some(Pick::Sample(i)),
            LibOpen::Mine(file) => self.open_mine(&file, how),
            LibOpen::User(slug, id) => self.open_user_entry(&slug, &id, how),
        }
    }

    /// `SearchResults`: every matching patch across libraries, in one list.
    fn search_results(
        &mut self,
        ui: &mut egui::Ui,
        samples: &[crate::Sample],
        current: Option<usize>,
    ) -> Option<Pick> {
        let q = self.query.clone();
        // (what opens, name, library, kind)
        let mut hits: Vec<(LibOpen, String, String, EntryKind)> = Vec::new();
        for e in &self.entries {
            if matches(&q, &[&e.name, &e.description, MINE]) {
                let open = LibOpen::Mine(e.file.clone());
                hits.push((open, e.name.clone(), MINE.into(), e.kind));
            }
        }
        for l in &self.libs {
            for e in &l.library.patches {
                let desc = e.description.as_deref().unwrap_or("");
                if matches(&q, &[&e.name, desc, &l.library.name]) {
                    let open = LibOpen::User(l.slug.clone(), e.id.clone());
                    hits.push((open, e.name.clone(), l.library.name.clone(), e.kind));
                }
            }
        }
        for (i, s) in samples.iter().enumerate() {
            if matches(&q, &[s.name, s.description, s.category, s.id]) {
                let open = LibOpen::Sample(i);
                hits.push((open, s.name.into(), s.category.into(), EntryKind::Patch));
            }
        }
        if hits.is_empty() {
            theme::empty_state(ui, "No patches match", "Try another name or keyword");
            return None;
        }
        let n = hits.len();
        hint(
            ui,
            &format!("{n} {}", if n == 1 { "result" } else { "results" }),
        );
        let mut chosen = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, (open, name, lib, kind)) in hits.iter().enumerate() {
                    let on = matches!(open, LibOpen::Sample(s) if current == Some(*s));
                    let r = pick_row(ui, on, &format!("{name}   ·  {lib}"), "", *kind);
                    if let Some(how) = entry_use(&r, *kind) {
                        chosen = Some((i, how));
                    }
                }
            });
        let (i, how) = chosen?;
        self.use_entry(hits.swap_remove(i).0, how)
    }

    fn save_form(&mut self, ui: &mut egui::Ui, patch: &PatchFile) {
        let libs: Vec<(String, String)> = self
            .libs
            .iter()
            .map(|l| (l.slug.clone(), l.library.name.clone()))
            .collect();
        let Some(form) = self.save.as_mut() else {
            return;
        };
        let mut done = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let title = if form.snippet.is_some() {
                "Save subpatch to library"
            } else {
                "Save to library"
            };
            ui.label(RichText::new(title).color(theme::FG_1).small());
            let label = |t: &Target| match t {
                Target::Mine => MINE.to_owned(),
                Target::User(slug) => libs
                    .iter()
                    .find(|(s, _)| s == slug)
                    .map_or_else(|| slug.clone(), |(_, n)| n.clone()),
                Target::New => "＋ New library…".to_owned(),
            };
            egui::ComboBox::from_id_salt("floret-save-target")
                .selected_text(label(&form.target))
                .show_ui(ui, |ui| {
                    let mut targets = vec![Target::Mine];
                    targets.extend(libs.iter().map(|(s, _)| Target::User(s.clone())));
                    targets.push(Target::New);
                    for t in targets {
                        let text = label(&t);
                        ui.selectable_value(&mut form.target, t, text);
                    }
                });
            if form.target == Target::New {
                ui.add(
                    egui::TextEdit::singleline(&mut form.new_library).hint_text("New library name"),
                );
            }
            ui.add(egui::TextEdit::singleline(&mut form.name).hint_text("Name"));
            ui.add(
                egui::TextEdit::multiline(&mut form.description)
                    .hint_text("Description (optional)")
                    .desired_rows(2),
            );
            let ok = !form.name.trim().is_empty()
                && (form.target != Target::New || !form.new_library.trim().is_empty());
            ui.horizontal(|ui| {
                if ui.add_enabled(ok, egui::Button::new("Save")).clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                let form = self.save.take().expect("open");
                self.status = Some(self.save_to(patch, &form));
            }
            Some(false) => self.save = None,
            None => {}
        }
    }

    fn save_to(&mut self, patch: &PatchFile, form: &SaveForm) -> String {
        let (Some(dir), Some(libs_dir)) = (self.dir.clone(), self.libs_dir()) else {
            return "No app data directory to save into.".into();
        };
        // A subpatch from the Subpatches panel, else the open patch.
        let (patch, kind) = match &form.snippet {
            Some(s) => (s, EntryKind::Subpatch),
            None => (patch, EntryKind::Patch),
        };
        let mut lib = match &form.target {
            Target::Mine => {
                let saved = save_patch(
                    &dir,
                    &mut self.entries,
                    patch,
                    &form.name,
                    &form.description,
                    kind,
                );
                return match saved {
                    Ok(e) => format!("Saved “{}”.", e.name),
                    Err(e) => format!("Couldn't save: {e}"),
                };
            }
            Target::User(slug) => match self.libs.iter().find(|l| &l.slug == slug) {
                Some(l) => l.library.clone(),
                None => return "That library is gone.".into(),
            },
            Target::New => PatchLibrary::new(&form.new_library, None),
        };
        lib.upsert(library_store::entry_for(
            patch,
            &form.name,
            &form.description,
            kind,
        ));
        let lib_name = lib.name.clone();
        match library_store::store(&libs_dir, &mut self.libs, lib) {
            Ok(_) => format!("Saved “{}” to {lib_name}.", form.name.trim()),
            Err(e) => format!("Couldn't save: {e}"),
        }
    }
}

/// A patch to open, by where it lives.
enum LibOpen {
    Sample(usize),
    Mine(String),
    User(String, String),
}

/// Tile widths the Tiles view's size control allows, and its default.
pub const TILE_MIN: f32 = 64.0;
pub const TILE_MAX: f32 = 240.0;
pub const TILE_DEFAULT: f32 = 96.0;
/// One click of the − / + buttons.
const TILE_STEP: f32 = 16.0;

/// A tile width within [`TILE_MIN`]..=[`TILE_MAX`] (the default when not a
/// number).
pub fn clamp_tile(w: f32) -> f32 {
    if w.is_finite() {
        w.clamp(TILE_MIN, TILE_MAX)
    } else {
        TILE_DEFAULT
    }
}

/// The title's font size for a tile `width` wide: 10.5 at the default
/// width, scaled with it but kept readable.
fn tile_font(width: f32) -> f32 {
    (10.5 * width / TILE_DEFAULT).clamp(9.0, 14.0)
}

/// A tile `width` wide: the swatch (two thirds as tall) and two lines of
/// title. 96 × 92 at the default width.
fn tile_size(width: f32) -> egui::Vec2 {
    vec2(width, width * 0.66 + 3.0 + tile_font(width) * 2.44)
}

/// A tile: the sample's thumbnail (else a muted swatch from the group
/// name), the title below; a saved subpatch (`mark`) carries its mark.
fn tile(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    group: &str,
    on: bool,
    thumb: Option<egui::TextureId>,
    mark: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(tile_size(width), egui::Sense::click());
    let p = ui.painter();
    let swatch = egui::Rect::from_min_size(rect.min, vec2(rect.width(), rect.width() * 0.66));
    match thumb {
        Some(id) => {
            // Centre-crop the square-ish thumbnail into the swatch.
            let a = swatch.height() / swatch.width();
            let uv = egui::Rect::from_min_max(
                egui::pos2(0.0, 0.5 - a / 2.0),
                egui::pos2(1.0, 0.5 + a / 2.0),
            );
            p.with_clip_rect(swatch)
                .image(id, swatch, uv, Color32::WHITE);
        }
        None => {
            let accent = egui::ecolor::Hsva::new(hue_for(group) / 360.0, 0.18, 0.5, 1.0);
            p.rect_filled(swatch, 5.0, Color32::from(accent).gamma_multiply(0.55));
        }
    }
    let stroke = if on {
        egui::Stroke::new(1.5, theme::GOLD)
    } else if resp.hovered() {
        egui::Stroke::new(1.0, theme::FG_2)
    } else {
        egui::Stroke::new(1.0, theme::BORDER)
    };
    p.rect_stroke(swatch, 5.0, stroke, egui::StrokeKind::Inside);
    if mark {
        // The subpatch mark on a small badge in the swatch's corner.
        let badge = egui::Rect::from_min_size(
            egui::pos2(swatch.right() - 21.0, swatch.top() + 5.0),
            vec2(16.0, 14.0),
        );
        p.rect_filled(badge, 3.0, theme::BG_1.gamma_multiply(0.9));
        paint_mark(p, badge.center());
    }
    let g = p.layout(
        title.to_owned(),
        egui::FontId::proportional(tile_font(width)),
        *theme::FG_1,
        rect.width(),
    );
    p.with_clip_rect(rect).galley(
        egui::pos2(rect.left(), swatch.bottom() + 3.0),
        g,
        *theme::FG_1,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_unique_names() {
        assert_eq!(slug("  My Patch!  v2 "), "my-patch-v2");
        assert_eq!(slug("???"), "patch");
        let e = |f: &str| Entry {
            file: f.into(),
            name: String::new(),
            description: String::new(),
            kind: EntryKind::Patch,
        };
        let entries = vec![e("a.flora"), e("a-2.flora")];
        assert_eq!(unique_file(&entries, "A"), "a-3.flora");
        assert_eq!(unique_file(&entries, "b"), "b.flora");
    }

    #[test]
    fn tiles_scale_from_the_default_size() {
        let d = tile_size(TILE_DEFAULT);
        assert_eq!(d.x, 96.0);
        assert!((d.y - 92.0).abs() < 0.1, "{d:?}");
        let (small, big) = (tile_size(TILE_MIN), tile_size(TILE_MAX));
        assert!(small.y < d.y && d.y < big.y);
        assert_eq!(clamp_tile(10.0), TILE_MIN);
        assert_eq!(clamp_tile(1e6), TILE_MAX);
        assert_eq!(clamp_tile(f32::NAN), TILE_DEFAULT);
    }

    #[test]
    fn search_and_hues() {
        assert!(matches("", &["x"]));
        assert!(matches("SLID", &["05 · Slider"]));
        assert!(!matches("zzz", &["a", "b"]));
        assert_eq!(hue_for("Tutorials"), hue_for("Tutorials"));
        assert!(hue_for("x") < 360.0);
    }

    #[test]
    fn imports_saves_into_and_lists_libraries() {
        let dir = std::env::temp_dir().join(format!("floret-lib-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = Library {
            dir: Some(dir.clone()),
            entries: Vec::new(),
            libs: Vec::new(),
            ..Library::default()
        };
        let samples = crate::SAMPLES;
        // A downloaded built-in category re-imports as a user library in
        // the same folder.
        let cat = samples[0].category;
        let exported = category_library(samples, cat);
        assert!(exported.patches.iter().all(|e| e.load().is_ok()));
        let file = dir.join("export.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, exported.to_json()).unwrap();
        assert_eq!(lib.import_file(&file), format!("Imported “{cat}”."));
        std::fs::write(&file, "{}").unwrap();
        assert!(lib.import_file(&file).starts_with("Not a valid library"));
        // Save into a new library, then into "My patches".
        let patch = crate::editor::test_util::patch(vec![], vec![]);
        let form = |target| SaveForm {
            name: "Night".into(),
            description: String::new(),
            target,
            new_library: "Sketches".into(),
            snippet: None,
        };
        assert_eq!(
            lib.save_to(&patch, &form(Target::New)),
            "Saved “Night” to Sketches."
        );
        assert_eq!(lib.save_to(&patch, &form(Target::Mine)), "Saved “Night”.");
        let rows = lib.column_rows(samples);
        assert_eq!(rows.folders[0].0, SAMPLES_FOLDER);
        let in_samples = rows
            .in_folder
            .iter()
            .filter(|r| r.0 == SAMPLES_FOLDER)
            .count();
        assert_eq!(in_samples, categories(samples).len() + 1);
        let root: Vec<&str> = rows.root.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(root, [MINE, "Sketches"]);
        assert_eq!(lib.mine_library().patches.len(), 1);
        // Reloads from disk.
        let back = library_store::load_all(&dir.join("libraries"));
        assert_eq!(back.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_and_removes_patches() {
        let dir = std::env::temp_dir().join(format!("floret-lib-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut entries = Vec::new();
        let patch = crate::editor::test_util::patch(vec![], vec![]);
        let save = |entries: &mut Vec<Entry>, desc| {
            save_patch(&dir, entries, &patch, "Night Sky", desc, EntryKind::Patch).unwrap()
        };
        let a = save(&mut entries, "stars");
        let b = save(&mut entries, "");
        assert_eq!(
            (a.file.as_str(), b.file.as_str()),
            ("night-sky.flora", "night-sky-2.flora")
        );
        assert_eq!(load_index(&dir), entries);
        let text = std::fs::read_to_string(dir.join(&a.file)).unwrap();
        let back = PatchFile::load(&text).unwrap();
        assert_eq!(back.meta.and_then(|m| m.name).as_deref(), Some("Night Sky"));
        remove_entry(&dir, &mut entries, &a.file).unwrap();
        assert_eq!(load_index(&dir).len(), 1);
        assert!(!dir.join(&a.file).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A subpatch saved from the Subpatches panel: the form saves the
    /// snippet (not the open patch), marked `kind: "subpatch"` in
    /// `library.json` and in a `.library.json` bundle; clicking it
    /// inserts, its menu can still open it.
    #[test]
    fn a_saved_subpatch_keeps_its_kind_and_inserts() {
        use crate::editor::test_util::{node, patch};
        let dir = std::env::temp_dir().join(format!("floret-lib-sub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = Library {
            dir: Some(dir.clone()),
            entries: Vec::new(),
            libs: Vec::new(),
            ..Library::default()
        };
        let open = patch(vec![node("a", "math/add"), node("b", "math/add")], vec![]);
        let snippet = patch(vec![node("s", "meta/subpatch")], vec![]);
        lib.open_save_form(snippet.clone(), "Wobble");
        let mut form = lib.save.take().expect("form open");
        assert_eq!(form.name, "Wobble");
        assert_eq!(lib.save_to(&open, &form), "Saved “Wobble”.");
        form.target = Target::New;
        form.new_library = "Bits".into();
        assert_eq!(lib.save_to(&open, &form), "Saved “Wobble” to Bits.");

        // "My patches": the index says subpatch, the file is the snippet.
        let index = std::fs::read_to_string(dir.join("library.json")).unwrap();
        assert!(index.contains("\"kind\": \"subpatch\""));
        assert_eq!(load_index(&dir), lib.entries);
        assert_eq!(lib.entries[0].kind, EntryKind::Subpatch);
        let file = lib.entries[0].file.clone();
        match lib.open_mine(&file, Use::Click) {
            Some(Pick::Insert(p)) => assert_eq!(p.nodes.len(), 1),
            _ => panic!("a click inserts a saved subpatch"),
        }
        assert!(matches!(
            lib.open_mine(&file, Use::Open),
            Some(Pick::Patch(_))
        ));
        // A bundle keeps it too, and exports it ("My patches" as well).
        let back = library_store::load_all(&dir.join("libraries"));
        assert_eq!(back[0].library.patches[0].kind, EntryKind::Subpatch);
        assert_eq!(back[0].library.patches[0].load().unwrap().nodes.len(), 1);
        match lib.open_user_entry("bits", "wobble", Use::Click) {
            Some(Pick::Insert(p)) => assert_eq!(p.nodes[0].def_type, "meta/subpatch"),
            _ => panic!("a click inserts a saved subpatch"),
        }
        assert_eq!(lib.mine_library().patches[0].kind, EntryKind::Subpatch);

        // A whole patch still opens on a click.
        lib.save_to(
            &open,
            &SaveForm {
                name: "Whole".into(),
                description: String::new(),
                target: Target::Mine,
                new_library: String::new(),
                snippet: None,
            },
        );
        let whole = lib.entries.iter().find(|e| e.name == "Whole").unwrap();
        assert_eq!(whole.kind, EntryKind::Patch);
        let file = whole.file.clone();
        match lib.open_mine(&file, Use::Click) {
            Some(Pick::Patch(p)) => assert_eq!(p.nodes.len(), 2),
            _ => panic!("a click opens a patch"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_index_without_kinds_loads_as_patches() {
        let dir = std::env::temp_dir().join(format!("floret-lib-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("library.json"),
            r#"[{"file":"old.flora","name":"Old","description":"from 0.1"}]"#,
        )
        .unwrap();
        let entries = load_index(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, EntryKind::Patch);
        // And a patch entry is written back without a `kind`.
        save_index(&dir, &entries).unwrap();
        let text = std::fs::read_to_string(dir.join("library.json")).unwrap();
        assert!(!text.contains("kind"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Everything the Library keeps comes back on the next launch, found
    /// through the config folder like the app does: "My patches" (a whole
    /// patch and a saved subpatch, each with its kind), a library the user
    /// made, and what was removed stays removed.
    #[test]
    fn the_library_survives_a_restart() {
        use crate::editor::test_util::{node, patch};
        let cfg = crate::test_env::config_dir("library");
        let open = patch(vec![node("a", "math/add"), node("b", "math/add")], vec![]);
        let snippet = patch(vec![node("s", "meta/subpatch")], vec![]);
        let form = |name: &str, target: Target, snippet: Option<PatchFile>| SaveForm {
            name: name.into(),
            description: format!("about {name}"),
            target,
            new_library: "Bits ✿".into(),
            snippet,
        };

        // First launch: nothing saved.
        let mut lib = Library::default();
        assert!(lib.entries.is_empty() && lib.libs.is_empty());
        assert_eq!(
            lib.save_to(&open, &form("Night Sky", Target::Mine, None)),
            "Saved “Night Sky”."
        );
        assert_eq!(
            lib.save_to(&open, &form("Wobble", Target::Mine, Some(snippet.clone()))),
            "Saved “Wobble”."
        );
        assert_eq!(
            lib.save_to(&open, &form("Spin", Target::New, Some(snippet.clone()))),
            "Saved “Spin” to Bits ✿."
        );
        assert_eq!(
            lib.save_to(&open, &form("Orbit", Target::User("bits".into()), None)),
            "Saved “Orbit” to Bits ✿."
        );
        let lib_dir = cfg.file("library");
        assert!(lib_dir.join("library.json").is_file());
        assert!(lib_dir.join("night-sky.flora").is_file());
        assert!(lib_dir
            .join("libraries")
            .join("bits.library.json")
            .is_file());
        drop(lib);

        // The next launch.
        let mut lib = Library::default();
        let mine: Vec<(&str, EntryKind, &str)> = lib
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.description.as_str()))
            .collect();
        assert_eq!(
            mine,
            [
                ("Night Sky", EntryKind::Patch, "about Night Sky"),
                ("Wobble", EntryKind::Subpatch, "about Wobble"),
            ]
        );
        assert_eq!(lib.libs.len(), 1);
        let bits = &lib.libs[0].library;
        assert_eq!(bits.name, "Bits ✿");
        let names: Vec<(&str, EntryKind)> = bits
            .patches
            .iter()
            .map(|e| (e.name.as_str(), e.kind))
            .collect();
        assert_eq!(
            names,
            [("Spin", EntryKind::Subpatch), ("Orbit", EntryKind::Patch)]
        );
        // What they open as is what was saved.
        let whole = lib.entries[0].file.clone();
        match lib.open_mine(&whole, Use::Click) {
            Some(Pick::Patch(p)) => assert_eq!(p.nodes.len(), 2),
            _ => panic!("a patch opens"),
        }
        let sub = lib.entries[1].file.clone();
        match lib.open_mine(&sub, Use::Click) {
            Some(Pick::Insert(p)) => assert_eq!(p.nodes[0].def_type, "meta/subpatch"),
            _ => panic!("a subpatch inserts"),
        }
        match lib.open_user_entry("bits", "spin", Use::Click) {
            Some(Pick::Insert(_)) => {}
            _ => panic!("a library's subpatch inserts"),
        }

        // Removing from both, then another launch.
        let dir = lib.dir.clone().unwrap();
        remove_entry(&dir, &mut lib.entries, &whole).unwrap();
        library_store::remove(&dir.join("libraries"), &mut lib.libs, "bits").unwrap();
        drop(lib);
        let lib = Library::default();
        assert_eq!(lib.entries.len(), 1);
        assert_eq!(lib.entries[0].name, "Wobble");
        assert!(lib.libs.is_empty());
        assert!(!dir.join("night-sky.flora").exists());
    }

    /// A damaged library file never stops the app starting; the entries
    /// that are fine still show.
    #[test]
    fn damaged_library_files_are_skipped() {
        let cfg = crate::test_env::config_dir("library-bad");
        let dir = cfg.file("library");
        std::fs::create_dir_all(dir.join("libraries")).unwrap();
        std::fs::write(dir.join("library.json"), "[{\"file\":").unwrap();
        std::fs::write(dir.join("libraries").join("broken.library.json"), "{").unwrap();
        std::fs::write(dir.join("libraries").join("notes.txt"), "hello").unwrap();
        let ok = lib_with_patches("Fine");
        std::fs::write(dir.join("libraries").join("fine.library.json"), ok).unwrap();
        let lib = Library::default();
        assert!(lib.entries.is_empty());
        assert_eq!(lib.libs.len(), 1);
        assert_eq!(lib.libs[0].library.name, "Fine");
    }

    fn lib_with_patches(name: &str) -> String {
        format!(
            r#"{{"kind":"floret.library","version":1,"name":"{name}","patches":[
                {{"id":"p","name":"P","patch":{{"kind":"floret.patch","version":4,"nodes":[],"edges":[]}}}}]}}"#
        )
    }
}
