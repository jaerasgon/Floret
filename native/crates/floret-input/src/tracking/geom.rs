//! MediaPipe's ROI geometry: rotated rects in normalised image coords,
//! cropping them into model tensors, and mapping model outputs back.

use std::f32::consts::PI;

/// An RGBA8 frame (straight alpha, row-major).
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

impl Frame<'_> {
    fn size(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }
}

/// MediaPipe's `NormalizedRect`: centre and size in 0..1 image units,
/// rotation in radians (clockwise on screen, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
}

/// A landmark in normalised image coords (z scaled like x).
pub type Point = [f32; 3];

pub fn normalize_radians(a: f32) -> f32 {
    a - 2.0 * PI * ((a + PI) / (2.0 * PI)).floor()
}

/// Rotation that turns the vector `p0 → p1` (normalised coords) to
/// `target` radians (`ComputeRotation` in the detection / landmark → rect
/// calculators).
pub fn rotation(p0: (f32, f32), p1: (f32, f32), target: f32, w: f32, h: f32) -> f32 {
    let (x0, y0) = (p0.0 * w, p0.1 * h);
    let (x1, y1) = (p1.0 * w, p1.1 * h);
    normalize_radians(target - (-(y1 - y0)).atan2(x1 - x0))
}

/// `RectTransformationCalculator`: shift, square up, scale.
pub fn transform(r: Rect, img: (f32, f32), scale: f32, shift_y: f32) -> Rect {
    let (iw, ih) = img;
    let mut out = r;
    if r.rotation == 0.0 {
        out.cy += r.h * shift_y;
    } else {
        let (s, c) = r.rotation.sin_cos();
        out.cx += (-ih * r.h * shift_y * s) / iw;
        out.cy += (ih * r.h * shift_y * c) / ih;
    }
    let long = (r.w * iw).max(r.h * ih);
    out.w = long / iw * scale;
    out.h = long / ih * scale;
    out
}

/// Normalised crop coords (0..1 across the rect) → normalised image coords.
pub fn project(r: &Rect, img: (f32, f32), x: f32, y: f32) -> (f32, f32) {
    let (iw, ih) = img;
    let (dx, dy) = ((x - 0.5) * r.w * iw, (y - 0.5) * r.h * ih);
    let (s, c) = r.rotation.sin_cos();
    (r.cx + (dx * c - dy * s) / iw, r.cy + (dx * s + dy * c) / ih)
}

/// Map crop-space landmarks to the image (`LandmarkProjectionCalculator`;
/// z scales with the rect's width).
pub fn project_points(r: &Rect, img: (f32, f32), pts: &mut [Point]) {
    for p in pts {
        let (x, y) = project(r, img, p[0], p[1]);
        *p = [x, y, p[2] * r.w];
    }
}

/// How pixels outside the frame are filled when cropping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Border {
    /// Black (letterbox padding).
    Zero,
    /// Repeat the nearest edge pixel.
    Replicate,
}

/// Sample `r` into a `size × size × 3` float tensor with values mapped from
/// 0..255 to `range` (`ImageToTensorCalculator`, bilinear).
pub fn crop(
    frame: &Frame,
    r: &Rect,
    size: usize,
    range: (f32, f32),
    border: Border,
    out: &mut [f32],
) {
    let (iw, ih) = frame.size();
    let (w, h) = (frame.width as i64, frame.height as i64);
    let (s, c) = r.rotation.sin_cos();
    let scale = (range.1 - range.0) / 255.0;
    let px = |x: i64, y: i64| -> [f32; 3] {
        let (x, y) = match border {
            Border::Replicate => (x.clamp(0, w - 1), y.clamp(0, h - 1)),
            Border::Zero if x < 0 || y < 0 || x >= w || y >= h => return [0.0; 3],
            Border::Zero => (x, y),
        };
        let o = (y * w + x) as usize * 4;
        let p = &frame.rgba[o..o + 3];
        [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]
    };
    let (rw, rh) = (r.w * iw, r.h * ih);
    for v in 0..size {
        let b = ((v as f32 + 0.5) / size as f32 - 0.5) * rh;
        for u in 0..size {
            let a = ((u as f32 + 0.5) / size as f32 - 0.5) * rw;
            // Pixel-centre coords of the source sample.
            let sx = r.cx * iw + a * c - b * s - 0.5;
            let sy = r.cy * ih + a * s + b * c - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (p00, p10, p01, p11) = (
                px(x0, y0),
                px(x0 + 1, y0),
                px(x0, y0 + 1),
                px(x0 + 1, y0 + 1),
            );
            let o = &mut out[(v * size + u) * 3..][..3];
            for k in 0..3 {
                let top = p00[k] + (p10[k] - p00[k]) * fx;
                let bot = p01[k] + (p11[k] - p01[k]) * fx;
                o[k] = range.0 + (top + (bot - top) * fy) * scale;
            }
        }
    }
}

