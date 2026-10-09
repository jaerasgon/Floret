//! H.264 MP4 export: frames in, an `.mp4` file out, through the platform's
//! licensed encoder where there is one.
//!
//! - **macOS**: Apple's encoder (AVFoundation / VideoToolbox), which ships
//!   with the OS under Apple's H.264 licence. The app's macOS build leaves
//!   OpenH264 out entirely.
//! - **Elsewhere**: OpenH264 ([`crate::encode`], cargo feature `openh264`).
//!
//! `FLORET_VIDEO=openh264` picks OpenH264 on macOS too, when it's built in.
//!
//! **Sound** ([`AudioSettings`], [`Mp4File::push_audio`]) is AAC through
//! Apple's encoder, so only on macOS for now: there is no AAC encoder we
//! could ship ourselves without royalties. Elsewhere an MP4 asked to carry
//! sound is written without it ([`Mp4File::has_sound`] says so).
//!
//! Sound and pictures share one clock: video frame `i` lasts `1 / fps`
//! from `i / fps`, and the audio played during it is exactly the sample
//! frames [`audio_span`] gives, so the two tracks end together.

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

/// Sound for an MP4: stereo, pushed as interleaved `f32` (L, R, L, R…)
/// at `sample_rate`, encoded as AAC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioSettings {
    pub sample_rate: u32,
    /// Target AAC bit rate, bits per second.
    pub bitrate: u32,
}

impl AudioSettings {
    /// Export quality: 48 kHz stereo AAC at 192 kb/s.
    pub fn export() -> AudioSettings {
        AudioSettings {
            sample_rate: 48_000,
            bitrate: 192_000,
        }
    }
}

/// The stereo sample frames that play during video frame `frame` at `fps`:
/// `start..end` in samples from the start of the file. Consecutive frames'
/// spans tile the timeline exactly, and `n` frames' spans add up to
/// `n · sample_rate / fps` (rounded down), the video's duration in samples.
pub fn audio_span(frame: usize, fps: u32, sample_rate: u32) -> std::ops::Range<u64> {
    let at = |i: u64| i * u64::from(sample_rate) / u64::from(fps.max(1));
    at(frame as u64)..at(frame as u64 + 1)
}

/// Whether MP4s exported here can carry sound (macOS, through Apple's
/// encoder; not when `FLORET_VIDEO=openh264` asks for OpenH264 there).
pub fn sound_supported() -> bool {
    cfg!(target_os = "macos")
        && !(cfg!(feature = "openh264")
            && std::env::var("FLORET_VIDEO").as_deref() == Ok("openh264"))
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
    /// Start `path` (replacing any file there), pictures only.
    pub fn create(path: &Path, settings: H264Settings) -> Result<Mp4File, String> {
        Mp4File::create_with_sound(path, settings, None)
    }

    /// Start `path` with a sound track too, where this platform can encode
    /// one ([`sound_supported`]); elsewhere the sound is dropped and the
    /// file has pictures only ([`Mp4File::has_sound`] is false).
    pub fn create_with_sound(
        path: &Path,
        settings: H264Settings,
        sound: Option<AudioSettings>,
    ) -> Result<Mp4File, String> {
        settings.check()?;
        let portable = std::env::var("FLORET_VIDEO").as_deref() == Ok("openh264");
        #[cfg(target_os = "macos")]
        if !(portable && cfg!(feature = "openh264")) {
            let w = crate::avf_write::Writer::create(path, settings, sound)?;
            return Ok(Mp4File {
                inner: Inner::Apple(w),
            });
        }
        let _ = (portable, sound);
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

    /// Whether this file is getting a sound track.
    pub fn has_sound(&self) -> bool {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Apple(w) => w.has_sound(),
            #[cfg(feature = "openh264")]
            Inner::OpenH264(_) => false,
            Inner::None(n) => match *n {},
        }
    }

    /// Add the sound that plays during the frames pushed so far:
    /// interleaved stereo `f32` in -1..1, continuing where the last call
    /// stopped. Push each frame's [`audio_span`] after the frame. Ignored
    /// when the file has no sound track.
    pub fn push_audio(&mut self, interleaved: &[f32]) -> Result<(), String> {
        let _ = interleaved;
        match &mut self.inner {
            #[cfg(target_os = "macos")]
            Inner::Apple(w) => w.push_audio(interleaved),
            #[cfg(feature = "openh264")]
            Inner::OpenH264(_) => Ok(()),
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

    #[test]
    fn audio_spans_tile_the_video_and_match_its_duration() {
        let rate = AudioSettings::export().sample_rate;
        for fps in [10, 12, 15, 24, 25, 30, 50, 60, 7] {
            let frames = 3 * fps as usize + 1;
            let mut next = 0;
            for i in 0..frames {
                let s = audio_span(i, fps, rate);
                assert_eq!(s.start, next, "{fps} fps, frame {i}");
                assert!(s.end > s.start);
                next = s.end;
            }
            // n frames last n / fps seconds: that many samples, to the
            // sample (whole for every rate the export offers).
            let want = frames as u64 * u64::from(rate) / u64::from(fps);
            assert_eq!(next, want, "{fps} fps");
            if 48_000 % fps == 0 {
                assert_eq!(next * u64::from(fps), frames as u64 * 48_000);
            }
        }
        assert_eq!(audio_span(0, 30, 48_000), 0..1600);
        assert_eq!(audio_span(29, 30, 48_000), 46_400..48_000);
    }

    #[test]
    fn sound_is_mac_only_and_dropped_elsewhere() {
        assert_eq!(sound_supported(), cfg!(target_os = "macos"));
    }

    /// Off macOS an MP4 asked for sound is written with pictures only.
    #[cfg(all(feature = "openh264", not(target_os = "macos")))]
    #[test]
    fn mp4s_elsewhere_drop_the_sound_and_still_write() {
        let path = std::env::temp_dir().join(format!("floret-nosound-{}.mp4", std::process::id()));
        let mut f = Mp4File::create_with_sound(
            &path,
            H264Settings::export(64, 64, 10),
            Some(AudioSettings::export()),
        )
        .unwrap();
        assert!(!f.has_sound());
        for i in 0..3 {
            f.push_rgba(&[128; 64 * 64 * 4]).unwrap();
            let s = audio_span(i, 10, 48_000);
            f.push_audio(&vec![0.25; (s.end - s.start) as usize * 2])
                .unwrap();
        }
        f.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp");
        let _ = std::fs::remove_file(&path);
    }
}
