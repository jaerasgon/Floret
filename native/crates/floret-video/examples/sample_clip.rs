//! Writes the little looping clip the `video-pixelate` sample embeds:
//! a drifting plasma with an orbiting disc, 4 s at 24 fps.
//!
//! `cargo run -p floret-video --example sample_clip -- out.mp4`

use floret_video::clip::{self, Clip};
use std::f64::consts::TAU;

const W: u32 = 192;
const H: u32 = 108;
const FRAMES: usize = 96;

fn paint(i: usize, rgb: &mut [u8]) {
    let t = i as f64 / FRAMES as f64;
    let (cx, cy) = (
        f64::from(W) / 2.0 + 52.0 * (TAU * t).cos(),
        f64::from(H) / 2.0 + 28.0 * (TAU * t).sin(),
    );
    for (p, px) in rgb.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let (x, y) = ((p % W as usize) as f64, (p / W as usize) as f64);
        let v = (x * 0.045 + TAU * t).sin()
            + (y * 0.07 - TAU * t).sin()
            + ((x + y) * 0.03 + 2.0 * TAU * t).sin();
        let k = v / 6.0 + 0.5;
        let c = |phase: f64| (127.5 + 127.5 * (TAU * (k + phase)).cos()) as u8;
        *px = [c(0.0), c(0.33), c(0.67)];
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        if d < 16.0 {
            *px = [250, 245, 235];
        }
    }
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample-clip.mp4".into());
    let bytes = clip::encode(
        &Clip {
            width: W,
            height: H,
            fps: 24,
            frames: FRAMES,
            keyframe_every: 24,
            max_qp: 34,
        },
        paint,
    )
    .expect("encode");
    std::fs::write(&out, &bytes).expect("write");
    println!("{out}: {} bytes", bytes.len());
}
