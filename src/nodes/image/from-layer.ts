import { registerNode } from '../registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import type { Layer } from '../../runtime/types';
import type { NodeParamContext, PortValue } from '../types';
import { renderLayersToCanvas } from '../../runtime/gl-shared-context';

/**
 * Rasterise a Layer (or spread of Layers) into an offscreen canvas
 * and hand back the canvas as an `image`. Bridges the retained-mode
 * scene tree back to raster data so downstream image nodes (filter,
 * blend, sample, pixelate, tile) can consume it.
 *
 * The offscreen surface is owned by this node via
 * `derivedImageRegistry.ensureGraphics()` — sized to `(w, h)`, reused
 * across frames, resized only when the dims change.
 *
 * Centre origin: matches the main renderer's convention so a layer
 * wired here draws into the same coordinate frame it would on the
 * main canvas.
 */
export const imageFromLayerNode = registerNode({
  type: 'image/from-layer',
  category: 'Image',
  label: 'Rasterize',
  description: 'Rasterize a layer into an image at (w, h).',
  inputs: [
    { name: 'layer', type: 'layer', kind: 'spread' },
    { name: 'w', type: 'number', default: 256 },
    { name: 'h', type: 'number', default: 256 },
  ],
  outputs: [{ name: 'image', type: 'image' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const layers = (inputs[0] ?? []) as ReadonlyArray<Layer>;
    const wIn = ((inputs[1] ?? [])[0] as number | undefined) ?? 256;
    const hIn = ((inputs[2] ?? [])[0] as number | undefined) ?? 256;
    const w = Math.max(1, Math.floor(wIn));
    const h = Math.max(1, Math.floor(hIn));

    const { canvas } = derivedImageRegistry.ensureGraphics(params.nodeId, w, h);
    // Transparent background (null) — the rasterised layer composites downstream.
    renderLayersToCanvas(layers, w, h, null, true, canvas);
    return [[canvas]];
  },
});
