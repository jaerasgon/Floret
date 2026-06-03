import type { SceneNode, ImageShape } from './scene';

/**
 * Render bridge — main-thread side of the worker offload.
 *
 * Owns the worker lifecycle and the per-frame post protocol:
 *
 *   1. `mount(canvas)` — transfers the canvas's drawing surface to a
 *      fresh worker via `transferControlToOffscreen()`. After this the
 *      main thread cannot draw on the canvas; only the worker can.
 *   2. `resize(w, h)` — relays canvas-size changes.
 *   3. `frame(layers, bgCss, centreOrigin)` — posts the scene tree
 *      for this frame. Walks the tree to convert any `HTMLCanvasElement`
 *      sources on image SceneNodes into transferable `ImageBitmap`s
 *      (structured-clone-safe + zero-copy when transferred).
 *   4. `dispose()` — tears the worker down on HMR / unmount.
 *
 * The worker is constructed via Vite's ESM-worker syntax. The build
 * step bundles the worker as a separate chunk; dev server serves it
 * directly with module resolution.
 */

let worker: Worker | null = null;
let mounted = false;

export function mount(canvas: HTMLCanvasElement): void {
  if (mounted) dispose();
  worker = new Worker(new URL('./render-worker.ts', import.meta.url), {
    type: 'module',
  });
  const offscreen = canvas.transferControlToOffscreen();
  worker.postMessage({ type: 'init', canvas: offscreen }, [offscreen]);
  mounted = true;
}

export function resize(w: number, h: number): void {
  worker?.postMessage({ type: 'resize', w, h });
}

/**
 * Post a frame for rendering. `layers` is the scene tree from this
 * tick; `bg` is the CSS background colour string; `centreOrigin` asks
 * the worker to apply `translate(width/2, height/2)` before drawing
 * (matches the runtime's centred-origin convention).
 *
 * Image SceneNodes are pre-processed: each unique HTMLCanvasElement
 * source is converted to an ImageBitmap and transferred in. After this
 * call, those bitmaps are owned by the worker.
 */
export async function frame(
  layers: ReadonlyArray<SceneNode>,
  bg: string,
  centreOrigin: boolean,
): Promise<void> {
  if (!worker || !mounted) return;
  // Collect every convertible canvas we need to ship over. De-dup by
  // identity so a tiling node with many image refs only converts once.
  const sources = new Map<ConvertibleSource, Promise<ImageBitmap>>();
  collectImageSources(layers, sources);
  if (sources.size === 0) {
    worker.postMessage({ type: 'frame', layers, bg, centreOrigin });
    return;
  }
  // Convert all canvases to bitmaps in parallel, then walk the scene
  // again to swap source references. This is allocation-heavy on
  // animated image scenes — future optimisation: per-canvas cached
  // bitmap that only re-builds when the canvas mutates.
  const entries = await Promise.all(
    Array.from(sources, async ([canvas, _]) => [canvas, await createImageBitmap(canvas)] as const),
  );
  const bitmapMap = new Map<ConvertibleSource, ImageBitmap>(entries);
  // The transfer list MUST contain each bitmap exactly once — passing the
  // same Transferable twice is a DataCloneError. The scene tree itself can
  // reference one bitmap many times (e.g. a Datamosh-style fan-out of one
  // image into 4 draw/image nodes); the worker reads the shared reference
  // from the post-transfer tree and draws from a single decoded bitmap.
  const transferList: Transferable[] = [];
  const seenForTransfer = new Set<ImageBitmap>();
  const swapped = swapImageSources(layers, bitmapMap, transferList, seenForTransfer);
  worker.postMessage({ type: 'frame', layers: swapped, bg, centreOrigin }, transferList);
}

export function dispose(): void {
  if (worker) {
    try { worker.postMessage({ type: 'dispose' }); } catch { /* ignore */ }
    worker.terminate();
    worker = null;
  }
  mounted = false;
}

/** Image sources we can't postMessage as-is. Both convert to ImageBitmap. */
type ConvertibleSource = HTMLCanvasElement | OffscreenCanvas;
function isConvertibleSource(v: CanvasImageSource): v is ConvertibleSource {
  return (typeof HTMLCanvasElement !== 'undefined' && v instanceof HTMLCanvasElement)
      || (typeof OffscreenCanvas !== 'undefined' && v instanceof OffscreenCanvas);
}

/** Walk the tree, register every convertible image source. */
function collectImageSources(
  nodes: ReadonlyArray<SceneNode>,
  out: Map<ConvertibleSource, Promise<ImageBitmap>>,
): void {
  for (const n of nodes) {
    if (!n) continue;
    if (n.kind === 'group') {
      collectImageSources(n.children, out);
    } else if (n.kind === 'image') {
      const src = n.source;
      if (isConvertibleSource(src) && !out.has(src)) {
        out.set(src, createImageBitmap(src));
      }
    }
  }
}

/** Return a structurally-copied tree where image SceneNode sources
 *  are replaced with their ImageBitmap counterparts, and populate
 *  `transferList` with the bitmaps so they ship zero-copy. */
function swapImageSources(
  nodes: ReadonlyArray<SceneNode>,
  bitmaps: Map<ConvertibleSource, ImageBitmap>,
  transferList: Transferable[],
  seenForTransfer: Set<ImageBitmap>,
): SceneNode[] {
  return nodes.map((n): SceneNode => {
    if (!n) return n;
    if (n.kind === 'group') {
      return {
        ...n,
        children: swapImageSources(n.children, bitmaps, transferList, seenForTransfer),
      };
    }
    if (n.kind === 'image') {
      const src = n.source;
      if (isConvertibleSource(src)) {
        const bmp = bitmaps.get(src);
        if (bmp) {
          if (!seenForTransfer.has(bmp)) {
            transferList.push(bmp);
            seenForTransfer.add(bmp);
          }
          return { ...n, source: bmp } as ImageShape;
        }
      }
      return n;
    }
    return n;
  });
}
