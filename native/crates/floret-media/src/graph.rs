//! The control side: turns each tick's declared [`AudioGraph`] into what
//! the audio thread runs.
//!
//! * **Parameters** go through [`Params`], a block of atomics per node that
//!   the control thread overwrites every tick and the audio thread reads
//!   once per render quantum (then ramps toward). No messages, no locks;
//!   the latest value wins, which is what parameters want.
//! * **Gates** (envelope / synth trigger, sound-file play) are levels in a
//!   param slot plus an edge counter bumped on every rising edge, so an
//!   on/off pair between two quanta still retriggers.
//! * **Topology** (which nodes exist, their wiring, and anything that
//!   needs a rebuilt DSP: wave type, noise colour, reverb IR, a newly
//!   loaded sample) is diffed tick to tick. A change builds a new
//!   [`Program`] here, where allocating is fine, and sends it over a
//!   lock-free ring. Each slot names the slot of the program before it
//!   whose DSP it inherits, so oscillator phases, delay lines and envelope
//!   levels carry over. The audio thread moves those DSPs across and sends
//!   the old program back on another ring to be freed here.

use crate::analysis::{Analyser, Tap};
use crate::decode::{SampleData, Samples};
use crate::dsp::Bus;
use crate::nodes::{slot, Dsp};
use crate::render::MicInput;
use floret_engine::audio::{
    AudioFeedback, AudioGraph, AudioNode, FilterType, MicState, NoiseColor, Oversample, Wave,
};
use floret_engine::host::Host;
use indexmap::IndexMap;
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Programs sent but not yet returned; past this, rebuilds wait a tick.
const MAX_IN_FLIGHT: usize = 4;
const RING: usize = 16;

/// One node's parameters, shared with the audio thread.
pub struct Params {
    vals: [AtomicU32; 8],
    epoch: AtomicU32,
    /// Playhead in seconds (f64 bits), NaN while not playing.
    playhead: AtomicU64,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            vals: Default::default(),
            epoch: AtomicU32::new(0),
            playhead: AtomicU64::new(f64::NAN.to_bits()),
        }
    }
}

impl Params {
    pub fn get(&self, i: usize) -> f32 {
        f32::from_bits(self.vals[i].load(Ordering::Relaxed))
    }

    pub fn set(&self, i: usize, v: f32) {
        self.vals[i].store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }

    fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::Release);
    }

    pub fn set_playhead(&self, seconds: Option<f64>) {
        let v = seconds.unwrap_or(f64::NAN);
        self.playhead.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn playhead(&self) -> Option<f64> {
        let v = f64::from_bits(self.playhead.load(Ordering::Relaxed));
        (!v.is_nan()).then_some(v)
    }
}

/// One node on the audio thread.
pub struct Slot {
    pub dsp: Option<Dsp>,
    /// Upstream slot (always earlier in the program).
    pub input: Option<usize>,
    pub params: Arc<Params>,
    /// Slot of the previous program whose DSP this one takes over.
    pub reuse: Option<usize>,
    pub out: Bus,
    pub is_output: bool,
}

/// What the audio thread runs: slots in dependency order.
#[derive(Default)]
pub struct Program {
    pub slots: Vec<Slot>,
}

pub enum Cmd {
    Program(Box<Program>),
    Mic(Option<Box<MicInput>>),
}

/// Things the audio thread hands back to be freed off-thread.
pub enum Garbage {
    Program(Box<Program>),
    Mic(Box<MicInput>),
}

/// What decides whether a node's DSP can carry over.
#[derive(Clone, Debug, PartialEq)]
enum Key {
    Plain(std::mem::Discriminant<AudioNode>),
    Osc(Wave),
    Noise(NoiseColor, u32),
    Reverb(u64, u64, bool, u32),
    Player(usize),
}

