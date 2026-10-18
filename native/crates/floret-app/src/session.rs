//! What happens to the open document when the user New / Open / Save /
//! Quits: the decisions, apart from the windows that ask them.
//!
//! The app (`app.rs`) draws the "Save changes?" and "Restore unsaved
//! work?" windows and does the I/O side effects (recent files, the
//! recovery bookkeeping, status messages); every decision it acts on is
//! made here, as plain data in and out, so each path is tested without a
//! window:
//!
//! - [`Guard`]: something that would discard unsaved changes is held until
//!   the user chooses Save / Don't Save / Cancel; closing the window asks
//!   the same way.
//! - [`save`]: Save writes in place, Save As (and Save of a patch with no
//!   file, a built-in sample included) asks where. A sample is never
//!   written over.
//! - [`restore`]: a recovery file back into a document that is still
//!   unsaved.

use crate::files::{self, Document, Origin};
use crate::recovery;
use floret_engine::patch::PatchFile;
use std::path::{Path, PathBuf};

/// Something that would discard unsaved changes, held while the user is
/// asked about them.
#[derive(Clone, Debug)]
pub enum After {
    New,
    /// Open a file (`None`: ask which).
    Open(Option<PathBuf>),
    Sample(usize),
    /// A patch from the library panel.
    Patch(Box<PatchFile>, String),
    Recover(recovery::Candidate),
    Quit,
}

impl After {
    /// The end of "…if you don't save them before ___" in the window.
    pub fn doing(&self) -> &'static str {
        match self {
            After::Quit => "quitting",
            After::New => "starting a new patch",
            After::Open(_) => "opening another file",
            After::Sample(_) => "opening the sample",
            After::Patch(..) => "opening the patch",
            After::Recover(_) => "restoring",
        }
    }
}

/// The user's answer to "Save changes before …?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Save,
    DontSave,
    Cancel,
}

/// The "unsaved changes" gate. Holds the action waiting on the user's
/// answer, and whether quitting has been approved (so the window close it
/// leads to goes through instead of asking again).
#[derive(Debug, Default)]
pub struct Guard {
    pending: Option<After>,
    quit_approved: bool,
}

impl Guard {
    /// The action waiting on an answer, while the window is up.
    pub fn pending(&self) -> Option<&After> {
        self.pending.as_ref()
    }

    pub fn is_asking(&self) -> bool {
        self.pending.is_some()
    }

    /// `after` was asked for. With unsaved changes it is held for an
    /// answer (replacing any other held action) and `None` comes back;
    /// otherwise it is returned to be done now.
    pub fn request(&mut self, dirty: bool, after: After) -> Option<After> {
        if dirty {
            self.pending = Some(after);
            None
        } else {
            Some(self.approved(after))
        }
    }

    /// The user answered. `save` runs for [`Choice::Save`] (the Save, or
    /// the Save As dialog for a patch with no file) and says whether the
    /// document was saved. Returns the held action when it should go
    /// ahead: after a save that worked, or Don't Save. After Cancel, or a
    /// save that was cancelled or failed, nothing happens and the held
    /// action is dropped.
    pub fn answer(&mut self, choice: Choice, save: impl FnOnce() -> bool) -> Option<After> {
        let after = self.pending.take()?;
        match choice {
            Choice::Save => save().then(|| self.approved(after)),
            Choice::DontSave => Some(self.approved(after)),
            Choice::Cancel => None,
        }
    }

    /// The window's close button (or Cmd+Q arriving as a close). True when
    /// the close must be held back and the question asked; false when it
    /// can go ahead (nothing unsaved, or quitting was already approved).
    pub fn close_requested(&mut self, dirty: bool) -> bool {
        if dirty && !self.quit_approved {
            self.pending = Some(After::Quit);
            true
        } else {
            false
        }
    }

    /// Quitting is approved from here on: the close it sends is let through.
    fn approved(&mut self, after: After) -> After {
        if matches!(after, After::Quit) {
            self.quit_approved = true;
        }
        after
    }
}

/// How the user asked to save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// Save: in place when the document is a file, else ask where.
    Save,
    /// Save As: always ask where.
    SaveAs,
}

/// What a save did.
#[derive(Debug, PartialEq, Eq)]
pub enum Saved {
    /// Written to `path`; the document is now that file.
    Done { path: PathBuf, name: String },
    /// Couldn't write `path`; the document is as it was.
    Failed { path: PathBuf, error: String },
    /// The Save dialog was cancelled.
    Cancelled,
}

impl Saved {
    /// Whether the document was saved.
    #[cfg(test)]
    pub fn ok(&self) -> bool {
        matches!(self, Saved::Done { .. })
    }
}

