import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import { isReadyImageCanvas } from '../_shared/coerce';
import type { NodeParamContext, PortValue } from '../types';

/**
 * Composite two images. The output is a new image: image A is drawn
 * straight into the output as the base layer; image B is then drawn
 * into the output's `(dx, dy, dw, dh)` rect using its source `(sx, sy,
 * sw, sh)` region with the chosen blend `mode`.
 *
 * This is the compositing version of `draw/image-region` — same 9-arg
 * source/dest rect contract, plus a base layer and a blend mode.
 *
 * Zero-as-natural mirrors the other image nodes:
 *   - `sw === 0` or `sh === 0` → source rect runs to B's edge.
 *   - `dw === 0` or `dh === 0` → dest matches the source rect size.
 *
 * Output dimensions track image A. If A is loading the output is a
 * 1×1 placeholder; if only B is loading, A passes through unmodified.
 *
 * Blend modes map straight onto canvas `globalCompositeOperation`
 * values, so they behave identically to p5's `blendMode()`.
 */

type BlendMode =
  | 'normal'
  | 'add'
  | 'darken'
  | 'lighten'
  | 'difference'
  | 'exclusion'
  | 'multiply'
  | 'screen'
  | 'overlay'
  | 'hard-light'
  | 'soft-light'
  | 'dodge'
  | 'burn';

const MODES: ReadonlyArray<BlendMode> = [
  'normal',
  'add',
  'darken',
  'lighten',
  'difference',
  'exclusion',
  'multiply',
  'screen',
  'overlay',
  'hard-light',
  'soft-light',
  'dodge',
  'burn',
];

const MODE_TO_GCO: Record<BlendMode, GlobalCompositeOperation> = {
  normal: 'source-over',
  add: 'lighter',
  darken: 'darken',
  lighten: 'lighten',
  difference: 'difference',
  exclusion: 'exclusion',
  multiply: 'multiply',
  screen: 'screen',
  overlay: 'overlay',
  'hard-light': 'hard-light',
  'soft-light': 'soft-light',
  dodge: 'color-dodge',
  burn: 'color-burn',
};

export const imageBlendNode = registerNode({
  type: 'image/blend',
  category: 'Image',
  label: 'Blend',
  description: 'Composite image B onto image A using the chosen blend mode.',
  defaultParams: { mode: 'normal' as BlendMode },
  paramOptions: { mode: MODES as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'a', type: 'image' },
    { name: 'b', type: 'image' },
    { name: 'sx', type: 'number', default: 0 },
    { name: 'sy', type: 'number', default: 0 },
    { name: 'sw', type: 'number', default: 0 },
    { name: 'sh', type: 'number', default: 0 },
    { name: 'dx', type: 'number', default: 0 },
    { name: 'dy', type: 'number', default: 0 },
    { name: 'dw', type: 'number', default: 0 },
    { name: 'dh', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const aRaw = (inputs[0] ?? [])[0];
    const bRaw = (inputs[1] ?? [])[0];
    const sx = ((inputs[2] ?? [])[0] as number | undefined) ?? 0;
    const sy = ((inputs[3] ?? [])[0] as number | undefined) ?? 0;
    let sw = ((inputs[4] ?? [])[0] as number | undefined) ?? 0;
    let sh = ((inputs[5] ?? [])[0] as number | undefined) ?? 0;
    const dx = ((inputs[6] ?? [])[0] as number | undefined) ?? 0;
    const dy = ((inputs[7] ?? [])[0] as number | undefined) ?? 0;
    let dw = ((inputs[8] ?? [])[0] as number | undefined) ?? 0;
    let dh = ((inputs[9] ?? [])[0] as number | undefined) ?? 0;
    const mode = params.param<BlendMode>('mode', 'normal');

    if (!isReadyImageCanvas(aRaw)) {
      return [[derivedImageRegistry.ensure(params.nodeId)]];
    }
    const a = aRaw;
    const w = a.width;
    const h = a.height;
    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    ctx.globalCompositeOperation = 'source-over';
    ctx.clearRect(0, 0, w, h);
    ctx.drawImage(a, 0, 0);
    // Skip the overlay pass entirely if B is unloaded or sizes collapse.
    if (isReadyImageCanvas(bRaw)) {
      const b = bRaw;
      if (sw === 0) sw = b.width - sx;
      if (sh === 0) sh = b.height - sy;
      if (dw === 0) dw = sw;
      if (dh === 0) dh = sh;
      if (sw > 0 && sh > 0 && dw > 0 && dh > 0) {
        ctx.globalCompositeOperation = MODE_TO_GCO[mode];
        ctx.drawImage(b, sx, sy, sw, sh, dx, dy, dw, dh);
        ctx.globalCompositeOperation = 'source-over';
      }
    }
    return [[canvas]];
  },
});