/// The whole frame letterboxed into a square (keep aspect ratio, pad).
pub fn letterbox_rect(img: (f32, f32)) -> Rect {
    let long = img.0.max(img.1);
    Rect {
        cx: 0.5,
        cy: 0.5,
        w: long / img.0,
        h: long / img.1,
        rotation: 0.0,
    }
}

/// Axis-aligned bounds `(xmin, ymin, xmax, ymax)` of a rotated rect.
pub fn bounds(r: &Rect, img: (f32, f32)) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let (px, py) = project(r, img, x, y);
        b = (b.0.min(px), b.1.min(py), b.2.max(px), b.3.max(py));
    }
    b
}

pub fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let iw = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
    let ih = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
    let inter = iw * ih;
    let union = (a.2 - a.0) * (a.3 - a.1) + (b.2 - b.0) * (b.3 - b.1) - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// `AlignmentPointsRectsCalculator`: centred on `center`, sized by twice
/// the distance to `scale` (pose ROI from detector keypoints or the
/// auxiliary landmarks).
pub fn alignment_rect(center: (f32, f32), scale: (f32, f32), target: f32, img: (f32, f32)) -> Rect {
    let (iw, ih) = img;
    let (dx, dy) = ((scale.0 - center.0) * iw, (scale.1 - center.1) * ih);
    let size = (dx * dx + dy * dy).sqrt() * 2.0;
    Rect {
        cx: center.0,
        cy: center.1,
        w: size / iw,
        h: size / ih,
        rotation: rotation(center, scale, target, iw, ih),
    }
}

/// `DetectionsToRectsCalculator` on a box: its centre and size, rotated by
/// the `p0 → p1` keypoint vector.
pub fn box_rect(
    bbox: (f32, f32, f32, f32),
    p0: (f32, f32),
    p1: (f32, f32),
    target: f32,
    img: (f32, f32),
) -> Rect {
    Rect {
        cx: (bbox.0 + bbox.2) / 2.0,
        cy: (bbox.1 + bbox.3) / 2.0,
        w: bbox.2 - bbox.0,
        h: bbox.3 - bbox.1,
        rotation: rotation(p0, p1, target, img.0, img.1),
    }
}

