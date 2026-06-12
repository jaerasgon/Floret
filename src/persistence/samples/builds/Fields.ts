import type { FloretNode } from '../../../editor/editor-state';
import { node, nodeWithPorts, edge } from '../helpers';


// ─── Sample: Ripple Field ──────────────────────────────────────────────────
// A 25×25 grid of dots. Each dot's size and hue are modulated by
// sin(dist_from_center · 0.04 − time · 3) — concentric ripples radiating
// from the canvas centre.
export function buildRippleField() {
  const time = node('rf_time', 'time/seconds', { x: -420, y: 20 });
  const grid = nodeWithPorts('rf_grid', 'spread/grid', { x: -420, y: 100 }, { cols: 25, rows: 25, w: 480, h: 480 });
  const cx = node('rf_cx', 'value/number', { x: -420, y: 220 }, { value: 0 });
  const cy = node('rf_cy', 'value/number', { x: -420, y: 280 }, { value: 0 });
  const distScale = node('rf_distScale', 'value/number', { x: -420, y: 340 }, { value: 0.05 });
  const timeScale = node('rf_timeScale', 'value/number', { x: -420, y: 400 }, { value: 3 });

  const RID = 'rf_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -140, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 900, height: 560, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 900, height: 560 },
  };

  // Inside: split point → x,y. dist = dist((x,y), (cx,cy)). ripple = sin(dist*0.04 - time*3)
  const split = node('rf_split', 'vec2/split', { x: 60, y: 60 }, {}, RID);
  // Offset grid (which starts at 0..480) by 10 to center it on 500-canvas
  const xOff = node('rf_xOff', 'math/add', { x: 220, y: 30 }, { b: 10 }, RID);
  const yOff = node('rf_yOff', 'math/add', { x: 220, y: 90 }, { b: 10 }, RID);
  const dist = node('rf_dist', 'math/dist', { x: 380, y: 60 }, {}, RID);
  const dScaled = node('rf_dScaled', 'math/mul', { x: 540, y: 30 }, {}, RID);
  const tScaled = node('rf_tScaled', 'math/mul', { x: 540, y: 90 }, {}, RID);
  const wavePhase = node('rf_wavePhase', 'math/sub', { x: 700, y: 60 }, {}, RID);
  const ripple = node('rf_ripple', 'math/sin', { x: 60, y: 200 }, {}, RID); // sin(wave) → -1..1
  // size = 3 + ripple * 6 → -3..9 (negative becomes zero ellipse, fine)
  const sizeBase = node('rf_sizeBase', 'value/number', { x: 220, y: 220 }, { value: 6 });
  const sizeMul = node('rf_sizeMul', 'math/mul', { x: 220, y: 160 }, { b: 6 }, RID);
  const sizeAdd = node('rf_sizeAdd', 'math/add', { x: 380, y: 200 }, {}, RID);
  // Hue = ripple * 60 + 200 (blue → cyan oscillation)
  const hueMul = node('rf_hueMul', 'math/mul', { x: 220, y: 300 }, { b: 60 }, RID);
  const hueBase = node('rf_hueBase', 'value/number', { x: 220, y: 360 }, { value: 200 });
  const hueAdd = node('rf_hueAdd', 'math/add', { x: 380, y: 300 }, {}, RID);
  const hsl = node('rf_hsl', 'color/hsl', { x: 540, y: 300 }, { s: 80, l: 60, a: 255 }, RID);
  const fill = node('rf_fill', 'style/fill', { x: 700, y: 300 }, { mode: 'color' }, RID);
  const ellipse = node('rf_ellipse', 'draw/ellipse', { x: 860, y: 100 }, {}, RID);

  const renderer = node('rf_renderer', 'render/renderer', { x: 940, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, grid, cx, cy, distScale, timeScale,
      region,
      split, xOff, yOff, dist, dScaled, tScaled, wavePhase, ripple,
      sizeBase, sizeMul, sizeAdd,
      hueMul, hueBase, hueAdd, hsl, fill, ellipse,
      renderer,
    ],
    edges: [
      edge('rf_grid', 'out', RID, 'p'),
      // split + offset
      edge(RID, 'p', 'rf_split', 'v'),
      edge('rf_split', 'x', 'rf_xOff', 'a'),
      edge('rf_split', 'y', 'rf_yOff', 'a'),
      // dist((x,y), (250,250))
      edge('rf_xOff', 'out', 'rf_dist', 'x1'),
      edge('rf_yOff', 'out', 'rf_dist', 'y1'),
      edge('rf_cx', 'out', 'rf_dist', 'x2'),
      edge('rf_cy', 'out', 'rf_dist', 'y2'),
      // wavePhase = dist*scale - time*tScale
      edge('rf_dist', 'out', 'rf_dScaled', 'a'),
      edge('rf_distScale', 'out', 'rf_dScaled', 'b'),
      edge('rf_time', 't', 'rf_tScaled', 'a'),
      edge('rf_timeScale', 'out', 'rf_tScaled', 'b'),
      edge('rf_dScaled', 'out', 'rf_wavePhase', 'a'),
      edge('rf_tScaled', 'out', 'rf_wavePhase', 'b'),
      edge('rf_wavePhase', 'out', 'rf_ripple', 'x'),
      // size = sizeBase + ripple*6
      edge('rf_ripple', 'out', 'rf_sizeMul', 'a'),
      edge('rf_sizeBase', 'out', 'rf_sizeAdd', 'a'),
      edge('rf_sizeMul', 'out', 'rf_sizeAdd', 'b'),
      // hue = ripple*60 + 200
      edge('rf_ripple', 'out', 'rf_hueMul', 'a'),
      edge('rf_hueBase', 'out', 'rf_hueAdd', 'a'),
      edge('rf_hueMul', 'out', 'rf_hueAdd', 'b'),
      edge('rf_hueAdd', 'out', 'rf_hsl', 'h'),
      edge('rf_hsl', 'out', 'rf_fill', 'color'),
      // Ellipse
      edge('rf_xOff', 'out', 'rf_ellipse', 'x'),
      edge('rf_yOff', 'out', 'rf_ellipse', 'y'),
      edge('rf_sizeAdd', 'out', 'rf_ellipse', 'w'),
      edge('rf_sizeAdd', 'out', 'rf_ellipse', 'h'),
      edge('rf_fill', 'out', 'rf_ellipse', 'style'),
      edge('rf_ellipse', 'out', RID, 'layer'),
      edge(RID, 'layer', 'rf_renderer', 'layers'),
    ],
  };
}



