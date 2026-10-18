//! Autosave and crash recovery (#73), in one directory:
//! `<data dir>/recovery/`.
//!
//! - While the document has unsaved changes, it is autosaved there every
//!   N seconds (`autosave-<session>.flora`, overwritten each time, written
//!   off the UI thread).
//! - The crash screen ([`crate::crash`]) writes its copies there too.
//!
//! - A panic nothing catches (in the editor, say) writes the latest unsaved
//!   state there from the panic hook ([`emergency_save`], at most a second
//!   old), and the exit that follows keeps it.
//!
//! Each `.flora` has a `.json` sidecar: which session wrote it, the file it
//! was editing, and when. A clean exit removes the session's autosave and
//! sidecars, so on the next launch whatever is left came from a session
//! that didn't exit cleanly (killed, crashed, power lost); those newer than
//! their saved file are offered for restore.

use crate::files;
use floret_engine::PatchFile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Recovery files older than this are deleted on launch.
const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);
/// How stale the state an emergency save writes may be.
const SNAPSHOT_EVERY: Duration = Duration::from_secs(1);
const NO_REV: u64 = u64::MAX;

pub fn dir() -> PathBuf {
    if cfg!(test) {
        std::env::temp_dir().join(format!("floret-recovery-test-{}", std::process::id()))
    } else {
        files::data_dir().join("recovery")
    }
}

/// This run of the app: `<pid>-<start time>`.
pub fn session() -> &'static str {
    static S: OnceLock<String> = OnceLock::new();
    S.get_or_init(|| format!("{}-{}", std::process::id(), files::unix_now()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Autosave,
    Crash,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub session: String,
    pub kind: Kind,
    /// The file being edited (none when untitled).
    pub source: Option<PathBuf>,
    pub name: String,
    /// Unix seconds.
    pub saved_at: u64,
}

/// What the open document is, for recovery files written by code that
/// doesn't know (the crash guard). Kept up to date by the app.
static CURRENT: Mutex<Option<(Option<PathBuf>, String)>> = Mutex::new(None);

pub fn set_current(source: Option<&Path>, name: &str) {
    if let Ok(mut c) = CURRENT.lock() {
        *c = Some((source.map(Path::to_owned), name.to_owned()));
    }
}

fn current() -> (Option<PathBuf>, String) {
    CURRENT
        .lock()
        .ok()
        .and_then(|c| c.clone())
        .unwrap_or_else(|| (None, "Untitled".into()))
}

fn sidecar(flora: &Path) -> PathBuf {
    flora.with_extension("json")
}

fn write_pair(dir: &Path, flora: &Path, text: &str, meta: &Meta) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    files::write_atomic(flora, text).map_err(|e| e.to_string())?;
    let m = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    files::write_atomic(&sidecar(flora), &m).map_err(|e| e.to_string())
}

/// A crash copy of `patch` (`<name>-<unix time>.flora`), for the crash
/// screen. Serialisation is guarded: the patch may be what panicked.
pub fn write_crash_copy(patch: &PatchFile) -> Result<PathBuf, String> {
    let (source, name) = current();
    let name = patch
        .meta
        .as_ref()
        .and_then(|m| m.name.clone())
        .unwrap_or(name);
    let now = files::unix_now();
    let dir = dir();
    let path = dir.join(format!("{}-{now}.flora", files::slug(&name)));
    let text = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        files::to_flora(patch, None, &files::now_iso())
    }))
    .map_err(|_| "the patch couldn't be serialised".to_owned())?;
    let meta = Meta {
        session: session().to_owned(),
        kind: Kind::Crash,
        source,
        name,
        saved_at: now,
    };
    write_pair(&dir, &path, &text, &meta)?;
    Ok(path)
}

/// What an emergency save writes: a recent copy of the unsaved document.
struct Snapshot {
    rev: u64,
    patch: PatchFile,
    meta: Meta,
}

/// An [`Autosaver`]'s state as the panic hook sees it.
struct Emergency {
    dir: PathBuf,
    path: PathBuf,
    snapshot: Mutex<Option<Snapshot>>,
    /// Revision an emergency save wrote, until the app shows it survived.
    written: AtomicU64,
}

impl Emergency {
    fn save(&self) {
        // The panic may have struck while the snapshot was being replaced.
        let Ok(guard) = self.snapshot.try_lock() else {
            return;
        };
        let Some(s) = guard.as_ref() else {
            return;
        };
        let text = files::to_flora(&s.patch, None, &files::now_iso());
        match write_pair(&self.dir, &self.path, &text, &s.meta) {
            Ok(()) => {
                self.written.store(s.rev, Ordering::SeqCst);
                eprintln!("floret: unsaved changes written to {}", self.path.display());
            }
            Err(e) => eprintln!("floret: emergency save failed: {e}"),
        }
    }
}

