//! Floret's media I/O. For now: the real-time audio engine behind the
//! `sound/*` nodes.
//!
//! The engine (`floret_engine::audio`) declares an audio graph every tick;
//! [`AudioSystem`] implements its `AudioBackend`:
//!
//! * `graph` diffs each tick's declarations on the control (app) thread
//!   and updates the audio thread lock-free: parameters through per-node
//!   atomics, topology through rebuilt programs sent over a ring.
//! * `render` is the audio thread: render quanta of 128 frames, no
//!   allocation or locking.
//! * `nodes`, `dsp`, `reverb`, `compressor` implement the Web Audio nodes
//!   the TS app used (oscillators, biquads, delay, convolver, waveshaper,
//!   dynamics compressor, AudioParam ramps and schedules); `wavetable` is
//!   Chromium's band-limited oscillator.
//! * `analysis` is the `AnalyserNode`, computed on the control thread from
//!   lock-free taps when a tick starts.
//! * `device` owns the cpal streams (output, microphone) on its own thread
//!   and falls back to a silent timer when there is no device.
//! * `decode` loads sound files (symphonia).

pub mod analysis;
pub mod compressor;
pub mod decode;
mod device;
pub mod dsp;
pub mod graph;
pub mod nodes;
pub mod render;
pub mod reverb;
mod system;
pub mod wavetable;

pub use system::{AudioSystem, DeviceInfo, Status, DEFAULT_SAMPLE_RATE};
