import type { SceneNode, ImageShape } from './scene';

/**
 * Render bridge — main-thread side of the worker offload.
 *
 * Owns the worker lifecycle and the per-frame post protocol:
 *
 *   1. `new RenderBridge(canvas)` — transfers the canvas's drawing
 *      surface to a fresh worker via `transferControlToOffscreen()`.
 *      After this the main thread cannot draw on the canvas; only the
 *      worker can.
 *   2. `resize(w, h)` — relays canvas-size changes.
 *   3. `frame(layers, bgCss, centreOrigin)` — posts the scene tree for
 *      this frame. Walks the tree to convert any `HTMLCanvasElement`
 *      sources on image SceneNodes into transferable `ImageBitmap`s
 *      (structured-clone-safe + zero-copy when transferred).
 *   4. `dispose()` — tears the worker down.
 *
 * Each Preview / renderer canvas owns its own `RenderBridge` so the
 * editor can host multiple Renderer outputs side-by-side. Workers are
 * one per bridge — heavyweight to create but cheap per-frame.
 */
export class RenderBridge {
  private worker: Worker | null = null;
  private mounted = false;

  constructor(canvas: HTMLCanvasElement) {
    this.worker = new Worker(new URL('./render-worker.ts', import.meta.url), {
      type: 'module',
    });
    const offscreen = canvas.transferControlToOffscreen();
    this.worker.postMessage({ type: 'init', canvas: offscreen }, [offscreen]);
    this.mounted = true;
  }

  resize(w: number, h: number): void {
    this.worker?.postMessage({ type: 'resize', w, h });
  }

  async frame(
    layers: ReadonlyArray<SceneNode>,
    bg: string,
    centreOrigin: boolean,
  ): Promise<void> {
    if (!this.worker || !this.mounted) return;
    const sources = new Map<ConvertibleSource, Promise<ImageBitmap>>();
    collectImageSources(layers, sources);
    if (sources.size === 0) {
      this.worker.postMessage({ type: 'frame', layers, bg, centreOrigin });
      return;
    }
    const entries = await Promise.all(
      Array.from(sources, async ([canvas, _]) => [canvas, await createImageBitmap(canvas)] as const),
    );
    const bitmapMap = new Map<ConvertibleSource, ImageBitmap>(entries);
    const transferList: Transferable[] = [];
    const seenForTransfer = new Set<ImageBitmap>();
    const swapped = swapImageSources(layers, bitmapMap, transferList, seenForTransfer);
    this.worker.postMessage({ type: 'frame', layers: swapped, bg, centreOrigin }, transferList);
  }

  dispose(): void {
    if (this.worker) {
      try { this.worker.postMessage({ type: 'dispose' }); } catch { /* ignore */ }
      this.worker.terminate();
      this.worker = null;
    }
    this.mounted = false;
  }
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
