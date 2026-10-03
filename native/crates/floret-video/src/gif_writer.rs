//! Animated GIF writing with the `gif` crate (MIT / Apache-2.0): each
//! frame gets its own 256-colour palette (NeuQuant), and the animation
//! loops forever.
//!
//! GIF delays are whole centiseconds, so frame times are rounded so the
//! running total stays exact: 30 fps plays as 3, 3, 4, 3, 3, 4 … cs and a
//! loop of N frames lasts exactly N / fps.

use std::io::Write;

/// The fastest frame rate GIFs play reliably at (browsers slow down
/// delays under 2 cs).
pub const MAX_FPS: u32 = 50;
/// NeuQuant sampling: 1 (best, slowest) ..= 30.
const SPEED: i32 = 10;

pub struct GifWriter<W: Write> {
    enc: gif::Encoder<W>,
    width: u16,
    height: u16,
    fps: u32,
    frames: u32,
}

/// Centiseconds frame `i` is shown at `fps`.
pub fn delay_cs(i: u32, fps: u32) -> u16 {
    let at = |i: u32| (u64::from(i) * 100 + u64::from(fps) / 2) / u64::from(fps.max(1));
    (at(i + 1) - at(i)).min(u64::from(u16::MAX)) as u16
}

impl<W: Write> GifWriter<W> {
    pub fn new(out: W, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        let (Ok(w), Ok(h)) = (u16::try_from(width), u16::try_from(height)) else {
            return Err(format!("{width} × {height} is too large for a GIF"));
        };
        if w == 0 || h == 0 {
            return Err("a GIF needs a frame size".into());
        }
        if fps == 0 || fps > MAX_FPS {
            return Err(format!("GIFs play at up to {MAX_FPS} fps, not {fps}"));
        }
        let mut enc = gif::Encoder::new(out, w, h, &[]).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        Ok(GifWriter {
            enc,
            width: w,
            height: h,
            fps,
            frames: 0,
        })
    }

    /// Add a frame of RGBA8 pixels (opaque), row-major. The buffer is used
    /// as scratch.
    pub fn push_rgba(&mut self, rgba: &mut [u8]) -> Result<(), String> {
        let (w, h) = (usize::from(self.width), usize::from(self.height));
        if rgba.len() != w * h * 4 {
            return Err(format!("frame is {} bytes, not {w} × {h} RGBA", rgba.len()));
        }
        let mut frame = gif::Frame::from_rgba_speed(self.width, self.height, rgba, SPEED);
        frame.delay = delay_cs(self.frames, self.fps);
        self.enc
            .write_frame(&frame)
            .map_err(|e| format!("writing the GIF: {e}"))?;
        self.frames += 1;
        Ok(())
    }

    /// Frames added so far.
    pub fn frames(&self) -> usize {
        self.frames as usize
    }

    /// Write the trailer. Returns the output.
    pub fn finish(self) -> Result<W, String> {
        self.enc
            .into_inner()
            .map_err(|e| format!("writing the GIF: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_add_up_exactly() {
        for fps in [10, 24, 25, 30, 50] {
            let total: u32 = (0..fps).map(|i| u32::from(delay_cs(i, fps))).sum();
            assert_eq!(total, 100, "{fps} fps");
        }
        assert_eq!(delay_cs(0, 25), 4);
        assert_eq!(
            (0..3).map(|i| delay_cs(i, 30)).collect::<Vec<_>>(),
            [3, 4, 3]
        );
    }

    #[test]
    fn rejects_bad_sizes_and_rates() {
        let new = |w, h, fps| GifWriter::new(Vec::new(), w, h, fps).map(|_| ());
        assert!(new(0, 10, 30).is_err());
        assert!(new(70_000, 10, 30).is_err());
        assert!(new(10, 10, 60).is_err());
        assert!(new(10, 10, 30).is_ok());
    }
}
