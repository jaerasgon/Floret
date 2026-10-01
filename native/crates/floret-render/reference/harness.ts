// Reference-render harness: the web app's real WebGL2 renderer (the same
// scene→batches walk + VectorRenderer the render worker runs), driven with a
// recorded / synthetic scene instead of a live patch. Bundled and run in
// headless Chromium by generate.mjs.

import { VectorRenderer, FrameBuilder } from '../../../../src/runtime/gl/gl-vector-renderer';
import { emit, ID } from '../../../../src/runtime/gl/scene-to-batches';
import type { SceneNode } from '../../../../src/runtime/scene';

interface RefScene {
  width: number;
  height: number;
  background: [number, number, number];
  layers: SceneNode[];
}

/** Image sources arrive as inline pixels `{w, h, rgba}`; make canvases. */
function withCanvases(n: unknown): unknown {
  if (Array.isArray(n)) return n.map(withCanvases);
  if (!n || typeof n !== 'object') return n;
  const o = n as Record<string, unknown>;
  if (o.kind === 'image') {
    const src = o.source as { w: number; h: number; rgba: number[] };
    const c = document.createElement('canvas');
    c.width = src.w;
    c.height = src.h;
    c.getContext('2d')!.putImageData(new ImageData(new Uint8ClampedArray(src.rgba), src.w, src.h), 0, 0);
    return { ...o, source: c };
  }
  if (o.kind === 'group') return { ...o, children: withCanvases(o.children) };
  return o;
}

function renderScene(s: RefScene): string {
  const canvas = document.createElement('canvas');
  canvas.width = s.width;
  canvas.height = s.height;
  document.body.appendChild(canvas);
  // Same context options as the worker, plus a readable drawing buffer.
  const renderer = new VectorRenderer(canvas, { preserveDrawingBuffer: true });
  const fb = new FrameBuilder();
  fb.reset();
  for (const layer of withCanvases(s.layers) as SceneNode[]) if (layer) emit(fb, renderer, layer, ID);
  renderer.render(fb, s.width, s.height, s.background);
  renderer.gl.finish();
  const url = canvas.toDataURL('image/png');
  canvas.remove();
  return url;
}

(window as unknown as { renderScene: typeof renderScene }).renderScene = renderScene;
