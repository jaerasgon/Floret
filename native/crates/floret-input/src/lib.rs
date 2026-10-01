//! Floret device input: MIDI ([`midi`]), cameras ([`camera`]) and hand /
//! face / pose tracking ([`tracking`]), behind the engine's
//! [`Devices`](floret_engine::devices::Devices) trait.
//!
//! The app creates one [`InputDevices`] and installs it with [`install`].
//! Nothing touches hardware until a node asks: the MIDI watcher starts on
//! the first MIDI read or device listing, a camera opens on its node's
//! `start`, tracking models load on the landmarks node's `start`.

pub mod camera;
pub mod midi;
pub mod nn;
pub mod tracking;

use floret_engine::devices::{
    CameraFrame, CameraRequest, Devices, LandmarkLayer, LandmarksRequest, MidiNote,
};
use std::sync::Arc;

#[derive(Default)]
pub struct InputDevices {
    pub midi: midi::Midi,
    pub cameras: camera::Cameras,
    pub trackers: tracking::Trackers,
}

impl Devices for InputDevices {
    fn midi_cc(&self, device: &str, channel: u8, cc: u8) -> Option<u8> {
        self.midi.cc(device, channel, cc)
    }

    fn midi_note(&self, device: &str, channel: u8) -> MidiNote {
        self.midi.note(device, channel)
    }

    fn camera(&self, node: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        self.cameras.request(node, req)
    }

    fn landmarks(
        &self,
        node: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        self.trackers.request(node, req)
    }
}

/// Create the process's devices and install them as the engine's provider.
pub fn install() -> Arc<InputDevices> {
    let devices = Arc::new(InputDevices::default());
    floret_engine::devices::install(Some(devices.clone()));
    devices
}
