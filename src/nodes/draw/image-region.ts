import { registerNode } from '../registry';
import { type Color, type Layer, colorToCss } from '../../runtime/types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Draw a sub-rectangle of an `image` onto a destination rect.
 *
 *   - `sw === 0` or `sh === 0` → source rect runs to the image edge.
 *   - `dw === 0` or `dh === 0` → destination matches the source rect size.
 */
export const drawImageRegionNode = registerNode({
  type: 'draw/image-region',
  category: 'Draw',
  label: 'ImageRegion',
  description: 'Draw a sub-rectangle of an image onto a destination rect.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'sx', type: 'number', default: 0 },
    { name: 'sy', type: 'number', default: 0 },
    { name: 'sw', type: 'number', default: 0 },
    { name: 'sh', type: 'number', default: 0 },
    { name: 'dx', type: 'number', default: 0 },
    { name: 'dy', type: 'number', default: 0 },
    { name: 'dw', type: 'number', default: 0 },
    { name: 'dh', type: 'number', default: 0 },
    { name: 'tint', type: 'color', default: null },
  ],
  outputs: [{ name: 'layer', type: 'layer' }],
  compute: (image, sx, sy, sw, sh, dx, dy, dw, dh, tint) => {
    if (!isReadyImageCanvas(image)) {
      return [{ kind: 'group', children: [] } as Layer];
    }
    const c = image;
    const sxv = sx as number;
    const syv = sy as number;
    let swv = sw as number;
    let shv = sh as number;
    const dxv = dx as number;
    const dyv = dy as number;
    let dwv = dw as number;
    let dhv = dh as number;
    if (swv === 0) swv = c.width - sxv;
    if (shv === 0) shv = c.height - syv;
    if (dwv === 0) dwv = swv;
    if (dhv === 0) dhv = shv;
    if (swv <= 0 || shv <= 0 || dwv <= 0 || dhv <= 0) {
      return [{ kind: 'group', children: [] } as Layer];
    }
    const tc = tint as Color | null;
    const node: Layer = {
      kind: 'image',
      source: c,
      src: { x: sxv, y: syv, w: swv, h: shv },
      dst: { x: dxv, y: dyv, w: dwv, h: dhv },
      tint: tc ? colorToCss(tc) : null,
    };
    return [node];
  },
});
