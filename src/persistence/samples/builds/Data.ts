import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';


// ─── Sample: CSV Bar Chart ─────────────────────────────────────────────────
// Inline CSV parsed into a number spread; bars rendered per value
// with HSL hue along the index axis. Edit the `data` param on the
// CSVInline node to load a different series.
export function buildCSVBarChart() {
  const csv = node('csv_csv', 'data/csv-inline', { x: -480, y: 60 }, {
    data: '12, 45, 22, 67, 38, 91, 54, 73, 29, 60, 84, 19, 41, 77, 33, 55',
  });
  const count = node('csv_count', 'spread/count', { x: -260, y: 60 });
  const idx = node('csv_idx', 'spread/range', { x: -480, y: 160 }, { count: 16 });
  // Bar width = canvasW / count; we hardcode 16 bars → 28px wide with 4px gap
  const barW = node('csv_barW', 'value/number', { x: -480, y: 240 }, { value: 26 });
  const gap = node('csv_gap', 'value/number', { x: -480, y: 300 }, { value: 6 });
  // baseY = 220 puts the bars sitting near the bottom edge (canvas spans -250..250).
  const baseY = node('csv_baseY', 'value/number', { x: -480, y: 360 }, { value: 220 });
  const heightScale = node('csv_heightScale', 'value/number', { x: -480, y: 420 }, { value: 4 });

  const RID = 'csv_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 200, y: 60 },
    data: {
      defType: 'region/foreach',
      params: { width: 880, height: 480, label: 'ForEach' },
      ports: [
        { id: 'v', name: 'v', type: 'number', direction: 'input' },
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 880, height: 480 },
  };

  // Inside: x = i * (barW + gap) + 20; height = v * heightScale; y = baseY - height
  const stride = node('csv_stride', 'math/add', { x: 60, y: 140 }, {}, RID);
  const xMul = node('csv_xMul', 'math/mul', { x: 240, y: 60 }, {}, RID);
  // Start the first bar near the left edge: -240 (canvas spans -250..250).
  const xFin = node('csv_xFin', 'math/add', { x: 420, y: 60 }, { b: -240 }, RID);
  const hMul = node('csv_hMul', 'math/mul', { x: 240, y: 140 }, {}, RID);
  const yFin = node('csv_yFin', 'math/sub', { x: 420, y: 140 }, {}, RID);
  // Hue across the spread
  const hueMul = node('csv_hueMul', 'math/mul', { x: 60, y: 260 }, { b: 22 }, RID);
  const hsl = node('csv_hsl', 'color/hsl', { x: 240, y: 260 }, { s: 80, l: 60, a: 255 }, RID);
  const fill = node('csv_fill', 'style/fill', { x: 420, y: 260 }, { mode: 'color' }, RID);
  const rect = node('csv_rect', 'draw/rect', { x: 600, y: 60 }, {}, RID);

  const bg = node('csv_bg', 'color/rgb', { x: -480, y: 500 }, { r: 24, g: 24, b: 30, a: 255 });
  const renderer = node('csv_renderer', 'render/renderer', { x: 1140, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      csv, count, idx, barW, gap, baseY, heightScale,
      region,
      stride, xMul, xFin, hMul, yFin,
      hueMul, hsl, fill, rect,
      bg, renderer,
    ],
    edges: [
      edge('csv_bg', 'out', 'csv_renderer', 'background'),
      edge('csv_csv', 'out', 'csv_count', 'spread'),
      edge('csv_csv', 'out', RID, 'v'),
      edge('csv_idx', 'out', RID, 'i'),
      // stride = barW + gap
      edge('csv_barW', 'out', 'csv_stride', 'a'),
      edge('csv_gap', 'out', 'csv_stride', 'b'),
      // x = i * stride + 20
      edge(RID, 'i', 'csv_xMul', 'a'),
      edge('csv_stride', 'out', 'csv_xMul', 'b'),
      edge('csv_xMul', 'out', 'csv_xFin', 'a'),
      // height = v * heightScale
      edge(RID, 'v', 'csv_hMul', 'a'),
      edge('csv_heightScale', 'out', 'csv_hMul', 'b'),
      // y = baseY - height (bar sits on baseY line)
      edge('csv_baseY', 'out', 'csv_yFin', 'a'),
      edge('csv_hMul', 'out', 'csv_yFin', 'b'),
      // Hue
      edge(RID, 'i', 'csv_hueMul', 'a'),
      edge('csv_hueMul', 'out', 'csv_hsl', 'h'),
      edge('csv_hsl', 'out', 'csv_fill', 'color'),
      // Rect
      edge('csv_xFin', 'out', 'csv_rect', 'x'),
      edge('csv_yFin', 'out', 'csv_rect', 'y'),
      edge('csv_barW', 'out', 'csv_rect', 'w'),
      edge('csv_hMul', 'out', 'csv_rect', 'h'),
      edge('csv_fill', 'out', 'csv_rect', 'style'),
      edge('csv_rect', 'out', RID, 'layer'),
      edge(RID, 'layer', 'csv_renderer', 'layers'),
    ],
  };
}
