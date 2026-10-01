//! SSD-style detector decoding (MediaPipe's `SsdAnchorsCalculator`,
//! `TensorsToDetectionsCalculator` and weighted non-max suppression).

/// Anchor grid options (all MediaPipe detectors here use fixed-size anchors,
/// one aspect ratio plus the interpolated scale).
pub struct AnchorSpec {
    pub input: usize,
    pub strides: &'static [usize],
}

/// Anchor centres in normalised tensor coords.
pub fn anchors(spec: &AnchorSpec) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut layer = 0;
    while layer < spec.strides.len() {
        // Layers sharing a stride share a grid: two anchors each (aspect
        // ratio 1 and the interpolated scale).
        let mut last = layer;
        let mut per_cell = 0;
        while last < spec.strides.len() && spec.strides[last] == spec.strides[layer] {
            per_cell += 2;
            last += 1;
        }
        let n = spec.input.div_ceil(spec.strides[layer]);
        for y in 0..n {
            for x in 0..n {
                for _ in 0..per_cell {
                    out.push(((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32));
                }
            }
        }
        layer = last;
    }
    out
}

/// A decoded detection in normalised tensor coords.
#[derive(Clone, Debug)]
pub struct Detection {
    pub score: f32,
    /// `(xmin, ymin, xmax, ymax)`.
    pub bbox: (f32, f32, f32, f32),
    pub keypoints: Vec<(f32, f32)>,
}

/// Decode raw boxes (`[n, coords]`, x-first) and score logits.
pub fn decode(
    boxes: &[f32],
    scores: &[f32],
    anchors: &[(f32, f32)],
    coords: usize,
    keypoints: usize,
    input: f32,
    min_score: f32,
) -> Vec<Detection> {
    let mut out = Vec::new();
    for (i, &(ax, ay)) in anchors.iter().enumerate() {
        let score = 1.0 / (1.0 + (-scores[i].clamp(-100.0, 100.0)).exp());
        if score < min_score {
            continue;
        }
        let r = &boxes[i * coords..][..coords];
        let (cx, cy) = (r[0] / input + ax, r[1] / input + ay);
        let (w, h) = (r[2] / input, r[3] / input);
        out.push(Detection {
            score,
            bbox: (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0),
            keypoints: (0..keypoints)
                .map(|k| (r[4 + 2 * k] / input + ax, r[5 + 2 * k] / input + ay))
                .collect(),
        });
    }
    out
}

/// Weighted NMS: each kept detection's box and keypoints are the
/// score-weighted mean of everything it suppresses.
pub fn weighted_nms(mut dets: Vec<Detection>, iou_threshold: f32, max: usize) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut out = Vec::new();
    while !dets.is_empty() && out.len() < max {
        let top = dets[0].bbox;
        let (group, rest): (Vec<Detection>, Vec<Detection>) = dets
            .into_iter()
            .partition(|d| super::geom::iou(top, d.bbox) > iou_threshold);
        dets = rest;
        let total: f32 = group.iter().map(|d| d.score).sum();
        let mut merged = group[0].clone();
        if group.len() > 1 && total > 0.0 {
            let wavg = |f: &dyn Fn(&Detection) -> f32| {
                group.iter().map(|d| d.score * f(d)).sum::<f32>() / total
            };
            merged.bbox = (
                wavg(&|d| d.bbox.0),
                wavg(&|d| d.bbox.1),
                wavg(&|d| d.bbox.2),
                wavg(&|d| d.bbox.3),
            );
            for k in 0..merged.keypoints.len() {
                merged.keypoints[k] = (wavg(&|d| d.keypoints[k].0), wavg(&|d| d.keypoints[k].1));
            }
        }
        out.push(merged);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_counts_match_the_models() {
        let n = |input, strides| anchors(&AnchorSpec { input, strides }).len();
        assert_eq!(n(192, &[8, 16, 16, 16]), 2016); // hand
        assert_eq!(n(128, &[8, 16, 16, 16]), 896); // face
        assert_eq!(n(224, &[8, 16, 32, 32, 32]), 2254); // pose
        let a = anchors(&AnchorSpec {
            input: 128,
            strides: &[8, 16, 16, 16],
        });
        assert_eq!(a[0], (0.5 / 16.0, 0.5 / 16.0));
        assert_eq!(a[1], a[0]);
        assert_eq!(a[512], (0.5 / 8.0, 0.5 / 8.0));
    }

    #[test]
    fn decode_and_merge() {
        let anchors = [(0.5, 0.5), (0.5, 0.5), (0.1, 0.1)];
        // Box rows: cx, cy, w, h, kx, ky (in 100-px tensor units).
        let boxes = [
            0.0, 0.0, 20.0, 20.0, 1.0, 1.0, //
            2.0, 0.0, 20.0, 20.0, 3.0, 1.0, //
            0.0, 0.0, 10.0, 10.0, 0.0, 0.0,
        ];
        let scores = [3.0, 0.0, 5.0];
        let dets = decode(&boxes, &scores, &anchors, 6, 1, 100.0, 0.4);
        assert_eq!(dets.len(), 3);
        let kept = weighted_nms(dets, 0.3, 5);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].bbox.0 - 0.05).abs() < 1e-6, "{:?}", kept[0].bbox);
        // Two overlapping boxes merged, weighted by sigmoid(3) and 0.5.
        let (s0, s1) = (1.0 / (1.0 + (-3.0f32).exp()), 0.5);
        let kx = (s0 * 0.51 + s1 * 0.53) / (s0 + s1);
        assert!((kept[1].keypoints[0].0 - kx).abs() < 1e-6);
        assert_eq!(weighted_nms(kept, 0.3, 1).len(), 1);
    }
}