// ─── Sample: Noise Wave ────────────────────────────────────────────────────
// A grid of colored squares whose positions and hues are bent by aggressive
// Perlin noise at multiple octaves. Cells slide off their natural lattice
// positions; bright RGB-glitch colours emerge where the noise spikes.
// Pure function of time, no state.
export function buildNoiseWave() {
  const time = node('dm_time', 'time/seconds', { x: -480, y: 20 });
  const grid = node('dm_grid', 'spread/grid', { x: -480, y: 100 }, { cols: 32, rows: 32, w: 500, h: 500 });
  const noiseAmt = node('dm_noiseAmt', 'io/slider', { x: -480, y: 220 }, { value: 24, min: 0, max: 60 });
  const noiseScale = node('dm_noiseScale', 'value/number', { x: -480, y: 300 }, { value: 0.018 });

  const RID = 'dm_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 200, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 1040, height: 540, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1040, height: 540 },
  };

  // Inside: split point, sample two perlins for displacement + hue.
  const split = node('dm_split', 'vec2/split', { x: 60, y: 60 }, {}, RID);
  const xScaled = node('dm_xScaled', 'math/mul', { x: 240, y: 40 }, {}, RID);
  const yScaled = node('dm_yScaled', 'math/mul', { x: 240, y: 100 }, {}, RID);
  const yPlusT = node('dm_yPlusT', 'math/add', { x: 420, y: 100 }, {}, RID);
  // Displacement noise — high octaves for a jittery look.
  const perlinD = node('dm_perlinD', 'noise/perlin', { x: 600, y: 60 }, { octaves: 3, falloff: 0.55 }, RID);
  // Hue noise — separate sample with offset for decorrelation.
  const yPlus100 = node('dm_yPlus100', 'math/add', { x: 240, y: 160 }, { b: 100 }, RID);
  const yHueT = node('dm_yHueT', 'math/add', { x: 420, y: 160 }, {}, RID);
  const perlinH = node('dm_perlinH', 'noise/perlin', { x: 600, y: 160 }, { octaves: 1 }, RID);
  // Displacement: (perlinD - 0.5) * amt
  const dispShift = node('dm_dispShift', 'math/sub', { x: 60, y: 240 }, { b: 0.5 }, RID);
  const dispScale = node('dm_dispScale', 'math/mul', { x: 240, y: 240 }, {}, RID);
  // Apply displacement to x and y both
  const xFin = node('dm_xFin', 'math/add', { x: 420, y: 240 }, {}, RID);
  const yFin = node('dm_yFin', 'math/add', { x: 420, y: 300 }, {}, RID);
  // Hue = perlinH * 360
  const hueScale = node('dm_hueScale', 'math/mul', { x: 60, y: 360 }, { b: 360 }, RID);
  const hsl = node('dm_hsl', 'color/hsl', { x: 240, y: 360 }, { s: 85, l: 60, a: 255 }, RID);
  const fill = node('dm_fill', 'style/fill', { x: 420, y: 360 }, { mode: 'color' }, RID);
  const cellSize = node('dm_cellSize', 'value/number', { x: 60, y: 440 }, { value: 11 });
  const rect = node('dm_rect', 'draw/rect', { x: 800, y: 240 }, {}, RID);

  const bg = node('dm_bg', 'color/rgb', { x: -480, y: 380 }, { r: 8, g: 8, b: 14, a: 255 });
  const renderer = node('dm_renderer', 'render/renderer', { x: 1280, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, grid, noiseAmt, noiseScale,
      region,
      split, xScaled, yScaled, yPlusT, perlinD,
      yPlus100, yHueT, perlinH,
      dispShift, dispScale, xFin, yFin,
      hueScale, hsl, fill, cellSize, rect,
      bg, renderer,
    ],
    edges: [
      edge('dm_bg', 'out', 'dm_renderer', 'background'),
      edge('dm_grid', 'out', RID, 'p'),
      edge(RID, 'p', 'dm_split', 'v'),
      // Scale coords for Perlin
      edge('dm_split', 'x', 'dm_xScaled', 'a'),
      edge('dm_noiseScale', 'out', 'dm_xScaled', 'b'),
      edge('dm_split', 'y', 'dm_yScaled', 'a'),
      edge('dm_noiseScale', 'out', 'dm_yScaled', 'b'),
      edge('dm_yScaled', 'out', 'dm_yPlusT', 'a'),
      edge('dm_time', 't', 'dm_yPlusT', 'b'),
      edge('dm_xScaled', 'out', 'dm_perlinD', 'x'),
      edge('dm_yPlusT', 'out', 'dm_perlinD', 'y'),
      // Hue Perlin — same x, offset y
      edge('dm_yScaled', 'out', 'dm_yPlus100', 'a'),
      edge('dm_yPlus100', 'out', 'dm_yHueT', 'a'),
      edge('dm_time', 't', 'dm_yHueT', 'b'),
      edge('dm_xScaled', 'out', 'dm_perlinH', 'x'),
      edge('dm_yHueT', 'out', 'dm_perlinH', 'y'),
      // Displacement
      edge('dm_perlinD', 'out', 'dm_dispShift', 'a'),
      edge('dm_dispShift', 'out', 'dm_dispScale', 'a'),
      edge('dm_noiseAmt', 'out', 'dm_dispScale', 'b'),
      edge('dm_split', 'x', 'dm_xFin', 'a'),
      edge('dm_dispScale', 'out', 'dm_xFin', 'b'),
      edge('dm_split', 'y', 'dm_yFin', 'a'),
      edge('dm_dispScale', 'out', 'dm_yFin', 'b'),
      // Hue / fill
      edge('dm_perlinH', 'out', 'dm_hueScale', 'a'),
      edge('dm_hueScale', 'out', 'dm_hsl', 'h'),
      edge('dm_hsl', 'out', 'dm_fill', 'color'),
      // Rect
      edge('dm_xFin', 'out', 'dm_rect', 'x'),
      edge('dm_yFin', 'out', 'dm_rect', 'y'),
      edge('dm_cellSize', 'out', 'dm_rect', 'w'),
      edge('dm_cellSize', 'out', 'dm_rect', 'h'),
      edge('dm_fill', 'out', 'dm_rect', 'style'),
      edge('dm_rect', 'out', RID, 'layer'),
      edge(RID, 'layer', 'dm_renderer', 'layers'),
    ],
  };
}


