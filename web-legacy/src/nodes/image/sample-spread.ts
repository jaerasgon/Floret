import { registerNode } from '../registry';
import type { Color, PortValue, Vec2 } from '../../runtime/types';
import { isReadyImageCanvas } from '../_shared/coerce';

const TRANSPARENT: Color = { r: 0, g: 0, b: 0, a: 0 };

/**
 * Sample a spread of points from an image, producing a spread of Colors.
 *
 * Does one full-bitmap `getImageData(0, 0, w, h)` per call and then
 * indexes into the resulting `Uint8ClampedArray` — so the cost is
 * O(width × height + points) rather than O(points) `getImageData`
 * calls. Worth it past ~50 points; cheap even for thousands.
 *
 * Tainted canvases (cross-origin without CORS headers) throw on the
 * read — we return a spread of transparent black so the patch keeps
 * running.
 */
export const imageSampleSpreadNode = registerNode({
  type: 'image/sample-spread',
  category: 'Image',
  label: 'SampleSpread',
  description: 'Read a spread of points from an image as a spread of colors.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'points', type: 'vec2', kind: 'spread' },
  ],
  outputs: [{ name: 'colors', type: 'color', kind: 'spread' }],
  computeRaw: (inputs: PortValue[]) => {
    const cRaw = (inputs[0] ?? [])[0];
    const points = (inputs[1] ?? []) as ReadonlyArray<Vec2>;
    if (!isReadyImageCanvas(cRaw) || points.length === 0) {
      return [points.map(() => TRANSPARENT)];
    }
    const c = cRaw;
    const ctx = c.getContext('2d');
    if (!ctx) return [points.map(() => TRANSPARENT)];
    let data: Uint8ClampedArray;
    try {
      data = ctx.getImageData(0, 0, c.width, c.height).data;
    } catch {
      return [points.map(() => TRANSPARENT)];
    }
    const out: Color[] = new Array(points.length);
    const w = c.width;
    const h = c.height;
    for (let i = 0; i < points.length; i++) {
      const p = points[i]!;
      const ix = Math.max(0, Math.min(w - 1, Math.floor(p.x)));
      const iy = Math.max(0, Math.min(h - 1, Math.floor(p.y)));
      const o = (iy * w + ix) * 4;
      out[i] = {
        r: data[o]! / 255,
        g: data[o + 1]! / 255,
        b: data[o + 2]! / 255,
        a: data[o + 3]! / 255,
      };
    }
    return [out];
  },
});
