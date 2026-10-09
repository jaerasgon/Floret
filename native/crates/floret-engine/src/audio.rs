//! The audio graph that `sound/*` nodes declare, and what flows back.
//!
//! In TS each sound node builds and pokes Web Audio nodes directly. Here the
//! engine stays pure: every tick, each sound node *declares* what it is
//! ([`AudioNode`]: kind, parameters, upstream node id) and wires out an
//! [`AudioRef`] naming itself. At the end of the tick the whole
//! [`AudioGraph`] goes to the app's [`AudioBackend`] (`floret-media`), which
//! diffs it against what is playing and updates the audio thread.
//!
//! Analysis flows the other way: at the start of each tick the backend
//! hands over an [`AudioFeedback`] snapshot (analyser data, microphone and
//! sample-loading state, playheads) that `sound/amplitude`, `sound/fft`,
//! `sound/peak-detect`, `sound/audio-in` and `sound/sound-file` read.
//!
//! Plumbing mirrors [`crate::host`]: `Engine::tick` installs its backend in
//! a thread-local for the duration of the tick. With no backend installed
//! the sound nodes fail like the TS ones outside a browser (no
//! `AudioContext`), so they emit empty outputs — what the golden traces
//! recorded.
//!
//! A node that exists in the patch but isn't evaluated on a tick (an
//! inactive branch, a cache hit) keeps its last declaration, like a Web
//! Audio node that stays alive until the node is deleted. Fan-out is free:
//! any number of nodes may name the same upstream.

