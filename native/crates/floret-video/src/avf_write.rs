//! macOS: H.264 MP4 export through Apple's encoder (`AVAssetWriter`,
//! hardware-accelerated through VideoToolbox). Apple licenses H.264 for
//! apps that use it, so the macOS build needs no encoder of its own.
//!
//! Frames go in as BGRA pixel buffers stamped `i / fps`; the session ends
//! at `frames / fps`, so the last frame lasts a full frame and a one-loop
//! export repeats seamlessly. Colours are tagged BT.709, as QuickTime and
//! browsers expect.

use crate::export::H264Settings;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoColorPrimariesKey, AVVideoColorPrimaries_ITU_R_709_2,
    AVVideoColorPropertiesKey, AVVideoCompressionPropertiesKey, AVVideoExpectedSourceFrameRateKey,
    AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey, AVVideoProfileLevelH264HighAutoLevel,
    AVVideoProfileLevelKey, AVVideoTransferFunctionKey, AVVideoTransferFunction_ITU_R_709_2,
    AVVideoWidthKey, AVVideoYCbCrMatrixKey, AVVideoYCbCrMatrix_ITU_R_709_2,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
use objc2_core_video::{
    kCVPixelBufferHeightKey, kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_32BGRA, kCVReturnSuccess, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferPool,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSURL};
use std::path::Path;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

/// How long to wait for the encoder to take a frame or close the file.
const PATIENCE: Duration = Duration::from_secs(60);

pub struct Writer {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    settings: H264Settings,
    frames: usize,
    finished: bool,
}

fn describe(e: Option<Retained<NSError>>) -> String {
    e.map(|e| e.localizedDescription().to_string())
        .unwrap_or_else(|| "unknown error".into())
}

fn key(k: Option<&'static NSString>) -> Result<&'static NSString, String> {
    k.ok_or_else(|| "AVFoundation: a video setting is missing on this macOS".into())
}

fn dict(pairs: &[(&NSString, &AnyObject)]) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = pairs.iter().map(|p| p.0).collect();
    let values: Vec<&AnyObject> = pairs.iter().map(|p| p.1).collect();
    NSDictionary::from_slices(&keys, &values)
}

fn num(n: u32) -> Retained<NSNumber> {
    NSNumber::new_u32(n)
}

