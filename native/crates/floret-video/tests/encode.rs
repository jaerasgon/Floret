//! Export encoders, checked by reading their output back: MP4 frames
//! through the platform's decoder (AVFoundation on macOS, else our own
//! demuxer + OpenH264), GIF frames through the `gif` decoder.

use floret_video::gif_writer::GifWriter;

const W: u32 = 96;
const H: u32 = 64;

/// Frame `i` of `n`: a flat colour whose red channel climbs with `i`, and
/// a white bar whose x position moves with it.
fn frame(i: usize, n: usize) -> Vec<u8> {
    let red = (i * 200 / n.max(1)) as u8;
    let bar = (i * (W as usize - 8) / n.max(1)) as u32;
    let mut px = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let white = x >= bar && x < bar + 8 && y < 16;
            px.extend_from_slice(&if white {
                [255, 255, 255, 255]
            } else {
                [red, 90, 200, 255]
            });
        }
    }
    px
}

#[cfg(any(feature = "openh264", target_os = "macos"))]
fn tmp(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name)
}

#[cfg(any(feature = "openh264", target_os = "macos"))]
#[test]
fn mp4_round_trips_through_the_decoder() {
    use floret_video::export::{H264Settings, Mp4File};
    use floret_video::{open_with, Backend};

    let (n, fps) = (24, 24);
    let path = tmp("floret-export-test.mp4");
    let mut enc = Mp4File::create(&path, H264Settings::export(W, H, fps)).unwrap();
    if cfg!(target_os = "macos") {
        assert_eq!(enc.encoder(), "apple");
    }
    for i in 0..n {
        enc.push_rgba(&frame(i, n)).unwrap();
    }
    assert_eq!(enc.frames(), n);
    enc.finish().unwrap();

    let backend = if cfg!(target_os = "macos") {
        Backend::Native
    } else {
        Backend::Portable
    };
    let mut m = open_with(path.to_str().unwrap(), backend).expect("open");
    assert_eq!((m.width, m.height), (W, H));
    assert_eq!(m.frames.len(), n);
    assert!(
        (m.frames.duration - 1.0).abs() < 1e-9,
        "{}",
        m.frames.duration
    );
    for i in [0, 7, n - 1] {
        let img = m.frame(i).expect("decode");
        assert_eq!((img.width(), img.height()), (W, H));
        let [r, g, b, _] = img.pixel(W - 4, H - 4);
        let want = (i * 200 / n) as i32;
        assert!(
            (i32::from(r) - want).abs() <= 6,
            "frame {i}: red {r} ≠ {want}"
        );
        assert!((i32::from(g) - 90).abs() <= 6 && (i32::from(b) - 200).abs() <= 6);
    }
}

#[test]
fn gif_round_trips_through_the_decoder() {
    let (n, fps) = (15, 30);
    let mut out = Vec::new();
    {
        let mut gif = GifWriter::new(&mut out, W, H, fps).unwrap();
        for i in 0..n {
            gif.push_rgba(&mut frame(i, n)).unwrap();
        }
        assert_eq!(gif.frames(), n);
        gif.finish().unwrap();
    }
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(&out[..]).unwrap();
    assert_eq!((u32::from(dec.width()), u32::from(dec.height())), (W, H));
    let mut frames = 0;
    let mut cs = 0u32;
    while let Some(f) = dec.read_next_frame().unwrap() {
        assert_eq!((u32::from(f.width), u32::from(f.height)), (W, H));
        cs += u32::from(f.delay);
        // The bar's left edge and the background colour come back.
        let px = |x: u32, y: u32| {
            let o = ((y * W + x) * 4) as usize;
            [f.buffer[o], f.buffer[o + 1], f.buffer[o + 2]]
        };
        let bar = (frames * (W as usize - 8) / n) as u32;
        assert!(
            px(bar + 2, 4).iter().all(|&c| c > 240),
            "frame {frames} bar"
        );
        let red = (frames * 200 / n) as i32;
        let [r, g, b] = px(W - 4, H - 4);
        assert!(
            (i32::from(r) - red).abs() <= 8,
            "frame {frames}: red {r} ≠ {red}"
        );
        assert!((i32::from(g) - 90).abs() <= 8 && (i32::from(b) - 200).abs() <= 8);
        frames += 1;
    }
    assert_eq!(frames, n);
    // 15 frames at 30 fps: half a second.
    assert_eq!(cs, 50);
    // It loops forever.
    assert_eq!(dec.repeat(), gif::Repeat::Infinite);
}
