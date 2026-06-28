/**
 * Derived-image registry — OffscreenCanvas-based.
 *
 * Tier 2 image nodes (`image/filter`, `image/adjust`, `image/convolve`,
 * `image/pixelate`, `image/from-layer`, `transform/cache`) all need a
 * persistent backing surface to draw their output bytes into.
 * Allocating a fresh canvas per tick would burn GC and thrash the GPU
 * upload cache; so each producing node owns ONE surface keyed by its
 * node id, persistent across frames, resized lazily.
 *
 * Backing type is `OffscreenCanvas`, not `HTMLCanvasElement`. Same
 * Canvas2D API surface, but:
 *   - No DOM attachment overhead.
 *   - Works in Workers (sets up Phase 3b — runtime-tick offload).
 *   - Can be transferred + ImageBitmap-converted for the existing
 *     render-bridge worker.
 *
 * Downstream consumers (draw/image, image/filter, render-bridge) accept
 * either an HTMLCanvasElement (from io/image-url) or an OffscreenCanvas
 * (from us) via the standard `CanvasImageSource` union — `drawImage`
 * and `createImageBitmap` work uniformly on both.
 *
 * Lifecycle mirrors `image-registry` and `audio-registry`:
 *   - `ensure(nodeId)`            — get (or create) a 1×1 placeholder.
 *   - `ensureGraphics(nodeId, w, h)` — get a sized surface.
 *   - `ensureMany(nodeId, count)` — image/slicer's sub-images.
 *   - `retain(liveIds)`           — sweep dead-node entries (called from compile).
 *   - `dispose(nodeId)`           — drop a single entry.
 *   - `reset()`                   — patch reload.
 */

export interface OffscreenSurface {
  canvas: OffscreenCanvas;
  ctx: OffscreenCanvasRenderingContext2D;
  width: number;
  height: number;
}

interface CanvasEntry {
  kind: 'canvas';
  canvas: OffscreenCanvas;
}

interface GraphicsEntry {
  kind: 'graphics';
  surface: OffscreenSurface;
}

interface ManyEntry {
  kind: 'many';
  canvases: OffscreenCanvas[];
}

type Entry = CanvasEntry | GraphicsEntry | ManyEntry;

function makeCanvas(w: number, h: number): OffscreenCanvas {
  return new OffscreenCanvas(Math.max(1, Math.floor(w)), Math.max(1, Math.floor(h)));
}

class DerivedImageRegistry {
  private entries = new Map<string, Entry>();

  /**
   * Placeholder surface for nodes whose source isn't ready yet (e.g.
   * an image still loading). Always returns a 1×1 OffscreenCanvas —
   * resizes a reused entry back to 1×1 so callers can trust the
   * dimensions without a defensive check. Image-producing nodes use
   * this as their "no-op" output during loading windows; the runtime
   * downstream sees `width === 1 && height === 1` and skips.
   */
  ensure(nodeId: string): OffscreenCanvas {
    const existing = this.entries.get(nodeId);
    if (existing && existing.kind === 'canvas') {
      if (existing.canvas.width !== 1 || existing.canvas.height !== 1) {
        existing.canvas.width = 1;
        existing.canvas.height = 1;
      }
      return existing.canvas;
    }
    if (existing) this.disposeEntry(existing);
    const canvas = makeCanvas(1, 1);
    this.entries.set(nodeId, { kind: 'canvas', canvas });
    return canvas;
  }

  /**
   * Get or create a sized offscreen surface owned by `nodeId`. Resizing
   * is debounced — we only touch the canvas dims when they change.
   * Returns the canvas + its 2D context plus a convenience snapshot.
   */
  ensureGraphics(
    nodeId: string,
    w: number,
    h: number,
  ): OffscreenSurface {
    const cw = Math.max(1, Math.floor(w));
    const ch = Math.max(1, Math.floor(h));
    const existing = this.entries.get(nodeId);
    if (existing && existing.kind === 'graphics') {
      const s = existing.surface;
      if (s.width !== cw || s.height !== ch) {
        s.canvas.width = cw;
        s.canvas.height = ch;
        s.width = cw;
        s.height = ch;
      }
      return s;
    }
    if (existing) this.disposeEntry(existing);
    const canvas = makeCanvas(cw, ch);
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('derivedImageRegistry: 2D context unavailable');
    const surface: OffscreenSurface = { canvas, ctx, width: cw, height: ch };
    this.entries.set(nodeId, { kind: 'graphics', surface });
    return surface;
  }

  /**
   * Ensure exactly `count` placeholder canvases for `nodeId`. Reuses
   * the existing array on count-unchanged calls. Used by image/slicer
   * to produce a `Spread<image>` of sub-images.
   */
  ensureMany(nodeId: string, count: number): OffscreenCanvas[] {
    const existing = this.entries.get(nodeId);
    if (existing && existing.kind === 'many') {
      while (existing.canvases.length < count) {
        existing.canvases.push(makeCanvas(1, 1));
      }
      if (existing.canvases.length > count) {
        existing.canvases.length = count;
      }
      return existing.canvases;
    }
    if (existing) this.disposeEntry(existing);
    const canvases: OffscreenCanvas[] = [];
    for (let i = 0; i < count; i++) canvases.push(makeCanvas(1, 1));
    this.entries.set(nodeId, { kind: 'many', canvases });
    return canvases;
  }

  dispose(nodeId: string): void {
    const e = this.entries.get(nodeId);
    if (!e) return;
    this.disposeEntry(e);
    this.entries.delete(nodeId);
  }

  retain(liveNodeIds: ReadonlySet<string>): void {
    for (const [id, e] of this.entries) {
      if (liveNodeIds.has(id)) continue;
      this.disposeEntry(e);
      this.entries.delete(id);
    }
  }

  reset(): void {
    for (const e of this.entries.values()) this.disposeEntry(e);
    this.entries.clear();
  }

  private disposeEntry(_e: Entry): void {
    // OffscreenCanvases get GC'd once the reference drops. No explicit
    // teardown needed.
  }
}

export const derivedImageRegistry = new DerivedImageRegistry();

if (typeof window !== 'undefined') {
  (window as unknown as { __derivedImageRegistry?: unknown }).__derivedImageRegistry =
    derivedImageRegistry;
}
