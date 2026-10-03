//! macOS: decode with AVFoundation (`AVAssetReader`, hardware-accelerated
//! through VideoToolbox), so HEVC, ProRes and anything else QuickTime
//! plays work too.
//!
//! Frame timing comes from the MP4 / MOV sample tables when `re_mp4` can
//! read them, else it's derived from the track's nominal frame rate. A seek
//! restarts the reader at the frame's time; AVFoundation decodes from the
//! keyframe itself and hands back frames from there.

use crate::demux::{self, Frames, ReadSeek};
use crate::{Decoder, Media};
use floret_engine::image::Image;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAsset, AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack,
    AVMediaTypeVideo, AVURLAsset,
};
use objc2_core_media::{kCMTimePositiveInfinity, CMSampleBuffer, CMTime, CMTimeRange};
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSURL};
use std::path::{Path, PathBuf};

pub struct Avf {
    asset: Retained<AVURLAsset>,
    track: Retained<AVAssetTrack>,
    reading: Option<(Retained<AVAssetReader>, Retained<AVAssetReaderTrackOutput>)>,
    pts: Vec<f64>,
    frames: Frames,
    /// The track's first presentation time (edit lists can shift it).
    start: f64,
    rotation: u16,
    last: Option<usize>,
}

// AVAsset and its tracks are immutable and documented as safe to use from
// any thread; the decoder is created and used on its session's thread.
unsafe impl Send for Avf {}

fn describe(e: &NSError) -> String {
    e.localizedDescription().to_string()
}

/// The clockwise rotation (0 / 90 / 180 / 270) of a display transform.
fn rotation_of(a: f64, b: f64) -> u16 {
    let deg = b.atan2(a).to_degrees().rem_euclid(360.0);
    ((deg / 90.0).round() as u16 % 4) * 90
}

/// AVFoundation reads files only: park an in-memory video (a `data:` URL)
/// in the temporary folder, named by its contents so reopening reuses it.
fn spill(reader: &mut dyn ReadSeek) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    use std::io::SeekFrom;
    let mut bytes = Vec::new();
    let io = |e: std::io::Error| e.to_string();
    reader.seek(SeekFrom::Start(0)).map_err(io)?;
    reader.read_to_end(&mut bytes).map_err(io)?;
    reader.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let dir = std::env::temp_dir().join("floret-video");
    let path = dir.join(format!("{:016x}.mp4", h.finish()));
    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(bytes.len() as u64) {
        std::fs::create_dir_all(&dir).map_err(io)?;
        let part = path.with_extension(format!("{}.part", std::process::id()));
        std::fs::write(&part, &bytes).map_err(io)?;
        std::fs::rename(&part, &path).map_err(io)?;
    }
    Ok(path)
}

pub fn open(path: Option<&Path>, reader: &mut dyn ReadSeek) -> Result<Media, String> {
    let spilled;
    let path = match path {
        Some(p) => p,
        None => {
            spilled = spill(reader)?;
            &spilled
        }
    };
    let url = NSURL::from_file_path(path).ok_or("not a file path")?;
    // SAFETY: plain AVFoundation calls with valid arguments; the deprecated
    // synchronous accessors load on this (background) thread.
    #[allow(deprecated)]
    unsafe {
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let video = AVMediaTypeVideo.ok_or("AVFoundation: no video media type")?;
        let track = asset
            .tracksWithMediaType(video)
            .firstObject()
            .ok_or("the file has no video track AVFoundation can read")?;
        if !track.isDecodable() {
            return Err("AVFoundation can't decode this video".into());
        }
        let size = track.naturalSize();
        let t = track.preferredTransform();
        let rotation = rotation_of(t.a, t.b);
        let range = track.timeRange();
        let start = range.start.seconds();
        let start = if start.is_finite() { start } else { 0.0 };
        let duration = range.duration.seconds();
        let fps = f64::from(track.nominalFrameRate());

        let frames = match demux::probe(reader) {
            Ok(tr) => tr.frames,
            Err(_) => {
                let duration = if duration.is_finite() && duration > 0.0 {
                    duration
                } else {
                    let d = AVAsset::duration(&asset).seconds();
                    if d.is_finite() {
                        d
                    } else {
                        0.0
                    }
                };
                let fps = if fps > 0.0 { fps } else { 30.0 };
                let count = (duration * fps).round().max(1.0) as usize;
                // Unknown keyframes: treat every second as one, so short
                // skips decode through instead of restarting the reader.
                let every = fps.round().max(1.0) as usize;
                Frames::uniform(count, duration, |i| i % every == 0)
            }
        };
        let (mut w, mut h) = (size.width.round() as u32, size.height.round() as u32);
        if !rotation.is_multiple_of(180) {
            std::mem::swap(&mut w, &mut h);
        }
        let avf = Avf {
            asset,
            track,
            reading: None,
            pts: frames.pts.clone(),
            frames: frames.clone(),
            start,
            rotation,
            last: None,
        };
        Ok(Media {
            frames,
            width: w,
            height: h,
            backend: "avfoundation",
            decoder: Box::new(avf),
        })
    }
}

