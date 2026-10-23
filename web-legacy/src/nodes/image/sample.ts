import { registerNode } from '../registry';
import type { Color } from '../../runtime/types';
import { isReadyImageCanvas } from '../_shared/coerce';

const TRANSPARENT: Color = { r: 0, g: 0, b: 0, a: 0 };

/**
 * Sample a single pixel from an image as a Color.
 *
 * Coordinates are clamped to the image bounds and rounded to integer
 * pixel addresses. For tainted canvases (cross-origin servers without
 * CORS headers) `getImageData` throws — we swallow the error and return
 * transparent black so the patch keeps running.
 *
 * For many sample points per frame use `image/sample-spread` instead —
 * it does one full-bitmap read and indexes in, rather than N separate
 * `getImageData(x, y, 1, 1)` calls.
 */
export const imageSampleNode = registerNode({
  type: 'image/sample',
  category: 'Image',
  label: 'Sample',
  description: 'Read one pixel of an image as a color.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'color', type: 'color' }],
  compute: (image, x, y) => {
    const c = image  as unknown | null;
    if (!c || !isReadyImageCanvas(c)) {
      return [TRANSPARENT];
    }
    const ix = Math.max(0, Math.min(c.width - 1, Math.floor(x as number)));
    const iy = Math.max(0, Math.min(c.height - 1, Math.floor(y as number)));
    const ctx = c.getContext('2d');
    if (!ctx) return [TRANSPARENT];
    try {
      const px = ctx.getImageData(ix, iy, 1, 1).data;
      return [{ r: px[0]! / 255, g: px[1]! / 255, b: px[2]! / 255, a: px[3]! / 255 }];
    } catch {
      // Tainted canvas (cross-origin without CORS) — fail soft.
      return [TRANSPARENT];
    }
  },
});
