//! Image sequences through the video provider: numbered runs named the
//! way Floret's PNG-sequence export names them, folders, patterns, lists
//! of data URLs and JPEG frames; playback, scrubbing, Frame At, looping
//! and the `flipbook-sequence` sample end to end.

use floret_engine::image::Image;
use floret_engine::video::{Seek, VideoRequest, Videos};
use floret_engine::{Engine, FrameCtx, Value};
use floret_video::sequence;
use floret_video::VideoPlayer;
use std::path::{Path, PathBuf};

const W: u32 = 32;
const H: u32 = 24;

/// Frame `i`: red counts frames (16 per frame), green 128.
fn frame(i: usize) -> image::RgbaImage {
    image::RgbaImage::from_pixel(W, H, image::Rgba([(i * 16) as u8, 128, 40, 255]))
}

fn frame_of(img: &Image) -> usize {
    let [r, g, _, _] = img.pixel(5, 5);
    assert!((i32::from(g) - 128).abs() <= 4, "green {g}");
    (f64::from(r) / 16.0).round() as usize
}

/// A fresh folder under the test tmp dir.
fn dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("seq-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `n` frames named like the PNG-sequence export (`stem-0001.png`, …).
fn export_run(d: &Path, stem: &str, n: usize) {
    for i in 0..n {
        frame(i)
            .save(d.join(format!("{stem}-{:04}.png", i + 1)))
            .unwrap();
    }
}

fn req(src: &str, fps: f64) -> VideoRequest<'_> {
    VideoRequest {
        source: src,
        play: true,
        looping: true,
        speed: 1.0,
        seek: None,
        dt: 0.0,
        restart: false,
        wait: true,
        fps,
    }
}

#[test]
fn a_picked_frame_plays_its_whole_run() {
    let d = dir("run");
    export_run(&d, "loop", 12);
    // Neighbours that aren't part of the run.
    frame(9).save(d.join("loop-01.png")).unwrap();
    frame(9).save(d.join("other-0001.png")).unwrap();
    std::fs::write(d.join("loop-0013.txt"), "not a frame").unwrap();
    let pick = d.join("loop-0005.png");
    let src = pick.to_str().unwrap().to_owned();
    assert!(sequence::is_sequence(&src));
    let frames = sequence::frames(&src).unwrap();
    assert_eq!(frames.len(), 12);
    assert_eq!(
        frames[0],
        sequence::FrameSource::Path(d.join("loop-0001.png"))
    );

    let player = VideoPlayer::default();
    let mut r = req(&src, 6.0);
    let f = player.video("n1", &r).unwrap();
    assert!(f.ready);
    assert_eq!((f.image.width(), f.image.height()), (W, H));
    assert!((f.duration - 2.0).abs() < 1e-9, "{}", f.duration);
    assert_eq!(frame_of(&f.image), 0, "starts at the start of the run");
    // Half a second a tick at 6 fps: three frames on each time.
    r.dt = 0.5;
    for k in 1..=3 {
        let f = player.video("n1", &r).unwrap();
        assert_eq!(frame_of(&f.image), 3 * k);
    }
    // Wraps round.
    let f = player.video("n1", &r).unwrap();
    assert_eq!(frame_of(&f.image), 0);
    // Scrub (0..1) and reverse.
    r.seek = Some(Seek::Position(0.75));
    assert_eq!(frame_of(&player.video("n1", &r).unwrap().image), 9);
    r.seek = None;
    r.speed = -1.0;
    r.dt = 1.0 / 6.0;
    assert_eq!(frame_of(&player.video("n1", &r).unwrap().image), 8);
    // Frame At: seconds at the sequence's rate.
    let mut at = req(&src, 6.0);
    at.play = false;
    at.seek = Some(Seek::Seconds(1.5));
    assert_eq!(frame_of(&player.video("n2", &at).unwrap().image), 9);
    // Another rate re-times it.
    let f = player.video("n3", &req(&src, 12.0)).unwrap();
    assert!((f.duration - 1.0).abs() < 1e-9);
}

