//! Audio files for `sound/sound-file`: read the URL (data:, file path or
//! http through the app's fetcher), decode with symphonia, keep up to two
//! channels at the file's own rate (the player resamples).

use floret_engine::audio::SampleState;
use floret_engine::host::Host;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// Longest file accepted (frames at the file's rate): 20 minutes at 48 kHz.
const MAX_FRAMES: usize = 48000 * 60 * 20;

/// Decoded audio: one or two planar channels.
#[derive(Debug)]
pub struct SampleData {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: u32,
}

impl SampleData {
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate.max(1))
    }

    /// Linearly interpolated (left, right) at fractional frame `pos`.
    pub fn frame_at(&self, pos: f64) -> (f32, f32) {
        let n = self.frames();
        if n == 0 || pos < 0.0 {
            return (0.0, 0.0);
        }
        let i0 = (pos.floor() as usize).min(n - 1);
        let i1 = (i0 + 1).min(n - 1);
        let f = (pos - pos.floor()) as f32;
        let at = |c: &Vec<f32>| c[i0] + (c[i1] - c[i0]) * f;
        let l = at(&self.channels[0]);
        let r = self.channels.get(1).map_or(l, at);
        (l, r)
    }
}

/// Decode a whole file in memory.
pub fn decode(bytes: Vec<u8>) -> Result<SampleData, String> {
    let mss = MediaSourceStream::new(
        Box::new(Cursor::new(bytes)),
        MediaSourceStreamOptions::default(),
    );
    let mut format = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("unsupported audio file: {e}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("no audio track")?
        .clone();
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("no audio codec parameters")?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;
    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let mut planes: Vec<Vec<f32>> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id != track.id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            // A corrupt packet: skip it, as players do.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        if sample_rate == 0 {
            sample_rate = buf.spec().rate();
        }
        buf.copy_to_vecs_planar(&mut planes);
        if channels.is_empty() {
            channels = vec![Vec::new(); planes.len().clamp(1, 2)];
        }
        match planes.len() {
            0 => {}
            1 | 2 => {
                for (c, p) in channels.iter_mut().zip(&planes) {
                    c.extend_from_slice(p);
                }
            }
            n => {
                // More than stereo: fold the extra channels into L / R.
                let frames = planes[0].len();
                for i in 0..frames {
                    let (mut l, mut r) = (0.0, 0.0);
                    for (k, p) in planes.iter().enumerate() {
                        if k % 2 == 0 {
                            l += p[i];
                        } else {
                            r += p[i];
                        }
                    }
                    let half = (n / 2) as f32;
                    channels[0].push(l / n.div_ceil(2) as f32);
                    channels[1].push(r / half);
                }
            }
        }
        if channels[0].len() > MAX_FRAMES {
            return Err("audio file too long".into());
        }
    }
    if channels.is_empty() || sample_rate == 0 {
        return Err("no audio decoded".into());
    }
    Ok(SampleData {
        channels,
        sample_rate,
    })
}

type Slot = Arc<Mutex<Option<Result<Arc<SampleData>, String>>>>;

/// Loads by URL, each on its own thread; shared by every node playing it.
#[derive(Default)]
pub struct Samples {
    entries: HashMap<Arc<str>, Slot>,
}

impl Samples {
    /// Start loading `url` if it isn't already.
    pub fn ensure(&mut self, url: &Arc<str>, host: Option<Arc<dyn Host>>) {
        if self.entries.contains_key(url) {
            return;
        }
        let slot: Slot = Arc::default();
        let shared = slot.clone();
        let src = url.to_string();
        let spawned = std::thread::Builder::new()
            .name("floret audio load".into())
            .spawn(move || {
                let r = floret_engine::image::read_source(&src, host.as_deref())
                    .and_then(decode)
                    .map(Arc::new);
                *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            });
        if let Err(e) = spawned {
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(e.to_string()));
        }
        self.entries.insert(url.clone(), slot);
    }

    /// The decoded audio, once ready.
    pub fn get(&self, url: &str) -> Option<Arc<SampleData>> {
        let slot = self.entries.get(url)?;
        let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref()?.as_ref().ok().cloned()
    }

    pub fn state(&self, url: &str) -> Option<SampleState> {
        let slot = self.entries.get(url)?;
        let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
        Some(match guard.as_ref() {
            None => SampleState::Loading,
            Some(Ok(d)) => SampleState::Ready {
                duration: d.duration(),
            },
            Some(Err(e)) => SampleState::Failed(e.clone()),
        })
    }

    /// Forget URLs no node uses any more.
    pub fn retain(&mut self, used: impl Fn(&str) -> bool) {
        self.entries.retain(|k, _| used(k));
    }

    pub fn states(&self) -> HashMap<Arc<str>, SampleState> {
        self.entries
            .keys()
            .filter_map(|k| Some((k.clone(), self.state(k)?)))
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 16-bit PCM WAV file.
    pub fn wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&sample_rate.to_le_bytes());
        v.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
        v.extend_from_slice(&(channels * 2).to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    #[test]
    fn decodes_wav() {
        let d = decode(wav(22050, 2, &[0, 16384, -16384, 0, 32767, -32768])).unwrap();
        assert_eq!((d.sample_rate, d.channels.len(), d.frames()), (22050, 2, 3));
        assert_eq!(d.channels[0], vec![0.0, -0.5, 32767.0 / 32768.0]);
        assert_eq!(d.channels[1], vec![0.5, 0.0, -1.0]);
        assert!(decode(b"not audio".to_vec()).is_err());
        let (l, r) = d.frame_at(0.5);
        assert_eq!((l, r), (-0.25, 0.25));
    }
}
