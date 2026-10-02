import type { FloretNode } from '../../../editor/editor-state';
import { node, nodeWithPorts, edge, note } from '../helpers';


// ─── Tutorial 01: Hello Rect ───────────────────────────────────────────────
// The smallest possible patch. One shape, one renderer.
export function buildTutHelloRect() {
  const c = note(
    't01_c',
    { x: -360, y: 0 },
    'Welcome to Floret!\n\nEvery patch ends at a Renderer (the box on the right with the canvas preview). Output pins are on the right of each node, input pins on the left. Drag from one to the other to make a wire.\n\nThis patch wires a single Rect into the Renderer. Open the Rect — change x/y/w/h to move and resize it.',
    { w: 360, h: 220 },
  );
  const rect = node('t01_rect', 'draw/rect', { x: 60, y: 60 }, { x: -50, y: -50, w: 100, h: 100 });
  const renderer = node('t01_renderer', 'render/renderer', { x: 420, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, rect, renderer],
    edges: [edge('t01_rect', 'shape', 't01_renderer', 'layers')],
  };
}


// ─── Tutorial 02: Color + Fill ─────────────────────────────────────────────
// Introduce the styling chain: Color → Fill → shape.style.
export function buildTutColorAndFill() {
  const c = note(
    't02_c',
    { x: -380, y: 0 },
    'Colors and styles\n\nShapes have a `style` input. Style nodes (Fill, Stroke, NoFill…) build up a style value. Color nodes (Color, Palette…) build colour values.\n\nHere: Color (pink) → Fill → Rect.style. Open the Color node to swap modes (rgb / hsl / hex).',
    { w: 360, h: 200 },
  );
  const col = node('t02_col', 'color/color', { x: 40, y: 0 }, { mode: 'rgb', r: 230, g: 80, b: 150, a: 255 });
  const fill = node('t02_fill', 'style/fill', { x: 240, y: 0 });
  const rect = node('t02_rect', 'draw/rect', { x: 440, y: 80 }, { x: -70, y: -70, w: 140, h: 140 });
  const renderer = node('t02_renderer', 'render/renderer', { x: 700, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, col, fill, rect, renderer],
    edges: [
      edge('t02_col', 'color', 't02_fill', 'color'),
      edge('t02_fill', 'style', 't02_rect', 'style'),
      edge('t02_rect', 'shape', 't02_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 03: Animate with Time ────────────────────────────────────────
// time/seconds → sin → scale → ellipse position. The classic intro.
export function buildTutAnimateWithTime() {
  const c = note(
    't03_c',
    { x: -380, y: 0 },
    'Animation = time → shape\n\n`time/seconds` outputs the time elapsed in seconds. Run it through `sin` and you get a smooth oscillation between -1 and 1. Multiply to scale the motion to pixels.\n\nThis ellipse swings left-right with sin(t)·150.',
    { w: 360, h: 200 },
  );
  const t = node('t03_t', 'time/seconds', { x: 40, y: 0 });
  const sinN = node('t03_sin', 'math/sin', { x: 220, y: 0 });
  const amp = node('t03_amp', 'math/mul', { x: 400, y: 0 }, { b: 150 });
  const ell = node('t03_ell', 'draw/ellipse', { x: 580, y: 60 }, { y: 0, w: 80, h: 80 });
  const renderer = node('t03_renderer', 'render/renderer', { x: 800, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, t, sinN, amp, ell, renderer],
    edges: [
      edge('t03_t', 't', 't03_sin', 'x'),
      edge('t03_sin', 'sine', 't03_amp', 'a'),
      edge('t03_amp', 'product', 't03_ell', 'x'),
      edge('t03_ell', 'shape', 't03_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 04: Mouse Input ──────────────────────────────────────────────
// input/mouse → ellipse position. Move the cursor in the preview.
export function buildTutMouse() {
  const c = note(
    't04_c',
    { x: -380, y: 0 },
    'Reading user input\n\n`input/mouse` exposes the cursor position inside the preview canvas (centred at 0,0). Wire `x` and `y` into a shape and it follows the mouse.\n\nHover the preview to drag the circle around.',
    { w: 360, h: 200 },
  );
  const m = node('t04_m', 'input/mouse', { x: 40, y: 0 });
  const ell = node('t04_ell', 'draw/ellipse', { x: 320, y: 40 }, { w: 60, h: 60 });
  const renderer = node('t04_renderer', 'render/renderer', { x: 560, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, m, ell, renderer],
    edges: [
      edge('t04_m', 'x', 't04_ell', 'x'),
      edge('t04_m', 'y', 't04_ell', 'y'),
      edge('t04_ell', 'shape', 't04_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 05: Slider Control ───────────────────────────────────────────
// A slider drives the rect's width.
export function buildTutSlider() {
  const c = note(
    't05_c',
    { x: -380, y: 0 },
    'Inline controls\n\n`io/slider` is a knob you can drag right on the node. Wire its output into any number input. Use the tiny boxes under the slider to set its min / max.\n\nDrag the slider to grow / shrink the rect.',
    { w: 360, h: 200 },
  );
  const s = node('t05_s', 'io/slider', { x: 60, y: 0 }, { value: 120, min: 20, max: 300 });
  const halfW = node('t05_halfW', 'math/mul', { x: 240, y: 0 }, { b: -0.5 });
  const rect = node('t05_rect', 'draw/rect', { x: 440, y: 40 }, { y: -50, h: 100 });
  const renderer = node('t05_renderer', 'render/renderer', { x: 660, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, s, halfW, rect, renderer],
    edges: [
      // x = -w/2 keeps the rect centred as the slider changes its width
      edge('t05_s', 'number', 't05_halfW', 'a'),
      edge('t05_halfW', 'product', 't05_rect', 'x'),
      edge('t05_s', 'number', 't05_rect', 'w'),
      edge('t05_rect', 'shape', 't05_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 06: Spreads & ForEach ────────────────────────────────────────
// RangeSpread → ForEach → 12 evenly-spaced rects.
export function buildTutSpreadForeach() {
  const c = note(
    't06_c',
    { x: -380, y: 0 },
    'Spreads = lists of values\n\nA spread is a list. `RangeSpread` produces 0,1,2,…,N-1. To use each value individually, wrap a ForEach region around the body — it runs the inside once per element.\n\nHere: 12 rects laid out across the canvas.',
    { w: 360, h: 220 },
  );
  const range = nodeWithPorts('t06_range', 'spread/range', { x: 60, y: 0 }, { count: 12 });

  const RID = 't06_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 300, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 460, height: 240, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 460, height: 240 },
  };

  // x = i*32 - 176 — spread the rects across the canvas
  const spaceX = node('t06_spaceX', 'math/mul', { x: 40, y: 60 }, { b: 32 }, RID);
  const offX = node('t06_offX', 'math/add', { x: 200, y: 60 }, { b: -176 }, RID);
  const rect = node('t06_rect', 'draw/rect', { x: 320, y: 60 }, { y: -10, w: 20, h: 20 }, RID);

  const renderer = node('t06_renderer', 'render/renderer', { x: 820, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, range, region, spaceX, offX, rect, renderer],
    edges: [
      edge('t06_range', 'indices', RID, 'i'),
      edge(RID, 'i', 't06_spaceX', 'a'),
      edge('t06_spaceX', 'product', 't06_offX', 'a'),
      edge('t06_offX', 'sum', 't06_rect', 'x'),
      edge('t06_rect', 'shape', RID, 'layer'),
      edge(RID, 'layer', 't06_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 07: Grid of Dots ─────────────────────────────────────────────
// GridSpread (Spread<Vec2>) → ForEach → ellipse per point.
export function buildTutGrid() {
  const c = note(
    't07_c',
    { x: -400, y: 0 },
    'Grids and vec2 spreads\n\n`GridSpread` emits a Spread<vec2> — one (x,y) per cell. Pass it into a ForEach, then `vec2/split` inside the region to break each vec2 into its x and y, and feed those into a shape.\n\nResult: an 8×8 grid of circles.',
    { w: 380, h: 240 },
  );
  const grid = nodeWithPorts('t07_grid', 'spread/grid', { x: 60, y: 0 }, { cols: 8, rows: 8, w: 320, h: 320 });

  const RID = 't07_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 300, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 460, height: 240, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 460, height: 240 },
  };

  const split = node('t07_split', 'vec2/split', { x: 40, y: 40 }, {}, RID);
  const ell = node('t07_ell', 'draw/ellipse', { x: 240, y: 40 }, { w: 14, h: 14 }, RID);

  const renderer = node('t07_renderer', 'render/renderer', { x: 820, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, grid, region, split, ell, renderer],
    edges: [
      edge('t07_grid', 'positions', RID, 'p'),
      edge(RID, 'p', 't07_split', 'v'),
      edge('t07_split', 'x', 't07_ell', 'x'),
      edge('t07_split', 'y', 't07_ell', 'y'),
      edge('t07_ell', 'shape', RID, 'layer'),
      edge(RID, 'layer', 't07_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 08: Palette + CycleSpread ────────────────────────────────────
// Stretch a 4-stop palette across a 64-point grid, colour each dot.
export function buildTutPalette() {
  const c = note(
    't08_c',
    { x: -400, y: 0 },
    'Palettes and colour spreads\n\n`Palette` emits a Spread<Color>. To colour a 64-point grid from a 4-stop palette, use `CycleSpread` to repeat the palette until it has 64 elements.\n\nThe ForEach takes two inputs (vec2 and color) and runs once per pair.',
    { w: 380, h: 220 },
  );
  const grid = nodeWithPorts('t08_grid', 'spread/grid', { x: 60, y: 0 }, { cols: 8, rows: 8, w: 320, h: 320 });
  const pal = node('t08_pal', 'color/palette', { x: 60, y: 220 }, { stopCount: 4 });
  const count = node('t08_count', 'value/number', { x: 60, y: 360 }, { value: 64 });
  const cyc = nodeWithPorts('t08_cyc', 'spread/cycle', { x: 280, y: 220 }, { count: 64 });

  const RID = 't08_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 540, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 500, height: 360, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input', y: 40 },
        { id: 'c', name: 'c', type: 'color', direction: 'input', y: 80 },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 500, height: 360 },
  };

  const split = node('t08_split', 'vec2/split', { x: 40, y: 40 }, {}, RID);
  const fill = node('t08_fill', 'style/fill', { x: 40, y: 200 }, {}, RID);
  const ell = node('t08_ell', 'draw/ellipse', { x: 280, y: 80 }, { w: 28, h: 28 }, RID);

  const renderer = node('t08_renderer', 'render/renderer', { x: 1080, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, grid, pal, count, cyc, region, split, fill, ell, renderer],
    edges: [
      edge('t08_pal', 'colors', 't08_cyc', 'spread'),
      edge('t08_count', 'number', 't08_cyc', 'count'),
      edge('t08_grid', 'positions', RID, 'p'),
      edge('t08_cyc', 'values', RID, 'c'),
      edge(RID, 'p', 't08_split', 'v'),
      edge('t08_split', 'x', 't08_ell', 'x'),
      edge('t08_split', 'y', 't08_ell', 'y'),
      edge(RID, 'c', 't08_fill', 'color'),
      edge('t08_fill', 'style', 't08_ell', 'style'),
      edge('t08_ell', 'shape', RID, 'layer'),
      edge(RID, 'layer', 't08_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 09: State (per-frame memory) ─────────────────────────────────
// A frame-incremented counter drives a hue rotation.
export function buildTutState() {
  const c = note(
    't09_c',
    { x: -400, y: 0 },
    'State = memory across frames\n\n`state/value` carries a value from one frame to the next.\n• `current` is what last frame stored.\n• Wire something into `next` and that becomes `current` on the very next frame.\n\nHere we add 1 every frame and use it to rotate the hue.',
    { w: 380, h: 240 },
  );
  const st = node('t09_state', 'state/value', { x: 60, y: 0 }, { init: 0 });
  const step = node('t09_step', 'math/add', { x: 260, y: 0 }, { b: 1 });
  const mod = node('t09_mod', 'math/mul', { x: 60, y: 200 }, { b: 1 }); // identity; visual aid only
  void mod;
  const col = node('t09_col', 'color/color', { x: 260, y: 200 }, { mode: 'hsl', s: 80, l: 55, a: 255 });
  const fill = node('t09_fill', 'style/fill', { x: 460, y: 200 });
  const ell = node('t09_ell', 'draw/ellipse', { x: 660, y: 0 }, { w: 160, h: 160 });
  const renderer = node('t09_renderer', 'render/renderer', { x: 880, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, st, step, col, fill, ell, renderer],
    edges: [
      // current → +1 → next  (the feedback loop)
      edge('t09_state', 'current', 't09_step', 'a'),
      edge('t09_step', 'sum', 't09_state', 'next'),
      // current → hue
      edge('t09_state', 'current', 't09_col', 'h'),
      edge('t09_col', 'color', 't09_fill', 'color'),
      edge('t09_fill', 'style', 't09_ell', 'style'),
      edge('t09_ell', 'shape', 't09_renderer', 'layers'),
    ],
  };
}



// ─── Tutorial 11: Polar Coordinates ────────────────────────────────────────
// Rotating arm: x = cos(t)·r, y = sin(t)·r. The foundation of every radial layout.
export function buildTutPolar() {
  const c = note(
    't11_c',
    { x: -380, y: 0 },
    'Polar coordinates\n\nA point on a circle is (cos(angle)·r, sin(angle)·r). Drive `angle` with time and you get rotation; vary `r` and you get spirals.\n\nThis line goes from the origin out to a rotating point.',
    { w: 360, h: 220 },
  );
  const t = node('t11_t', 'time/seconds', { x: 40, y: 0 });
  const cosN = node('t11_cos', 'math/cos', { x: 200, y: -40 });
  const sinN = node('t11_sin', 'math/sin', { x: 200, y: 40 });
  const rx = node('t11_rx', 'math/mul', { x: 360, y: -40 }, { b: 180 });
  const ry = node('t11_ry', 'math/mul', { x: 360, y: 40 }, { b: 180 });
  const line = node('t11_line', 'draw/line', { x: 540, y: 0 }, { x1: 0, y1: 0 });
  const renderer = node('t11_renderer', 'render/renderer', { x: 760, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, t, cosN, sinN, rx, ry, line, renderer],
    edges: [
      edge('t11_t', 't', 't11_cos', 'x'),
      edge('t11_t', 't', 't11_sin', 'x'),
      edge('t11_cos', 'cosine', 't11_rx', 'a'),
      edge('t11_sin', 'sine', 't11_ry', 'a'),
      edge('t11_rx', 'product', 't11_line', 'x2'),
      edge('t11_ry', 'product', 't11_line', 'y2'),
      edge('t11_line', 'shape', 't11_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 12: Circle of Shapes ─────────────────────────────────────────
// RingSpread = N points on a circle. Here we use its `angles` output to
// build the layout by hand; `positions` would give the points directly.
export function buildTutCircleOfShapes() {
  const c = note(
    't12_c',
    { x: -400, y: 0 },
    'Layouts on a circle\n\n`RingSpread` lays N points around a circle. Its `positions` output gives them directly — but here we use the `angles` output and build the layout by hand: inside a ForEach, take cos/sin of each angle, multiply by a radius, and draw a shape.\n\nResult: 24 dots evenly placed on a ring.',
    { w: 380, h: 220 },
  );
  const circ = nodeWithPorts('t12_circ', 'spread/ring', { x: 60, y: 0 }, { count: 24, phase: 0 });
  const R = node('t12_R', 'value/number', { x: 60, y: 180 }, { value: 150 });

  const RID = 't12_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 480, height: 280, label: 'ForEach' },
      ports: [
        { id: 'a', name: 'angle', type: 'number', direction: 'input', y: 40 },
        { id: 'r', name: 'r', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 480, height: 280 },
  };
  const cosN = node('t12_cos', 'math/cos', { x: 40, y: 40 }, {}, RID);
  const sinN = node('t12_sin', 'math/sin', { x: 40, y: 100 }, {}, RID);
  const mulX = node('t12_mulX', 'math/mul', { x: 200, y: 40 }, {}, RID);
  const mulY = node('t12_mulY', 'math/mul', { x: 200, y: 100 }, {}, RID);
  const ell = node('t12_ell', 'draw/ellipse', { x: 360, y: 60 }, { w: 18, h: 18 }, RID);

  const renderer = node('t12_renderer', 'render/renderer', { x: 860, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, circ, R, region, cosN, sinN, mulX, mulY, ell, renderer],
    edges: [
      edge('t12_circ', 'angles', RID, 'a'),
      edge('t12_R', 'number', RID, 'r'),
      edge(RID, 'a', 't12_cos', 'x'),
      edge(RID, 'a', 't12_sin', 'x'),
      edge('t12_cos', 'cosine', 't12_mulX', 'a'),
      edge(RID, 'r', 't12_mulX', 'b'),
      edge('t12_sin', 'sine', 't12_mulY', 'a'),
      edge(RID, 'r', 't12_mulY', 'b'),
      edge('t12_mulX', 'product', 't12_ell', 'x'),
      edge('t12_mulY', 'product', 't12_ell', 'y'),
      edge('t12_ell', 'shape', RID, 'l'),
      edge(RID, 'l', 't12_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 13: Range Remapping (math/map) ───────────────────────────────
// math/map: convert one numeric range to another. The most-used number node.
export function buildTutMap() {
  const c = note(
    't13_c',
    { x: -400, y: 0 },
    'Remapping ranges\n\n`math/map` converts a value from one range to another. Example: mouseX is in [-200, 200] but a hue needs [0, 360].\n\nMove the mouse left/right — the ellipse changes hue across the rainbow.',
    { w: 380, h: 220 },
  );
  const m = node('t13_m', 'input/mouse', { x: 60, y: 0 });
  const map = node('t13_map', 'math/map', { x: 240, y: 0 }, { inMin: -200, inMax: 200, outMin: 0, outMax: 360 });
  const col = node('t13_col', 'color/color', { x: 460, y: 0 }, { mode: 'hsl', s: 80, l: 55, a: 255 });
  const fill = node('t13_fill', 'style/fill', { x: 640, y: 0 });
  const ell = node('t13_ell', 'draw/ellipse', { x: 820, y: 40 }, { w: 200, h: 200 });
  const renderer = node('t13_renderer', 'render/renderer', { x: 1040, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, m, map, col, fill, ell, renderer],
    edges: [
      edge('t13_m', 'x', 't13_map', 'x'),
      edge('t13_map', 'mapped', 't13_col', 'h'),
      edge('t13_col', 'color', 't13_fill', 'color'),
      edge('t13_fill', 'style', 't13_ell', 'style'),
      edge('t13_ell', 'shape', 't13_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 14: Lerp Two Colours ─────────────────────────────────────────
// color/lerp blends colour A → colour B by parameter t.
export function buildTutColorLerp() {
  const c = note(
    't14_c',
    { x: -400, y: 0 },
    'Blending colours\n\n`color/lerp` mixes two colours by t ∈ [0, 1]. sin(time)*0.5+0.5 oscillates t between 0 and 1, so the fill colour smoothly slides between magenta and cyan and back.',
    { w: 380, h: 200 },
  );
  const colA = node('t14_colA', 'color/color', { x: 60, y: 0 }, { mode: 'rgb', r: 230, g: 60, b: 200, a: 255 });
  const colB = node('t14_colB', 'color/color', { x: 60, y: 200 }, { mode: 'rgb', r: 60, g: 200, b: 230, a: 255 });
  const t = node('t14_t', 'time/seconds', { x: 60, y: 400 });
  const sinN = node('t14_sin', 'math/sin', { x: 240, y: 400 });
  // t = sin*0.5 + 0.5
  const half = node('t14_half', 'math/mul', { x: 420, y: 400 }, { b: 0.5 });
  const off = node('t14_off', 'math/add', { x: 600, y: 400 }, { b: 0.5 });
  const lerp = node('t14_lerp', 'color/lerp', { x: 300, y: 100 });
  const fill = node('t14_fill', 'style/fill', { x: 500, y: 100 });
  const rect = node('t14_rect', 'draw/rect', { x: 700, y: 140 }, { x: -120, y: -120, w: 240, h: 240 });
  const renderer = node('t14_renderer', 'render/renderer', { x: 920, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, colA, colB, t, sinN, half, off, lerp, fill, rect, renderer],
    edges: [
      edge('t14_t', 't', 't14_sin', 'x'),
      edge('t14_sin', 'sine', 't14_half', 'a'),
      edge('t14_half', 'product', 't14_off', 'a'),
      edge('t14_colA', 'color', 't14_lerp', 'a'),
      edge('t14_colB', 'color', 't14_lerp', 'b'),
      edge('t14_off', 'sum', 't14_lerp', 't'),
      edge('t14_lerp', 'color', 't14_fill', 'color'),
      edge('t14_fill', 'style', 't14_rect', 'style'),
      edge('t14_rect', 'shape', 't14_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 15: Style Chains ─────────────────────────────────────────────
// Fill + Stroke + StrokeWeight, layered via each style's `base` input.
export function buildTutStyleChain() {
  const c = note(
    't15_c',
    { x: -400, y: 0 },
    'Chaining styles\n\nEvery style node has a `base` input. Wire one style INTO another\'s `base` and the second layers its override on top.\n\nFill (yellow) → Stroke (black, 6px) → Rect. The rectangle has both fill AND stroke.',
    { w: 380, h: 220 },
  );
  const colF = node('t15_colF', 'color/color', { x: 60, y: 0 }, { mode: 'rgb', r: 250, g: 200, b: 60, a: 255 });
  const colS = node('t15_colS', 'color/color', { x: 60, y: 200 }, { mode: 'rgb', r: 30, g: 30, b: 30, a: 255 });
  const fill = node('t15_fill', 'style/fill', { x: 260, y: 0 });
  const stroke = node('t15_stroke', 'style/stroke', { x: 460, y: 100 }, { width: 6 });
  const rect = node('t15_rect', 'draw/rect', { x: 700, y: 120 }, { x: -100, y: -100, w: 200, h: 200 });
  const renderer = node('t15_renderer', 'render/renderer', { x: 920, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, colF, colS, fill, stroke, rect, renderer],
    edges: [
      edge('t15_colF', 'color', 't15_fill', 'color'),
      edge('t15_colS', 'color', 't15_stroke', 'color'),
      // Fill → Stroke.base: Stroke layers stroke ON TOP of Fill
      edge('t15_fill', 'style', 't15_stroke', 'base'),
      edge('t15_stroke', 'style', 't15_rect', 'style'),
      edge('t15_rect', 'shape', 't15_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 16: Group + Affine Transform ─────────────────────────────────
// Group two shapes, rotate the whole group with time.
export function buildTutGroupTransform() {
  const c = note(
    't16_c',
    { x: -400, y: 0 },
    'Group + Transform\n\n`transform/group` turns multiple layers into one. `transform/rotate` then rotates everything together.\n\nTwo rectangles offset from origin, grouped, rotated. The pair spins as a unit.',
    { w: 380, h: 220 },
  );
  const r1 = node('t16_r1', 'draw/rect', { x: 60, y: 0 }, { x: -120, y: -20, w: 80, h: 40 });
  const r2 = node('t16_r2', 'draw/rect', { x: 60, y: 120 }, { x: 40, y: -20, w: 80, h: 40 });
  const grp = node('t16_grp', 'transform/group', { x: 300, y: 60 }, { slotCount: 2 });
  const t = node('t16_t', 'time/seconds', { x: 60, y: 280 });
  const rot = node('t16_rot', 'transform/rotate', { x: 540, y: 60 });
  const renderer = node('t16_renderer', 'render/renderer', { x: 760, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, r1, r2, grp, t, rot, renderer],
    edges: [
      edge('t16_r1', 'shape', 't16_grp', 's1'),
      edge('t16_r2', 'shape', 't16_grp', 's2'),
      edge('t16_grp', 'grouped', 't16_rot', 'layer'),
      edge('t16_t', 't', 't16_rot', 'angle'),
      edge('t16_rot', 'rotated', 't16_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 17: Perlin Noise ─────────────────────────────────────────────
// Per-cell noise sample drives the dot's y offset + hue.
export function buildTutPerlin() {
  const c = note(
    't17_c',
    { x: -400, y: 0 },
    'Perlin noise\n\nUnlike random/uniform (jumpy), Perlin noise is smooth — neighbouring inputs give neighbouring outputs. Perfect for organic motion.\n\nFor each cell, sample noise at (cellX, time). Use the result to nudge y AND shift hue.',
    { w: 380, h: 240 },
  );
  const range = nodeWithPorts('t17_range', 'spread/range', { x: 60, y: 0 }, { count: 40 });
  const t = node('t17_t', 'time/seconds', { x: 60, y: 200 });

  const RID = 't17_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 600, height: 360, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
        { id: 'tm', name: 't', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 600, height: 360 },
  };
  // x = i*10 - 200
  const stepX = node('t17_stepX', 'math/mul', { x: 40, y: 40 }, { b: 10 }, RID);
  const offX = node('t17_offX', 'math/add', { x: 200, y: 40 }, { b: -200 }, RID);
  // noise(x*0.01, time*0.3)
  const noiseX = node('t17_noiseX', 'math/mul', { x: 40, y: 120 }, { b: 0.01 }, RID);
  const noiseT = node('t17_noiseT', 'math/mul', { x: 40, y: 180 }, { b: 0.3 }, RID);
  const n = node('t17_n', 'noise/perlin', { x: 200, y: 140 }, {}, RID);
  // y = noise * 300 - 150
  const yScale = node('t17_yScale', 'math/mul', { x: 360, y: 120 }, { b: 300 }, RID);
  const yOff = node('t17_yOff', 'math/add', { x: 360, y: 180 }, { b: -150 }, RID);
  // hue = noise * 360
  const hue = node('t17_hue', 'math/mul', { x: 360, y: 240 }, { b: 360 }, RID);
  const col = node('t17_col', 'color/color', { x: 360, y: 300 }, { mode: 'hsl', s: 80, l: 55, a: 255 }, RID);
  const fill = node('t17_fill', 'style/fill', { x: 520, y: 300 }, {}, RID);
  const ell = node('t17_ell', 'draw/ellipse', { x: 520, y: 100 }, { w: 12, h: 12 }, RID);

  const renderer = node('t17_renderer', 'render/renderer', { x: 960, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, range, t, region, stepX, offX, noiseX, noiseT, n, yScale, yOff, hue, col, fill, ell, renderer],
    edges: [
      edge('t17_range', 'indices', RID, 'i'),
      edge('t17_t', 't', RID, 'tm'),
      // x
      edge(RID, 'i', 't17_stepX', 'a'),
      edge('t17_stepX', 'product', 't17_offX', 'a'),
      edge('t17_offX', 'sum', 't17_ell', 'x'),
      // noise inputs
      edge(RID, 'i', 't17_noiseX', 'a'),
      edge(RID, 'tm', 't17_noiseT', 'a'),
      edge('t17_noiseX', 'product', 't17_n', 'x'),
      edge('t17_noiseT', 'product', 't17_n', 'y'),
      // y
      edge('t17_n', 'noise', 't17_yScale', 'a'),
      edge('t17_yScale', 'product', 't17_yOff', 'a'),
      edge('t17_yOff', 'sum', 't17_ell', 'y'),
      // hue
      edge('t17_n', 'noise', 't17_hue', 'a'),
      edge('t17_hue', 'product', 't17_col', 'h'),
      edge('t17_col', 'color', 't17_fill', 'color'),
      edge('t17_fill', 'style', 't17_ell', 'style'),
      edge('t17_ell', 'shape', RID, 'l'),
      edge(RID, 'l', 't17_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 18: Random Spread ────────────────────────────────────────────
// random/uniform sampled per-iteration with the index as seed → chaos.
export function buildTutRandomSpread() {
  const c = note(
    't18_c',
    { x: -400, y: 0 },
    'Seeded randomness\n\nUse the loop index as the seed of `random/uniform` and you get a stable scatter — same patch loaded later gives the same scatter. Change the seed input to reroll.\n\n80 ellipses placed at deterministic random (x, y).',
    { w: 380, h: 240 },
  );
  const range = nodeWithPorts('t18_range', 'spread/range', { x: 60, y: 0 }, { count: 80 });
  const seed = node('t18_seed', 'value/number', { x: 60, y: 200 }, { value: 17 });

  const RID = 't18_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 580, height: 320, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
        { id: 's', name: 'seed', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 580, height: 320 },
  };
  // Two uniform samples per iter — one for X, one for Y. Different seeds so
  // they aren\'t correlated. seedX = seed + i, seedY = seed + i + 1000.
  const seedX = node('t18_seedX', 'math/add', { x: 40, y: 40 }, {}, RID);
  const seedYoff = node('t18_seedYoff', 'math/add', { x: 40, y: 120 }, { b: 1000 }, RID);
  const seedY = node('t18_seedY', 'math/add', { x: 200, y: 120 }, {}, RID);
  const ux = node('t18_ux', 'random/uniform', { x: 360, y: 40 }, { min: -180, max: 180 }, RID);
  const uy = node('t18_uy', 'random/uniform', { x: 360, y: 120 }, { min: -180, max: 180 }, RID);
  const ell = node('t18_ell', 'draw/ellipse', { x: 520, y: 80 }, { w: 14, h: 14 }, RID);

  const renderer = node('t18_renderer', 'render/renderer', { x: 940, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, range, seed, region, seedX, seedYoff, seedY, ux, uy, ell, renderer],
    edges: [
      edge('t18_range', 'indices', RID, 'i'),
      edge('t18_seed', 'number', RID, 's'),
      edge(RID, 's', 't18_seedX', 'a'),
      edge(RID, 'i', 't18_seedX', 'b'),
      edge(RID, 'i', 't18_seedYoff', 'a'),
      edge('t18_seedYoff', 'sum', 't18_seedY', 'a'),
      edge(RID, 's', 't18_seedY', 'b'),
      edge('t18_seedX', 'sum', 't18_ux', 'seed'),
      edge('t18_seedY', 'sum', 't18_uy', 'seed'),
      edge('t18_ux', 'sample', 't18_ell', 'x'),
      edge('t18_uy', 'sample', 't18_ell', 'y'),
      edge('t18_ell', 'shape', RID, 'l'),
      edge(RID, 'l', 't18_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 19: If Region (Conditional Draw) ─────────────────────────────
// Show / hide a shape based on whether the mouse is near the centre.
export function buildTutIfRegion() {
  const c = note(
    't19_c',
    { x: -440, y: 0 },
    'Conditional execution\n\n`region/if` only runs its body when `condition` is true — its outputs become empty when false.\n\nHere `sin(time) > 0` toggles the condition, so the red rectangle blinks on and off. Swap the time source for a mouse, slider, or key to drive it yourself.',
    { w: 420, h: 240 },
  );
  // sin(time) > 0 → a condition that auto-toggles, so the demo blinks on
  // load instead of needing mouse interaction.
  const t = node('t19_time', 'time/seconds', { x: 60, y: 0 });
  const sin = node('t19_sin', 'math/sin', { x: 240, y: 0 });
  const gt = node('t19_gt', 'logic/gt', { x: 420, y: 0 }, { b: 0 });

  const RID = 't19_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 620, y: 0 },
    data: {
      defType: 'region/if',
      params: { width: 460, height: 240, label: 'If' },
      ports: [
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 460, height: 240 },
  };
  const col = node('t19_col', 'color/color', { x: 40, y: 40 }, { mode: 'rgb', r: 220, g: 50, b: 50, a: 255 }, RID);
  const fill = node('t19_fill', 'style/fill', { x: 200, y: 40 }, {}, RID);
  const rect = node('t19_rect', 'draw/rect', { x: 240, y: 120 }, { x: -50, y: -50, w: 100, h: 100 }, RID);
  const renderer = node('t19_renderer', 'render/renderer', { x: 1140, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, t, sin, gt, region, col, fill, rect, renderer],
    edges: [
      edge('t19_time', 't', 't19_sin', 'x'),
      edge('t19_sin', 'sine', 't19_gt', 'a'),
      edge('t19_gt', 'result', RID, 'condition'),
      edge('t19_col', 'color', 't19_fill', 'color'),
      edge('t19_fill', 'style', 't19_rect', 'style'),
      edge('t19_rect', 'shape', RID, 'l'),
      edge(RID, 'l', 't19_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 20: Subpatch (reusable boxes) ────────────────────────────────
// Wrap a small "coloured rect at (x,y)" graph behind a subpatch with three
// inputs (x, y, hue) and one output (layer). Use it twice on the canvas.
export function buildTutSubpatch() {
  const c = note(
    't20_c',
    { x: -440, y: 0 },
    'Subpatches: your own reusable nodes\n\nA `meta/subpatch` is a box that hides a sub-graph behind custom input/output ports. Double-click the subpatch to descend into it; click the breadcrumb to come back.\n\nHere we built a "ColoredRect(x, y, hue)" subpatch and used it twice.',
    { w: 420, h: 240 },
  );

  const SID = 't20_sub';
  const sub: FloretNode = {
    id: SID,
    type: 'floret',
    position: { x: 60, y: 0 },
    data: {
      defType: 'meta/subpatch',
      params: { label: 'ColoredRect', width: 480, height: 260 },
      ports: [
        { id: 'x', name: 'x', type: 'number', direction: 'input', y: 40 },
        { id: 'y', name: 'y', type: 'number', direction: 'input', y: 80 },
        { id: 'hue', name: 'hue', type: 'number', direction: 'input', y: 120 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 480, height: 260 },
  };
  // Inside: hsl → fill → rect
  const col = node('t20_col', 'color/color', { x: 40, y: 40 }, { mode: 'hsl', s: 80, l: 55, a: 255 }, SID);
  const fill = node('t20_fill', 'style/fill', { x: 200, y: 40 }, {}, SID);
  const rect = node('t20_rect', 'draw/rect', { x: 240, y: 120 }, { w: 80, h: 80 }, SID);

  // Two callers
  const t = node('t20_t', 'time/seconds', { x: 60, y: 320 });
  const hueSpeed = node('t20_hueSpeed', 'math/mul', { x: 240, y: 320 }, { b: 40 });

  // Use the subpatch output, pair it with a sibling shape, group, render.
  const sib = node('t20_sib', 'draw/ellipse', { x: 700, y: 320 }, { x: 120, y: 0, w: 60, h: 60 });

  const grp = node('t20_grp', 'transform/group', { x: 700, y: 200 }, { slotCount: 2 });
  const renderer = node('t20_renderer', 'render/renderer', { x: 920, y: 200 }, { width: 400, height: 400 });

  // Outer drivers
  const x1 = node('t20_x1', 'value/number', { x: 60, y: 580 }, { value: -100 });
  const y1 = node('t20_y1', 'value/number', { x: 60, y: 640 }, { value: 0 });

  return {
    nodes: [c, sub, col, fill, rect, t, hueSpeed, sib, grp, renderer, x1, y1],
    edges: [
      // inside the subpatch: hue → Color.h → Fill → Rect.style; x,y → Rect.x,y
      edge(SID, 'hue', 't20_col', 'h'),
      edge('t20_col', 'color', 't20_fill', 'color'),
      edge('t20_fill', 'style', 't20_rect', 'style'),
      edge(SID, 'x', 't20_rect', 'x'),
      edge(SID, 'y', 't20_rect', 'y'),
      edge('t20_rect', 'shape', SID, 'l'),
      // outside: drive the subpatch
      edge('t20_t', 't', 't20_hueSpeed', 'a'),
      edge('t20_hueSpeed', 'product', SID, 'hue'),
      edge('t20_x1', 'number', SID, 'x'),
      edge('t20_y1', 'number', SID, 'y'),
      // Group the subpatch's output with a sibling ellipse and render
      edge(SID, 'l', 't20_grp', 's1'),
      edge('t20_sib', 'shape', 't20_grp', 's2'),
      edge('t20_grp', 'grouped', 't20_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 21: Trails (Write* / Read*) ──────────────────────────────────
// Append the current mouse position to a running buffer, draw the whole
// buffer as a smoothed path. The trail follows the cursor.
export function buildTutTrails() {
  const c = note(
    't21_c',
    { x: -440, y: 0 },
    'Trails: state + spread\n\nThe Write* / Read* pair stash a SPREAD between frames. Read* emits last frame\'s spread; we append today\'s mouse vec2 to it (Stack), then Write* keeps the new spread for tomorrow.\n\nDraw the buffer as a Path → the line trails the cursor.',
    { w: 420, h: 240 },
  );
  const m = node('t21_m', 'input/mouse', { x: 60, y: 0 });
  const mkVec = node('t21_mkVec', 'vec2/make', { x: 260, y: 0 });
  // wrap today\'s vec2 as a length-1 spread by feeding into a Range(1)→repeat… simpler: use spread/stack of [single] (which auto-wraps) with the previous buffer.
  const read = node('t21_read', 'state/read-spread', { x: 60, y: 200 }, { slot: 1 });
  const stack = node('t21_stack', 'spread/stack', { x: 460, y: 100 });
  // Trim: keep the LAST 80 points to bound memory.
  const take = node('t21_take', 'spread/slice', { x: 640, y: 100 }, { start: -80, end: 0 });
  const write = node('t21_write', 'state/write-spread', { x: 820, y: 200 }, { slot: 1 });

  // Render — Path through the buffer.
  const colN = node('t21_col', 'color/color', { x: 460, y: 320 }, { mode: 'rgb', r: 240, g: 90, b: 180, a: 255 });
  const path = node('t21_path', 'draw/path', { x: 820, y: 60 }, { strokeWidth: 3 });
  const renderer = node('t21_renderer', 'render/renderer', { x: 1040, y: 0 }, { width: 500, height: 500 });

  return {
    nodes: [c, m, mkVec, read, stack, take, write, colN, path, renderer],
    edges: [
      edge('t21_m', 'x', 't21_mkVec', 'x'),
      edge('t21_m', 'y', 't21_mkVec', 'y'),
      // prev buffer + today's vec2 → trimmed buffer
      edge('t21_read', 'current', 't21_stack', 'a'),
      edge('t21_mkVec', 'vector', 't21_stack', 'b'),
      edge('t21_stack', 'values', 't21_take', 'spread'),
      edge('t21_take', 'values', 't21_write', 'next'),
      // draw the buffer
      edge('t21_take', 'values', 't21_path', 'points'),
      edge('t21_col', 'color', 't21_path', 'color'),
      edge('t21_path', 'shape', 't21_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 22: Cache Region (performance) ───────────────────────────────
// A Cache region skips recompute when inputs haven't changed. Heavy
// expression INSIDE; cheap driver OUTSIDE.
export function buildTutCacheRegion() {
  const c = note(
    't22_c',
    { x: -440, y: 0 },
    'Cache: skip work when inputs are stable\n\n`region/cache` runs its body only when an input changes. Wrap an expensive computation inside it and the runtime returns the same output every tick until you nudge an input.\n\nHere: a 200-point Fibonacci scatter, cached. The Path is rebuilt only when `scale` changes.',
    { w: 420, h: 240 },
  );
  const scaleS = node('t22_scale', 'io/slider', { x: 60, y: 0 }, { value: 10, min: 4, max: 20 });

  const RID = 't22_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/cache',
      params: { width: 500, height: 240, label: 'Cache' },
      ports: [
        { id: 's', name: 'scale', type: 'number', direction: 'input', y: 40 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 500, height: 240 },
  };
  const fib = nodeWithPorts('t22_fib', 'spread/fibonacci', { x: 40, y: 40 }, { count: 200, phase: 0 }, RID);
  const col = node('t22_col', 'color/color', { x: 220, y: 120 }, { mode: 'rgb', r: 90, g: 200, b: 200, a: 255 }, RID);
  const poly = node('t22_poly', 'draw/polygon', { x: 360, y: 40 }, { closed: false }, RID);
  const fill = node('t22_fill', 'style/fill', { x: 360, y: 120 }, {}, RID);

  const renderer = node('t22_renderer', 'render/renderer', { x: 880, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, scaleS, region, fib, col, poly, fill, renderer],
    edges: [
      edge('t22_scale', 'number', RID, 's'),
      // inside: fib scale is wired so the cache invalidates with scaleS
      edge(RID, 's', 't22_fib', 'scale'),
      edge('t22_fib', 'positions', 't22_poly', 'points'),
      edge('t22_col', 'color', 't22_fill', 'color'),
      edge('t22_fill', 'style', 't22_poly', 'style'),
      edge('t22_poly', 'shape', RID, 'l'),
      edge(RID, 'l', 't22_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 23: Polygon from Points ──────────────────────────────────────
// Same Spread<vec2> input — but as ONE polygon shape, not N ellipses.
export function buildTutPolygon() {
  const c = note(
    't23_c',
    { x: -400, y: 0 },
    'Polygons from a spread\n\n`draw/polygon` consumes a Spread<vec2> as one shape (not N shapes — no ForEach needed). Use it whenever you want a single outline that connects the dots.\n\nA Fibonacci sunflower drawn as one closed polygon.',
    { w: 380, h: 220 },
  );
  const fib = nodeWithPorts('t23_fib', 'spread/fibonacci', { x: 60, y: 0 }, { count: 120, scale: 14, phase: 0 });
  const col = node('t23_col', 'color/color', { x: 60, y: 200 }, { mode: 'rgb', r: 240, g: 200, b: 80, a: 255 });
  const colS = node('t23_colS', 'color/color', { x: 60, y: 320 }, { mode: 'rgb', r: 30, g: 30, b: 30, a: 255 });
  const fill = node('t23_fill', 'style/fill', { x: 260, y: 200 });
  const stroke = node('t23_stroke', 'style/stroke', { x: 460, y: 260 }, { width: 1 });
  const poly = node('t23_poly', 'draw/polygon', { x: 460, y: 60 }, { closed: true });
  const renderer = node('t23_renderer', 'render/renderer', { x: 720, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, fib, col, colS, fill, stroke, poly, renderer],
    edges: [
      edge('t23_fib', 'positions', 't23_poly', 'points'),
      edge('t23_col', 'color', 't23_fill', 'color'),
      edge('t23_colS', 'color', 't23_stroke', 'color'),
      edge('t23_fill', 'style', 't23_stroke', 'base'),
      edge('t23_stroke', 'style', 't23_poly', 'style'),
      edge('t23_poly', 'shape', 't23_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 24: Path with Tension (Catmull–Rom) ──────────────────────────
// Tension > 0 turns straight segments into smooth Catmull-Rom curves.
export function buildTutPathTension() {
  const c = note(
    't24_c',
    { x: -400, y: 0 },
    'Curved paths\n\n`draw/path`\'s `tension` input smooths the polyline through your points. tension = 0 → straight segments; tension → 0.5 gives Catmull-Rom curves.\n\nTime drives the tension so the wave breathes between sharp and smooth.',
    { w: 380, h: 220 },
  );
  // 12 points along sin(x) for a wavy guide
  const range = nodeWithPorts('t24_range', 'spread/range', { x: 60, y: 0 }, { count: 12 });

  const RID = 't24_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 280, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 480, height: 220, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
        { id: 'p', name: 'p', type: 'vec2', direction: 'output', y: 60 },
      ],
    },
    style: { width: 480, height: 220 },
  };
  // x = i*40 - 220 ; y = sin(i*0.7) * 80
  const xMul = node('t24_xMul', 'math/mul', { x: 40, y: 40 }, { b: 40 }, RID);
  const xOff = node('t24_xOff', 'math/add', { x: 200, y: 40 }, { b: -220 }, RID);
  const yIn = node('t24_yIn', 'math/mul', { x: 40, y: 120 }, { b: 0.7 }, RID);
  const ySin = node('t24_ySin', 'math/sin', { x: 200, y: 120 }, {}, RID);
  const yMul = node('t24_yMul', 'math/mul', { x: 360, y: 120 }, { b: 80 }, RID);
  const mkVec = node('t24_mkVec', 'vec2/make', { x: 360, y: 60 }, {}, RID);

  // tension = sin(time)*0.25 + 0.25
  const t = node('t24_t', 'time/seconds', { x: 60, y: 280 });
  const tSin = node('t24_tSin', 'math/sin', { x: 240, y: 280 });
  const tHalf = node('t24_tHalf', 'math/mul', { x: 420, y: 280 }, { b: 0.25 });
  const tOff = node('t24_tOff', 'math/add', { x: 600, y: 280 }, { b: 0.25 });

  const col = node('t24_col', 'color/color', { x: 60, y: 400 }, { mode: 'rgb', r: 90, g: 200, b: 90, a: 255 });
  const path = node('t24_path', 'draw/path', { x: 820, y: 60 }, { strokeWidth: 4 });
  const renderer = node('t24_renderer', 'render/renderer', { x: 1040, y: 0 }, { width: 500, height: 400 });

  return {
    nodes: [c, range, region, xMul, xOff, yIn, ySin, yMul, mkVec, t, tSin, tHalf, tOff, col, path, renderer],
    edges: [
      edge('t24_range', 'indices', RID, 'i'),
      edge(RID, 'i', 't24_xMul', 'a'),
      edge('t24_xMul', 'product', 't24_xOff', 'a'),
      edge('t24_xOff', 'sum', 't24_mkVec', 'x'),
      edge(RID, 'i', 't24_yIn', 'a'),
      edge('t24_yIn', 'product', 't24_ySin', 'x'),
      edge('t24_ySin', 'sine', 't24_yMul', 'a'),
      edge('t24_yMul', 'product', 't24_mkVec', 'y'),
      edge('t24_mkVec', 'vector', RID, 'p'),
      // tension oscillates
      edge('t24_t', 't', 't24_tSin', 'x'),
      edge('t24_tSin', 'sine', 't24_tHalf', 'a'),
      edge('t24_tHalf', 'product', 't24_tOff', 'a'),
      // draw
      edge(RID, 'p', 't24_path', 'points'),
      edge('t24_tOff', 'sum', 't24_path', 'tension'),
      edge('t24_col', 'color', 't24_path', 'color'),
      edge('t24_path', 'shape', 't24_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 25: Spring-follow Mouse ──────────────────────────────────────
// Physics: pos += vel; vel += (mouse - pos)*k - vel*damping; per frame.
export function buildTutSpringFollow() {
  const c = note(
    't25_c',
    { x: -460, y: 0 },
    'Spring physics with state\n\nA position chases the mouse with Hooke + damping. Two state slots (vec2): position and velocity. Each frame:\n  • a = (mouse − pos) · k − vel · damp\n  • vel ← vel + a\n  • pos ← pos + vel\n\nWrite/Read are used twice — one for pos, one for vel.',
    { w: 440, h: 240 },
  );
  const m = node('t25_m', 'input/mouse', { x: 60, y: 0 });
  const mVec = node('t25_mVec', 'vec2/make', { x: 240, y: 0 });

  // pos read
  const posR = node('t25_posR', 'state/read', { x: 60, y: 160 }, { slot: 1, init: { x: 0, y: 0 } });
  // vel read
  const velR = node('t25_velR', 'state/read', { x: 60, y: 240 }, { slot: 2, init: { x: 0, y: 0 } });

  // Split mouse and pos so we can do vec subtraction component-wise
  const mS = node('t25_mS', 'vec2/split', { x: 420, y: 0 });
  const pS = node('t25_pS', 'vec2/split', { x: 420, y: 160 });
  const vS = node('t25_vS', 'vec2/split', { x: 420, y: 240 });

  // dx = (mx - px) * k
  const k = node('t25_k', 'value/number', { x: 60, y: 380 }, { value: 0.05 });
  const damp = node('t25_damp', 'value/number', { x: 60, y: 440 }, { value: 0.85 });
  // ax = (mx - px) * k
  const subX = node('t25_subX', 'math/sub', { x: 600, y: 80 });
  const subY = node('t25_subY', 'math/sub', { x: 600, y: 140 });
  const accX = node('t25_accX', 'math/mul', { x: 760, y: 80 });
  const accY = node('t25_accY', 'math/mul', { x: 760, y: 140 });
  // dampedVx = vx * damp
  const dvX = node('t25_dvX', 'math/mul', { x: 600, y: 220 });
  const dvY = node('t25_dvY', 'math/mul', { x: 600, y: 280 });
  // vx' = dampedVx + ax
  const nvX = node('t25_nvX', 'math/add', { x: 920, y: 160 });
  const nvY = node('t25_nvY', 'math/add', { x: 920, y: 220 });
  // px' = px + vx'
  const npX = node('t25_npX', 'math/add', { x: 1080, y: 80 });
  const npY = node('t25_npY', 'math/add', { x: 1080, y: 140 });
  // make vec2s for writeback
  const nP = node('t25_nP', 'vec2/make', { x: 1240, y: 100 });
  const nV = node('t25_nV', 'vec2/make', { x: 1240, y: 200 });
  const posW = node('t25_posW', 'state/write', { x: 1420, y: 100 }, { slot: 1 });
  const velW = node('t25_velW', 'state/write', { x: 1420, y: 200 }, { slot: 2 });

  // render: ellipse at position
  const ell = node('t25_ell', 'draw/ellipse', { x: 1080, y: 320 }, { w: 30, h: 30 });
  const renderer = node('t25_renderer', 'render/renderer', { x: 1620, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [
      c, m, mVec, posR, velR, mS, pS, vS, k, damp,
      subX, subY, accX, accY, dvX, dvY, nvX, nvY, npX, npY,
      nP, nV, posW, velW, ell, renderer,
    ],
    edges: [
      edge('t25_m', 'x', 't25_mVec', 'x'),
      edge('t25_m', 'y', 't25_mVec', 'y'),
      edge('t25_mVec', 'vector', 't25_mS', 'v'),
      edge('t25_posR', 'current', 't25_pS', 'v'),
      edge('t25_velR', 'current', 't25_vS', 'v'),
      // accel
      edge('t25_mS', 'x', 't25_subX', 'a'),
      edge('t25_pS', 'x', 't25_subX', 'b'),
      edge('t25_mS', 'y', 't25_subY', 'a'),
      edge('t25_pS', 'y', 't25_subY', 'b'),
      edge('t25_subX', 'difference', 't25_accX', 'a'),
      edge('t25_k', 'number', 't25_accX', 'b'),
      edge('t25_subY', 'difference', 't25_accY', 'a'),
      edge('t25_k', 'number', 't25_accY', 'b'),
      // damped vel
      edge('t25_vS', 'x', 't25_dvX', 'a'),
      edge('t25_damp', 'number', 't25_dvX', 'b'),
      edge('t25_vS', 'y', 't25_dvY', 'a'),
      edge('t25_damp', 'number', 't25_dvY', 'b'),
      // new vel
      edge('t25_dvX', 'product', 't25_nvX', 'a'),
      edge('t25_accX', 'product', 't25_nvX', 'b'),
      edge('t25_dvY', 'product', 't25_nvY', 'a'),
      edge('t25_accY', 'product', 't25_nvY', 'b'),
      // new pos
      edge('t25_pS', 'x', 't25_npX', 'a'),
      edge('t25_nvX', 'sum', 't25_npX', 'b'),
      edge('t25_pS', 'y', 't25_npY', 'a'),
      edge('t25_nvY', 'sum', 't25_npY', 'b'),
      // writeback
      edge('t25_npX', 'sum', 't25_nP', 'x'),
      edge('t25_npY', 'sum', 't25_nP', 'y'),
      edge('t25_nvX', 'sum', 't25_nV', 'x'),
      edge('t25_nvY', 'sum', 't25_nV', 'y'),
      edge('t25_nP', 'vector', 't25_posW', 'next'),
      edge('t25_nV', 'vector', 't25_velW', 'next'),
      // render
      edge('t25_npX', 'sum', 't25_ell', 'x'),
      edge('t25_npY', 'sum', 't25_ell', 'y'),
      edge('t25_ell', 'shape', 't25_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 26: Render-to-image (Rasterize) ──────────────────────────────
// Convert a vector layer into a bitmap with image/from-layer.
export function buildTutRasterize() {
  const c = note(
    't26_c',
    { x: -440, y: 0 },
    'Rasterising layers to bitmaps\n\n`image/from-layer` (Rasterize) draws a layer onto an offscreen canvas and outputs it as an image. Once you\'re in image-land you can sample, filter, blend, save, …\n\nHere: a fibonacci scatter rasterised at 256×256 and then displayed back via draw/image.',
    { w: 420, h: 240 },
  );
  const fib = nodeWithPorts('t26_fib', 'spread/fibonacci', { x: 60, y: 0 }, { count: 200, scale: 10, phase: 0 });
  const col = node('t26_col', 'color/color', { x: 60, y: 200 }, { mode: 'rgb', r: 250, g: 200, b: 60, a: 255 });
  const fill = node('t26_fill', 'style/fill', { x: 260, y: 200 });
  const poly = node('t26_poly', 'draw/polygon', { x: 260, y: 0 }, { closed: false });

  const rast = node('t26_rast', 'image/from-layer', { x: 460, y: 60 }, { w: 256, h: 256 });
  const drawImg = node('t26_draw', 'draw/image', { x: 640, y: 60 }, { x: 0, y: 0, w: 200, h: 200 });
  const renderer = node('t26_renderer', 'render/renderer', { x: 860, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, fib, col, fill, poly, rast, drawImg, renderer],
    edges: [
      edge('t26_fib', 'positions', 't26_poly', 'points'),
      edge('t26_col', 'color', 't26_fill', 'color'),
      edge('t26_fill', 'style', 't26_poly', 'style'),
      edge('t26_poly', 'shape', 't26_rast', 'layer'),
      edge('t26_rast', 'image', 't26_draw', 'image'),
      edge('t26_draw', 'layer', 't26_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 27: Image Sampling Grid ──────────────────────────────────────
// Sample colours from a rasterised image at grid points → recolour a dot grid.
export function buildTutImageSample() {
  const c = note(
    't27_c',
    { x: -460, y: 0 },
    'Image sampling\n\n`image/sample-spread` reads N colours from an image at N points. Combine with a grid generator and you get a "downsample" pipeline — perfect for posterising or pixel-art looks.\n\nA noisy gradient rasterised, then sampled at 24×24 grid centres.',
    { w: 440, h: 240 },
  );
  // Build a colourful source layer to sample from
  const t = node('t27_t', 'time/seconds', { x: 60, y: 0 });
  const colA = node('t27_colA', 'color/color', { x: 60, y: 120 }, { mode: 'hsl', s: 80, l: 55, a: 255 });
  const fillA = node('t27_fillA', 'style/fill', { x: 240, y: 120 });
  const rectA = node('t27_rectA', 'draw/rect', { x: 420, y: 120 }, { x: -150, y: -150, w: 300, h: 300 });
  const hueDrive = node('t27_hueDrive', 'math/mul', { x: 60, y: 60 }, { b: 60 });

  const rast = node('t27_rast', 'image/from-layer', { x: 600, y: 120 }, { w: 256, h: 256 });

  // grid of points (image space: 0..256)
  const grid = nodeWithPorts('t27_grid', 'spread/grid', { x: 60, y: 320 }, { cols: 24, rows: 24, w: 256, h: 256, cx: 128, cy: 128 });
  const samp = node('t27_samp', 'image/sample-spread', { x: 800, y: 200 });

  // ForEach: per (cell, colour) → draw dot at canvas coords
  const gridScreen = nodeWithPorts('t27_gridScreen', 'spread/grid', { x: 60, y: 440 }, { cols: 24, rows: 24, w: 360, h: 360, cx: 0, cy: 0 });

  const RID = 't27_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 1000, y: 80 },
    data: {
      defType: 'region/foreach',
      params: { width: 500, height: 320, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input', y: 40 },
        { id: 'col', name: 'col', type: 'color', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 500, height: 320 },
  };
  const split = node('t27_split', 'vec2/split', { x: 40, y: 40 }, {}, RID);
  const fill = node('t27_fill', 'style/fill', { x: 40, y: 180 }, {}, RID);
  const rect = node('t27_rect', 'draw/rect', { x: 260, y: 80 }, { w: 16, h: 16 }, RID);

  const renderer = node('t27_renderer', 'render/renderer', { x: 1560, y: 0 }, { width: 500, height: 500 });

  return {
    nodes: [
      c, t, colA, fillA, rectA, hueDrive, rast, grid, samp, gridScreen,
      region, split, fill, rect, renderer,
    ],
    edges: [
      // build source
      edge('t27_t', 't', 't27_hueDrive', 'a'),
      edge('t27_hueDrive', 'product', 't27_colA', 'h'),
      edge('t27_colA', 'color', 't27_fillA', 'color'),
      edge('t27_fillA', 'style', 't27_rectA', 'style'),
      edge('t27_rectA', 'shape', 't27_rast', 'layer'),
      // sample
      edge('t27_rast', 'image', 't27_samp', 'image'),
      edge('t27_grid', 'positions', 't27_samp', 'points'),
      // foreach (screen-grid positions + sampled colours)
      edge('t27_gridScreen', 'positions', RID, 'p'),
      edge('t27_samp', 'colors', RID, 'col'),
      // inside
      edge(RID, 'p', 't27_split', 'v'),
      edge('t27_split', 'x', 't27_rect', 'x'),
      edge('t27_split', 'y', 't27_rect', 'y'),
      edge(RID, 'col', 't27_fill', 'color'),
      edge('t27_fill', 'style', 't27_rect', 'style'),
      edge('t27_rect', 'shape', RID, 'l'),
      edge(RID, 'l', 't27_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 28: Audio-Reactive Visual ────────────────────────────────────
// Microphone amplitude drives a circle's radius. Needs mic permission.
export function buildTutAudioReactive() {
  const c = note(
    't28_c',
    { x: -460, y: 0 },
    'Audio-reactive shapes\n\n`sound/audio-in` taps the microphone; `sound/amplitude` measures the RMS level. Map the level to a number you can wire into any shape input.\n\nMake noise → the circle grows.\n\n(You\'ll need to grant the browser microphone permission.)',
    { w: 440, h: 240 },
  );
  const start = node('t28_start', 'value/number', { x: 60, y: 0 }, { value: 1 });
  const mic = node('t28_mic', 'sound/audio-in', { x: 240, y: 0 }, { amp: 1 });
  const amp = node('t28_amp', 'sound/amplitude', { x: 440, y: 0 }, { smoothing: 0.7 });
  // size = amp * 400 + 30
  const sz = node('t28_sz', 'math/mul', { x: 620, y: 0 }, { b: 400 });
  const szO = node('t28_szO', 'math/add', { x: 800, y: 0 }, { b: 30 });
  const ell = node('t28_ell', 'draw/ellipse', { x: 980, y: 40 });
  const renderer = node('t28_renderer', 'render/renderer', { x: 1200, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, start, mic, amp, sz, szO, ell, renderer],
    edges: [
      edge('t28_start', 'number', 't28_mic', 'start'),
      edge('t28_mic', 'audio', 't28_amp', 'in'),
      edge('t28_amp', 'level', 't28_sz', 'a'),
      edge('t28_sz', 'product', 't28_szO', 'a'),
      edge('t28_szO', 'sum', 't28_ell', 'w'),
      edge('t28_szO', 'sum', 't28_ell', 'h'),
      edge('t28_ell', 'shape', 't28_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 29: Sound Oscillator (Mouse-driven) ──────────────────────────
// mouseY → frequency. A sine wave plays through the speakers.
export function buildTutOscillator() {
  const c = note(
    't29_c',
    { x: -440, y: 0 },
    'Generating sound\n\n`sound/oscillator` produces a waveform; `sound/output` sends it to the speakers. Wire mouseY through a `math/map` and the pitch slides as you move the cursor.\n\nThe big ellipse is just visual feedback for how loud it is.',
    { w: 420, h: 240 },
  );
  const m = node('t29_m', 'input/mouse', { x: 60, y: 0 });
  const freq = node('t29_freq', 'math/map', { x: 260, y: 0 }, { inMin: -200, inMax: 200, outMin: 800, outMax: 100 });
  const osc = node('t29_osc', 'sound/oscillator', { x: 460, y: 0 }, { type: 'sine', amp: 0.2 });
  const out = node('t29_out', 'sound/output', { x: 640, y: 0 }, { volume: 0.5 });

  // visual side
  const amp = node('t29_amp', 'sound/amplitude', { x: 460, y: 200 }, { smoothing: 0.5 });
  const sz = node('t29_sz', 'math/mul', { x: 640, y: 200 }, { b: 600 });
  const szO = node('t29_szO', 'math/add', { x: 820, y: 200 }, { b: 40 });
  const ell = node('t29_ell', 'draw/ellipse', { x: 1000, y: 200 });
  const renderer = node('t29_renderer', 'render/renderer', { x: 1220, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, m, freq, osc, out, amp, sz, szO, ell, renderer],
    edges: [
      edge('t29_m', 'y', 't29_freq', 'x'),
      edge('t29_freq', 'mapped', 't29_osc', 'freq'),
      edge('t29_osc', 'audio', 't29_out', 'in'),
      // visual: split audio between speakers and amplitude
      edge('t29_osc', 'audio', 't29_amp', 'in'),
      edge('t29_amp', 'level', 't29_sz', 'a'),
      edge('t29_sz', 'product', 't29_szO', 'a'),
      edge('t29_szO', 'sum', 't29_ell', 'w'),
      edge('t29_szO', 'sum', 't29_ell', 'h'),
      edge('t29_ell', 'shape', 't29_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 30: Keyboard Branching ───────────────────────────────────────
// Two If regions gate by a string equality on input/keyboard.key.
export function buildTutKeyboardBranch() {
  const c = note(
    't30_c',
    { x: -460, y: 0 },
    'Branching on keyboard input\n\n`input/keyboard.key` outputs the last key pressed (e.g. "a", "ArrowLeft"). `logic/eq` checks equality against a constant string. Wire the result into a `region/if` and you get key-driven branches.\n\nPress R for red rect, B for blue ellipse — others draw nothing.',
    { w: 440, h: 240 },
  );
  const kb = node('t30_kb', 'input/keyboard', { x: 60, y: 0 });
  const rKey = node('t30_rKey', 'value/string', { x: 60, y: 180 }, { value: 'r' });
  const bKey = node('t30_bKey', 'value/string', { x: 60, y: 340 }, { value: 'b' });
  const isR = node('t30_isR', 'logic/eq', { x: 240, y: 80 });
  const isB = node('t30_isB', 'logic/eq', { x: 240, y: 280 });

  // If-region "R"
  const R_ID = 't30_R';
  const regionR: FloretNode = {
    id: R_ID,
    type: 'floret',
    position: { x: 480, y: 0 },
    data: {
      defType: 'region/if',
      params: { width: 360, height: 200, label: 'If R' },
      ports: [
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 360, height: 200 },
  };
  const colR = node('t30_colR', 'color/color', { x: 40, y: 40 }, { mode: 'rgb', r: 230, g: 60, b: 60, a: 255 }, R_ID);
  const fillR = node('t30_fillR', 'style/fill', { x: 180, y: 40 }, {}, R_ID);
  const rectR = node('t30_rectR', 'draw/rect', { x: 220, y: 120 }, { x: -80, y: -80, w: 160, h: 160 }, R_ID);

  // If-region "B"
  const B_ID = 't30_B';
  const regionB: FloretNode = {
    id: B_ID,
    type: 'floret',
    position: { x: 480, y: 240 },
    data: {
      defType: 'region/if',
      params: { width: 360, height: 200, label: 'If B' },
      ports: [
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 360, height: 200 },
  };
  const colB = node('t30_colB', 'color/color', { x: 40, y: 40 }, { mode: 'rgb', r: 60, g: 130, b: 230, a: 255 }, B_ID);
  const fillB = node('t30_fillB', 'style/fill', { x: 180, y: 40 }, {}, B_ID);
  const ellB = node('t30_ellB', 'draw/ellipse', { x: 220, y: 120 }, { w: 200, h: 200 }, B_ID);

  const grp = node('t30_grp', 'transform/group', { x: 900, y: 120 }, { slotCount: 2 });
  const renderer = node('t30_renderer', 'render/renderer', { x: 1120, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [
      c, kb, rKey, bKey, isR, isB,
      regionR, colR, fillR, rectR,
      regionB, colB, fillB, ellB,
      grp, renderer,
    ],
    edges: [
      edge('t30_kb', 'key', 't30_isR', 'a'),
      edge('t30_rKey', 'text', 't30_isR', 'b'),
      edge('t30_kb', 'key', 't30_isB', 'a'),
      edge('t30_bKey', 'text', 't30_isB', 'b'),
      edge('t30_isR', 'result', R_ID, 'condition'),
      edge('t30_isB', 'result', B_ID, 'condition'),
      // inside R
      edge('t30_colR', 'color', 't30_fillR', 'color'),
      edge('t30_fillR', 'style', 't30_rectR', 'style'),
      edge('t30_rectR', 'shape', R_ID, 'l'),
      // inside B
      edge('t30_colB', 'color', 't30_fillB', 'color'),
      edge('t30_fillB', 'style', 't30_ellB', 'style'),
      edge('t30_ellB', 'shape', B_ID, 'l'),
      // group + render
      edge(R_ID, 'l', 't30_grp', 's1'),
      edge(B_ID, 'l', 't30_grp', 's2'),
      edge('t30_grp', 'grouped', 't30_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 31: Vec2 Math ────────────────────────────────────────────────
// Operate on vec2s directly with vec2/add, vec2/scale — no splitting.
export function buildTutVec2Math() {
  const c = note(
    't31_c',
    { x: -440, y: 0 },
    'Vec2 math (no splitting)\n\n`vec2/*` nodes operate on vec2 as one unit. Add, Scale, Length, Normalize all take vec2 inputs and produce vec2 outputs — much cleaner than splitting and recombining.\n\nMake a base vec2, scale it by sin(time), draw an ellipse there.',
    { w: 420, h: 220 },
  );
  const base = node('t31_base', 'vec2/make', { x: 60, y: 0 }, { x: 150, y: 0 });
  const t = node('t31_t', 'time/seconds', { x: 60, y: 160 });
  const sN = node('t31_s', 'math/sin', { x: 240, y: 160 });
  const sc = node('t31_sc', 'vec2/scale', { x: 440, y: 60 });
  const sp = node('t31_sp', 'vec2/split', { x: 620, y: 60 });
  const ell = node('t31_ell', 'draw/ellipse', { x: 800, y: 100 }, { w: 50, h: 50 });
  const renderer = node('t31_renderer', 'render/renderer', { x: 1020, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, base, t, sN, sc, sp, ell, renderer],
    edges: [
      edge('t31_base', 'vector', 't31_sc', 'v'),
      edge('t31_t', 't', 't31_s', 'x'),
      edge('t31_s', 'sine', 't31_sc', 's'),
      edge('t31_sc', 'scaled', 't31_sp', 'v'),
      edge('t31_sp', 'x', 't31_ell', 'x'),
      edge('t31_sp', 'y', 't31_ell', 'y'),
      edge('t31_ell', 'shape', 't31_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 32: Normalize + Polar ────────────────────────────────────────
// vec2/normalize turns any direction into a unit vector. Multiply by length
// → put an arrow at that distance.
export function buildTutNormalize() {
  const c = note(
    't32_c',
    { x: -440, y: 0 },
    'Unit vectors\n\n`vec2/normalize` divides by length so the result has magnitude 1. Useful for "give me the DIRECTION but not the SIZE", then scale by whatever length you want.\n\nA unit vector from origin to mouse, then scaled to a fixed radius — an arrow that always reaches the same distance.',
    { w: 420, h: 220 },
  );
  const m = node('t32_m', 'input/mouse', { x: 60, y: 0 });
  const mv = node('t32_mv', 'vec2/make', { x: 240, y: 0 });
  const n = node('t32_n', 'vec2/normalize', { x: 420, y: 0 });
  const sc = node('t32_sc', 'vec2/scale', { x: 600, y: 0 }, { s: 180 });
  const sp = node('t32_sp', 'vec2/split', { x: 780, y: 0 });
  const line = node('t32_line', 'draw/line', { x: 960, y: 40 }, { x1: 0, y1: 0 });
  const renderer = node('t32_renderer', 'render/renderer', { x: 1180, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, m, mv, n, sc, sp, line, renderer],
    edges: [
      edge('t32_m', 'x', 't32_mv', 'x'),
      edge('t32_m', 'y', 't32_mv', 'y'),
      edge('t32_mv', 'vector', 't32_n', 'v'),
      edge('t32_n', 'normalized', 't32_sc', 'v'),
      edge('t32_sc', 'scaled', 't32_sp', 'v'),
      edge('t32_sp', 'x', 't32_line', 'x2'),
      edge('t32_sp', 'y', 't32_line', 'y2'),
      edge('t32_line', 'shape', 't32_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 33: Gradient Sampling ────────────────────────────────────────
// color/gradient samples a Spread<Color> at t to get a single colour.
export function buildTutGradient() {
  const c = note(
    't33_c',
    { x: -460, y: 0 },
    'Gradients = interpolated palettes\n\n`color/gradient` reads a Spread<Color> and a t ∈ [0, 1]; it linearly interpolates between adjacent stops. Smoother than CycleSpread when you want a continuous transition.\n\nMouseX → t → sampled gradient → fill colour.',
    { w: 440, h: 220 },
  );
  const pal = node('t33_pal', 'color/palette', { x: 60, y: 0 }, { stopCount: 5 });
  const m = node('t33_m', 'input/mouse', { x: 60, y: 280 });
  const tmap = node('t33_tmap', 'math/map', { x: 260, y: 280 }, { inMin: -200, inMax: 200, outMin: 0, outMax: 1 });
  const grad = node('t33_grad', 'color/gradient', { x: 460, y: 80 });
  const fill = node('t33_fill', 'style/fill', { x: 640, y: 80 });
  const rect = node('t33_rect', 'draw/rect', { x: 820, y: 120 }, { x: -150, y: -150, w: 300, h: 300 });
  const renderer = node('t33_renderer', 'render/renderer', { x: 1040, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, pal, m, tmap, grad, fill, rect, renderer],
    edges: [
      edge('t33_pal', 'colors', 't33_grad', 'colors'),
      edge('t33_m', 'x', 't33_tmap', 'x'),
      edge('t33_tmap', 'mapped', 't33_grad', 't'),
      edge('t33_grad', 'color', 't33_fill', 'color'),
      edge('t33_fill', 'style', 't33_rect', 'style'),
      edge('t33_rect', 'shape', 't33_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 34: Draw Text ────────────────────────────────────────────────
// Text layer with a fixed string and a fixed size.
export function buildTutText() {
  const c = note(
    't34_c',
    { x: -420, y: 0 },
    'Text on the canvas\n\n`draw/text` is just another draw node — it has x, y, size, and style inputs like any shape.\n\nDouble-click the node to edit its `string` param, or wire a value/string into it from outside.',
    { w: 400, h: 200 },
  );
  const colN = node('t34_col', 'color/color', { x: 60, y: 0 }, { mode: 'rgb', r: 230, g: 240, b: 250, a: 255 });
  const fill = node('t34_fill', 'style/fill', { x: 240, y: 0 });
  const txt = node('t34_text', 'draw/text', { x: 440, y: 40 }, { string: 'hello, floret', x: -120, y: 0, size: 28, align: 'left' });
  const renderer = node('t34_renderer', 'render/renderer', { x: 700, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, colN, fill, txt, renderer],
    edges: [
      edge('t34_col', 'color', 't34_fill', 'color'),
      edge('t34_fill', 'style', 't34_text', 'style'),
      edge('t34_text', 'layer', 't34_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 35: Wanderer (random drift) ──────────────────────────────────
// random/wanderer2d gives smooth Perlin-driven random motion.
export function buildTutWanderer() {
  const c = note(
    't35_c',
    { x: -460, y: 0 },
    'Smooth random drift\n\n`random/wanderer2d` is a vec2 source driven by time — same idea as Perlin, but it gives you a coordinate that wanders smoothly around an area.\n\nIdeal for "natural" motion: floating particles, eyes, idle animations.',
    { w: 440, h: 220 },
  );
  const t = node('t35_t', 'time/seconds', { x: 60, y: 0 });
  const w = node('t35_w', 'random/wanderer2d', { x: 240, y: 0 }, { speed: 0.5, area: 200, seed: 1 });
  const sp = node('t35_sp', 'vec2/split', { x: 440, y: 0 });
  const ell = node('t35_ell', 'draw/ellipse', { x: 640, y: 40 }, { w: 60, h: 60 });
  const renderer = node('t35_renderer', 'render/renderer', { x: 860, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, t, w, sp, ell, renderer],
    edges: [
      edge('t35_t', 't', 't35_w', 't'),
      edge('t35_w', 'position', 't35_sp', 'v'),
      edge('t35_sp', 'x', 't35_ell', 'x'),
      edge('t35_sp', 'y', 't35_ell', 'y'),
      edge('t35_ell', 'shape', 't35_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 36: Gaussian Scatter ─────────────────────────────────────────
// Bell-curve random placement — denser near centre, thinner at edges.
export function buildTutGaussian() {
  const c = note(
    't36_c',
    { x: -460, y: 0 },
    'Gaussian (bell-curve) random\n\n`random/gaussian` clusters samples around `mean` with standard deviation `sd`. Unlike Uniform (flat), most samples land near the mean — perfect for "natural" scatter.\n\n200 dots placed with mean=0, sd=80. See the cluster in the centre.',
    { w: 440, h: 220 },
  );
  const range = nodeWithPorts('t36_range', 'spread/range', { x: 60, y: 0 }, { count: 200 });

  const RID = 't36_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 540, height: 280, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 540, height: 280 },
  };
  const seedX = node('t36_sx', 'math/add', { x: 40, y: 40 }, { b: 0 }, RID);
  const seedY = node('t36_sy', 'math/add', { x: 40, y: 100 }, { b: 1000 }, RID);
  const gx = node('t36_gx', 'random/gaussian', { x: 200, y: 40 }, { mean: 0, sd: 80 }, RID);
  const gy = node('t36_gy', 'random/gaussian', { x: 200, y: 100 }, { mean: 0, sd: 80 }, RID);
  const ell = node('t36_ell', 'draw/ellipse', { x: 360, y: 80 }, { w: 8, h: 8 }, RID);

  const renderer = node('t36_renderer', 'render/renderer', { x: 900, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, range, region, seedX, seedY, gx, gy, ell, renderer],
    edges: [
      edge('t36_range', 'indices', RID, 'i'),
      edge(RID, 'i', 't36_sx', 'a'),
      edge(RID, 'i', 't36_sy', 'a'),
      edge('t36_sx', 'sum', 't36_gx', 'seed'),
      edge('t36_sy', 'sum', 't36_gy', 'seed'),
      edge('t36_gx', 'sample', 't36_ell', 'x'),
      edge('t36_gy', 'sample', 't36_ell', 'y'),
      edge('t36_ell', 'shape', RID, 'l'),
      edge(RID, 'l', 't36_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 37: Image from URL ───────────────────────────────────────────
export function buildTutImageUrl() {
  const c = note(
    't37_c',
    { x: -440, y: 0 },
    'Loading images\n\n`io/image-url` fetches an image asynchronously. Once it lands, the `image` output carries a canvas you can draw, sample, or filter.\n\nDouble-click the node to change the URL. Picsum returns a fresh photo per seed.',
    { w: 420, h: 200 },
  );
  const url = node('t37_url', 'io/image-url', { x: 60, y: 0 }, { url: 'https://picsum.photos/seed/floret/500/500' });
  const drawImg = node('t37_draw', 'draw/image', { x: 280, y: 40 }, { x: 0, y: 0, w: 320, h: 320 });
  const renderer = node('t37_renderer', 'render/renderer', { x: 540, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, url, drawImg, renderer],
    edges: [
      edge('t37_url', 'image', 't37_draw', 'image'),
      edge('t37_draw', 'layer', 't37_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 38: Image Adjust ─────────────────────────────────────────────
export function buildTutImageAdjust() {
  const c = note(
    't38_c',
    { x: -440, y: 0 },
    'Image adjustments\n\nImage filter nodes are pipelines: image in, image out. Chain them like styles.\n\nload → adjust (brightness/contrast/saturation/hue) → draw. Tweak the inputs to see the change immediately.',
    { w: 420, h: 200 },
  );
  const url = node('t38_url', 'io/image-url', { x: 60, y: 0 }, { url: 'https://picsum.photos/seed/floret-adjust/500/500' });
  const adj = node('t38_adj', 'image/adjust', { x: 280, y: 0 }, { brightness: 1.2, contrast: 1.3, saturate: 1.5, hue: 30 });
  const drawImg = node('t38_draw', 'draw/image', { x: 520, y: 40 }, { x: 0, y: 0, w: 320, h: 320 });
  const renderer = node('t38_renderer', 'render/renderer', { x: 780, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, url, adj, drawImg, renderer],
    edges: [
      edge('t38_url', 'image', 't38_adj', 'image'),
      edge('t38_adj', 'image', 't38_draw', 'image'),
      edge('t38_draw', 'layer', 't38_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 39: Pixelate ─────────────────────────────────────────────────
export function buildTutPixelate() {
  const c = note(
    't39_c',
    { x: -440, y: 0 },
    'Retro pixel look\n\n`image/pixelate` downsamples then upsamples — chunky blocks.\n\nA slider drives the block size. Big values give Atari, small values keep most detail.',
    { w: 420, h: 200 },
  );
  const url = node('t39_url', 'io/image-url', { x: 60, y: 0 }, { url: 'https://picsum.photos/seed/floret-pixelate/500/500' });
  const sz = node('t39_sz', 'io/slider', { x: 60, y: 200 }, { value: 8, min: 1, max: 32 });
  const pix = node('t39_pix', 'image/pixelate', { x: 320, y: 0 });
  const drawImg = node('t39_draw', 'draw/image', { x: 540, y: 40 }, { x: 0, y: 0, w: 320, h: 320 });
  const renderer = node('t39_renderer', 'render/renderer', { x: 780, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, url, sz, pix, drawImg, renderer],
    edges: [
      edge('t39_url', 'image', 't39_pix', 'image'),
      edge('t39_sz', 'number', 't39_pix', 'size'),
      edge('t39_pix', 'image', 't39_draw', 'image'),
      edge('t39_draw', 'layer', 't39_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 40: Pixel Sort ───────────────────────────────────────────────
export function buildTutPixelSort() {
  const c = note(
    't40_c',
    { x: -460, y: 0 },
    'Pixel sorting (datamosh)\n\n`pixel/sort` reorders runs of pixels along rows or columns by brightness/saturation. The "lo/hi" inputs mask which pixels get sorted, leaving the rest in place.\n\nA classic glitch-art look.',
    { w: 440, h: 220 },
  );
  const url = node('t40_url', 'io/image-url', { x: 60, y: 0 }, { url: 'https://picsum.photos/seed/floret-sort/500/500' });
  const sort = node('t40_sort', 'pixel/sort', { x: 280, y: 0 }, { axis: 'rows', criterion: 'brightness', order: 'ascending' });
  const drawImg = node('t40_draw', 'draw/image', { x: 520, y: 40 }, { x: 0, y: 0, w: 320, h: 320 });
  const renderer = node('t40_renderer', 'render/renderer', { x: 780, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, url, sort, drawImg, renderer],
    edges: [
      edge('t40_url', 'image', 't40_sort', 'image'),
      edge('t40_sort', 'image', 't40_draw', 'image'),
      edge('t40_draw', 'layer', 't40_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 41: Spread Sort + Shuffle ────────────────────────────────────
// Same input spread, two different orderings — visualised as bar chart.
export function buildTutSortShuffle() {
  const c = note(
    't41_c',
    { x: -460, y: 0 },
    'Reordering spreads\n\n`SortSpread` arranges numbers ascending/descending; `ShuffleSpread` permutes them deterministically by seed.\n\nA RandomSpread of 30 numbers, sorted, drawn as a bar chart. Try wiring the shuffle node back in instead.',
    { w: 440, h: 220 },
  );
  const range = nodeWithPorts('t41_range', 'spread/range', { x: 60, y: 0 }, { count: 30 });
  // Wait — we want a random-looking spread of numbers. Use spread/randomize on a Range? That shuffles the range. We'll sort it back to ascending.
  const shuf = node('t41_shuf', 'spread/randomize', { x: 260, y: 0 }, { seed: 7 });
  const sort = node('t41_sort', 'spread/sort', { x: 460, y: 0 });

  const RID = 't41_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 700, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 540, height: 280, label: 'ForEach' },
      ports: [
        { id: 'v', name: 'v', type: 'number', direction: 'input', y: 40 },
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 540, height: 280 },
  };
  const idx = nodeWithPorts('t41_idx', 'spread/range', { x: 60, y: 220 }, { count: 30 });
  const xMul = node('t41_xMul', 'math/mul', { x: 40, y: 40 }, { b: 12 }, RID);
  const xOff = node('t41_xOff', 'math/add', { x: 200, y: 40 }, { b: -180 }, RID);
  const hMul = node('t41_hMul', 'math/mul', { x: 40, y: 120 }, { b: -8 }, RID);
  const rect = node('t41_rect', 'draw/rect', { x: 360, y: 80 }, { y: 100, w: 8 }, RID);
  const renderer = node('t41_renderer', 'render/renderer', { x: 1280, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, range, shuf, sort, region, idx, xMul, xOff, hMul, rect, renderer],
    edges: [
      edge('t41_range', 'indices', 't41_shuf', 'value'),
      edge('t41_shuf', 'values', 't41_sort', 'spread'),
      edge('t41_sort', 'values', RID, 'v'),
      edge('t41_idx', 'indices', RID, 'i'),
      edge(RID, 'i', 't41_xMul', 'a'),
      edge('t41_xMul', 'product', 't41_xOff', 'a'),
      edge('t41_xOff', 'sum', 't41_rect', 'x'),
      edge(RID, 'v', 't41_hMul', 'a'),
      edge('t41_hMul', 'product', 't41_rect', 'h'),
      edge('t41_rect', 'shape', RID, 'l'),
      edge(RID, 'l', 't41_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 42: Elementary Cellular Automaton ────────────────────────────
// Wolfram rule N. One row + a rule → next row. We just show one step here.
export function buildTutElementaryCA() {
  const c = note(
    't42_c',
    { x: -460, y: 0 },
    'Cellular automata: one step\n\n`spread/elementary-ca` advances a 1-D row of booleans by one tick using a Wolfram rule (try 30, 90, 110, 184).\n\nHere a single-cell starting row is advanced once. Combine with `state/write-spread`/`read-spread` to grow the classic triangle over time.',
    { w: 440, h: 240 },
  );
  // Build a 60-cell row with only the centre cell on. Use range + map index → bool.
  const range = nodeWithPorts('t42_range', 'spread/range', { x: 60, y: 0 }, { count: 60 });

  const RID = 't42_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 500, height: 240, label: 'seedRow' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
        { id: 'b', name: 'b', type: 'boolean', direction: 'output', y: 60 },
      ],
    },
    style: { width: 500, height: 240 },
  };
  const eqMid = node('t42_eq', 'logic/eq', { x: 40, y: 40 }, { b: 30 }, RID);
  const rule = node('t42_rule', 'spread/elementary-ca', { x: 880, y: 0 }, { rule: 90 });

  // Visualise the result row as a row of small rects.
  const idx = nodeWithPorts('t42_idx', 'spread/range', { x: 60, y: 280 }, { count: 60 });
  const RID2 = 't42_draw';
  const region2: FloretNode = {
    id: RID2,
    type: 'floret',
    position: { x: 1100, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 480, height: 260, label: 'drawRow' },
      ports: [
        { id: 'b', name: 'b', type: 'boolean', direction: 'input', y: 40 },
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 480, height: 260 },
  };
  const xMul = node('t42_xMul', 'math/mul', { x: 40, y: 80 }, { b: 8 }, RID2);
  const xOff = node('t42_xOff', 'math/add', { x: 200, y: 80 }, { b: -240 }, RID2);
  // bool → number scale so b=true means h=20, b=false means h=0
  const hScale = node('t42_hScale', 'math/mul', { x: 40, y: 40 }, { b: 20 }, RID2);
  const rect = node('t42_rect', 'draw/rect', { x: 320, y: 80 }, { y: 0, w: 6 }, RID2);

  const renderer = node('t42_renderer', 'render/renderer', { x: 1620, y: 0 }, { width: 500, height: 400 });

  return {
    nodes: [c, range, region, eqMid, rule, idx, region2, xMul, xOff, hScale, rect, renderer],
    edges: [
      edge('t42_range', 'indices', RID, 'i'),
      edge(RID, 'i', 't42_eq', 'a'),
      edge('t42_eq', 'result', RID, 'b'),
      edge(RID, 'b', 't42_rule', 'row'),
      edge('t42_rule', 'next', RID2, 'b'),
      edge('t42_idx', 'indices', RID2, 'i'),
      edge(RID2, 'b', 't42_hScale', 'a'),
      edge('t42_hScale', 'product', 't42_rect', 'h'),
      edge(RID2, 'i', 't42_xMul', 'a'),
      edge('t42_xMul', 'product', 't42_xOff', 'a'),
      edge('t42_xOff', 'sum', 't42_rect', 'x'),
      edge('t42_rect', 'shape', RID2, 'l'),
      edge(RID2, 'l', 't42_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 43: Game of Life (single step) ───────────────────────────────
export function buildTutGoL() {
  const c = note(
    't43_c',
    { x: -460, y: 0 },
    'Game of Life\n\n`spread/gol-step` advances a 2-D boolean grid by one Conway tick.\n\nHere the grid is read from state, stepped, written back. Plus a starter pattern in the upper-left so frame 0 has something to chew on. Watch the colony evolve.',
    { w: 440, h: 240 },
  );
  const cols = node('t43_cols', 'value/number', { x: 60, y: 0 }, { value: 32 });
  const rows = node('t43_rows', 'value/number', { x: 60, y: 60 }, { value: 32 });
  const total = node('t43_total', 'value/number', { x: 60, y: 120 }, { value: 32 * 32 });

  // Build the initial seed: a 32x32 random sparse grid.
  const range = nodeWithPorts('t43_range', 'spread/range', { x: 60, y: 200 }, { count: 32 * 32 });
  const RIDseed = 't43_seed';
  const seedRegion: FloretNode = {
    id: RIDseed,
    type: 'floret',
    position: { x: 320, y: 160 },
    data: {
      defType: 'region/foreach',
      params: { width: 380, height: 240, label: 'seed' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
        { id: 'b', name: 'b', type: 'boolean', direction: 'output', y: 60 },
      ],
    },
    style: { width: 380, height: 240 },
  };
  const u = node('t43_u', 'random/uniform', { x: 40, y: 40 }, { min: 0, max: 1 }, RIDseed);
  const lt = node('t43_lt', 'logic/lt', { x: 200, y: 40 }, { b: 0.35 }, RIDseed);

  // Read previous grid from state.
  const rState = node('t43_rState', 'state/read-spread', { x: 60, y: 460 }, { slot: 1 });
  // Step it.
  const step = node('t43_step', 'spread/gol-step', { x: 760, y: 360 }, { cols: 32, rows: 32 });
  // Save it back.
  const wState = node('t43_wState', 'state/write-spread', { x: 980, y: 360 }, { slot: 1 });

  // Render: foreach (cell index, on?) drawing rects in a 32×32 grid.
  const idx = nodeWithPorts('t43_idx', 'spread/range', { x: 60, y: 580 }, { count: 32 * 32 });

  const RID = 't43_draw';
  const drawRegion: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 1180, y: 360 },
    data: {
      defType: 'region/foreach',
      params: { width: 500, height: 280, label: 'draw' },
      ports: [
        { id: 'b', name: 'b', type: 'boolean', direction: 'input', y: 40 },
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 500, height: 280 },
  };
  // x = (i % 32) * 8 - 128; y = floor(i / 32) * 8 - 128
  // We'll approximate with math: ix = i mod 32. There's no mod node; use math/fract: i - floor(i/32)*32.
  const div = node('t43_div', 'math/mul', { x: 40, y: 80 }, { b: 1 / 32 }, RID);
  const fl = node('t43_fl', 'math/floor', { x: 200, y: 80 }, {}, RID);
  const rowMul = node('t43_rowMul', 'math/mul', { x: 360, y: 80 }, { b: 32 }, RID);
  const colSub = node('t43_colSub', 'math/sub', { x: 40, y: 140 }, {}, RID);
  const xPos = node('t43_xPos', 'math/mul', { x: 200, y: 140 }, { b: 7 }, RID);
  const xOff = node('t43_xOff', 'math/add', { x: 360, y: 140 }, { b: -120 }, RID);
  const yPos = node('t43_yPos', 'math/mul', { x: 360, y: 200 }, { b: 7 }, RID);
  const yOff = node('t43_yOff', 'math/add', { x: 520, y: 200 }, { b: -120 }, RID);
  // size = b ? 6 : 0
  const sz = node('t43_sz', 'math/mul', { x: 40, y: 40 }, { b: 6 }, RID);
  const rect = node('t43_rect', 'draw/rect', { x: 700, y: 100 }, {}, RID);

  const renderer = node('t43_renderer', 'render/renderer', { x: 1760, y: 0 }, { width: 500, height: 500 });

  return {
    nodes: [
      c, cols, rows, total, range, seedRegion, u, lt,
      rState, step, wState, idx, drawRegion,
      div, fl, rowMul, colSub, xPos, xOff, yPos, yOff, sz, rect,
      renderer,
    ],
    edges: [
      // build the seed row (used as the state init via state/read-spread.init)
      edge('t43_range', 'indices', RIDseed, 'i'),
      edge(RIDseed, 'i', 't43_u', 'seed'),
      edge('t43_u', 'sample', 't43_lt', 'a'),
      edge('t43_lt', 'result', RIDseed, 'b'),
      edge(RIDseed, 'b', 't43_rState', 'init'),
      // step
      edge('t43_rState', 'current', 't43_step', 'grid'),
      edge('t43_step', 'next', 't43_wState', 'next'),
      // draw the current state
      edge('t43_idx', 'indices', RID, 'i'),
      edge('t43_rState', 'current', RID, 'b'),
      // inside draw
      edge(RID, 'i', 't43_div', 'a'),
      edge('t43_div', 'product', 't43_fl', 'x'),
      edge('t43_fl', 'floored', 't43_rowMul', 'a'),
      edge(RID, 'i', 't43_colSub', 'a'),
      edge('t43_rowMul', 'product', 't43_colSub', 'b'),
      edge('t43_colSub', 'difference', 't43_xPos', 'a'),
      edge('t43_xPos', 'product', 't43_xOff', 'a'),
      edge('t43_xOff', 'sum', 't43_rect', 'x'),
      edge('t43_fl', 'floored', 't43_yPos', 'a'),
      edge('t43_yPos', 'product', 't43_yOff', 'a'),
      edge('t43_yOff', 'sum', 't43_rect', 'y'),
      edge(RID, 'b', 't43_sz', 'a'),
      edge('t43_sz', 'product', 't43_rect', 'w'),
      edge('t43_sz', 'product', 't43_rect', 'h'),
      edge('t43_rect', 'shape', RID, 'l'),
      edge(RID, 'l', 't43_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 44: Accumulator (running sum in ForEach) ─────────────────────
export function buildTutAccumulator() {
  const c = note(
    't44_c',
    { x: -460, y: 0 },
    'Accumulator: fold within a ForEach\n\nThe split Read / Write pair lets you fold a spread without ever drawing a back-edge wire. Drop **AccRead** on the left of the region (it emits the running value via `read`) and **AccWrite** on the right (its `write` input becomes the next iteration\'s read; its `final` output is the last value once the loop ends). The pair is matched by `slot`.\n\nHere we sum 1..10 by accumulating an integer — every wire flows left to right.',
    { w: 460, h: 260 },
  );
  const range = nodeWithPorts('t44_range', 'spread/range', { x: 60, y: 0 }, { count: 10 });
  const initVal = node('t44_init', 'value/number', { x: 60, y: 160 }, { value: 0 });

  const RID = 't44_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 320, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 680, height: 300, label: 'sum' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 40 },
      ],
    },
    style: { width: 680, height: 300 },
  };
  // LTR layout inside the region:
  //   AccRead (left) → math/add(+1) → math/add → AccWrite (right)
  //   splicer-in i  ↗
  const read  = node('t44_read',  'region/accumulator-read/number',  { x: 40,  y: 80 }, { slot: 1 }, RID);
  const inc   = node('t44_inc',   'math/add', { x: 220, y: 160 }, { b: 1 }, RID); // i + 1
  const sum   = node('t44_sum',   'math/add', { x: 400, y: 80 }, {}, RID);
  const write = node('t44_write', 'region/accumulator-write/number', { x: 580, y: 80 }, { slot: 1 }, RID);

  // Display the final number with draw/text.
  const colN = node('t44_col', 'color/color', { x: 60, y: 280 }, { mode: 'rgb', r: 230, g: 240, b: 250, a: 255 });
  const fill = node('t44_fill', 'style/fill', { x: 240, y: 280 });
  const txt = node('t44_text', 'draw/text', { x: 1120, y: 80 }, { string: '0', x: -60, y: 0, size: 48, align: 'left' });

  const renderer = node('t44_renderer', 'render/renderer', { x: 1360, y: 0 }, { width: 400, height: 400 });

  return {
    nodes: [c, range, initVal, region, read, inc, sum, write, colN, fill, txt, renderer],
    edges: [
      edge('t44_range', 'indices', RID, 'i'),
      // Init flows into Read.init (outer edge, LTR).
      edge('t44_init', 'number', 't44_read', 'init'),
      // Inner: running = read + (i + 1), wired LTR.
      edge(RID, 'i', 't44_inc', 'a'),
      edge('t44_read', 'read', 't44_sum', 'a'),
      edge('t44_inc', 'sum', 't44_sum', 'b'),
      edge('t44_sum', 'sum', 't44_write', 'write'),
      // Style chain for the displayed number (LTR).
      edge('t44_col', 'color', 't44_fill', 'color'),
      edge('t44_fill', 'style', 't44_text', 'style'),
      // Final from the pair drives the displayed number's x-position.
      edge('t44_write', 'final', 't44_text', 'x'),
      edge('t44_text', 'layer', 't44_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 45: MIDI CC ──────────────────────────────────────────────────
export function buildTutMidiCC() {
  const c = note(
    't45_c',
    { x: -460, y: 0 },
    'MIDI CC → parameter\n\n`input/midi-cc` reads one Control Change knob from a connected MIDI device. The output is already mapped to [0, 1] via `value`; `raw` gives the original 0–127.\n\nPlug in a controller, pick the device + CC number; rotate the knob to scale the rectangle.',
    { w: 440, h: 220 },
  );
  const cc = node('t45_cc', 'input/midi-cc', { x: 60, y: 0 }, { device: '', channel: 0, cc: 1 });
  const sz = node('t45_sz', 'math/mul', { x: 260, y: 0 }, { b: 300 });
  const szO = node('t45_szO', 'math/add', { x: 420, y: 0 }, { b: 30 });
  const half = node('t45_half', 'math/mul', { x: 420, y: 100 }, { b: -0.5 });
  const rect = node('t45_rect', 'draw/rect', { x: 620, y: 40 });
  const renderer = node('t45_renderer', 'render/renderer', { x: 860, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, cc, sz, szO, half, rect, renderer],
    edges: [
      edge('t45_cc', 'value', 't45_sz', 'a'),
      edge('t45_sz', 'product', 't45_szO', 'a'),
      edge('t45_szO', 'sum', 't45_rect', 'w'),
      edge('t45_szO', 'sum', 't45_rect', 'h'),
      edge('t45_szO', 'sum', 't45_half', 'a'),
      edge('t45_half', 'product', 't45_rect', 'x'),
      edge('t45_half', 'product', 't45_rect', 'y'),
      edge('t45_rect', 'shape', 't45_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 46: Sound Filter Chain ───────────────────────────────────────
export function buildTutSoundFilter() {
  const c = note(
    't46_c',
    { x: -440, y: 0 },
    'Filter the sound\n\n`sound/filter` is a biquad (lowpass / highpass / bandpass). Wire any audio source into it, then on into `sound/output`.\n\nMouseX → filter cutoff frequency. A saw wave through a sweeping filter — the classic synth sound.',
    { w: 420, h: 220 },
  );
  const osc = node('t46_osc', 'sound/oscillator', { x: 60, y: 0 }, { type: 'sawtooth', freq: 80, amp: 0.3 });
  const m = node('t46_m', 'input/mouse', { x: 60, y: 200 });
  const freq = node('t46_freq', 'math/map', { x: 240, y: 200 }, { inMin: -200, inMax: 200, outMin: 80, outMax: 4000 });
  const filt = node('t46_filt', 'sound/filter', { x: 260, y: 0 }, { type: 'lowpass', res: 8 });
  const out = node('t46_out', 'sound/output', { x: 460, y: 0 }, { volume: 0.5 });
  const ell = node('t46_ell', 'draw/ellipse', { x: 460, y: 200 }, { w: 100, h: 100 });
  const renderer = node('t46_renderer', 'render/renderer', { x: 680, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, osc, m, freq, filt, out, ell, renderer],
    edges: [
      edge('t46_osc', 'audio', 't46_filt', 'in'),
      edge('t46_m', 'x', 't46_freq', 'x'),
      edge('t46_freq', 'mapped', 't46_filt', 'freq'),
      edge('t46_filt', 'audio', 't46_out', 'in'),
      edge('t46_ell', 'shape', 't46_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 47: Envelope (ADSR) ──────────────────────────────────────────
export function buildTutEnvelope() {
  const c = note(
    't47_c',
    { x: -440, y: 0 },
    'ADSR envelope\n\n`sound/envelope` shapes amplitude over time using Attack / Decay / Sustain / Release. Wire an audio source through it, then re-trigger the gate to hear the shape.\n\nMouseY drives a gate (pressed = mouse below origin). Click the canvas to "tap a note".',
    { w: 420, h: 240 },
  );
  const osc = node('t47_osc', 'sound/oscillator', { x: 60, y: 0 }, { type: 'sine', freq: 220, amp: 0.4 });
  const env = node('t47_env', 'sound/envelope', { x: 260, y: 0 }, { attack: 0.05, decay: 0.2, sustain: 0.4, release: 0.6, level: 1 });
  const m = node('t47_m', 'input/mouse', { x: 60, y: 200 });
  const out = node('t47_out', 'sound/output', { x: 460, y: 0 }, { volume: 0.5 });
  const ell = node('t47_ell', 'draw/ellipse', { x: 460, y: 200 }, { w: 80, h: 80 });
  const renderer = node('t47_renderer', 'render/renderer', { x: 680, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, osc, env, m, out, ell, renderer],
    edges: [
      edge('t47_osc', 'audio', 't47_env', 'in'),
      edge('t47_m', 'pressed', 't47_env', 'trigger'),
      edge('t47_env', 'audio', 't47_out', 'in'),
      edge('t47_ell', 'shape', 't47_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 48: Pulse rhythm ─────────────────────────────────────────────
export function buildTutPulse() {
  const c = note(
    't48_c',
    { x: -440, y: 0 },
    'Drone / rhythm with Pulse\n\n`sound/pulse` is a duty-cycle-controllable square wave. With `width` near 0 it sounds hollow; near ±1 it gets thin. Sweep `width` with sin(time) for a moving timbre.\n\nVisualised by a circle that breathes with the level.',
    { w: 420, h: 240 },
  );
  const t = node('t48_t', 'time/seconds', { x: 60, y: 200 });
  const sN = node('t48_s', 'math/sin', { x: 240, y: 200 });
  const half = node('t48_half', 'math/mul', { x: 420, y: 200 }, { b: 0.9 });
  const pulse = node('t48_pulse', 'sound/pulse', { x: 60, y: 0 }, { freq: 110, amp: 0.3, width: 0 });
  const out = node('t48_out', 'sound/output', { x: 260, y: 0 }, { volume: 0.5 });
  const amp = node('t48_amp', 'sound/amplitude', { x: 260, y: 60 }, { smoothing: 0.6 });
  const sz = node('t48_sz', 'math/mul', { x: 460, y: 60 }, { b: 500 });
  const szO = node('t48_szO', 'math/add', { x: 620, y: 60 }, { b: 30 });
  const ell = node('t48_ell', 'draw/ellipse', { x: 800, y: 60 });
  const renderer = node('t48_renderer', 'render/renderer', { x: 1020, y: 0 }, { width: 400, height: 400 });
  return {
    nodes: [c, t, sN, half, pulse, out, amp, sz, szO, ell, renderer],
    edges: [
      edge('t48_t', 't', 't48_s', 'x'),
      edge('t48_s', 'sine', 't48_half', 'a'),
      edge('t48_half', 'product', 't48_pulse', 'width'),
      edge('t48_pulse', 'audio', 't48_out', 'in'),
      edge('t48_pulse', 'audio', 't48_amp', 'in'),
      edge('t48_amp', 'level', 't48_sz', 'a'),
      edge('t48_sz', 'product', 't48_szO', 'a'),
      edge('t48_szO', 'sum', 't48_ell', 'w'),
      edge('t48_szO', 'sum', 't48_ell', 'h'),
      edge('t48_ell', 'shape', 't48_renderer', 'layers'),
    ],
  };
}


// ─── Tutorial 49: FFT Spectrum Bars ────────────────────────────────────────
export function buildTutFFT() {
  const c = note(
    't49_c',
    { x: -460, y: 0 },
    'FFT spectrum bars\n\n`sound/fft` analyses a live audio signal and outputs a `spectrum` spread (one number per frequency bin, 0..1). Wrap it in a ForEach with an index and you get a bar-chart visualiser.\n\nMic → 128 bins → 128 rectangles. Grant microphone permission.',
    { w: 440, h: 240 },
  );
  const start = node('t49_start', 'value/number', { x: 60, y: 0 }, { value: 1 });
  const mic = node('t49_mic', 'sound/audio-in', { x: 240, y: 0 }, { amp: 1 });
  const fft = node('t49_fft', 'sound/fft', { x: 420, y: 0 }, { smoothing: 0.7 });

  const idx = nodeWithPorts('t49_idx', 'spread/range', { x: 60, y: 200 }, { count: 128 });

  const RID = 't49_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 640, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 540, height: 320, label: 'bars' },
      ports: [
        { id: 'b', name: 'bin', type: 'number', direction: 'input', y: 40 },
        { id: 'i', name: 'i', type: 'number', direction: 'input', y: 80 },
        { id: 'l', name: 'layer', type: 'layer', direction: 'output', y: 60 },
      ],
    },
    style: { width: 540, height: 320 },
  };
  // x = i*4 - 250
  const xMul = node('t49_xMul', 'math/mul', { x: 40, y: 80 }, { b: 4 }, RID);
  const xOff = node('t49_xOff', 'math/add', { x: 200, y: 80 }, { b: -250 }, RID);
  // h = bin * 400; rect at y=200, h=-h (grow upward)
  const hScale = node('t49_hScale', 'math/mul', { x: 40, y: 40 }, { b: -400 }, RID);
  const rect = node('t49_rect', 'draw/rect', { x: 360, y: 80 }, { y: 200, w: 3 }, RID);
  const renderer = node('t49_renderer', 'render/renderer', { x: 1200, y: 0 }, { width: 500, height: 400 });
  return {
    nodes: [c, start, mic, fft, idx, region, xMul, xOff, hScale, rect, renderer],
    edges: [
      edge('t49_start', 'number', 't49_mic', 'start'),
      edge('t49_mic', 'audio', 't49_fft', 'in'),
      edge('t49_fft', 'spectrum', RID, 'b'),
      edge('t49_idx', 'indices', RID, 'i'),
      edge(RID, 'b', 't49_hScale', 'a'),
      edge('t49_hScale', 'product', 't49_rect', 'h'),
      edge(RID, 'i', 't49_xMul', 'a'),
      edge('t49_xMul', 'product', 't49_xOff', 'a'),
      edge('t49_xOff', 'sum', 't49_rect', 'x'),
      edge('t49_rect', 'shape', RID, 'l'),
      edge(RID, 'l', 't49_renderer', 'layers'),
    ],
  };
}
