import type { FrameCtx } from './types';
import type { Color } from './types';
import type { SceneNode } from './scene';
import { colorToCss } from './scene';
import { tick } from './tick';
import { publishTickOutputs, publishRegionStats } from './inspector';
import type { RuntimeGraph } from './graph';
import { isDirty, clearDirty } from './dirty';
import { paramStore } from './param-store';
import * as renderBridge from './render-bridge';

/**
 * Canvas driver — owns the canvas DOM element, the RAF loop, and the
 * input event listeners. Mounted by `preview/Preview.tsx`.
 *
 * Owns three things only:
 *   1. The canvas DOM element + its 2D context.
 *   2. The RAF loop that ticks the runtime + renders the scene tree.
 *   3. The input event listeners (mouse/key/wheel) that feed FrameCtx.
 *
 * Drawing is delegated to `renderScene(ctx, node)` from
 * `scene-renderer.ts`. The runtime is unchanged — it still emits
 * a spread of "layers" on the renderer's `layers` port, but those
 * layers are now `SceneNode`s (data), not `(p: p5) => void` thunks.
 *
 * Lifecycle (matches the old `mountP5` API so callers can swap):
 *   - `mountCanvas(container, opts)` — create canvas, attach RAF, return handle.
 *   - `unmountCanvas()` — stop loop, detach listeners, remove DOM node.
 *   - `getActiveInstance()` — read-only accessor for the active driver
 *     (used by nodes that allocate offscreen surfaces).
 *
 * No `window`/`document` access at module top-level — module is
 * import-safe in workers and in Node test environments.
 */

const DEFAULT_W = 500;
const DEFAULT_H = 500;
const MIN_CANVAS = 32;
const MAX_CANVAS = 4096;

export interface DriverOptions {
  graphRef: { current: RuntimeGraph | null };
  onTick?: (fps: number, frameCount: number) => void;
  isRunning: () => boolean;
}

export interface ActiveDriver {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  container: HTMLElement;
  /** Stop the RAF loop and remove listeners. Idempotent. */
  stop: () => void;
}

let active: ActiveDriver | null = null;

/**
 * Construct an offscreen canvas + 2D context — used by nodes that need
 * to rasterise something into a bitmap (image/from-layer, future cache
 * primitives). Returns a fresh canvas each call; the caller is
 * responsible for stashing it in a per-node registry if they want it
 * to survive across frames.
 */
export function createOffscreen(w: number, h: number): {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
} {
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.floor(w));
  canvas.height = Math.max(1, Math.floor(h));
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('createOffscreen: 2D context unavailable');
  return { canvas, ctx };
}

/** Read-only accessor for the currently mounted driver. */
export function getActiveDriver(): ActiveDriver | null {
  return active;
}

