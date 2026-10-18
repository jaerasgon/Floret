//! Tests that run against a temporary config folder: `FLORET_CONFIG_DIR`
//! pointed at an empty folder for the length of the test, so the real
//! read / write functions (which find their files through it) are the ones
//! tested, and a developer's own settings are never touched.
//!
//! The environment is shared by every test thread, so users of this hold a
//! lock until they are done.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// An empty config folder in use. Dropping it restores the environment and
/// removes the folder.
pub struct ConfigDir {
    pub dir: PathBuf,
    previous: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl ConfigDir {
    /// A path inside the folder.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

/// Point `FLORET_CONFIG_DIR` at a new folder (not yet created: the app
/// makes it on the first write, as on a first launch).
pub fn config_dir(name: &str) -> ConfigDir {
    let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("floret-cfg-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let previous = std::env::var_os("FLORET_CONFIG_DIR");
    std::env::set_var("FLORET_CONFIG_DIR", &dir);
    ConfigDir {
        dir,
        previous,
        _lock: lock,
    }
}

impl Drop for ConfigDir {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => std::env::set_var("FLORET_CONFIG_DIR", v),
            None => std::env::remove_var("FLORET_CONFIG_DIR"),
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
