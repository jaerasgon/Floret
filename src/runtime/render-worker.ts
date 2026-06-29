/// <reference lib="webworker" />
/**
 * Render worker — owns the OffscreenCanvas, runs the WebGL2 GPU renderer.
 *
 * The main thread does everything else (React UI, input event capture,
 * runtime tick, audio). Each frame it posts the scene tree + background
 * + canvas dims; this worker walks the SceneNode tree into GPU batches and
 * rasterises them with the GL vector renderer. Main stays responsive even
 * when a dense scene takes 100ms+ to render.
 *
 * Messages received:
 *
 *   { type: 'init', canvas: OffscreenCanvas } — once, at mount
 *   { type: 'resize', w, h }                  — when renderer params change
 *   { type: 'frame', layers, bg, centreOrigin }
 *                                              — per visible frame
 *   { type: 'dispose' }                       — on unmount / HMR
 */

import { VectorRenderer, FrameBuilder } from './gl/gl-vector-renderer';
import type { RenderClock } from './gl/gl-vector-renderer';
import { emit, parseColor, ID } from './gl/scene-to-batches';
import type { SceneNode, Affine } from './scene';

let canvas: OffscreenCanvas | null = null;
let renderer: VectorRenderer | null = null;
const fb = new FrameBuilder();

interface InitMsg { type: 'init'; canvas: OffscreenCanvas }
interface ResizeMsg { type: 'resize'; w: number; h: number }
interface FrameMsg {
  type: 'frame';
  layers: ReadonlyArray<SceneNode>;
  bg: string;
  centreOrigin: boolean;
  clock?: RenderClock;
}
interface DisposeMsg { type: 'dispose' }
type Msg = InitMsg | ResizeMsg | FrameMsg | DisposeMsg;

self.onmessage = (e: MessageEvent<Msg>) => {
  const m = e.data;
  switch (m.type) {
    case 'init': {
      canvas = m.canvas;
      try { renderer = new VectorRenderer(canvas); } catch (err) { renderer = null; console.error('[render-worker] GL init failed', err); }
      break;
    }
    case 'resize': {
      if (canvas && (canvas.width !== m.w || canvas.height !== m.h)) {
        canvas.width = m.w;
        canvas.height = m.h;
      }
      break;
    }
    case 'frame': {
      if (!renderer || !canvas) return;
      const w = canvas.width, h = canvas.height;
      // The GL renderer is centre-origin; for top-left content, pre-translate.
      const m0: Affine = m.centreOrigin ? ID : [1, 0, 0, 1, -w / 2, -h / 2];
      fb.reset();
      for (const layer of m.layers) { if (layer) emit(fb, renderer, layer, m0); }
      const c = parseColor(m.bg);
      renderer.render(fb, w, h, [c[0], c[1], c[2]], m.clock);
      break;
    }
    case 'dispose': {
      canvas = null;
      renderer = null;
      break;
    }
  }
};
