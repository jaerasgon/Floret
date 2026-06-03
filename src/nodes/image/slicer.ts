import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { NodeParamContext, PortValue } from '../types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Slice a source image into a `cols × rows` grid of sub-images.
 *
 * Outputs `Spread<image>` of length `cols × rows`, row-major (same
 * order as `spread/grid`). Each sub-image is owned by this node via
 * the derived-image registry — canvases are reused across frames so
 * the bitmap doesn't churn the GC.
 *
 * Cap: a user dragging both sliders to ~100 would allocate ~10 000
 * canvases per frame. MAX_CELLS bounds the spread length; when
 * `cols × rows` exceeds it, the larger axis is shrunk until under
 * the cap (kept as square as possible). 256 picked because 16×16
 * grids are plenty for typical creative-coding uses and a 256-entry
 * spread renders cheaply downstream.
 */
const MAX_CELLS = 256;

export const imageSlicerNode = registerNode({
  type: 'image/slicer',
  category: 'Image',
  label: 'Slicer',
  description: 'Split an image into a cols × rows grid of sub-images.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'cols', type: 'number', default: 4, integer: true },
    { name: 'rows', type: 'number', default: 4, integer: true },
  ],
  outputs: [{ name: 'images', type: 'image', kind: 'spread' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const src = (inputs[0] ?? [])[0]  as unknown | null;
    const colsRaw = Math.max(1, Math.floor(Number((inputs[1] ?? [])[0] ?? 1)));
    const rowsRaw = Math.max(1, Math.floor(Number((inputs[2] ?? [])[0] ?? 1)));
    let cols = colsRaw;
    let rows = rowsRaw;
    // Respect the cell cap. Shrink whichever axis is larger until we're
    // under the limit so the grid stays as square as possible.
    while (cols * rows > MAX_CELLS) {
      if (cols >= rows) cols--; else rows--;
    }

    const count = cols * rows;
    const canvases = derivedImageRegistry.ensureMany(params.nodeId, count);

    if (!src || !isReadyImageCanvas(src)) {
      // Source not ready — emit 1×1 placeholder canvases.
      for (const c of canvases) {
        if (c.width !== 1 || c.height !== 1) { c.width = 1; c.height = 1; }
      }
      return [canvases.slice()];
    }

    const cellW = Math.max(1, Math.floor(src.width / cols));
    const cellH = Math.max(1, Math.floor(src.height / rows));

    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const idx = r * cols + c;
        const out = canvases[idx]!;
        if (out.width !== cellW || out.height !== cellH) {
          out.width = cellW;
          out.height = cellH;
        }
        const ctx = out.getContext('2d');
        if (ctx) {
          ctx.clearRect(0, 0, cellW, cellH);
          ctx.drawImage(
            src,
            c * cellW, r * cellH, cellW, cellH, // source rect
            0, 0, cellW, cellH,                 // dest rect
          );
        }
      }
    }
    return [canvases.slice()];
  },
});
