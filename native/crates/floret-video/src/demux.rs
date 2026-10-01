//! MP4 / MOV demuxing (`re_mp4`): the video track's samples, its frame
//! timing in display order, and the H.264 decoder configuration.

use floret_engine::image::Image;
use re_mp4::{Mp4, StsdBoxContent, TrackKind};
use std::io::{Read, Seek, SeekFrom};

/// Where a source's bytes come from: a file, or memory (`data:` URLs).
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// One sample (an encoded frame), in decode order.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub offset: u64,
    pub size: u32,
    /// Its display-order index in [`Frames`].
    pub display: usize,
}

/// Every frame of a track in display order.
#[derive(Clone, Debug, PartialEq)]
pub struct Frames {
    /// Presentation time of each frame, seconds, ascending.
    pub pts: Vec<f64>,
    /// For each frame, the frame a decoder has to start from to reach it
    /// (the latest keyframe at or before it).
    pub key: Vec<usize>,
    pub duration: f64,
}

impl Frames {
    pub fn len(&self) -> usize {
        self.pts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pts.is_empty()
    }

    /// The frame on screen at `t` seconds: the last one starting at or
    /// before it.
    pub fn index_at(&self, t: f64) -> usize {
        self.pts
            .partition_point(|&p| p <= t + 1e-6)
            .saturating_sub(1)
            .min(self.len().saturating_sub(1))
    }

    /// Frames at a constant rate (`count` frames over `duration`).
    pub fn uniform(count: usize, duration: f64, keys: impl Fn(usize) -> bool) -> Frames {
        let count = count.max(1);
        let mut key = Vec::with_capacity(count);
        for i in 0..count {
            key.push(if keys(i) || i == 0 { i } else { key[i - 1] });
        }
        Frames {
            pts: (0..count)
                .map(|i| i as f64 * duration / count as f64)
                .collect(),
            key,
            duration,
        }
    }

    pub fn fps(&self) -> f64 {
        if self.duration > 0.0 {
            self.len() as f64 / self.duration
        } else {
            30.0
        }
    }
}

/// H.264 stream configuration (`avcC`).
#[derive(Clone, Debug, Default)]
pub struct AvcConfig {
    pub length_size: usize,
    pub sps: Vec<Vec<u8>>,
    pub pps: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub enum Codec {
    H264(AvcConfig),
    /// Anything else, by its codec string (`hvc1…`, `av01…`, a fourcc).
    Other(String),
}

pub struct Track {
    /// Coded size (before `rotation`).
    pub width: u32,
    pub height: u32,
    /// Clockwise display rotation from the track matrix (phone videos).
    pub rotation: u16,
    pub codec: Codec,
    /// Decode order.
    pub samples: Vec<Sample>,
    pub frames: Frames,
}

/// Parse a file's moov (sample tables only; `mdat` is skipped, not read)
/// and pick its first video track.
pub fn probe(reader: &mut dyn ReadSeek) -> Result<Track, String> {
    let size = reader.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    reader.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    // re_mp4 indexes its tables without bounds checks; a malformed file
    // must not take the decoder thread down.
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Mp4::read(&mut *reader, size)
    }))
    .map_err(|_| "unreadable video file".to_owned())?
    .map_err(|e| format!("not an MP4 / MOV file ({e})"))?;
    let track = parsed
        .tracks()
        .values()
        .find(|t| t.kind == Some(TrackKind::Video) && !t.samples.is_empty())
        .ok_or("the file has no video track")?;

    let trak = track.trak(&parsed);
    let m = &trak.tkhd.matrix;
    let rotation = ((f64::from(m.b)
        .atan2(f64::from(m.a))
        .to_degrees()
        .rem_euclid(360.0)
        / 90.0)
        .round() as u16
        % 4)
        * 90;
    let contents = &trak.mdia.minf.stbl.stsd.contents;
    let codec = match contents {
        StsdBoxContent::Avc1(avc1) => {
            let c = &avc1.avcc.contents;
            Codec::H264(AvcConfig {
                length_size: usize::from(c.length_size_minus_one) + 1,
                sps: c
                    .sequence_parameter_sets
                    .iter()
                    .map(|n| n.bytes.clone())
                    .collect(),
                pps: c
                    .picture_parameter_sets
                    .iter()
                    .map(|n| n.bytes.clone())
                    .collect(),
            })
        }
        other => Codec::Other(
            other
                .codec_string()
                .unwrap_or_else(|| "an unknown codec".into()),
        ),
    };

    let timescale = track.timescale.max(1) as f64;
    // Display order: by composition time (B-frames decode out of order).
    let mut order: Vec<usize> = (0..track.samples.len()).collect();
    order.sort_by_key(|&i| (track.samples[i].composition_timestamp, i));
    let mut display = vec![0; order.len()];
    for (d, &s) in order.iter().enumerate() {
        display[s] = d;
    }
    let pts: Vec<f64> = order
        .iter()
        .map(|&s| track.samples[s].composition_timestamp as f64 / timescale)
        .collect();
    let last = &track.samples[*order.last().expect("non-empty")];
    let end = (last.composition_timestamp as f64 + last.duration as f64) / timescale;
    let duration = end.max(track.duration as f64 / timescale);

    // A frame's keyframe: the latest sync sample, in decode order, whose
    // display time is at or before the frame's. Closed GOPs (what encoders
    // write by default) decode correctly from there.
    let syncs: Vec<usize> = (0..track.samples.len())
        .filter(|&s| track.samples[s].is_sync)
        .collect();
    let key = (0..pts.len())
        .map(|d| {
            let n = syncs.partition_point(|&s| display[s] <= d);
            n.checked_sub(1).map_or(0, |i| display[syncs[i]])
        })
        .collect();

    let samples = track
        .samples
        .iter()
        .zip(&display)
        .map(|(s, &d)| Sample {
            offset: s.offset,
            size: s.size as u32,
            display: d,
        })
        .collect();
    Ok(Track {
        width: u32::from(track.width),
        height: u32::from(track.height),
        rotation,
        codec,
        samples,
        frames: Frames { pts, key, duration },
    })
}

