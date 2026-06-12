/**
 * Test-environment canvas polyfill.
 *
 * Floret's runtime is correct: `OffscreenCanvas` exists in every target
 * browser, and rasterising nodes (transform/cache, image/*, particle
 * text rasterisation) rely on it. But the default vitest environment is
 * plain Node with no DOM and no `OffscreenCanvas`, so those nodes throw
 * `ReferenceError: OffscreenCanvas is not defined` the moment a sample
 * test ticks them — which previously aborted the frame and, post-#13,
 * merely emits an empty layer (still failing "produces ≥1 layer").
 *
 * This installs a minimal, side-effect-free mock so the rasterising path
 * runs to completion in tests. It draws nothing real (no pixels), but
 * every Canvas2D call is a safe no-op and every data-returning method
 * hands back a correctly-shaped stub. That's enough for the assertions
 * these tests actually make — that a sample compiles, ticks, and emits a
 * layer — without pulling in a native canvas backend.
 *
 * Registered via `test.setupFiles` in vite.config.ts.
 */

// A no-op 2D context. A Proxy backs it so any method we didn't explicitly
// model is still a callable no-op (returns undefined) rather than throwing,
// and any property assignment (fillStyle, font, lineWidth, …) is simply
// stored. The handful of methods that callers read structured data back
// from get real-enough return shapes.
function makeContext(canvas: unknown): unknown {
  const store: Record<string, unknown> = {};

  const dataReturning: Record<string, (...args: unknown[]) => unknown> = {
    measureText: (text: unknown) => ({
      width: String(text ?? '').length * 6,
      actualBoundingBoxAscent: 8,
      actualBoundingBoxDescent: 2,
      actualBoundingBoxLeft: 0,
      actualBoundingBoxRight: String(text ?? '').length * 6,
    }),
    createLinearGradient: () => ({ addColorStop: () => {} }),
    createRadialGradient: () => ({ addColorStop: () => {} }),
    createConicGradient: () => ({ addColorStop: () => {} }),
    createPattern: () => ({}),
    getImageData: (..._args: unknown[]) => {
      const w = Math.max(1, Math.floor(Number(_args[2]) || 1));
      const h = Math.max(1, Math.floor(Number(_args[3]) || 1));
      return { data: new Uint8ClampedArray(w * h * 4), width: w, height: h };
    },
    createImageData: (..._args: unknown[]) => {
      const w = Math.max(1, Math.floor(Number(_args[0]) || 1));
      const h = Math.max(1, Math.floor(Number(_args[1]) || 1));
      return { data: new Uint8ClampedArray(w * h * 4), width: w, height: h };
    },
    getLineDash: () => [],
  };

  const target: Record<string, unknown> = { canvas };

  return new Proxy(target, {
    get(t, prop: string) {
      if (prop === 'canvas') return canvas;
      if (prop in dataReturning) return dataReturning[prop];
      if (prop in store) return store[prop]; // previously-set property
      if (prop in t) return (t as Record<string, unknown>)[prop];
      // Unknown access → a callable no-op. Covers every drawing method
      // (save/restore/beginPath/fill/stroke/translate/…) uniformly.
      return () => undefined;
    },
    set(_t, prop: string, value) {
      store[prop] = value;
      return true;
    },
  });
}

class MockOffscreenCanvas {
  width: number;
  height: number;
  private _ctx: unknown = null;

  constructor(width: number, height: number) {
    this.width = Math.max(1, Math.floor(width));
    this.height = Math.max(1, Math.floor(height));
  }

  getContext(kind: string): unknown {
    if (kind !== '2d') return null;
    if (!this._ctx) this._ctx = makeContext(this);
    return this._ctx;
  }

  // Some pipelines convert a surface to a bitmap; hand back a tiny stub.
  transferToImageBitmap(): unknown {
    return { width: this.width, height: this.height, close: () => {} };
  }
}

// Only install if the host genuinely lacks it (don't clobber a real
// browser-like environment such as jsdom-with-canvas).
const g = globalThis as unknown as {
  OffscreenCanvas?: unknown;
  createImageBitmap?: unknown;
};
if (typeof g.OffscreenCanvas === 'undefined') {
  g.OffscreenCanvas = MockOffscreenCanvas as unknown;
}
if (typeof g.createImageBitmap === 'undefined') {
  g.createImageBitmap = (src: unknown) =>
    Promise.resolve({
      width: (src as { width?: number })?.width ?? 1,
      height: (src as { height?: number })?.height ?? 1,
      close: () => {},
    });
}
