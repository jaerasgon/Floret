//! Decode generated H.264 clips: frame count, size, colours, seeks, and
//! playback through the provider. On macOS the default backend is
//! AVFoundation, so both decoders get the same checks there.
#![cfg(feature = "openh264")]

use floret_engine::image::Image;
use floret_engine::video::{Seek, VideoRequest, Videos};
use floret_video::clip::{self, Clip};
use floret_video::{open_with, Backend, VideoPlayer};
use std::path::PathBuf;
use std::sync::OnceLock;

const FRAMES: usize = 30;
const FPS: u32 = 30;

/// Frame `i`: a flat colour whose red channel counts frames (8 per frame),
/// with a white square in the top-left corner.
fn paint(i: usize, rgb: &mut [u8]) {
    for (p, px) in rgb.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let (x, y) = (p % 64, p / 64);
        *px = if x < 16 && y < 16 {
            [255, 255, 255]
        } else {
            [(i * 8) as u8, 128, 255 - (i * 8) as u8]
        };
    }
}

/// The frame index painted into `img`, from its red channel.
fn frame_of(img: &Image) -> usize {
    let [r, g, _, a] = img.pixel(40, 30);
    assert_eq!(a, 255);
    assert!((i32::from(g) - 128).abs() <= 6, "green {g}");
    let i = (f64::from(r) / 8.0).round() as usize;
    assert!(
        (i32::from(r) - (i * 8) as i32).abs() <= 3,
        "red {r} isn't a frame colour"
    );
    i
}

fn clip_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let bytes = clip::encode(
            &Clip {
                width: 64,
                height: 48,
                fps: FPS,
                frames: FRAMES,
                keyframe_every: 10,
                max_qp: 24,
            },
            paint,
        )
        .expect("encode");
        assert!(bytes.len() < 64 * 1024, "clip is {} bytes", bytes.len());
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("floret-video-clip.mp4");
        std::fs::write(&path, bytes).unwrap();
        path
    })
}

fn backends() -> Vec<Backend> {
    if cfg!(target_os = "macos") {
        vec![Backend::Portable, Backend::Native]
    } else {
        vec![Backend::Portable]
    }
}

#[test]
fn decodes_every_frame_in_order() {
    for backend in backends() {
        let mut m = open_with(clip_path().to_str().unwrap(), backend).expect("open");
        assert_eq!((m.width, m.height), (64, 48), "{}", m.backend);
        assert_eq!(m.frames.len(), FRAMES);
        assert!((m.frames.duration - 1.0).abs() < 1e-9);
        assert!((m.frames.pts[3] - 0.1).abs() < 1e-9);
        m.decoder.seek(0).unwrap();
        let mut seen = Vec::new();
        while let Some((i, img)) = m.decoder.next(&|_| true).unwrap() {
            let img = img.expect("kept frame");
            assert_eq!((img.width(), img.height()), (64, 48));
            let white = img.pixel(4, 4);
            assert!(white.iter().all(|&c| c >= 250), "{} {white:?}", m.backend);
            assert_eq!(frame_of(&img), i, "{} frame {i}", m.backend);
            seen.push(i);
        }
        assert_eq!(seen, (0..FRAMES).collect::<Vec<_>>(), "{}", m.backend);
    }
}

#[test]
fn seeks_land_on_the_exact_frame() {
    for backend in backends() {
        let mut m = open_with(clip_path().to_str().unwrap(), backend).expect("open");
        for i in [17, 3, 29, 0, 10, 9, 21] {
            let img = m
                .frame(i)
                .unwrap_or_else(|e| panic!("{} frame {i}: {e}", m.backend));
            assert_eq!(frame_of(&img), i, "{}", m.backend);
        }
        // Keyframes every 10 frames.
        assert_eq!(m.frames.key[17], 10);
        assert_eq!(m.frames.key[9], 0);
        assert_eq!(m.frames.index_at(0.5), 15);
    }
}

