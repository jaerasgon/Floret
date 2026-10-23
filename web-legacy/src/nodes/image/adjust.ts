import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import { isReadyImageCanvas } from '../_shared/coerce';
import type { NodeParamContext, PortValue } from '../types';

/**
 * Brightness / contrast / saturation / hue adjustments via the canvas
 * 2D `filter` property — same primitives as CSS filter, accelerated
 * by the browser.
 *
 * Inputs:
 *   - brightness (1.0 = identity, 0 = black, 2 = double)
 *   - contrast   (1.0 = identity, 0 = grey,  2 = double)
 *   - saturate   (1.0 = identity, 0 = grey,  2 = oversaturated)
 *   - hue        (degrees, 0 = identity, ±360 wraps)
 *
 * Defaults pass the image through unchanged.
 */
export const imageAdjustNode = registerNode({
  type: 'image/adjust',
  category: 'Image',
  label: 'Adjust',
  description: 'Adjust brightness, contrast, saturation, and hue.',
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'brightness', type: 'number', default: 1 },
    { name: 'contrast', type: 'number', default: 1 },
    { name: 'saturate', type: 'number', default: 1 },
    { name: 'hue', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const srcRaw = (inputs[0] ?? [])[0];
    const br = ((inputs[1] ?? [])[0] as number | undefined) ?? 1;
    const ct = ((inputs[2] ?? [])[0] as number | undefined) ?? 1;
    const sa = ((inputs[3] ?? [])[0] as number | undefined) ?? 1;
    const hu = ((inputs[4] ?? [])[0] as number | undefined) ?? 0;

    if (!isReadyImageCanvas(srcRaw)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const src = srcRaw;
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(
      params.nodeId, src.width, src.height,
    );
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    // Build the filter string. Skip identity factors to keep the
    // browser fast-path active when only one knob is moved.
    const parts: string[] = [];
    if (br !== 1) parts.push(`brightness(${Math.max(0, br)})`);
    if (ct !== 1) parts.push(`contrast(${Math.max(0, ct)})`);
    if (sa !== 1) parts.push(`saturate(${Math.max(0, sa)})`);
    if (hu !== 0) parts.push(`hue-rotate(${hu}deg)`);
    ctx.filter = parts.length > 0 ? parts.join(' ') : 'none';
    ctx.drawImage(src, 0, 0);
    ctx.filter = 'none';
    ctx.restore();
    return [[canvas]];
  },
});
