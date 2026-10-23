import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';

// ─── Sample: Smoothed Path ─────────────────────────────────────────────────
// A jagged noise polyline (grey) with a smoothed overlay (gold). The smoothing
// is a 3-point box blur done with spread/neighbor: each point averages its
// left + self + right. Showcases in-region neighbour access — broadcast the
// point spread into the second ForEach and read offsets via the `index` pin.
// Centre-origin coordinates.
export function buildSmoothedPath() {
  const R1 = 'sm_r1';
  const R2 = 'sm_r2';

  const count = node('sm_count', 'value/number', { x: -620, y: 60 }, { value: 64 });
  const range = node('sm_range', 'spread/range', { x: -470, y: 60 });

  // ── Region 1: build the jagged points ──
  const region1: FloretNode = {
    id: R1, type: 'floret', position: { x: -260, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 760, height: 520, label: 'build points' },
      ports: [
        { id: 'n', name: 'n', type: 'number', direction: 'input' },
        { id: 'pts', name: 'pts', type: 'vec2', direction: 'output' },
      ],
    },
    style: { width: 760, height: 520 },
  };
  const nfreq = node('sm_nfreq', 'math/mul', { x: -120, y: 300 }, { b: 1.3 }, R1);  // index · 1.3
  const perlin = node('sm_perlin', 'noise/perlin', { x: 40, y: 300 }, { y: 0, seed: 4 }, R1);
  const yMul = node('sm_yMul', 'math/mul', { x: 200, y: 300 }, { b: 200 }, R1);
  const yOff = node('sm_yOff', 'math/add', { x: 340, y: 300 }, { b: -100 }, R1);    // -100…100
  const xMul = node('sm_xMul', 'math/mul', { x: -120, y: 120 }, { b: 460 }, R1);    // fraction · 460
  const xOff = node('sm_xOff', 'math/add', { x: 40, y: 120 }, { b: -230 }, R1);     // -230…230
  const pos1 = node('sm_pos1', 'vec2/make', { x: 520, y: 160 }, {}, R1);

  // Root-scope passthrough: a region's output can't wire directly into another
  // region, so the points spread hops through here before region 2.
  const pass = node('sm_pass', 'spread/take', { x: 560, y: 40 }, { count: 9999 });

  // raw polyline (grey)
  const rawCol = node('sm_rawCol', 'color/hex', { x: 560, y: 420 }, { hex: '#555a66' });
  const rawStroke = node('sm_rawStroke', 'style/stroke', { x: 720, y: 420 }, { width: 1 });
  const rawPoly = node('sm_rawPoly', 'draw/polygon', { x: 900, y: 360 }, { closed: false });

  // ── Region 2: 3-point neighbour average ──
  const region2: FloretNode = {
    id: R2, type: 'floret', position: { x: 560, y: -40 },
    data: {
      defType: 'region/foreach',
      params: { width: 820, height: 560, label: 'smooth (neighbours)' },
      ports: [
        { id: 'n2', name: 'self', type: 'vec2', direction: 'input' },
        { id: 'spts', name: 'spts', type: 'vec2', direction: 'output' },
      ],
    },
    style: { width: 820, height: 560 },
  };
  const nbL = node('sm_nbL', 'spread/neighbor', { x: 720, y: 220 }, { offset: -1, edges: 'clamp' }, R2);
  const nbR = node('sm_nbR', 'spread/neighbor', { x: 720, y: 360 }, { offset: 1, edges: 'clamp' }, R2);
  const sum = node('sm_sum', 'math/add', { x: 920, y: 160 }, { slotCount: 3, valueType: 'vec2' }, R2);
  const avg = node('sm_avg', 'vec2/scale', { x: 1100, y: 160 }, { s: 0.3333333 }, R2);

  // smoothed polyline (gold)
  const smCol = node('sm_smCol', 'color/hex', { x: 1120, y: 380 }, { hex: '#ffd166' });
  const smStroke = node('sm_smStroke', 'style/stroke', { x: 1280, y: 380 }, { width: 3 });
  const smPoly = node('sm_smPoly', 'draw/polygon', { x: 1460, y: 320 }, { closed: false });

  const group = node('sm_group', 'transform/group', { x: 1680, y: 200 }, { slotCount: 2 });
  const renderer = node('sm_renderer', 'render/renderer', { x: 1880, y: 120 }, { width: 500, height: 500 });

  return {
    nodes: [
      count, range, region1, nfreq, perlin, yMul, yOff, xMul, xOff, pos1,
      pass, rawCol, rawStroke, rawPoly,
      region2, nbL, nbR, sum, avg,
      smCol, smStroke, smPoly,
      group, renderer,
    ],
    edges: [
      edge('sm_count', 'number', 'sm_range', 'count'),
      edge('sm_range', 'indices', R1, 'n'),       // drives N
      // build points: x from fraction, y from perlin(index)
      edge(R1, '__loopvar_index', 'sm_nfreq', 'a'),
      edge('sm_nfreq', 'result', 'sm_perlin', 'x'),
      edge('sm_perlin', 'noise', 'sm_yMul', 'a'),
      edge('sm_yMul', 'result', 'sm_yOff', 'a'),
      edge(R1, '__loopvar_fraction', 'sm_xMul', 'a'),
      edge('sm_xMul', 'result', 'sm_xOff', 'a'),
      edge('sm_xOff', 'result', 'sm_pos1', 'x'),
      edge('sm_yOff', 'result', 'sm_pos1', 'y'),
      edge('sm_pos1', 'vector', R1, 'pts'),
      // raw polyline
      edge('sm_rawCol', 'color', 'sm_rawStroke', 'color'),
      edge(R1, 'pts', 'sm_pass', 'spread'),
      edge(R1, 'pts', 'sm_rawPoly', 'points'),
      edge('sm_rawStroke', 'style', 'sm_rawPoly', 'style'),
      // smooth: self via splicer, neighbours via broadcast spread + index pin
      edge('sm_pass', 'values', R2, 'n2'),        // per-point self + drives N
      edge('sm_pass', 'values', 'sm_nbL', 'spread'), // broadcast whole spread in
      edge('sm_pass', 'values', 'sm_nbR', 'spread'),
      edge(R2, '__loopvar_index', 'sm_nbL', 'index'),
      edge(R2, '__loopvar_index', 'sm_nbR', 'index'),
      edge('sm_nbL', 'value', 'sm_sum', 'a'),
      edge(R2, 'n2', 'sm_sum', 'b'),     // input port id 'n2' → per-iteration self
      edge('sm_nbR', 'value', 'sm_sum', 'c'),
      edge('sm_sum', 'result', 'sm_avg', 'v'),
      edge('sm_avg', 'scaled', R2, 'spts'),
      // smoothed polyline
      edge('sm_smCol', 'color', 'sm_smStroke', 'color'),
      edge(R2, 'spts', 'sm_smPoly', 'points'),
      edge('sm_smStroke', 'style', 'sm_smPoly', 'style'),
      // collect
      edge('sm_rawPoly', 'shape', 'sm_group', 's1'),
      edge('sm_smPoly', 'shape', 'sm_group', 's2'),
      edge('sm_group', 'grouped', 'sm_renderer', 'layers'),
    ],
  };
}
