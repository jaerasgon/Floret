//! f32 kernels for the TFLite ops MediaPipe's landmark models use. NHWC,
//! batch 1. Semantics follow TFLite's reference kernels.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    None,
    Relu,
    ReluN1To1,
    Relu6,
    Tanh,
}

impl Act {
    pub fn from_tflite(code: u8) -> Result<Act, String> {
        Ok(match code {
            0 => Act::None,
            1 => Act::Relu,
            2 => Act::ReluN1To1,
            3 => Act::Relu6,
            4 => Act::Tanh,
            c => return Err(format!("unsupported fused activation {c}")),
        })
    }

    pub fn apply(self, xs: &mut [f32]) {
        match self {
            Act::None => {}
            Act::Relu => xs.iter_mut().for_each(|x| *x = x.max(0.0)),
            Act::ReluN1To1 => xs.iter_mut().for_each(|x| *x = x.clamp(-1.0, 1.0)),
            Act::Relu6 => xs.iter_mut().for_each(|x| *x = x.clamp(0.0, 6.0)),
            Act::Tanh => xs.iter_mut().for_each(|x| *x = x.tanh()),
        }
    }
}

/// Spatial geometry of a conv / pool: input and output height/width,
/// kernel, stride, dilation and the leading (top, left) padding.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub in_hw: (usize, usize),
    pub out_hw: (usize, usize),
    pub kernel: (usize, usize),
    pub stride: (usize, usize),
    pub dilation: (usize, usize),
    pub pad: (usize, usize),
}

impl Window {
    /// TFLite `SAME` / `VALID` output size and padding (extra padding goes
    /// to the bottom / right).
    pub fn new(
        in_hw: (usize, usize),
        kernel: (usize, usize),
        stride: (usize, usize),
        dilation: (usize, usize),
        same: bool,
    ) -> Window {
        let dim = |n: usize, k: usize, s: usize, d: usize| {
            let eff = (k - 1) * d + 1;
            if same {
                let out = n.div_ceil(s);
                let total = ((out - 1) * s + eff).saturating_sub(n);
                (out, total / 2)
            } else {
                ((n.saturating_sub(eff)) / s + 1, 0)
            }
        };
        let (oh, ph) = dim(in_hw.0, kernel.0, stride.0, dilation.0);
        let (ow, pw) = dim(in_hw.1, kernel.1, stride.1, dilation.1);
        Window {
            in_hw,
            out_hw: (oh, ow),
            kernel,
            stride,
            dilation,
            pad: (ph, pw),
        }
    }

    /// Input row / column for output `o` and kernel tap `k`, if inside.
    #[inline]
    fn src(o: usize, k: usize, stride: usize, dil: usize, pad: usize, n: usize) -> Option<usize> {
        let i = (o * stride + k * dil).checked_sub(pad)?;
        (i < n).then_some(i)
    }

    #[inline]
    fn y(&self, oy: usize, ky: usize) -> Option<usize> {
        Self::src(
            oy,
            ky,
            self.stride.0,
            self.dilation.0,
            self.pad.0,
            self.in_hw.0,
        )
    }

    #[inline]
    fn x(&self, ox: usize, kx: usize) -> Option<usize> {
        Self::src(
            ox,
            kx,
            self.stride.1,
            self.dilation.1,
            self.pad.1,
            self.in_hw.1,
        )
    }

    fn is_pointwise(&self) -> bool {
        self.kernel == (1, 1) && self.stride == (1, 1) && self.pad == (0, 0)
    }
}

/// `C[m×n] = A[m×k] · B` where B is stored as `n` rows of `k` (TFLite's
/// `[out, …, in]` weight layout), accumulating onto C's existing values.
fn gemm_bt(m: usize, k: usize, n: usize, a: &[f32], b: &[f32], c: &mut [f32]) {
    assert!(a.len() >= m * k && b.len() >= n * k && c.len() >= m * n);
    // SAFETY: the asserts above bound every access the strides describe.
    unsafe {
        matrixmultiply::sgemm(
            m,
            k,
            n,
            1.0,
            a.as_ptr(),
            k as isize,
            1,
            b.as_ptr(),
            1,
            k as isize,
            1.0,
            c.as_mut_ptr(),
            n as isize,
            1,
        );
    }
}