/// The app's autosaver, for the panic hook.
static APP_EMERGENCY: OnceLock<Arc<Emergency>> = OnceLock::new();

/// Write the open document's unsaved state now (from the panic hook, for a
/// panic nothing will catch). Its autosave stays on disk through the exit
/// that follows, so the next launch offers it.
pub fn emergency_save() {
    if let Some(e) = APP_EMERGENCY.get() {
        e.save();
    }
}

/// Periodic autosave of the open document while it has unsaved changes.
pub struct Autosaver {
    dir: PathBuf,
    session: String,
    /// Revision last written (`None`: nothing on disk).
    written: Option<u64>,
    last: Instant,
    pending: Option<std::thread::JoinHandle<()>>,
    emergency: Arc<Emergency>,
    /// When the snapshot was last taken, and of which revision.
    snapshot: Option<(Instant, u64)>,
}

impl Default for Autosaver {
    fn default() -> Self {
        let a = Self::new(dir(), session());
        let _ = APP_EMERGENCY.set(a.emergency.clone());
        a
    }
}

impl Autosaver {
    pub fn new(dir: PathBuf, session: &str) -> Autosaver {
        let path = dir.join(format!("autosave-{session}.flora"));
        Autosaver {
            emergency: Arc::new(Emergency {
                dir: dir.clone(),
                path,
                snapshot: Mutex::new(None),
                written: AtomicU64::new(NO_REV),
            }),
            dir,
            session: session.to_owned(),
            written: None,
            last: Instant::now(),
            pending: None,
            snapshot: None,
        }
    }

    fn meta(&self, doc: &files::Document) -> Meta {
        Meta {
            session: self.session.clone(),
            kind: Kind::Autosave,
            source: doc.path().map(Path::to_owned),
            name: doc.name.clone(),
            saved_at: files::unix_now(),
        }
    }

    fn set_snapshot(&self, s: Option<Snapshot>) {
        if let Ok(mut g) = self.emergency.snapshot.lock() {
            *g = s;
        }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("autosave-{}.flora", self.session))
    }

    /// Call every frame. Writes when `dirty`, the revision changed since
    /// the last write and `interval` has passed; removes the autosave once
    /// the document is clean (saved, or changes undone / discarded).
    pub fn tick(
        &mut self,
        now: Instant,
        interval: Option<Duration>,
        doc: &files::Document,
        rev: u64,
        patch: &PatchFile,
    ) {
        // Still running after an emergency save: that panic was survived,
        // and the copy is an ordinary autosave now.
        let e = self.emergency.written.swap(NO_REV, Ordering::SeqCst);
        if e != NO_REV {
            self.written = Some(e);
        }
        if !doc.is_dirty(rev) {
            if self.written.is_some() {
                self.clear();
            }
            if self.snapshot.take().is_some() {
                self.set_snapshot(None);
            }
            self.last = now;
            return;
        }
        let Some(interval) = interval else {
            return;
        };
        let due = self
            .snapshot
            .is_none_or(|(at, r)| r != rev && now.duration_since(at) >= SNAPSHOT_EVERY);
        if due {
            self.snapshot = Some((now, rev));
            self.set_snapshot(Some(Snapshot {
                rev,
                patch: patch.clone(),
                meta: self.meta(doc),
            }));
        }
        let busy = self.pending.as_ref().is_some_and(|h| !h.is_finished());
        if busy || self.written == Some(rev) || now.duration_since(self.last) < interval {
            return;
        }
        self.last = now;
        self.written = Some(rev);
        let patch = patch.clone();
        let meta = self.meta(doc);
        let (dir, path) = (self.dir.clone(), self.path());
        self.pending = Some(std::thread::spawn(move || {
            let text = files::to_flora(&patch, None, &files::now_iso());
            if let Err(e) = write_pair(&dir, &path, &text, &meta) {
                eprintln!("floret: autosave failed: {e}");
            }
        }));
    }

    /// Write at the next due check, without waiting out the interval
    /// (after restoring a recovery file).
    pub fn soon(&mut self) {
        self.written = None;
        self.last = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(Instant::now);
    }

    pub(crate) fn wait(&mut self) {
        if let Some(h) = self.pending.take() {
            let _ = h.join();
        }
    }

    /// Remove this session's autosave.
    pub fn clear(&mut self) {
        self.wait();
        let p = self.path();
        let _ = std::fs::remove_file(sidecar(&p));
        let _ = std::fs::remove_file(&p);
        self.written = None;
    }

    /// A clean exit: drop the autosave, and the sidecars of this session's
    /// crash copies (the copies stay where the crash screen said they are,
    /// but are no longer offered). Not after an emergency save: the exit is
    /// the panic's.
    pub fn clean_exit(&mut self) {
        if self.emergency.written.load(Ordering::SeqCst) != NO_REV {
            self.wait();
            return;
        }
        self.clear();
        for (flora, meta) in entries(&self.dir) {
            if meta.session == self.session {
                let _ = std::fs::remove_file(sidecar(&flora));
            }
        }
    }
}

