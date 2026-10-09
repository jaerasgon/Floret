//! The audio thread: runs the current [`Program`] one render quantum at a
//! time and mixes every `sound/output` into the device buffer. Nothing in
//! here allocates, frees or blocks: programs and microphone streams arrive
//! over a ring and leave over another one.

use crate::dsp::{Bus, Ramp, QUANTUM};
use crate::graph::{Cmd, Ends, Garbage, Program};
use crate::nodes::Ctx;
use rtrb::{Consumer, Producer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// The app's volume and mute, applied after the patch's outputs.
pub struct Master {
    volume: AtomicU32,
    muted: AtomicBool,
}

impl Default for Master {
    fn default() -> Master {
        Master {
            volume: AtomicU32::new(1f32.to_bits()),
            muted: AtomicBool::new(false),
        }
    }
}

impl Master {
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn set_muted(&self, m: bool) {
        self.muted.store(m, Ordering::Relaxed);
    }

    fn gain(&self) -> f32 {
        if self.muted() {
            0.0
        } else {
            self.volume()
        }
    }
}

/// Microphone samples from the input device's callback, resampled to the
/// output rate (linear) and kept at a small, bounded latency.
pub struct MicInput {
    rx: Consumer<f32>,
    /// Input frames per output frame.
    ratio: f64,
    frac: f64,
    prev: f32,
    next: f32,
    primed: bool,
    prime: usize,
    max_backlog: usize,
}

impl MicInput {
    pub fn new(rx: Consumer<f32>, input_rate: f32, output_rate: f32) -> MicInput {
        MicInput {
            rx,
            ratio: f64::from(input_rate) / f64::from(output_rate),
            frac: 0.0,
            prev: 0.0,
            next: 0.0,
            primed: false,
            prime: (input_rate * 0.02) as usize,
            max_backlog: (input_rate * 0.15) as usize,
        }
    }

    fn fill(&mut self, out: &mut [f32; QUANTUM]) {
        let backlog = self.rx.slots();
        if !self.primed {
            if backlog < self.prime.max(1) {
                *out = [0.0; QUANTUM];
                return;
            }
            self.primed = true;
        }
        // Input running ahead of output (clock drift): drop the excess.
        if backlog > self.max_backlog {
            for _ in 0..backlog - self.prime {
                let _ = self.rx.pop();
            }
        }
        for o in out.iter_mut() {
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                self.prev = self.next;
                match self.rx.pop() {
                    Ok(x) => self.next = x,
                    Err(_) => {
                        self.primed = false;
                        self.next = 0.0;
                    }
                }
            }
            *o = self.prev + (self.next - self.prev) * self.frac as f32;
            self.frac += self.ratio;
        }
    }
}

pub struct Renderer {
    sample_rate: f32,
    program: Box<Program>,
    cmds: Consumer<Cmd>,
    garbage: Producer<Garbage>,
    mic: Option<Box<MicInput>>,
    mic_block: [f32; QUANTUM],
    master: Arc<Master>,
    gain: Ramp,
    block: [[f32; QUANTUM]; 2],
    used: usize,
    frames: Arc<AtomicU64>,
    /// A Mix's inputs summed, for the quantum.
    mix_bus: Bus,
}

impl Renderer {
    pub fn new(sample_rate: f32, ends: Ends, master: Arc<Master>) -> Renderer {
        Renderer {
            sample_rate,
            program: Box::default(),
            cmds: ends.cmds,
            garbage: ends.garbage,
            mic: None,
            mic_block: [0.0; QUANTUM],
            gain: Ramp::new(master.gain(), 0.02, sample_rate),
            master,
            block: [[0.0; QUANTUM]; 2],
            used: QUANTUM,
            frames: Arc::default(),
            mix_bus: Bus::new(),
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Frames rendered so far.
    pub fn clock(&self) -> Arc<AtomicU64> {
        self.frames.clone()
    }

    /// Fill an interleaved buffer of `channels` channels.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        for frame in out.chunks_mut(channels) {
            if self.used == QUANTUM {
                self.quantum();
                self.used = 0;
            }
            let (l, r) = (self.block[0][self.used], self.block[1][self.used]);
            self.used += 1;
            match frame {
                [m] => *m = 0.5 * (l + r),
                [a, b, rest @ ..] => {
                    *a = l;
                    *b = r;
                    rest.fill(0.0);
                }
                [] => {}
            }
        }
    }

    fn give_back(&mut self, g: Garbage) {
        if let Err(rtrb::PushError::Full(g)) = self.garbage.push(g) {
            // Never free on this thread; the control side bounds what is in
            // flight, so this doesn't happen in practice.
            std::mem::forget(g);
        }
    }

    fn poll(&mut self) {
        while let Ok(cmd) = self.cmds.pop() {
            match cmd {
                Cmd::Program(mut next) => {
                    for s in next.slots.iter_mut() {
                        if let Some(j) = s.reuse {
                            s.dsp = self.program.slots.get_mut(j).and_then(|o| o.dsp.take());
                        }
                    }
                    let old = std::mem::replace(&mut self.program, next);
                    self.give_back(Garbage::Program(old));
                }
                Cmd::Mic(m) => {
                    if let Some(old) = std::mem::replace(&mut self.mic, m) {
                        self.give_back(Garbage::Mic(old));
                    }
                }
            }
        }
    }

    fn quantum(&mut self) {
        self.poll();
        let mic = match &mut self.mic {
            Some(m) => {
                m.fill(&mut self.mic_block);
                Some(&self.mic_block)
            }
            None => None,
        };
        let cx = Ctx {
            sample_rate: self.sample_rate,
            mic,
        };
        let slots = &mut self.program.slots;
        let mix_bus = &mut self.mix_bus;
        for i in 0..slots.len() {
            let (done, rest) = slots.split_at_mut(i);
            let s = &mut rest[0];
            let input = if s.mix.is_empty() {
                s.input.filter(|&j| j < i).map(|j| &done[j].out)
            } else {
                mix_bus.silence();
                for &j in s.mix.iter().filter(|&&j| j < i) {
                    mix_bus.add(&done[j].out);
                }
                Some(&*mix_bus)
            };
            match &mut s.dsp {
                Some(d) => d.process(&s.params, input, &mut s.out, &cx),
                None => s.out.silence(),
            }
        }
        let mut mix = [[0f32; QUANTUM]; 2];
        for s in slots.iter().filter(|s| s.is_output) {
            for (c, m) in mix.iter_mut().enumerate() {
                for (y, x) in m.iter_mut().zip(s.out.chan(c)) {
                    *y += x;
                }
            }
        }
        self.gain.set(self.master.gain());
        for i in 0..QUANTUM {
            let g = self.gain.tick();
            for (block, m) in self.block.iter_mut().zip(&mix) {
                let y = m[i] * g;
                // Clip like a DAC; never pass NaN / inf to the device.
                block[i] = if y.is_finite() {
                    y.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
        }
        self.frames.fetch_add(QUANTUM as u64, Ordering::Relaxed);
    }
}
