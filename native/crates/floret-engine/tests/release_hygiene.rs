//! What ships mentions no AI tooling.
//!
//! The project's working notes file and the local tool folder stay in the
//! repository but are never part of a release:
//! `.gitattributes` marks them `export-ignore`, so `git archive` (and
//! GitHub's source archives and releases) leave them out. Everything that
//! does ship must not contain the word in its path or its text.
//!
//! The scan runs on tracked files (`git ls-files`) and skips itself when
//! the sources aren't a git checkout (a source archive, a vendored copy).

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The word, assembled so this file doesn't contain it.
fn needle() -> String {
    ["cla", "ude"].concat()
}

/// The working-notes file name, assembled like [`needle`].
fn notes_file() -> String {
    format!("{}.md", needle().to_uppercase())
}

/// The local tool folder, with its trailing slash, assembled like [`needle`].
fn tool_dir() -> String {
    format!(".{}/", needle())
}

fn repo_root() -> PathBuf {
    // native/crates/floret-engine -> the repository root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repository root")
}

fn git(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Tracked files, or `None` when this isn't a git checkout.
fn tracked_files(root: &Path) -> Option<Vec<String>> {
    let out = git(root, &["ls-files", "-z"])?;
    Some(
        out.split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect(),
    )
}

/// The subset of `files` whose `export-ignore` attribute is set, as `git`
/// itself reads `.gitattributes`.
fn export_ignored(root: &Path, files: &[String]) -> Option<BTreeSet<String>> {
    let mut child = Command::new("git")
        .args(["check-attr", "-z", "--stdin", "export-ignore"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let input: Vec<u8> = files
        .iter()
        .flat_map(|f| f.bytes().chain(std::iter::once(0)))
        .collect();
    // Feed from another thread: the answer is larger than a pipe buffer.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let out = child.wait_with_output().ok()?;
    writer.join().ok()?;
    if !out.status.success() {
        return None;
    }
    // path NUL attribute NUL value NUL, repeated.
    let parts: Vec<&[u8]> = out.stdout.split(|b| *b == 0).collect();
    let mut set = BTreeSet::new();
    for t in parts.as_chunks::<3>().0 {
        if t[2] == b"set" {
            set.insert(String::from_utf8_lossy(t[0]).into_owned());
        }
    }
    Some(set)
}

fn skipped(path: &str) -> bool {
    let this = "native/crates/floret-engine/tests/release_hygiene.rs";
    path == this
        || path == "native/Cargo.lock"
        || path.ends_with("/Cargo.lock")
        || path == "Cargo.lock"
        || path.split('/').any(|c| c == "node_modules")
}

#[test]
fn release_has_no_ai_tool_mentions() {
    let root = repo_root();
    let Some(files) = tracked_files(&root) else {
        eprintln!("not a git checkout: skipping the release scan");
        return;
    };
    if files.is_empty() {
        return;
    }
    let ignored = export_ignored(&root, &files).expect("git check-attr");
    let needle = needle();

    let mut offenders = Vec::new();
    let mut scanned = 0;
    for path in &files {
        if ignored.contains(path) || skipped(path) {
            continue;
        }
        if path.to_lowercase().contains(&needle) {
            offenders.push(format!("{path}: the path itself"));
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(path)) else {
            continue; // deleted in the working tree, or a directory entry
        };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue; // binary
        }
        scanned += 1;
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let shown: String = line.trim().chars().take(120).collect();
                offenders.push(format!("{path}:{}: {shown}", i + 1));
            }
        }
    }
    assert!(scanned > 100, "only {scanned} text files scanned");
    assert!(
        offenders.is_empty(),
        "tracked files that ship mention the tool ({} hits). Remove the mention, \
         or mark the file export-ignore in .gitattributes if it must stay in the \
         repository:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn working_notes_are_marked_export_ignore() {
    let root = repo_root();
    let Some(files) = tracked_files(&root) else {
        return;
    };
    let ignored = export_ignored(&root, &files).expect("git check-attr");
    assert!(
        ignored.contains(notes_file().as_str()),
        "the working-notes file must be `export-ignore` in .gitattributes"
    );
    // Anything tracked under the tool folder is ignored as well.
    for f in files.iter().filter(|f| f.starts_with(tool_dir().as_str())) {
        assert!(ignored.contains(f), "{f} must be export-ignore");
    }
    let attrs = std::fs::read_to_string(root.join(".gitattributes")).expect(".gitattributes");
    let has = |pat: &str| {
        attrs.lines().any(|l| {
            let mut w = l.split_whitespace();
            w.next() == Some(pat) && w.any(|a| a == "export-ignore")
        })
    };
    assert!(
        has(&notes_file()),
        "the working-notes line in .gitattributes"
    );
    assert!(has(&tool_dir()), "the tool-folder line in .gitattributes");
}

#[test]
fn source_archive_leaves_out_the_working_notes() {
    let root = repo_root();
    if tracked_files(&root).is_none() {
        return;
    }
    // `git archive HEAD | tar -t`: what a release contains.
    let Ok(mut archive) = Command::new("git")
        .args(["archive", "HEAD"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let tar = Command::new("tar")
        .arg("-t")
        .stdin(archive.stdout.take().expect("archive stdout"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let _ = archive.wait();
    let Ok(tar) = tar else {
        eprintln!("no tar: skipping the archive listing");
        return;
    };
    if !tar.status.success() {
        return; // no commits yet, or tar can't read it
    }
    let listing = String::from_utf8_lossy(&tar.stdout);
    let entries: Vec<&str> = listing.lines().collect();
    if entries.is_empty() {
        return;
    }
    assert!(
        entries.contains(&"LICENSE"),
        "the archive should hold the source"
    );
    let leaked: Vec<&&str> = entries
        .iter()
        .filter(|e| {
            let e = e.trim_end_matches('/');
            e.split('/')
                .any(|c| c == notes_file() || c.eq_ignore_ascii_case(&format!(".{}", needle())))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "a source archive of HEAD still contains {leaked:?}; commit the .gitattributes export-ignore lines"
    );
}

// ---------------------------------------------------------------------------
// Licensing: Floret is free and open source under the Apache License 2.0.
// ---------------------------------------------------------------------------

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}

#[test]
fn licence_is_the_apache_2_text() {
    let root = repo_root();
    let text = read(&root, "LICENSE");
    let head: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(3)
        .collect();
    assert_eq!(
        head,
        [
            "Apache License",
            "Version 2.0, January 2004",
            "http://www.apache.org/licenses/"
        ],
        "LICENSE must start with the standard Apache License 2.0 text"
    );
    for part in [
        "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION",
        "4. Redistribution.",
        "(d) If the Work includes a \"NOTICE\" text file",
        "END OF TERMS AND CONDITIONS",
        "limitations under the License.",
    ] {
        assert!(text.contains(part), "LICENSE is missing `{part}`");
    }
    assert!(
        text.lines().count() > 190,
        "LICENSE looks cut short ({} lines)",
        text.lines().count()
    );
}

#[test]
fn notice_names_the_author() {
    let root = repo_root();
    let notice = read(&root, "NOTICE");
    assert!(
        notice.starts_with("Floret\n"),
        "NOTICE starts with the name"
    );
    assert!(notice.contains("Copyright 2026 Jaafer Elsadig"));
    assert!(notice.contains("https://github.com/jaerasgon/Floret"));
    assert!(notice.contains("keep this"), "NOTICE asks to be kept");
}

/// One `key = value` line of a `[section]` of a Cargo manifest. An inherited
/// `key.workspace = true` reads as `workspace`.
fn manifest_value(manifest: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == format!("[{section}]");
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some(v) = rest.strip_prefix('=') {
            return Some(v.trim().trim_matches('"').to_owned());
        }
        if let Some(v) = rest.strip_prefix(".workspace") {
            if v.trim_start().strip_prefix('=').map(str::trim) == Some("true") {
                return Some("workspace".to_owned());
            }
        }
    }
    None
}

#[test]
fn every_workspace_crate_is_apache_2() {
    let root = repo_root();
    let ws = read(&root, "native/Cargo.toml");
    for key in ["license", "repository", "homepage"] {
        assert!(
            manifest_value(&ws, "workspace.package", key).is_some(),
            "native/Cargo.toml [workspace.package] has no `{key}`"
        );
    }
    assert_eq!(
        manifest_value(&ws, "workspace.package", "license").as_deref(),
        Some("Apache-2.0")
    );
    assert_eq!(
        manifest_value(&ws, "workspace.package", "publish").as_deref(),
        Some("false"),
        "the workspace stays unpublished"
    );

    let mut crates = 0;
    for entry in std::fs::read_dir(root.join("native/crates")).expect("native/crates") {
        let dir = entry.expect("dir entry").path();
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        crates += 1;
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&manifest).expect("manifest");
        // Either inherited from the workspace or spelled out; never anything else.
        let licence = manifest_value(&text, "package", "license");
        assert!(
            matches!(licence.as_deref(), Some("workspace" | "Apache-2.0")),
            "{name}: licence must be Apache-2.0 (found {licence:?})"
        );
        for key in ["repository", "homepage"] {
            assert!(
                manifest_value(&text, "package", key).is_some(),
                "{name}: `{key}` is missing"
            );
        }
    }
    assert!(crates >= 7, "only {crates} crates found");
}

#[test]
fn other_manifests_and_scripts_agree_on_the_licence() {
    let root = repo_root();
    let package = read(&root, "web-legacy/package.json");
    assert!(
        package.contains("\"license\": \"Apache-2.0\""),
        "web-legacy/package.json licence"
    );
    // The app bundle carries the NOTICE next to the LICENSE (the bundle
    // script is shared by `floret.sh install` and the release packaging).
    let script = read(&root, "tools/package/build-macos-app.sh");
    assert!(script.contains("LICENSE") && script.contains("NOTICE"));
}

/// Third-party licence texts quote other projects' copyright lines, so they
/// are not part of this check.
fn third_party_text(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("native/licenses/")
        || path.starts_with("tools/license-texts/")
        || path.starts_with("public/mediapipe/")
        || path.contains("/assets/licenses/")
        || name.starts_with("LICENSE-")
        || name == "Cargo.lock"
        || path.split('/').any(|c| c == "node_modules")
}

#[test]
fn nothing_claims_all_rights_are_reserved() {
    let root = repo_root();
    let Some(files) = tracked_files(&root) else {
        eprintln!("not a git checkout: skipping the licence scan");
        return;
    };
    if files.is_empty() {
        return;
    }
    let needles = [
        ["all rights ", "reserved"].concat(),
        ["propri", "etary"].concat(),
    ];
    let this = "native/crates/floret-engine/tests/release_hygiene.rs";
    let mut offenders = Vec::new();
    let mut scanned = 0;
    for path in &files {
        if path == this || third_party_text(path) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(path)) else {
            continue;
        };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue; // binary
        }
        scanned += 1;
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if needles.iter().any(|n| lower.contains(n.as_str())) {
                let shown: String = line.trim().chars().take(120).collect();
                offenders.push(format!("{path}:{}: {shown}", i + 1));
            }
        }
    }
    assert!(scanned > 100, "only {scanned} text files scanned");
    assert!(
        offenders.is_empty(),
        "Floret is Apache-2.0; these lines say otherwise:\n  {}",
        offenders.join("\n  ")
    );
}