/// A recovery file offered on launch.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub flora: PathBuf,
    pub meta: Meta,
}

impl Candidate {
    /// The patch as the copy holds it. File references are left as the
    /// editor had them: the copy lives here, not next to the files they
    /// name ([`crate::session::restore`] anchors the relative ones).
    pub fn read(&self) -> Result<PatchFile, String> {
        files::read_unresolved(&self.flora).map(|(patch, _)| patch)
    }

    /// Forget it (and delete an autosave; crash copies stay on disk).
    pub fn discard(&self) {
        let _ = std::fs::remove_file(sidecar(&self.flora));
        if self.meta.kind == Kind::Autosave {
            let _ = std::fs::remove_file(&self.flora);
        }
    }

    /// "3 minutes ago" style age.
    pub fn age(&self) -> String {
        let s = files::unix_now().saturating_sub(self.meta.saved_at);
        match s {
            0..60 => "just now".into(),
            60..3600 => plural(s / 60, "minute"),
            3600..86_400 => plural(s / 3600, "hour"),
            _ => plural(s / 86_400, "day"),
        }
    }
}

fn plural(n: u64, unit: &str) -> String {
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

fn entries(dir: &Path) -> Vec<(PathBuf, Meta)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|j| {
            let meta: Meta = serde_json::from_str(&std::fs::read_to_string(&j).ok()?).ok()?;
            Some((j.with_extension("flora"), meta))
        })
        .collect()
}

fn mtime_secs(p: &Path) -> Option<u64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

/// Recovery files left by other sessions that are worth offering: the
/// newest per session, only when newer than the file it was editing. Stale
/// autosaves are deleted, and anything older than a month is pruned.
pub fn scan() -> Vec<Candidate> {
    scan_dir(&dir(), session())
}