fn fill_bias(out: &mut [f32], bias: Option<&[f32]>, channels: usize) {
    match bias {
        Some(b) => out
            .chunks_exact_mut(channels)
            .for_each(|row| row.copy_from_slice(&b[..channels])),
        None => out.fill(0.0),
    }
}

/// CONV_2D. `filter` is `[out_c, kh, kw, in_c]`.
#[allow(clippy::too_many_arguments)]
pub fn conv2d(
    input: &[f32],
    in_c: usize,
    filter: &[f32],
    bias: Option<&[f32]>,
    out_c: usize,
    w: &Window,
    act: Act,
    out: &mut [f32],
    scratch: &mut Vec<f32>,
) {
    let (oh, ow) = w.out_hw;
    let m = oh * ow;
    fill_bias(out, bias, out_c);
    if w.is_pointwise() {
        gemm_bt(m, in_c, out_c, input, filter, out);
    } else {
        let (kh, kw) = w.kernel;
        let k = kh * kw * in_c;
        scratch.clear();
        scratch.resize(m * k, 0.0);
        for oy in 0..oh {
            for ox in 0..ow {
                let row = &mut scratch[(oy * ow + ox) * k..][..k];
                for ky in 0..kh {
                    let Some(iy) = w.y(oy, ky) else { continue };
                    for kx in 0..kw {
                        let Some(ix) = w.x(ox, kx) else { continue };
                        let src = &input[(iy * w.in_hw.1 + ix) * in_c..][..in_c];
                        row[(ky * kw + kx) * in_c..][..in_c].copy_from_slice(src);
                    }
                }
            }
        }
        gemm_bt(m, k, out_c, scratch, filter, out);
    }
    act.apply(out);
}

/// DEPTHWISE_CONV_2D with depth multiplier 1. `filter` is `[1, kh, kw, c]`.
pub fn depthwise(
    input: &[f32],
    c: usize,
    filter: &[f32],
    bias: Option<&[f32]>,
    w: &Window,
    act: Act,
    out: &mut [f32],
) {
    let (oh, ow) = w.out_hw;
    let (kh, kw) = w.kernel;
    fill_bias(out, bias, c);
    for oy in 0..oh {
        for ox in 0..ow {
            let acc = &mut out[(oy * ow + ox) * c..][..c];
            for ky in 0..kh {
                let Some(iy) = w.y(oy, ky) else { continue };
                for kx in 0..kw {
                    let Some(ix) = w.x(ox, kx) else { continue };
                    let src = &input[(iy * w.in_hw.1 + ix) * c..][..c];
                    let wt = &filter[(ky * kw + kx) * c..][..c];
                    for ((a, s), f) in acc.iter_mut().zip(src).zip(wt) {
                        *a += s * f;
                    }
                }
            }
        }
    }
    act.apply(out);
}

/// MAX_POOL_2D (padding cells don't take part).
pub fn max_pool(input: &[f32], c: usize, w: &Window, act: Act, out: &mut [f32]) {
    let (oh, ow) = w.out_hw;
    let (kh, kw) = w.kernel;
    out.fill(f32::NEG_INFINITY);
    for oy in 0..oh {
        for ox in 0..ow {
            let acc = &mut out[(oy * ow + ox) * c..][..c];
            for ky in 0..kh {
                let Some(iy) = w.y(oy, ky) else { continue };
                for kx in 0..kw {
                    let Some(ix) = w.x(ox, kx) else { continue };
                    let src = &input[(iy * w.in_hw.1 + ix) * c..][..c];
                    for (a, s) in acc.iter_mut().zip(src) {
                        *a = a.max(*s);
                    }
                }
            }
        }
    }
    act.apply(out);
}

/// Row-major strides of `shape`, with 0 for broadcast (size-1) axes when
/// the tensor is being stretched to `out`.
fn broadcast_strides(shape: &[usize], out: &[usize]) -> Vec<usize> {
    let pad = out.len() - shape.len();
    let mut strides = vec![0; out.len()];
    let mut s = 1;
    for i in (0..shape.len()).rev() {
        strides[pad + i] = if shape[i] == 1 { 0 } else { s };
        s *= shape[i];
    }
    strides
}

