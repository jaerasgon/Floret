import type { FrameCtx } from './types';
import type { Color } from './types';
import type { SceneNode } from './scene';
import { colorToCss } from './scene';
import { tick } from './tick';
import { publishTickOutputs, publishRegionStats } from './inspector';
import type { RuntimeGraph, NodeId } from './graph';
import { isDirty, clearDirty } from './dirty';
import { paramStore } from './param-store';
import { RenderBridge } from './render-bridge';

/**
 * Canvas driver — owns ONE canvas + RAF loop + input listeners for ONE
 * Renderer node in the graph. The editor instantiates one driver per
 * mounted Preview panel; multiple Previews each get their own driver
 * (and their own worker).
 *
 * Lifecycle:
 *   - `createCanvasDriver(container, opts)` — create canvas, attach
 *     RAF, return handle.
 *   - `driver.stop()` — stop loop, detach listeners, remove DOM node.
 */

const DEFAULT_W = 500;
const DEFAULT_H = 500;
const MIN_CANVAS = 32;
const MAX_CANVAS = 4096;

export interface DriverOptions {
  graphRef: { current: RuntimeGraph | null };
  /** Which Renderer's layer spread this driver should display.
   *  When `null`, the driver auto-binds to whichever renderer is at
   *  index 0 of `graph.rendererIds` each tick — matching legacy
   *  single-renderer behaviour. */
  rendererId?: NodeId | null;
  onTick?: (fps: number, frameCount: number) => void;
  isRunning: () => boolean;
}

export interface ActiveDriver {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  container: HTMLElement;
  /** Stop the RAF loop and remove listeners. Idempotent. */
  stop: () => void;
  /** Snapshot the most recent frame's flattened scene — used by SVG
   *  export, which needs the retained SceneNodes (not pixels). */
  captureScene: () => { layers: SceneNode[]; background: string; width: number; height: number };
}

/** Currently-mounted drivers, keyed by their container element. Used by
 *  the few callers that grab an arbitrary "active" driver (legacy from
 *  the singleton era). New code should hold its driver reference. */
const drivers = new Set<ActiveDriver>();

/** Construct an offscreen canvas + 2D context — used by nodes that
 *  need to rasterise something into a bitmap (image/from-layer). */
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

/** Read-only accessor for any active driver. When multiple Previews
 *  are mounted this returns the first registered one — callers should
 *  retain their own reference if they need a specific driver. */
export function getActiveDriver(): ActiveDriver | null {
  for (const d of drivers) return d;
  return null;
}

export function createCanvasDriver(container: HTMLElement, opts: DriverOptions): ActiveDriver {
  const canvas = document.createElement('canvas');
  canvas.width = DEFAULT_W;
  canvas.height = DEFAULT_H;
  canvas.style.display = 'block';
  canvas.style.width = '100%';
  canvas.style.height = '100%';
  canvas.style.maxWidth = `${DEFAULT_W}px`;
  canvas.style.maxHeight = `${DEFAULT_H}px`;
  container.appendChild(canvas);

  let canvasW = DEFAULT_W;
  let canvasH = DEFAULT_H;
  const bridge = new RenderBridge(canvas);
  const ctx = null as unknown as CanvasRenderingContext2D;

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

  let frameCount = 0;
  let raf = 0;
  let stopped = false;
  // Latest frame's scene, retained for SVG export (which needs SceneNodes,
  // not pixels).
  let lastFlat: SceneNode[] = [];
  let lastBg = '#000000';
  const startTime = performance.now() / 1000;
  let lastTime = startTime;
  const fpsWindow: number[] = [];

  /** Resolve which Renderer this driver is showing this tick. Honours
   *  the driver's `rendererId` option when set; otherwise falls back to
   *  `graph.rendererIds[0]`. Returns null if the patch has no renderers. */
  const resolveRendererId = (g: RuntimeGraph | null): NodeId | null => {
    if (!g) return null;
    if (opts.rendererId) {
      // Verify the explicit choice still exists; if the user removed
      // that Renderer the driver gracefully falls back to nothing.
      return g.rendererIds.includes(opts.rendererId) ? opts.rendererId : null;
    }
    return g.rendererIds[0] ?? null;
  };

  const loop = () => {
    if (stopped) return;
    raf = requestAnimationFrame(loop);

    if (!opts.isRunning()) return;

    const g = opts.graphRef.current;
    const rendererId = resolveRendererId(g);

    // Resize → message the worker. Style sizing is ours to set so the
    // visible HTMLCanvasElement reflects the new dims.
    if (rendererId) {
      const w = paramStore.getKey<number>(rendererId, 'width');
      const h = paramStore.getKey<number>(rendererId, 'height');
      const cw = clamp(typeof w === 'number' && isFinite(w) ? w : DEFAULT_W);
      const ch = clamp(typeof h === 'number' && isFinite(h) ? h : DEFAULT_H);
      if (cw !== canvasW || ch !== canvasH) {
        canvasW = cw; canvasH = ch;
        bridge.resize(cw, ch);
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

    if (!g || !rendererId) {
      void bridge.frame([], '#000000', false);
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

    const bgRaw = (result.outputs.get(rendererId)?.[1] as ReadonlyArray<unknown> | undefined)?.[0];
    const bgCss = (bgRaw && typeof bgRaw === 'object' && 'r' in bgRaw)
      ? colorToCss(bgRaw as Color)
      : '#000000';

    const myLayers = result.layersByRenderer.get(rendererId) ?? [];
    const scene = myLayers as unknown as ReadonlyArray<SceneNode | SceneNode[]>;
    const flat: SceneNode[] = [];
    for (const layer of scene) {
      if (Array.isArray(layer)) {
        for (const n of layer) if (n) flat.push(n);
      } else if (layer) {
        flat.push(layer);
      }
    }
    void bridge.frame(flat, bgCss, true);
    lastFlat = flat;
    lastBg = bgCss;

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
    bridge.dispose();
    if (canvas.parentNode === container) container.removeChild(canvas);
    drivers.delete(driver);
  };

  const captureScene = () => ({
    layers: lastFlat,
    background: lastBg,
    width: canvas.width,
    height: canvas.height,
  });

  const driver: ActiveDriver = { canvas, ctx, container, stop, captureScene };
  drivers.add(driver);
  return driver;
}

/** Legacy single-driver API for callers that haven't migrated yet.
 *  Wraps `createCanvasDriver`; tearing down via `unmountCanvas` only
 *  affects the most-recently mounted driver. */
let legacy: ActiveDriver | null = null;
export function mountCanvas(container: HTMLElement, opts: DriverOptions): ActiveDriver {
  if (legacy) {
    legacy.stop();
    legacy = null;
  }
  legacy = createCanvasDriver(container, opts);
  return legacy;
}
export function unmountCanvas(): void {
  if (legacy) {
    legacy.stop();
    legacy = null;
  }
}

function clamp(v: number): number {
  return Math.max(MIN_CANVAS, Math.min(MAX_CANVAS, Math.floor(v)));
}

if (typeof import.meta.hot !== 'undefined' && import.meta.hot) {
  import.meta.hot.dispose(() => {
    for (const d of [...drivers]) d.stop();
    legacy = null;
  });
}
