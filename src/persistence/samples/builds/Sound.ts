import type { FloretNode } from '../../../editor/editor-state';
import { node, nodeWithPorts, edge } from '../helpers';



// ─── Sample: Tone Surface ──────────────────────────────────────────────────
// Mouse X picks pitch (110..880 Hz), mouse Y picks amp (0..0.4). A 20×20
// background grid colours each dot by distance to the mouse, so the field
// "lights up" around the cursor. Click → audio resumes (browser autoplay).
export function buildToneSurface() {
  const mouse = node('ts_mouse', 'input/mouse', { x: -420, y: 20 });
  // freq = map(mx, 0, 500, 110, 880)
  const freqMap = node('ts_freqMap', 'math/map', { x: -200, y: 20 }, { inMin: -250, inMax: 250, outMin: 110, outMax: 880 });
  // amp = map(my, 0, 500, 0.4, 0)  (top of canvas = loud)
  const ampMap = node('ts_ampMap', 'math/map', { x: -200, y: 100 }, { inMin: -250, inMax: 250, outMin: 0.4, outMax: 0 });
  const osc = node('ts_osc', 'sound/oscillator', { x: 20, y: 20 }, { type: 'sine' });
  const out = node('ts_out', 'sound/output', { x: 240, y: 20 });
  // Grid backdrop
  const grid = nodeWithPorts('ts_grid', 'spread/grid', { x: -420, y: 200 }, { cols: 20, rows: 20, w: 480, h: 480 });

  const RID = 'ts_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -160, y: 200 },
    data: {
      defType: 'region/foreach',
      params: { width: 900, height: 460, label: 'ForEach' },
      // Only the grid spread iterates. mouse.x and mouse.y are
      // broadcast directly to the inner dist node — wiring them
      // through region ports would wrap them as length-1 spreads
      // and limit the ForEach to a single iteration.
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 900, height: 460 },
  };

  const split = node('ts_split', 'vec2/split', { x: 60, y: 60 }, {}, RID);
  const xOff = node('ts_xOff', 'math/add', { x: 220, y: 20 }, { b: 10 }, RID);
  const yOff = node('ts_yOff', 'math/add', { x: 220, y: 90 }, { b: 10 }, RID);
  const dist = node('ts_dist', 'math/dist', { x: 380, y: 60 }, {}, RID);
  // hue = 220 - dist * 0.4
  const distScaled = node('ts_distScaled', 'math/mul', { x: 540, y: 60 }, { b: 0.4 }, RID);
  const hueBase = node('ts_hueBase', 'value/number', { x: 540, y: 120 }, { value: 220 });
  const hue = node('ts_hue', 'math/sub', { x: 700, y: 60 }, {}, RID);
  // light = 80 - dist*0.07 (closer = brighter); clamp [10, 80]
  const lScaled = node('ts_lScaled', 'math/mul', { x: 540, y: 180 }, { b: 0.07 }, RID);
  const lBase = node('ts_lBase', 'value/number', { x: 540, y: 240 }, { value: 80 });
  const lit = node('ts_lit', 'math/sub', { x: 700, y: 180 }, {}, RID);
  const lClamp = node('ts_lClamp', 'math/clamp', { x: 700, y: 240 }, { min: 10, max: 80 }, RID);
  const hsl = node('ts_hsl', 'color/hsl', { x: 860, y: 180 }, { s: 75, a: 255 }, RID);
  const fill = node('ts_fill', 'style/fill', { x: 60, y: 320 }, { mode: 'color' }, RID);
  const dotSize = node('ts_dotSize', 'value/number', { x: 60, y: 380 }, { value: 6 });
  const dot = node('ts_dot', 'draw/ellipse', { x: 240, y: 320 }, {}, RID);

  const renderer = node('ts_renderer', 'render/renderer', { x: 480, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      mouse, freqMap, ampMap, osc, out,
      grid,
      region,
      split, xOff, yOff, dist,
      distScaled, hueBase, hue,
      lScaled, lBase, lit, lClamp, hsl,
      fill, dotSize, dot,
      renderer,
    ],
    edges: [
      // Mouse → freq/amp → osc → out
      edge('ts_mouse', 'x', 'ts_freqMap', 'x'),
      edge('ts_mouse', 'y', 'ts_ampMap', 'x'),
      edge('ts_freqMap', 'mapped', 'ts_osc', 'freq'),
      edge('ts_ampMap', 'mapped', 'ts_osc', 'amp'),
      edge('ts_osc', 'audio', 'ts_out', 'in'),
      // Grid → ForEach; mouse broadcasts directly into the inner dist node.
      edge('ts_grid', 'positions', RID, 'p'),
      edge(RID, 'p', 'ts_split', 'v'),
      edge('ts_split', 'x', 'ts_xOff', 'a'),
      edge('ts_split', 'y', 'ts_yOff', 'a'),
      edge('ts_xOff', 'sum', 'ts_dist', 'x1'),
      edge('ts_yOff', 'sum', 'ts_dist', 'y1'),
      edge('ts_mouse', 'x', 'ts_dist', 'x2'),
      edge('ts_mouse', 'y', 'ts_dist', 'y2'),
      // hue
      edge('ts_dist', 'distance', 'ts_distScaled', 'a'),
      edge('ts_hueBase', 'number', 'ts_hue', 'a'),
      edge('ts_distScaled', 'product', 'ts_hue', 'b'),
      edge('ts_hue', 'difference', 'ts_hsl', 'h'),
      // lightness
      edge('ts_dist', 'distance', 'ts_lScaled', 'a'),
      edge('ts_lBase', 'number', 'ts_lit', 'a'),
      edge('ts_lScaled', 'product', 'ts_lit', 'b'),
      edge('ts_lit', 'difference', 'ts_lClamp', 'x'),
      edge('ts_lClamp', 'clamped', 'ts_hsl', 'l'),
      edge('ts_hsl', 'color', 'ts_fill', 'color'),
      // Dot
      edge('ts_xOff', 'sum', 'ts_dot', 'x'),
      edge('ts_yOff', 'sum', 'ts_dot', 'y'),
      edge('ts_dotSize', 'number', 'ts_dot', 'w'),
      edge('ts_dotSize', 'number', 'ts_dot', 'h'),
      edge('ts_fill', 'style', 'ts_dot', 'style'),
      edge('ts_dot', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'ts_renderer', 'layers'),
    ],
  };
}


