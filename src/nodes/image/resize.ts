import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { NodeParamContext, PortValue } from '../types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Resize an image into a new image of size `(w, h)`.
 *
 * Wraps `ctx.drawImage(src, 0, 0, w, h)` — the browser does smooth
 * downsampling automatically (the canvas's `imageSmoothingEnabled`
 * default is `true`). For pixel-art upscales the patch can flip
 * smoothing off later via a param if we ever need it.
 *
 * Zero-as-natural: zero `w` or `h` falls back to the source's pixel
 * dimensions (no resize), useful as a passthrough or to extract just
 * the source's aspect by feeding a single dimension.
 */
export const imageResizeNode = registerNode({
  type: 'image/resize',
  category: 'Image',
  label: 'Resize',
  description: 'Resize an image to (w, h).',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'w', type: 'number', default: 0 },
    { name: 'h', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const src = (inputs[0] ?? [])[0]  as unknown | null;
    let wIn = ((inputs[1] ?? [])[0] as number | undefined) ?? 0;
    let hIn = ((inputs[2] ?? [])[0] as number | undefined) ?? 0;

    if (!src || !isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    if (wIn <= 0) wIn = src.width;
    if (hIn <= 0) hIn = src.height;
    const w = Math.max(1, Math.floor(wIn));
    const h = Math.max(1, Math.floor(hIn));
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    ctx.clearRect(0, 0, w, h);
    ctx.drawImage(src, 0, 0, w, h);
    return [[canvas]];
  },
});