impl Avf {
    fn half_frame(&self) -> f64 {
        0.5 / self.frames.fps()
    }

    fn start_reading(&mut self, from: f64) -> Result<(), String> {
        if let Some((reader, _)) = self.reading.take() {
            // SAFETY: cancelling a reader we own.
            unsafe { reader.cancelReading() };
        }
        // SAFETY: AVFoundation calls with valid, retained arguments; the
        // settings dictionary maps an NSString key to an NSNumber.
        unsafe {
            let reader = AVAssetReader::assetReaderWithAsset_error(&self.asset)
                .map_err(|e| format!("AVFoundation: {}", describe(&e)))?;
            let key: &NSString = kCVPixelBufferPixelFormatTypeKey.as_ref();
            let format = NSNumber::numberWithUnsignedInt(kCVPixelFormatType_32BGRA);
            let value: &AnyObject = &format;
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[value]);
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                &self.track,
                Some(&settings),
            );
            output.setAlwaysCopiesSampleData(false);
            if !reader.canAddOutput(&output) {
                return Err("AVFoundation can't read this track".into());
            }
            reader.addOutput(&output);
            let at = CMTime::with_seconds((from + self.start).max(0.0), 600_000);
            reader.setTimeRange(CMTimeRange {
                start: at,
                duration: kCMTimePositiveInfinity,
            });
            if !reader.startReading() {
                let why = reader.error().map(|e| describe(&e)).unwrap_or_default();
                return Err(format!("AVFoundation couldn't start reading: {why}"));
            }
            self.reading = Some((reader, output));
        }
        Ok(())
    }

    fn to_image(&self, sb: &CMSampleBuffer) -> Result<Image, String> {
        // SAFETY: the pixel buffer is locked while its base address is read,
        // and every row read lies inside `height * bytes_per_row`.
        unsafe {
            let pb = sb.image_buffer().ok_or("a video frame without pixels")?;
            if CVPixelBufferGetPixelFormatType(&pb) != kCVPixelFormatType_32BGRA {
                return Err("unexpected pixel format".into());
            }
            CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly);
            let (w, h) = (CVPixelBufferGetWidth(&pb), CVPixelBufferGetHeight(&pb));
            let stride = CVPixelBufferGetBytesPerRow(&pb);
            let base = CVPixelBufferGetBaseAddress(&pb).cast::<u8>();
            let mut rgba = vec![0u8; w * h * 4];
            if !base.is_null() {
                let src = std::slice::from_raw_parts(base, stride * h);
                for y in 0..h {
                    let row = &src[y * stride..y * stride + w * 4];
                    let out = &mut rgba[y * w * 4..(y + 1) * w * 4];
                    for (o, i) in out
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(row.as_chunks::<4>().0)
                    {
                        *o = [i[2], i[1], i[0], 255];
                    }
                }
            }
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly);
            Ok(demux::rotated(w as u32, h as u32, rgba, self.rotation))
        }
    }
}

impl Decoder for Avf {
    fn seek(&mut self, target: usize) -> Result<(), String> {
        let at = self.pts.get(target).copied().unwrap_or(0.0);
        self.last = None;
        // A reader whose range starts inside a frame's display interval
        // returns that frame (retimed to the range start), so start a
        // quarter frame *into* the target: starting a hair early handed back
        // the previous frame labelled as the target.
        self.start_reading(at + 0.5 * self.half_frame())
    }

    fn next(
        &mut self,
        keep: &dyn Fn(usize) -> bool,
    ) -> Result<Option<(usize, Option<Image>)>, String> {
        loop {
            let Some((reader, output)) = &self.reading else {
                return Ok(None);
            };
            // SAFETY: the output belongs to a reader that has started.
            let Some(sb) = (unsafe { output.copyNextSampleBuffer() }) else {
                // SAFETY: status / error of a reader we own.
                let failed = unsafe { reader.status() } == AVAssetReaderStatus::Failed;
                let why = unsafe { reader.error() }.map(|e| describe(&e));
                self.reading = None;
                return match (failed, why) {
                    (true, Some(e)) => Err(format!("AVFoundation: {e}")),
                    _ => Ok(None),
                };
            };
            // SAFETY: reading a sample buffer's metadata.
            let (count, pts) =
                unsafe { (sb.num_samples(), sb.presentation_time_stamp().seconds()) };
            if count == 0 || !pts.is_finite() {
                continue;
            }
            let i = self.frames.index_at(pts - self.start + self.half_frame());
            if self.last.is_some_and(|l| i <= l) {
                continue;
            }
            self.last = Some(i);
            let img = if keep(i) {
                Some(self.to_image(&sb)?)
            } else {
                None
            };
            return Ok(Some((i, img)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rotation_of;

    #[test]
    fn rotations() {
        assert_eq!(rotation_of(1.0, 0.0), 0);
        assert_eq!(rotation_of(0.0, 1.0), 90);
        assert_eq!(rotation_of(-1.0, 0.0), 180);
        assert_eq!(rotation_of(0.0, -1.0), 270);
    }
}
