import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Sample: Local Peaks ───────────────────────────────────────────────────
// A row of noise bars. Each bar lights up green only if it is taller than BOTH
// of its neighbours — a local maximum. That per-element "compare to my left and
// right" is exactly what spread/neighbor enables inside a ForEach: broadcast the
// whole heights spread in, then read offset −1 / +1 against the `index` pin.
// Centre-origin coordinates.
export function buildLocalPeaks() {
  const RID = 'lp_region';

  // Jagged 1-D series at the root, ready to broadcast into the loop.
  const heights = node('lp_heights', 'spread/noise', { x: -460, y: 60 }, { count: 40, frequency: 4, seed: 7 });

  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -40, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1340, height: 640, label: 'ForEach' },
      ports: [
        { id: 'h', name: 'h', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1340, height: 640 },
  };

  // ── Inside: compare self against left/right neighbours ──
  const nbL = node('lp_nbL', 'spread/neighbor', { x: 120, y: 320 }, { offset: -1, edges: 'clamp' }, RID);
  const nbR = node('lp_nbR', 'spread/neighbor', { x: 120, y: 460 }, { offset: 1, edges: 'clamp' }, RID);
  const gtL = node('lp_gtL', 'math/step', { x: 320, y: 320 }, {}, RID);     // self >= left ? 1 : 0
  const gtR = node('lp_gtR', 'math/step', { x: 320, y: 460 }, {}, RID);     // self >= right ? 1 : 0
  const peak = node('lp_peak', 'math/mul', { x: 500, y: 380 }, {}, RID);    // both → local max
  const sat = node('lp_sat', 'math/mul', { x: 660, y: 380 }, { b: 95 }, RID);
  const hsl = node('lp_hsl', 'color/hsl', { x: 820, y: 380 }, { h: 135, l: 55, a: 255 }, RID);
  const fill = node('lp_fill', 'style/fill', { x: 980, y: 380 }, {}, RID);

  const xMul = node('lp_xMul', 'math/mul', { x: 320, y: 120 }, { b: 460 }, RID);   // fraction · 460
  const xOff = node('lp_xOff', 'math/add', { x: 480, y: 120 }, { b: -230 }, RID);  // -230…230
  const hMul = node('lp_hMul', 'math/mul', { x: 320, y: 220 }, { b: 210 }, RID);   // bar height
  const pos = node('lp_pos', 'vec2/make', { x: 660, y: 140 }, { y: 200 }, RID);    // baseline y = 200
  const bar = node('lp_bar', 'draw/rect', { x: 1160, y: 140 }, { w: 11, anchor: 'bl' }, RID);

  const renderer = node('lp_renderer', 'render/renderer', { x: 1420, y: 40 }, { width: 500, height: 500 });

  return {
    nodes: [heights, region, nbL, nbR, gtL, gtR, peak, sat, hsl, fill, xMul, xOff, hMul, pos, bar, renderer],
    edges: [
      edge('lp_heights', 'samples', RID, 'h'),          // drives N + per-bar self height
      edge('lp_heights', 'samples', 'lp_nbL', 'spread'), // broadcast whole series in
      edge('lp_heights', 'samples', 'lp_nbR', 'spread'),
      edge(RID, '__loopvar_index', 'lp_nbL', 'index'),
      edge(RID, '__loopvar_index', 'lp_nbR', 'index'),
      // peak = (self >= left) AND (self >= right)
      edge('lp_nbL', 'value', 'lp_gtL', 'edge'),
      edge(RID, 'h', 'lp_gtL', 'x'),
      edge('lp_nbR', 'value', 'lp_gtR', 'edge'),
      edge(RID, 'h', 'lp_gtR', 'x'),
      edge('lp_gtL', 'result', 'lp_peak', 'a'),
      edge('lp_gtR', 'result', 'lp_peak', 'b'),
      edge('lp_peak', 'result', 'lp_sat', 'a'),
      edge('lp_sat', 'result', 'lp_hsl', 's'),           // grey unless a peak → green
      edge('lp_hsl', 'color', 'lp_fill', 'color'),
      // bar geometry
      edge(RID, '__loopvar_fraction', 'lp_xMul', 'a'),
      edge('lp_xMul', 'result', 'lp_xOff', 'a'),
      edge('lp_xOff', 'result', 'lp_pos', 'x'),
      edge(RID, 'h', 'lp_hMul', 'a'),
      edge('lp_hMul', 'result', 'lp_bar', 'h'),
      edge('lp_pos', 'vector', 'lp_bar', 'pos'),
      edge('lp_fill', 'style', 'lp_bar', 'style'),
      edge('lp_bar', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'lp_renderer', 'layers'),
    ],
  };
}
