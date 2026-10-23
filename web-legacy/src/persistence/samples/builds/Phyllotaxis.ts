import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Sample: Phyllotaxis ───────────────────────────────────────────────────
// A sunflower spiral. Each of N dots is placed by the golden angle:
//   angle = index · 2.39996,  radius = sqrt(index) · k
// driven entirely by the ForEach's built-in `index` / `fraction` loop vars —
// no data math to fake per-item variation. Hue sweeps with `fraction`.
// Centre-origin coordinates (vec2/from-angle is centred on 0,0).
export function buildPhyllotaxis() {
  const RID = 'ph_region';

  const count = node('ph_count', 'value/number', { x: -460, y: 60 }, { value: 500 });
  const range = node('ph_range', 'spread/range', { x: -300, y: 60 });

  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -40, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1300, height: 600, label: 'ForEach' },
      ports: [
        { id: 'n', name: 'n', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1300, height: 600 },
  };

  // ── Inside: golden-angle placement from the index loop-var ──
  const angleMul = node('ph_angle', 'math/mul', { x: 120, y: 80 }, { b: 2.399963 }, RID);   // index · golden angle
  const sqrtN = node('ph_sqrt', 'math/sqrt', { x: 120, y: 220 }, {}, RID);                  // sqrt(index)
  const radMul = node('ph_rad', 'math/mul', { x: 280, y: 220 }, { b: 10.5 }, RID);          // · k
  const pos = node('ph_pos', 'vec2/from-angle', { x: 460, y: 120 }, {}, RID);
  const hueMul = node('ph_hue', 'math/mul', { x: 120, y: 380 }, { b: 320 }, RID);           // fraction · 320
  const hsl = node('ph_hsl', 'color/hsl', { x: 300, y: 380 }, { s: 85, l: 60, a: 255 }, RID);
  const fill = node('ph_fill', 'style/fill', { x: 480, y: 380 }, {}, RID);
  const dot = node('ph_dot', 'draw/ellipse', { x: 660, y: 120 }, { w: 8, h: 8 }, RID);

  const renderer = node('ph_renderer', 'render/renderer', { x: 1380, y: 40 }, { width: 500, height: 500 });

  return {
    nodes: [count, range, region, angleMul, sqrtN, radMul, pos, hueMul, hsl, fill, dot, renderer],
    edges: [
      edge('ph_count', 'number', 'ph_range', 'count'),
      edge('ph_range', 'indices', RID, 'n'),       // drives N iterations
      // placement from the built-in index pin
      edge(RID, '__loopvar_index', 'ph_angle', 'a'),
      edge(RID, '__loopvar_index', 'ph_sqrt', 'x'),
      edge('ph_angle', 'result', 'ph_pos', 'angle'),
      edge('ph_sqrt', 'result', 'ph_rad', 'a'),
      edge('ph_rad', 'result', 'ph_pos', 'mag'),
      edge('ph_pos', 'vector', 'ph_dot', 'pos'),
      // hue from the built-in fraction pin
      edge(RID, '__loopvar_fraction', 'ph_hue', 'a'),
      edge('ph_hue', 'result', 'ph_hsl', 'h'),
      edge('ph_hsl', 'color', 'ph_fill', 'color'),
      edge('ph_fill', 'style', 'ph_dot', 'style'),
      // collect → render
      edge('ph_dot', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'ph_renderer', 'layers'),
    ],
  };
}
