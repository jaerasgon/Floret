import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';


// ─── Sample: Random Walk Trail ─────────────────────────────────────────────
// A walker drifts via hashed (time, seed) angles. A length-N vec2 trail
// is stored in a State Read/Write slot pair (slot 1) — each tick the
// walker reads the current trail, drops the oldest point, appends a new
// one, and writes it back. Read on the left, Write on the right —
// strictly left-to-right with no back-edge in the graph.
export function buildRandomWalkTrail() {
  const N = 200;
  const time = node('rw_time', 'time/seconds', { x: -480, y: 20 });
  const step = node('rw_step', 'io/slider', { x: -480, y: 100 }, { value: 6, min: 1, max: 20 });
  const seed = node('rw_seed', 'io/slider', { x: -480, y: 180 }, { value: 7, min: 1, max: 99 });
  // Seed the trail with N copies of (250, 250) so the walker starts at canvas centre.
  const center = node('rw_center', 'vec2/make', { x: -480, y: 260 }, { x: 0, y: 0 });
  const init = node('rw_init', 'spread/repeat', { x: -260, y: 260 }, { count: N });
  // Read* on the left of the walk, Write* on the right — flow stays
  // strictly left-to-right with no back-edge through the graph.
  const read = node('rw_read', 'state/read-spread', { x: 140, y: 80 }, { slot: 1 });
  const walk = node('rw_walk', 'spread/walk-step', { x: 360, y: 80 }, { bounds: 230 });
  const write = node('rw_write', 'state/write-spread', { x: 580, y: 80 }, { slot: 1 });
  const idx = node('rw_idx', 'spread/range', { x: -480, y: 340 }, { count: N });

  const RID = 'rw_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 200, y: 80 },
    data: {
      defType: 'region/foreach',
      params: { width: 720, height: 460, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 720, height: 460 },
  };

  // Inside: hue = i / N * 360, alpha = i / N * 255 (newest is brightest)
  const split = node('rw_split', 'vec2/split', { x: 60, y: 60 }, {}, RID);
  const hueMul = node('rw_hueMul', 'math/mul', { x: 60, y: 160 }, { b: 360 / N }, RID);
  const alpMul = node('rw_alpMul', 'math/mul', { x: 60, y: 220 }, { b: 255 / N }, RID);
  const sizeMul = node('rw_sizeMul', 'math/mul', { x: 60, y: 280 }, { b: 4 / N }, RID);
  const sizeAdd = node('rw_sizeAdd', 'math/add', { x: 240, y: 280 }, { b: 1 }, RID);
  const hsl = node('rw_hsl', 'color/hsl', { x: 240, y: 160 }, { s: 85, l: 60 }, RID);
  const fill = node('rw_fill', 'style/fill', { x: 420, y: 160 }, { mode: 'color' }, RID);
  const dot = node('rw_dot', 'draw/ellipse', { x: 600, y: 60 }, {}, RID);

  const bg = node('rw_bg', 'color/rgb', { x: -480, y: 420 }, { r: 14, g: 14, b: 26, a: 255 });
  const renderer = node('rw_renderer', 'render/renderer', { x: 1000, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, step, seed, center, init, read, walk, write, idx,
      region,
      split, hueMul, alpMul, sizeMul, sizeAdd, hsl, fill, dot,
      bg, renderer,
    ],
    edges: [
      edge('rw_bg', 'color', 'rw_renderer', 'background'),
      // Seed: a length-N spread of (centre, centre) feeds Read*.init for frame 0.
      edge('rw_center', 'vector', 'rw_init', 'value'),
      edge('rw_init', 'values', 'rw_read', 'init'),
      edge('rw_read', 'current', 'rw_walk', 'trail'),
      edge('rw_time', 't', 'rw_walk', 'time'),
      edge('rw_step', 'number', 'rw_walk', 'step'),
      edge('rw_seed', 'number', 'rw_walk', 'seed'),
      edge('rw_walk', 'trail', 'rw_write', 'next'),
      // Render trail — the freshly written value also drives this frame's render.
      edge('rw_walk', 'trail', RID, 'p'),
      edge('rw_idx', 'indices', RID, 'i'),
      edge(RID, 'p', 'rw_split', 'v'),
      edge(RID, 'i', 'rw_hueMul', 'a'),
      edge(RID, 'i', 'rw_alpMul', 'a'),
      edge(RID, 'i', 'rw_sizeMul', 'a'),
      edge('rw_sizeMul', 'product', 'rw_sizeAdd', 'a'),
      edge('rw_hueMul', 'product', 'rw_hsl', 'h'),
      edge('rw_alpMul', 'product', 'rw_hsl', 'a'),
      edge('rw_hsl', 'color', 'rw_fill', 'color'),
      edge('rw_split', 'x', 'rw_dot', 'x'),
      edge('rw_split', 'y', 'rw_dot', 'y'),
      edge('rw_sizeAdd', 'sum', 'rw_dot', 'w'),
      edge('rw_sizeAdd', 'sum', 'rw_dot', 'h'),
      edge('rw_fill', 'style', 'rw_dot', 'style'),
      edge('rw_dot', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'rw_renderer', 'layers'),
    ],
  };
}




