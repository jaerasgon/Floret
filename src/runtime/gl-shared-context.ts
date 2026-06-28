// ─── Shared main-thread GL context ──────────────────────────────────────────
// One WebGL2 context serving every non-preview render surface — thumbnails,
// probe/slider previews, and the mid-graph rasterisers (image/from-layer,
// transform/cache) + io/send. Each Preview owns its own worker-side context;
// this singleton covers everything on the main thread so we never approach the
// browser's per-page WebGL context limit with one-context-per-thumbnail.
//
// The GL output is copied into the caller's 2D canvas (a dumb blit target —
// it never rasterises scene content), so callers keep their existing
// canvas-ownership (derived-image registry, thumbnail surfaces, …).

import { VectorRenderer, FrameBuilder } from './gl/gl-vector-renderer';
import { emit, parseColor } from './gl/scene-to-batches';
import type { SceneNode, Affine } from './scene';

type AnyCanvas = HTMLCanvasElement | OffscreenCanvas;
type Any2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

let renderer: VectorRenderer | null = null;
let glCanvas: AnyCanvas | null = null;
let unavailable = false;
const fb = new FrameBuilder();

// Returns the shared renderer, or null when WebGL2 isn't available (headless
// tests, blocked contexts) — callers then degrade to a cleared canvas.
function ensure(w: number, h: number): VectorRenderer | null {
  if (unavailable) return null;
  if (!renderer || !glCanvas) {
    try {
      glCanvas = typeof document !== 'undefined' ? document.createElement('canvas') : new OffscreenCanvas(w, h);
      renderer = new VectorRenderer(glCanvas, { alpha: true, preserveDrawingBuffer: true });
    } catch { unavailable = true; return null; }
  }
  if (glCanvas.width !== w) glCanvas.width = w;
  if (glCanvas.height !== h) glCanvas.height = h;
  return renderer;
}

/**
 * Render `layers` to `target` via the shared GL context. `bg` is a CSS colour
 * string, or null for a transparent background (rasterising a layer for
 * downstream compositing). `centreOrigin` matches the main renderer's frame.
 */
export function renderLayersToCanvas(
  layers: ReadonlyArray<SceneNode>,
  w: number, h: number,
  bg: string | null,
  centreOrigin: boolean,
  target: AnyCanvas,
  scale = 1,
): AnyCanvas {
  const r = ensure(w, h);
  if (target.width !== w) target.width = w;
  if (target.height !== h) target.height = h;
  if (!r) {
    // No GL (headless/test): hand back a cleared (optionally bg-filled) canvas
    // so callers get a valid surface instead of throwing.
    const ctx2 = target.getContext('2d') as Any2D | null;
    if (ctx2) { ctx2.clearRect(0, 0, w, h); if (bg) { ctx2.fillStyle = bg; ctx2.fillRect(0, 0, w, h); } }
    return target;
  }
  const m0: Affine = centreOrigin
    ? [scale, 0, 0, scale, 0, 0]
    : [scale, 0, 0, scale, -w / 2, -h / 2];
  fb.reset();
  for (const layer of layers) if (layer) emit(fb, r, layer, m0);
  let rgb: [number, number, number] | null = null;
  if (bg) { const c = parseColor(bg); rgb = [c[0], c[1], c[2]]; }
  r.render(fb, w, h, rgb);

  if (target.width !== w) target.width = w;
  if (target.height !== h) target.height = h;
  const ctx = target.getContext('2d') as Any2D;
  ctx.clearRect(0, 0, w, h);
  ctx.drawImage(glCanvas!, 0, 0);
  return target;
}