/// An elementwise binary op with NumPy broadcasting.
pub fn binary(
    a: &[f32],
    a_shape: &[usize],
    b: &[f32],
    b_shape: &[usize],
    out_shape: &[usize],
    out: &mut [f32],
    f: impl Fn(f32, f32) -> f32,
) {
    if a.len() == out.len() && b.len() == out.len() {
        for ((o, x), y) in out.iter_mut().zip(a).zip(b) {
            *o = f(*x, *y);
        }
        return;
    }
    if a.len() == out.len()
        && !b.is_empty()
        && out.len() % b.len() == 0
        && b_is_trailing(b_shape, out_shape)
    {
        for (row_o, row_a) in out.chunks_exact_mut(b.len()).zip(a.chunks_exact(b.len())) {
            for ((o, x), y) in row_o.iter_mut().zip(row_a).zip(b) {
                *o = f(*x, *y);
            }
        }
        return;
    }
    let sa = broadcast_strides(a_shape, out_shape);
    let sb = broadcast_strides(b_shape, out_shape);
    let rank = out_shape.len();
    let mut idx = vec![0usize; rank];
    for o in out.iter_mut() {
        let (mut ia, mut ib) = (0, 0);
        for d in 0..rank {
            ia += idx[d] * sa[d];
            ib += idx[d] * sb[d];
        }
        *o = f(a[ia], b[ib]);
        for d in (0..rank).rev() {
            idx[d] += 1;
            if idx[d] < out_shape[d] {
                break;
            }
            idx[d] = 0;
        }
    }
}

/// `b`'s shape is `out`'s trailing dims (after leading 1s), so it repeats
/// contiguously.
fn b_is_trailing(b_shape: &[usize], out_shape: &[usize]) -> bool {
    let b: Vec<usize> = b_shape.iter().copied().skip_while(|&d| d == 1).collect();
    out_shape.ends_with(&b)
}

pub fn logistic(input: &[f32], out: &mut [f32]) {
    for (o, x) in out.iter_mut().zip(input) {
        *o = 1.0 / (1.0 + (-x).exp());
    }
}

/// PAD with zeros. `pads[d] = (before, after)`.
pub fn pad(input: &[f32], in_shape: &[usize], pads: &[(usize, usize)], out: &mut [f32]) {
    out.fill(0.0);
    let rank = in_shape.len();
    let out_shape: Vec<usize> = (0..rank)
        .map(|d| in_shape[d] + pads[d].0 + pads[d].1)
        .collect();
    let inner = in_shape[rank - 1];
    let rows = input.len() / inner.max(1);
    let mut idx = vec![0usize; rank - 1];
    for r in 0..rows {
        let mut o = 0;
        for d in 0..rank - 1 {
            o = o * out_shape[d] + idx[d] + pads[d].0;
        }
        o = o * out_shape[rank - 1] + pads[rank - 1].0;
        out[o..o + inner].copy_from_slice(&input[r * inner..][..inner]);
        for d in (0..rank - 1).rev() {
            idx[d] += 1;
            if idx[d] < in_shape[d] {
                break;
            }
            idx[d] = 0;
        }
    }
}

/// CONCATENATION along `axis`.
pub fn concat(inputs: &[(&[f32], &[usize])], axis: usize, out: &mut [f32]) {
    let outer: usize = inputs[0].1[..axis].iter().product();
    let mut o = 0;
    for i in 0..outer {
        for (data, shape) in inputs {
            let chunk: usize = shape[axis..].iter().product();
            out[o..o + chunk].copy_from_slice(&data[i * chunk..][..chunk]);
            o += chunk;
        }
    }
}

