//! Device input the app lends the engine: MIDI, cameras and landmark
//! tracking (`input/midi-cc`, `input/midi-note`, `input/camera`,
//! `input/landmarks`).
//!
//! In TS these nodes read browser registries (`midiRegistry`,
//! `cameraRegistry`, `mediapipeRegistry`). Here the engine only sees the
//! [`Devices`] trait; `floret-input` implements it (midir, nokhwa, the
//! MediaPipe models) and the app installs it with [`install`]. With nothing
//! installed the nodes behave as they did when the golden traces were
//! recorded without browser media: MIDI reads "nothing received yet", the
//! camera node errors (empty outputs) and landmarks report no detections.
//!
//! Camera and tracker sessions are keyed by node id and driven by the node
//! every tick (like the TS registries' `ensure`); an implementation should
//! stop a session its node hasn't polled for a while, so deleting a node
//! or switching patch turns the camera off.

use crate::image::Image;
use crate::landmarks::Tracker;
use std::sync::{Arc, RwLock};

/// Held-note state on a device / channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MidiNote {
    /// Most recently pressed note still held (`getLastHeldNote`).
    pub note: Option<u8>,
    /// That note's velocity, 0..127.
    pub velocity: u8,
    /// Any note held (`isAnyNoteHeld`).
    pub pressed: bool,
}

/// What an `input/camera` node asks for this tick.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraRequest<'a> {
    /// Device id from the device list; empty for the default camera.
    pub device: &'a str,
    /// Capture runs while true; going false stops it (camera light off).
    pub start: bool,
    /// Flip frames horizontally.
    pub mirror: bool,
}

#[derive(Clone, Debug)]
pub struct CameraFrame {
    /// The latest frame, or a 1×1 placeholder until one arrives.
    pub image: Arc<Image>,
    pub ready: bool,
}

/// What an `input/landmarks` node asks for this tick.
#[derive(Clone, Debug)]
pub struct LandmarksRequest {
    /// The frame to track (only ready images are passed).
    pub image: Option<Arc<Image>>,
    /// Models load on the rising edge and are freed on the falling edge.
    pub start: bool,
    /// One tracker, or all three (`tracker: all`), whose person ids are
    /// then unified around the pose.
    pub trackers: Vec<Tracker>,
    /// Max detections per tracker, 1..6.
    pub count: usize,
}

/// One tracker's latest detections.
#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkLayer {
    pub tracker: Tracker,
    /// Per detection: landmarks in normalised image coords (x, y in 0..1,
    /// z scaled like x), MediaPipe's indices.
    pub dets: Vec<Vec<[f32; 3]>>,
    /// Stable id per detection, kept frame to frame.
    pub ids: Vec<u32>,
}

pub trait Devices: Send + Sync {
    /// Latest value (0..127) of `cc` on `device` (empty: any) and `channel`
    /// (0: any), `None` until one arrives (`getCC`).
    fn midi_cc(&self, device: &str, channel: u8, cc: u8) -> Option<u8> {
        let _ = (device, channel, cc);
        None
    }

    fn midi_note(&self, device: &str, channel: u8) -> MidiNote {
        let _ = (device, channel);
        MidiNote::default()
    }

    /// Drive `node`'s camera. `Err` explains why there's no picture (no
    /// camera, permission denied); the node shows it as its error.
    fn camera(&self, node: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        let _ = (node, req);
        Err("no camera support".into())
    }

    /// Drive `node`'s trackers. `Ok(None)` while stopped or loading.
    fn landmarks(
        &self,
        node: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        let _ = (node, req);
        Ok(None)
    }
}

fn slot() -> &'static RwLock<Option<Arc<dyn Devices>>> {
    static DEVICES: RwLock<Option<Arc<dyn Devices>>> = RwLock::new(None);
    &DEVICES
}

/// Install (or remove) the process-wide device provider.
pub fn install(devices: Option<Arc<dyn Devices>>) {
    *slot().write().unwrap_or_else(|e| e.into_inner()) = devices;
}

/// The installed provider, if any.
pub fn installed() -> Option<Arc<dyn Devices>> {
    slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}
