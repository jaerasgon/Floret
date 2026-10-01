//! Crash screen (#48): a panic in the engine tick or the preview render
//! doesn't close the app. The frame is abandoned, evaluation pauses, the
//! patch is written to a recovery file, and a dialog shows the error with
//! Copy details / Save patch as… / Reload patch / Continue.
//!
//! A panic hook records the message, location and backtrace of each panic
//! on its own thread (the default hook still prints it), so whoever catches
//! it with [`Guard::run`] can show where it came from.

use crate::theme;
use eframe::egui::{self, RichText};
use floret_engine::PatchFile;
use std::backtrace::Backtrace;
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;

thread_local! {
    static LAST_PANIC: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Record every panic's location and backtrace (then run the default hook).
pub fn install_hook() {
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let location = info.location().map_or_else(String::new, |l| {
            format!("{}:{}:{}", l.file(), l.line(), l.column())
        });
        let bt = Backtrace::force_capture().to_string();
        LAST_PANIC.with(|p| *p.borrow_mut() = Some((location, bt)));
        default(info);
    }));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Tick,
    Render,
}

pub struct Report {
    pub stage: Stage,
    pub message: String,
    pub location: String,
    pub backtrace: String,
    /// Where the patch was saved when the panic was caught.
    pub recovery: Result<PathBuf, String>,
}

impl Report {
    pub fn details(&self) -> String {
        format!(
            "Floret {} — panic during {}\n\n{}\nat {}\n\nPatch saved to: {}\n\nBacktrace:\n{}",
            env!("CARGO_PKG_VERSION"),
            match self.stage {
                Stage::Tick => "evaluation",
                Stage::Render => "rendering",
            },
            self.message,
            self.location,
            match &self.recovery {
                Ok(p) => p.display().to_string(),
                Err(e) => format!("(failed: {e})"),
            },
            self.backtrace,
        )
    }
}

/// What the app must do after the crash dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Rebuild the engine from the patch (fresh state) and run again.
    Reload,
    /// Drop the outputs: the last scene made the renderer panic.
    ClearOutputs,
}

#[derive(Default)]
pub struct Guard {
    report: Option<Report>,
    dialog_open: bool,
    paused: bool,
    show_backtrace: bool,
    note: Option<String>,
}

impl Guard {
    /// Evaluation is paused after a panic until Reload or Resume.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Skip the preview while the scene that made it panic is still there.
    pub fn render_ok(&self) -> bool {
        !(self.dialog_open
            && self
                .report
                .as_ref()
                .is_some_and(|r| r.stage == Stage::Render))
    }

    /// Run `f`, catching a panic: then pause, save `patch` for recovery and
    /// open the crash dialog. `None` when it panicked.
    pub fn run<R>(&mut self, stage: Stage, patch: &PatchFile, f: impl FnOnce() -> R) -> Option<R> {
        match panic::catch_unwind(AssertUnwindSafe(|| {
            test_panic(stage);
            f()
        })) {
            Ok(r) => Some(r),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "(no message)".to_owned());
                let (location, backtrace) = LAST_PANIC
                    .with(|p| p.borrow_mut().take())
                    .unwrap_or_default();
                self.report = Some(Report {
                    stage,
                    message,
                    location,
                    backtrace,
                    recovery: save_recovery(patch),
                });
                self.dialog_open = true;
                self.paused = true;
                self.show_backtrace = false;
                self.note = None;
                None
            }
        }
    }

    /// The crash dialog (while open) or the "paused" banner.
    pub fn show(&mut self, ctx: &egui::Context, patch: &PatchFile) -> Option<Action> {
        if self.dialog_open {
            self.dialog(ctx, patch)
        } else if self.paused {
            self.banner(ctx)
        } else {
            None
        }
    }

    fn dialog(&mut self, ctx: &egui::Context, patch: &PatchFile) -> Option<Action> {
        let report = self.report.as_ref()?;
        let mut action = None;
        egui::Modal::new(egui::Id::new("floret-crash")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(
                RichText::new("Something went wrong")
                    .color(theme::DANGER)
                    .strong()
                    .size(16.0),
            );
            ui.add_space(4.0);
            let doing = match report.stage {
                Stage::Tick => "evaluating the patch",
                Stage::Render => "drawing the preview",
            };
            ui.label(
                RichText::new(format!(
                    "Floret hit an internal error while {doing}. Evaluation is paused; \
                     your patch is still open and nothing was lost."
                ))
                .color(theme::FG_1),
            );
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(theme::BG_0)
                .corner_radius(4)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&report.message).monospace());
                    if !report.location.is_empty() {
                        let at = RichText::new(format!("at {}", report.location));
                        ui.label(at.monospace().color(theme::FG_2).small());
                    }
                });
            let (saved, col) = match &report.recovery {
                Ok(p) => (
                    format!("A copy of the patch was saved to {}", p.display()),
                    theme::FG_2,
                ),
                Err(e) => (format!("Couldn't save a recovery copy: {e}"), theme::DANGER),
            };
            ui.label(RichText::new(saved).color(col).small());
            ui.add_space(4.0);
            ui.checkbox(&mut self.show_backtrace, "Show backtrace");
            if self.show_backtrace {
                egui::ScrollArea::both().max_height(220.0).show(ui, |ui| {
                    ui.label(RichText::new(&report.backtrace).monospace().size(10.0));
                });
            }
            if let Some(n) = &self.note {
                ui.label(RichText::new(n).color(theme::FG_2).small());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Copy details").clicked() {
                    ui.ctx().copy_text(report.details());
                    self.note = Some("Details copied to the clipboard.".into());
                }
                if ui.button("Save patch as…").clicked() {
                    self.note = save_as(patch);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Continue (paused)")
                        .on_hover_text("Close this and keep editing with evaluation paused")
                        .clicked()
                    {
                        action = (report.stage == Stage::Render).then_some(Action::ClearOutputs);
                        self.dialog_open = false;
                    }
                    if ui
                        .button(RichText::new("Reload patch").strong())
                        .on_hover_text("Restart the patch from a fresh state")
                        .clicked()
                    {
                        action = Some(Action::Reload);
                        self.dialog_open = false;
                        self.paused = false;
                    }
                });
            });
        });
        action
    }

    fn banner(&mut self, ctx: &egui::Context) -> Option<Action> {
        let mut action = None;
        egui::Area::new(egui::Id::new("floret-paused"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 46.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("⏸ Evaluation paused after an error").color(theme::FG_1),
                        );
                        if ui.button("Resume").clicked() {
                            self.paused = false;
                        }
                        if ui.button("Reload patch").clicked() {
                            self.paused = false;
                            action = Some(Action::Reload);
                        }
                        if ui.button("Details").clicked() {
                            self.dialog_open = true;
                        }
                    });
                });
            });
        action
    }
}