/// Save `patch` for `doc` at revision `rev`. `ask` is the Save dialog
/// (given the document, for the suggested name and folder). On success
/// `doc` becomes the saved file, clean at `rev`, and the patch is named
/// after it.
pub fn save(
    doc: &mut Document,
    patch: &mut PatchFile,
    rev: u64,
    how: How,
    ask: impl FnOnce(&Document) -> Option<PathBuf>,
) -> Saved {
    let in_place = match how {
        How::Save => doc.path().map(Path::to_owned),
        How::SaveAs => None,
    };
    let Some(path) = in_place.or_else(|| ask(doc)) else {
        return Saved::Cancelled;
    };
    // Paths the patch holds relative to its old folder keep pointing at
    // the same files when it moves to another.
    let from = doc.path().and_then(Path::parent).map(Path::to_owned);
    match files::save_from(patch, &path, from.as_deref()) {
        Ok(()) => {
            let name = files::file_stem(&path);
            patch.meta.get_or_insert_with(Default::default).name = Some(name.clone());
            doc.saved_as(path.clone(), rev);
            Saved::Done { path, name }
        }
        Err(error) => Saved::Failed { path, error },
    }
}

/// A recovery file read back: the patch, and the document it comes back as.
#[derive(Debug)]
pub struct Restored {
    pub patch: PatchFile,
    pub origin: Origin,
    pub name: String,
}

