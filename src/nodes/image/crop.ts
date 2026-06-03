import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { NodeParamContext, PortValue } from '../types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Crop a source image into a derived image.
 *
 * Produces an `image` whose pixel dimensions are `(sw, sh)` and whose
 * contents are the source's `(sx, sy, sw, sh)` rectangle. Backed by an
 * offscreen canvas owned by this node via the derived-image registry,
 * so the output canvas is reused across frames — no per-tick GC churn.
 *
 * Zero-as-natural: a zero `sw` or `sh` runs the rect to the image edge.
 * If the source is loading or the rect is empty, returns a 1×1 placeholder
 * canvas — same convention the URL registry uses.
 */
export const imageCropNode = registerNode({
  type: 'image/crop',
  category: 'Image',
  label: 'Crop',
  description: 'Crop an image to the rectangle (sx, sy, sw, sh).',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'sx', type: 'number', default: 0 },
    { name: 'sy', type: 'number', default: 0 },
    { name: 'sw', type: 'number', default: 0 },
    { name: 'sh', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const src = (inputs[0] ?? [])[0]  as unknown | null;
    const sx = ((inputs[1] ?? [])[0] as number | undefined) ?? 0;
    const sy = ((inputs[2] ?? [])[0] as number | undefined) ?? 0;
    let sw = ((inputs[3] ?? [])[0] as number | undefined) ?? 0;
    let sh = ((inputs[4] ?? [])[0] as number | undefined) ?? 0;

    if (!src || !isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    if (sw === 0) sw = src.width - sx;
    if (sh === 0) sh = src.height - sy;
    const w = Math.max(1, Math.floor(sw));
    const h = Math.max(1, Math.floor(sh));
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    ctx.clearRect(0, 0, w, h);
    // drawImage is happy with float source coords — no need to floor.
    ctx.drawImage(src, sx, sy, sw, sh, 0, 0, w, h);
    return [[canvas]];
  },
});
