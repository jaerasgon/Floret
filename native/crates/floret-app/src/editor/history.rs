//! Undo / redo. Port of `src/editor/history.ts`.
//!
//! An entry is a snapshot of the patch (nodes, edges, params — params live
//! on the nodes here — plus the loop length, patch seed and presets). Every committed mutation pushes one entry; live
//! scrubs don't push at all; a param commit on the same `(node, key)`
//! within 250 ms of the previous one replaces that entry instead of
//! stacking, so typing collapses into one step.
//!
//! Every entry has a unique id; the current entry's id is the patch's
//! revision, which the app compares with the saved one for dirty tracking.

use floret_engine::patch::{PatchEdge, PatchFile, PatchNode, Preset};
use std::time::{Duration, Instant};

pub const MAX_HISTORY: usize = 100;
pub const COALESCE: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct Entry {
    nodes: Vec<PatchNode>,
    edges: Vec<PatchEdge>,
    loop_length: Option<f64>,
    seed: u32,
    presets: Vec<Preset>,
    at: Instant,
    /// `"<node>.<key>"` for param commits (coalescing key).
    param_key: Option<String>,
    id: u64,
}

#[derive(Default)]
pub struct History {
    past: Vec<Entry>,
    future: Vec<Entry>,
    next_id: u64,
}

impl History {
    fn entry(&mut self, patch: &PatchFile, at: Instant, param_key: Option<String>) -> Entry {
        self.next_id += 1;
        Entry {
            nodes: patch.nodes.clone(),
            edges: patch.edges.clone(),
            loop_length: patch.loop_length,
            seed: patch.seed,
            presets: patch.presets.clone(),
            at,
            param_key,
            id: self.next_id,
        }
    }

    /// Id of the current state; changes with every edit, undo and redo.
    pub fn revision(&self) -> u64 {
        self.past.last().map_or(0, |e| e.id)
    }

    /// Seed with a freshly loaded patch: the first undo can't go past it.
    pub fn reset(&mut self, patch: &PatchFile) {
        self.past = vec![self.entry(patch, Instant::now(), None)];
        self.future.clear();
    }

    pub fn push(&mut self, patch: &PatchFile, param_key: Option<&str>) {
        self.push_at(patch, param_key, Instant::now());
    }

