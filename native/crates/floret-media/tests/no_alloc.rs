//! The audio thread must never allocate or free. Render a graph using
//! every node type while parameters, gates and the topology change between
//! quanta, and count heap calls made from inside the render.

use floret_engine::audio::{
    AudioBackend, AudioGraph, AudioNode, FilterType, NoiseColor, Oversample, SampleState, Wave,
};
use floret_media::AudioSystem;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Counting;

static HEAP_CALLS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

fn note() {
    if ARMED.try_with(Cell::get).unwrap_or(false) {
        HEAP_CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note();
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn wav(path: &std::path::Path) {
    let samples: Vec<i16> = (0..4000).map(|n| ((n % 50) * 600 - 15000) as i16).collect();
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    for x in [16u32, 1 | (1 << 16), 22050, 44100, 2 | (16 << 16)] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in &samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, v).unwrap();
}

fn graph(step: usize, url: &Arc<str>, with_gain: bool) -> AudioGraph {
    let i = |s: &str| Some(Arc::<str>::from(s));
    let gate = step % 3 != 0;
    let x = step as f64;
    let mut nodes = vec![
        (
            "osc",
            AudioNode::Oscillator {
                wave: Wave::Sawtooth,
                freq: 220.0 + x * 7.0,
                amp: 0.3,
            },
        ),
        (
            "pulse",
            AudioNode::Pulse {
                freq: 110.0,
                amp: 0.2,
                width: (x * 0.1) % 0.9,
            },
        ),
        (
            "noise",
            AudioNode::Noise {
                color: NoiseColor::Pink,
                amp: 0.1,
                seed: 1,
            },
        ),
        (
            "synth",
            AudioNode::MonoSynth {
                freq: 330.0,
                velocity: 0.5,
                attack: 0.0,
                decay: 0.01,
                sustain: 0.5,
                release: 0.01,
                gate,
            },
        ),
        (
            "file",
            AudioNode::SoundFile {
                url: url.clone(),
                rate: 1.5,
                pan: -0.5,
                amp: 1.0,
                looping: true,
                play: gate,
            },
        ),
        (
            "mic",
            AudioNode::AudioIn {
                amp: 0.5,
                start: true,
            },
        ),
        (
            "filter",
            AudioNode::Filter {
                input: i("osc"),
                kind: [
                    FilterType::Lowpass,
                    FilterType::Highpass,
                    FilterType::Bandpass,
                ][step % 3],
                freq: 500.0 + x * 100.0,
                q: 2.0,
            },
        ),
        (
            "delay",
            AudioNode::Delay {
                input: i("filter"),
                time: 0.01 + x * 0.002,
                feedback: 0.5,
                damping: 3000.0,
            },
        ),
        (
            "dist",
            AudioNode::Distortion {
                input: i("pulse"),
                amount: (x * 0.05) % 1.0,
                wet: 0.7,
                oversample: [Oversample::None, Oversample::X2, Oversample::X4][step % 3],
            },
        ),
        (
            "comp",
            AudioNode::Compressor {
                input: i("dist"),
                threshold: -30.0 + x,
                ratio: 8.0,
                knee: 10.0,
                attack: 0.003,
                release: 0.1,
            },
        ),
        (
            "env",
            AudioNode::Envelope {
                input: i("noise"),
                attack: 0.002,
                decay: 0.01,
                sustain: 0.3,
                release: 0.02,
                level: 1.0,
                gate,
            },
        ),
        (
            "verb",
            AudioNode::Reverb {
                input: i("env"),
                seconds: 0.5,
                decay: 3.0,
                reverse: false,
                wet: 0.4,
                seed: 3,
            },
        ),
        (
            "fft",
            AudioNode::Analyser {
                input: i("delay"),
                fft_size: 1024,
                smoothing: 0.8,
            },
        ),
    ];
    let mix = if with_gain { "gain" } else { "delay" };
    if with_gain {
        nodes.push((
            "gain",
            AudioNode::Gain {
                input: i("delay"),
                level: 0.5,
            },
        ));
    }
    for (n, from) in [
        ("o1", mix),
        ("o2", "comp"),
        ("o3", "synth"),
        ("o4", "file"),
        ("o5", "mic"),
        ("o6", "verb"),
    ] {
        nodes.push((
            n,
            AudioNode::Output {
                input: i(from),
                volume: 0.5,
            },
        ));
    }
    AudioGraph {
        live: nodes.iter().map(|(n, _)| Arc::from(*n)).collect(),
        declared: nodes
            .into_iter()
            .map(|(n, d)| (Arc::from(n), Some(d)))
            .collect(),
    }
}

#[test]
fn rendering_never_touches_the_heap() {
    let path = std::env::temp_dir().join(format!("floret-noalloc-{}.wav", std::process::id()));
    wav(&path);
    let url: Arc<str> = Arc::from(path.to_string_lossy().as_ref());
    let sys = AudioSystem::offline(48000.0);
    let mut mic = sys.attach_test_mic(44100.0);
    // Wait for the file so the player has data.
    for _ in 0..500 {
        sys.submit(graph(0, &url, false));
        if matches!(
            sys.feedback().samples.get(&url),
            Some(SampleState::Ready { .. })
        ) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    sys.submit(graph(0, &url, false));
    let mut buf = vec![0f32; 2 * 300];
    let mut fed = 0u32;
    let mut peak = 0f32;
    for step in 0..120 {
        // Control side (allowed to allocate): new params, gates, rewiring.
        sys.submit(graph(step, &url, step % 10 >= 5));
        let _ = sys.feedback();
        for _ in 0..300 {
            fed += 1;
            let _ = mic.push(((fed % 100) as f32 / 100.0) - 0.5);
        }
        ARMED.with(|a| a.set(true));
        sys.render_into(&mut buf);
        ARMED.with(|a| a.set(false));
        peak = buf.iter().fold(peak, |m, v| m.max(v.abs()));
    }
    let _ = std::fs::remove_file(&path);
    assert!(peak > 0.1, "the graph made no sound");
    assert_eq!(
        HEAP_CALLS.load(Ordering::Relaxed),
        0,
        "allocations on the audio path"
    );
}
