//! Run a tracker on a still image and print the landmarks as JSON.
//!
//! `cargo run --release -p floret-input --example track_image -- hands|face|pose photo.jpg [count]`

use floret_input::tracking::geom::Frame;
use floret_input::tracking::landmarker::{Landmarker, Tracker as Kind};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(kind), Some(path)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: track_image hands|face|pose <image> [count]");
        std::process::exit(2);
    };
    let kind = match kind.as_str() {
        "hands" => Kind::Hands,
        "face" => Kind::Face,
        "pose" => Kind::Pose,
        other => panic!("unknown tracker {other}"),
    };
    let count = args.get(3).and_then(|c| c.parse().ok()).unwrap_or(1);
    let img = image::open(path).expect("image").to_rgba8();
    let frame = Frame {
        width: img.width(),
        height: img.height(),
        rgba: img.as_raw(),
    };
    let t0 = Instant::now();
    let mut lm = Landmarker::new(kind, count).expect("models");
    let loaded = t0.elapsed();
    let t1 = Instant::now();
    let mut dets = lm.process(&frame, 0.0);
    let first = t1.elapsed();
    // Later frames take the tracking path (ROI from the landmarks).
    let frames: u32 = args.get(4).and_then(|c| c.parse().ok()).unwrap_or(1);
    let t2 = Instant::now();
    for i in 1..frames {
        dets = lm.process(&frame, f64::from(i) / 30.0);
    }
    eprintln!(
        "load {loaded:?}, detect+landmarks {first:?}, tracked {:?}/frame; {} found",
        t2.elapsed() / frames.saturating_sub(1).max(1),
        dets.len(),
    );
    let json: Vec<String> = dets
        .iter()
        .map(|d| {
            let pts: Vec<String> = d
                .iter()
                .map(|p| format!("[{},{},{}]", p[0], p[1], p[2]))
                .collect();
            format!("[{}]", pts.join(","))
        })
        .collect();
    println!("[{}]", json.join(","));
}
