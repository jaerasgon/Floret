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

function renderScene(s: RefScene): string {
  const canvas = document.createElement('canvas');
  canvas.width = s.width;
  canvas.height = s.height;
  document.body.appendChild(canvas);
  // Same context options as the worker, plus a readable drawing buffer.
  const renderer = new VectorRenderer(canvas, { preserveDrawingBuffer: true });
  const fb = new FrameBuilder();
  fb.reset();
  for (const layer of s.layers) if (layer) emit(fb, renderer, layer, ID);
  renderer.render(fb, s.width, s.height, s.background);
  renderer.gl.finish();
  const url = canvas.toDataURL('image/png');
  canvas.remove();
  return url;
}

(window as unknown as { renderScene: typeof renderScene }).renderScene = renderScene;
