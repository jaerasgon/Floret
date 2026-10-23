import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import { isReadyImageCanvas } from '../_shared/coerce';
import { colorFromBytes, colorToBytes } from '../_shared/color-bytes';
import type { Color, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Per-axis pixel access: one row or column of an image as `Spread<Color>`,
 * and the inverse write-back nodes.
 *
 * Pair with `region/foreach` + a Read/Write accumulator pair (image)
 * to build per-row / per-column transforms without paying the
 * full-image Spread<Color> cost: only the active row/col flows
 * through the spread layer (≤500 elements for a typical canvas).
 *
 *   image → ForEach(0..h-1) ──┐
 *                             │
 *   image ──→ Read(init) → read → get-row → spread/sort → set-row → Write ──→ final
 *                                                                              ↓
 *                                                                          rendered
 *
 * Each set-row / set-col uses the same `derivedImageRegistry.ensureGraphics`
 * canvas across iterations, so successive writes inside the region
 * compose on a stable surface; the accumulator threads that surface
 * across iterations as the evolving image.
 */

function clampInt(v: unknown, lo: number, hi: number): number {
  const n = Math.floor(Number(v ?? 0));
  return n < lo ? lo : n > hi ? hi : n;
}

// ─── Getters ───────────────────────────────────────────────────────────────

export const pixelGetRowNode = registerNode({
  type: 'pixel/get-row',
  category: 'Pixel',
  label: 'GetRow',
  description: 'Read one row of pixels (clamped to image bounds) as a spread of colors.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'y',     type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'pixels', type: 'color', kind: 'spread' }],
  computeRaw: (inputs: PortValue[]): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    if (!isReadyImageCanvas(src)) return [[]];
    const ctx = src.getContext('2d');
    if (!ctx) return [[]];
    const w = src.width;
    const h = src.height;
    const y = clampInt((inputs[1] ?? [])[0], 0, h - 1);
    let data: Uint8ClampedArray;
    try {
      data = ctx.getImageData(0, y, w, 1).data;
    } catch {
      return [[]];
    }
    const pixels: Color[] = new Array(w);
    for (let x = 0; x < w; x++) pixels[x] = colorFromBytes(data, x * 4);
    return [pixels];
  },
});

export const pixelGetColNode = registerNode({
  type: 'pixel/get-col',
  category: 'Pixel',
  label: 'GetCol',
  description: 'Read one column of pixels (clamped to image bounds) as a spread of colors.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'x',     type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'pixels', type: 'color', kind: 'spread' }],
  computeRaw: (inputs: PortValue[]): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    if (!isReadyImageCanvas(src)) return [[]];
    const ctx = src.getContext('2d');
    if (!ctx) return [[]];
    const w = src.width;
    const h = src.height;
    const x = clampInt((inputs[1] ?? [])[0], 0, w - 1);
    let data: Uint8ClampedArray;
    try {
      data = ctx.getImageData(x, 0, 1, h).data;
    } catch {
      return [[]];
    }
    const pixels: Color[] = new Array(h);
    for (let y = 0; y < h; y++) pixels[y] = colorFromBytes(data, y * 4);
    return [pixels];
  },
});

// ─── Setters ───────────────────────────────────────────────────────────────

/**
 * Shared body for set-row / set-col. Returns the derived canvas seeded
 * with the source image, with `pixels` written along the specified
 * axis. `axis = 'row'` writes a horizontal line at `index`; `axis = 'col'`
 * writes a vertical line.
 */
function writeAxisLine(
  src: HTMLCanvasElement | OffscreenCanvas,
  pixels: ReadonlyArray<Color | undefined>,
  axis: 'row' | 'col',
  index: number,
  nodeId: string,
): HTMLCanvasElement | OffscreenCanvas {
  const w = src.width;
  const h = src.height;
  const { canvas, ctx } = derivedImageRegistry.ensureGraphics(nodeId, w, h);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, w, h);
  ctx.drawImage(src, 0, 0);
  if (axis === 'row') {
    const y = index < 0 ? 0 : index > h - 1 ? h - 1 : index;
    const line = ctx.getImageData(0, y, w, 1);
    const limit = Math.min(pixels.length, w);
    for (let x = 0; x < limit; x++) {
      const c = pixels[x];
      if (!c) continue;
      colorToBytes(c, line.data, x * 4);
    }
    ctx.putImageData(line, 0, y);
  } else {
    const x = index < 0 ? 0 : index > w - 1 ? w - 1 : index;
    const line = ctx.getImageData(x, 0, 1, h);
    const limit = Math.min(pixels.length, h);
    for (let y = 0; y < limit; y++) {
      const c = pixels[y];
      if (!c) continue;
      colorToBytes(c, line.data, y * 4);
    }
    ctx.putImageData(line, x, 0);
  }
  ctx.restore();
  return canvas;
}

export const pixelSetRowNode = registerNode({
  type: 'pixel/set-row',
  category: 'Pixel',
  label: 'SetRow',
  description: 'Write a spread of colors as one row of the image.',
  inputs: [
    { name: 'image',  type: 'image' },
    { name: 'y',      type: 'number', default: 0, integer: true },
    { name: 'pixels', type: 'color', kind: 'spread' },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    const pixels = (inputs[2] ?? []) as ReadonlyArray<Color | undefined>;
    if (!isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const y = Math.floor(Number((inputs[1] ?? [])[0] ?? 0));
    return [[writeAxisLine(src, pixels, 'row', y, params.nodeId)]];
  },
});

export const pixelSetColNode = registerNode({
  type: 'pixel/set-col',
  category: 'Pixel',
  label: 'SetCol',
  description: 'Write a spread of colors as one column of the image.',
  inputs: [
    { name: 'image',  type: 'image' },
    { name: 'x',      type: 'number', default: 0, integer: true },
    { name: 'pixels', type: 'color', kind: 'spread' },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    const pixels = (inputs[2] ?? []) as ReadonlyArray<Color | undefined>;
    if (!isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const x = Math.floor(Number((inputs[1] ?? [])[0] ?? 0));
    return [[writeAxisLine(src, pixels, 'col', x, params.nodeId)]];
  },
});