fn key_of(node: &AudioNode, sample: Option<&Arc<SampleData>>) -> Key {
    match node {
        AudioNode::Oscillator { wave, .. } => Key::Osc(*wave),
        AudioNode::Noise { color, seed, .. } => Key::Noise(*color, *seed),
        AudioNode::Reverb {
            seconds,
            decay,
            reverse,
            seed,
            ..
        } => Key::Reverb(seconds.to_bits(), decay.to_bits(), *reverse, *seed),
        AudioNode::SoundFile { .. } => Key::Player(sample.map_or(0, |s| Arc::as_ptr(s) as usize)),
        other => Key::Plain(std::mem::discriminant(other)),
    }
}

struct Node {
    desc: AudioNode,
    params: Arc<Params>,
    tap: Option<Arc<Tap>>,
    sample: Option<Arc<SampleData>>,
    gate: bool,
}

pub struct Control {
    sample_rate: f32,
    cmds: Producer<Cmd>,
    garbage: Consumer<Garbage>,
    in_flight: usize,
    nodes: IndexMap<Arc<str>, Node>,
    /// The topology of the last program sent.
    topology: Vec<(Arc<str>, Key, Option<Arc<str>>)>,
    /// Node → (slot, key) in the last program sent.
    layout: HashMap<Arc<str>, (usize, Key)>,
    samples: Samples,
    host: Option<Arc<dyn Host>>,
    analysers: HashMap<Arc<str>, Analyser>,
    pub mic: MicState,
}

/// The audio-thread ends of a fresh [`Control`].
pub struct Ends {
    pub cmds: Consumer<Cmd>,
    pub garbage: Producer<Garbage>,
}

