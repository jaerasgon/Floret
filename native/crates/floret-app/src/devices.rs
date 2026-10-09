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

/// Install the devices as the engine's provider, and the local time zone
/// for the Clock node.
pub fn install() {
    shared();
    #[cfg(unix)]
    floret_engine::nodes::time::install_local_offset(local_offset);
}

/// The local time zone's offset from UTC at a Unix time, in seconds
/// (`localtime_r`, so daylight saving and `TZ` count). Other platforms
/// read `local` as UTC; the Clock's `offset` pin moves it.
#[cfg(unix)]
// `time_t` and `c_long` are i64 on 64-bit targets but not everywhere.
#[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
fn local_offset(unix_seconds: i64) -> i64 {
    let t = unix_seconds as libc::time_t;
    // SAFETY: `tm` is plain data that `localtime_r` fills in; both
    // pointers are valid for the call and it keeps neither.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::localtime_r(&t, &mut tm) };
    if r.is_null() {
        0
    } else {
        i64::from(tm.tm_gmtoff)
    }
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
