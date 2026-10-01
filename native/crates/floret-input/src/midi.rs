//! MIDI input (`midi.ts`): per-device "latest value" state for CC and
//! held notes, fed by every connected input port.
//!
//! A watcher thread rescans ports every second, connecting new boards and
//! dropping state for unplugged ones, so the device list and values follow
//! hot-plugging. Devices are identified by port name (with ` #2`, ` #3`…
//! for identical boards), which is also what patches store in `device`.

use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[derive(Default)]
struct DeviceState {
    /// `(channel 1..16, cc)` → value.
    cc: HashMap<(u8, u8), u8>,
    /// `(channel, note)` → velocity, while held.
    velocity: HashMap<(u8, u8), u8>,
    /// Per channel, held notes in press order (last = most recent). Kept in
    /// first-use order, which "any channel" reads follow, like the TS maps.
    held: IndexMap<u8, Vec<u8>>,
}

/// Every device's state, in connection order.
#[derive(Default)]
pub struct MidiState {
    devices: IndexMap<String, DeviceState>,
}

impl MidiState {
    fn matching<'a>(&'a self, device: &str) -> Box<dyn Iterator<Item = &'a DeviceState> + 'a> {
        if device.is_empty() {
            Box::new(self.devices.values())
        } else {
            Box::new(self.devices.get(device).into_iter())
        }
    }

    /// Apply one MIDI message from `device`.
    pub fn handle(&mut self, device: &str, data: &[u8]) {
        let (Some(&status), Some(&d1)) = (data.first(), data.get(1)) else {
            return;
        };
        let d2 = data.get(2).copied().unwrap_or(0);
        let channel = (status & 0x0f) + 1;
        let ds = self.devices.entry(device.to_owned()).or_default();
        match status & 0xf0 {
            0xb0 => {
                ds.cc.insert((channel, d1), d2);
            }
            // Note-on with velocity 0 is the conventional note-off.
            0x90 if d2 > 0 => {
                ds.velocity.insert((channel, d1), d2);
                let stack = ds.held.entry(channel).or_default();
                stack.retain(|&n| n != d1);
                stack.push(d1);
            }
            0x80 | 0x90 => {
                ds.velocity.remove(&(channel, d1));
                if let Some(stack) = ds.held.get_mut(&channel) {
                    stack.retain(|&n| n != d1);
                }
            }
            _ => {}
        }
    }

    /// Forget a device (unplugged), so it stops reporting stale values.
    pub fn remove(&mut self, device: &str) {
        self.devices.shift_remove(device);
    }

    /// `getCC`: the last match wins when reading any device / channel.
    pub fn cc(&self, device: &str, channel: u8, cc: u8) -> Option<u8> {
        let mut v = None;
        for ds in self.matching(device) {
            let channels = if channel == 0 {
                1..=16
            } else {
                channel..=channel
            };
            for ch in channels {
                if let Some(&got) = ds.cc.get(&(ch, cc)) {
                    v = Some(got);
                }
            }
        }
        v
    }

    /// `getLastHeldNote` / `getNoteVelocity` / `isAnyNoteHeld`.
    pub fn note(&self, device: &str, channel: u8) -> floret_engine::devices::MidiNote {
        let mut note = None;
        let mut pressed = false;
        for ds in self.matching(device) {
            for (&ch, stack) in &ds.held {
                if channel != 0 && ch != channel {
                    continue;
                }
                if let Some(&top) = stack.last() {
                    note = Some(top);
                    pressed = true;
                }
            }
        }
        let velocity = note.map_or(0, |n| {
            self.matching(device)
                .find_map(|ds| {
                    let channels = if channel == 0 {
                        1..=16
                    } else {
                        channel..=channel
                    };
                    channels
                        .into_iter()
                        .find_map(|ch| ds.velocity.get(&(ch, n)).copied())
                })
                .unwrap_or(0)
        });
        floret_engine::devices::MidiNote {
            note,
            velocity,
            pressed,
        }
    }
}

/// Port names made unique (`Name`, `Name #2`, …), in port order.
pub fn unique_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    names
        .into_iter()
        .map(|n| {
            let k = seen.entry(n.clone()).or_insert(0);
            *k += 1;
            if *k == 1 {
                n
            } else {
                format!("{n} #{k}")
            }
        })
        .collect()
}

#[derive(Default)]
struct Shared {
    state: Mutex<MidiState>,
    /// Connected input names, for the device dropdown.
    names: Mutex<Vec<String>>,
    /// Why MIDI isn't available, if it isn't.
    error: Mutex<Option<String>>,
}

/// MIDI input across all connected ports. The watcher starts on first use.
#[derive(Default)]
pub struct Midi {
    shared: Arc<Shared>,
    started: OnceLock<()>,
}

impl Midi {
    fn ensure_started(&self) {
        self.started.get_or_init(|| {
            let shared = self.shared.clone();
            let spawned = std::thread::Builder::new()
                .name("floret-midi".into())
                .spawn(move || watch(&shared));
            if let Err(e) = spawned {
                *self.shared.error.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(format!("MIDI watcher failed to start: {e}"));
            }
        });
    }

    pub fn cc(&self, device: &str, channel: u8, cc: u8) -> Option<u8> {
        self.ensure_started();
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cc(device, channel, cc)
    }