export function mountCanvas(container: HTMLElement, opts: DriverOptions): ActiveDriver {
  if (active) {
    active.stop();
    active = null;
  }

  const canvas = document.createElement('canvas');
  canvas.width = DEFAULT_W;
  canvas.height = DEFAULT_H;
  canvas.style.display = 'block';
  canvas.style.width = '100%';
  canvas.style.height = '100%';
  canvas.style.maxWidth = `${DEFAULT_W}px`;
  canvas.style.maxHeight = `${DEFAULT_H}px`;
  container.appendChild(canvas);

  // Render pipeline lives in a Worker via OffscreenCanvas — main thread
  // never touches the canvas's 2D context after this point. Local
  // `canvasW`/`canvasH` mirror the size we last asked the worker for,
  // since `canvas.width` is the OffscreenCanvas's now, not ours.
  let canvasW = DEFAULT_W;
  let canvasH = DEFAULT_H;
  renderBridge.mount(canvas);
  // For backward compat with `ActiveDriver.ctx` — we no longer expose
  // a usable 2D context. Anything that wants ctx access has to talk
  // to the worker via render-bridge.
  const ctx = null as unknown as CanvasRenderingContext2D;

  // ── Input state ─────────────────────────────────────────────────────
  // Mouse coords are reported relative to the canvas centre (matching
  // the runtime's centred-origin convention). When the cursor leaves
  // the canvas we freeze the last in-bounds value so patches don't see
  // jumps to (-window/2, -window/2).
  let mouseX = 0, mouseY = 0;
  let mouseInBounds = false;
  let mouseIsPressed = false;
  let key = '';
  let keyIsPressed = false;

  const onMouseMove = (e: MouseEvent) => {
    const r = canvas.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * canvas.width;
    const py = ((e.clientY - r.top) / r.height) * canvas.height;
    const inB = px >= 0 && px <= canvas.width && py >= 0 && py <= canvas.height;
    mouseInBounds = inB;
    if (inB) {
      mouseX = px - canvas.width / 2;
      mouseY = py - canvas.height / 2;
    }
  };
  const onMouseDown = () => { if (mouseInBounds) mouseIsPressed = true; };
  const onMouseUp = () => { mouseIsPressed = false; };
  const onKeyDown = (e: KeyboardEvent) => {
    key = e.key;
    keyIsPressed = true;
  };
  const onKeyUp = () => { keyIsPressed = false; };
  const onMouseLeave = () => { mouseInBounds = false; };

  canvas.addEventListener('mousemove', onMouseMove);
  canvas.addEventListener('mouseleave', onMouseLeave);
  window.addEventListener('mousedown', onMouseDown);
  window.addEventListener('mouseup', onMouseUp);
  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);

  // ── Frame loop ──────────────────────────────────────────────────────
  let frameCount = 0;
  let raf = 0;
  let stopped = false;
  const startTime = performance.now() / 1000;
  let lastTime = startTime;
  const fpsWindow: number[] = [];

  const loop = () => {
    if (stopped) return;
    raf = requestAnimationFrame(loop);

    if (!opts.isRunning()) return;

    const g = opts.graphRef.current;

    // Resize → message the worker. Style sizing is ours to set so the
    // visible HTMLCanvasElement reflects the new dims.
    if (g?.rendererId) {
      const w = paramStore.getKey<number>(g.rendererId, 'width');
      const h = paramStore.getKey<number>(g.rendererId, 'height');
      const cw = clamp(typeof w === 'number' && isFinite(w) ? w : DEFAULT_W);
      const ch = clamp(typeof h === 'number' && isFinite(h) ? h : DEFAULT_H);
      if (cw !== canvasW || ch !== canvasH) {
        canvasW = cw; canvasH = ch;
        renderBridge.resize(cw, ch);
        canvas.style.maxWidth = `${cw}px`;
        canvas.style.maxHeight = `${ch}px`;
      }
    }

    const nowSec = performance.now() / 1000;
    const dt = nowSec - lastTime;
    lastTime = nowSec;
    fpsWindow.push(nowSec);
    while (fpsWindow.length > 30) fpsWindow.shift();
    const fps = fpsWindow.length > 1
      ? (fpsWindow.length - 1) / (fpsWindow[fpsWindow.length - 1]! - fpsWindow[0]!)
      : 0;

    if (!g) {
      // No graph compiled — paint black through the worker.
      void renderBridge.frame([], '#000000', false);
      frameCount++;
      if (opts.onTick) opts.onTick(fps, frameCount);
      return;
    }

    // Cook-on-change short-circuit. Static patches idle at zero CPU
    // until something marks dirty.
    if (!g.alwaysAnimate && !isDirty()) {
      frameCount++;
      if (opts.onTick) opts.onTick(fps, frameCount);
      return;
    }

    const frame: FrameCtx = {
      t: nowSec - startTime,
      dt,
      frameCount,
      width: canvasW,
      height: canvasH,
      mouseX,
      mouseY,
      mouseIsPressed: mouseInBounds && mouseIsPressed,
      key,
      keyIsPressed,
    };

    const result = tick(g, frame);
    publishTickOutputs(result.outputs);
    publishRegionStats(result.regionStats);

    // Background colour from the renderer's second output port.
    const bgRaw = g.rendererId
      ? (result.outputs.get(g.rendererId)?.[1] as ReadonlyArray<unknown> | undefined)?.[0]
      : null;
    const bgCss = (bgRaw && typeof bgRaw === 'object' && 'r' in bgRaw)
      ? colorToCss(bgRaw as Color)
      : '#000000';

    // Flatten the runtime's `layers` spread. ForEach output may include
    // nested arrays; the worker bridge expects a flat list.
    const scene = result.layers as unknown as ReadonlyArray<SceneNode | SceneNode[]>;
    const flat: SceneNode[] = [];
    for (const layer of scene) {
      if (Array.isArray(layer)) {
        for (const n of layer) if (n) flat.push(n);
      } else if (layer) {
        flat.push(layer);
      }
    }
    void renderBridge.frame(flat, bgCss, true);

    clearDirty();
    frameCount++;
    if (opts.onTick) opts.onTick(fps, frameCount);
  };

  raf = requestAnimationFrame(loop);

  const stop = () => {
    if (stopped) return;
    stopped = true;
    cancelAnimationFrame(raf);
    canvas.removeEventListener('mousemove', onMouseMove);
    canvas.removeEventListener('mouseleave', onMouseLeave);
    window.removeEventListener('mousedown', onMouseDown);
    window.removeEventListener('mouseup', onMouseUp);
    window.removeEventListener('keydown', onKeyDown);
    window.removeEventListener('keyup', onKeyUp);
    renderBridge.dispose();
    if (canvas.parentNode === container) container.removeChild(canvas);
  };

  active = { canvas, ctx, container, stop };
  return active;
}

export function unmountCanvas(): void {
  if (active) {
    active.stop();
    active = null;
  }
}

function clamp(v: number): number {
  return Math.max(MIN_CANVAS, Math.min(MAX_CANVAS, Math.floor(v)));
}

if (typeof import.meta.hot !== 'undefined' && import.meta.hot) {
  import.meta.hot.dispose(() => {
    unmountCanvas();
  });
}