use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The value on an `audio` wire: which sound node's output this is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioRef {
    pub node: Arc<str>,
    /// The TS `AudioHandle.kind` (`"oscillator"`, `"filter"`, …).
    pub kind: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wave {
    Sine,
    Triangle,
    Sawtooth,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoiseColor {
    White,
    Pink,
    Brown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Oversample {
    None,
    X2,
    X4,
}

/// One sound node's declaration. Numbers are already clamped the way the
/// TS node clamps them before handing them to Web Audio. `input` is the
/// upstream node id (`None` when unwired). Gates (`gate`, `play`, `start`)
/// are levels; the backend turns their edges into triggers.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioNode {
    Oscillator {
        wave: Wave,
        freq: f64,
        amp: f64,
    },
    /// Sawtooth through a step waveshaper (`threshold = 2·width − 1`).
    Pulse {
        freq: f64,
        amp: f64,
        width: f64,
    },
    /// A looping 2 s noise buffer from `seed`.
    Noise {
        color: NoiseColor,
        amp: f64,
        seed: u32,
    },
    /// Sawtooth voice through an ADSR gain peaking at `velocity`.
    MonoSynth {
        freq: f64,
        velocity: f64,
        attack: f64,
        decay: f64,
        sustain: f64,
        release: f64,
        gate: bool,
    },
    /// Buffer player → equal-power panner → gain. Rising `play` starts at 0,
    /// falling stops.
    SoundFile {
        url: Arc<str>,
        rate: f64,
        pan: f64,
        amp: f64,
        looping: bool,
        play: bool,
    },
    /// Microphone → gain. The backend opens the input device while any
    /// `AudioIn` has `start`.
    AudioIn {
        amp: f64,
        start: bool,
    },
    Gain {
        input: Option<Arc<str>>,
        level: f64,
    },
    /// Web Audio `BiquadFilterNode` (`q` is in dB for low/highpass).
    Filter {
        input: Option<Arc<str>>,
        kind: FilterType,
        freq: f64,
        q: f64,
    },
    /// Feedback echo: dry + lowpass(delay), with the lowpassed signal fed
    /// back by `feedback`.
    Delay {
        input: Option<Arc<str>>,
        time: f64,
        feedback: f64,
        damping: f64,
    },
    /// Convolution with a seeded, exponentially decaying stereo noise IR.
    Reverb {
        input: Option<Arc<str>>,
        seconds: f64,
        decay: f64,
        reverse: bool,
        wet: f64,
        seed: u32,
    },
    Distortion {
        input: Option<Arc<str>>,
        amount: f64,
        wet: f64,
        oversample: Oversample,
    },
    /// Web Audio `DynamicsCompressorNode`.
    Compressor {
        input: Option<Arc<str>>,
        threshold: f64,
        ratio: f64,
        knee: f64,
        attack: f64,
        release: f64,
    },
    /// ADSR gain on the input, peaking at `level`.
    Envelope {
        input: Option<Arc<str>>,
        attack: f64,
        decay: f64,
        sustain: f64,
        release: f64,
        level: f64,
        gate: bool,
    },
    /// An `AnalyserNode` tap; its data comes back in
    /// [`AudioFeedback::analysers`].
    Analyser {
        input: Option<Arc<str>>,
        fft_size: usize,
        smoothing: f64,
    },
    /// Sums `input × volume` into the speakers.
    Output {
        input: Option<Arc<str>>,
        volume: f64,
    },
    /// Sums several inputs, × `level` (`sound/mix`): one chain for many
    /// voices, so they can share an effect or an analyser.
    Mix {
        inputs: Vec<Arc<str>>,
        level: f64,
    },
}

impl AudioNode {
    /// Every upstream node this one reads: [`AudioNode::input`], or a
    /// Mix's inputs.
    pub fn sources(&self) -> impl Iterator<Item = &Arc<str>> {
        let mix: &[Arc<str>] = match self {
            AudioNode::Mix { inputs, .. } => inputs,
            _ => &[],
        };
        self.input().into_iter().chain(mix)
    }

    /// The single upstream node this one reads, if any (a Mix reads
    /// several: see [`AudioNode::sources`]).
    pub fn input(&self) -> Option<&Arc<str>> {
        use AudioNode::*;
        match self {
            Gain { input, .. }
            | Filter { input, .. }
            | Delay { input, .. }
            | Reverb { input, .. }
            | Distortion { input, .. }
            | Compressor { input, .. }
            | Envelope { input, .. }
            | Analyser { input, .. }
            | Output { input, .. } => input.as_ref(),
            Oscillator { .. }
            | Pulse { .. }
            | Noise { .. }
            | MonoSynth { .. }
            | SoundFile { .. }
            | AudioIn { .. }
            | Mix { .. } => None,
        }
    }
}

/// Everything the sound nodes declared on one tick.
#[derive(Clone, Debug, Default)]
pub struct AudioGraph {
    /// Declarations in evaluation order (a node evaluated more than once,
    /// inside a ForEach, keeps its last). `None`: the node has no audio
    /// right now (a SoundFile with no URL).
    pub declared: IndexMap<Arc<str>, Option<AudioNode>>,
    /// Every sound node in the patch. A live node that wasn't declared this
    /// tick keeps its previous declaration; anything not live is removed.
    pub live: Vec<Arc<str>>,
}

/// An `AnalyserNode`'s view of its input when the tick started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    /// `getFloatTimeDomainData`: the last `fft_size` samples.
    pub time: Arc<[f32]>,
    /// `getByteFrequencyData`: `fft_size / 2` bins, smoothed, 0..255.
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum MicState {
    #[default]
    Off,
    /// Waiting for the device (or the OS permission prompt).
    Starting,
    Live,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SampleState {
    Loading,
    Ready { duration: f64 },
    Failed(String),
}

/// What the backend reports back at the start of a tick.
#[derive(Clone, Debug, Default)]
pub struct AudioFeedback {
    /// The audio context's sample rate (Hz).
    pub sample_rate: f64,
    /// Per analyser node id.
    pub analysers: HashMap<Arc<str>, Analysis>,
    pub mic: MicState,
    /// Per sound-file URL.
    pub samples: HashMap<Arc<str>, SampleState>,
    /// Playhead (seconds) per sound-file node id, while it plays.
    pub playheads: HashMap<Arc<str>, f64>,
}

/// The app's audio engine (`floret-media`).
pub trait AudioBackend: Send + Sync {
    /// Called as a tick starts.
    fn feedback(&self) -> Arc<AudioFeedback>;
    /// Called as a tick ends, with every declaration made during it.
    fn submit(&self, graph: AudioGraph);
}

struct TickAudio {
    backend: Arc<dyn AudioBackend>,
    feedback: Arc<AudioFeedback>,
    graph: AudioGraph,
}

thread_local! {
    static CURRENT: RefCell<Option<TickAudio>> = const { RefCell::new(None) };
}

/// The feedback for the tick running on this thread; `None` without a
/// backend.
pub fn feedback() -> Option<Arc<AudioFeedback>> {
    CURRENT.with(|c| c.borrow().as_ref().map(|t| t.feedback.clone()))
}

/// Record `node`'s audio for this tick (no-op without a backend).
pub fn declare(node: &str, audio: Option<AudioNode>) {
    CURRENT.with(|c| {
        if let Some(t) = c.borrow_mut().as_mut() {
            t.graph.declared.insert(Arc::from(node), audio);
        }
    });
}

/// Installs a backend for the current thread's tick. Dropping the guard
/// submits the graph collected meanwhile (unless unwinding from a panic)
/// and restores the previously installed one.
pub struct AudioGuard(Option<Option<TickAudio>>);

/// Start a tick with `backend` (if any); `live` lists the patch's sound
/// node ids.
pub fn begin(backend: Option<Arc<dyn AudioBackend>>, live: Vec<Arc<str>>) -> AudioGuard {
    let Some(backend) = backend else {
        return AudioGuard(Some(CURRENT.with(|c| c.replace(None))));
    };
    let tick = TickAudio {
        feedback: backend.feedback(),
        backend,
        graph: AudioGraph {
            declared: IndexMap::new(),
            live,
        },
    };
    AudioGuard(Some(CURRENT.with(|c| c.replace(Some(tick)))))
}

impl Drop for AudioGuard {
    fn drop(&mut self) {
        let prev = self.0.take().flatten();
        let done = CURRENT.with(|c| c.replace(prev));
        if let Some(t) = done {
            if !std::thread::panicking() {
                t.backend.submit(t.graph);
            }
        }
    }
}

/// A backend that records what it is sent and replays fixed feedback, for
/// tests.
#[derive(Default)]
pub struct RecordingBackend {
    pub feedback: std::sync::Mutex<AudioFeedback>,
    pub graphs: std::sync::Mutex<Vec<AudioGraph>>,
}

impl AudioBackend for RecordingBackend {
    fn feedback(&self) -> Arc<AudioFeedback> {
        Arc::new(
            self.feedback
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        )
    }

    fn submit(&self, graph: AudioGraph) {
        self.graphs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(graph);
    }
}

impl RecordingBackend {
    /// The most recently submitted graph.
    pub fn last(&self) -> Option<AudioGraph> {
        self.graphs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_collects_and_submits() {
        let rec = Arc::new(RecordingBackend::default());
        assert!(feedback().is_none());
        {
            let _g = begin(Some(rec.clone()), vec![Arc::from("a")]);
            assert!(feedback().is_some());
            declare(
                "a",
                Some(AudioNode::AudioIn {
                    amp: 0.5,
                    start: false,
                }),
            );
            declare(
                "a",
                Some(AudioNode::AudioIn {
                    amp: 0.25,
                    start: true,
                }),
            );
        }
        assert!(feedback().is_none());
        let g = rec.last().unwrap();
        assert_eq!(g.declared.len(), 1);
        assert_eq!(
            g.declared["a"],
            Some(AudioNode::AudioIn {
                amp: 0.25,
                start: true
            })
        );
        // Without a backend declarations go nowhere.
        let _g = begin(None, vec![]);
        declare("b", None);
        assert!(feedback().is_none());
    }
}
