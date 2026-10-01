//! Stable per-person ids (ported from `mediapipe-registry.ts`): each
//! detection keeps its id, and so its colour, while it moves, and in
//! `all` mode faces and hands take the id of the nearest pose.

use super::geom::Point;
use floret_engine::landmarks::Tracker;

/// Squared centroid distance under which a detection is last frame's entity.
const MATCH_DIST2: f32 = 0.25 * 0.25;
/// Frames an entity may go unseen before its id is freed.
const MAX_MISS: u32 = 12;

struct Tracked {
    id: u32,
    cx: f32,
    cy: f32,
    miss: u32,
}

#[derive(Default)]
pub struct IdTracker {
    tracked: Vec<Tracked>,
}

fn centroid(det: &[Point]) -> (f32, f32) {
    let n = det.len().max(1) as f32;
    let (sx, sy) = det.iter().fold((0.0, 0.0), |a, p| (a.0 + p[0], a.1 + p[1]));
    (sx / n, sy / n)
}

impl IdTracker {
    /// Ids for this frame's detections: greedy nearest match to last
    /// frame's entities, else the smallest free id.
    pub fn assign(&mut self, dets: &[Vec<Point>]) -> Vec<u32> {
        let cs: Vec<(f32, f32)> = dets.iter().map(|d| centroid(d)).collect();
        let mut ids: Vec<Option<u32>> = vec![None; cs.len()];
        let orig = self.tracked.len();
        let mut matched = vec![false; orig];
        for (i, c) in cs.iter().enumerate() {
            let mut best = None;
            let mut best_d = MATCH_DIST2;
            for (t, tr) in self.tracked.iter().enumerate() {
                if matched[t] {
                    continue;
                }
                let d = (c.0 - tr.cx).powi(2) + (c.1 - tr.cy).powi(2);
                if d < best_d {
                    best_d = d;
                    best = Some(t);
                }
            }
            if let Some(t) = best {
                matched[t] = true;
                let tr = &mut self.tracked[t];
                (tr.cx, tr.cy, tr.miss) = (c.0, c.1, 0);
                ids[i] = Some(tr.id);
            }
        }
        for (i, c) in cs.iter().enumerate() {
            if ids[i].is_some() {
                continue;
            }
            let id = (0..)
                .find(|n| !self.tracked.iter().any(|t| t.id == *n))
                .unwrap_or(0);
            self.tracked.push(Tracked {
                id,
                cx: c.0,
                cy: c.1,
                miss: 0,
            });
            ids[i] = Some(id);
        }
        for (t, m) in matched.iter().enumerate() {
            if !m {
                self.tracked[t].miss += 1;
            }
        }
        self.tracked.retain(|t| t.miss <= MAX_MISS);
        ids.into_iter().map(|i| i.unwrap_or(0)).collect()
    }
}

/// One tracker's frame result: detections and their ids.
pub struct Layer {
    pub tracker: Tracker,
    pub dets: Vec<Vec<Point>>,
    pub ids: Vec<u32>,
}

/// Pose-anchored ids for `all` mode: a face takes the id of the pose whose
/// nose is nearest its centroid, a hand the pose with the nearest wrist
/// (15 / 16) to its own wrist. Without a pose the ids stand.
pub fn unify(layers: &mut [Layer]) {
    let Some(pose) = layers.iter().find(|l| l.tracker == Tracker::Pose) else {
        return;
    };
    if pose.dets.is_empty() {
        return;
    }
    struct Person {
        id: u32,
        nose: Option<(f32, f32)>,
        wrists: [Option<(f32, f32)>; 2],
    }
    let at = |d: &[Point], i: usize| d.get(i).map(|p| (p[0], p[1]));
    let persons: Vec<Person> = pose
        .dets
        .iter()
        .enumerate()
        .map(|(di, d)| Person {
            id: pose.ids.get(di).copied().unwrap_or(di as u32),
            nose: at(d, 0),
            wrists: [at(d, 15), at(d, 16)],
        })
        .collect();
    for layer in layers.iter_mut().filter(|l| l.tracker != Tracker::Pose) {
        let hand = layer.tracker == Tracker::Hands;
        layer.ids = layer
            .dets
            .iter()
            .map(|det| {
                let p = if hand {
                    at(det, 0).unwrap_or((0.0, 0.0))
                } else {
                    centroid(det)
                };
                let mut best = (persons[0].id, f32::INFINITY);
                for person in &persons {
                    let targets: &[Option<(f32, f32)>] = if hand {
                        &person.wrists
                    } else {
                        std::slice::from_ref(&person.nose)
                    };
                    for t in targets.iter().flatten() {
                        let d = (p.0 - t.0).powi(2) + (p.1 - t.1).powi(2);
                        if d < best.1 {
                            best = (person.id, d);
                        }
                    }
                }
                best.0
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Vec<Point> {
        vec![[x, y, 0.0]]
    }

    #[test]
    fn ids_follow_movement_and_free_up() {
        let mut t = IdTracker::default();
        assert_eq!(t.assign(&[at(0.2, 0.5), at(0.8, 0.5)]), [0, 1]);
        // Both move a little, listed in the other order.
        assert_eq!(t.assign(&[at(0.75, 0.5), at(0.25, 0.5)]), [1, 0]);
        // Person 0 leaves; a newcomer far away can't take id 0 while it's
        // remembered, so gets 2.
        assert_eq!(t.assign(&[at(0.75, 0.5)]), [1]);
        assert_eq!(t.assign(&[at(0.75, 0.5), at(0.25, 0.95)]), [1, 2]);
        let mut t = IdTracker::default();
        t.assign(&[at(0.1, 0.1)]);
        for _ in 0..=MAX_MISS {
            t.assign(&[]);
        }
        // Forgotten after MAX_MISS unseen frames: the id is reused.
        assert_eq!(t.assign(&[at(0.9, 0.9)]), [0]);
    }

    #[test]
    fn faces_and_hands_take_the_nearest_pose_id() {
        let mut pose = vec![[0.0f32; 3]; 33];
        pose[0] = [0.3, 0.2, 0.0];
        pose[15] = [0.2, 0.6, 0.0];
        pose[16] = [0.4, 0.6, 0.0];
        let mut other = vec![[0.0f32; 3]; 33];
        other[0] = [0.8, 0.2, 0.0];
        other[15] = [0.7, 0.6, 0.0];
        other[16] = [0.9, 0.6, 0.0];
        let mut layers = vec![
            Layer {
                tracker: Tracker::Hands,
                dets: vec![at(0.88, 0.62), at(0.21, 0.58)],
                ids: vec![0, 1],
            },
            Layer {
                tracker: Tracker::Face,
                dets: vec![at(0.31, 0.22)],
                ids: vec![0],
            },
            Layer {
                tracker: Tracker::Pose,
                dets: vec![pose, other],
                ids: vec![4, 7],
            },
        ];
        unify(&mut layers);
        assert_eq!(layers[0].ids, [7, 4]);
        assert_eq!(layers[1].ids, [4]);
        assert_eq!(layers[2].ids, [4, 7]);
    }
}
