import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Showcase (first-run default) ───────────────────────────────────────────
// The patch a brand-new visitor sees. A grid of stroked squares, each offset
// by sin(dist_from_centre · scale + time), so a radial ripple travels across
// the lattice. Stroke colours cycle a palette across the cells. Two sliders
// (grid density + cell size) invite a first interaction.
//
// It exercises the load-bearing ideas in Floret at a glance: a `spread/grid`
// source, a `region/foreach` that maps a sub-graph over every cell, vec2 math,
// time-driven animation, and palette/spread colour — the things worth showing
// off. Laid out compact and anchored near the origin so the graph reads on
// load (there is no fit-to-view on first paint) while the Preview animates.
//
// Topology mirrors the proven "Grid Ripple" sample; only the layout is tuned
// for the default canvas viewport. Used raw (no spaced() rescale) so the
// hand-placed coordinates are exactly what loads.
export function buildShowcase() {
  const RID = 'sh_region';

  // ── Sources (left of the region) ──
  const dim = node('sh_dim', 'io/slider', { x: -760, y: 40 }, { value: 14, min: 2, max: 40 });
  const size = node('sh_size', 'io/slider', { x: -760, y: -120 }, { value: 26, min: 4, max: 60 });
  const delay = node('sh_delay', 'value/number', { x: -560, y: -120 }, { value: 12 });
  const grid = node('sh_grid', 'spread/grid', { x: -540, y: 40 });
  const palette = node('sh_palette', 'color/palette', { x: -760, y: 320 }, {
    stopCount: 4,
    c0: '#fed000', c1: '#fc8405', c2: '#ed361a', c3: '#4464a1',
    c4: '#cf3895', c5: '#6d358a', c6: '#06b4b0', c7: '#4b8a5f',
  });
  const count = node('sh_count', 'spread/count', { x: -540, y: 300 });
  const cycle = node('sh_cycle', 'spread/cycle', { x: -540, y: 430 });
  const randomize = node('sh_randomize', 'spread/randomize', { x: -540, y: 560 });

  // ── Region frame (relative-positioned children) ──
  const REGION_W = 1240;
  const REGION_H = 700;
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -280, y: -40 },
    data: {
      defType: 'region/foreach',
      params: { width: REGION_W, height: REGION_H, label: 'ForEach cell' },
      ports: [
        { id: 'pos', name: 'pos', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
        { id: 'col', name: 'col', type: 'color', direction: 'input' },
      ],
    },
    style: { width: REGION_W, height: REGION_H },
  };

  // ── Inside the region ──
  const mag = node('sh_mag', 'vec2/mag', { x: 40, y: 150 }, {}, RID);
  const time = node('sh_time', 'time/seconds', { x: 40, y: 440 }, {}, RID);
  const phaseMul = node('sh_phaseMul', 'math/mul', { x: 250, y: 200 }, { slotCount: 2, b: 0.01 }, RID);
  const phaseAdd = node('sh_phaseAdd', 'math/add', { x: 450, y: 300 }, { slotCount: 2 }, RID);
  const sin = node('sh_sin', 'math/sin', { x: 640, y: 360 }, {}, RID);
  const amp = node('sh_amp', 'math/mul', { x: 820, y: 360 }, { slotCount: 2, b: 22 }, RID);
  const offset = node('sh_offset', 'vec2/make', { x: 820, y: 180 }, {}, RID);
  // Adaptive add latched to vec2 (position + offset). Sample compiles bypass
  // the editor's type-latch, so set valueType explicitly.
  const pos = node('sh_pos', 'math/add', { x: 1010, y: 70 }, { valueType: 'vec2' }, RID);
  const stroke = node('sh_stroke', 'style/stroke', { x: 820, y: 520 }, {}, RID);
  const rect = node('sh_rect', 'draw/rect', { x: 1010, y: 320 }, {}, RID);

  // ── Output (right of the region) ──
  const renderer = node('sh_renderer', 'render/renderer', { x: 1060, y: -40 }, { width: 1080, height: 1920 });
  const bg = node('sh_bg', 'color/rgb', { x: 1060, y: 160 }, { r: 16, g: 14, b: 22, a: 255 });

  return {
    nodes: [
      dim, size, delay, grid, palette, count, cycle, randomize,
      region,
      mag, time, phaseMul, phaseAdd, sin, amp, offset, pos, stroke, rect,
      renderer, bg,
    ],
    edges: [
      // Grid → ForEach; iterate positions
      edge('sh_grid', 'positions', RID, 'pos'),
      edge(RID, 'pos', 'sh_pos', 'a'),
      edge(RID, 'pos', 'sh_mag', 'v'),
      // Radial ripple phase: dist·scale + time → sin → amplitude
      edge('sh_mag', 'length', 'sh_phaseMul', 'a'),
      edge('sh_phaseMul', 'result', 'sh_phaseAdd', 'a'),
      edge('sh_time', 't', 'sh_phaseAdd', 'b'),
      edge('sh_phaseAdd', 'result', 'sh_sin', 'x'),
      edge('sh_sin', 'result', 'sh_amp', 'a'),
      edge('sh_amp', 'result', 'sh_offset', 'y'),
      edge('sh_offset', 'vector', 'sh_pos', 'b'),
      edge('sh_pos', 'result', 'sh_rect', 'pos'),
      // Per-cell stroke colour: palette cycled to grid length, then shuffled
      edge('sh_palette', 'colors', 'sh_cycle', 'spread'),
      edge('sh_grid', 'positions', 'sh_count', 'spread'),
      edge('sh_count', 'count', 'sh_cycle', 'count'),
      edge('sh_cycle', 'values', 'sh_randomize', 'source'),
      edge('sh_randomize', 'values', RID, 'col'),
      edge(RID, 'col', 'sh_stroke', 'color'),
      edge('sh_stroke', 'style', 'sh_rect', 'style'),
      // Rect → collect layers → renderer
      edge('sh_rect', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'sh_renderer', 'layers'),
      edge('sh_bg', 'color', 'sh_renderer', 'background'),
      // Sliders + per-iteration time delay
      edge('sh_dim', 'number', 'sh_grid', 'cols'),
      edge('sh_dim', 'number', 'sh_grid', 'rows'),
      edge('sh_size', 'number', 'sh_rect', 'w'),
      edge('sh_size', 'number', 'sh_rect', 'h'),
      edge('sh_delay', 'number', RID, 'delay'),
    ],
  };
}
