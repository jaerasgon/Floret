//! H.264 MP4 encoding: OpenH264's encoder (built from the source the
//! `openh264` crate bundles, BSD) into [`crate::mux::Mp4Muxer`]. Used by
//! the app's video export and by [`crate::clip`]'s test clips.

use crate::mux::Mp4Muxer;
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, QpRange,
    RateControlMode, VuiConfig,
};
use openh264::formats::{RgbSliceU8, RgbaSliceU8, YUVBuffer};
use openh264::OpenH264API;
use std::io::{Seek, Write};

pub use crate::export::{H264Settings, MAX_SIZE};

/// Split an Annex B stream at its start codes.
pub(crate) fn nal_units(stream: &[u8]) -> Vec<&[u8]> {
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

/// Frames in, an H.264 MP4 out.
pub struct Mp4Encoder<W: Write + Seek> {
    enc: Encoder,
    mux: Mp4Muxer<W>,
    yuv: YUVBuffer,
    settings: H264Settings,
    sps: Vec<u8>,
    pps: Vec<u8>,
    sample: Vec<u8>,
}

impl<W: Write + Seek> Mp4Encoder<W> {
    pub fn new(out: W, settings: H264Settings) -> Result<Self, String> {
        settings.check()?;
        let config = EncoderConfig::new()
            .skip_frames(false)
            .rate_control_mode(RateControlMode::Quality)
            .bitrate(BitRate::from_bps(settings.bitrate))
            .max_frame_rate(FrameRate::from_hz(settings.fps as f32))
            .intra_frame_period(IntraFramePeriod::from_num_frames(settings.keyframe_every))
            .qp(QpRange::new(settings.qp.0, settings.qp.1))
            // The RGB → YUV conversion below is limited-range BT.601.
            .vui(VuiConfig::bt601());
        let enc = Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| format!("H.264 encoder: {e}"))?;
        let mux = Mp4Muxer::new(out, settings.width, settings.height, settings.fps)
            .map_err(|e| e.to_string())?;
        Ok(Mp4Encoder {
            enc,
            mux,
            yuv: YUVBuffer::new(settings.width as usize, settings.height as usize),
            settings,
            sps: Vec::new(),
            pps: Vec::new(),
            sample: Vec::new(),
        })
    }

    fn dims(&self) -> (usize, usize) {
        (self.settings.width as usize, self.settings.height as usize)
    }

    /// Add a frame of RGBA8 pixels (alpha ignored), row-major.
    pub fn push_rgba(&mut self, rgba: &[u8]) -> Result<(), String> {
        let (w, h) = self.dims();
        if rgba.len() != w * h * 4 {
            return Err(format!("frame is {} bytes, not {w} × {h} RGBA", rgba.len()));
        }
        self.yuv.read_rgba8(RgbaSliceU8::new(rgba, (w, h)));
        self.encode()
    }

    /// Add a frame of RGB8 pixels, row-major.
    pub fn push_rgb(&mut self, rgb: &[u8]) -> Result<(), String> {
        let (w, h) = self.dims();
        if rgb.len() != w * h * 3 {
            return Err(format!("frame is {} bytes, not {w} × {h} RGB", rgb.len()));
        }
        self.yuv.read_rgb8(RgbSliceU8::new(rgb, (w, h)));
        self.encode()
    }

    fn encode(&mut self) -> Result<(), String> {
        let i = self.mux.frames();
        let bits = self
            .enc
            .encode(&self.yuv)
            .map_err(|e| format!("H.264 encode: {e}"))?;
        let idr = matches!(bits.frame_type(), FrameType::IDR);
        let stream = bits.to_vec();
        self.sample.clear();
        for nal in nal_units(&stream) {
            match nal.first().map(|b| b & 0x1f) {
                Some(7) => self.sps = nal.to_vec(),
                Some(8) => self.pps = nal.to_vec(),
                Some(_) => {
                    self.sample
                        .extend_from_slice(&(nal.len() as u32).to_be_bytes());
                    self.sample.extend_from_slice(nal);
                }
                None => {}
            }
        }
        if self.sample.is_empty() {
            return Err(format!("frame {i} encoded to nothing"));
        }
        self.mux
            .push(&self.sample, idr)
            .map_err(|e| format!("writing the video: {e}"))
    }

    /// Frames added so far.
    pub fn frames(&self) -> usize {
        self.mux.frames()
    }

    /// Write the MP4 index. Returns the output.
    pub fn finish(self) -> Result<W, String> {
        self.mux
            .finish(&self.sps, &self.pps)
            .map_err(|e| format!("writing the video: {e}"))
    }
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
