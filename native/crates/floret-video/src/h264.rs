//! H.264 decoding with OpenH264 (built from source by the `openh264`
//! crate, BSD): works the same on every OS.

use crate::demux::{self, AvcConfig, ReadSeek, Sample};
use crate::Decoder;
use floret_engine::image::Image;
use openh264::decoder::{Decoder as Oh264, DecoderConfig, Flush};
use openh264::formats::YUVSource;
use openh264::OpenH264API;
use std::collections::{BTreeSet, VecDeque};

pub struct H264 {
    reader: Box<dyn ReadSeek>,
    samples: Vec<Sample>,
    config: AvcConfig,
    decoder: Option<Oh264>,
    /// Next sample to feed, decode order.
    next: usize,
    /// Display indices fed but not yet output. OpenH264 hands frames back
    /// in display order, so each output is the smallest one pending.
    pending: BTreeSet<usize>,
    outbox: VecDeque<(usize, Option<Image>)>,
    /// Display index → decode index, for seeks.
    decode_of: Vec<usize>,
    /// Display index → the keyframe to start from (`Frames::key`).
    key: Vec<usize>,
    rotation: u16,
    sample: Vec<u8>,
    annex_b: Vec<u8>,
}

impl H264 {
    pub fn new(
        reader: Box<dyn ReadSeek>,
        samples: Vec<Sample>,
        key: Vec<usize>,
        rotation: u16,
        config: AvcConfig,
    ) -> H264 {
        let mut decode_of = vec![0; samples.len()];
        for (i, s) in samples.iter().enumerate() {
            decode_of[s.display] = i;
        }
        H264 {
            reader,
            samples,
            config,
            decoder: None,
            next: usize::MAX,
            pending: BTreeSet::new(),
            outbox: VecDeque::new(),
            decode_of,
            key,
            rotation,
            sample: Vec::new(),
            annex_b: Vec::new(),
        }
    }

    fn fresh_decoder(&self) -> Result<Oh264, String> {
        let cfg = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
        let mut dec = Oh264::with_api_config(OpenH264API::from_source(), cfg)
            .map_err(|e| format!("H.264 decoder: {e}"))?;
        let mut headers = Vec::new();
        for nal in self.config.sps.iter().chain(&self.config.pps) {
            headers.extend_from_slice(&[0, 0, 0, 1]);
            headers.extend_from_slice(nal);
        }
        if !headers.is_empty() {
            dec.decode(&headers)
                .map_err(|e| format!("H.264 parameter sets: {e}"))?;
        }
        Ok(dec)
    }
}

fn to_image(yuv: &impl YUVSource, rotation: u16, write: impl FnOnce(&mut [u8])) -> Image {
    let (w, h) = yuv.dimensions();
    let mut rgba = vec![0; w * h * 4];
    write(&mut rgba);
    demux::rotated(w as u32, h as u32, rgba, rotation)
}

impl Decoder for H264 {
    fn seek(&mut self, target: usize) -> Result<(), String> {
        let start = self.key.get(target).copied().unwrap_or(0);
        self.decoder = Some(self.fresh_decoder()?);
        self.next = self.decode_of.get(start).copied().unwrap_or(0);
        self.pending.clear();
        self.outbox.clear();
        Ok(())
    }

    fn next(
        &mut self,
        keep: &dyn Fn(usize) -> bool,
    ) -> Result<Option<(usize, Option<Image>)>, String> {
        let rotation = self.rotation;
        loop {
            if let Some(out) = self.outbox.pop_front() {
                return Ok(Some(out));
            }
            let Some(dec) = self.decoder.as_mut() else {
                return Ok(None);
            };
            if let Some(s) = self.samples.get(self.next).copied() {
                self.next += 1;
                demux::read_sample(&mut *self.reader, &s, &mut self.sample)?;
                self.annex_b.clear();
                demux::to_annex_b(&self.sample, self.config.length_size, &mut self.annex_b)?;
                self.pending.insert(s.display);
                match dec.decode(&self.annex_b) {
                    Ok(Some(yuv)) => {
                        let Some(i) = self.pending.pop_first() else {
                            continue;
                        };
                        let img =
                            keep(i).then(|| to_image(&yuv, rotation, |px| yuv.write_rgba8(px)));
                        return Ok(Some((i, img)));
                    }
                    Ok(None) => {}
                    // A damaged frame: skip it and carry on (its slot
                    // just never fills).
                    Err(_) => {
                        self.pending.remove(&s.display);
                    }
                }
            } else {
                // End of stream: drain the reorder buffer, then stop.
                let frames = dec.flush_remaining().unwrap_or_default();
                for yuv in &frames {
                    if let Some(i) = self.pending.pop_first() {
                        let img =
                            keep(i).then(|| to_image(yuv, rotation, |px| yuv.write_rgba8(px)));
                        self.outbox.push_back((i, img));
                    }
                }
                self.decoder = None;
                self.pending.clear();
            }
        }
    }
}
