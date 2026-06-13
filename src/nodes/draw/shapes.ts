import { registerNode } from '../registry';
import {
  resolveStyle,
  type Style,
  DEFAULT_STYLE,
  LINE_DEFAULT_STYLE,
  type Layer,
  type Vec2,
  type Color,
  colorToCss,
} from '../../runtime/types';
import type { PortValue } from '../types';
import { toVec2 } from '../_shared/coerce';
import { buildOffsetPolygon, widthsAreVariable } from '../../runtime/stroke-offset';

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
  outputs: [{ name: 'shape', type: 'layer' }],
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
  outputs: [{ name: 'shape', type: 'layer' }],
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
  outputs: [{ name: 'shape', type: 'layer' }],
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
  outputs: [{ name: 'shape', type: 'layer' }],
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
  description:
    'Draw a polygon from a spread of vec2 points. Optionally wire a `widths` spread to taper the stroke into a filled offset polygon.',
  inputs: [
    { name: 'points', type: 'vec2',    kind: 'spread' },
    { name: 'closed', type: 'boolean', default: true },
    { name: 'style',  type: 'style' },
    // Per-vertex stroke widths. When non-empty AND varying, the node
    // emits the Illustrator-style outlined polygon — fill colour comes
    // from the resolved style's `stroke` (or `fill` if there's no
    // stroke). Uniform / empty widths fall through to the regular
    // stroke render.
    { name: 'widths', type: 'number',  kind: 'spread', default: 0 },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  computeRaw: (inputs): PortValue[] => {
    const ptsSpread = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const closed = Boolean(inputs[1]?.[0] ?? true);
    const s = (inputs[2]?.[0] as Style | undefined) ?? null;
    const widths = ((inputs[3] ?? []) as ReadonlyArray<unknown>).map((v) => Number(v));
    const pts: Vec2[] = ptsSpread.map(toVec2);
    const baseStyle = closed ? DEFAULT_STYLE : LINE_DEFAULT_STYLE;
    const resolved = resolveStyle(s, baseStyle);

    if (widthsAreVariable(widths) && pts.length >= 2) {
      // Variable-width branch — outline the polyline and emit as a
      // filled polygon. Prefer the user's fill colour (the ribbon IS
      // a filled shape) and fall through to stroke when no fill is
      // configured. Drop the stroke so we don't ALSO draw a thin
      // outline around the ribbon.
      const ribbon = buildOffsetPolygon(pts, widths, resolved.strokeWeight, closed);
      const node: Layer = {
        kind: 'polygon',
        points: ribbon,
        closed: true,
        style: {
          ...resolved,
          fill: resolved.fill ?? resolved.stroke ?? '#ffffff',
          stroke: null,
        },
      };
      return [[node]];
    }

    const node: Layer = {
      kind: 'polygon',
      points: pts,
      closed,
      style: resolved,
    };
    return [[node]];
  },
});

/**
 * Path — the "draw a line through these points" convenience node.
 *
 * Polygon's pin-heavy interface (points, closed, style, plus a separate
 * Stroke / StrokeWeight chain) is too much friction for the common case
 * of "stroke a smooth curve in this colour at this width." Path takes
 * the three knobs that 95% of strokes actually need and bakes them into
 * the resolved style itself — no Style chain required.
 *
 *   points       vec2 spread       polyline vertices
 *   color        Color (or null)   stroke colour; null falls back to black
 *   strokeWidth  number            px; clamps to 0 (invisible) or above
 *   tension      number 0..1       0 = straight lines, 1 = full Catmull–Rom
 *
 * The path is rendered as an OPEN polyline (no fill), which is what
 * "path" reads as in every vector tool. Wire a `transform/close` or use
 * Polygon if you need a closed filled region.
 */
export const pathNode = registerNode({
  type: 'draw/path',
  category: 'Draw',
  label: 'Path',
  description:
    'Stroke a path through a spread of vec2 points. Tension > 0 turns straight lines into Catmull–Rom curves. Wire `widths` to taper.',
  inputs: [
    { name: 'points',      type: 'vec2',   kind: 'spread' },
    { name: 'color',       type: 'color',  default: null },
    { name: 'strokeWidth', type: 'number', default: 2 },
    { name: 'tension',     type: 'number', default: 0 },
    // Optional variable-width input — when non-empty and varying, the
    // path becomes a filled offset polygon (Illustrator-style ribbon)
    // instead of a stroked polyline. Tension > 0 still smooths the
    // polyline before offsetting via straight Catmull–Rom sampling
    // would be ideal, but for v1 we only offset the raw vertices.
    { name: 'widths',      type: 'number', kind: 'spread', default: 0 },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  computeRaw: (inputs): PortValue[] => {
    const ptsSpread = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const colorIn = inputs[1]?.[0] as Color | null | undefined;
    const widthIn = Number(inputs[2]?.[0] ?? 2);
    const tensionIn = Number(inputs[3]?.[0] ?? 0);
    const widths = ((inputs[4] ?? []) as ReadonlyArray<unknown>).map((v) => Number(v));
    const pts: Vec2[] = ptsSpread.map(toVec2);
    // Clamp tension into [0, 1]. Above 1 starts producing loops at sharp
    // corners — interesting but rarely what the user asked for. Below 0
    // is meaningless. We let the renderer's `tension <= 0` branch handle
    // the "straight lines" path so this stays a pure clamp.
    const tension = tensionIn < 0 ? 0 : tensionIn > 1 ? 1 : tensionIn;
    // Path is stroke-driven: start from the line default (no fill, thin
    // black stroke), then override stroke colour & width with the inputs.
    // A null colour input falls back to the default black so an unwired
    // node still renders something visible.
    const stroke = colorIn ? colorToCss(colorIn) : LINE_DEFAULT_STYLE.stroke;
    const baseWidth = Math.max(0, widthIn);

    if (widthsAreVariable(widths) && pts.length >= 2) {
      // Variable-width ribbon. Tension/smoothing is ignored for v1 —
      // applying Catmull–Rom resampling before offsetting is a worthy
      // follow-up.
      const ribbon = buildOffsetPolygon(pts, widths, baseWidth, false);
      const node: Layer = {
        kind: 'polygon',
        points: ribbon,
        closed: true,
        style: {
          ...LINE_DEFAULT_STYLE,
          fill: stroke ?? '#000',
          stroke: null,
        },
      };
      return [[node]];
    }

    const node: Layer = {
      kind: 'polygon',
      points: pts,
      closed: false,
      tension,
      style: {
        ...LINE_DEFAULT_STYLE,
        stroke,
        strokeWeight: baseWidth,
      },
    };
    return [[node]];
  },
});
