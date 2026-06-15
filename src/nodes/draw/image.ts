import { registerNode } from '../registry';
import { type Color, type Layer, colorToCss } from '../../runtime/types';
import { isReadyImageCanvas, toVec2 } from '../_shared/coerce';
import { anchorTopLeft, ANCHOR_OPTIONS } from '../_shared/anchor';

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
  description: 'Draw an image at `pos` with size (w, h).',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'pos', type: 'vec2', default: { x: 0, y: 0 } },
    { name: 'w', type: 'number', default: 0 },
    { name: 'h', type: 'number', default: 0 },
    { name: 'anchor', type: 'string', default: 'center', param: 'anchor' },
    { name: 'tint', type: 'color', default: null },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  defaultParams: { anchor: 'center' },
  paramOptions: { anchor: [...ANCHOR_OPTIONS] },
  compute: (img, pos, w, h, anchor, tint) => {
    if (!isReadyImageCanvas(img)) {
      return [{ kind: 'group', children: [] } as Layer];
    }
    const c = img;
    const wv = w as number;
    const hv = h as number;
    const dstW = wv === 0 ? c.width : wv;
    const dstH = hv === 0 ? c.height : hv;
    const { x: xv, y: yv } = anchorTopLeft(toVec2(pos), dstW, dstH, anchor as string);
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