/// `HandLandmarksToRectCalculator`: a rect around the palm landmarks,
/// aligned wrist → middle finger.
pub fn hand_rect(lm: &[Point], img: (f32, f32)) -> Rect {
    const PALM: [usize; 12] = [0, 1, 2, 3, 5, 6, 9, 10, 13, 14, 17, 18];
    let (iw, ih) = img;
    let p = |i: usize| (lm[i][0], lm[i][1]);
    let (x1, y1) = ((p(5).0 + p(13).0) / 2.0, (p(5).1 + p(13).1) / 2.0);
    let mid = ((x1 + p(9).0) / 2.0, (y1 + p(9).1) / 2.0);
    let rot = rotation(p(0), mid, PI / 2.0, iw, ih);

    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for &i in &PALM {
        lo = (lo.0.min(lm[i][0]), lo.1.min(lm[i][1]));
        hi = (hi.0.max(lm[i][0]), hi.1.max(lm[i][1]));
    }
    let axis_c = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
    let (s, c) = (-rot).sin_cos();
    let (mut plo, mut phi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for &i in &PALM {
        let (ox, oy) = ((lm[i][0] - axis_c.0) * iw, (lm[i][1] - axis_c.1) * ih);
        let (px, py) = (ox * c - oy * s, ox * s + oy * c);
        plo = (plo.0.min(px), plo.1.min(py));
        phi = (phi.0.max(px), phi.1.max(py));
    }
    let pc = ((plo.0 + phi.0) / 2.0, (plo.1 + phi.1) / 2.0);
    let (s, c) = rot.sin_cos();
    Rect {
        cx: (pc.0 * c - pc.1 * s + iw * axis_c.0) / iw,
        cy: (pc.0 * s + pc.1 * c + ih * axis_c.1) / ih,
        w: (phi.0 - plo.0) / iw,
        h: (phi.1 - plo.1) / ih,
        rotation: rot,
    }
}

/// Bounding box `(xmin, ymin, xmax, ymax)` of landmarks.
pub fn landmark_bounds(lm: &[Point]) -> (f32, f32, f32, f32) {
    lm.iter()
        .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| {
            (b.0.min(p[0]), b.1.min(p[1]), b.2.max(p[0]), b.3.max(p[1]))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn rotation_targets_and_projection() {
        // Wrist below the middle finger: already upright for a 90° target.
        assert!(close(
            rotation((0.5, 0.8), (0.5, 0.2), PI / 2.0, 640.0, 480.0),
            0.0
        ));
        // Pointing right → rotate a quarter turn.
        assert!(close(
            rotation((0.2, 0.5), (0.8, 0.5), PI / 2.0, 640.0, 480.0),
            PI / 2.0
        ));
        let r = Rect {
            cx: 0.5,
            cy: 0.5,
            w: 0.5,
            h: 0.5,
            rotation: PI / 2.0,
        };
        // Crop top-left corner lands at image top-right when turned 90°.
        let (x, y) = project(&r, (100.0, 100.0), 0.0, 0.0);
        assert!(close(x, 0.75) && close(y, 0.25), "{x} {y}");
    }

    #[test]
    fn transform_squares_long_side() {
        let r = Rect {
            cx: 0.5,
            cy: 0.5,
            w: 0.1,
            h: 0.4,
            rotation: 0.0,
        };
        let t = transform(r, (200.0, 100.0), 2.0, -0.5);
        // Long side 40 px → 80 px square: 0.4 of the width, 0.8 of the height.
        assert!(close(t.w, 0.4) && close(t.h, 0.8));
        assert!(close(t.cy, 0.3));
    }

    #[test]
    fn crop_identity_and_letterbox() {
        // 2×1 frame: red, blue. Letterboxed into 2×2: top row black, then
        // ... the frame occupies the middle band only.
        let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
        let f = Frame {
            width: 2,
            height: 1,
            rgba: &rgba,
        };
        let mut out = vec![0.0; 2 * 2 * 3];
        crop(
            &f,
            &letterbox_rect((2.0, 1.0)),
            2,
            (0.0, 1.0),
            Border::Zero,
            &mut out,
        );
        // Sample centres at y = ±0.5 px from the frame's centre line: half
        // black, half colour.
        assert!(close(out[0], 0.5) && close(out[2], 0.0));
        assert!(close(out[3 + 2], 0.5));
        let mut out = vec![0.0; 2 * 3];
        crop(
            &f,
            &Rect {
                cx: 0.5,
                cy: 0.5,
                w: 1.0,
                h: 1.0,
                rotation: 0.0,
            },
            1,
            (-1.0, 1.0),
            Border::Replicate,
            &mut out,
        );
        // One sample at the centre: mean of red and blue.
        assert!(close(out[0], 0.0) && close(out[1], -1.0) && close(out[2], 0.0));
    }
}
