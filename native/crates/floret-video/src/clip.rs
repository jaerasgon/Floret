//! Tiny H.264 MP4 clips made from painted frames (see [`crate::encode`]):
//! the decoder tests' fixtures, and the video the `video-pixelate` sample
//! embeds (`examples/sample_clip.rs`).

use crate::encode::{H264Settings, Mp4Encoder};

pub struct Clip {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: usize,
    /// A keyframe every this many frames.
    pub keyframe_every: u32,
    /// Encoder quality, 0 (best) ..= 51.
    pub max_qp: u8,
}

/// Encode `clip.frames` frames; `paint(i, rgb)` fills frame `i` (RGB8,
/// row-major).
pub fn encode(clip: &Clip, mut paint: impl FnMut(usize, &mut [u8])) -> Result<Vec<u8>, String> {
    let (w, h) = (clip.width as usize, clip.height as usize);
    let settings = H264Settings {
        width: clip.width,
        height: clip.height,
        fps: clip.fps,
        keyframe_every: clip.keyframe_every,
        qp: (0, clip.max_qp),
        // OpenH264's default target.
        bitrate: 120_000,
    };
    let mut enc = Mp4Encoder::new(std::io::Cursor::new(Vec::new()), settings)?;
    let mut rgb = vec![0; w * h * 3];
    for i in 0..clip.frames {
        paint(i, &mut rgb);
        enc.push_rgb(&rgb)?;
    }
    Ok(enc.finish()?.into_inner())
}
