import { registerNode } from '../registry';
import {
  resolveStyle,
  type Style,
  DEFAULT_STYLE,
  LINE_DEFAULT_STYLE,
  type Layer,
} from '../../runtime/types';
import { toVec2, ZERO_VEC2 } from '../_shared/coerce';
import { anchorTopLeft, ANCHOR_OPTIONS } from '../_shared/anchor';

/**
 * Shape nodes return SceneNodes — plain data describing what to draw.
 * The scene renderer (`runtime/scene-renderer.ts`) walks the tree and
 * emits Canvas2D calls. When no `style` wire is plugged in, shapes fall
 * back to a default (white fill / no stroke for filled shapes; thin
 * black stroke / no fill for stroke-driven shapes like Line).
 */

export const rectNode = registerNode({
  type: 'draw/rect',
  category: 'Draw',
  label: 'Rect',
  description:
    'Draw a rectangle anchored at `pos`. `anchor` picks which point of the ' +
    'box `pos` is — center (default) or a corner (tl/tr/bl/br).',
  inputs: [
    { name: 'pos', type: 'vec2', default: ZERO_VEC2 },
    { name: 'w', type: 'number', default: 50 },
    { name: 'h', type: 'number', default: 50 },
    { name: 'anchor', type: 'string', default: 'center', param: 'anchor' },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  defaultParams: { anchor: 'center' },
  paramOptions: { anchor: [...ANCHOR_OPTIONS] },
  compute: (pos, w, h, anchor, style) => {
    const ww = w as number;
    const hh = h as number;
    const { x, y } = anchorTopLeft(toVec2(pos), ww, hh, anchor as string);
    const resolved = resolveStyle(style as Style | null, DEFAULT_STYLE);
    const node: Layer = {
      kind: 'rect',
      x,
      y,
      w: ww,
      h: hh,
      mode: 'corner',
      style: resolved,
    };
    return [node];
  },
});

export const ellipseNode = registerNode({
  type: 'draw/ellipse',
  category: 'Draw',
  label: 'Ellipse',
  description:
    'Draw an ellipse anchored at `pos`. `anchor` picks which point of the ' +
    'bounding box `pos` is — center (default) or a corner (tl/tr/bl/br).',
  inputs: [
    { name: 'pos', type: 'vec2', default: ZERO_VEC2 },
    { name: 'w', type: 'number', default: 50 },
    { name: 'h', type: 'number', default: 50 },
    { name: 'anchor', type: 'string', default: 'center', param: 'anchor' },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  defaultParams: { anchor: 'center' },
  paramOptions: { anchor: [...ANCHOR_OPTIONS] },
  compute: (pos, w, h, anchor, style) => {
    const ww = w as number;
    const hh = h as number;
    const { x, y } = anchorTopLeft(toVec2(pos), ww, hh, anchor as string);
    const resolved = resolveStyle(style as Style | null, DEFAULT_STYLE);
    const node: Layer = {
      kind: 'ellipse',
      x,
      y,
      w: ww,
      h: hh,
      mode: 'corner',
      style: resolved,
    };
    return [node];
  },
});

export const lineNode = registerNode({
  type: 'draw/line',
  category: 'Draw',
  label: 'Line',
  description: 'Draw a line from point `a` to point `b`.',
  inputs: [
    { name: 'a', type: 'vec2', default: { x: 0, y: 0 } },
    { name: 'b', type: 'vec2', default: { x: 100, y: 100 } },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  compute: (a, b, style) => {
    const pa = toVec2(a), pb = toVec2(b);
    const node: Layer = {
      kind: 'line',
      x1: pa.x,
      y1: pa.y,
      x2: pb.x,
      y2: pb.y,
      // Stroke-driven default: thin black stroke, no fill.
      style: resolveStyle(style as Style | null, LINE_DEFAULT_STYLE),
    };
    return [node];
  },
});
