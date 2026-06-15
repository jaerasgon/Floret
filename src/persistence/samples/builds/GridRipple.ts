import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Sample: Grid Ripple ───────────────────────────────────────────────────
// A grid of stroked squares. Each cell is offset vertically by
// sin(dist_from_centre · scale + time), so a radial ripple travels across the
// lattice. The ForEach iterates the grid positions; `delay` staggers each
// iteration's clock. Stroke colours are a palette cycled + shuffled across the
// cells (a wired Stroke style means the squares are outline-only — no fill).
//
// Authored from a live patch, so node positions are real editor coordinates
// (no `spaced()` rescale) and every port name matches the current node defs.
export function buildGridRipple() {
  const RID = 'gr_region';

  const renderer = node('gr_renderer', 'render/renderer', { x: 2214, y: 201 }, { width: 500, height: 500 });
  const grid = node('gr_grid', 'spread/grid', { x: -288, y: 105 });
  const palette = node('gr_palette', 'color/palette', { x: -441, y: 519 }, {
    stopCount: 4,
    c0: '#fed000', c1: '#fc8405', c2: '#ed361a', c3: '#4464a1',
    c4: '#cf3895', c5: '#6d358a', c6: '#06b4b0', c7: '#4b8a5f',
  });
  const count = node('gr_count', 'spread/count', { x: -249, y: 328 });
  const cycle = node('gr_cycle', 'spread/cycle', { x: -173, y: 441 });
  const randomize = node('gr_randomize', 'spread/randomize', { x: -124, y: 749 });
  const dim = node('gr_dim', 'io/slider', { x: -570, y: -7 }, { value: 14.2, min: 0, max: 100 });
  const size = node('gr_size', 'io/slider', { x: 925, y: -229 }, { value: 25.7, min: 0, max: 100 });
  const delay = node('gr_delay', 'value/number', { x: -106, y: -35 }, { value: 12.66 });

  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 129, y: 50 },
    data: {
      defType: 'region/foreach',
      params: { width: 1656, height: 692, label: 'ForEach' },
      ports: [
        { id: 'pos', name: 'pos', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
        { id: 'col', name: 'col', type: 'color', direction: 'input' },
      ],
    },
    style: { width: 1656, height: 692 },
  };

  // ── Inside the region ──
  const mag = node('gr_mag', 'vec2/mag', { x: 102, y: 132 }, {}, RID);
  const phaseMul = node('gr_phaseMul', 'math/mul', { x: 293, y: 179 }, { slotCount: 2, b: 0.01 }, RID);
  const time = node('gr_time', 'time/seconds', { x: 114, y: 475 }, {}, RID);
  const phaseAdd = node('gr_phaseAdd', 'math/add', { x: 481, y: 274 }, { slotCount: 2 }, RID);
  const sin = node('gr_sin', 'math/sin', { x: 672, y: 344 }, {}, RID);
  const amp = node('gr_amp', 'math/mul', { x: 822, y: 342 }, { slotCount: 2, b: 22.62 }, RID);
  const offset = node('gr_offset', 'vec2/make', { x: 1041, y: 228 }, {}, RID);
  const pos = node('gr_pos', 'vec2/add', { x: 1246, y: 76 }, {}, RID);
  const stroke = node('gr_stroke', 'style/stroke', { x: 1196, y: 470 }, {}, RID);
  const rect = node('gr_rect', 'draw/rect', { x: 1481, y: 76 }, {}, RID);

  return {
    nodes: [
      renderer, grid, palette, count, cycle, randomize, dim, size, delay,
      region,
      mag, phaseMul, time, phaseAdd, sin, amp, offset, pos, stroke, rect,
    ],
    edges: [
      // Grid → ForEach; iterate positions
      edge('gr_grid', 'positions', RID, 'pos'),
      edge(RID, 'pos', 'gr_pos', 'a'),
      edge(RID, 'pos', 'gr_mag', 'v'),
      // Radial ripple phase: dist·scale + time → sin → amplitude
      edge('gr_mag', 'length', 'gr_phaseMul', 'a'),
      edge('gr_phaseMul', 'result', 'gr_phaseAdd', 'a'),
      edge('gr_time', 't', 'gr_phaseAdd', 'b'),
      edge('gr_phaseAdd', 'result', 'gr_sin', 'x'),
      edge('gr_sin', 'result', 'gr_amp', 'a'),
      edge('gr_amp', 'result', 'gr_offset', 'y'),
      edge('gr_offset', 'vector', 'gr_pos', 'b'),
      edge('gr_pos', 'vector', 'gr_rect', 'pos'),
      // Per-cell stroke colour: palette cycled to grid length, then shuffled
      edge('gr_palette', 'colors', 'gr_cycle', 'spread'),
      edge('gr_grid', 'positions', 'gr_count', 'spread'),
      edge('gr_count', 'count', 'gr_cycle', 'count'),
      edge('gr_cycle', 'values', 'gr_randomize', 'source'),
      edge('gr_randomize', 'values', RID, 'col'),
      edge(RID, 'col', 'gr_stroke', 'color'),
      edge('gr_stroke', 'style', 'gr_rect', 'style'),
      // Rect → collect layers → renderer
      edge('gr_rect', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'gr_renderer', 'layers'),
      // Sliders + per-iteration time delay
      edge('gr_dim', 'number', 'gr_grid', 'cols'),
      edge('gr_dim', 'number', 'gr_grid', 'rows'),
      edge('gr_size', 'number', 'gr_rect', 'w'),
      edge('gr_size', 'number', 'gr_rect', 'h'),
      edge('gr_delay', 'number', RID, 'delay'),
    ],
  };
}