#[test]
fn data_urls_decode() {
    let bytes = std::fs::read(clip_path()).unwrap();
    let url = format!("data:video/mp4;base64,{}", base64(&bytes));
    let mut m = open_with(&url, Backend::Auto).expect("open");
    assert_eq!(frame_of(&m.frame(12).unwrap()), 12);
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            out.push(if i <= c.len() {
                T[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

fn req(src: &str) -> VideoRequest<'_> {
    VideoRequest {
        source: src,
        play: true,
        looping: true,
        speed: 1.0,
        seek: None,
        dt: 0.0,
        restart: false,
        wait: true,
    }
}

#[test]
fn player_plays_scrubs_and_loops() {
    let player = VideoPlayer::default();
    let src = clip_path().to_str().unwrap().to_owned();
    let mut r = req(&src);
    let f = player.video("n1", &r).unwrap();
    assert!(f.ready);
    assert_eq!(f.duration, 1.0);
    assert_eq!(frame_of(&f.image), 0);

    // Play 1/10 s per tick: frames 3, 6, 9 …
    r.dt = 0.1;
    for k in 1..=5 {
        let f = player.video("n1", &r).unwrap();
        assert!((f.time - 0.1 * f64::from(k)).abs() < 1e-9);
        assert_eq!(frame_of(&f.image), 3 * k as usize);
    }
    // Scrub backwards and forwards (0..1 of the duration).
    for (p, want) in [(0.2, 6), (0.9, 27), (0.0, 0), (0.55, 16)] {
        r.seek = Some(Seek::Position(p));
        let f = player.video("n1", &r).unwrap();
        assert_eq!(frame_of(&f.image), want, "scrub {p}");
    }
    // Unwire the scrub: playback carries on from there, wrapping.
    r.seek = None;
    r.dt = 0.5;
    let f = player.video("n1", &r).unwrap();
    assert!((f.time - 0.05).abs() < 1e-9, "{}", f.time);
    assert_eq!(frame_of(&f.image), 1);
    // Reverse.
    r.speed = -1.0;
    r.dt = 0.1;
    let f = player.video("n1", &r).unwrap();
    assert_eq!(frame_of(&f.image), 28);

    // Another node has its own playhead.
    let f = player.video("n2", &req(&src)).unwrap();
    assert_eq!(frame_of(&f.image), 0);
}

#[test]
fn player_never_blocks_without_wait() {
    let player = VideoPlayer::default();
    let src = clip_path().to_str().unwrap().to_owned();
    let mut r = req(&src);
    r.wait = false;
    let first = player.video("n", &r).unwrap();
    assert_eq!(first.time, 0.0);
    // Frames turn up shortly.
    let t0 = std::time::Instant::now();
    loop {
        let f = player.video("n", &r).unwrap();
        if f.ready {
            assert_eq!(frame_of(&f.image), 0);
            break;
        }
        assert!(t0.elapsed().as_secs() < 10, "no frame");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let err = player.video("m", &req("/no/such/video.mp4")).unwrap_err();
    assert!(err.contains("video.mp4"), "{err}");
}

/// Scrubbing without waiting: each jump lands on the exact frame after at
/// most a GOP's decode (keyframes every 2 s here).
#[test]
fn scrubs_land_quickly() {
    let bytes = clip::encode(
        &Clip {
            width: 320,
            height: 176,
            fps: 30,
            frames: 120,
            keyframe_every: 60,
            max_qp: 30,
        },
        |i, rgb| {
            for px in rgb.as_chunks_mut::<3>().0 {
                *px = [(i % 10 * 25) as u8, (i / 10 * 20) as u8, 64];
            }
        },
    )
    .unwrap();
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("floret-video-scrub.mp4");
    std::fs::write(&path, bytes).unwrap();
    let src = path.to_str().unwrap().to_owned();
    let player = VideoPlayer::default();
    let mut r = req(&src);
    player.video("s", &r).unwrap();
    r.wait = false;
    let frame = |img: &Image| {
        let [r, g, ..] = img.pixel(10, 10).map(f64::from);
        (r / 25.0).round() as usize + 10 * (g / 20.0).round() as usize
    };
    let mut worst = std::time::Duration::ZERO;
    for p in [0.9, 0.1, 0.55, 0.45, 0.99, 0.02, 0.7] {
        r.seek = Some(Seek::Position(p));
        let want = (p * 120.0 + 1e-6).floor() as usize;
        let t0 = std::time::Instant::now();
        while frame(&player.video("s", &r).unwrap().image) != want {
            assert!(t0.elapsed().as_secs() < 5, "scrub to {p} never landed");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        worst = worst.max(t0.elapsed());
    }
    eprintln!("slowest scrub: {worst:?}");
}
