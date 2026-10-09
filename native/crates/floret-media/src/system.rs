//! [`AudioSystem`]: the engine's [`AudioBackend`], tying the control side,
//! the renderer and the devices together.

use crate::device::{DeviceCmd, DeviceThread};
use crate::graph::Control;
use crate::render::{Master, MicInput, Renderer};
use floret_engine::audio::{AudioBackend, AudioFeedback, AudioGraph, MicState};
use floret_engine::host::Host;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// The rate used until a device says otherwise (and with no device).
pub const DEFAULT_SAMPLE_RATE: f32 = 48000.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceInfo {
    /// cpal's device id (`host:id`), stable across runs.
    pub id: String,
    pub name: String,
}

/// What the app shows about audio.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// The output device in use; `None` when running without one.
    pub output: Option<String>,
    /// Why there is no output device (or the last device error).
    pub error: Option<String>,
    pub outputs: Vec<DeviceInfo>,
    pub inputs: Vec<DeviceInfo>,
    /// Chosen devices (`None` = the system default).
    pub selected_output: Option<String>,
    pub selected_input: Option<String>,
    pub sample_rate: f32,
    pub mic: MicState,
}

pub(crate) struct Shared {
    pub control: Mutex<Control>,
    pub renderer: Mutex<Arc<Mutex<Renderer>>>,
    pub master: Arc<Master>,
    pub status: Mutex<Status>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn new(sample_rate: f32) -> Arc<Shared> {
        let (control, ends) = Control::new(sample_rate);
        let master = Arc::new(Master::default());
        let renderer = Renderer::new(sample_rate, ends, master.clone());
        Arc::new(Shared {
            control: Mutex::new(control),
            renderer: Mutex::new(Arc::new(Mutex::new(renderer))),
            master,
            status: Mutex::new(Status {
                sample_rate,
                ..Default::default()
            }),
        })
    }

    /// The renderer for a device at `sample_rate`, replacing the current
    /// one (and rebuilding every DSP) if the rate differs.
    pub fn renderer_for(&self, sample_rate: f32) -> Arc<Mutex<Renderer>> {
        let mut slot = lock(&self.renderer);
        if lock(&slot).sample_rate() == sample_rate {
            return slot.clone();
        }
        let ends = lock(&self.control).reset(sample_rate);
        let r = Arc::new(Mutex::new(Renderer::new(
            sample_rate,
            ends,
            self.master.clone(),
        )));
        *slot = r.clone();
        lock(&self.status).sample_rate = sample_rate;
        r
    }

    pub fn renderer(&self) -> Arc<Mutex<Renderer>> {
        lock(&self.renderer).clone()
    }

    pub fn set_mic_state(&self, state: MicState) {
        lock(&self.control).mic = state.clone();
        lock(&self.status).mic = state;
    }
}

/// The audio engine. [`AudioSystem::start`] runs it on the default output
/// device (or silently on a timer when there is none);
/// [`AudioSystem::offline`] renders on demand for tests and tools.
pub struct AudioSystem {
    shared: Arc<Shared>,
    devices: Option<DeviceThread>,
    mic_wanted: AtomicBool,
}

impl AudioSystem {
    /// Start on the default devices. Never fails: without a usable output
    /// device the graph still runs (analysers keep working) on a timer.
    pub fn start() -> AudioSystem {
        let shared = Shared::new(DEFAULT_SAMPLE_RATE);
        let devices = Some(DeviceThread::spawn(shared.clone()));
        AudioSystem {
            shared,
            devices,
            mic_wanted: AtomicBool::new(false),
        }
    }

    /// No devices: audio is produced only by [`AudioSystem::render`].
    pub fn offline(sample_rate: f32) -> AudioSystem {
        AudioSystem {
            shared: Shared::new(sample_rate),
            devices: None,
            mic_wanted: AtomicBool::new(false),
        }
    }

    /// For rendering a patch's sound into a file (video export): offline,
    /// and sound files load as the tick that first plays them is submitted
    /// rather than in the background, so the result never depends on how
    /// fast a file happened to load. Render each frame's share with
    /// [`AudioSystem::render_into`] right after ticking it.
    pub fn for_export(sample_rate: f32) -> AudioSystem {
        let sys = AudioSystem::offline(sample_rate);
        lock(&sys.shared.control).set_blocking_loads(true);
        sys
    }

    /// Render `frames` stereo frames (offline use; with devices running
    /// this steals their audio).
    pub fn render(&self, frames: usize) -> Vec<[f32; 2]> {
        let mut buf = vec![0f32; frames * 2];
        lock(&self.shared.renderer()).render(&mut buf, 2);
        buf.chunks(2).map(|c| [c[0], c[1]]).collect()
    }

    /// Render into an interleaved stereo buffer, as the audio thread does.
    pub fn render_into(&self, buf: &mut [f32]) {
        let r = lock(&self.shared.renderer).clone();
        lock(&r).render(buf, 2);
    }

    /// Lend the loader the app's fetcher (http(s) sound files).
    pub fn set_host(&self, host: Option<Arc<dyn Host>>) {
        lock(&self.shared.control).set_host(host);
    }

    pub fn master(&self) -> &Master {
        &self.shared.master
    }

    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }

    pub fn sample_rate(&self) -> f32 {
        lock(&self.shared.control).sample_rate()
    }

    fn send(&self, cmd: DeviceCmd) {
        if let Some(d) = &self.devices {
            d.send(cmd);
        }
    }

    /// Use output device `id` (`None` = system default).
    pub fn set_output_device(&self, id: Option<String>) {
        lock(&self.shared.status).selected_output = id.clone();
        self.send(DeviceCmd::Output(id));
    }

    /// Use input device `id` for the microphone (`None` = system default).
    pub fn set_input_device(&self, id: Option<String>) {
        lock(&self.shared.status).selected_input = id.clone();
        self.send(DeviceCmd::Input(id));
    }

    /// Re-list the devices.
    pub fn refresh_devices(&self) {
        self.send(DeviceCmd::Refresh);
    }

    /// Silence everything (a new document): drop the whole graph.
    pub fn clear(&self) {
        self.submit(AudioGraph::default());
    }

    /// Offline: feed the microphone from a ring the caller fills, as if
    /// the input device were open at `input_rate`.
    pub fn attach_test_mic(&self, input_rate: f32) -> rtrb::Producer<f32> {
        let (tx, rx) = rtrb::RingBuffer::new(input_rate as usize);
        let mic = MicInput::new(rx, input_rate, self.sample_rate());
        lock(&self.shared.control).set_mic(Some(Box::new(mic)));
        self.shared.set_mic_state(MicState::Live);
        tx
    }
}

impl AudioBackend for AudioSystem {
    fn feedback(&self) -> Arc<AudioFeedback> {
        Arc::new(lock(&self.shared.control).feedback())
    }

    fn submit(&self, graph: AudioGraph) {
        let wanted = {
            let mut c = lock(&self.shared.control);
            c.submit(graph);
            c.mic_wanted()
        };
        if self.mic_wanted.swap(wanted, Ordering::Relaxed) != wanted {
            self.send(DeviceCmd::Mic(wanted));
        }
    }
}
