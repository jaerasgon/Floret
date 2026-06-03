import { registerNode } from '../registry';
import {
  resolveStyle,
  type Style,
  DEFAULT_STYLE,
  LINE_DEFAULT_STYLE,
  type Layer,
} from '../../runtime/types';

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
  description: 'Draw a rectangle at (x, y) with size (w, h).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'w', type: 'number', default: 50 },
    { name: 'h', type: 'number', default: 50 },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x, y, w, h, style) => {
    const resolved = resolveStyle(style as Style | null, DEFAULT_STYLE);
    const node: Layer = {
      kind: 'rect',
      x: x as number,
      y: y as number,
      w: w as number,
      h: h as number,
      // Read mode off the resolved style — a chained `style/rect-mode`
      // node can override here.
      mode: resolved.rectMode,
      style: resolved,
    };
    return [node];
  },
});

export const ellipseNode = registerNode({
  type: 'draw/ellipse',
  category: 'Draw',
  label: 'Ellipse',
  description: 'Draw an ellipse at (x, y) with size (w, h).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'w', type: 'number', default: 50 },
    { name: 'h', type: 'number', default: 50 },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x, y, w, h, style) => {
    const resolved = resolveStyle(style as Style | null, DEFAULT_STYLE);
    const node: Layer = {
      kind: 'ellipse',
      x: x as number,
      y: y as number,
      w: w as number,
      h: h as number,
      mode: resolved.ellipseMode,
      style: resolved,
    };
    return [node];
  },
});

export const lineNode = registerNode({
  type: 'draw/line',
  category: 'Draw',
  label: 'Line',
  description: 'Draw a line from (x1, y1) to (x2, y2).',
  inputs: [
    { name: 'x1', type: 'number', default: 0 },
    { name: 'y1', type: 'number', default: 0 },
    { name: 'x2', type: 'number', default: 100 },
    { name: 'y2', type: 'number', default: 100 },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x1, y1, x2, y2, style) => {
    const node: Layer = {
      kind: 'line',
      x1: x1 as number,
      y1: y1 as number,
      x2: x2 as number,
      y2: y2 as number,
      // Stroke-driven default: thin black stroke, no fill.
      style: resolveStyle(style as Style | null, LINE_DEFAULT_STYLE),
    };
    return [node];
  },
});
