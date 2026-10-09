//! Image sequences played like a video: numbered PNG / JPEG frames (such
//! as Floret's own PNG-sequence export, `name-0001.png …`) at a chosen
//! frame rate.
//!
//! A video node's source names a sequence when it is
//!
//! - a **folder**: every PNG / JPEG in it, in natural order (`2` before
//!   `10`);
//! - one **frame** of a numbered run (`shot_0001.png`): every file beside
//!   it with the same name round a number of the same width (any width
//!   when the numbers aren't zero-padded), in number order; a picture with
//!   no number is a one-frame sequence;
//! - a **pattern**: `shot_####.png` or `shot_%04d.png` (`%d` for any
//!   width);
//! - a **list**, one frame per line: paths or `data:` URLs (how a patch
//!   carries a short sequence inside itself).
//!
//! Frames decode on the session's thread like video frames, into the same
//! bounded cache; only the paths (or the list's bytes) are held otherwise.

use crate::demux::Frames;
use crate::{Decoder, Media};
use floret_engine::image::Image;
use std::path::{Path, PathBuf};

/// Picture files a sequence is made of.
pub const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];
/// Most frames read from a folder or run (it stops there).
pub const MAX_FRAMES: usize = 100_000;

/// One frame's picture.
#[derive(Clone, Debug, PartialEq)]
pub enum FrameSource {
    Path(PathBuf),
    /// A `data:` URL (from a list).
    Data(String),
}

impl FrameSource {
    fn decode(&self) -> Result<Image, String> {
        match self {
            FrameSource::Path(p) => {
                let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                floret_engine::image::decode(&bytes).map_err(|e| format!("{}: {e}", p.display()))
            }
            FrameSource::Data(url) => {
                let bytes = floret_engine::image::decode_data_url(url)?;
                floret_engine::image::decode(&bytes)
            }
        }
    }
}

fn is_picture(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// A path from a source line (`file://` URLs decoded).
fn path_of(s: &str) -> PathBuf {
    match s.strip_prefix("file://") {
        Some(rest) => PathBuf::from(crate::percent_decode(rest)),
        None => PathBuf::from(s),
    }
}

fn is_data(s: &str) -> bool {
    s.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("data:"))
}

/// Whether `source` names an image sequence rather than a video file
/// (decided without touching the disk, except to see if it's a folder).
pub fn is_sequence(source: &str) -> bool {
    let s = source.trim();
    if s.contains('\n') {
        return true;
    }
    if is_data(s) {
        // A picture is a one-frame sequence; anything else a video.
        return s
            .get(..11)
            .is_some_and(|p| p.eq_ignore_ascii_case("data:image/"));
    }
    let p = path_of(s);
    is_picture(&p) || p.is_dir()
}

/// A file name split round its last run of digits: (before, digits, after).
fn split_number(name: &str) -> Option<(&str, &str, &str)> {
    let stem_end = name.rfind('.').unwrap_or(name.len());
    let stem = &name[..stem_end];
    let end = stem.rfind(|c: char| c.is_ascii_digit())? + 1;
    let start = stem[..end]
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    Some((&name[..start], &name[start..end], &name[end..]))
}

/// A name pattern: before, digit width (`None`: any), after.
#[derive(Clone, Debug, PartialEq)]
struct Pattern {
    before: String,
    width: Option<usize>,
    after: String,
}

impl Pattern {
    /// `shot_####.png`, `shot_%04d.png` or `shot_%d.png`.
    fn parse(name: &str) -> Option<Pattern> {
        if let Some(i) = name.find('#') {
            let n = name[i..].chars().take_while(|c| *c == '#').count();
            return Some(Pattern {
                before: name[..i].to_owned(),
                width: Some(n),
                after: name[i + n..].to_owned(),
            });
        }
        let i = name.find('%')?;
        let rest = &name[i + 1..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let tail = rest[digits.len()..].strip_prefix('d')?;
        let width = digits.trim_start_matches('0').parse::<usize>().ok();
        Some(Pattern {
            before: name[..i].to_owned(),
            width: if digits.starts_with('0') { width } else { None },
            after: tail.to_owned(),
        })
    }

    /// The frame number `name` has under this pattern.
    fn number(&self, name: &str) -> Option<u64> {
        let mid = name.strip_prefix(&self.before)?.strip_suffix(&self.after)?;
        let ok = !mid.is_empty()
            && mid.bytes().all(|b| b.is_ascii_digit())
            && self.width.is_none_or(|w| mid.len() == w);
        ok.then(|| mid.parse().ok()).flatten()
    }
}

/// The files in `dir` matching `pat`, by frame number.
fn run(dir: &Path, pat: &Pattern) -> Result<Vec<FrameSource>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut found: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            Some((pat.number(&name)?, e.path()))
        })
        .filter(|(_, p)| is_picture(p))
        .collect();
    found.sort();
    found.truncate(MAX_FRAMES);
    Ok(found
        .into_iter()
        .map(|(_, p)| FrameSource::Path(p))
        .collect())
}

/// Natural order: digit runs compare as numbers.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return a.cmp(b),
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let n = x.iter().take_while(|c| c.is_ascii_digit()).count();
                let m = y.iter().take_while(|c| c.is_ascii_digit()).count();
                let (u, v) = (&x[..n], &y[..m]);
                let strip = |s: &[u8]| -> usize { s.iter().take_while(|c| **c == b'0').count() };
                let (u, v) = (&u[strip(u)..], &v[strip(v)..]);
                let ord = u.len().cmp(&v.len()).then(u.cmp(v));
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                x = &x[n..];
                y = &y[m..];
            }
            (Some(p), Some(q)) => {
                let ord = p.to_ascii_lowercase().cmp(&q.to_ascii_lowercase());
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                x = &x[1..];
                y = &y[1..];
            }
        }
    }
}

