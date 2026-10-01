//! Floret video playback: video files as `image` values for the
//! `video/*` nodes, behind the engine's
//! [`Videos`](floret_engine::video::Videos) provider.
//!
//! Decoding, chosen to ship without anything for users to install:
//!
//! - **Everywhere**: MP4 / MOV demuxed by `re_mp4` (pure Rust), H.264
//!   decoded by OpenH264, compiled from the source the `openh264` crate
//!   bundles (BSD; cargo feature `openh264`, on by default).
//! - **macOS**: H.264 MP4s go to OpenH264 (frame-exact everywhere);
//!   everything else (HEVC, ProRes, anything QuickTime plays) to
//!   AVFoundation / VideoToolbox, which also catches files the MP4 reader
//!   can't. `FLORET_VIDEO=openh264` never uses AVFoundation;
//!   `FLORET_VIDEO=avfoundation` tries it first for every file.
//!
//! Audio tracks are ignored. Each playing node gets a [`session`]: a
//! decoder thread with a small frame cache around the playhead.

pub mod demux;
mod session;

#[cfg(target_os = "macos")]
mod avf;
#[cfg(feature = "openh264")]
pub mod clip;
#[cfg(feature = "openh264")]
mod h264;

use demux::{Frames, ReadSeek};
use floret_engine::image::Image;
use floret_engine::video::{VideoFrame, VideoRequest, Videos};
use session::Session;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A decoder positioned somewhere in a stream, producing frames in display
/// order.
pub trait Decoder: Send {
    /// Restart so that frame `target` (display index) comes out next or
    /// after a few frames before it.
    fn seek(&mut self, target: usize) -> Result<(), String>;

    /// The next frame: its display index, and its pixels if `keep(index)`.
    /// `None` at the end of the stream (until the next seek).
    fn next(
        &mut self,
        keep: &dyn Fn(usize) -> bool,
    ) -> Result<Option<(usize, Option<Image>)>, String>;
}

/// An open video.
pub struct Media {
    pub frames: Frames,
    pub width: u32,
    pub height: u32,
    /// Which decoder plays it (`"openh264"`, `"avfoundation"`).
    pub backend: &'static str,
    pub decoder: Box<dyn Decoder>,
}

impl Media {
    /// Decode one frame exactly (seek, then run up to it).
    pub fn frame(&mut self, index: usize) -> Result<Image, String> {
        self.decoder.seek(index)?;
        while let Some((i, img)) = self.decoder.next(&|i| i == index)? {
            if i == index {
                return img.ok_or_else(|| "frame not decoded".to_owned());
            }
            if i > index {
                break;
            }
        }
        Err(format!("frame {index} couldn't be decoded"))
    }
}

enum Source {
    Path(PathBuf),
    Memory(Vec<u8>),
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn resolve(source: &str) -> Result<Source, String> {
    let lower = source.get(..8).unwrap_or(source).to_ascii_lowercase();
    if lower.starts_with("data:") {
        floret_engine::image::decode_data_url(source).map(Source::Memory)
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        Err("video URLs aren't supported; pick a file".into())
    } else if let Some(rest) = source.strip_prefix("file://") {
        Ok(Source::Path(PathBuf::from(percent_decode(rest))))
    } else {
        Ok(Source::Path(PathBuf::from(source)))
    }
}

fn reader(src: &Source) -> Result<Box<dyn ReadSeek>, String> {
    Ok(match src {
        Source::Memory(bytes) => Box::new(std::io::Cursor::new(bytes.clone())),
        Source::Path(p) => {
            let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
            Box::new(std::io::BufReader::new(f))
        }
    })
}

/// Which decoders [`open_with`] may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// OpenH264 for H.264 MP4s, else the platform's own (AVFoundation on
    /// macOS).
    Auto,
    /// The platform's own first for every file, then OpenH264.
    Native,
    /// MP4 demux + OpenH264 only.
    Portable,
}

/// Open a video: a file path, `file://` URL or `data:` URL.
pub fn open(source: &str) -> Result<Media, String> {
    let backend = match std::env::var("FLORET_VIDEO").as_deref() {
        Ok("openh264") => Backend::Portable,
        Ok("avfoundation") => Backend::Native,
        _ => Backend::Auto,
    };
    open_with(source, backend)
}

