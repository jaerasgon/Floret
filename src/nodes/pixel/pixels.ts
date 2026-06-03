import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import { isReadyImageCanvas } from '../_shared/coerce';
import { colorFromBytes, colorToBytes } from '../_shared/color-bytes';
import type { Color, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Whole-image pixel primitives.
 *
 *   pixel/get-pixels: image → Spread<Color> (length w*h, row-major) + w + h
 *   pixel/set-pixels: image + Spread<Color> → image (writes into a derived
 *                     canvas; missing entries fall back to the source pixel
 *                     so partial / per-row writes compose cleanly).
 *
 * These are the flexible-but-slow path: a 500×500 image is a 250 000-element
 * spread, so they're appropriate for global ops with `spread/sort`,
 * `spread/shuffle`, etc., but for the common "sort each row by brightness"
 * case use the specialised `pixel/sort` node — it stays in
 * `ImageData.data` and avoids the spread allocation entirely.
 */

export const pixelGetPixelsNode = registerNode({
  type: 'pixel/get-pixels',
  category: 'Pixel',
  label: 'GetPixels',
  description: 'Read every pixel of an image as a row-major spread of colors.',
  inputs: [{ name: 'image', type: 'image' }],
  outputs: [
    { name: 'pixels', type: 'color',  kind: 'spread' },
    { name: 'width',  type: 'number' },
    { name: 'height', type: 'number' },
  ],
  computeRaw: (inputs: PortValue[]): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    if (!isReadyImageCanvas(src)) return [[], [0], [0]];
    const ctx = src.getContext('2d');
    if (!ctx) return [[], [0], [0]];
    let data: Uint8ClampedArray;
    try {
      data = ctx.getImageData(0, 0, src.width, src.height).data;
    } catch {
      return [[], [0], [0]];
    }
    const n = src.width * src.height;
    const pixels: Color[] = new Array(n);
    for (let i = 0; i < n; i++) pixels[i] = colorFromBytes(data, i * 4);
    return [pixels, [src.width], [src.height]];
  },
});

export const pixelSetPixelsNode = registerNode({
  type: 'pixel/set-pixels',
  category: 'Pixel',
  label: 'SetPixels',
  description: 'Write a spread of colors into a copy of the source image.',
  inputs: [
    { name: 'image',  type: 'image' },
    { name: 'pixels', type: 'color', kind: 'spread' },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const src = (inputs[0] ?? [])[0];
    const pixels = (inputs[1] ?? []) as ReadonlyArray<Color | undefined>;
    if (!isReadyImageCanvas(src)) {
      const out = derivedImageRegistry.ensure(params.nodeId);
      return [[out]];
    }
    const w = src.width;
    const h = src.height;
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    // Seed the derived canvas with the source so unwritten pixels fall back
    // to the source (rather than the previous frame's leftovers or
    // transparent black). Then overlay the spread.
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.drawImage(src, 0, 0);
    const img = ctx.getImageData(0, 0, w, h);
    const limit = Math.min(pixels.length, w * h);
    for (let i = 0; i < limit; i++) {
      const c = pixels[i];
      if (!c) continue;
      colorToBytes(c, img.data, i * 4);
    }
    ctx.putImageData(img, 0, 0);
    ctx.restore();
    return [[canvas]];
  },
});
