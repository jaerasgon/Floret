import type { FloretNode } from '../../../editor/editor-state';
import { node, nodeWithPorts, edge } from '../helpers';

export function buildLissajous() {
  const N = 240;
  const aS = node('lj_a', 'io/slider', { x: -380, y: 20 }, { value: 3, min: 1, max: 8 });
  const bS = node('lj_b', 'io/slider', { x: -380, y: 100 }, { value: 5, min: 1, max: 8 });
  const time = node('lj_time', 'time/seconds', { x: -380, y: 180 });
  const range = nodeWithPorts('lj_range', 'spread/range', { x: -380, y: 260 }, { count: N });
  // tStep = TAU / N
  const tStep = node('lj_tStep', 'value/number', { x: -380, y: 320 }, { value: (Math.PI * 2) / N });
  // R = 200, cx = cy = 250
  const radius = node('lj_radius', 'value/number', { x: -380, y: 380 }, { value: 200 });
  const cx = node('lj_cx', 'value/number', { x: -380, y: 440 }, { value: 0 });
  const cy = node('lj_cy', 'value/number', { x: -380, y: 500 }, { value: 0 });

  const RID = 'lj_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -120, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 880, height: 560, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'p', name: 'p', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 880, height: 560 },
  };

  // Inside: t = i * tStep
  const tMul = node('lj_tMul', 'math/mul', { x: 40, y: 60 }, {}, RID);
  // angleX = t*a + time, angleY = t*b + time
  const axMul = node('lj_axMul', 'math/mul', { x: 220, y: 40 }, {}, RID);
  const ayMul = node('lj_ayMul', 'math/mul', { x: 220, y: 110 }, {}, RID);
  const axAdd = node('lj_axAdd', 'math/add', { x: 380, y: 40 }, {}, RID);
  const ayAdd = node('lj_ayAdd', 'math/add', { x: 380, y: 110 }, {}, RID);
  // x = cos(angleX)*R + cx, y = sin(angleY)*R + cy
  const cosX = node('lj_cosX', 'math/cos', { x: 540, y: 40 }, {}, RID);
  const sinY = node('lj_sinY', 'math/sin', { x: 540, y: 110 }, {}, RID);
  const rX = node('lj_rX', 'math/mul', { x: 540, y: 170 }, {}, RID);
  const rY = node('lj_rY', 'math/mul', { x: 540, y: 230 }, {}, RID);
  const xFin = node('lj_xFin', 'math/add', { x: 700, y: 40 }, {}, RID);
  const yFin = node('lj_yFin', 'math/add', { x: 700, y: 110 }, {}, RID);
  // Hue = i / N * 360
  const hueMul = node('lj_hueMul', 'math/mul', { x: 40, y: 280 }, { b: 360 / N }, RID);
  const hsl = node('lj_hsl', 'color/hsl', { x: 220, y: 280 }, { s: 75, l: 60, a: 255 }, RID);
  const fill = node('lj_fill', 'style/fill', { x: 400, y: 280 }, { mode: 'color' }, RID);
  const dotSize = node('lj_dotSize', 'value/number', { x: 540, y: 280 }, { value: 4 });
  const dot = node('lj_dot', 'draw/ellipse', { x: 700, y: 220 }, {}, RID);

  const renderer = node('lj_renderer', 'render/renderer', { x: 800, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      aS, bS, time, range, tStep, radius, cx, cy,
      region,
      tMul, axMul, ayMul, axAdd, ayAdd,
      cosX, sinY, rX, rY, xFin, yFin,
      hueMul, hsl, fill, dotSize, dot,
      renderer,
    ],
    edges: [
      edge('lj_range', 'indices', RID, 'i'),
      // t = i * tStep
      edge(RID, 'i', 'lj_tMul', 'a'),
      edge('lj_tStep', 'number', 'lj_tMul', 'b'),
      // angleX = t*a + time
      edge('lj_tMul', 'product', 'lj_axMul', 'a'),
      edge('lj_a', 'number', 'lj_axMul', 'b'),
      edge('lj_axMul', 'product', 'lj_axAdd', 'a'),
      edge('lj_time', 't', 'lj_axAdd', 'b'),
      // angleY = t*b + time
      edge('lj_tMul', 'product', 'lj_ayMul', 'a'),
      edge('lj_b', 'number', 'lj_ayMul', 'b'),
      edge('lj_ayMul', 'product', 'lj_ayAdd', 'a'),
      edge('lj_time', 't', 'lj_ayAdd', 'b'),
      // cos/sin then scale by R then add center
      edge('lj_axAdd', 'sum', 'lj_cosX', 'x'),
      edge('lj_ayAdd', 'sum', 'lj_sinY', 'x'),
      edge('lj_cosX', 'cosine', 'lj_rX', 'a'),
      edge('lj_radius', 'number', 'lj_rX', 'b'),
      edge('lj_sinY', 'sine', 'lj_rY', 'a'),
      edge('lj_radius', 'number', 'lj_rY', 'b'),
      edge('lj_rX', 'product', 'lj_xFin', 'a'),
      edge('lj_cx', 'number', 'lj_xFin', 'b'),
      edge('lj_rY', 'product', 'lj_yFin', 'a'),
      edge('lj_cy', 'number', 'lj_yFin', 'b'),
      // Hue + fill
      edge(RID, 'i', 'lj_hueMul', 'a'),
      edge('lj_hueMul', 'product', 'lj_hsl', 'h'),
      edge('lj_hsl', 'color', 'lj_fill', 'color'),
      // Dot
      edge('lj_xFin', 'sum', 'lj_dot', 'x'),
      edge('lj_yFin', 'sum', 'lj_dot', 'y'),
      edge('lj_dotSize', 'number', 'lj_dot', 'w'),
      edge('lj_dotSize', 'number', 'lj_dot', 'h'),
      edge('lj_fill', 'style', 'lj_dot', 'style'),
      edge('lj_dot', 'shape', RID, 'p'),
      edge(RID, 'p', 'lj_renderer', 'layers'),
    ],
  };
}