/// An image from `w × h` RGBA pixels turned `deg` (0 / 90 / 180 / 270)
/// clockwise.
pub fn rotated(w: u32, h: u32, rgba: Vec<u8>, deg: u16) -> Image {
    let (wu, hu) = (w as usize, h as usize);
    let px = |x: usize, y: usize| &rgba[(y * wu + x) * 4..(y * wu + x) * 4 + 4];
    let mut out = Vec::with_capacity(rgba.len());
    match deg {
        90 => {
            for y in 0..wu {
                for x in 0..hu {
                    out.extend_from_slice(px(y, hu - 1 - x));
                }
            }
            Image::new(h, w, out)
        }
        180 => {
            for y in (0..hu).rev() {
                for x in (0..wu).rev() {
                    out.extend_from_slice(px(x, y));
                }
            }
            Image::new(w, h, out)
        }
        270 => {
            for y in 0..wu {
                for x in 0..hu {
                    out.extend_from_slice(px(wu - 1 - y, x));
                }
            }
            Image::new(h, w, out)
        }
        _ => Image::new(w, h, rgba),
    }
}

/// Read one sample's bytes into `buf`.
pub fn read_sample(reader: &mut dyn ReadSeek, s: &Sample, buf: &mut Vec<u8>) -> Result<(), String> {
    buf.resize(s.size as usize, 0);
    reader
        .seek(SeekFrom::Start(s.offset))
        .and_then(|_| reader.read_exact(buf))
        .map_err(|e| format!("reading the video: {e}"))
}

/// Length-prefixed NAL units (MP4) → Annex B start codes (what OpenH264
/// reads).
pub fn to_annex_b(sample: &[u8], length_size: usize, out: &mut Vec<u8>) -> Result<(), String> {
    let mut rest = sample;
    while !rest.is_empty() {
        if rest.len() < length_size {
            return Err("truncated NAL unit".into());
        }
        let len = rest[..length_size]
            .iter()
            .fold(0usize, |acc, &b| (acc << 8) | usize::from(b));
        rest = &rest[length_size..];
        if len > rest.len() {
            return Err("truncated NAL unit".into());
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&rest[..len]);
        rest = &rest[len..];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_at_picks_the_frame_on_screen() {
        let f = Frames::uniform(4, 2.0, |i| i % 2 == 0);
        assert_eq!(f.pts, vec![0.0, 0.5, 1.0, 1.5]);
        assert_eq!(f.key, vec![0, 0, 2, 2]);
        assert_eq!(f.index_at(-1.0), 0);
        assert_eq!(f.index_at(0.49), 0);
        assert_eq!(f.index_at(0.5), 1);
        assert_eq!(f.index_at(1.99), 3);
        assert_eq!(f.index_at(9.0), 3);
        assert_eq!(f.fps(), 2.0);
    }

    #[test]
    fn rotates_clockwise() {
        // 2×1: red, green.
        let px = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let r = rotated(2, 1, px.clone(), 90);
        assert_eq!((r.width(), r.height()), (1, 2));
        assert_eq!(r.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(r.pixel(0, 1), [0, 255, 0, 255]);
        let r = rotated(2, 1, px.clone(), 270);
        assert_eq!(r.pixel(0, 0), [0, 255, 0, 255]);
        let r = rotated(2, 1, px.clone(), 180);
        assert_eq!(r.pixel(0, 0), [0, 255, 0, 255]);
        assert_eq!(rotated(2, 1, px, 0).pixel(0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn annex_b() {
        let mut out = Vec::new();
        to_annex_b(&[0, 0, 0, 2, 9, 8, 0, 0, 0, 1, 7], 4, &mut out).unwrap();
        assert_eq!(out, [0, 0, 0, 1, 9, 8, 0, 0, 0, 1, 7]);
        assert!(to_annex_b(&[0, 0, 0, 5, 1], 4, &mut Vec::new()).is_err());
    }
}
