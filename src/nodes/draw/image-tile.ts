import { registerNode } from '../registry';
import { type Color, type Layer, colorToCss } from '../../runtime/types';
import type { SceneNode } from '../../runtime/scene';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Tile an image `cols × rows` times across a (w × h) rect anchored at
 * (x, y). Each cell shows the full source image stretched into a
 * `(w/cols, h/rows)` box. `tint` applies uniformly to every tile.
 *
 * Implementation: emit one `image` SceneNode per cell, all under a
 * single Group. The scene renderer walks them in order; no per-frame
 * imperative loop.
 */
export const drawImageTileNode = registerNode({
  type: 'draw/image-tile',
  category: 'Draw',
  label: 'ImageTile',
  description: 'Tile an image `cols × rows` times across a rect.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'w', type: 'number', default: 200 },
    { name: 'h', type: 'number', default: 200 },
    { name: 'cols', type: 'number', default: 4, integer: true },
    { name: 'rows', type: 'number', default: 4, integer: true },
    { name: 'tint', type: 'color', default: null },
  ],
  outputs: [{ name: 'layer', type: 'layer' }],
  compute: (image, x, y, w, h, cols, rows, tint) => {
    if (!isReadyImageCanvas(image)) {
      return [{ kind: 'group', children: [] } as Layer];
    }
    const c = image;
    const xv = x as number;
    const yv = y as number;
    const wv = w as number;
    const hv = h as number;
    if (wv <= 0 || hv <= 0) {
      return [{ kind: 'group', children: [] } as Layer];
    }
    const colsv = Math.max(1, Math.floor(cols as number));
    const rowsv = Math.max(1, Math.floor(rows as number));
    const cw = wv / colsv;
    const ch = hv / rowsv;
    const tintCss = tint ? colorToCss(tint as Color) : null;

    const children: SceneNode[] = [];
    for (let r = 0; r < rowsv; r++) {
      for (let cIdx = 0; cIdx < colsv; cIdx++) {
        children.push({
          kind: 'image',
          source: c,
          src: null,
          dst: { x: xv + cIdx * cw, y: yv + r * ch, w: cw, h: ch },
          tint: tintCss,
        });
      }
    }
    return [{ kind: 'group', children } as Layer];
  },
});