/// RESIZE_BILINEAR on `[1, h, w, c]`.
pub fn resize_bilinear(
    input: &[f32],
    in_hw: (usize, usize),
    c: usize,
    out_hw: (usize, usize),
    align_corners: bool,
    half_pixel: bool,
    out: &mut [f32],
) {
    let scale = |i: usize, o: usize| {
        if align_corners && o > 1 {
            (i - 1) as f32 / (o - 1) as f32
        } else {
            i as f32 / o as f32
        }
    };
    let (sy, sx) = (scale(in_hw.0, out_hw.0), scale(in_hw.1, out_hw.1));
    let interp = |v: usize, s: f32, n: usize| {
        let f = if half_pixel {
            (v as f32 + 0.5) * s - 0.5
        } else {
            v as f32 * s
        };
        let lo = (f.floor().max(0.0) as usize).min(n - 1);
        let hi = (f.ceil().max(0.0) as usize).min(n - 1);
        (f - f.floor(), lo, hi)
    };
    for y in 0..out_hw.0 {
        let (fy, y0, y1) = interp(y, sy, in_hw.0);
        for x in 0..out_hw.1 {
            let (fx, x0, x1) = interp(x, sx, in_hw.1);
            let p = |yy: usize, xx: usize| &input[(yy * in_hw.1 + xx) * c..][..c];
            let (a, b, cc, d) = (p(y0, x0), p(y0, x1), p(y1, x0), p(y1, x1));
            let o = &mut out[(y * out_hw.1 + x) * c..][..c];
            for i in 0..c {
                let top = a[i] + (b[i] - a[i]) * fx;
                let bot = cc[i] + (d[i] - cc[i]) * fx;
                o[i] = top + (bot - top) * fy;
            }
        }
    }
}

/// MEAN over `axes` (sorted, deduplicated).
pub fn mean(input: &[f32], shape: &[usize], axes: &[usize], out: &mut [f32]) {
    let rank = shape.len();
    let kept: Vec<usize> = (0..rank).filter(|d| !axes.contains(d)).collect();
    out.fill(0.0);
    let mut idx = vec![0usize; rank];
    for x in input {
        let mut o = 0;
        for &d in &kept {
            o = o * shape[d] + idx[d];
        }
        out[o] += x;
        for d in (0..rank).rev() {
            idx[d] += 1;
            if idx[d] < shape[d] {
                break;
            }
            idx[d] = 0;
        }
    }
    let n: usize = axes.iter().map(|&a| shape[a]).product();
    let inv = 1.0 / n.max(1) as f32;
    out.iter_mut().for_each(|o| *o *= inv);
}

/// FULLY_CONNECTED: `input` is `rows × in_n`, `weights` `[out_n, in_n]`.
pub fn fully_connected(
    input: &[f32],
    in_n: usize,
    weights: &[f32],
    bias: Option<&[f32]>,
    out_n: usize,
    act: Act,
    out: &mut [f32],
) {
    let rows = input.len() / in_n;
    fill_bias(out, bias, out_n);
    gemm_bt(rows, in_n, out_n, input, weights, out);
    act.apply(out);
}

/// DEPTH_TO_SPACE (DCR order) on `[1, h, w, c]`.
pub fn depth_to_space(
    input: &[f32],
    in_hw: (usize, usize),
    c: usize,
    block: usize,
    out: &mut [f32],
) {
    let oc = c / (block * block);
    let ow = in_hw.1 * block;
    for y in 0..in_hw.0 {
        for x in 0..in_hw.1 {
            let src = &input[(y * in_hw.1 + x) * c..][..c];
            for dy in 0..block {
                for dx in 0..block {
                    let o = ((y * block + dy) * ow + x * block + dx) * oc;
                    out[o..o + oc].copy_from_slice(&src[(dy * block + dx) * oc..][..oc]);
                }
            }
        }
    }
}

