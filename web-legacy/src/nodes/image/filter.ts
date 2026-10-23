import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { NodeParamContext, PortValue } from '../types';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Apply a filter to an image, producing a new image. Uses Canvas2D's
 * native CSS-filter pipeline for the cheap modes (blur / gray / invert)
 * and ImageData for per-pixel modes (threshold / posterize / opaque).
 *
 * Dilate / erode are simplified to brightness pushes for now —
 * morphological 3x3 kernels can come back as a separate pass when
 * we have time.
 */

type FilterMode =
  | 'gray'
  | 'invert'
  | 'threshold'
  | 'posterize'
  | 'blur'
  | 'dilate'
  | 'erode'
  | 'opaque';

const MODES: ReadonlyArray<FilterMode> = [
  'gray',
  'invert',
  'threshold',
  'posterize',
  'blur',
  'dilate',
  'erode',
  'opaque',
];

export const imageFilterNode = registerNode({
  type: 'image/filter',
  category: 'Image',
  label: 'Filter',
  description: 'Apply an image filter (blur, gray, threshold, posterize, …).',
  defaultParams: { mode: 'gray' as FilterMode, level: 0.5 },
  paramOptions: { mode: MODES as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'level', type: 'number', default: 0.5, param: 'level' },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const src = (inputs[0] ?? [])[0]  as unknown | null;
    const level = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.5;
    const mode = params.param<FilterMode>('mode', 'gray');

    if (!src || !isReadyImageCanvas(src)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }

    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(
      params.nodeId, src.width, src.height,
    );
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.globalCompositeOperation = 'source-over';

    // CSS-filter fast paths.
    let cssFilter: string | null = null;
    switch (mode) {
      case 'gray':   cssFilter = 'grayscale(1)'; break;
      case 'invert': cssFilter = 'invert(1)'; break;
      case 'blur':   cssFilter = `blur(${Math.max(0, level)}px)`; break;
      case 'dilate': cssFilter = `brightness(${1 + 0.5 * level})`; break;
      case 'erode':  cssFilter = `brightness(${Math.max(0, 1 - 0.5 * level)})`; break;
    }
    if (cssFilter) {
      ctx.filter = cssFilter;
      ctx.drawImage(src, 0, 0);
      ctx.filter = 'none';
      ctx.restore();
      return [[canvas]];
    }

    // ImageData paths.
    ctx.drawImage(src, 0, 0);
    const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const d = img.data;
    if (mode === 'threshold') {
      const t = Math.max(0, Math.min(1, level)) * 255;
      for (let i = 0; i < d.length; i += 4) {
        const lum = 0.299 * d[i]! + 0.587 * d[i + 1]! + 0.114 * d[i + 2]!;
        const v = lum < t ? 0 : 255;
        d[i] = v; d[i + 1] = v; d[i + 2] = v;
      }
    } else if (mode === 'posterize') {
      const levels = Math.max(2, Math.min(255, Math.round(level)));
      const step = 255 / (levels - 1);
      for (let i = 0; i < d.length; i += 4) {
        d[i]     = Math.round(d[i]!     / step) * step;
        d[i + 1] = Math.round(d[i + 1]! / step) * step;
        d[i + 2] = Math.round(d[i + 2]! / step) * step;
      }
    } else if (mode === 'opaque') {
      for (let i = 3; i < d.length; i += 4) d[i] = 255;
    }
    ctx.putImageData(img, 0, 0);
    ctx.restore();
    return [[canvas]];
  },
});