pub fn open_with(source: &str, backend: Backend) -> Result<Media, String> {
    let src = resolve(source)?;
    #[cfg(not(target_os = "macos"))]
    let _ = backend;
    #[cfg(target_os = "macos")]
    let mut native_err = None;
    #[cfg(target_os = "macos")]
    let native_first = match backend {
        Backend::Native => true,
        Backend::Portable => false,
        // H.264 MP4s decode frame-exactly with OpenH264; AVFoundation takes
        // the rest.
        Backend::Auto => {
            !cfg!(feature = "openh264")
                || !matches!(
                    demux::probe(&mut *reader(&src)?).map(|t| t.codec),
                    Ok(demux::Codec::H264(_))
                )
        }
    };
    #[cfg(target_os = "macos")]
    if native_first {
        let path = match &src {
            Source::Path(p) => Some(p.clone()),
            Source::Memory(_) => None,
        };
        match avf::open(path.as_deref(), &mut *reader(&src)?) {
            Ok(m) => return Ok(m),
            Err(e) => native_err = Some(e),
        }
    }
    let mut r = reader(&src)?;
    let track = match demux::probe(&mut *r) {
        Ok(t) => t,
        #[cfg(target_os = "macos")]
        Err(_) if native_err.is_some() => return Err(native_err.unwrap_or_default()),
        Err(e) => return Err(e),
    };
    let (width, height) = if track.rotation.is_multiple_of(180) {
        (track.width, track.height)
    } else {
        (track.height, track.width)
    };
    match track.codec {
        #[cfg(feature = "openh264")]
        demux::Codec::H264(cfg) => Ok(Media {
            decoder: Box::new(h264::H264::new(
                r,
                track.samples,
                track.frames.key.clone(),
                track.rotation,
                cfg,
            )),
            frames: track.frames,
            width,
            height,
            backend: "openh264",
        }),
        codec => {
            let _ = (width, height);
            let name = match codec {
                demux::Codec::H264(_) => "H.264".to_owned(),
                demux::Codec::Other(s) => s,
            };
            #[cfg(target_os = "macos")]
            if let Some(e) = native_err {
                return Err(format!("can't play this {name} video: {e}"));
            }
            Err(format!(
                "can't play {name} video here; re-encode it as H.264 MP4"
            ))
        }
    }
}

/// The app's video provider: one [`Session`] per node and source.
#[derive(Default)]
pub struct VideoPlayer {
    sessions: Mutex<HashMap<(String, u64), Arc<Session>>>,
}

fn source_key(source: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    h.finish()
}

impl Videos for VideoPlayer {
    fn video(&self, node: &str, req: &VideoRequest) -> Result<VideoFrame, String> {
        let session = {
            let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
            sessions.retain(|_, s| !s.stale());
            let key = (node.to_owned(), source_key(req.source));
            sessions
                .entry(key)
                .or_insert_with(|| Arc::new(Session::spawn(req.source)))
                .clone()
        };
        // Outside the lock: a blocking poll mustn't hold up other nodes.
        session.poll(req)
    }
}

/// Create the process's video player and install it as the engine's
/// provider.
pub fn install() -> Arc<VideoPlayer> {
    let player = Arc::new(VideoPlayer::default());
    floret_engine::video::install(Some(player.clone()));
    player
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources() {
        assert!(
            matches!(resolve("data:video/mp4;base64,AAAA"), Ok(Source::Memory(b)) if b == [0, 0, 0])
        );
        assert!(
            matches!(resolve("file:///a%20b.mp4"), Ok(Source::Path(p)) if p == std::path::Path::new("/a b.mp4"))
        );
        assert!(
            matches!(resolve("/x/y.mov"), Ok(Source::Path(p)) if p == std::path::Path::new("/x/y.mov"))
        );
        assert!(resolve("https://example.com/a.mp4").is_err());
        assert!(open("/definitely/not/here.mp4").is_err());
    }
}
