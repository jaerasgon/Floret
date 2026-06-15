import type { Vec2 } from '../../runtime/types';

/**
 * Box anchor — which point of a w×h box the `pos` input refers to.
 * Shared by the boxed draw primitives (rect, ellipse, arc, image,
 * image-tile) so they all interpret position the same way.
 */
export const ANCHOR_OPTIONS = ['center', 'tl', 'tr', 'bl', 'br'] as const;
export type Anchor = (typeof ANCHOR_OPTIONS)[number];

/**
 * Resolve `pos` (the named anchor point) to the box's top-left corner,
 * so the renderer can draw it in plain corner mode.
 */
export function anchorTopLeft(
  pos: Vec2,
  w: number,
  h: number,
  anchor: string | undefined,
): { x: number; y: number } {
  let x = pos.x;
  let y = pos.y;
  switch (anchor ?? 'center') {
    case 'center': x -= w / 2; y -= h / 2; break;
    case 'tr': x -= w; break;
    case 'bl': y -= h; break;
    case 'br': x -= w; y -= h; break;
    // 'tl' → (x, y) unchanged.
  }
  return { x, y };
}
