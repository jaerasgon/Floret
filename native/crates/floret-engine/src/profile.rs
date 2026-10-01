//! Per-node timing for the last tick (see [`crate::Engine::profile`]).

use crate::graph::NodeIdx;
use std::time::Duration;

/// Weight of the newest tick in the smoothed averages.
const SMOOTHING: f64 = 0.1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeProfile {
    /// Self time last tick: the node's own gather + compute (a region's
    /// bookkeeping, excluding its body). Zero unless profiling is on.
    pub time: Duration,
    /// `time` in ms, smoothed over recent ticks.
    pub avg_ms: f64,
    /// Computes run last tick (one per loop iteration inside a region).
    pub evals: u32,
    /// Computes skipped last tick because inputs and params were unchanged.
    pub hits: u32,
}

/// Counters are always kept; times only while profiling is on
/// ([`crate::Engine::set_profiling`]), since timing every node costs a
/// clock read per evaluation.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub enabled: bool,
    pub ticks: u64,
    /// Wall time of the last tick.
    pub tick: Duration,
    /// `tick` in ms, smoothed over recent ticks.
    pub tick_avg_ms: f64,
    /// Indexed by node.
    pub nodes: Vec<NodeProfile>,
}

impl Profile {
    pub(crate) fn begin(&mut self, n: usize) {
        self.nodes.resize_with(n, NodeProfile::default);
        for p in &mut self.nodes {
            p.time = Duration::ZERO;
            p.evals = 0;
            p.hits = 0;
        }
    }

    pub(crate) fn end(&mut self, tick: Duration) {
        let blend = |avg: f64, ms: f64, first: bool| {
            if first {
                ms
            } else {
                avg + (ms - avg) * SMOOTHING
            }
        };
        let first = self.ticks == 0;
        self.tick = tick;
        self.tick_avg_ms = blend(self.tick_avg_ms, tick.as_secs_f64() * 1e3, first);
        if self.enabled {
            for p in &mut self.nodes {
                p.avg_ms = blend(p.avg_ms, p.time.as_secs_f64() * 1e3, first);
            }
        }
        self.ticks += 1;
    }

    /// The `n` nodes with the most (smoothed) self time, slowest first.
    pub fn top(&self, n: usize) -> Vec<(NodeIdx, &NodeProfile)> {
        let mut all: Vec<(NodeIdx, &NodeProfile)> = self.nodes.iter().enumerate().collect();
        all.sort_by(|a, b| b.1.avg_ms.total_cmp(&a.1.avg_ms));
        all.truncate(n);
        all
    }

    /// Computes run and skipped across all nodes last tick.
    pub fn totals(&self) -> (u64, u64) {
        self.nodes.iter().fold((0, 0), |(e, h), p| {
            (e + u64::from(p.evals), h + u64::from(p.hits))
        })
    }
}