// ─── Sample: Force Field Arrows ────────────────────────────────────────────
// A 14×14 grid of arrows pointing along a Perlin-noise angle field that
// rotates slowly with time. Each arrow is drawn as a line — its end
// point is computed by `vec2/from-angle`.
export function buildForceField() {
  const time = node('ff_time', 'time/seconds', { x: -480, y: 20 });
  const grid = node('ff_grid', 'spread/grid', { x: -480, y: 100 }, { cols: 14, rows: 14, w: 460, h: 460 });
  const noiseScale = node('ff_noiseScale', 'io/slider', { x: -480, y: 220 }, { value: 0.012, min: 0.002, max: 0.04 });
  const arrowLen = node('ff_arrowLen', 'io/slider', { x: -480, y: 300 }, { value: 30, min: 6, max: 40 });
  const timeScale = node('ff_timeScale', 'value/number', { x: -480, y: 380 }, { value: 0.4 });
  const sw = node('ff_sw', 'style/stroke-weight', { x: -260, y: 380 }, { w: 2 });

  const RID = 'ff_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 200, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 980, height: 540, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 980, height: 540 },
  };

  const split = node('ff_split', 'vec2/split', { x: 60, y: 60 }, {}, RID);
  const xOff = node('ff_xOff', 'math/add', { x: 60, y: 380 }, { b: 20 }, RID);
  const yOff = node('ff_yOff', 'math/add', { x: 60, y: 440 }, { b: 20 }, RID);
  const xScaled = node('ff_xScaled', 'math/mul', { x: 240, y: 60 }, {}, RID);
  const yScaled = node('ff_yScaled', 'math/mul', { x: 240, y: 130 }, {}, RID);
  const tScaled = node('ff_tScaled', 'math/mul', { x: 240, y: 200 }, {}, RID);
  const ySum = node('ff_ySum', 'math/add', { x: 420, y: 130 }, {}, RID);
  const perlin = node('ff_perlin', 'noise/perlin', { x: 600, y: 80 }, { octaves: 2 }, RID);
  const angleMul = node('ff_angleMul', 'math/mul', { x: 60, y: 260 }, { b: 6.2831853 }, RID);
  const tipVec = node('ff_tipVec', 'vec2/from-angle', { x: 240, y: 260 }, {}, RID);
  const tipSplit = node('ff_tipSplit', 'vec2/split', { x: 420, y: 260 }, {}, RID);
  const tipX = node('ff_tipX', 'math/add', { x: 600, y: 260 }, {}, RID);
  const tipY = node('ff_tipY', 'math/add', { x: 600, y: 320 }, {}, RID);
  const hueMul = node('ff_hueMul', 'math/mul', { x: 60, y: 200 }, { b: 360 }, RID);
  const hsl = node('ff_hsl', 'color/hsl', { x: 240, y: 340 }, { s: 75, l: 60, a: 255 }, RID);
  const fill = node('ff_fill', 'style/fill', { x: 420, y: 340 }, { mode: 'color' }, RID);
  // Stroke style chain: NoFill → StrokeWeight → Stroke(hsl-color) → line.
  // p5 lines are stroke-driven; without a stroke colour they'd be invisible
  // against the dark backdrop.
  const noFill = node('ff_noFill', 'style/no-fill', { x: 600, y: 340 }, {}, RID);
  const strokeC = node('ff_strokeC', 'style/stroke', { x: 780, y: 340 }, {}, RID);
  const strokeW = node('ff_strokeW', 'style/stroke-weight', { x: 960, y: 340 }, { w: 2.5 }, RID);
  const line = node('ff_line', 'draw/line', { x: 780, y: 80 }, {}, RID);
  const tipDot = node('ff_tipDot', 'draw/ellipse', { x: 780, y: 260 }, { w: 5, h: 5 }, RID);
  // Combine line + tipDot into a single per-iteration layer via Stack + Group.
  // Each input wire to Stack auto-wraps the single layer into a length-1
  // spread; Stack concatenates them; Group flattens the spread to one Layer.
  const pair = node('ff_pair', 'spread/stack', { x: 980, y: 160 }, {}, RID);
  const group = node('ff_group', 'transform/group', { x: 1160, y: 160 }, {}, RID);

  const bg = node('ff_bg', 'color/rgb', { x: -480, y: 460 }, { r: 18, g: 20, b: 30, a: 255 });
  const renderer = node('ff_renderer', 'render/renderer', { x: 1240, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, grid, noiseScale, arrowLen, timeScale, sw,
      region,
      split, xOff, yOff, xScaled, yScaled, tScaled, ySum, perlin,
      angleMul, tipVec, tipSplit, tipX, tipY,
      hueMul, hsl, fill, noFill, strokeC, strokeW, line, tipDot, pair, group,
      bg, renderer,
    ],
    edges: [
      edge('ff_bg', 'out', 'ff_renderer', 'background'),
      edge('ff_grid', 'out', RID, 'p'),
      edge(RID, 'p', 'ff_split', 'v'),
      edge('ff_split', 'x', 'ff_xOff', 'a'),
      edge('ff_split', 'y', 'ff_yOff', 'a'),
      edge('ff_xOff', 'out', 'ff_xScaled', 'a'),
      edge('ff_noiseScale', 'out', 'ff_xScaled', 'b'),
      edge('ff_yOff', 'out', 'ff_yScaled', 'a'),
      edge('ff_noiseScale', 'out', 'ff_yScaled', 'b'),
      edge('ff_time', 't', 'ff_tScaled', 'a'),
      edge('ff_timeScale', 'out', 'ff_tScaled', 'b'),
      edge('ff_yScaled', 'out', 'ff_ySum', 'a'),
      edge('ff_tScaled', 'out', 'ff_ySum', 'b'),
      edge('ff_xScaled', 'out', 'ff_perlin', 'x'),
      edge('ff_ySum', 'out', 'ff_perlin', 'y'),
      edge('ff_perlin', 'out', 'ff_angleMul', 'a'),
      edge('ff_angleMul', 'out', 'ff_tipVec', 'angle'),
      edge('ff_arrowLen', 'out', 'ff_tipVec', 'mag'),
      edge('ff_tipVec', 'out', 'ff_tipSplit', 'v'),
      edge('ff_xOff', 'out', 'ff_tipX', 'a'),
      edge('ff_tipSplit', 'x', 'ff_tipX', 'b'),
      edge('ff_yOff', 'out', 'ff_tipY', 'a'),
      edge('ff_tipSplit', 'y', 'ff_tipY', 'b'),
      // Hue from perlin → drives both fill (tip dot) and stroke (line)
      edge('ff_perlin', 'out', 'ff_hueMul', 'a'),
      edge('ff_hueMul', 'out', 'ff_hsl', 'h'),
      edge('ff_hsl', 'out', 'ff_fill', 'color'),
      // Stroke style chain: NoFill → Stroke → StrokeWeight → line.style
      edge('ff_hsl', 'out', 'ff_strokeC', 'color'),
      edge('ff_noFill', 'out', 'ff_strokeC', 'base'),
      edge('ff_strokeC', 'out', 'ff_strokeW', 'base'),
      // Line from (xOff, yOff) → (tipX, tipY)
      edge('ff_xOff', 'out', 'ff_line', 'x1'),
      edge('ff_yOff', 'out', 'ff_line', 'y1'),
      edge('ff_tipX', 'out', 'ff_line', 'x2'),
      edge('ff_tipY', 'out', 'ff_line', 'y2'),
      edge('ff_strokeW', 'out', 'ff_line', 'style'),
      // Tip dot (filled circle marking the arrow head)
      edge('ff_tipX', 'out', 'ff_tipDot', 'x'),
      edge('ff_tipY', 'out', 'ff_tipDot', 'y'),
      edge('ff_fill', 'out', 'ff_tipDot', 'style'),
      // Combine line + tipDot via Stack → Group → one Layer per iteration
      edge('ff_line', 'out', 'ff_pair', 'a'),
      edge('ff_tipDot', 'out', 'ff_pair', 'b'),
      edge('ff_pair', 'out', 'ff_group', 's1'),
      edge('ff_group', 'out', RID, 'layer'),
      edge(RID, 'layer', 'ff_renderer', 'layers'),
    ],
  };
}