// ─── Sample: Kaleidoscope ──────────────────────────────────────────────────
// A base triangle is offset by the mouse position from center, then a ForEach
// rotates it through 12 angles around the canvas centre. Time cycles the hue.
export function buildKaleidoscope() {
  // True image-kaleidoscope: a small wedge of an image is drawn 12 times
  // around the centre, with alternating mirror-flip so adjacent slices
  // reflect into each other (the optical-tube illusion). Time slowly
  // moves the source crop window across the image so the pattern
  // morphs without any state.
  const N = 12;
  const url = node('ks_url', 'io/image-url', { x: -560, y: 20 }, {
    url: 'https://picsum.photos/seed/floret-kaleido/500/500',
  });
  const time = node('ks_time', 'time/seconds', { x: -560, y: 100 });

  // sx = 200 + sin(t*0.3)*120 ; sy = 200 + cos(t*0.3)*120. The 500-px
  // picsum image leaves room for the 200-px sample window to wander
  // around the middle. The values are tuned to keep the wedge inside.
  const tScale = node('ks_tScale', 'math/mul', { x: -360, y: 100 }, { b: 0.3 });
  const sinT = node('ks_sinT', 'math/sin', { x: -180, y: 80 });
  const cosT = node('ks_cosT', 'math/cos', { x: -180, y: 140 });
  const wOff = node('ks_wOff', 'math/mul', { x: 0, y: 80 }, { b: 120 });
  const hOff = node('ks_hOff', 'math/mul', { x: 0, y: 140 }, { b: 120 });
  const sxAnim = node('ks_sxAnim', 'math/add', { x: 180, y: 80 }, { b: 200 });
  const syAnim = node('ks_syAnim', 'math/add', { x: 180, y: 140 }, { b: 200 });

  // The image wedge: 200×200 region drawn off-centre so rotation around
  // the origin sweeps it through every angle. Each wedge's near corner
  // touches (30, -100) — close to the origin but not on top of it, so
  // the slices meet near the centre without piling up.
  const imgRegion = node('ks_imgRegion', 'draw/image-region', { x: 380, y: 100 },
    { sw: 200, sh: 200, dx: 30, dy: -100, dw: 200, dh: 200 });

  const angles = nodeWithPorts('ks_angles', 'spread/ring', { x: -560, y: 220 }, { count: N });
  const idxs = nodeWithPorts('ks_idxs', 'spread/range', { x: -560, y: 280 }, { count: N });

  const RID = 'ks_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 620, y: 220 },
    data: {
      defType: 'region/foreach',
      params: { width: 700, height: 360, label: 'ForEach Wedge' },
      ports: [
        { id: 'angle', name: 'angle', type: 'number', direction: 'input' },
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'out', name: 'out', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 700, height: 360 },
  };

  // Inside: sign = 1 - 2 * (i mod 2)  → 1, -1, 1, -1, …
  // That gets fed to transform/scale.sx to flip alternate wedges.
  const iMod = node('ks_iMod', 'math/mod', { x: 60, y: 40 }, { b: 2 }, RID);
  const iMul = node('ks_iMul', 'math/mul', { x: 220, y: 40 }, { b: -2 }, RID);
  const sign = node('ks_sign', 'math/add', { x: 380, y: 40 }, { b: 1 }, RID);
  // Apply mirror then rotation. Order matters — scale first, then rotate.
  const scaled = node('ks_scaled', 'transform/scale', { x: 80, y: 160 }, { sy: 1 }, RID);
  const rotated = node('ks_rotated', 'transform/rotate', { x: 280, y: 160 }, {}, RID);

  const bg = node('ks_bg', 'color/rgb', { x: -560, y: 360 }, { r: 6, g: 6, b: 14, a: 255 });
  const renderer = node('ks_renderer', 'render/renderer', { x: 1340, y: 20 },
    { width: 500, height: 500 });

  return {
    nodes: [
      url, time, tScale, sinT, cosT, wOff, hOff, sxAnim, syAnim,
      imgRegion, angles, idxs, region,
      iMod, iMul, sign, scaled, rotated,
      bg, renderer,
    ],
    edges: [
      edge('ks_bg', 'color', 'ks_renderer', 'background'),
      // Time → sin/cos
      edge('ks_time', 't', 'ks_tScale', 'a'),
      edge('ks_tScale', 'product', 'ks_sinT', 'x'),
      edge('ks_tScale', 'product', 'ks_cosT', 'x'),
      // Offset × 120
      edge('ks_sinT', 'sine', 'ks_wOff', 'a'),
      edge('ks_cosT', 'cosine', 'ks_hOff', 'a'),
      // + 200 → centred-ish source coords
      edge('ks_wOff', 'product', 'ks_sxAnim', 'a'),
      edge('ks_hOff', 'product', 'ks_syAnim', 'a'),
      // Image region: wire image + animated sx/sy
      edge('ks_url', 'image', 'ks_imgRegion', 'image'),
      edge('ks_sxAnim', 'sum', 'ks_imgRegion', 'sx'),
      edge('ks_syAnim', 'sum', 'ks_imgRegion', 'sy'),
      // ForEach: angle + i
      edge('ks_angles', 'angles', RID, 'angle'),
      edge('ks_idxs', 'indices', RID, 'i'),
      // Sign = 1 - 2 * (i % 2)
      edge(RID, 'i', 'ks_iMod', 'a'),
      edge('ks_iMod', 'remainder', 'ks_iMul', 'a'),
      edge('ks_iMul', 'product', 'ks_sign', 'a'),
      // scale(sign, 1) the image wedge (broadcast in from outer scope)
      edge('ks_imgRegion', 'layer', 'ks_scaled', 'layer'),
      edge('ks_sign', 'sum', 'ks_scaled', 'sx'),
      // rotate(angle) the scaled wedge
      edge('ks_scaled', 'scaled', 'ks_rotated', 'layer'),
      edge(RID, 'angle', 'ks_rotated', 'angle'),
      // Out
      edge('ks_rotated', 'rotated', RID, 'out'),
      edge(RID, 'out', 'ks_renderer', 'layers'),
    ],
  };
}



