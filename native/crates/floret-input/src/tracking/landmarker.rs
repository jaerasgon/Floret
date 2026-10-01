//! One tracker (hands, face or pose): MediaPipe's two-stage pipeline.
//!
//! A detector finds palms / faces / bodies on the letterboxed frame; each
//! detection becomes a rotated ROI that the landmark model reads. The
//! landmarks then give next frame's ROI, so the detector only runs while
//! fewer than `max` objects are tracked (MediaPipe's VIDEO mode). A
//! tracked object is dropped when the landmark model's presence score
//! falls below 0.5. Pose and face landmarks are smoothed with MediaPipe's
//! one-euro filter; hands aren't (nor are they in MediaPipe's VIDEO mode).
//!
//! Check against real photos with the `track_image` example.

use super::bundle::{self, Models};
use super::detect::{self, AnchorSpec, Detection};
use super::geom::{self, Border, Frame, Point, Rect};
use super::smooth::{self, Smoother};
use crate::nn::Interpreter;
pub use floret_engine::landmarks::Tracker;
use std::f32::consts::FRAC_PI_2;

trait Config {
    fn spec(self) -> Spec;
    fn models(self) -> Result<std::sync::Arc<Models>, String>;
}

impl Config for Tracker {
    fn spec(self) -> Spec {
        match self {
            Tracker::Hands => Spec {
                det_size: 192,
                det_range: (0.0, 1.0),
                strides: &[8, 16, 16, 16],
                keypoints: 7,
                lm_size: 224,
                lm_count: 21,
                lm_stride: 3,
                normalize_z: 0.4,
                presence_logit: false,
                smooth: false,
            },
            Tracker::Face => Spec {
                det_size: 128,
                det_range: (-1.0, 1.0),
                strides: &[8, 16, 16, 16],
                keypoints: 6,
                lm_size: 256,
                lm_count: 478,
                lm_stride: 3,
                normalize_z: 1.0,
                presence_logit: true,
                smooth: true,
            },
            Tracker::Pose => Spec {
                det_size: 224,
                det_range: (-1.0, 1.0),
                strides: &[8, 16, 32, 32, 32],
                keypoints: 4,
                lm_size: 256,
                // 33 body landmarks + 6 auxiliary (33/34 drive the ROI).
                lm_count: 39,
                lm_stride: 5,
                normalize_z: 1.0,
                presence_logit: false,
                smooth: true,
            },
        }
    }

    fn models(self) -> Result<std::sync::Arc<Models>, String> {
        match self {
            Tracker::Hands => bundle::hand(),
            Tracker::Face => bundle::face(),
            Tracker::Pose => bundle::pose(),
        }
    }
}

struct Spec {
    det_size: usize,
    det_range: (f32, f32),
    strides: &'static [usize],
    keypoints: usize,
    lm_size: usize,
    lm_count: usize,
    lm_stride: usize,
    /// `TensorsToLandmarksCalculator`'s extra z divisor.
    normalize_z: f32,
    presence_logit: bool,
    smooth: bool,
}

const MIN_DETECTION: f32 = 0.5;
const MIN_PRESENCE: f32 = 0.5;
const NMS_IOU: f32 = 0.3;
/// A detection overlapping a tracked object this much is the same object.
const SAME_OBJECT_IOU: f32 = 0.5;

struct Track {
    /// ROI for the next frame.
    rect: Rect,
    /// Output landmarks, auxiliary (ROI) landmarks.
    smoothers: (Smoother, Smoother),
}

pub struct Landmarker {
    kind: Tracker,
    spec: Spec,
    max: usize,
    det: Interpreter,
    lm: Interpreter,
    anchors: Vec<(f32, f32)>,
    /// Which detector output holds boxes (the other holds scores).
    box_output: usize,
    tracked: Vec<Track>,
}

