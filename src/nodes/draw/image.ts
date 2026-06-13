import { registerNode } from '../registry';
import { type Color, type Layer, colorToCss } from '../../runtime/types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Draw an `image` (HTMLCanvasElement) at (x, y) sized (w, h). If
 * either dimension is 0, the image draws at its natural pixel size.
 *
 * `tint` is an optional colour multiplied over the image (chromatic-
 * aberration / glitch effects). Default `null` means no tint.
 *
 * Renders nothing while the image is still loading — the registry
 * hands out a 1×1 placeholder until the bytes arrive.
 */
export const drawImageNode = registerNode({
  type: 'draw/image',
  category: 'Draw',
  label: 'Image',
  description: 'Draw an image at (x, y) with size (w, h).',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'w', type: 'number', default: 0 },
    { name: 'h', type: 'number', default: 0 },
    { name: 'tint', type: 'color', default: null },
  ],
  outputs: [{ name: 'layer', type: 'layer' }],
  compute: (img, x, y, w, h, tint) => {
    if (!isReadyImageCanvas(img)) {
      return [{ kind: 'group', children: [] } as Layer];
    }
    const c = img;
    const xv = x as number;
    const yv = y as number;
    const wv = w as number;
    const hv = h as number;
    const dstW = wv === 0 ? c.width : wv;
    const dstH = hv === 0 ? c.height : hv;
    const tc = tint as Color | null;
    const node: Layer = {
      kind: 'image',
      source: c,
      src: null,
      dst: { x: xv, y: yv, w: dstW, h: dstH },
      tint: tc ? colorToCss(tc) : null,
    };
    return [node];
  },
});