// ─── Sample: Paint Trail ───────────────────────────────────────────────────
// Move the mouse to paint. Each frame, the current brush dot is composited
// ON TOP of the previous frame's image using image/from-layer. The result is
// stored in state/value and used as the "canvas" for the next frame, giving
// infinite accumulation. image/crop makes a copy of the old canvas before
// from-layer clears it — this is the double-buffer trick that prevents
// flicker.
export function buildPaintTrail() {
  const mouse    = node('pt_mouse',    'input/mouse',      { x: -500, y:  20 });
  const time     = node('pt_time',     'time/seconds',     { x: -500, y: 140 });
  // Read on the left (slot 1) holds the accumulated canvas. Frame 0 is
  // blank because no Write has run yet (Read.init unwired = empty).
  const stateRead= node('pt_read',     'state/read',       { x: -280, y:  20 }, { slot: 1 });
  // Copy previous frame before from-layer clears its surface.
  const prevCopy = node('pt_prevCopy', 'image/crop',       { x:  -60, y:  20 }, { sx: 0, sy: 0, sw: 0, sh: 0 });
  // Layer 1: render previous accumulated canvas.
  const drawPrev = node('pt_drawPrev', 'draw/image',       { x:  220, y:  20 }, { x: -250, y: -250, w: 500, h: 500 });
  // Brush color: hue cycles with time.
  const hueMul   = node('pt_hueMul',   'math/mul',         { x: -500, y: 240 }, { b: 50 });
  const hsl      = node('pt_hsl',      'color/hsl',        { x: -300, y: 240 }, { s: 90, l: 60, a: 255 });
  const fill     = node('pt_fill',     'style/fill',       { x: -100, y: 240 }, { mode: 'color' });
  // Brush size from slider.
  const sizeSlider = node('pt_size',   'io/slider',        { x: -500, y: 340 }, { value: 28, min: 4, max: 80 });
  // Layer 2: new brush dot at mouse.
  const brush    = node('pt_brush',    'draw/ellipse',     { x:  220, y: 240 });
  // Combine both layers.
  const stack    = node('pt_stack',    'spread/stack',     { x:  480, y: 120 });
  // Render combined layers into an offscreen canvas.
  const fromLayer= node('pt_fromLayer','image/from-layer', { x:  680, y:  80 }, { w: 500, h: 500 });
  // Write on the right (slot 1) — completes the L→R flow.
  const stateWrite=node('pt_write',    'state/write',      { x:  880, y:  80 }, { slot: 1 });
  // Display on screen.
  const display  = node('pt_display',  'draw/image',       { x: 1080, y:  80 }, { x: -250, y: -250, w: 500, h: 500 });
  const renderer = node('pt_renderer', 'render/renderer',  { x: 1280, y:  80 }, { width: 500, height: 500 });

  return {
    nodes: [mouse, time, stateRead, prevCopy, drawPrev, hueMul, hsl, fill, sizeSlider, brush, stack, fromLayer, stateWrite, display, renderer],
    edges: [
      // Previous frame: Read → copy → drawPrev layer
      edge('pt_read',     'current', 'pt_prevCopy', 'image'),
      edge('pt_prevCopy', 'image',   'pt_drawPrev', 'image'),
      // Brush color driven by time
      edge('pt_time',     't',       'pt_hueMul',   'a'),
      edge('pt_hueMul', 'product',     'pt_hsl',      'h'),
      edge('pt_hsl', 'color',     'pt_fill',     'color'),
      edge('pt_fill', 'style',     'pt_brush',    'style'),
      edge('pt_mouse',    'x',       'pt_brush',    'x'),
      edge('pt_mouse',    'y',       'pt_brush',    'y'),
      edge('pt_size', 'number',     'pt_brush',    'w'),
      edge('pt_size', 'number',     'pt_brush',    'h'),
      // Stack: [prev layer, brush layer] → from-layer
      edge('pt_drawPrev', 'layer',     'pt_stack',    'a'),
      edge('pt_brush', 'shape',     'pt_stack',    'b'),
      edge('pt_stack', 'values',     'pt_fromLayer', 'layer'),
      // from-layer → Write (feedback) + display. Both end in L→R nodes
      // sitting to the right of from-layer — no back-edge.
      edge('pt_fromLayer','image',   'pt_write',    'next'),
      edge('pt_fromLayer','image',   'pt_display',  'image'),
      edge('pt_display', 'layer',     'pt_renderer', 'layers'),
    ],
  };
}
