/**
 * Image registry.
 *
 * Per-node-id cache of `HTMLImageElement`s loaded from URLs. Mirrors
 * the audio registry's lifecycle: `ensure()` creates the element on
 * first call and reuses it across frames; `dispose()` drops it; URL
 * changes trigger a recreate.
 *
 * We load into a plain `<img>` and copy it into a `<canvas>`, which is
 * what flows down the "image" wire. The GL renderer uploads that canvas
 * straight to a texture (`texImage2D` accepts any `CanvasImageSource`),
 * so no wrapper object is needed.
 *
 * CORS: `crossOrigin = 'anonymous'` is set so a cooperating server's
 * image is non-tainted (pixel reads work later if we add a filter
 * node). For servers without CORS the image still loads and draws —
 * it's just opaque to `getImageData`.
 */

interface Entry {
  url: string;
  img: HTMLImageElement;
  /**
   * Once the image loads we copy it into a real `<canvas>` and hand
   * *that* out as the "image" wire value — a uniform `CanvasImageSource`
   * the GL renderer can texture directly, regardless of the original URL.
   */
  canvas: HTMLCanvasElement;
  loaded: boolean;
  failed: boolean;
}

class ImageRegistry {
  private entries = new Map<string, Entry>();

  /**
   * Ensure a loaded image for (nodeId, url). On URL change the previous
   * entry is disposed. Returns the canvas-wrapped element — initially a
   * 1×1 transparent canvas, replaced with the loaded bytes once they
   * arrive. Call `isLoaded(nodeId)` per frame to gate drawing.
   */
  ensure(nodeId: string, url: string): HTMLCanvasElement {
    const existing = this.entries.get(nodeId);
    if (existing && existing.url === url) return existing.canvas;
    if (existing) this.disposeEntry(existing);

    const img = new Image();
    img.crossOrigin = 'anonymous';
    const canvas = document.createElement('canvas');
    canvas.width = 1;
    canvas.height = 1;
    const entry: Entry = { url, img, canvas, loaded: false, failed: false };
    this.entries.set(nodeId, entry);
    img.onload = () => {
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext('2d');
      if (ctx) ctx.drawImage(img, 0, 0);
      entry.loaded = true;
    };
    img.onerror = () => {
      entry.failed = true;
      // eslint-disable-next-line no-console
      console.warn(`image load failed: ${url}`);
    };
    img.src = url;
    return canvas;
  }

  isLoaded(nodeId: string): boolean {
    return this.entries.get(nodeId)?.loaded === true;
  }

  /** Tear down a single node's image element. */
  dispose(nodeId: string): void {
    const e = this.entries.get(nodeId);
    if (!e) return;
    this.disposeEntry(e);
    this.entries.delete(nodeId);
  }

  /** Drop entries for any node not in the live set. Matches the audio
   *  registry's retain() pattern — called from the editor's _compile. */
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

  private disposeEntry(e: Entry): void {
    // Cancel the in-flight load if any; null src + handlers so the GC
    // can reclaim the element.
    e.img.onload = null;
    e.img.onerror = null;
    e.img.src = '';
  }
}

export const imageRegistry = new ImageRegistry();

if (typeof window !== 'undefined') {
  (window as unknown as { __imageRegistry?: unknown }).__imageRegistry = imageRegistry;
}