/// Read a recovery file. It comes back to the file it was editing when
/// that is still there (so Save writes it in place), else as an untitled
/// patch.
pub fn restore(c: &recovery::Candidate) -> Result<Restored, String> {
    let mut patch = c.read()?;
    let origin = match &c.meta.source {
        Some(s) if s.exists() => Origin::File(s.clone()),
        _ => Origin::Untitled,
    };
    // Recovery copies hold the paths as the editor had them; any that were
    // still relative mean the folder of the file being edited.
    if let Origin::File(s) = &origin {
        files::resolve_paths(&mut patch, s.parent());
    }
    Ok(Restored {
        patch,
        origin,
        name: c.meta.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::{Autosaver, Kind};
    use std::time::{Duration, Instant};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("floret-session-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn patch() -> PatchFile {
        floret_engine::load_patch(include_str!("../../../samples/tut-03-animate.flora")).unwrap()
    }

    /// All the things that can be asked for.
    fn actions() -> Vec<After> {
        vec![
            After::New,
            After::Open(None),
            After::Open(Some(PathBuf::from("/x/a.flora"))),
            After::Sample(3),
            After::Patch(Box::new(patch()), "Lib".into()),
            After::Quit,
        ]
    }

    fn same(a: &After, b: &After) -> bool {
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }

    #[test]
    fn a_clean_document_goes_straight_ahead() {
        for a in actions() {
            let mut g = Guard::default();
            let kind = a.clone();
            let now = g.request(false, a).expect("done now");
            assert!(same(&now, &kind));
            assert!(!g.is_asking(), "{:?} asked about nothing", kind);
        }
    }

    #[test]
    fn a_dirty_document_asks_first() {
        for a in actions() {
            let mut g = Guard::default();
            let kind = a.clone();
            assert!(g.request(true, a).is_none(), "{kind:?} waits");
            assert!(g.is_asking() && same(g.pending().unwrap(), &kind));
        }
    }

    #[test]
    fn dont_save_does_the_action_without_saving() {
        for a in actions() {
            let mut g = Guard::default();
            let kind = a.clone();
            g.request(true, a);
            let done = g.answer(Choice::DontSave, || panic!("must not save"));
            assert!(same(&done.expect("goes ahead"), &kind));
            assert!(!g.is_asking());
        }
    }

    #[test]
    fn cancel_does_nothing_and_forgets_the_action() {
        for a in actions() {
            let mut g = Guard::default();
            g.request(true, a);
            assert!(g
                .answer(Choice::Cancel, || panic!("must not save"))
                .is_none());
            assert!(!g.is_asking());
        }
        // A cancelled quit doesn't approve closing the window.
        let mut g = Guard::default();
        g.request(true, After::Quit);
        g.answer(Choice::Cancel, || true);
        assert!(g.close_requested(true), "still asks on the next close");
    }

    #[test]
    fn save_then_continues_with_the_action() {
        for a in actions() {
            let mut g = Guard::default();
            let kind = a.clone();
            g.request(true, a);
            let mut saved = false;
            let done = g.answer(Choice::Save, || {
                saved = true;
                true
            });
            assert!(saved);
            assert!(same(&done.expect("continues"), &kind));
            assert!(!g.is_asking());
        }
    }

    /// A save that fails, or a Save As dialog that is cancelled, stops
    /// the action: nothing is discarded, the user is back in the patch.
    #[test]
    fn a_failed_or_cancelled_save_stops_the_action() {
        for a in actions() {
            let mut g = Guard::default();
            g.request(true, a);
            assert!(g.answer(Choice::Save, || false).is_none());
            assert!(!g.is_asking(), "the question is closed");
        }
        // Quit in particular must not leave quitting approved.
        let mut g = Guard::default();
        g.request(true, After::Quit);
        assert!(g.answer(Choice::Save, || false).is_none());
        assert!(g.close_requested(true));
    }

    #[test]
    fn answering_with_nothing_asked_does_nothing() {
        let mut g = Guard::default();
        assert!(g.answer(Choice::DontSave, || true).is_none());
        assert!(g
            .answer(Choice::Save, || panic!("nothing to save for"))
            .is_none());
    }

    #[test]
    fn a_newer_request_replaces_the_held_one() {
        let mut g = Guard::default();
        g.request(true, After::New);
        g.request(true, After::Sample(2));
        assert!(matches!(g.pending(), Some(After::Sample(2))));
    }

    #[test]
    fn closing_the_window_asks_when_dirty_and_quits_once_approved() {
        let mut g = Guard::default();
        assert!(!g.close_requested(false), "clean: closes");
        assert!(!g.is_asking());
        // Dirty: held back, asks about quitting.
        assert!(g.close_requested(true));
        assert!(matches!(g.pending(), Some(After::Quit)));
        // Don't Save → quit; the close that follows goes through.
        let q = g.answer(Choice::DontSave, || panic!());
        assert!(matches!(q, Some(After::Quit)));
        assert!(!g.close_requested(true), "approved: no second question");
        // The same after Save.
        let mut g = Guard::default();
        g.close_requested(true);
        let q = g.answer(Choice::Save, || true);
        assert!(matches!(q, Some(After::Quit)));
        assert!(!g.close_requested(true));
    }

    #[test]
    fn quit_from_the_menu_asks_and_approves_like_the_close_button() {
        let mut g = Guard::default();
        assert!(g.request(true, After::Quit).is_none());
        assert!(matches!(
            g.answer(Choice::DontSave, || true),
            Some(After::Quit)
        ));
        assert!(!g.close_requested(true));
        // Clean quit is approved at once.
        let mut g = Guard::default();
        assert!(matches!(g.request(false, After::Quit), Some(After::Quit)));
        assert!(!g.close_requested(true));
    }

    #[test]
    fn the_window_says_what_it_is_doing() {
        let said: Vec<&str> = actions().iter().map(After::doing).collect();
        assert_eq!(
            said,
            [
                "starting a new patch",
                "opening another file",
                "opening another file",
                "opening the sample",
                "opening the patch",
                "quitting",
            ]
        );
        assert_eq!(
            After::Recover(recovery::Candidate {
                flora: PathBuf::new(),
                meta: recovery::Meta {
                    session: String::new(),
                    kind: Kind::Autosave,
                    source: None,
                    name: String::new(),
                    saved_at: 0,
                }
            })
            .doing(),
            "restoring"
        );
    }

    // ── Saving ──────────────────────────────────────────────────────────

    #[test]
    fn save_writes_a_file_in_place_without_asking() {
        let dir = tmp("inplace");
        let path = dir.join("a.flora");
        let mut p = patch();
        files::save(&p, &path).unwrap();
        let mut doc = Document::file(path.clone(), 1);
        p.nodes[0].position.x += 10.0;
        let out = save(&mut doc, &mut p, 2, How::Save, |_| panic!("no dialog"));
        assert_eq!(
            out,
            Saved::Done {
                path: path.clone(),
                name: "a".into()
            }
        );
        assert!(!doc.is_dirty(2));
        assert_eq!(doc.path(), Some(path.as_path()));
        let back = files::read(&path).unwrap();
        assert_eq!(back.nodes[0].position.x, p.nodes[0].position.x);
        assert_eq!(back.meta.unwrap().name.as_deref(), Some("a"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_of_an_untitled_patch_asks_where_and_becomes_that_file() {
        let dir = tmp("untitled");
        let target = dir.join("new.flora");
        let mut p = patch();
        let mut doc = Document::untitled(1);
        let mut asked = None;
        let out = save(&mut doc, &mut p, 4, How::Save, |d| {
            asked = Some(d.suggested_file_name());
            Some(target.clone())
        });
        assert_eq!(asked.as_deref(), Some("untitled.flora"));
        assert!(out.ok());
        assert_eq!(doc.origin, Origin::File(target.clone()));
        assert_eq!(doc.name, "new");
        assert!(!doc.is_dirty(4) && doc.is_dirty(5));
        assert_eq!(p.meta.as_ref().unwrap().name.as_deref(), Some("new"));
        assert!(target.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cancelling_the_dialog_leaves_the_document_alone() {
        let mut p = patch();
        let mut doc = Document::untitled(1);
        let out = save(&mut doc, &mut p, 2, How::Save, |_| None);
        assert_eq!(out, Saved::Cancelled);
        assert!(!out.ok());
        assert_eq!(doc.origin, Origin::Untitled);
        assert!(doc.is_dirty(2));
    }

    /// Saving a built-in sample asks where; the sample itself is a
    /// compiled-in string and nothing is written to it. The result is an
    /// ordinary file named after the choice.
    #[test]
    fn save_of_a_sample_asks_and_never_overwrites_the_sample() {
        let dir = tmp("sample");
        let target = dir.join("my-ripple.flora");
        let text = crate::SAMPLES
            .iter()
            .find(|s| s.id == "grid-ripple")
            .unwrap()
            .flora;
        let before = text.to_owned();
        let mut p = floret_engine::load_patch(text).unwrap();
        let mut doc = Document::sample("grid-ripple", "Grid Ripple", 1);
        assert!(doc.path().is_none());
        let mut asked = None;
        let out = save(&mut doc, &mut p, 3, How::Save, |d| {
            asked = Some(d.suggested_file_name());
            Some(target.clone())
        });
        assert_eq!(asked.as_deref(), Some("grid-ripple.flora"));
        assert!(out.ok());
        assert_eq!(doc.origin, Origin::File(target.clone()));
        assert!(target.exists());
        assert_eq!(text, before, "the sample is untouched");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_as_always_asks_and_moves_the_document() {
        let dir = tmp("saveas");
        let (a, b) = (dir.join("a.flora"), dir.join("b.flora"));
        let mut p = patch();
        files::save(&p, &a).unwrap();
        let mut doc = Document::file(a.clone(), 1);
        let out = save(&mut doc, &mut p, 2, How::SaveAs, |d| {
            assert_eq!(d.path(), Some(a.as_path()), "dialog starts in the folder");
            Some(b.clone())
        });
        assert!(out.ok());
        assert_eq!(doc.origin, Origin::File(b.clone()));
        assert!(a.exists() && b.exists(), "the original stays");
        // Cancelling Save As keeps the document where it was.
        let out = save(&mut doc, &mut p, 3, How::SaveAs, |_| None);
        assert_eq!(out, Saved::Cancelled);
        assert_eq!(doc.origin, Origin::File(b));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_save_reports_and_changes_nothing() {
        let dir = tmp("fail");
        // A file where the folder should be: the write cannot succeed.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let target = blocker.join("sub").join("a.flora");
        let mut p = patch();
        p.meta = None;
        let mut doc = Document::untitled(1);
        let out = save(&mut doc, &mut p, 2, How::Save, |_| Some(target.clone()));
        match out {
            Saved::Failed { path, error } => {
                assert_eq!(path, target);
                assert!(!error.is_empty());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(doc.origin, Origin::Untitled, "still not a file");
        assert!(doc.is_dirty(2), "still unsaved");
        assert!(p.meta.is_none(), "not renamed");
        // Through the gate: the held action is dropped, the document kept.
        let mut g = Guard::default();
        g.request(true, After::New);
        let mut result = None;
        let done = g.answer(Choice::Save, || {
            let o = save(&mut doc, &mut p, 2, How::Save, |_| Some(target.clone()));
            result = Some(o.ok());
            o.ok()
        });
        assert_eq!(result, Some(false));
        assert!(done.is_none(), "New did not happen");
        assert!(doc.is_dirty(2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_through_the_gate_for_a_sample_then_opens_the_next_one() {
        let dir = tmp("gate");
        let target = dir.join("kept.flora");
        let mut p = patch();
        let mut doc = Document::sample("tut-03-animate", "Animate", 1);
        let mut g = Guard::default();
        assert!(g.request(true, After::Sample(1)).is_none());
        let done = g.answer(Choice::Save, || {
            save(&mut doc, &mut p, 2, How::Save, |_| Some(target.clone())).ok()
        });
        assert!(matches!(done, Some(After::Sample(1))));
        assert!(target.exists());
        // Save → Cancel in the dialog: the sample is not opened.
        let mut doc = Document::sample("tut-03-animate", "Animate", 1);
        g.request(true, After::Sample(1));
        let done = g.answer(Choice::Save, || {
            save(&mut doc, &mut p, 2, How::Save, |_| None).ok()
        });
        assert!(done.is_none());
        assert!(doc.is_dirty(2));
        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Recovery ────────────────────────────────────────────────────────

    /// Make `file` look saved a minute ago, so an autosave counts as newer.
    fn age(file: &Path) {
        let f = std::fs::File::options().write(true).open(file).unwrap();
        f.set_modified(std::time::SystemTime::now() - Duration::from_secs(60))
            .unwrap();
    }

    /// An autosave of an unsaved edit of `source`, as a crashed session
    /// leaves it, found by the next one.
    fn crashed_autosave(dir: &Path, source: Option<&Path>, name: &str) -> recovery::Candidate {
        let mut p = patch();
        p.meta.get_or_insert_with(Default::default).name = Some(name.into());
        let mut a = Autosaver::new(dir.to_owned(), "old-session");
        let doc = match source {
            Some(s) => Document::file(s.to_owned(), 1),
            None => Document::untitled(1),
        };
        let mut doc = doc;
        doc.name = name.into();
        let t0 = Instant::now();
        a.tick(t0, Some(Duration::ZERO), &doc, 2, &p);
        a.wait();
        let mut found = recovery::scan_dir(dir, "new-session");
        assert_eq!(found.len(), 1);
        found.remove(0)
    }

    #[test]
    fn recover_restores_the_origin_path_and_stays_unsaved() {
        let dir = tmp("recover");
        let source = dir.join("work.flora");
        // The file as last saved, older than the autosave.
        files::save(&patch(), &source).unwrap();
        age(&source);
        let c = crashed_autosave(&dir.join("rec"), Some(&source), "work");
        assert_eq!(c.meta.kind, Kind::Autosave);
        let restored = restore(&c).unwrap();
        assert_eq!(restored.origin, Origin::File(source.clone()));
        assert_eq!(restored.name, "work");
        let Restored {
            patch: p,
            origin,
            name,
        } = restored;
        let doc = Document::recovered(origin, name, 7);
        assert_eq!(
            doc.path(),
            Some(source.as_path()),
            "Save writes it in place"
        );
        assert!(doc.is_dirty(7), "dirty before any edit");
        assert!(!p.nodes.is_empty());
        // Saving clears the dirty state.
        let mut doc = doc;
        let mut p = p;
        let out = save(&mut doc, &mut p, 7, How::Save, |_| panic!("in place"));
        assert!(out.ok());
        assert!(!doc.is_dirty(7));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recover_of_an_untitled_patch_or_a_vanished_file_is_untitled() {
        let dir = tmp("recover-untitled");
        let c = crashed_autosave(&dir.join("rec1"), None, "Untitled");
        let restored = restore(&c).unwrap();
        assert_eq!(restored.origin, Origin::Untitled);
        let doc = Document::recovered(restored.origin, restored.name, 1);
        assert!(doc.is_dirty(1) && doc.path().is_none());
        // Its file has since been deleted.
        let gone = dir.join("gone.flora");
        std::fs::write(&gone, "x").unwrap();
        age(&gone);
        let c = crashed_autosave(&dir.join("rec2"), Some(&gone), "gone");
        std::fs::remove_file(&gone).unwrap();
        let restored = restore(&c).unwrap();
        assert_eq!(restored.origin, Origin::Untitled);
        assert_eq!(restored.name, "gone");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recover_of_a_damaged_file_says_why() {
        let dir = tmp("recover-bad");
        let c = crashed_autosave(&dir.join("rec"), None, "x");
        std::fs::write(&c.flora, "{nope").unwrap();
        let err = restore(&c).unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Restoring while the open document has changes asks first, like any
    /// other action; Cancel leaves the recovery file for later.
    #[test]
    fn restoring_over_unsaved_changes_asks_first() {
        let dir = tmp("recover-ask");
        let c = crashed_autosave(&dir.join("rec"), None, "x");
        let mut g = Guard::default();
        assert!(g.request(true, After::Recover(c.clone())).is_none());
        assert_eq!(g.pending().unwrap().doing(), "restoring");
        assert!(g.answer(Choice::Cancel, || true).is_none());
        assert!(c.flora.exists(), "still on disk");
        // Don't Save: it goes ahead.
        g.request(true, After::Recover(c));
        assert!(matches!(
            g.answer(Choice::DontSave, || true),
            Some(After::Recover(_))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
