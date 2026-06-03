import { registerNode } from '../registry';
import {
  resolveStyle,
  type Style,
  DEFAULT_STYLE,
  LINE_DEFAULT_STYLE,
  type Layer,
  type Vec2,
} from '../../runtime/types';
import type { PortValue } from '../types';
import { toVec2 } from '../_shared/coerce';

/**
 * Additional 2D primitives: arc, triangle, quad, point, polygon.
 * Same default-style convention as rect/ellipse/line — filled shapes
 * default to white fill, stroke-driven (Line, Point, open Polygon)
 * default to thin black stroke.
 */

export const arcNode = registerNode({
  type: 'draw/arc',
  category: 'Draw',
  label: 'Arc',
  description: 'Draw an arc segment from `start` to `stop` radians.',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'w', type: 'number', default: 100 },
    { name: 'h', type: 'number', default: 100 },
    { name: 'start', type: 'number', default: 0 },
    { name: 'stop', type: 'number', default: Math.PI },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x, y, w, h, start, stop, style) => {
    const resolved = resolveStyle(style as Style | null, DEFAULT_STYLE);
    const node: Layer = {
      kind: 'arc',
      x: x as number,
      y: y as number,
      w: w as number,
      h: h as number,
      start: start as number,
      stop: stop as number,
      mode: resolved.ellipseMode,
      style: resolved,
    };
    return [node];
  },
});

export const triangleNode = registerNode({
  type: 'draw/triangle',
  category: 'Draw',
  label: 'Triangle',
  description: 'Draw a triangle from three corner points.',
  inputs: [
    { name: 'x1', type: 'number', default: 0 },
    { name: 'y1', type: 'number', default: 0 },
    { name: 'x2', type: 'number', default: 50 },
    { name: 'y2', type: 'number', default: 100 },
    { name: 'x3', type: 'number', default: -50 },
    { name: 'y3', type: 'number', default: 100 },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x1, y1, x2, y2, x3, y3, style) => {
    const node: Layer = {
      kind: 'triangle',
      x1: x1 as number, y1: y1 as number,
      x2: x2 as number, y2: y2 as number,
      x3: x3 as number, y3: y3 as number,
      style: resolveStyle(style as Style | null, DEFAULT_STYLE),
    };
    return [node];
  },
});

export const quadNode = registerNode({
  type: 'draw/quad',
  category: 'Draw',
  label: 'Quad',
  description: 'Draw a quadrilateral from four corner points.',
  inputs: [
    { name: 'x1', type: 'number', default: 0 },
    { name: 'y1', type: 'number', default: 0 },
    { name: 'x2', type: 'number', default: 100 },
    { name: 'y2', type: 'number', default: 0 },
    { name: 'x3', type: 'number', default: 100 },
    { name: 'y3', type: 'number', default: 100 },
    { name: 'x4', type: 'number', default: 0 },
    { name: 'y4', type: 'number', default: 100 },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x1, y1, x2, y2, x3, y3, x4, y4, style) => {
    const node: Layer = {
      kind: 'quad',
      x1: x1 as number, y1: y1 as number,
      x2: x2 as number, y2: y2 as number,
      x3: x3 as number, y3: y3 as number,
      x4: x4 as number, y4: y4 as number,
      style: resolveStyle(style as Style | null, DEFAULT_STYLE),
    };
    return [node];
  },
});

export const pointNode = registerNode({
  type: 'draw/point',
  category: 'Draw',
  label: 'Point',
  description: 'Draw a single point at (x, y).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x, y, style) => {
    const node: Layer = {
      kind: 'point',
      x: x as number,
      y: y as number,
      style: resolveStyle(style as Style | null, LINE_DEFAULT_STYLE),
    };
    return [node];
  },
});

export const polygonNode = registerNode({
  type: 'draw/polygon',
  category: 'Draw',
  label: 'Polygon',
  description: 'Draw a polygon from a spread of vec2 points.',
  inputs: [
    { name: 'points', type: 'vec2', kind: 'spread' },
    { name: 'closed', type: 'boolean', default: true },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  computeRaw: (inputs): PortValue[] => {
    const ptsSpread = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const closed = Boolean(inputs[1]?.[0] ?? true);
    const s = (inputs[2]?.[0] as Style | undefined) ?? null;
    const pts: Vec2[] = ptsSpread.map(toVec2);
    const baseStyle = closed ? DEFAULT_STYLE : LINE_DEFAULT_STYLE;
    const node: Layer = {
      kind: 'polygon',
      points: pts,
      closed,
      style: resolveStyle(s, baseStyle),
    };
    return [[node]];
  },
});
