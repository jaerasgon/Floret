import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import { isReadyImageCanvas } from '../_shared/coerce';
import type { NodeParamContext, PortValue } from '../types';

/**
 * Pixel sort — the classic glitch art technique.
 *
 * For each row (or column) of the image, identify maximal runs of pixels
 * whose criterion value falls within the `[lo, hi]` mask, sort each run
 * by that criterion, and write back. Pixels outside the mask stay put —
 * that's what produces the characteristic streaked bands rather than a
 * smooth gradient sweep.
 *
 * Stays inside `ImageData.data` (Uint8ClampedArray, RGBA bytes) the whole
 * way; no per-pixel allocation, no Spread<Color> traffic through the
 * runtime. A 500×500 image is ~250 000 pixels and runs comfortably
 * per-frame.
 *
 * For per-row work that does NOT match this template (custom sort key,
 * shuffle, reverse, …) use the `pixel/get-row` + `pixel/set-row`
 * primitives with a ForEach + image accumulator.
 */

type Axis = 'rows' | 'cols';
type Criterion =
  | 'brightness'
  | 'hue'
  | 'saturation'
  | 'lightness'
  | 'red'
  | 'green'
  | 'blue';
type Order = 'ascending' | 'descending';

const AXES: ReadonlyArray<Axis> = ['rows', 'cols'];
const CRITERIA: ReadonlyArray<Criterion> = [
  'brightness',
  'hue',
  'saturation',
  'lightness',
  'red',
  'green',
  'blue',
];
const ORDERS: ReadonlyArray<Order> = ['ascending', 'descending'];

/**
 * Criterion functions operate directly on three 0..255 bytes and return
 * a unitless number used purely for ordering (no need to normalise to
 * 0..1 since we never expose the value).
 *
 * For ordering we want a single monotonic key per pixel:
 *   brightness  = perceptual luma (Rec. 709 weights)
 *   lightness   = HSL L = (max + min) / 2
 *   saturation  = HSL S, expressed as (max - min) / 255 — close enough
 *                 for ordering (the L-dependent denominator only matters
 *                 if you need true 0..1, which we don't)
 *   hue         = 0..6 from the HSL hue calc, scaled. Wrap-around at
 *                 the red end is unavoidable without picking an origin;
 *                 the standard origin works fine for visual sorting.
 *   red/green/blue — raw channel byte
 */
type KeyFn = (r: number, g: number, b: number) => number;

