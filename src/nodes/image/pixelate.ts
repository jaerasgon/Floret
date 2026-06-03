import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { NodeParamContext, PortValue } from '../types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Pixelate an image — downscale to a smaller resolution then upscale
 * back with nearest-neighbor sampling. The `size` input is the side
 * length of each chunky pixel in source pixels (size=8 → cells are
 * 8×8 of the source).
 *
 * Implemented as a two-step canvas draw via a temporary downscale
 * surface. `imageSmoothingEnabled = false` on the upscale step gives
 * crisp pixel boundaries — no bilinear blur between chunks.
 *
 * Output dimensions match the source.
 */
export const imagePixelateNode = registerNode({
  type: 'image/pixelate',
  category: 'Image',
  label: 'Pixelate',
  description: 'Pixelate an image into chunky blocks of `size` source pixels.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'size', type: 'number', default: 8, integer: true },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const src = (inputs[0] ?? [])[0]  as unknown | null;
    const sizeIn = ((inputs[1] ?? [])[0] as number | undefined) ?? 8;
    const size = Math.max(1, Math.floor(sizeIn));

    if (!src || !isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const w = src.width;
    const h = src.height;
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const smallW = Math.max(1, Math.floor(w / size));
    const smallH = Math.max(1, Math.floor(h / size));
    const prev = ctx.imageSmoothingEnabled;
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(src, 0, 0, w, h, 0, 0, smallW, smallH);
    ctx.imageSmoothingEnabled = false;
    // Now upscale our own downscaled region back. Draw from the canvas
    // back onto itself — the source rect (0,0,smallW,smallH) holds the
    // freshly written downscaled image.
    ctx.drawImage(canvas, 0, 0, smallW, smallH, 0, 0, w, h);
    ctx.imageSmoothingEnabled = prev;
    ctx.restore();
    return [[canvas]];
  },
});