    pub fn note(&self, device: &str, channel: u8) -> floret_engine::devices::MidiNote {
        self.ensure_started();
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .note(device, channel)
    }

    /// Connected input devices (ids = display names).
    pub fn devices(&self) -> Vec<String> {
        self.ensure_started();
        self.shared
            .names
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Why MIDI is unavailable (no ALSA sequencer, …), if it is.
    pub fn error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

const CLIENT: &str = "Floret";

/// Without the ALSA sequencer device libasound fails, printing to stderr
/// on every retry; check for it first.
fn sequencer_present() -> bool {
    !cfg!(target_os = "linux") || std::path::Path::new("/dev/snd/seq").exists()
}

/// Rescan ports forever, keeping one connection per port.
fn watch(shared: &Arc<Shared>) {
    let mut conns: IndexMap<String, midir::MidiInputConnection<()>> = IndexMap::new();
    let mut scanner: Option<midir::MidiInput> = None;
    loop {
        if scanner.is_none() && !sequencer_present() {
            *shared.error.lock().unwrap_or_else(|e| e.into_inner()) =
                Some("MIDI unavailable: no ALSA sequencer (/dev/snd/seq)".into());
        } else if scanner.is_none() {
            scanner = midir::MidiInput::new(CLIENT)
                .map_err(|e| {
                    *shared.error.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(format!("MIDI unavailable: {e}"));
                })
                .ok();
        }
        match &scanner {
            Some(scan) => {
                *shared.error.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let ports = scan.ports();
                let raw: Vec<String> = ports
                    .iter()
                    .map(|p| scan.port_name(p).unwrap_or_else(|_| "MIDI input".into()))
                    .collect();
                let names = unique_names(raw);
                conns.retain(|name, _| {
                    let keep = names.contains(name);
                    if !keep {
                        shared
                            .state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(name);
                    }
                    keep
                });
                for (i, name) in names.iter().enumerate() {
                    if conns.contains_key(name) {
                        continue;
                    }
                    if let Some(c) = connect(shared, &ports[i], name) {
                        conns.insert(name.clone(), c);
                    }
                }
                *shared.names.lock().unwrap_or_else(|e| e.into_inner()) =
                    conns.keys().cloned().collect();
            }
            None => std::thread::sleep(Duration::from_secs(4)),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn connect(
    shared: &Arc<Shared>,
    port: &midir::MidiInputPort,
    name: &str,
) -> Option<midir::MidiInputConnection<()>> {
    let mut input = midir::MidiInput::new(CLIENT).ok()?;
    input.ignore(midir::Ignore::All);
    let s = shared.clone();
    let device = name.to_owned();
    input
        .connect(
            port,
            "floret-in",
            move |_, msg, _| {
                s.state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .handle(&device, msg);
            },
            (),
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc_reads_last_value_per_device_and_channel() {
        let mut m = MidiState::default();
        assert_eq!(m.cc("", 0, 7), None);
        m.handle("A", &[0xb0, 7, 100]); // ch 1
        m.handle("A", &[0xb3, 7, 20]); // ch 4
        m.handle("B", &[0xb0, 7, 5]);
        assert_eq!(m.cc("A", 1, 7), Some(100));
        assert_eq!(m.cc("A", 4, 7), Some(20));
        // Any channel: the highest channel with a value wins.
        assert_eq!(m.cc("A", 0, 7), Some(20));
        // Any device: the last device in connection order wins.
        assert_eq!(m.cc("", 1, 7), Some(5));
        assert_eq!(m.cc("C", 0, 7), None);
        m.remove("B");
        assert_eq!(m.cc("", 1, 7), Some(100));
        // Short / unknown messages are ignored.
        m.handle("A", &[0xb0]);
        m.handle("A", &[0xe0, 0, 64]);
        assert_eq!(m.cc("A", 1, 7), Some(100));
    }

    #[test]
    fn notes_stack_monophonically() {
        let mut m = MidiState::default();
        let on = |n, v| [0x90, n, v];
        m.handle("K", &on(60, 90));
        m.handle("K", &on(64, 70));
        let s = m.note("", 0);
        assert_eq!((s.note, s.velocity, s.pressed), (Some(64), 70, true));
        // Releasing E (note-on, velocity 0) falls back to C.
        m.handle("K", &on(64, 0));
        assert_eq!(m.note("K", 1).note, Some(60));
        assert_eq!(m.note("K", 1).velocity, 90);
        // Retrigger moves C to the top rather than duplicating it.
        m.handle("K", &on(62, 50));
        m.handle("K", &on(60, 100));
        m.handle("K", &[0x80, 60, 0]);
        assert_eq!(m.note("K", 0).note, Some(62));
        m.handle("K", &[0x80, 62, 0]);
        let s = m.note("K", 0);
        assert_eq!((s.note, s.velocity, s.pressed), (None, 0, false));
        // Another channel isn't seen by a channel-2 read.
        m.handle("K", &[0x91, 48, 10]);
        assert_eq!(m.note("K", 1).note, None);
        assert_eq!(m.note("K", 2).note, Some(48));
    }

    #[test]
    fn duplicate_port_names_are_numbered() {
        let names = unique_names(["Pad".into(), "Keys".into(), "Pad".into()]);
        assert_eq!(names, ["Pad", "Keys", "Pad #2"]);
    }
}
