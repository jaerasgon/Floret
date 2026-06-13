import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { Layer } from '../../runtime/types';
import { renderScene } from '../../runtime/scene-renderer';
import type { SceneNode } from '../../runtime/scene';
import type { NodeParamContext, PortValue } from '../types';

/**
 * Cache — rasterise a Layer (or spread of Layers) into a persistent
 * offscreen canvas, then emit an `image` SceneNode that blits the
 * bitmap. Compute runs every tick (we don't try to detect input
 * stability from inside the compute body any more) but produces a
 * scene node referencing a stable offscreen canvas — the canvas only
 * gets re-rendered when this compute runs, which the runtime
 * memoiser already gates on input changes.
 *
 * With cook-on-change at the driver level, static patches don't even
 * tick — Cache is most useful when there's a heavy-but-rare subtree
 * inside an otherwise live patch (e.g. a hand-drawn background under a
 * live particle system).
 *
 * `w` / `h` default to 0 → match the rasterise to a default 720×720
 * surface for now. (Tracking the live canvas size requires a callback
 * that we'd thread through; defer.)
 */

const DEFAULT_CACHE_SIZE = 720;

export const cacheLayerNode = registerNode({
  type: 'transform/cache',
  category: 'Transform',
  label: 'Cache',
  description: 'Rasterise a layer into a cached bitmap and blit it.',
  inputs: [
    { name: 'layer', type: 'layer', kind: 'spread' },
    { name: 'w', type: 'number', default: 0 },
    { name: 'h', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'rasterized', type: 'layer' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const layers = (inputs[0] ?? []) as ReadonlyArray<Layer>;
    const wIn = ((inputs[1] ?? [])[0] as number | undefined) ?? 0;
    const hIn = ((inputs[2] ?? [])[0] as number | undefined) ?? 0;
    const w = Math.max(1, Math.floor(wIn > 0 ? wIn : DEFAULT_CACHE_SIZE));
    const h = Math.max(1, Math.floor(hIn > 0 ? hIn : DEFAULT_CACHE_SIZE));

    const { canvas, ctx } = derivedImageRegistry.ensureGraphics(
      params.nodeId, w, h,
    );

    // Rasterise the scene tree into the offscreen surface NOW (this
    // tick). Centre origin to match the main renderer.
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.translate(w / 2, h / 2);
    for (const layer of layers) {
      if (!layer) continue;
      ctx.save();
      try {
        renderScene(ctx, layer);
      } finally {
        ctx.restore();
      }
    }
    ctx.restore();

    // Emit an `image` SceneNode. The main renderer's translate has
    // already centred its origin on the canvas, so blit at (-w/2, -h/2)
    // for the centred layout.
    const node: SceneNode = {
      kind: 'image',
      source: canvas,
      src: null,
      dst: { x: -w / 2, y: -h / 2, w, h },
      tint: null,
    };
    return [[node]];
  },
});
