import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { NodeParamContext, PortValue } from '../types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Apply a 3×3 convolution kernel to an image — edge detection,
 * sharpen, emboss, and a Sobel-magnitude variant.
 *
 * Each output pixel is the weighted sum of its 3×3 neighbourhood,
 * computed per RGB channel; alpha is passed through unchanged. Edges
 * are handled by clamping the sample coordinate to the image bounds.
 *
 * Sobel is implemented as `sqrt(Gx² + Gy²)` per channel so it produces
 * direction-agnostic edge magnitude instead of the one-sided emboss
 * look of a single-direction kernel.
 *
 * `amount` (0..1) cross-fades between the source and the convolved
 * result — 0 leaves the image untouched, 1 is the full effect.
 */

type ConvMode = 'edge' | 'edge-sobel' | 'sharpen' | 'emboss';

interface Kernel {
  data: ReadonlyArray<number>;
  bias?: number;
}

const KERNELS: Record<Exclude<ConvMode, 'edge-sobel'>, Kernel> = {
  // Laplacian — sums to zero, surfaces second-derivative edges.
  edge: { data: [0, -1, 0, -1, 4, -1, 0, -1, 0] },
  // Sharpen — center weighted positive, neighbours subtract.
  sharpen: { data: [0, -1, 0, -1, 5, -1, 0, -1, 0] },
  // Emboss — top-left dark, bottom-right bright. Bias 128 to recenter.
  emboss: { data: [-2, -1, 0, -1, 1, 1, 0, 1, 2], bias: 128 },
};

const MODES: ReadonlyArray<ConvMode> = ['edge', 'edge-sobel', 'sharpen', 'emboss'];

function applyKernel(
  srcData: Uint8ClampedArray,
  dstData: Uint8ClampedArray,
  w: number,
  h: number,
  kernel: ReadonlyArray<number>,
  bias: number,
) {
  // Pull 9 kernel weights into locals so the hot inner loop doesn't
  // pay for `noUncheckedIndexedAccess` undefined-check overhead.
  const k0 = kernel[0] ?? 0, k1 = kernel[1] ?? 0, k2 = kernel[2] ?? 0;
  const k3 = kernel[3] ?? 0, k4 = kernel[4] ?? 0, k5 = kernel[5] ?? 0;
  const k6 = kernel[6] ?? 0, k7 = kernel[7] ?? 0, k8 = kernel[8] ?? 0;
  // Manual 3×3 unroll. Boundary handled by clamp.
  for (let y = 0; y < h; y++) {
    const ym = Math.max(0, y - 1);
    const yp = Math.min(h - 1, y + 1);
    for (let x = 0; x < w; x++) {
      const xm = Math.max(0, x - 1);
      const xp = Math.min(w - 1, x + 1);
      const tl = (ym * w + xm) * 4;
      const tc = (ym * w + x) * 4;
      const tr = (ym * w + xp) * 4;
      const ml = (y * w + xm) * 4;
      const mc = (y * w + x) * 4;
      const mr = (y * w + xp) * 4;
      const bl = (yp * w + xm) * 4;
      const bc = (yp * w + x) * 4;
      const br = (yp * w + xp) * 4;
      for (let ch = 0; ch < 3; ch++) {
        const v =
          srcData[tl + ch]! * k0 +
          srcData[tc + ch]! * k1 +
          srcData[tr + ch]! * k2 +
          srcData[ml + ch]! * k3 +
          srcData[mc + ch]! * k4 +
          srcData[mr + ch]! * k5 +
          srcData[bl + ch]! * k6 +
          srcData[bc + ch]! * k7 +
          srcData[br + ch]! * k8 +
          bias;
        dstData[mc + ch] = v < 0 ? 0 : v > 255 ? 255 : v;
      }
      dstData[mc + 3] = srcData[mc + 3] ?? 255;
    }
  }
}

function applySobelMagnitude(
  srcData: Uint8ClampedArray,
  dstData: Uint8ClampedArray,
  w: number,
  h: number,
) {
  // Sobel-X coefficients: [-1,0,1,-2,0,2,-1,0,1] — center column is 0.
  // Sobel-Y coefficients: [-1,-2,-1,0,0,0,1,2,1] — center row is 0.
  for (let y = 0; y < h; y++) {
    const ym = Math.max(0, y - 1);
    const yp = Math.min(h - 1, y + 1);
    for (let x = 0; x < w; x++) {
      const xm = Math.max(0, x - 1);
      const xp = Math.min(w - 1, x + 1);
      const tl = (ym * w + xm) * 4;
      const tc = (ym * w + x) * 4;
      const tr = (ym * w + xp) * 4;
      const ml = (y * w + xm) * 4;
      const mr = (y * w + xp) * 4;
      const bl = (yp * w + xm) * 4;
      const bc = (yp * w + x) * 4;
      const br = (yp * w + xp) * 4;
      const mc = (y * w + x) * 4;
      for (let ch = 0; ch < 3; ch++) {
        const gx =
          -srcData[tl + ch]! + srcData[tr + ch]!
          - 2 * srcData[ml + ch]! + 2 * srcData[mr + ch]!
          - srcData[bl + ch]! + srcData[br + ch]!;
        const gy =
          -srcData[tl + ch]! - 2 * srcData[tc + ch]! - srcData[tr + ch]!
          + srcData[bl + ch]! + 2 * srcData[bc + ch]! + srcData[br + ch]!;
        const mag = Math.sqrt(gx * gx + gy * gy);
        dstData[mc + ch] = mag > 255 ? 255 : mag;
      }
      dstData[mc + 3] = srcData[mc + 3] ?? 255;
    }
  }
}

export const imageConvolveNode = registerNode({
  type: 'image/convolve',
  category: 'Image',
  label: 'Convolve',
  description: 'Apply a 3×3 convolution kernel (edge, sharpen, emboss).',
  defaultParams: { mode: 'edge' as ConvMode },
  paramOptions: { mode: MODES as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'amount', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const src = (inputs[0] ?? [])[0]  as unknown | null;
    const amount = Math.max(0, Math.min(1, ((inputs[1] ?? [])[0] as number | undefined) ?? 1));
    const mode = params.param<ConvMode>('mode', 'edge');

    if (!src || !isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const w = src.width;
    const h = src.height;
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    // Pull source pixels.
    ctx.drawImage(src, 0, 0);
    const srcImg = ctx.getImageData(0, 0, w, h);
    const dstImg = ctx.createImageData(w, h);

    if (mode === 'edge-sobel') {
      applySobelMagnitude(srcImg.data, dstImg.data, w, h);
    } else {
      const k = KERNELS[mode];
      applyKernel(srcImg.data, dstImg.data, w, h, k.data, k.bias ?? 0);
    }

    // Cross-fade source ↔ result if amount < 1.
    if (amount < 1) {
      const inv = 1 - amount;
      const sd = srcImg.data;
      const dd = dstImg.data;
      for (let i = 0; i < dd.length; i += 4) {
        dd[i] = (sd[i] ?? 0) * inv + (dd[i] ?? 0) * amount;
        dd[i + 1] = (sd[i + 1] ?? 0) * inv + (dd[i + 1] ?? 0) * amount;
        dd[i + 2] = (sd[i + 2] ?? 0) * inv + (dd[i + 2] ?? 0) * amount;
        dd[i + 3] = sd[i + 3] ?? 255;
      }
    }
    ctx.putImageData(dstImg, 0, 0);
    ctx.restore();
    return [[canvas]];
  },
});