/// Debug builds: `FLORET_TEST_PANIC=tick|render` panics once in that stage
/// on the 120th call, to try the crash screen.
fn test_panic(stage: Stage) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static CALLS: AtomicU32 = AtomicU32::new(0);
    if !cfg!(debug_assertions) {
        return;
    }
    let want = match std::env::var("FLORET_TEST_PANIC").as_deref() {
        Ok("tick") => Stage::Tick,
        Ok("render") => Stage::Render,
        _ => return,
    };
    if stage == want && CALLS.fetch_add(1, Ordering::Relaxed) == 120 {
        panic!("FLORET_TEST_PANIC: deliberate panic during {stage:?}");
    }
}

fn patch_name(patch: &PatchFile) -> String {
    let name = patch
        .meta
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .unwrap_or("untitled");
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || "._-".contains(c) {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug
}

/// Write the patch to `<temp>/floret-recovery/<name>-<unix time>.flora`.
fn save_recovery(patch: &PatchFile) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("floret-recovery");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = dir.join(format!("{}-{secs}.flora", patch_name(patch)));
    let json = panic::catch_unwind(AssertUnwindSafe(|| patch.to_json()))
        .map_err(|_| "the patch couldn't be serialised".to_owned())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Native Save dialog for the patch (TS `suggestedFilename`). Returns a note.
fn save_as(patch: &PatchFile) -> Option<String> {
    let path = rfd::FileDialog::new()
        .set_title("Save patch as")
        .add_filter("Floret patch", &["flora"])
        .set_file_name(format!("{}.flora", patch_name(patch)))
        .save_file()?;
    Some(match std::fs::write(&path, patch.to_json()) {
        Ok(()) => format!("Saved to {}", path.display()),
        Err(e) => format!("Couldn't save: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> PatchFile {
        PatchFile::load(r#"{"kind":"floret.patch","version":2,"meta":{"name":"My Sketch"},"nodes":[],"edges":[]}"#)
            .unwrap()
    }

    #[test]
    fn a_panic_is_caught_reported_and_pauses() {
        install_hook();
        let mut g = Guard::default();
        assert_eq!(g.run(Stage::Tick, &patch(), || 7), Some(7));
        assert!(!g.paused());
        let r: Option<()> = g.run(Stage::Render, &patch(), || panic!("boom {}", 1));
        assert!(r.is_none());
        assert!(g.paused() && !g.render_ok());
        let rep = g.report.as_ref().unwrap();
        assert_eq!(rep.message, "boom 1");
        assert!(rep.location.contains("crash.rs"));
        let saved = rep.recovery.as_ref().unwrap();
        assert!(saved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("my-sketch-"));
        assert!(PatchFile::load(&std::fs::read_to_string(saved).unwrap()).is_ok());
        let _ = std::fs::remove_file(saved);
    }
}