const KEY: Record<Criterion, KeyFn> = {
  brightness: (r, g, b) => 0.2126 * r + 0.7152 * g + 0.0722 * b,
  lightness:  (r, g, b) => (Math.max(r, g, b) + Math.min(r, g, b)) * 0.5,
  saturation: (r, g, b) => Math.max(r, g, b) - Math.min(r, g, b),
  hue: (r, g, b) => {
    const mx = Math.max(r, g, b);
    const mn = Math.min(r, g, b);
    if (mx === mn) return 0;
    const d = mx - mn;
    let h: number;
    if (mx === r) h = (g - b) / d + (g < b ? 6 : 0);
    else if (mx === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    return h; // 0..6 — monotonic for ordering
  },
  red:   (r) => r,
  green: (_, g) => g,
  blue:  (_, __, b) => b,
};

/**
 * Sort one segment of an ImageData buffer in place. The segment is a run
 * of `length` pixels starting at `start`, with each successive pixel
 * `stride` bytes later. `stride = 4` for rows; `stride = width * 4` for
 * columns. We copy out → sort → copy back rather than swap in place,
 * because in-place RGBA sorting in a typed array is fiddlier than its
 * payoff at these scales.
 */
function sortSegment(
  data: Uint8ClampedArray,
  start: number,
  length: number,
  stride: number,
  key: KeyFn,
  descending: boolean,
): void {
  if (length < 2) return;
  // Pack (key, r, g, b, a) into a small array we can sort.
  const buf: Array<{ k: number; r: number; g: number; b: number; a: number }> =
    new Array(length);
  for (let i = 0; i < length; i++) {
    const o = start + i * stride;
    const r = data[o] ?? 0;
    const g = data[o + 1] ?? 0;
    const b = data[o + 2] ?? 0;
    const a = data[o + 3] ?? 0;
    buf[i] = { k: key(r, g, b), r, g, b, a };
  }
  buf.sort(descending ? (p, q) => q.k - p.k : (p, q) => p.k - q.k);
  for (let i = 0; i < length; i++) {
    const o = start + i * stride;
    const p = buf[i]!;
    data[o]     = p.r;
    data[o + 1] = p.g;
    data[o + 2] = p.b;
    data[o + 3] = p.a;
  }
}

/**
 * Walk a single line (row or column) and sort every maximal run of
 * pixels whose criterion key is within `[loByte, hiByte]`.
 */
function sortLine(
  data: Uint8ClampedArray,
  start: number,
  length: number,
  stride: number,
  key: KeyFn,
  descending: boolean,
  loByte: number,
  hiByte: number,
): void {
  let runStart = -1;
  let runLen = 0;
  for (let i = 0; i < length; i++) {
    const o = start + i * stride;
    const k = key(data[o] ?? 0, data[o + 1] ?? 0, data[o + 2] ?? 0);
    const inMask = k >= loByte && k <= hiByte;
    if (inMask) {
      if (runStart < 0) {
        runStart = start + i * stride;
        runLen = 1;
      } else {
        runLen++;
      }
    } else if (runStart >= 0) {
      sortSegment(data, runStart, runLen, stride, key, descending);
      runStart = -1;
      runLen = 0;
    }
  }
  if (runStart >= 0) {
    sortSegment(data, runStart, runLen, stride, key, descending);
  }
}

export const pixelSortNode = registerNode({
  type: 'pixel/sort',
  category: 'Pixel',
  label: 'PixelSort',
  description: 'Sort runs of pixels along rows or cols, masked by a value range.',
  defaultParams: {
    axis: 'rows' as Axis,
    criterion: 'brightness' as Criterion,
    order: 'ascending' as Order,
  },
  paramOptions: {
    axis: AXES as unknown as ReadonlyArray<string>,
    criterion: CRITERIA as unknown as ReadonlyArray<string>,
    order: ORDERS as unknown as ReadonlyArray<string>,
  },
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'lo',    type: 'number', default: 0.25 },
    { name: 'hi',    type: 'number', default: 0.8 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    if (!isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const lo = Number((inputs[1] ?? [])[0] ?? 0);
    const hi = Number((inputs[2] ?? [])[0] ?? 1);
    const axis = params.param<Axis>('axis', 'rows');
    const criterion = params.param<Criterion>('criterion', 'brightness');
    const order = params.param<Order>('order', 'ascending');
    const descending = order === 'descending';
    const key = KEY[criterion];

    // Mask thresholds are 0..1 on the input pins; criterion keys are in
    // 0..255 byte space (hue is 0..6 — we scale it to keep the slider
    // sensible). Normalise the bounds into the same space as the key.
    let scale: number;
    if (criterion === 'hue') scale = 6;
    else if (criterion === 'saturation') scale = 255; // sat key uses 0..255 too
    else scale = 255;
    const loByte = lo * scale;
    const hiByte = hi * scale;

    const w = src.width;
    const h = src.height;
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.drawImage(src, 0, 0);
    const img = ctx.getImageData(0, 0, w, h);

    if (axis === 'rows') {
      // Sort each row: start = y * w * 4, length = w, stride = 4
      for (let y = 0; y < h; y++) {
        sortLine(img.data, y * w * 4, w, 4, key, descending, loByte, hiByte);
      }
    } else {
      // Sort each column: start = x * 4, length = h, stride = w * 4
      const rowStride = w * 4;
      for (let x = 0; x < w; x++) {
        sortLine(img.data, x * 4, h, rowStride, key, descending, loByte, hiByte);
      }
    }

    ctx.putImageData(img, 0, 0);
    ctx.restore();
    return [[canvas]];
  },
});