/// IEEE half → single.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = u32::from(h >> 15) << 31;
    let exp = u32::from((h >> 10) & 0x1f);
    let man = u32::from(h & 0x3ff);
    let bits = match (exp, man) {
        (0, 0) => sign,
        (0, m) => {
            // Subnormal: renormalise.
            let shift = m.leading_zeros() - 21;
            let m = (m << shift) & 0x3ff;
            sign | ((113 - shift) << 23) | (m << 13)
        }
        (0x1f, m) => sign | 0x7f80_0000 | (m << 13),
        (e, m) => sign | ((e + 112) << 23) | (m << 13),
    };
    f32::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x7bff), 65504.0);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
        assert_eq!(f16_to_f32(0x0200), 2f32.powi(-15));
        assert!(f16_to_f32(0x7c00).is_infinite());
        assert_eq!(f16_to_f32(0x8000).to_bits(), 0x8000_0000);
    }

    #[test]
    fn same_padding_matches_tflite() {
        // 224 → 112 with a 3×3 stride-2 kernel: total pad 1, all after.
        let w = Window::new((224, 224), (3, 3), (2, 2), (1, 1), true);
        assert_eq!((w.out_hw, w.pad), ((112, 112), (0, 0)));
        let w = Window::new((5, 5), (5, 5), (1, 1), (1, 1), true);
        assert_eq!((w.out_hw, w.pad), ((5, 5), (2, 2)));
        let w = Window::new((8, 8), (2, 2), (2, 2), (1, 1), false);
        assert_eq!(w.out_hw, (4, 4));
    }

    #[test]
    fn conv_matches_direct_sum() {
        // 4×4×2 input, 3×3 kernel, 3 outputs, SAME, stride 1, vs a naive loop.
        let input: Vec<f32> = (0..32).map(|i| (i as f32 * 0.37).sin()).collect();
        let filter: Vec<f32> = (0..54).map(|i| (i as f32 * 0.11).cos()).collect();
        let bias = [0.5, -0.25, 0.0];
        let w = Window::new((4, 4), (3, 3), (1, 1), (1, 1), true);
        let mut out = vec![0.0; 48];
        conv2d(
            &input,
            2,
            &filter,
            Some(&bias),
            3,
            &w,
            Act::None,
            &mut out,
            &mut Vec::new(),
        );
        for oy in 0..4i32 {
            for ox in 0..4i32 {
                for o in 0..3 {
                    let mut s = bias[o];
                    for ky in 0..3i32 {
                        for kx in 0..3i32 {
                            let (iy, ix) = (oy + ky - 1, ox + kx - 1);
                            if !(0..4).contains(&iy) || !(0..4).contains(&ix) {
                                continue;
                            }
                            for c in 0..2 {
                                s += input[((iy * 4 + ix) * 2) as usize + c]
                                    * filter[o * 18 + ((ky * 3 + kx) * 2) as usize + c];
                            }
                        }
                    }
                    let got = out[((oy * 4 + ox) * 3) as usize + o];
                    assert!((got - s).abs() < 1e-5, "{got} vs {s}");
                }
            }
        }
    }

    #[test]
    fn broadcasting_and_reshaping_ops() {
        let mut out = vec![0.0; 6];
        binary(
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            &[1, 2, 3],
            &[10.0, 20.0, 30.0],
            &[1, 1, 3],
            &[1, 2, 3],
            &mut out,
            |a, b| a + b,
        );
        assert_eq!(out, [11.0, 22.0, 33.0, 14.0, 25.0, 36.0]);
        binary(
            &[1.0, 2.0],
            &[2, 1],
            &[10.0, 20.0, 30.0],
            &[1, 3],
            &[2, 3],
            &mut out,
            |a, b| a * b,
        );
        assert_eq!(out, [10.0, 20.0, 30.0, 20.0, 40.0, 60.0]);

        let mut out = vec![0.0; 2 * 4];
        pad(&[1.0, 2.0, 3.0, 4.0], &[2, 2], &[(0, 0), (1, 1)], &mut out);
        assert_eq!(out, [0.0, 1.0, 2.0, 0.0, 0.0, 3.0, 4.0, 0.0]);

        let mut out = vec![0.0; 6];
        concat(
            &[(&[1.0, 2.0], &[2, 1]), (&[3.0, 4.0, 5.0, 6.0], &[2, 2])],
            1,
            &mut out,
        );
        assert_eq!(out, [1.0, 3.0, 4.0, 2.0, 5.0, 6.0]);

        let mut out = vec![0.0; 2];
        mean(
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            &[1, 2, 2, 2],
            &[1, 2],
            &mut out,
        );
        assert_eq!(out, [4.0, 5.0]);

        // 1×1×1×4 → 1×2×2×1.
        let mut out = vec![0.0; 4];
        depth_to_space(&[1.0, 2.0, 3.0, 4.0], (1, 1), 4, 2, &mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn bilinear_half_pixel() {
        // 1×2 → 1×4 with half-pixel centres: [1, 1.25, 1.75, 2].
        let mut out = vec![0.0; 4];
        resize_bilinear(&[1.0, 2.0], (1, 2), 1, (1, 4), false, true, &mut out);
        assert_eq!(out, [1.0, 1.25, 1.75, 2.0]);
    }
}