// ─── Sample: FFT Bloom ─────────────────────────────────────────────────────
// An oscillator (whose freq is driven by mouse Y) feeds an FFT. The 64-bin
// spectrum is plotted polar-style: each bin becomes a dot on a ring whose
// radius is 80 + magnitude · 0.7. Hue cycles around the ring.
export function buildFFTBloom() {
  const mouse = node('fb_mouse', 'input/mouse', { x: -420, y: 20 });
  const freqMap = node('fb_freqMap', 'math/map', { x: -200, y: 20 }, { inMin: -250, inMax: 250, outMin: 80, outMax: 500 });
  const ampS = node('fb_amp', 'io/slider', { x: -420, y: 110 }, { value: 0.25, min: 0, max: 1 });
  const osc = node('fb_osc', 'sound/oscillator', { x: 20, y: 20 }, { type: 'sawtooth' });
  const out = node('fb_out', 'sound/output', { x: 240, y: 20 });
  const fft = node('fb_fft', 'sound/fft', { x: 20, y: 110 }, { bins: '64' });
  const idxs = nodeWithPorts('fb_idxs', 'spread/range', { x: -420, y: 200 }, { count: 64 });

  const RID = 'fb_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -160, y: 200 },
    data: {
      defType: 'region/foreach',
      params: { width: 900, height: 460, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'v', name: 'v', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 900, height: 460 },
  };

  // Inside: angle = i / 64 * TAU; radius = 80 + v * 0.7
  const angleMul = node('fb_angleMul', 'math/mul', { x: 60, y: 40 }, { b: (Math.PI * 2) / 64 }, RID);
  const vScaled = node('fb_vScaled', 'math/mul', { x: 60, y: 110 }, { b: 0.7 }, RID);
  const radius = node('fb_radius', 'math/add', { x: 220, y: 110 }, { b: 80 }, RID);
  const fromAng = node('fb_fromAng', 'vec2/from-angle', { x: 380, y: 70 }, {}, RID);
  const splitPos = node('fb_splitPos', 'vec2/split', { x: 540, y: 70 }, {}, RID);
  const xCenter = node('fb_xCenter', 'math/add', { x: 700, y: 40 }, { b: 0 }, RID);
  const yCenter = node('fb_yCenter', 'math/add', { x: 700, y: 110 }, { b: 0 }, RID);
  // Hue = i * 5.625 (64*5.625 = 360)
  const hueMul = node('fb_hueMul', 'math/mul', { x: 60, y: 240 }, { b: 5.625 }, RID);
  const hsl = node('fb_hsl', 'color/hsl', { x: 220, y: 240 }, { s: 80, l: 60, a: 255 }, RID);
  const fill = node('fb_fill', 'style/fill', { x: 380, y: 240 }, { mode: 'color' }, RID);
  // dot size = 3 + v * 0.04
  const sScaled = node('fb_sScaled', 'math/mul', { x: 540, y: 240 }, { b: 0.04 }, RID);
  const sAdd = node('fb_sAdd', 'math/add', { x: 700, y: 240 }, { b: 3 }, RID);
  const dot = node('fb_dot', 'draw/ellipse', { x: 860, y: 100 }, {}, RID);

  // Dark backdrop — bright spectral dots pop more.
  const bgColor = node('fb_bg', 'color/rgb', { x: -420, y: 280 }, { r: 12, g: 8, b: 28, a: 255 });
  const renderer = node('fb_renderer', 'render/renderer', { x: 480, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      mouse, freqMap, ampS, osc, out, fft, idxs,
      region,
      angleMul, vScaled, radius, fromAng, splitPos, xCenter, yCenter,
      hueMul, hsl, fill, sScaled, sAdd, dot,
      bgColor, renderer,
    ],
    edges: [
      edge('fb_bg', 'color', 'fb_renderer', 'background'),
      // Mouse Y → freq → osc → output + FFT
      edge('fb_mouse', 'y', 'fb_freqMap', 'x'),
      edge('fb_freqMap', 'mapped', 'fb_osc', 'freq'),
      edge('fb_amp', 'number', 'fb_osc', 'amp'),
      edge('fb_osc', 'audio', 'fb_out', 'in'),
      edge('fb_osc', 'audio', 'fb_fft', 'in'),
      // ForEach over (i, spectrum)
      edge('fb_idxs', 'indices', RID, 'i'),
      edge('fb_fft', 'spectrum', RID, 'v'),
      // angle = i * step
      edge(RID, 'i', 'fb_angleMul', 'a'),
      // radius = 80 + v * 0.7
      edge(RID, 'v', 'fb_vScaled', 'a'),
      edge('fb_vScaled', 'product', 'fb_radius', 'a'),
      edge('fb_angleMul', 'product', 'fb_fromAng', 'angle'),
      edge('fb_radius', 'sum', 'fb_fromAng', 'mag'),
      edge('fb_fromAng', 'vector', 'fb_splitPos', 'v'),
      edge('fb_splitPos', 'x', 'fb_xCenter', 'a'),
      edge('fb_splitPos', 'y', 'fb_yCenter', 'a'),
      // hue + size
      edge(RID, 'i', 'fb_hueMul', 'a'),
      edge('fb_hueMul', 'product', 'fb_hsl', 'h'),
      edge('fb_hsl', 'color', 'fb_fill', 'color'),
      edge(RID, 'v', 'fb_sScaled', 'a'),
      edge('fb_sScaled', 'product', 'fb_sAdd', 'a'),
      // Dot
      edge('fb_xCenter', 'sum', 'fb_dot', 'x'),
      edge('fb_yCenter', 'sum', 'fb_dot', 'y'),
      edge('fb_sAdd', 'sum', 'fb_dot', 'w'),
      edge('fb_sAdd', 'sum', 'fb_dot', 'h'),
      edge('fb_fill', 'style', 'fb_dot', 'style'),
      edge('fb_dot', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'fb_renderer', 'layers'),
    ],
  };
}