impl Landmarker {
    /// A tracker for up to `max` objects.
    pub fn new(kind: Tracker, max: usize) -> Result<Landmarker, String> {
        let models = kind.models()?;
        let spec = kind.spec();
        let det = Interpreter::new(models.detector.clone());
        let anchors = detect::anchors(&AnchorSpec {
            input: spec.det_size,
            strides: spec.strides,
        });
        let coords = 4 + 2 * spec.keypoints;
        let box_output = (0..2)
            .find(|&o| det.model().output_shape(o).last() == Some(&coords))
            .ok_or("detector outputs don't match the expected layout")?;
        if det.model().output_shape(box_output).get(1) != Some(&anchors.len()) {
            return Err("detector anchor count mismatch".into());
        }
        Ok(Landmarker {
            kind,
            spec,
            max: max.max(1),
            det,
            // Landmarks, presence, and for pose the refinement heatmap; not
            // the segmentation mask, world landmarks or handedness.
            lm: Interpreter::new(models.landmarks.clone()).with_outputs(if kind == Tracker::Pose {
                &[0, 1, 3]
            } else {
                &[0, 1]
            }),
            anchors,
            box_output,
            tracked: Vec::new(),
        })
    }

    pub fn kind(&self) -> Tracker {
        self.kind
    }

    /// Forget tracked objects (the next frame runs the detector).
    pub fn reset(&mut self) {
        self.tracked.clear();
    }

    /// Landmarks of every object in `frame` (normalised image coords),
    /// `t` the frame time in seconds.
    pub fn process(&mut self, frame: &Frame, t: f64) -> Vec<Vec<Point>> {
        let img = (frame.width as f32, frame.height as f32);
        let mut rois: Vec<(Rect, Option<(Smoother, Smoother)>)> = std::mem::take(&mut self.tracked)
            .into_iter()
            .map(|tr| (tr.rect, Some(tr.smoothers)))
            .collect();
        if rois.len() < self.max {
            for d in self.detect(frame) {
                if rois.len() >= self.max {
                    break;
                }
                let r = self.detection_rect(&d, img);
                let b = geom::bounds(&r, img);
                if !rois
                    .iter()
                    .any(|(t, _)| geom::iou(geom::bounds(t, img), b) > SAME_OBJECT_IOU)
                {
                    rois.push((r, None));
                }
            }
        }

        let mut out = Vec::new();
        for (rect, smoothers) in rois {
            let Some(mut pts) = self.landmarks(frame, &rect) else {
                continue;
            };
            let mut aux = pts.split_off(self.kind.group());
            let (mut smoother, mut aux_smoother) = smoothers.unwrap_or_else(|| {
                (
                    Smoother::new(smooth::LANDMARKS),
                    Smoother::new(smooth::AUX_LANDMARKS),
                )
            });
            // Pose's ROI follows its auxiliary landmarks, smoothed with a
            // stronger filter and sized by this frame's ROI.
            let roi_scale = ((rect.w * img.0) + (rect.h * img.1)) / 2.0;
            aux_smoother.apply_scaled(t, img, &mut aux, Some(roi_scale));
            let next = self.next_rect(&pts, &aux, img);
            let nb = geom::bounds(&next, img);
            if self
                .tracked
                .iter()
                .any(|t| geom::iou(geom::bounds(&t.rect, img), nb) > SAME_OBJECT_IOU)
            {
                // Two ROIs converged on one object.
                continue;
            }
            if self.spec.smooth {
                let scale = (self.kind == Tracker::Pose).then_some(roi_scale);
                smoother.apply_scaled(t, img, &mut pts, scale);
            }
            self.tracked.push(Track {
                rect: next,
                smoothers: (smoother, aux_smoother),
            });
            out.push(pts);
        }
        out
    }

    /// Next frame's ROI from this frame's (unsmoothed) landmarks, or for
    /// pose its smoothed auxiliary landmarks.
    fn next_rect(&self, pts: &[Point], aux: &[Point], img: (f32, f32)) -> Rect {
        let xy = |p: &Point| (p[0], p[1]);
        match self.kind {
            Tracker::Hands => geom::transform(geom::hand_rect(pts, img), img, 2.0, -0.1),
            Tracker::Face => {
                let r = geom::box_rect(
                    geom::landmark_bounds(pts),
                    xy(&pts[33]),
                    xy(&pts[263]),
                    0.0,
                    img,
                );
                geom::transform(r, img, 1.5, 0.0)
            }
            Tracker::Pose => {
                let r = geom::alignment_rect(xy(&aux[0]), xy(&aux[1]), FRAC_PI_2, img);
                geom::transform(r, img, 1.25, 0.0)
            }
        }
    }

