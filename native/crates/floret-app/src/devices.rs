//! What the app lends the engine for device nodes (`floret_engine::devices`):
//! MIDI, cameras and landmark tracking from `floret-input`. One shared
//! instance, installed at startup; nothing touches hardware until a node
//! (or a device dropdown) asks.

use floret_input::InputDevices;
use std::sync::{Arc, OnceLock};

fn shared() -> &'static Arc<InputDevices> {
    static DEVICES: OnceLock<Arc<InputDevices>> = OnceLock::new();
    DEVICES.get_or_init(floret_input::install)
}

/// Install the devices as the engine's provider.
pub fn install() {
    shared();
}

/// Choices for a device node's `device` param: `""` (the default camera /
/// any MIDI input) first, then what's connected now.
pub fn options(node_type: &str, key: &str) -> Option<Vec<String>> {
    if key != "device" {
        return None;
    }
    let connected = match node_type {
        "input/midi-cc" | "input/midi-note" => shared().midi.devices(),
        "input/camera" => shared().cameras.devices(),
        _ => return None,
    };
    Some(std::iter::once(String::new()).chain(connected).collect())
}

/// Why a node type's devices are unavailable (shown on hover), if they are.
pub fn status(node_type: &str) -> Option<String> {
    match node_type {
        "input/midi-cc" | "input/midi-note" => shared().midi.error().or_else(|| {
            shared()
                .midi
                .devices()
                .is_empty()
                .then(|| "No MIDI input connected.".to_owned())
        }),
        "input/camera" => shared()
            .cameras
            .devices()
            .is_empty()
            .then(|| "No camera found.".to_owned()),
        _ => None,
    }
}
