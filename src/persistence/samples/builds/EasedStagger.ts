import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Sample: Eased Stagger ─────────────────────────────────────────────────
// A row of dots bobbing up and down. The wave TRAVELS because the ForEach
// `delay` pin rewinds each iteration's clock by index·delay, so every dot
// reads `time` slightly behind its neighbour. The bob shape comes from
// motion/ease (a Hermite curve), and hue + x come from the `fraction` loop
// var. Showcases the Phase-1 delay pin + Motion ease + loop vars together.
// Centre-origin coordinates.
export function buildEasedStagger() {
  const RID = 'es_region';

  const count = node('es_count', 'value/number', { x: -460, y: 60 }, { value: 28 });
  const range = node('es_range', 'spread/range', { x: -300, y: 60 });
  const delayV = node('es_delayV', 'value/number', { x: -300, y: 160 }, { value: 0.11 });

  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -40, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1320, height: 620, label: 'ForEach' },
      ports: [
        { id: 'n', name: 'n', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1320, height: 620 },
  };

  // ── Inside: each dot reads its own (delayed) clock ──
  const itime = node('es_time', 'time/seconds', { x: 120, y: 360 }, {}, RID);
  const tmul = node('es_tmul', 'math/mul', { x: 280, y: 360 }, { b: 0.45 }, RID);  // slow the cycle
  const saw = node('es_saw', 'math/fract', { x: 440, y: 360 }, {}, RID);           // 0…1 ramp
  const eased = node('es_eased', 'motion/ease', { x: 600, y: 360 }, { curve: 'inOutCubic' }, RID);
  const yMul = node('es_yMul', 'math/mul', { x: 760, y: 360 }, { b: 170 }, RID);
  const yOff = node('es_yOff', 'math/add', { x: 900, y: 360 }, { b: -85 }, RID);   // -85…85
  const xMul = node('es_xMul', 'math/mul', { x: 280, y: 120 }, { b: 470 }, RID);   // fraction · 470
  const xOff = node('es_xOff', 'math/add', { x: 440, y: 120 }, { b: -235 }, RID);  // -235…235
  const pos = node('es_pos', 'vec2/make', { x: 760, y: 160 }, {}, RID);
  const hue = node('es_hue', 'math/mul', { x: 280, y: 500 }, { b: 300 }, RID);     // fraction · 300
  const hsl = node('es_hsl', 'color/hsl', { x: 600, y: 500 }, { s: 85, l: 62, a: 255 }, RID);
  const fill = node('es_fill', 'style/fill', { x: 900, y: 500 }, {}, RID);
  const dot = node('es_dot', 'draw/ellipse', { x: 1060, y: 160 }, { w: 22, h: 22 }, RID);

  const renderer = node('es_renderer', 'render/renderer', { x: 1400, y: 40 }, { width: 500, height: 500 });

  return {
    nodes: [count, range, delayV, region, itime, tmul, saw, eased, yMul, yOff, xMul, xOff, pos, hue, hsl, fill, dot, renderer],
    edges: [
      edge('es_count', 'number', 'es_range', 'count'),
      edge('es_range', 'indices', RID, 'n'),     // drives N
      edge('es_delayV', 'number', RID, 'delay'), // per-iteration clock offset → travelling wave
      // delayed clock → eased bob → y
      edge('es_time', 't', 'es_tmul', 'a'),
      edge('es_tmul', 'result', 'es_saw', 'x'),
      edge('es_saw', 'result', 'es_eased', 't'),
      edge('es_eased', 'result', 'es_yMul', 'a'),
      edge('es_yMul', 'result', 'es_yOff', 'a'),
      // x from fraction
      edge(RID, '__loopvar_fraction', 'es_xMul', 'a'),
      edge('es_xMul', 'result', 'es_xOff', 'a'),
      edge('es_xOff', 'result', 'es_pos', 'x'),
      edge('es_yOff', 'result', 'es_pos', 'y'),
      edge('es_pos', 'vector', 'es_dot', 'pos'),
      // hue from fraction
      edge(RID, '__loopvar_fraction', 'es_hue', 'a'),
      edge('es_hue', 'result', 'es_hsl', 'h'),
      edge('es_hsl', 'color', 'es_fill', 'color'),
      edge('es_fill', 'style', 'es_dot', 'style'),
      edge('es_dot', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'es_renderer', 'layers'),
    ],
  };
}
