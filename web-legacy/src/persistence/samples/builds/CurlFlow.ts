import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Sample: Curl Flow ─────────────────────────────────────────────────────
// A grid of streaks aligned to a divergence-free curl-noise field. Each grid
// point samples noise/curl (drifting with time) and draws a short line along
// the flow — the classic swirling, incompressible flow-field look.
// Authored with current port names + real editor coordinates (no spaced()).
export function buildCurlFlow() {
  const RID = 'cf_region';

  const renderer = node('cf_renderer', 'render/renderer', { x: 1640, y: 40 }, { width: 500, height: 500 });
  const grid = node('cf_grid', 'spread/grid', { x: -320, y: 80 }, { cols: 26, rows: 26, w: 460, h: 460, center: { x: 250, y: 250 } });

  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 60, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1360, height: 640, label: 'ForEach' },
      ports: [
        { id: 'pos', name: 'pos', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1360, height: 640 },
  };

  // ── Inside the region ──
  const time = node('cf_time', 'time/seconds', { x: 60, y: 420 }, {}, RID);
  const driftMul = node('cf_driftMul', 'math/mul', { x: 240, y: 420 }, { slotCount: 2, b: 18 }, RID);
  const drift = node('cf_drift', 'vec2/make', { x: 420, y: 400 }, {}, RID);          // (time·18, 0)
  const sampleP = node('cf_sampleP', 'math/add', { x: 600, y: 140 }, { slotCount: 2, valueType: 'vec2' }, RID);
  const curl = node('cf_curl', 'noise/curl', { x: 780, y: 140 }, { scale: 0.012, seed: 7, octaves: 2, falloff: 0.5 }, RID);
  const flowScale = node('cf_flowScale', 'vec2/scale', { x: 960, y: 200 }, { s: 16 }, RID);
  const endP = node('cf_endP', 'math/add', { x: 1140, y: 100 }, { slotCount: 2, valueType: 'vec2' }, RID);
  const col = node('cf_col', 'color/hex', { x: 960, y: 360 }, { hex: '#38e0d0' }, RID);
  const stroke = node('cf_stroke', 'style/stroke', { x: 1140, y: 340 }, { width: 2 }, RID);
  const line = node('cf_line', 'draw/line', { x: 1320, y: 100 }, {}, RID);

  return {
    nodes: [renderer, grid, region, time, driftMul, drift, sampleP, curl, flowScale, endP, col, stroke, line],
    edges: [
      edge('cf_grid', 'positions', RID, 'pos'),
      // sample point = grid point + time drift, advanced through curl noise
      edge('cf_time', 't', 'cf_driftMul', 'a'),
      edge('cf_driftMul', 'result', 'cf_drift', 'x'),
      edge(RID, 'pos', 'cf_sampleP', 'a'),
      edge('cf_drift', 'vector', 'cf_sampleP', 'b'),
      edge('cf_sampleP', 'result', 'cf_curl', 'pos'),
      // streak from the grid point along the flow vector
      edge('cf_curl', 'flow', 'cf_flowScale', 'v'),
      edge(RID, 'pos', 'cf_endP', 'a'),
      edge('cf_flowScale', 'scaled', 'cf_endP', 'b'),
      edge(RID, 'pos', 'cf_line', 'a'),
      edge('cf_endP', 'result', 'cf_line', 'b'),
      edge('cf_col', 'color', 'cf_stroke', 'color'),
      edge('cf_stroke', 'style', 'cf_line', 'style'),
      edge('cf_line', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'cf_renderer', 'layers'),
    ],
  };
}
