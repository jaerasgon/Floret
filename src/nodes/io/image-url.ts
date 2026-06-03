import { registerNode } from '../registry';
import { imageRegistry } from '../../runtime/image-registry';
import type { NodeParamContext } from '../types';

/**
 * Load an image from a URL.
 *
 * `url` is a node param (not an input wire) — loading is async and
 * we don't want a slider to retrigger it. The URL is fetched on first
 * tick and cached on the image registry; URL-param changes dispose
 * the old element and reload.
 *
 * Until the bytes arrive, `image` is null and `loaded` is false —
 * draw/image safely no-ops in that window so the canvas stays clean
 * during the load.
 *
 * CORS: the registry sets `crossOrigin='anonymous'` so cooperating
 * servers return non-tainted bytes (needed if a future filter node
 * wants `getImageData`). Servers without CORS still load and draw —
 * just opaque to pixel reads.
 */
export const imageUrlNode = registerNode({
  type: 'io/image-url',
  category: 'IO',
  label: 'ImageURL',
  description: 'Load an image from a URL.',
  defaultParams: {
    // Picsum returns deterministic JPEGs per seed and serves CORS headers.
    url: 'https://picsum.photos/seed/nodule/500/500',
  },
  inputs: [],
  outputs: [
    { name: 'image', type: 'image' },
    { name: 'loaded', type: 'boolean' },
    { name: 'width', type: 'number' },
    { name: 'height', type: 'number' },
  ],
  readFromContext: (_frame, params: NodeParamContext) => {
    const url = params.param<string>('url', '').trim();
    if (!url) return [[null], [false], [0], [0]];
    const canvas = imageRegistry.ensure(params.nodeId, url);
    const loaded = imageRegistry.isLoaded(params.nodeId);
    return [
      [canvas],
      [loaded],
      [loaded ? canvas.width : 0],
      [loaded ? canvas.height : 0],
    ];
  },
});
