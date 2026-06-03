import type { Color, Vec2 } from '../../runtime/types';

/**
 * Defensive coercions for `any`-typed wires.
 *
 * The runtime's port-type system enforces type compatibility at connect
 * time, so in steady state every `vec2`-typed input arrives as a Vec2
 * and every `color`-typed input arrives as a Color. But the `any` port
 * type bypasses that check, and several spread / convert nodes route
 * through it — so each consumer that READS these shapes does a runtime
 * structural check and falls back to a sensible default.
 *
 * Centralised here so the default values and shape checks stay aligned
 * with `runtime/types.ts` definitions. If the canonical shape changes
 * (e.g. Vec2 gains a z), bump these and every consumer follows.
 */

export const ZERO_VEC2: Vec2 = { x: 0, y: 0 };
export const WHITE_COLOR: Color = { r: 1, g: 1, b: 1, a: 1 };

export function toVec2(v: unknown): Vec2 {
  if (v && typeof v === 'object') {
    const o = v as { x?: unknown; y?: unknown };
    if (typeof o.x === 'number' && typeof o.y === 'number') {
      return { x: o.x, y: o.y };
    }
  }
  return ZERO_VEC2;
}

export function toColor(v: unknown): Color {
  if (v && typeof v === 'object') {
    const o = v as Partial<Color>;
    if (typeof o.r === 'number' && typeof o.g === 'number' && typeof o.b === 'number') {
      return { r: o.r, g: o.g, b: o.b, a: typeof o.a === 'number' ? o.a : 1 };
    }
  }
  return WHITE_COLOR;
}

/**
 * Image wire values can be either an HTMLCanvasElement (from io/image-url
 * after a successful load) or an OffscreenCanvas (from derived-image-registry
 * after Phase 3c). Both expose width/height + getContext('2d') + work with
 * drawImage and createImageBitmap. Guard the consumer side with this.
 */
export type ImageCanvas = HTMLCanvasElement | OffscreenCanvas;

export function isImageCanvas(v: unknown): v is ImageCanvas {
  if (!v) return false;
  if (typeof HTMLCanvasElement !== 'undefined' && v instanceof HTMLCanvasElement) return true;
  if (typeof OffscreenCanvas !== 'undefined' && v instanceof OffscreenCanvas) return true;
  return false;
}

/** Image-canvas is ready to draw when both dims > 1 (registry hands out
 *  1×1 placeholders during the load window). */
export function isReadyImageCanvas(v: unknown): v is ImageCanvas {
  return isImageCanvas(v) && v.width > 1 && v.height > 1;
}