#[test]
fn folders_patterns_and_unpadded_runs() {
    let d = dir("folder");
    for i in [1usize, 2, 10, 3] {
        frame(i).save(d.join(format!("f{i}.png"))).unwrap();
    }
    let player = VideoPlayer::default();
    let src = d.to_str().unwrap().to_owned();
    let f = player.video("a", &req(&src, 4.0)).unwrap();
    assert!((f.duration - 1.0).abs() < 1e-9);
    // Natural order: f1, f2, f3, f10.
    let mut r = req(&src, 4.0);
    r.seek = Some(Seek::Seconds(0.75));
    assert_eq!(frame_of(&player.video("a", &r).unwrap().image), 10);
    // One frame of an unpadded run finds the others too.
    let one = d.join("f2.png");
    assert_eq!(sequence::frames(one.to_str().unwrap()).unwrap().len(), 4);
    // Patterns: `####` and `%04d`.
    let e = dir("pattern");
    export_run(&e, "shot", 5);
    for pat in ["shot-####.png", "shot-%04d.png", "shot-%d.png"] {
        let p = e.join(pat);
        let frames = sequence::frames(p.to_str().unwrap()).unwrap();
        assert_eq!(frames.len(), 5, "{pat}");
    }
    // No such frames: an error the node shows.
    let none = e.join("nothing-####.png");
    let err = player
        .video("b", &req(none.to_str().unwrap(), 30.0))
        .unwrap_err();
    assert!(err.contains("no PNG or JPEG frames"), "{err}");
    let missing = e.join("gone-0001.png");
    assert!(player
        .video("c", &req(missing.to_str().unwrap(), 30.0))
        .is_err());
}

#[test]
fn lists_of_data_urls_and_jpeg_frames() {
    let mut lines = Vec::new();
    for i in 0..3 {
        let mut png = Vec::new();
        frame(i * 2)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        lines.push(format!("data:image/png;base64,{}", b64(&png)));
    }
    let src = lines.join("\n");
    let player = VideoPlayer::default();
    let mut r = req(&src, 3.0);
    let f = player.video("l", &r).unwrap();
    assert!((f.duration - 1.0).abs() < 1e-9);
    r.seek = Some(Seek::Seconds(0.7));
    assert_eq!(frame_of(&player.video("l", &r).unwrap().image), 4);
    // One picture alone is a one-frame sequence.
    let f = player.video("m", &req(&lines[1], 30.0)).unwrap();
    assert_eq!(frame_of(&f.image), 2);
    // JPEG frames.
    let d = dir("jpeg");
    for i in 0..4 {
        image::DynamicImage::ImageRgba8(frame(i))
            .to_rgb8()
            .save(d.join(format!("j_{i:03}.jpg")))
            .unwrap();
    }
    let pick = d.join("j_002.jpg");
    let mut r = req(pick.to_str().unwrap(), 4.0);
    r.seek = Some(Seek::Seconds(0.5));
    assert_eq!(frame_of(&player.video("j", &r).unwrap().image), 2);
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn flipbook_sample_plays() {
    floret_video::install();
    let _wait = floret_engine::video::wait_for_frames();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/flipbook-sequence.flora");
    let (mut engine, warnings) =
        Engine::load_with_warnings(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let video = engine.graph.idx("fb_video").unwrap();
    let mut ids = Vec::new();
    for n in 0..4 {
        let r = engine.tick(&FrameCtx {
            t: f64::from(n) / 12.0,
            dt: 1.0 / 12.0,
            frame_count: f64::from(n),
            width: 480.0,
            height: 420.0,
            ..FrameCtx::default()
        });
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let out = r.outputs[video].as_ref().unwrap();
        let Some(Value::Image(img)) = out[0].first() else {
            panic!("no frame")
        };
        assert_eq!((img.width(), img.height()), (120, 120));
        assert!(matches!(out[2].first(), Some(Value::Num(d)) if (d - 16.0 / 12.0).abs() < 1e-9));
        assert!(matches!(out[5].first(), Some(Value::Bool(true))));
        ids.push(img.id());
    }
    // A new frame every tick at 12 fps.
    ids.dedup();
    assert_eq!(ids.len(), 4);
}
