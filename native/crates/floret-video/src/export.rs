//! H.264 MP4 export: frames in, an `.mp4` file out, through the platform's
//! licensed encoder where there is one.
//!
//! - **macOS**: Apple's encoder (AVFoundation / VideoToolbox), which ships
//!   with the OS under Apple's H.264 licence. The app's macOS build leaves
//!   OpenH264 out entirely.
//! - **Elsewhere**: OpenH264 ([`crate::encode`], cargo feature `openh264`).
//!
//! `FLORET_VIDEO=openh264` picks OpenH264 on macOS too, when it's built in.

use std::path::Path;

/// The largest frame exported, either orientation (H.264 level 5.2).
pub const MAX_SIZE: (u32, u32) = (3840, 2160);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct H264Settings {
    /// Frame size; both even (4:2:0 chroma), at most [`MAX_SIZE`].
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// A keyframe every this many frames.
    pub keyframe_every: u32,
    /// Encoder quantiser range, 0 (best) ..= 51 (OpenH264 only).
    pub qp: (u8, u8),
    /// Target bit rate, bits per second.
    pub bitrate: u32,
}

impl H264Settings {
    /// Export quality: a keyframe every two seconds and a generous bit
    /// rate (about 0.2 bits per pixel), for crisp generative graphics.
    pub fn export(width: u32, height: u32, fps: u32) -> H264Settings {
        let px = u64::from(width) * u64::from(height) * u64::from(fps.max(1));
        H264Settings {
            width,
            height,
            fps: fps.max(1),
            keyframe_every: fps.max(1) * 2,
            qp: (4, 36),
            bitrate: (px / 5).clamp(500_000, 80_000_000) as u32,
        }
    }

    /// Why these settings can't be encoded, if they can't.
    pub fn check(&self) -> Result<(), String> {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 || w % 2 == 1 || h % 2 == 1 {
            return Err(format!("H.264 needs an even frame size, not {w} × {h}"));
        }
        if w.max(h) > MAX_SIZE.0 || w.min(h) > MAX_SIZE.1 {
            return Err(format!(
                "{w} × {h} is larger than H.264 allows here ({} × {})",
                MAX_SIZE.0, MAX_SIZE.1
            ));
        }
        Ok(())
    }
}

enum Inner {
    #[cfg(target_os = "macos")]
    Apple(crate::avf_write::Writer),
    #[cfg(feature = "openh264")]
    OpenH264(Box<crate::encode::Mp4Encoder<std::io::BufWriter<std::fs::File>>>),
    /// No encoder in this build.
    #[allow(dead_code)]
    None(std::convert::Infallible),
}

/// An MP4 file being written, frame by frame.
pub struct Mp4File {
    inner: Inner,
}

impl Mp4File {
    /// Start `path` (replacing any file there).
    pub fn create(path: &Path, settings: H264Settings) -> Result<Mp4File, String> {
        settings.check()?;
        let portable = std::env::var("FLORET_VIDEO").as_deref() == Ok("openh264");
        #[cfg(target_os = "macos")]
        if !(portable && cfg!(feature = "openh264")) {
            let w = crate::avf_write::Writer::create(path, settings)?;
            return Ok(Mp4File {
                inner: Inner::Apple(w),
            });
        }
        let _ = portable;
        #[cfg(feature = "openh264")]
        {
            let file = std::fs::File::create(path)
                .map(std::io::BufWriter::new)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let enc = crate::encode::Mp4Encoder::new(file, settings)?;
            Ok(Mp4File {
                inner: Inner::OpenH264(Box::new(enc)),
            })
        }
        #[cfg(not(feature = "openh264"))]
        {
            let _ = path;
            Err("MP4 export isn't available in this build".into())
        }
    }

    /// Which encoder is writing (`"apple"`, `"openh264"`).
    pub fn encoder(&self) -> &'static str {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Apple(_) => "apple",
            #[cfg(feature = "openh264")]
            Inner::OpenH264(_) => "openh264",
            Inner::None(n) => match *n {},
        }
    }

    /// Add a frame of RGBA8 pixels (alpha ignored), row-major.
    pub fn push_rgba(&mut self, rgba: &[u8]) -> Result<(), String> {
        let _ = rgba;
        match &mut self.inner {
            #[cfg(target_os = "macos")]
            Inner::Apple(w) => w.push_rgba(rgba),
            #[cfg(feature = "openh264")]
            Inner::OpenH264(e) => e.push_rgba(rgba),
            Inner::None(n) => match *n {},
        }
    }

    /// Frames added so far.
    pub fn frames(&self) -> usize {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Apple(w) => w.frames(),
            #[cfg(feature = "openh264")]
            Inner::OpenH264(e) => e.frames(),
            Inner::None(n) => match *n {},
        }
    }

    /// Write the index and close the file.
    pub fn finish(self) -> Result<(), String> {
        match self.inner {
            #[cfg(target_os = "macos")]
            Inner::Apple(w) => w.finish(),
            #[cfg(feature = "openh264")]
            Inner::OpenH264(e) => {
                use std::io::Write;
                let mut out = e.finish()?;
                out.flush().map_err(|e| format!("writing the video: {e}"))
            }
            Inner::None(n) => match n {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_sizes() {
        assert!(H264Settings::export(640, 360, 30).check().is_ok());
        assert!(H264Settings::export(2160, 3840, 30).check().is_ok());
        assert!(H264Settings::export(641, 360, 30).check().is_err());
        assert!(H264Settings::export(4096, 2160, 30).check().is_err());
        assert!(H264Settings::export(0, 2, 30).check().is_err());
    }
}
