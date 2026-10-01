//! Tiny H.264 MP4 clips made from painted frames (OpenH264's encoder and a
//! minimal MP4 writer): the decoder tests' fixtures, and the video the
//! `video-pixelate` sample embeds (`examples/sample_clip.rs`).

use openh264::encoder::{Encoder, EncoderConfig, FrameType, IntraFramePeriod};
use openh264::formats::{RgbSliceU8, YUVBuffer};
use openh264::OpenH264API;

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

/// Split an Annex B stream at its start codes.
fn nal_units(stream: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= stream.len() {
        if stream[i..i + 3] == [0, 0, 1] {
            starts.push((i, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &(_, body))| {
            let end = starts.get(k + 1).map_or(stream.len(), |&(s, _)| s);
            // A 4-byte start code leaves a zero on the previous unit.
            let mut nal = &stream[body..end];
            while nal.last() == Some(&0) && k + 1 < starts.len() {
                nal = &nal[..nal.len() - 1];
            }
            nal
        })
        .collect()
}

struct Encoded {
    samples: Vec<Vec<u8>>,
    sync: Vec<bool>,
    sps: Vec<u8>,
    pps: Vec<u8>,
}

/// Encode `clip.frames` frames; `paint(i, rgb)` fills frame `i` (RGB8,
/// row-major).
pub fn encode(clip: &Clip, mut paint: impl FnMut(usize, &mut [u8])) -> Result<Vec<u8>, String> {
    let (w, h) = (clip.width as usize, clip.height as usize);
    let config = EncoderConfig::new()
        .skip_frames(false)
        .max_frame_rate(openh264::encoder::FrameRate::from_hz(clip.fps as f32))
        .intra_frame_period(IntraFramePeriod::from_num_frames(clip.keyframe_every))
        .qp(openh264::encoder::QpRange::new(0, clip.max_qp));
    let mut enc = Encoder::with_api_config(OpenH264API::from_source(), config)
        .map_err(|e| format!("encoder: {e}"))?;
    let mut out = Encoded {
        samples: Vec::new(),
        sync: Vec::new(),
        sps: Vec::new(),
        pps: Vec::new(),
    };
    let mut rgb = vec![0; w * h * 3];
    for i in 0..clip.frames {
        paint(i, &mut rgb);
        let yuv = YUVBuffer::from_rgb8_source(RgbSliceU8::new(&rgb, (w, h)));
        let bits = enc.encode(&yuv).map_err(|e| format!("encode: {e}"))?;
        let idr = matches!(bits.frame_type(), FrameType::IDR);
        let stream = bits.to_vec();
        let mut sample = Vec::new();
        for nal in nal_units(&stream) {
            match nal.first().map(|b| b & 0x1f) {
                Some(7) => out.sps = nal.to_vec(),
                Some(8) => out.pps = nal.to_vec(),
                Some(_) => {
                    sample.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                    sample.extend_from_slice(nal);
                }
                None => {}
            }
        }
        if sample.is_empty() {
            return Err(format!("frame {i} encoded to nothing"));
        }
        out.samples.push(sample);
        out.sync.push(idr);
    }
    Ok(mp4(clip, &out))
}

// ─── MP4 writing ─────────────────────────────────────────────────────────────

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// A full box: version 0 and `flags`.
fn full(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = flags.to_be_bytes().to_vec();
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn u32s(vals: &[u32]) -> Vec<u8> {
    vals.iter().flat_map(|v| v.to_be_bytes()).collect()
}

const MATRIX: [u32; 9] = [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000];

fn mp4(clip: &Clip, enc: &Encoded) -> Vec<u8> {
    let timescale = clip.fps * 1000;
    let delta = 1000;
    let n = enc.samples.len() as u32;
    let duration = n * delta;
    let duration_ms = n * 1000 / clip.fps.max(1);

    let ftyp = bx(b"ftyp", b"isom\0\0\x02\0isomiso2avc1mp41");
    let payload: Vec<u8> = enc.samples.concat();
    let mdat = bx(b"mdat", &payload);
    let data_offset = (ftyp.len() + 8) as u32;

    let mut mvhd = u32s(&[0, 0, 1000, duration_ms, 0x10000]);
    mvhd.extend_from_slice(&[1, 0, 0, 0]); // volume 1.0, reserved
    mvhd.extend_from_slice(&[0; 8]);
    mvhd.extend(u32s(&MATRIX));
    mvhd.extend_from_slice(&[0; 24]);
    mvhd.extend(u32s(&[2])); // next track id
    let mvhd = full(b"mvhd", 0, &mvhd);

    let mut tkhd = u32s(&[0, 0, 1, 0, duration_ms, 0, 0]);
    tkhd.extend_from_slice(&[0; 8]); // layer, group, volume, reserved
    tkhd.extend(u32s(&MATRIX));
    tkhd.extend(u32s(&[clip.width << 16, clip.height << 16]));
    let tkhd = full(b"tkhd", 3, &tkhd);

    let mut mdhd = u32s(&[0, 0, timescale, duration]);
    mdhd.extend_from_slice(&[0x55, 0xc4, 0, 0]); // "und"
    let mdhd = full(b"mdhd", 0, &mdhd);
    let mut hdlr = u32s(&[0]);
    hdlr.extend_from_slice(b"vide");
    hdlr.extend_from_slice(&[0; 12]);
    hdlr.extend_from_slice(b"VideoHandler\0");
    let hdlr = full(b"hdlr", 0, &hdlr);

    let vmhd = full(b"vmhd", 1, &[0; 8]);
    let dref = full(b"dref", 0, &[u32s(&[1]), full(b"url ", 1, &[])].concat());
    let dinf = bx(b"dinf", &dref);

    let mut avcc = vec![1, enc.sps[1], enc.sps[2], enc.sps[3], 0xff, 0xe1];
    avcc.extend_from_slice(&(enc.sps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(&enc.sps);
    avcc.push(1);
    avcc.extend_from_slice(&(enc.pps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(&enc.pps);
    let mut avc1 = vec![0; 6];
    avc1.extend_from_slice(&[0, 1]); // data reference index
    avc1.extend_from_slice(&[0; 16]);
    avc1.extend_from_slice(&(clip.width as u16).to_be_bytes());
    avc1.extend_from_slice(&(clip.height as u16).to_be_bytes());
    avc1.extend(u32s(&[0x48_0000, 0x48_0000, 0]));
    avc1.extend_from_slice(&[0, 1]); // frame count
    avc1.extend_from_slice(&[0; 32]); // compressor name
    avc1.extend_from_slice(&[0, 0x18, 0xff, 0xff]); // depth, pre-defined
    avc1.extend(bx(b"avcC", &avcc));
    let stsd = full(b"stsd", 0, &[u32s(&[1]), bx(b"avc1", &avc1)].concat());

    let stts = full(b"stts", 0, &u32s(&[1, n, delta]));
    let keys: Vec<u32> = (1..=n).filter(|&i| enc.sync[i as usize - 1]).collect();
    let stss = full(
        b"stss",
        0,
        &[u32s(&[keys.len() as u32]), u32s(&keys)].concat(),
    );
    let stsc = full(b"stsc", 0, &u32s(&[1, 1, n, 1]));
    let sizes: Vec<u32> = enc.samples.iter().map(|s| s.len() as u32).collect();
    let stsz = full(b"stsz", 0, &[u32s(&[0, n]), u32s(&sizes)].concat());
    let stco = full(b"stco", 0, &u32s(&[1, data_offset]));
    let stbl = bx(b"stbl", &[stsd, stts, stss, stsc, stsz, stco].concat());

    let minf = bx(b"minf", &[vmhd, dinf, stbl].concat());
    let mdia = bx(b"mdia", &[mdhd, hdlr, minf].concat());
    let trak = bx(b"trak", &[tkhd, mdia].concat());
    let moov = bx(b"moov", &[mvhd, trak].concat());
    [ftyp, mdat, moov].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_annex_b() {
        let s = [
            0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 4, 0,
        ];
        let units = nal_units(&s);
        assert_eq!(units, vec![&[0x67, 1, 2][..], &[0x68, 3], &[0x65, 4, 0]]);
    }
}
