//! macOS: H.264 MP4 export through Apple's encoder (`AVAssetWriter`,
//! hardware-accelerated through VideoToolbox). Apple licenses H.264 for
//! apps that use it, so the macOS build needs no encoder of its own.
//!
//! Frames go in as BGRA pixel buffers stamped `i / fps`; the session ends
//! at `frames / fps`, so the last frame lasts a full frame and a one-loop
//! export repeats seamlessly. Colours are tagged BT.709, as QuickTime and
//! browsers expect.
//!
//! With sound, a second input takes interleaved float PCM and Apple's AAC
//! encoder (licensed with the OS, like H.264) compresses it. Sound buffers
//! are stamped in samples (`n / sample_rate`) and queued here, then handed
//! over whenever the audio input is ready, including while a video frame
//! waits, so the writer can always interleave the two tracks. The session
//! end trims the sound to the pictures (they are pushed to the same
//! length anyway, see `export::audio_span`); AVFoundation records the AAC
//! encoder delay in the file, so playback starts on sample 0.

use crate::export::{AudioSettings, H264Settings};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoColorPrimariesKey, AVVideoColorPrimaries_ITU_R_709_2,
    AVVideoColorPropertiesKey, AVVideoCompressionPropertiesKey, AVVideoExpectedSourceFrameRateKey,
    AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey, AVVideoProfileLevelH264HighAutoLevel,
    AVVideoProfileLevelKey, AVVideoTransferFunctionKey, AVVideoTransferFunction_ITU_R_709_2,
    AVVideoWidthKey, AVVideoYCbCrMatrixKey, AVVideoYCbCrMatrix_ITU_R_709_2,
};
use objc2_avf_audio::{AVEncoderBitRateKey, AVFormatIDKey, AVNumberOfChannelsKey, AVSampleRateKey};
use objc2_core_audio_types::{
    kAudioFormatFlagsNativeFloatPacked, kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
    AudioStreamBasicDescription,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    kCMBlockBufferAssureMemoryNowFlag, CMAudioFormatDescription, CMAudioFormatDescriptionCreate,
    CMAudioSampleBufferCreateReadyWithPacketDescriptions, CMBlockBuffer, CMFormatDescription,
    CMSampleBuffer, CMTime,
};
use objc2_core_video::{
    kCVPixelBufferHeightKey, kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_32BGRA, kCVReturnSuccess, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferPool,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSURL};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

/// How long to wait for the encoder to take a frame or close the file.
const PATIENCE: Duration = Duration::from_secs(60);

/// Stereo, interleaved `f32`.
const CHANNELS: usize = 2;
/// Sample frames per sound buffer handed to the writer.
const SOUND_CHUNK: usize = 1024;

pub struct Writer {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    sound: Option<Sound>,
    settings: H264Settings,
    frames: usize,
    finished: bool,
}

/// The sound track: its writer input, the PCM format pushed into it, and
/// the samples not handed over yet.
struct Sound {
    input: Retained<AVAssetWriterInput>,
    format: CFRetained<CMAudioFormatDescription>,
    rate: u32,
    pending: VecDeque<f32>,
    /// Sample frames handed over so far: the next buffer's time stamp.
    written: u64,
}