// ─── Sample: Grid Art ─────────────────────────────────────────────────────
// 6×6 grid of stochastic walker-cluster cells. Each cell is independently
// seeded (seedBase slider + iteration index), so the 36 compositions all
// share the palette and motion language but vary in detail. A black-
// outlined border rect frames each cell. Port of a generative sketch by
// Sergi Calabuig — the heavy lifting (spawning walkers, stepping them
// 200× with decaying stroke weight, picking line-vs-circle drawing) all
// lives inside the draw/walker-cluster node.
export function buildGridArt() {
  const CANVAS = 720;
  const CELL = 110;
  const COLS = 6;
  const GRID_SPAN = 600;
  const N = COLS * COLS;

  const seedBase = node('ga_seedBase', 'io/slider', { x: -480, y: 20 },
                        { value: 0, min: 0, max: 999, step: 1 });
  const grid = node('ga_grid', 'spread/grid', { x: -480, y: 120 },
                    { cols: COLS, rows: COLS, w: GRID_SPAN, h: GRID_SPAN });
  const idx = node('ga_idx', 'spread/range', { x: -480, y: 220 }, { count: N });
  const bg = node('ga_bg', 'color/rgb', { x: -480, y: 320 },
                  { r: 255, g: 255, b: 255, a: 255 });
  const renderer = node('ga_renderer', 'render/renderer', { x: 1080, y: 20 },
                        { width: CANVAS, height: CANVAS });

  const RID = 'ga_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -200, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 1240, height: 460, label: 'ForEach cells' },
      ports: [
        // `p` and `i` are spread inputs — the splicer-in distributes
        // them one slot per iteration. Iteration count = min(len(p), len(i)).
        // The slider is intentionally NOT a region port: scalar inputs
        // routed through a splicer-in would collapse the iteration count
        // to 1. Instead it crosses the boundary as a direct broadcast
        // edge (slider → seedSum.b below).
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1240, height: 460 },
  };

  // ── Inside region ────────────────────────────────────────────────────
  // Grid points are already centred on origin (−300..+300). The render driver
  // applies `translate(width/2, height/2)` before drawing each layer, so
  // coordinates pass through as-is — no manual canvas-centre offset.
  const split = node('ga_split', 'vec2/split', { x: 40, y: 60 }, {}, RID);
  const seedSum = node('ga_seedSum', 'math/add', { x: 220, y: 140 }, {}, RID);

  // The walker-cluster node — falls back to its built-in 14-colour palette
  // when nothing is wired into `colors`, which is exactly what we want here.
  const walker = node('ga_walker', 'draw/walker-cluster', { x: 420, y: 20 },
                      { size: CELL, density: 1, steps: 200 }, RID);

  // Cell border: NoFill → Stroke (black, w=2) → RectMode(center) → Rect
  const noFill = node('ga_noFill', 'style/no-fill', { x: 40, y: 260 }, {}, RID);
  const stroke = node('ga_stroke', 'style/stroke', { x: 220, y: 260 },
                      { width: 2 }, RID);
  const rectMode = node('ga_rectMode', 'style/rect-mode', { x: 420, y: 260 },
                        { mode: 'center' }, RID);
  const border = node('ga_border', 'draw/rect', { x: 620, y: 260 },
                      { w: CELL, h: CELL }, RID);

  // Group walker + border (border drawn on top of walker fill).
  const group = node('ga_group', 'transform/group', { x: 860, y: 100 },
                     { slotCount: 2 }, RID);

  // Cache — rasterise the 36-cell ForEach output into one bitmap on
  // first paint, then blit every frame. With runtime memoisation in
  // place the wrapped Layer chain returns the SAME closure refs across
  // ticks, so the cache's `dirty` flag stays false and we pay one
  // `drawImage` per frame instead of replaying ~400k canvas calls.
  const cache = node('ga_cache', 'transform/cache', { x: 720, y: 80 });

  return {
    nodes: [
      seedBase, grid, idx, bg, renderer,
      region,
      split, seedSum,
      walker,
      noFill, stroke, rectMode, border,
      group,
      cache,
    ],
    edges: [
      edge('ga_bg', 'color', 'ga_renderer', 'background'),
      // External → ForEach ports (spreads — drive the 36-iteration loop).
      edge('ga_grid', 'positions', RID, 'p'),
      edge('ga_idx', 'indices', RID, 'i'),
      // Inside: split the iteration's vec2 into x, y.
      edge(RID, 'p', 'ga_split', 'v'),
      // Per-cell seed = i + seedBase. Slider re-rolls all cells together.
      // Slider edge bypasses the region boundary as a broadcast.
      edge(RID, 'i', 'ga_seedSum', 'a'),
      edge('ga_seedBase', 'number', 'ga_seedSum', 'b'),
      // Walker — coords go straight through (driver centres the origin).
      edge('ga_split', 'x', 'ga_walker', 'x'),
      edge('ga_split', 'y', 'ga_walker', 'y'),
      edge('ga_seedSum', 'sum', 'ga_walker', 'seed'),
      // Border style chain.
      edge('ga_noFill', 'style', 'ga_stroke', 'base'),
      edge('ga_stroke', 'style', 'ga_rectMode', 'base'),
      edge('ga_split', 'x', 'ga_border', 'x'),
      edge('ga_split', 'y', 'ga_border', 'y'),
      edge('ga_rectMode', 'style', 'ga_border', 'style'),
      // Group → ForEach output → Cache → Renderer. Runtime memoisation
      // keeps the upstream layer refs stable across ticks; Cache uses
      // that stability to skip rasterising and just blit the bitmap.
      edge('ga_walker', 'layer', 'ga_group', 's1'),
      edge('ga_border', 'shape', 'ga_group', 's2'),
      edge('ga_group', 'grouped', RID, 'layer'),
      edge(RID, 'layer', 'ga_cache', 'layer'),
      edge('ga_cache', 'rasterized', 'ga_renderer', 'layers'),
    ],
  };
}
