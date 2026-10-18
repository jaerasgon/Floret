//! What ships mentions no AI tooling.
//!
//! The project's working notes (`CLAUDE.md`) and the local tool folder
//! (`.claude/`) stay in the repository but are never part of a release:
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
fn release_has_no_claude_mentions() {
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
        ignored.contains("CLAUDE.md"),
        "CLAUDE.md must be `export-ignore` in .gitattributes"
    );
    // Anything tracked under the tool folder is ignored as well.
    for f in files.iter().filter(|f| f.starts_with(".claude/")) {
        assert!(ignored.contains(f), "{f} must be export-ignore");
    }
    let attrs = std::fs::read_to_string(root.join(".gitattributes")).expect(".gitattributes");
    let has = |pat: &str| {
        attrs.lines().any(|l| {
            let mut w = l.split_whitespace();
            w.next() == Some(pat) && w.any(|a| a == "export-ignore")
        })
    };
    assert!(has("CLAUDE.md"), "CLAUDE.md line in .gitattributes");
    assert!(has(".claude/"), ".claude/ line in .gitattributes");
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
                .any(|c| c == "CLAUDE.md" || c.eq_ignore_ascii_case(&format!(".{}", needle())))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "a source archive of HEAD still contains {leaked:?}; commit the .gitattributes export-ignore lines"
    );
}