    /// Detector pass on the letterboxed frame, in normalised image coords.
    fn detect(&mut self, frame: &Frame) -> Vec<Detection> {
        let img = (frame.width as f32, frame.height as f32);
        let lb = geom::letterbox_rect(img);
        let s = &self.spec;
        geom::crop(
            frame,
            &lb,
            s.det_size,
            s.det_range,
            Border::Zero,
            self.det.input_mut(0),
        );
        self.det.run();
        let dets = detect::decode(
            self.det.output(self.box_output),
            self.det.output(1 - self.box_output),
            &self.anchors,
            4 + 2 * s.keypoints,
            s.keypoints,
            s.det_size as f32,
            MIN_DETECTION,
        );
        let mut dets = detect::weighted_nms(dets, NMS_IOU, self.max);
        for d in &mut dets {
            let (x0, y0) = geom::project(&lb, img, d.bbox.0, d.bbox.1);
            let (x1, y1) = geom::project(&lb, img, d.bbox.2, d.bbox.3);
            d.bbox = (x0, y0, x1, y1);
            for k in &mut d.keypoints {
                *k = geom::project(&lb, img, k.0, k.1);
            }
        }
        dets
    }

    /// The landmark ROI for a detection.
    fn detection_rect(&self, d: &Detection, img: (f32, f32)) -> Rect {
        let kp = &d.keypoints;
        match self.kind {
            Tracker::Hands => geom::transform(
                geom::box_rect(d.bbox, kp[0], kp[2], FRAC_PI_2, img),
                img,
                2.6,
                -0.5,
            ),
            Tracker::Face => geom::transform(
                geom::box_rect(d.bbox, kp[0], kp[1], 0.0, img),
                img,
                1.5,
                0.0,
            ),
            Tracker::Pose => geom::transform(
                geom::alignment_rect(kp[0], kp[1], FRAC_PI_2, img),
                img,
                1.25,
                0.0,
            ),
        }
    }

    /// Every landmark the model outputs for `rect`, in image coords, or
    /// `None` when the model says nothing is there.
    fn landmarks(&mut self, frame: &Frame, rect: &Rect) -> Option<Vec<Point>> {
        let img = (frame.width as f32, frame.height as f32);
        let s = &self.spec;
        let size = s.lm_size as f32;
        geom::crop(
            frame,
            rect,
            s.lm_size,
            (0.0, 1.0),
            Border::Replicate,
            self.lm.input_mut(0),
        );
        self.lm.run();
        let mut presence = self.lm.output(1)[0];
        if s.presence_logit {
            presence = 1.0 / (1.0 + (-presence).exp());
        }
        if presence < MIN_PRESENCE {
            return None;
        }
        let raw = self.lm.output(0);
        let mut pts: Vec<Point> = (0..s.lm_count)
            .map(|i| {
                let v = &raw[i * s.lm_stride..];
                [v[0] / size, v[1] / size, v[2] / size / s.normalize_z]
            })
            .collect();
        if self.kind == Tracker::Pose {
            let hm = self.lm.model().output_shape(3).to_vec();
            refine_from_heatmap(&mut pts, self.lm.output(3), &hm, 7, 0.5);
        }
        geom::project_points(rect, img, &mut pts);
        Some(pts)
    }
}

/// `RefineLandmarksFromHeatmapCalculator`: move each landmark to the
/// confidence-weighted centre of a `kernel × kernel` heatmap window.
fn refine_from_heatmap(
    pts: &mut [Point],
    hm: &[f32],
    dims: &[usize],
    kernel: usize,
    min_conf: f32,
) {
    let (h, w, ch) = (dims[1], dims[2], dims[3]);
    let off = (kernel - 1) / 2;
    for (i, p) in pts.iter_mut().enumerate().take(ch) {
        let (col, row) = ((p[0] * w as f32) as i64, (p[1] * h as f32) as i64);
        if col < 0 || row < 0 || col >= w as i64 || row >= h as i64 {
            continue;
        }
        let (col, row) = (col as usize, row as usize);
        let (mut sum, mut wc, mut wr, mut max) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for r in row.saturating_sub(off)..(row + off + 1).min(h) {
            for c in col.saturating_sub(off)..(col + off + 1).min(w) {
                let conf = 1.0 / (1.0 + (-hm[(r * w + c) * ch + i]).exp());
                sum += conf;
                max = max.max(conf);
                wc += c as f32 * conf;
                wr += r as f32 * conf;
            }
        }
        if max >= min_conf && sum > 0.0 {
            p[0] = wc / w as f32 / sum;
            p[1] = wr / h as f32 / sum;
        }
    }
}