impl Sound {
    /// Add an AAC input for `settings`, fed float PCM, to a writer that
    /// hasn't started writing.
    fn add(writer: &AVAssetWriter, settings: AudioSettings) -> Result<Sound, String> {
        let rate = settings.sample_rate.max(1);
        // SAFETY: AVFoundation / CoreMedia calls with valid, retained
        // arguments (the settings dictionary maps NSString keys to NSNumber
        // values, as AVAudioSettings.h documents); the format description
        // out-pointer is checked before it's adopted.
        unsafe {
            let (format_id, sample_rate, channels, bitrate) = (
                num(kAudioFormatMPEG4AAC),
                NSNumber::new_f64(f64::from(rate)),
                num(CHANNELS as u32),
                num(settings.bitrate),
            );
            let output = dict(&[
                (key(AVFormatIDKey)?, &format_id),
                (key(AVSampleRateKey)?, &sample_rate),
                (key(AVNumberOfChannelsKey)?, &channels),
                (key(AVEncoderBitRateKey)?, &bitrate),
            ]);
            let audio = AVMediaTypeAudio.ok_or("AVFoundation: no audio media type")?;
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                audio,
                Some(&output),
            );
            input.setExpectsMediaDataInRealTime(false);
            if !writer.canAddInput(&input) {
                return Err("AVFoundation can't write AAC sound at this rate".into());
            }
            writer.addInput(&input);

            let bytes = (CHANNELS * std::mem::size_of::<f32>()) as u32;
            let asbd = AudioStreamBasicDescription {
                mSampleRate: f64::from(rate),
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagsNativeFloatPacked,
                mBytesPerPacket: bytes,
                mFramesPerPacket: 1,
                mBytesPerFrame: bytes,
                mChannelsPerFrame: CHANNELS as u32,
                mBitsPerChannel: 32,
                mReserved: 0,
            };
            let mut out: *const CMAudioFormatDescription = std::ptr::null();
            let status = CMAudioFormatDescriptionCreate(
                None,
                NonNull::from(&asbd),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                None,
                NonNull::from(&mut out),
            );
            let format = match NonNull::new(out.cast_mut()) {
                Some(f) if status == 0 => CFRetained::from_raw(f),
                _ => return Err(format!("CoreMedia: no PCM format ({status})")),
            };
            Ok(Sound {
                input,
                format,
                rate,
                pending: VecDeque::new(),
                written: 0,
            })
        }
    }

    /// Hand the writer one buffer of up to [`SOUND_CHUNK`] queued frames.
    fn append_chunk(&mut self) -> Result<bool, String> {
        let frames = (self.pending.len() / CHANNELS).min(SOUND_CHUNK);
        if frames == 0 {
            return Ok(false);
        }
        let samples: Vec<f32> = self.pending.drain(..frames * CHANNELS).collect();
        let len = std::mem::size_of_val(samples.as_slice());
        // SAFETY: a block buffer of `len` bytes, allocated now and filled
        // from `samples` (exactly `len` bytes); the sample buffer refers to
        // it and the format description, both retained until appended.
        unsafe {
            let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
            let status = CMBlockBuffer::create_with_memory_block(
                None,
                std::ptr::null_mut(),
                len,
                None,
                std::ptr::null(),
                0,
                len,
                kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut block),
            );
            let block = match NonNull::new(block) {
                Some(b) if status == 0 => CFRetained::from_raw(b),
                _ => return Err(format!("CoreMedia: no sound buffer ({status})")),
            };
            let src = NonNull::new(samples.as_ptr().cast_mut().cast::<c_void>())
                .ok_or("CoreMedia: no sound samples")?;
            let status = CMBlockBuffer::replace_data_bytes(src, &block, 0, len);
            if status != 0 {
                return Err(format!(
                    "CoreMedia: couldn't fill a sound buffer ({status})"
                ));
            }
            let at = CMTime::new(self.written as i64, self.rate as i32);
            let mut sample: *mut CMSampleBuffer = std::ptr::null_mut();
            let format: &CMFormatDescription = &self.format;
            let status = CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &block,
                format,
                frames as _,
                at,
                std::ptr::null(),
                NonNull::from(&mut sample),
            );
            let sample = match NonNull::new(sample) {
                Some(s) if status == 0 => CFRetained::from_raw(s),
                _ => return Err(format!("CoreMedia: no sound sample buffer ({status})")),
            };
            if !self.input.appendSampleBuffer(&sample) {
                return Err("AVFoundation didn't take the sound".into());
            }
        }
        self.written += frames as u64;
        Ok(true)
    }
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
    pub fn create(
        path: &Path,
        settings: H264Settings,
        sound: Option<AudioSettings>,
    ) -> Result<Writer, String> {
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
            let sound = sound.map(|a| Sound::add(&writer, a)).transpose()?;
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
                sound,
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

    pub fn has_sound(&self) -> bool {
        self.sound.is_some()
    }

    /// Queue interleaved stereo samples (see `Mp4File::push_audio`) and
    /// hand over what the writer will take now.
    pub fn push_audio(&mut self, interleaved: &[f32]) -> Result<(), String> {
        let Some(s) = self.sound.as_mut() else {
            return Ok(());
        };
        s.pending.extend(interleaved.iter().map(|v| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        }));
        self.pump_sound()?;
        Ok(())
    }

    /// Hand queued sound to the writer while its input is ready. True if
    /// anything went.
    fn pump_sound(&mut self) -> Result<bool, String> {
        let mut sent = false;
        loop {
            let Some(s) = self.sound.as_mut() else {
                return Ok(sent);
            };
            // SAFETY: readiness of an input we own.
            if s.pending.len() < CHANNELS || !unsafe { s.input.isReadyForMoreMediaData() } {
                return Ok(sent);
            }
            if let Err(e) = s.append_chunk() {
                return Err(self.failed().unwrap_or(e));
            }
            sent = true;
        }
    }

    /// Hand over all queued sound, waiting for the writer as needed.
    fn drain_sound(&mut self) -> Result<(), String> {
        let since = Instant::now();
        while self
            .sound
            .as_ref()
            .is_some_and(|s| s.pending.len() >= CHANNELS)
        {
            if let Some(e) = self.failed() {
                return Err(e);
            }
            if since.elapsed() > PATIENCE {
                return Err("AVFoundation stopped taking sound".into());
            }
            if !self.pump_sound()? {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
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
            // The writer may be waiting for sound to interleave.
            if !self.pump_sound()? {
                std::thread::sleep(Duration::from_millis(1));
            }
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
        // The pictures are all in: let the sound catch up, then close both.
        // SAFETY: finishing an input we own.
        unsafe { self.input.markAsFinished() };
        self.drain_sound()?;
        self.finished = true;
        let (tx, rx) = std::sync::mpsc::channel();
        let done = RcBlock::new(move || {
            let _ = tx.send(());
        });
        // SAFETY: finishing a writer we own; the block only signals the
        // channel, from whichever queue AVFoundation calls it on.
        unsafe {
            if let Some(s) = &self.sound {
                s.input.markAsFinished();
            }
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
