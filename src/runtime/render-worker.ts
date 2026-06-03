/// <reference lib="webworker" />
/**
 * Render worker — owns the OffscreenCanvas, runs the scene renderer.
 *
 * The main thread does everything else (React UI, input event capture,
 * runtime tick, audio). Each frame it posts the scene tree + background
 * + canvas dims; this worker rasterises. Main stays responsive even
 * when a dense scene takes 100ms+ to render.
 *
 * Messages received:
 *
 *   { type: 'init', canvas: OffscreenCanvas } — once, at mount
 *   { type: 'resize', w, h }                  — when renderer params change
 *   { type: 'frame', layers, bg, centreOrigin }
 *                                              — per visible frame
 *   { type: 'dispose' }                       — on unmount / HMR
 *
 * Messages sent: none yet. Future: render-error reports, frame-time
 * telemetry for the FPS overlay.
 */

import { renderScene } from './scene-renderer';
import type { SceneNode } from './scene';

let canvas: OffscreenCanvas | null = null;
let ctx: OffscreenCanvasRenderingContext2D | null = null;

interface InitMsg { type: 'init'; canvas: OffscreenCanvas }
interface ResizeMsg { type: 'resize'; w: number; h: number }
interface FrameMsg {
  type: 'frame';
  layers: ReadonlyArray<SceneNode>;
  bg: string;
  centreOrigin: boolean;
}
interface DisposeMsg { type: 'dispose' }
type Msg = InitMsg | ResizeMsg | FrameMsg | DisposeMsg;

self.onmessage = (e: MessageEvent<Msg>) => {
  const m = e.data;
  switch (m.type) {
    case 'init': {
      canvas = m.canvas;
      // 2D context is safe for OffscreenCanvas everywhere we care about
      // (Chrome/Edge/Safari/Firefox modern).
      ctx = canvas.getContext('2d');
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
      if (!ctx || !canvas) return;
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = m.bg;
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      if (m.centreOrigin) ctx.translate(canvas.width / 2, canvas.height / 2);
      for (const layer of m.layers) {
        if (!layer) continue;
        ctx.save();
        try {
          renderScene(ctx as unknown as CanvasRenderingContext2D, layer);
        } finally {
          ctx.restore();
        }
      }
      ctx.restore();
      break;
    }
    case 'dispose': {
      canvas = null;
      ctx = null;
      break;
    }
  }
};