impl Control {
    pub fn new(sample_rate: f32) -> (Control, Ends) {
        let (cmds, cmd_rx) = RingBuffer::new(RING);
        let (garbage_tx, garbage) = RingBuffer::new(RING * 2);
        let c = Control {
            sample_rate,
            cmds,
            garbage,
            in_flight: 0,
            nodes: IndexMap::new(),
            topology: Vec::new(),
            layout: HashMap::new(),
            samples: Samples::default(),
            host: None,
            analysers: HashMap::new(),
            mic: MicState::Off,
        };
        (
            c,
            Ends {
                cmds: cmd_rx,
                garbage: garbage_tx,
            },
        )
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_host(&mut self, host: Option<Arc<dyn Host>>) {
        self.host = host;
    }

    /// Load sound files on the control thread, before the tick's graph
    /// goes out (see [`Samples::blocking`]).
    pub fn set_blocking_loads(&mut self, on: bool) {
        self.samples.blocking = on;
    }

    /// Start over on a new audio thread (another device / sample rate):
    /// every DSP is rebuilt on the next program.
    pub fn reset(&mut self, sample_rate: f32) -> Ends {
        let (cmds, cmd_rx) = RingBuffer::new(RING);
        let (garbage_tx, garbage) = RingBuffer::new(RING * 2);
        self.sample_rate = sample_rate;
        self.cmds = cmds;
        self.garbage = garbage;
        self.in_flight = 0;
        self.topology.clear();
        self.layout.clear();
        self.rebuild_if_changed();
        Ends {
            cmds: cmd_rx,
            garbage: garbage_tx,
        }
    }

    fn collect_garbage(&mut self) {
        while let Ok(g) = self.garbage.pop() {
            if let Garbage::Program(_) = g {
                self.in_flight = self.in_flight.saturating_sub(1);
            }
        }
    }

    /// Hand the audio thread a microphone stream (or take it away).
    pub fn set_mic(&mut self, mic: Option<Box<MicInput>>) {
        let _ = self.cmds.push(Cmd::Mic(mic));
    }

    /// Does any `sound/audio-in` want the microphone?
    pub fn mic_wanted(&self) -> bool {
        self.nodes
            .values()
            .any(|n| matches!(n.desc, AudioNode::AudioIn { start: true, .. }))
    }

    pub fn submit(&mut self, graph: AudioGraph) {
        self.collect_garbage();
        let live: HashSet<&str> = graph.live.iter().map(|s| &**s).collect();
        self.nodes.retain(|id, _| live.contains(&**id));
        for (id, decl) in graph.declared {
            if !live.contains(&*id) {
                continue;
            }
            match decl {
                None => {
                    self.nodes.shift_remove(&id);
                }
                Some(desc) => match self.nodes.get_mut(&id) {
                    Some(n) => n.desc = desc,
                    None => {
                        let tap = matches!(desc, AudioNode::Analyser { .. })
                            .then(|| Arc::new(Tap::default()));
                        self.nodes.insert(
                            id,
                            Node {
                                desc,
                                params: Arc::default(),
                                tap,
                                sample: None,
                                gate: false,
                            },
                        );
                    }
                },
            }
        }
        for n in self.nodes.values_mut() {
            if let AudioNode::SoundFile { url, .. } = &n.desc {
                self.samples.ensure(url, self.host.clone());
                n.sample = self.samples.get(url);
            }
            write_params(n);
        }
        let urls: HashSet<Arc<str>> = self
            .nodes
            .values()
            .filter_map(|n| match &n.desc {
                AudioNode::SoundFile { url, .. } => Some(url.clone()),
                _ => None,
            })
            .collect();
        self.samples.retain(|u| urls.contains(u));
        self.analysers.retain(|id, _| {
            self.nodes
                .get(id)
                .is_some_and(|n| matches!(n.desc, AudioNode::Analyser { .. }))
        });
        self.rebuild_if_changed();
    }

    fn rebuild_if_changed(&mut self) {
        let mut topology: Vec<(Arc<str>, Key, Option<Arc<str>>)> = self
            .nodes
            .iter()
            .map(|(id, n)| {
                let input = n
                    .desc
                    .input()
                    .filter(|i| self.nodes.contains_key(&***i))
                    .cloned();
                (id.clone(), key_of(&n.desc, n.sample.as_ref()), input)
            })
            .collect();
        topology.sort_by(|a, b| a.0.cmp(&b.0));
        if topology == self.topology || self.in_flight >= MAX_IN_FLIGHT {
            return;
        }
        let (program, layout) = self.build();
        // If the ring is full this is retried next tick.
        if self.cmds.push(Cmd::Program(Box::new(program))).is_ok() {
            self.in_flight += 1;
            self.topology = topology;
            self.layout = layout;
        }
    }

    fn build(&self) -> (Program, HashMap<Arc<str>, (usize, Key)>) {
        // Depth-first so every input precedes its consumer.
        fn visit<'a>(
            id: &'a Arc<str>,
            nodes: &'a IndexMap<Arc<str>, Node>,
            state: &mut HashMap<&'a str, bool>,
            order: &mut Vec<&'a Arc<str>>,
        ) {
            if state.contains_key(&**id) {
                return;
            }
            state.insert(id, false);
            if let Some(input) = nodes[id].desc.input() {
                if let Some((k, _)) = nodes.get_key_value(input) {
                    visit(k, nodes, state, order);
                }
            }
            state.insert(id, true);
            order.push(id);
        }
        let mut state = HashMap::new();
        let mut order = Vec::new();
        for id in self.nodes.keys() {
            visit(id, &self.nodes, &mut state, &mut order);
        }
        let mut layout = HashMap::new();
        let mut slots = Vec::with_capacity(order.len());
        for id in order {
            let n = &self.nodes[id];
            let key = key_of(&n.desc, n.sample.as_ref());
            let reuse = self
                .layout
                .get(id)
                .filter(|(_, k)| *k == key)
                .map(|(i, _)| *i);
            let input = n
                .desc
                .input()
                .and_then(|i| layout.get(i))
                .map(|(i, _): &(usize, Key)| *i);
            slots.push(Slot {
                dsp: reuse
                    .is_none()
                    .then(|| Dsp::new(&n.desc, self.sample_rate, n.sample.clone(), n.tap.clone())),
                input,
                params: n.params.clone(),
                reuse,
                out: Bus::new(),
                is_output: matches!(n.desc, AudioNode::Output { .. }),
            });
            layout.insert(id.clone(), (slots.len() - 1, key));
        }
        (Program { slots }, layout)
    }