pub fn scan_dir(dir: &Path, current_session: &str) -> Vec<Candidate> {
    let now = files::unix_now();
    // Old files, sidecar or not.
    if let Ok(rd) = std::fs::read_dir(dir) {
        for p in rd.flatten().map(|e| e.path()) {
            if mtime_secs(&p).is_some_and(|t| now.saturating_sub(t) > MAX_AGE.as_secs()) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    let mut newest: Vec<Candidate> = Vec::new();
    for (flora, meta) in entries(dir) {
        if meta.session == current_session {
            continue;
        }
        let c = Candidate { flora, meta };
        if !c.flora.exists() {
            let _ = std::fs::remove_file(sidecar(&c.flora));
            continue;
        }
        let stale = c
            .meta
            .source
            .as_deref()
            .and_then(mtime_secs)
            .is_some_and(|saved| saved >= c.meta.saved_at);
        if stale {
            c.discard();
            continue;
        }
        match newest.iter_mut().find(|n| n.meta.session == c.meta.session) {
            Some(n) if n.meta.saved_at >= c.meta.saved_at => {}
            Some(n) => *n = c,
            None => newest.push(c),
        }
    }
    newest.sort_by_key(|c| std::cmp::Reverse(c.meta.saved_at));
    newest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(name: &str) -> PatchFile {
        PatchFile::load(&format!(
            r#"{{"kind":"floret.patch","version":2,"meta":{{"name":"{name}"}},"nodes":[],"edges":[]}}"#
        ))
        .unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("floret-rec-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn autosaves_while_dirty_and_clears_when_clean() {
        let dir = tmp("auto");
        let mut a = Autosaver::new(dir.clone(), "s1");
        let doc = files::Document::untitled(1);
        let t0 = Instant::now();
        let every = Some(Duration::from_secs(10));
        a.tick(t0, every, &doc, 1, &patch("x"));
        a.wait();
        assert!(!a.path().exists(), "clean: nothing written");
        a.tick(t0 + Duration::from_secs(5), every, &doc, 2, &patch("x"));
        a.wait();
        assert!(!a.path().exists(), "not due yet");
        a.tick(t0 + Duration::from_secs(11), every, &doc, 2, &patch("x"));
        a.wait();
        assert!(a.path().exists() && sidecar(&a.path()).exists());
        // Another session sees it.
        let found = scan_dir(&dir, "s2");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.kind, Kind::Autosave);
        assert_eq!(found[0].meta.name, "Untitled");
        assert!(found[0].read().is_ok());
        assert!(scan_dir(&dir, "s1").is_empty(), "never offers its own");
        // Back to the saved revision: removed.
        a.tick(t0 + Duration::from_secs(12), every, &doc, 1, &patch("x"));
        assert!(!a.path().exists());
        // Autosave off: nothing written.
        a.tick(t0 + Duration::from_secs(40), None, &doc, 3, &patch("x"));
        a.wait();
        assert!(!a.path().exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_autosaves_are_dropped() {
        let dir = tmp("stale");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("doc.flora");
        std::fs::write(&source, "{}").unwrap();
        let old = Meta {
            session: "old".into(),
            kind: Kind::Autosave,
            source: Some(source.clone()),
            name: "doc".into(),
            saved_at: files::unix_now() - 100,
        };
        let f = dir.join("autosave-old.flora");
        write_pair(&dir, &f, &files::to_flora(&patch("doc"), None, "t"), &old).unwrap();
        assert!(scan_dir(&dir, "now").is_empty(), "the saved file is newer");
        assert!(!f.exists(), "stale autosave deleted");
        // Newer than the saved file: offered.
        let newer = Meta {
            saved_at: files::unix_now() + 100,
            ..old.clone()
        };
        write_pair(&dir, &f, &files::to_flora(&patch("doc"), None, "t"), &newer).unwrap();
        let found = scan_dir(&dir, "now");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.source.as_deref(), Some(source.as_path()));
        found[0].discard();
        assert!(scan_dir(&dir, "now").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn newest_per_session_and_clean_exit() {
        let dir = tmp("sessions");
        let mk = |session: &str, file: &str, kind, at| {
            let m = Meta {
                session: session.into(),
                kind,
                source: None,
                name: file.into(),
                saved_at: at,
            };
            let f = dir.join(format!("{file}.flora"));
            write_pair(&dir, &f, &files::to_flora(&patch(file), None, "t"), &m).unwrap();
            f
        };
        let now = files::unix_now();
        mk("a", "a-crash", Kind::Crash, now - 50);
        mk("a", "autosave-a", Kind::Autosave, now - 10);
        mk("b", "autosave-b", Kind::Autosave, now - 5);
        let found = scan_dir(&dir, "c");
        let names: Vec<&str> = found.iter().map(|c| c.meta.name.as_str()).collect();
        assert_eq!(names, ["autosave-b", "autosave-a"]);
        // Session "a" exits cleanly: nothing of it is offered any more,
        // but its crash copy stays on disk.
        let mut a = Autosaver::new(dir.clone(), "a");
        a.clean_exit();
        let found = scan_dir(&dir, "c");
        assert_eq!(found.len(), 1);
        assert!(dir.join("a-crash.flora").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn emergency_save_survives_the_exit_after_a_panic() {
        let dir = tmp("emergency");
        let mut a = Autosaver::new(dir.clone(), "e1");
        let doc = files::Document::untitled(1);
        let t0 = Instant::now();
        let every = Some(Duration::from_secs(30));
        // Nothing to save while clean.
        a.tick(t0, every, &doc, 1, &patch("x"));
        a.emergency.save();
        assert!(!a.path().exists());
        // Dirty: no autosave is due yet, but a panic writes the snapshot.
        a.tick(t0 + Duration::from_millis(10), every, &doc, 2, &patch("x"));
        a.emergency.save();
        assert!(a.path().exists());
        // The exit that follows the panic keeps it for the next launch.
        a.clean_exit();
        let found = scan_dir(&dir, "e2");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.kind, Kind::Autosave);
        // Had the app survived, the next tick adopts it as its autosave and
        // a save removes it.
        a.tick(t0 + Duration::from_secs(1), every, &doc, 1, &patch("x"));
        assert!(!a.path().exists());
        a.clean_exit();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn crash_copies_name_and_source() {
        set_current(Some(Path::new("/tmp/x.flora")), "x");
        let p = write_crash_copy(&patch("Recovery Sketch")).unwrap();
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("recovery-sketch-"), "{name}");
        let meta: Meta =
            serde_json::from_str(&std::fs::read_to_string(sidecar(&p)).unwrap()).unwrap();
        assert_eq!(meta.kind, Kind::Crash);
        assert_eq!(meta.source.as_deref(), Some(Path::new("/tmp/x.flora")));
        assert!(files::read(&p).is_ok());
        let _ = std::fs::remove_file(sidecar(&p));
        let _ = std::fs::remove_file(p);
    }
}
