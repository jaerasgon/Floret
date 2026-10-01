//! Font dropdowns: the generic families, then every installed family (and
//! any loaded font file's), for `text/font`'s family and
//! `style/text-params`' font.

use std::sync::Mutex;

const GENERIC: &[&str] = &["sans-serif", "serif", "monospace"];

/// Choices for a font field, or `None` if `key` isn't one.
pub fn options(node_type: &str, key: &str) -> Option<Vec<String>> {
    match (node_type, key) {
        ("text/font", "family") | ("style/text-params", "font") => Some(families()),
        _ => None,
    }
}

/// Cached until the engine loads another font.
fn families() -> Vec<String> {
    static CACHE: Mutex<Option<(u64, Vec<String>)>> = Mutex::new(None);
    let generation = floret_engine::text::generation();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((g, list)) = &*cache {
        if *g == generation {
            return list.clone();
        }
    }
    let list: Vec<String> = GENERIC
        .iter()
        .map(|g| (*g).to_owned())
        .chain(floret_engine::text::families())
        .collect();
    *cache = Some((generation, list.clone()));
    list
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_generic_then_installed_families() {
        let o = super::options("text/font", "family").unwrap();
        assert_eq!(&o[..3], ["sans-serif", "serif", "monospace"]);
        assert!(o.iter().any(|f| f == floret_engine::text::VARIABLE_FAMILY));
        assert!(super::options("style/text-params", "font").is_some());
        assert!(super::options("text/font", "file").is_none());
    }
}