    /// The analysis, loading and playback state for the next tick.
    pub fn feedback(&mut self) -> AudioFeedback {
        self.collect_garbage();
        let mut fb = AudioFeedback {
            sample_rate: f64::from(self.sample_rate),
            mic: self.mic.clone(),
            samples: self.samples.states(),
            ..Default::default()
        };
        for (id, n) in &self.nodes {
            match &n.desc {
                AudioNode::Analyser {
                    fft_size,
                    smoothing,
                    ..
                } => {
                    if let Some(tap) = &n.tap {
                        let a = self.analysers.entry(id.clone()).or_default();
                        fb.analysers
                            .insert(id.clone(), a.analyse(tap, *fft_size, *smoothing));
                    }
                }
                AudioNode::SoundFile { .. } => {
                    if let Some(t) = n.params.playhead() {
                        fb.playheads.insert(id.clone(), t);
                    }
                }
                _ => {}
            }
        }
        fb
    }
}

/// Copy a declaration into its param block; turn gate edges into epochs.
fn write_params(n: &mut Node) {
    let p = &n.params;
    let set = |vals: &[f64]| {
        for (i, v) in vals.iter().enumerate() {
            p.set(i, *v as f32);
        }
    };
    let gate = match &n.desc {
        AudioNode::Oscillator { freq, amp, .. } => {
            set(&[*freq, *amp]);
            None
        }
        AudioNode::Pulse { freq, amp, width } => {
            set(&[*freq, *amp, *width]);
            None
        }
        AudioNode::Noise { amp, .. }
        | AudioNode::AudioIn { amp, .. }
        | AudioNode::Gain { level: amp, .. }
        | AudioNode::Output { volume: amp, .. }
        | AudioNode::Reverb { wet: amp, .. } => {
            set(&[*amp]);
            None
        }
        AudioNode::MonoSynth {
            freq,
            velocity,
            attack,
            decay,
            sustain,
            release,
            gate,
        } => {
            set(&[*freq, *velocity, *attack, *decay, *sustain, *release]);
            Some(*gate)
        }
        AudioNode::SoundFile {
            rate,
            pan,
            amp,
            looping,
            play,
            ..
        } => {
            set(&[*rate, *pan, *amp, f64::from(u8::from(*looping))]);
            // Like TS, play edges only count once the file has loaded.
            Some(*play && n.sample.is_some())
        }
        AudioNode::Filter { kind, freq, q, .. } => {
            let k = match kind {
                FilterType::Lowpass => 0.0,
                FilterType::Highpass => 1.0,
                FilterType::Bandpass => 2.0,
            };
            set(&[*freq, *q, k]);
            None
        }
        AudioNode::Delay {
            time,
            feedback,
            damping,
            ..
        } => {
            set(&[*time, *feedback, *damping]);
            None
        }
        AudioNode::Distortion {
            amount,
            wet,
            oversample,
            ..
        } => {
            let f = match oversample {
                Oversample::None => 1.0,
                Oversample::X2 => 2.0,
                Oversample::X4 => 4.0,
            };
            set(&[*amount, *wet, f]);
            None
        }
        AudioNode::Compressor {
            threshold,
            ratio,
            knee,
            attack,
            release,
            ..
        } => {
            set(&[*threshold, *ratio, *knee, *attack, *release]);
            None
        }
        AudioNode::Envelope {
            attack,
            decay,
            sustain,
            release,
            level,
            gate,
            ..
        } => {
            set(&[*attack, *decay, *sustain, *release, *level]);
            Some(*gate)
        }
        AudioNode::Analyser { .. } => None,
    };
    if let Some(open) = gate {
        // Level first: the audio thread reads the epoch, then the level.
        p.set(slot::GATE, if open { 1.0 } else { 0.0 });
        if open && !n.gate {
            p.bump_epoch();
        }
        n.gate = open;
    }
}