    pub fn push_at(&mut self, patch: &PatchFile, param_key: Option<&str>, now: Instant) {
        let next = self.entry(patch, now, param_key.map(str::to_owned));
        let coalesce = match (self.past.last(), param_key) {
            (Some(last), Some(k)) => {
                last.param_key.as_deref() == Some(k) && now.duration_since(last.at) < COALESCE
            }
            _ => false,
        };
        if coalesce {
            *self.past.last_mut().expect("checked") = next;
        } else {
            self.past.push(next);
            if self.past.len() > MAX_HISTORY {
                self.past.remove(0);
            }
        }
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        self.past.len() > 1
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Step back; writes the restored state into `patch`.
    pub fn undo(&mut self, patch: &mut PatchFile) -> bool {
        if !self.can_undo() {
            return false;
        }
        let current = self.past.pop().expect("checked");
        self.future.push(current);
        Self::apply(self.past.last().expect("checked"), patch);
        true
    }

    pub fn redo(&mut self, patch: &mut PatchFile) -> bool {
        let Some(target) = self.future.pop() else {
            return false;
        };
        Self::apply(&target, patch);
        self.past.push(target);
        true
    }

    /// Write the current state back into `patch`, dropping changes made
    /// since the last push (a half-done edit).
    pub fn restore(&self, patch: &mut PatchFile) {
        if let Some(e) = self.past.last() {
            Self::apply(e, patch);
        }
    }

    fn apply(e: &Entry, patch: &mut PatchFile) {
        patch.nodes = e.nodes.clone();
        patch.edges = e.edges.clone();
        patch.loop_length = e.loop_length;
        patch.seed = e.seed;
        patch.presets = e.presets.clone();
    }

    /// Number of undo states held (including the current one).
    #[cfg(test)]
    pub fn depth(&self) -> usize {
        self.past.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{node, patch};

    fn set_value(p: &mut PatchFile, v: f64) {
        p.nodes[0].params.insert("value".into(), v.into());
    }

    fn value(p: &PatchFile) -> f64 {
        p.nodes[0].params["value"].as_f64().unwrap()
    }

    #[test]
    fn undo_reverts_add_and_redo_replays() {
        let mut p = patch(vec![], vec![]);
        let mut h = History::default();
        h.reset(&p);
        assert!(!h.can_undo());
        p.nodes.push(node("a", "value/number"));
        h.push(&p, None);
        assert!(h.undo(&mut p));
        assert!(p.nodes.is_empty());
        assert!(h.redo(&mut p));
        assert_eq!(p.nodes[0].id, "a");
        assert!(!h.redo(&mut p));
    }

    #[test]
    fn restore_drops_uncommitted_changes() {
        let mut p = patch(vec![node("a", "value/number")], vec![]);
        let mut h = History::default();
        h.reset(&p);
        p.nodes.push(node("half", "value/number"));
        p.edges.clear();
        h.restore(&mut p);
        assert_eq!(p.nodes.len(), 1);
        assert!(!h.can_undo());
    }

    #[test]
    fn same_param_within_250ms_coalesces() {
        let mut p = patch(vec![node("a", "value/number")], vec![]);
        let mut h = History::default();
        h.reset(&p);
        let t0 = Instant::now();
        set_value(&mut p, 1.0);
        h.push_at(&p, Some("a.value"), t0);
        set_value(&mut p, 2.0);
        h.push_at(&p, Some("a.value"), t0 + Duration::from_millis(100));
        assert_eq!(h.depth(), 2, "rapid edits on one key merge");
        // A different key, or a pause, makes a new step.
        set_value(&mut p, 3.0);
        h.push_at(&p, Some("a.value"), t0 + Duration::from_millis(500));
        assert_eq!(h.depth(), 3);
        set_value(&mut p, 4.0);
        h.push_at(&p, Some("a.other"), t0 + Duration::from_millis(520));
        assert_eq!(h.depth(), 4);
        // Structural pushes never merge.
        h.push_at(&p, None, t0 + Duration::from_millis(530));
        h.push_at(&p, None, t0 + Duration::from_millis(531));
        assert_eq!(h.depth(), 6);
        h.undo(&mut p);
        h.undo(&mut p);
        h.undo(&mut p);
        assert_eq!(value(&p), 3.0);
        h.undo(&mut p);
        assert_eq!(value(&p), 2.0, "merged step lands on the last value");
    }

    #[test]
    fn loop_length_edits_undo() {
        let mut p = patch(vec![], vec![]);
        let mut h = History::default();
        h.reset(&p);
        p.loop_length = Some(3.0);
        h.push(&p, Some("patch.loopLength"));
        p.loop_length = Some(6.0);
        h.push(&p, Some("patch.loopLength"));
        assert_eq!(h.depth(), 2, "dragging the setting is one step");
        h.undo(&mut p);
        assert_eq!(p.loop_length, None);
        h.redo(&mut p);
        assert_eq!(p.loop_length, Some(6.0));
    }

    #[test]
    fn undo_covers_the_seed_and_presets() {
        let mut p = patch(vec![node("a", "value/number")], vec![]);
        let mut h = History::default();
        h.reset(&p);
        p.seed = 42;
        h.push(&p, None);
        p.presets.push(Preset {
            name: "A".into(),
            ..Preset::default()
        });
        h.push(&p, None);
        h.undo(&mut p);
        assert!(p.presets.is_empty());
        assert_eq!(p.seed, 42);
        h.undo(&mut p);
        assert_eq!(p.seed, 0);
        h.redo(&mut p);
        h.redo(&mut p);
        assert_eq!((p.seed, p.presets.len()), (42, 1));
    }

    #[test]
    fn new_mutation_clears_redo() {
        let mut p = patch(vec![], vec![]);
        let mut h = History::default();
        h.reset(&p);
        p.nodes.push(node("a", "value/number"));
        h.push(&p, None);
        h.undo(&mut p);
        assert!(h.can_redo());
        p.nodes.push(node("b", "value/number"));
        h.push(&p, None);
        assert!(!h.can_redo());
    }

    #[test]
    fn revision_tracks_the_current_state() {
        let mut p = patch(vec![], vec![]);
        let mut h = History::default();
        h.reset(&p);
        let saved = h.revision();
        p.nodes.push(node("a", "value/number"));
        h.push(&p, None);
        let edited = h.revision();
        assert_ne!(edited, saved);
        h.undo(&mut p);
        assert_eq!(h.revision(), saved, "undo back to the saved state");
        h.redo(&mut p);
        assert_eq!(h.revision(), edited);
        // A coalesced param edit is still a change.
        let t0 = Instant::now();
        h.push_at(&p, Some("a.value"), t0);
        let r1 = h.revision();
        h.push_at(&p, Some("a.value"), t0 + Duration::from_millis(10));
        assert_ne!(h.revision(), r1);
        h.reset(&p);
        assert_ne!(h.revision(), saved, "a reload is a new revision");
    }

    #[test]
    fn capped_at_max() {
        let mut p = patch(vec![], vec![]);
        let mut h = History::default();
        h.reset(&p);
        for i in 0..150 {
            p.nodes.push(node(&format!("n{i}"), "value/number"));
            h.push(&p, None);
        }
        assert_eq!(h.depth(), MAX_HISTORY);
    }
}
