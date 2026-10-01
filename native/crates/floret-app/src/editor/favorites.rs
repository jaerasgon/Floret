//! Favorite node types (`favorites.ts`): starred from the palette or the
//! search popover, pinned atop the palette and listed in the Q menu.
//! Insertion order is display order. Persisted as a JSON array in the
//! per-user config dir; stale types are filtered by the readers.

use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Favorites {
    types: Vec<String>,
    path: Option<PathBuf>,
}

/// Floret's per-user config directory (`FLORET_CONFIG_DIR` overrides).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FLORET_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        env("APPDATA").map(|d| d.join("Floret"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/Floret"))
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .map(|d| d.join("floret"))
    }
}

impl Favorites {
    /// Load from the config dir (empty when missing or unreadable).
    pub fn load() -> Favorites {
        Self::load_from(config_dir().map(|d| d.join("favorites.json")))
    }

    pub fn load_from(path: Option<PathBuf>) -> Favorites {
        let types = path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default();
        Favorites { types, path }
    }

    pub fn types(&self) -> &[String] {
        &self.types
    }

    pub fn contains(&self, ty: &str) -> bool {
        self.types.iter().any(|t| t == ty)
    }

    /// Add if absent, remove if present; persists immediately.
    pub fn toggle(&mut self, ty: &str) {
        if self.contains(ty) {
            self.types.retain(|t| t != ty);
        } else {
            self.types.push(ty.to_owned());
        }
        if let Some(p) = &self.path {
            // Best effort: a read-only config dir just loses persistence.
            let _ = save(p, &self.types);
        }
    }
}

fn save(path: &Path, types: &[String]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(types)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_in_order_and_persists() {
        let dir = std::env::temp_dir().join(format!("floret-fav-{}", std::process::id()));
        let path = dir.join("favorites.json");
        let _ = std::fs::remove_file(&path);
        let mut f = Favorites::load_from(Some(path.clone()));
        assert!(f.types().is_empty());
        for t in ["a", "b", "c"] {
            f.toggle(t);
        }
        assert_eq!(f.types(), ["a", "b", "c"]);
        f.toggle("b");
        assert_eq!(f.types(), ["a", "c"]);
        assert!(f.contains("a") && !f.contains("b"));
        let again = Favorites::load_from(Some(path));
        assert_eq!(again.types(), ["a", "c"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_is_empty() {
        let f = Favorites::load_from(Some(PathBuf::from("/nonexistent/floret/fav.json")));
        assert!(f.types().is_empty());
        assert!(Favorites::load_from(None).types().is_empty());
    }
}
