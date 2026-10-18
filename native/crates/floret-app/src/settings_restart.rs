//! Settings survive a restart (#82): each per-user file is written by the
//! app's own code into the config folder (`FLORET_CONFIG_DIR`, here a
//! temporary one) and read back by a fresh load, as the next launch does.
//! The dock layout and the Library have theirs in `workspace.rs` and
//! `library.rs`, next to the private state they restore.

use crate::editor::favorites::{self, Favorites};
use crate::files::Recent;
use crate::prefs::{Prefs, Startup, ThemePref};
use crate::test_env::config_dir;

/// A file in the config folder that is damaged (half written by an older
/// build, edited by hand, from a newer version) never stops the app
/// starting: it loads as if it were not there.
const DAMAGED: [&str; 4] = ["", "{", "[1, 2", "\u{0}\u{0}garbage"];

#[test]
fn prefs_are_written_to_prefs_json_and_come_back() {
    let cfg = config_dir("prefs");
    assert_eq!(
        favorites::config_dir().as_deref(),
        Some(cfg.dir.as_path()),
        "the app looks where the test says"
    );
    // First launch: nothing there, the defaults.
    assert_eq!(Prefs::load(), Prefs::default());
    let changed = Prefs {
        theme: ThemePref::Light,
        ui_scale: 1.25,
        flow_animation: true,
        snap_to_grid: true,
        node_gravity: false,
        grid_size: 32,
        show_grid: false,
        show_minimap: false,
        ambient_motion: false,
        reduce_motion: true,
        palette_visible: true,
        default_render_width: 1920,
        default_render_height: 1080,
        startup_patch: Startup::Blank,
        autosave_enabled: false,
        autosave_interval_secs: 120,
        show_welcome: false,
        library_tile_width: 160.0,
    };
    // The config folder does not exist yet: saving makes it.
    assert!(!cfg.dir.exists());
    changed.save();
    assert!(cfg.file("prefs.json").is_file());
    // A restart: a new load reads what was saved.
    assert_eq!(Prefs::load(), changed);
    // Changing one setting changes only that one.
    let mut next = changed.clone();
    next.theme = ThemePref::System;
    next.save();
    let back = Prefs::load();
    assert_eq!(back.theme, ThemePref::System);
    assert_eq!(back.grid_size, 32);
    assert_eq!(back.default_render_width, 1920);
    assert_eq!(back.startup_patch, Startup::Blank);
    assert!(!cfg.file("prefs.json.tmp").exists());
}

#[test]
fn prefs_from_an_older_or_damaged_file_load() {
    let cfg = config_dir("prefs-old");
    std::fs::create_dir_all(&cfg.dir).unwrap();
    // An older build's file has fewer keys; the rest take their defaults.
    std::fs::write(cfg.file("prefs.json"), r#"{"theme":"light","gridSize":48}"#).unwrap();
    let p = Prefs::load();
    assert_eq!((p.theme, p.grid_size), (ThemePref::Light, 48));
    assert_eq!(p.startup_patch, Prefs::default().startup_patch);
    // A newer build's file has keys this one doesn't know.
    std::fs::write(
        cfg.file("prefs.json"),
        r#"{"theme":"dark","fancyNewThing":{"a":1},"gridSize":16}"#,
    )
    .unwrap();
    assert_eq!(Prefs::load().grid_size, 16);
    for bad in DAMAGED {
        std::fs::write(cfg.file("prefs.json"), bad).unwrap();
        assert_eq!(Prefs::load(), Prefs::default(), "{bad:?}");
    }
}

#[test]
fn favorites_keep_their_order_across_a_restart() {
    let cfg = config_dir("favorites");
    assert!(Favorites::load().types().is_empty());
    let mut f = Favorites::load();
    for t in ["draw/rect", "math/add", "time/seconds"] {
        f.toggle(t);
    }
    assert!(cfg.file("favorites.json").is_file());
    let mut again = Favorites::load();
    assert_eq!(again.types(), ["draw/rect", "math/add", "time/seconds"]);
    again.toggle("math/add");
    again.toggle("draw/ellipse");
    // And once more, as a third launch.
    let third = Favorites::load();
    assert_eq!(third.types(), ["draw/rect", "time/seconds", "draw/ellipse"]);
    assert!(third.contains("draw/ellipse") && !third.contains("math/add"));
    assert!(!cfg.file("favorites.json.tmp").exists());
    for bad in DAMAGED {
        std::fs::write(cfg.file("favorites.json"), bad).unwrap();
        assert!(Favorites::load().types().is_empty(), "{bad:?}");
    }
    // A damaged file doesn't stop starring a node: it is replaced.
    let mut f = Favorites::load();
    f.toggle("draw/rect");
    assert_eq!(Favorites::load().types(), ["draw/rect"]);
}

#[test]
fn recent_files_survive_a_restart_most_recent_first() {
    let cfg = config_dir("recent");
    let files: Vec<_> = (0..12)
        .map(|i| {
            let p = std::env::temp_dir()
                .join(format!("floret-recent-{}-{i}.flora", std::process::id()));
            std::fs::write(&p, "x").unwrap();
            p
        })
        .collect();
    let mut r = Recent::load();
    assert!(r.paths().is_empty());
    for p in &files {
        r.push(p);
    }
    assert!(cfg.file("recent.json").is_file());
    // Ten are kept, newest first (paths are stored as the OS names them).
    let again = Recent::load();
    assert_eq!(again.paths().len(), 10);
    let canon = |p: &std::path::PathBuf| p.canonicalize().unwrap();
    assert_eq!(again.paths()[0], canon(&files[11]));
    assert_eq!(again.paths()[9], canon(&files[2]));
    // Opening an old one again brings it to the front for the next launch.
    let mut r = Recent::load();
    r.push(&files[5]);
    assert_eq!(Recent::load().paths()[0], canon(&files[5]));
    assert_eq!(
        Recent::load()
            .paths()
            .iter()
            .filter(|p| **p == canon(&files[5]))
            .count(),
        1
    );
    // Removing one (a file that is gone), then clearing the list.
    let mut r = Recent::load();
    r.remove(&canon(&files[11]));
    assert!(!Recent::load().paths().contains(&canon(&files[11])));
    r.clear();
    assert!(Recent::load().paths().is_empty());
    for bad in DAMAGED {
        std::fs::write(cfg.file("recent.json"), bad).unwrap();
        assert!(Recent::load().paths().is_empty(), "{bad:?}");
    }
    for p in files {
        let _ = std::fs::remove_file(p);
    }
}

/// Opening a file puts it in the list the next launch reads, through the
/// real save path (`session::save`), not only `Recent` on its own.
#[test]
fn saving_a_patch_puts_it_in_the_recent_list_for_the_next_launch() {
    use crate::files::{self, Document};
    use crate::session::{self, How};
    let cfg = config_dir("recent-save");
    let dir = cfg.dir.join("work");
    let target = dir.join("kept.flora");
    let mut patch = crate::app::empty_patch();
    let mut doc = Document::untitled(0);
    let out = session::save(&mut doc, &mut patch, 1, How::Save, |_| Some(target.clone()));
    // What the app does with a successful save.
    let session::Saved::Done { path, .. } = out else {
        panic!("{out:?}");
    };
    let mut r = Recent::load();
    r.push(&path);
    let next_launch = Recent::load();
    assert_eq!(next_launch.paths(), [path.canonicalize().unwrap()]);
    assert!(files::read(&next_launch.paths()[0]).is_ok());
}
