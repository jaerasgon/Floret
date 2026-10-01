//! With or without an audio device (CI machines usually have none), a
//! started engine keeps running the graph and reports what it is doing.

use floret_engine::audio::{AudioBackend, AudioGraph, AudioNode, MicState, Wave};
use floret_media::AudioSystem;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn runs_the_graph_whatever_the_devices() {
    let sys = AudioSystem::start();
    let nodes = vec![
        (
            "o",
            AudioNode::Oscillator {
                wave: Wave::Sine,
                freq: 440.0,
                amp: 0.5,
            },
        ),
        // Analysed but never sent to an output: nothing is audible.
        (
            "a",
            AudioNode::Analyser {
                input: Some(Arc::from("o")),
                fft_size: 512,
                smoothing: 0.0,
            },
        ),
    ];
    let graph = || AudioGraph {
        live: nodes.iter().map(|(n, _)| Arc::from(*n)).collect(),
        declared: nodes
            .iter()
            .map(|(n, d)| (Arc::from(*n), Some(d.clone())))
            .collect(),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut level = 0f32;
    while Instant::now() < deadline && level < 0.4 {
        sys.submit(graph());
        if let Some(a) = sys.feedback().analysers.get("a") {
            level = a.time.iter().fold(0f32, |m, v| m.max(v.abs()));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(level > 0.4, "the graph isn't being rendered");
    let st = sys.status();
    assert!(
        st.output.is_some() || st.error.is_some(),
        "status says nothing: {st:?}"
    );
    assert_eq!(sys.feedback().mic, MicState::Off);
}