impl Writer {
    pub fn create(path: &Path, settings: H264Settings) -> Result<Writer, String> {
        // AVAssetWriter won't overwrite.
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("{}: {e}", path.display()))
            }
            _ => {}
        }
        let url = NSURL::from_file_path(path).ok_or("not a file path")?;
        let (w, h, fps) = (settings.width, settings.height, settings.fps);
        // SAFETY: AVFoundation calls with valid, retained arguments; every
        // settings dictionary maps NSString keys to NSString / NSNumber /
        // NSDictionary values, as AVVideoSettings.h documents.
        unsafe {
            let file_type = AVFileTypeMPEG4.ok_or("AVFoundation: no MPEG-4 file type")?;
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|e| format!("AVFoundation: {}", describe(Some(e))))?;

            let (bitrate, keyframes, rate) = (
                num(settings.bitrate),
                num(settings.keyframe_every),
                num(fps),
            );
            let profile = key(AVVideoProfileLevelH264HighAutoLevel)?;
            let compression = dict(&[
                (key(AVVideoAverageBitRateKey)?, &bitrate),
                (key(AVVideoMaxKeyFrameIntervalKey)?, &keyframes),
                (key(AVVideoExpectedSourceFrameRateKey)?, &rate),
                (key(AVVideoProfileLevelKey)?, profile),
            ]);
            let colour = dict(&[
                (
                    key(AVVideoColorPrimariesKey)?,
                    key(AVVideoColorPrimaries_ITU_R_709_2)?,
                ),
                (
                    key(AVVideoTransferFunctionKey)?,
                    key(AVVideoTransferFunction_ITU_R_709_2)?,
                ),
                (
                    key(AVVideoYCbCrMatrixKey)?,
                    key(AVVideoYCbCrMatrix_ITU_R_709_2)?,
                ),
            ]);
            let codec: &NSString = AVVideoCodecTypeH264.ok_or("AVFoundation: no H.264 codec")?;
            let (width, height) = (num(w), num(h));
            let output = dict(&[
                (key(AVVideoCodecKey)?, codec),
                (key(AVVideoWidthKey)?, &width),
                (key(AVVideoHeightKey)?, &height),
                (key(AVVideoCompressionPropertiesKey)?, &compression),
                (key(AVVideoColorPropertiesKey)?, &colour),
            ]);
            let video = AVMediaTypeVideo.ok_or("AVFoundation: no video media type")?;
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                video,
                Some(&output),
            );
            input.setExpectsMediaDataInRealTime(false);

            let bgra = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let source = dict(&[
                (kCVPixelBufferPixelFormatTypeKey.as_ref(), &bgra),
                (kCVPixelBufferWidthKey.as_ref(), &width),
                (kCVPixelBufferHeightKey.as_ref(), &height),
            ]);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &input,
                Some(&source),
            );
            if !writer.canAddInput(&input) {
                return Err("AVFoundation can't write H.264 at this size".into());
            }
            writer.addInput(&input);
            if !writer.startWriting() {
                return Err(format!(
                    "AVFoundation couldn't start writing: {}",
                    describe(writer.error())
                ));
            }
            writer.startSessionAtSourceTime(CMTime::new(0, fps as i32));
            Ok(Writer {
                writer,
                input,
                adaptor,
                settings,
                frames: 0,
                finished: false,
            })
        }
    }

    fn failed(&self) -> Option<String> {
        // SAFETY: status / error of a writer we own.
        unsafe {
            (self.writer.status() == AVAssetWriterStatus::Failed)
                .then(|| format!("AVFoundation: {}", describe(self.writer.error())))
        }
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Add a frame of RGBA8 pixels (alpha ignored), row-major.
    pub fn push_rgba(&mut self, rgba: &[u8]) -> Result<(), String> {
        let (w, h) = (self.settings.width as usize, self.settings.height as usize);
        if rgba.len() != w * h * 4 {
            return Err(format!("frame is {} bytes, not {w} × {h} RGBA", rgba.len()));
        }
        let since = Instant::now();
        // SAFETY: readiness of an input we own.
        while !unsafe { self.input.isReadyForMoreMediaData() } {
            if let Some(e) = self.failed() {
                return Err(e);
            }
            if since.elapsed() > PATIENCE {
                return Err("AVFoundation stopped taking frames".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let pb = self.pixel_buffer()?;
        // SAFETY: the buffer is locked while written, and every row written
        // lies inside `height * bytes_per_row` of a buffer at least w × h.
        unsafe {
            CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags::empty());
            let stride = CVPixelBufferGetBytesPerRow(&pb);
            let base = CVPixelBufferGetBaseAddress(&pb).cast::<u8>();
            let ok = !base.is_null()
                && CVPixelBufferGetWidth(&pb) >= w
                && CVPixelBufferGetHeight(&pb) >= h
                && stride >= w * 4;
            if ok {
                let dst = std::slice::from_raw_parts_mut(base, stride * h);
                for y in 0..h {
                    let row = &mut dst[y * stride..y * stride + w * 4];
                    let src = &rgba[y * w * 4..(y + 1) * w * 4];
                    for (o, i) in row
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(src.as_chunks::<4>().0)
                    {
                        *o = [i[2], i[1], i[0], 255];
                    }
                }
            }
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags::empty());
            if !ok {
                return Err("AVFoundation gave a pixel buffer of the wrong size".into());
            }
            let at = CMTime::new(self.frames as i64, self.settings.fps as i32);
            if !self.adaptor.appendPixelBuffer_withPresentationTime(&pb, at) {
                return Err(self
                    .failed()
                    .unwrap_or_else(|| "AVFoundation didn't take a frame".into()));
            }
        }
        self.frames += 1;
        Ok(())
    }

    fn pixel_buffer(&self) -> Result<CFRetained<CVPixelBuffer>, String> {
        // SAFETY: the pool exists once writing has started; a successful
        // create hands back a +1 buffer, which `CFRetained` takes over.
        unsafe {
            let pool = self.adaptor.pixelBufferPool().ok_or_else(|| {
                self.failed()
                    .unwrap_or_else(|| "AVFoundation has no pixel buffers".into())
            })?;
            let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
            let r = CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut out));
            match NonNull::new(out) {
                Some(pb) if r == kCVReturnSuccess => Ok(CFRetained::from_raw(pb)),
                _ => Err(format!("AVFoundation couldn't make a frame buffer ({r})")),
            }
        }
    }

    /// End the video at `frames / fps` and close the file.
    pub fn finish(mut self) -> Result<(), String> {
        self.finished = true;
        let (tx, rx) = std::sync::mpsc::channel();
        let done = RcBlock::new(move || {
            let _ = tx.send(());
        });
        // SAFETY: finishing a writer we own; the block only signals the
        // channel, from whichever queue AVFoundation calls it on.
        unsafe {
            self.input.markAsFinished();
            let end = CMTime::new(self.frames as i64, self.settings.fps as i32);
            self.writer.endSessionAtSourceTime(end);
            self.writer.finishWritingWithCompletionHandler(&done);
        }
        rx.recv_timeout(PATIENCE)
            .map_err(|_| "AVFoundation didn't finish the file".to_owned())?;
        // SAFETY: status of a writer we own.
        match unsafe { self.writer.status() } {
            AVAssetWriterStatus::Completed => Ok(()),
            _ => Err(self
                .failed()
                .unwrap_or_else(|| "AVFoundation didn't finish the file".into())),
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // Dropped unfinished (cancelled, or an error): stop the encoder.
        // SAFETY: status / cancel of a writer we own.
        unsafe {
            if !self.finished && self.writer.status() == AVAssetWriterStatus::Writing {
                self.writer.cancelWriting();
            }
        }
    }
}