/// The frames `source` names, in order (see the module docs).
pub fn frames(source: &str) -> Result<Vec<FrameSource>, String> {
    let s = source.trim();
    if s.contains('\n') || is_data(s) {
        return Ok(s
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(MAX_FRAMES)
            .map(|l| {
                if is_data(l) {
                    FrameSource::Data(l.to_owned())
                } else {
                    FrameSource::Path(path_of(l))
                }
            })
            .collect());
    }
    let path = path_of(s);
    if path.is_dir() {
        let entries = std::fs::read_dir(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_picture(p))
            .collect();
        files.sort_by(|a, b| {
            let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned());
            natural(&name(a).unwrap_or_default(), &name(b).unwrap_or_default())
        });
        files.truncate(MAX_FRAMES);
        return Ok(files.into_iter().map(FrameSource::Path).collect());
    }
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{}: not a file name", path.display()))?;
    if let Some(pat) = Pattern::parse(name) {
        return run(dir, &pat);
    }
    if !path.is_file() {
        return Err(format!("{}: no such file", path.display()));
    }
    match split_number(name) {
        Some((before, digits, after)) => {
            let padded = digits.len() > 1 && digits.starts_with('0');
            let pat = Pattern {
                before: before.to_owned(),
                width: padded.then_some(digits.len()),
                after: after.to_owned(),
            };
            let all = run(dir, &pat)?;
            // A zero-padded run takes only its own width; an unpadded one
            // (1, 2, … 10) any width.
            Ok(if all.is_empty() {
                vec![FrameSource::Path(path)]
            } else {
                all
            })
        }
        None => Ok(vec![FrameSource::Path(path)]),
    }
}

/// Decodes a sequence's frames in order.
struct Sequence {
    frames: Vec<FrameSource>,
    next: usize,
}

impl Decoder for Sequence {
    fn seek(&mut self, target: usize) -> Result<(), String> {
        self.next = target;
        Ok(())
    }

    fn next(
        &mut self,
        keep: &dyn Fn(usize) -> bool,
    ) -> Result<Option<(usize, Option<Image>)>, String> {
        let i = self.next;
        let Some(f) = self.frames.get(i) else {
            return Ok(None);
        };
        self.next += 1;
        if !keep(i) {
            return Ok(Some((i, None)));
        }
        Ok(Some((i, Some(f.decode()?))))
    }
}

/// Frames per second for a sequence: `fps` when it's sensible, else 30.
pub fn rate(fps: f64) -> f64 {
    if fps.is_finite() && fps > 0.0 {
        fps.min(1000.0)
    } else {
        30.0
    }
}

/// Open `source` as a sequence at `fps` frames a second (its first frame
/// decoded for the size).
pub fn open(source: &str, fps: f64) -> Result<Media, String> {
    let frames = frames(source)?;
    let first = frames
        .first()
        .ok_or_else(|| format!("{}: no PNG or JPEG frames", source.trim()))?
        .decode()?;
    let fps = rate(fps);
    let n = frames.len();
    Ok(Media {
        frames: Frames::uniform(n, n as f64 / fps, |_| true),
        width: first.width(),
        height: first.height(),
        backend: "images",
        decoder: Box::new(Sequence { frames, next: 0 }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_split_out_of_names() {
        assert_eq!(
            split_number("loop-0012.png"),
            Some(("loop-", "0012", ".png"))
        );
        assert_eq!(split_number("a7b.jpg"), Some(("a", "7", "b.jpg")));
        assert_eq!(split_number("12.png"), Some(("", "12", ".png")));
        assert_eq!(split_number("still.png"), None);
    }

    #[test]
    fn patterns() {
        let p = Pattern::parse("shot_####.png").unwrap();
        assert_eq!(p.number("shot_0042.png"), Some(42));
        assert_eq!(p.number("shot_042.png"), None);
        assert_eq!(p.number("shot_00a2.png"), None);
        let p = Pattern::parse("f%03d.jpg").unwrap();
        assert_eq!(p.width, Some(3));
        assert_eq!(p.number("f007.jpg"), Some(7));
        let p = Pattern::parse("f%d.jpg").unwrap();
        assert_eq!(p.width, None);
        assert_eq!(p.number("f1234.jpg"), Some(1234));
        assert!(Pattern::parse("plain.png").is_none());
        assert!(Pattern::parse("100%.png").is_none());
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["f10.png", "f2.png", "F1.png", "f02b.png", "f02a.png"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["F1.png", "f2.png", "f02a.png", "f02b.png", "f10.png"]);
    }

    #[test]
    fn what_counts_as_a_sequence() {
        assert!(is_sequence("/x/frame-0001.png"));
        assert!(is_sequence("/x/frame-0001.JPG"));
        assert!(is_sequence("a.png\nb.png"));
        assert!(is_sequence(std::env::temp_dir().to_str().unwrap()));
        assert!(!is_sequence("/x/clip.mp4"));
        assert!(is_sequence("data:image/png;base64,AAAA"));
        assert!(!is_sequence("data:video/mp4;base64,AAAA"));
    }

    #[test]
    fn rates() {
        assert_eq!(rate(24.0), 24.0);
        assert_eq!(rate(0.0), 30.0);
        assert_eq!(rate(f64::NAN), 30.0);
        assert_eq!(rate(-5.0), 30.0);
    }
}
