import type { FloretNode } from '../../../editor/editor-state';
import { node, edge, note } from '../helpers';


// ─── Sample: Datamosh (image manipulation) ─────────────────────────────────
// Load an image, then composite it with itself at multiple offsets, tints,
// and blend modes for a chromatic-aberration / RGB-channel-shift glitch
// look. Time animates the channel offsets so the image breathes apart and
// back together.
//
// Pattern:
//   io/image-url → draw/image (base, no tint, normal blend)
//                → draw/image (red shift, +dx, screen blend)
//                → draw/image (green shift, 0, difference blend)
//                → draw/image (blue shift, -dx, screen blend)
//                → small overlay slice scrambled by Perlin noise
//
// Stack + Group bundles the per-image layers into one drawn-in-order stack.
export function buildDatamosh() {
  const time = node('imd_time', 'time/seconds', { x: -540, y: 20 });
  // Time-driven offset: amplitude·sin(t·speed)
  const sinT = node('imd_sinT', 'math/sin', { x: -360, y: 20 });
  const tSpeed = node('imd_tSpeed', 'math/mul', { x: -540, y: 80 }, { b: 1.2 });
  const offsetMul = node('imd_offsetMul', 'math/mul', { x: -180, y: 20 }, { b: 22 });
  // Negative offset for the cyan channel
  const negOffset = node('imd_negOffset', 'math/mul', { x: -180, y: 80 }, { b: -1 });

  // Load image. Set a reasonable default; the user can swap the URL.
  const imgLoader = node('imd_img', 'io/image-url', { x: -540, y: 160 }, {
    url: 'https://picsum.photos/seed/floret/500/500',
  });

  // Image bounds: top-left at (-200, -200) with size (400, 400) so the
  // image is centred on the 500×500 centred-origin canvas.
  const baseX = node('imd_baseX', 'value/number', { x: -540, y: 280 }, { value: -200 });
  const baseY = node('imd_baseY', 'value/number', { x: -540, y: 340 }, { value: -200 });
  const drawW = node('imd_drawW', 'value/number', { x: -540, y: 400 }, { value: 400 });
  const drawH = node('imd_drawH', 'value/number', { x: -540, y: 460 }, { value: 400 });
  // Shifted x = baseX + offset
  const xShifted = node('imd_xShifted', 'math/add', { x: 0, y: 280 });
  const xShiftedNeg = node('imd_xShiftedNeg', 'math/add', { x: 0, y: 340 });

  // Tint colors — pure red, green, blue at half alpha so blend modes
  // can pick up the channel.
  const redTint = node('imd_redTint', 'color/rgb', { x: -300, y: 160 }, { r: 255, g: 0, b: 0, a: 200 });
  const grnTint = node('imd_grnTint', 'color/rgb', { x: -300, y: 220 }, { r: 0, g: 255, b: 0, a: 200 });
  const bluTint = node('imd_bluTint', 'color/rgb', { x: -300, y: 280 }, { r: 0, g: 0, b: 255, a: 200 });

  // Four draw/image layers — base + R / G / B tinted versions
  const drawBase = node('imd_drawBase', 'draw/image', { x: 240, y: 60 });
  const drawR = node('imd_drawR', 'draw/image', { x: 240, y: 160 });
  const drawG = node('imd_drawG', 'draw/image', { x: 240, y: 260 });
  const drawB = node('imd_drawB', 'draw/image', { x: 240, y: 360 });

  // Blend mode wrappers — base draws ONCE normally; R/G/B use ADD to
  // layer the tinted copies on top, so where they're offset from the base
  // the channel separation becomes visible at the edges of every shape.
  const blendBase = node('imd_blendBase', 'transform/blend', { x: 460, y: 60 }, { mode: 'blend' });
  const blendR = node('imd_blendR', 'transform/blend', { x: 460, y: 160 }, { mode: 'add' });
  const blendG = node('imd_blendG', 'transform/blend', { x: 460, y: 260 }, { mode: 'subtract' });
  const blendB = node('imd_blendB', 'transform/blend', { x: 460, y: 360 }, { mode: 'add' });

  // Combine 4 layers via 3 Stacks → 1 Group (Stack chains since each Stack
  // takes 2 inputs; final layer-spread length 4).
  const s1 = node('imd_s1', 'spread/stack', { x: 660, y: 100 });
  const s2 = node('imd_s2', 'spread/stack', { x: 660, y: 180 });
  const s3 = node('imd_s3', 'spread/stack', { x: 660, y: 260 });
  const finalGroup = node('imd_finalGroup', 'transform/group', { x: 660, y: 340 });

  const bg = node('imd_bg', 'color/rgb', { x: -540, y: 520 }, { r: 10, g: 8, b: 16, a: 255 });
  const renderer = node('imd_renderer', 'render/renderer', { x: 880, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, sinT, tSpeed, offsetMul, negOffset,
      imgLoader,
      baseX, baseY, drawW, drawH, xShifted, xShiftedNeg,
      redTint, grnTint, bluTint,
      drawBase, drawR, drawG, drawB,
      blendBase, blendR, blendG, blendB,
      s1, s2, s3, finalGroup,
      bg, renderer,
    ],
    edges: [
      edge('imd_bg', 'color', 'imd_renderer', 'background'),
      // Time → offset
      edge('imd_time', 't', 'imd_tSpeed', 'a'),
      edge('imd_tSpeed', 'product', 'imd_sinT', 'x'),
      edge('imd_sinT', 'sine', 'imd_offsetMul', 'a'),
      edge('imd_offsetMul', 'product', 'imd_negOffset', 'a'),
      // xShifted = baseX + offset, xShiftedNeg = baseX + (-offset)
      edge('imd_baseX', 'number', 'imd_xShifted', 'a'),
      edge('imd_offsetMul', 'product', 'imd_xShifted', 'b'),
      edge('imd_baseX', 'number', 'imd_xShiftedNeg', 'a'),
      edge('imd_negOffset', 'product', 'imd_xShiftedNeg', 'b'),
      // Image feeds all four draws (broadcast)
      edge('imd_img', 'image', 'imd_drawBase', 'image'),
      edge('imd_img', 'image', 'imd_drawR', 'image'),
      edge('imd_img', 'image', 'imd_drawG', 'image'),
      edge('imd_img', 'image', 'imd_drawB', 'image'),
      // Base draw — no tint, no offset
      edge('imd_baseX', 'number', 'imd_drawBase', 'x'),
      edge('imd_baseY', 'number', 'imd_drawBase', 'y'),
      edge('imd_drawW', 'number', 'imd_drawBase', 'w'),
      edge('imd_drawH', 'number', 'imd_drawBase', 'h'),
      // R-tint, +offset
      edge('imd_xShifted', 'sum', 'imd_drawR', 'x'),
      edge('imd_baseY', 'number', 'imd_drawR', 'y'),
      edge('imd_drawW', 'number', 'imd_drawR', 'w'),
      edge('imd_drawH', 'number', 'imd_drawR', 'h'),
      edge('imd_redTint', 'color', 'imd_drawR', 'tint'),
      // G-tint, no shift
      edge('imd_baseX', 'number', 'imd_drawG', 'x'),
      edge('imd_baseY', 'number', 'imd_drawG', 'y'),
      edge('imd_drawW', 'number', 'imd_drawG', 'w'),
      edge('imd_drawH', 'number', 'imd_drawG', 'h'),
      edge('imd_grnTint', 'color', 'imd_drawG', 'tint'),
      // B-tint, -offset
      edge('imd_xShiftedNeg', 'sum', 'imd_drawB', 'x'),
      edge('imd_baseY', 'number', 'imd_drawB', 'y'),
      edge('imd_drawW', 'number', 'imd_drawB', 'w'),
      edge('imd_drawH', 'number', 'imd_drawB', 'h'),
      edge('imd_bluTint', 'color', 'imd_drawB', 'tint'),
      // Wrap each in a blend
      edge('imd_drawBase', 'layer', 'imd_blendBase', 'layer'),
      edge('imd_drawR', 'layer', 'imd_blendR', 'layer'),
      edge('imd_drawG', 'layer', 'imd_blendG', 'layer'),
      edge('imd_drawB', 'layer', 'imd_blendB', 'layer'),
      // Stack 4 blended layers → group → renderer
      edge('imd_blendBase', 'blended', 'imd_s1', 'a'),
      edge('imd_blendR', 'blended', 'imd_s1', 'b'),
      edge('imd_s1', 'values', 'imd_s2', 'a'),
      edge('imd_blendG', 'blended', 'imd_s2', 'b'),
      edge('imd_s2', 'values', 'imd_s3', 'a'),
      edge('imd_blendB', 'blended', 'imd_s3', 'b'),
      edge('imd_s3', 'values', 'imd_finalGroup', 's1'),
      edge('imd_finalGroup', 'grouped', 'imd_renderer', 'layers'),
    ],
  };
}


// ─── Sample: Photo Mosaic ──────────────────────────────────────────────────
// Load a photo, sample every pixel in a 32×32 grid with image/sample, draw
// each point as a circle whose size is proportional to the pixel's luminance.
// The result is a pointillist / Warhol dot-matrix of the source image that
// breathes gently with sin(time).
export function buildPhotoMosaic() {
  const img = node('pm_img', 'io/image-url', { x: -340, y: 20 }, {
    url: 'https://picsum.photos/seed/mosaic/500/500',
  });
  const grid = node('pm_grid', 'spread/grid', { x: -340, y: 140 }, { cols: 32, rows: 32, w: 500, h: 500 });
  const time = node('pm_time', 'time/seconds', { x: -340, y: 260 });
  const renderer = node('pm_renderer', 'render/renderer', { x: 1120, y: 20 }, { width: 500, height: 500 });
  const bg = node('pm_bg', 'color/rgb', { x: -340, y: 340 }, { r: 8, g: 8, b: 12, a: 255 });

  const RID = 'pm_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 0, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 1060, height: 620, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'dot', name: 'dot', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1060, height: 620 },
  };

  // Inside region: split → sample → luminance → size → fill → dot
  const split    = node('pm_split',    'vec2/split',       { x: 40,  y: 60 },  {},                                      RID);
  const sample   = node('pm_sample',   'image/sample',     { x: 220, y: 60 },  {},                                      RID);
  const srgb     = node('pm_srgb',     'color/split-rgb',  { x: 420, y: 60 },  {},                                      RID);
  const rg       = node('pm_rg',       'math/add',         { x: 580, y: 40 },  {},                                      RID);
  const rgb      = node('pm_rgb',      'math/add',         { x: 580, y: 100 }, {},                                      RID);
  const lum      = node('pm_lum',      'math/mul',         { x: 680, y: 60 },  { b: 0.333 },                            RID);
  const sizeBase = node('pm_sizeBase', 'math/map',         { x: 760, y: 60 },  { inMin: 0, inMax: 255, outMin: 0.5, outMax: 13 }, RID);
  // Gentle breathing scale driven by time (broadcast from outer scope)
  const tMul     = node('pm_tMul',     'math/mul',         { x: 200, y: 340 }, { b: 1.8 },                              RID);
  const sinT     = node('pm_sinT',     'math/sin',         { x: 340, y: 340 }, {},                                      RID);
  const wobble   = node('pm_wobble',   'math/map',         { x: 460, y: 340 }, { inMin: -1, inMax: 1, outMin: 0.75, outMax: 1.25 }, RID);
  const dotSize  = node('pm_dotSize',  'math/mul',         { x: 620, y: 200 }, {},                                      RID);
  const fill     = node('pm_fill',     'style/fill',       { x: 760, y: 200 }, { mode: 'color' },                       RID);
  const ellipse  = node('pm_ellipse',  'draw/ellipse',     { x: 880, y: 100 }, {},                                      RID);

  return {
    nodes: [img, grid, time, renderer, bg, region, split, sample, srgb, rg, rgb, lum, sizeBase, tMul, sinT, wobble, dotSize, fill, ellipse],
    edges: [
      edge('pm_bg', 'color',    'pm_renderer', 'background'),
      edge('pm_grid', 'positions', RID,           'p'),
      // Inner: split point → sample → lum → size
      edge(RID,          'p',     'pm_split',   'v'),
      edge('pm_split',   'x',     'pm_sample',  'x'),
      edge('pm_split',   'y',     'pm_sample',  'y'),
      edge('pm_img',     'image', 'pm_sample',  'image'),   // broadcast from outer
      edge('pm_sample',  'color', 'pm_srgb',    'color'),
      edge('pm_srgb',    'r',     'pm_rg',      'a'),
      edge('pm_srgb',    'g',     'pm_rg',      'b'),
      edge('pm_rg', 'sum',   'pm_rgb',     'a'),
      edge('pm_srgb',    'b',     'pm_rgb',     'b'),
      edge('pm_rgb', 'sum',   'pm_lum',     'a'),
      edge('pm_lum', 'product',   'pm_sizeBase','x'),
      // Time wobble (time broadcast from outer)
      edge('pm_time',    't',     'pm_tMul',    'a'),
      edge('pm_tMul', 'product',   'pm_sinT',    'x'),
      edge('pm_sinT', 'sine',   'pm_wobble',  'x'),
      edge('pm_sizeBase', 'mapped',   'pm_dotSize', 'a'),
      edge('pm_wobble', 'mapped',   'pm_dotSize', 'b'),
      // Fill and draw
      edge('pm_sample',  'color', 'pm_fill',    'color'),
      edge('pm_fill', 'style',   'pm_ellipse', 'style'),
      edge('pm_split',   'x',     'pm_ellipse', 'x'),
      edge('pm_split',   'y',     'pm_ellipse', 'y'),
      edge('pm_dotSize', 'product',   'pm_ellipse', 'w'),
      edge('pm_dotSize', 'product',   'pm_ellipse', 'h'),
      edge('pm_ellipse', 'shape',   RID,          'dot'),
      edge(RID,          'dot',   'pm_renderer','layers'),
    ],
  };
}


// ─── Sample: Webcam Mosaic ─────────────────────────────────────────────────
// A live, mirrored pointillist portrait of you. The camera feed is squared
// off with image/resize, sampled on a 36×36 grid, and each cell drawn as a
// dot sized by its brightness and tinted by its colour. Flip the Toggle wired
// into `start` to open the camera (grant permission when the browser asks).
//
// PERFORMANCE: all sampling happens ONCE per frame with image/sample-spread —
// a single getImageData(0,0,w,h) read indexed N times — NOT N separate
// getImageData(x,y,1,1) calls inside the loop (each of those forces a
// synchronous GPU→CPU readback; 1296/frame is what kills the frame rate).
// The ForEach therefore only does cheap per-dot draw work: the colour for
// each cell arrives pre-sampled as a spread.
//
// Two grids, identical layout, different origin:
//   • drawGrid   — centre (0,0), range −250…250 → where each dot is drawn
//   • sampleGrid — centre (250,250), range 0…500 → pixel coords to sample
// so there's zero per-cell coordinate math.
export function buildWebcamMosaic() {
  // ─────────── Annotations (meta/comment) frame each block ───────────
  const overview = note('wm_doc', { x: -940, y: -260 },
    'WEBCAM MOSAIC — how it works\n\n' +
    'A live halftone of the camera. The frame is read once, sampled on a grid, ' +
    'and each cell becomes a YELLOW CIRCLE (where the image is light) or a ' +
    'BLUE SQUARE (where it is dark).\n\n' +
    '①→② Read the feed · ③ Lay out a grid · ④ Per cell decide a shape · ⑤ Render',
    { w: 560, h: 200 });

  // ① Live source
  const nSrc = note('wm_n_src', { x: -640, y: -120 },
    '① LIVE SOURCE\nThe Toggle opens the camera (start). image/resize squares the ' +
    'feed to 500×500 so grid coords line up with pixels 1:1.', { w: 420, h: 120 });
  const cam   = node('wm_cam',   'input/camera', { x: -640, y: 40 },  { device: '', mirror: true });
  const start = node('wm_start', 'io/toggle',    { x: -640, y: 180 }, { value: true });   // flip to open camera
  const resize = node('wm_resize', 'image/resize', { x: -440, y: 40 }, { w: 500, h: 500 });

  // ② Grids + single read
  const nGrid = note('wm_n_grid', { x: -260, y: -120 },
    '② GRID + ONE READ\nTwo matching 36×36 grids: the DRAW grid (centred — where ' +
    'shapes go) and the SAMPLE grid (pixel space — where to read). image/sample-' +
    'spread reads the whole frame ONCE → one colour per cell (fast).', { w: 440, h: 140 });
  const drawGrid   = node('wm_drawGrid',   'spread/grid', { x: -440, y: 200 }, { cols: 36, rows: 36, w: 500, h: 500 });
  const sampleGrid = node('wm_sampleGrid', 'spread/grid', { x: -440, y: 340 }, { cols: 36, rows: 36, w: 500, h: 500, center: { x: 250, y: 250 } });
  const colors = node('wm_colors', 'image/sample-spread', { x: -240, y: 120 });

  const bg   = node('wm_bg',   'color/rgb', { x: -640, y: 320 }, { r: 6, g: 7, b: 12, a: 255 });

  // ⑤ Render
  const nRender = note('wm_n_render', { x: 1340, y: -120 },
    '⑤ RENDER\nThe ForEach hands back one chosen shape per cell; the Renderer ' +
    'draws them all into the live mosaic.', { w: 360, h: 120 });
  const renderer = node('wm_renderer', 'render/renderer', { x: 1340, y: 40 }, { width: 500, height: 500 });

  const RID = 'wm_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 0, y: 40 },
    data: {
      defType: 'region/foreach',
      params: { width: 1260, height: 760, label: '④ ForEach cell' },
      ports: [
        { id: 'p',   name: 'p',   type: 'vec2',  direction: 'input' },
        { id: 'col', name: 'col', type: 'color', direction: 'input' },
        { id: 'dot', name: 'dot', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1260, height: 760 },
  };

  // ── Inside the loop (runs once per grid cell) ──
  // a · Cell position (shared by both shapes)
  const nPos = note('wm_n_pos', { x: 40, y: 30 },
    'a · POSITION\nWhere this cell sits on the centred canvas — fed to both shapes.', { w: 320, h: 70 }, RID);
  const split = node('wm_split', 'vec2/split', { x: 60, y: 120 }, {}, RID);
  const pos   = node('wm_pos',   'vec2/make',  { x: 220, y: 120 }, {}, RID);

  // b · Brightness → light/dark decision
  const nLum = note('wm_n_lum', { x: 40, y: 210 },
    'b · BRIGHTNESS\nAverage the pixel\'s R, G, B (0–255). Greater than 128 (mid-grey) → "light".', { w: 420, h: 70 }, RID);
  const srgb  = node('wm_srgb',  'color/split-rgb', { x: 60, y: 300 }, {}, RID);
  const rg    = node('wm_rg',    'math/add',        { x: 240, y: 280 }, {}, RID);
  const rgb   = node('wm_rgb',   'math/add',        { x: 240, y: 340 }, {}, RID);
  const lum   = node('wm_lum',   'math/mul',        { x: 400, y: 310 }, { b: 0.333 }, RID); // 0..255
  const isLight = node('wm_isLight', 'logic/gt',    { x: 560, y: 310 }, { b: 128 }, RID);   // light if > mid-grey

  // c · Two candidate shapes
  const nShapes = note('wm_n_shapes', { x: 40, y: 430 },
    'c · TWO SHAPES\nBoth are built every cell: a yellow circle and a blue square.', { w: 360, h: 70 }, RID);
  const yCol  = node('wm_yCol',  'color/rgb',   { x: 60, y: 520 }, { r: 250, g: 210, b: 40, a: 255 }, RID);
  const yFill = node('wm_yFill', 'style/fill',  { x: 230, y: 520 }, { mode: 'color' }, RID);
  const circle= node('wm_circle','draw/ellipse',{ x: 560, y: 500 }, { w: 13, h: 13 }, RID);
  const bCol  = node('wm_bCol',  'color/rgb',   { x: 60, y: 640 }, { r: 40, g: 90, b: 225, a: 255 }, RID);
  const bFill = node('wm_bFill', 'style/fill',  { x: 230, y: 640 }, { mode: 'color' }, RID);
  const square= node('wm_square','draw/rect',   { x: 560, y: 620 }, { w: 12, h: 12 }, RID);

  // d · Pick one
  const nPick = note('wm_n_pick', { x: 820, y: 400 },
    'd · SELECT\nlight → yellow circle\ndark → blue square\nOnly the picked shape is emitted.', { w: 300, h: 130 }, RID);
  const pick  = node('wm_pick',  'logic/select',{ x: 860, y: 560 }, {}, RID);

  return {
    nodes: [overview, nSrc, nGrid, nRender,
            cam, start, resize, drawGrid, sampleGrid, colors, bg, renderer, region,
            nPos, split, pos, nLum, srgb, rg, rgb, lum, isLight,
            nShapes, yCol, yFill, circle, bCol, bFill, square, nPick, pick],
    edges: [
      // Camera → resize → ONE sample-spread → colour spread
      edge('wm_start',      'bool',      'wm_cam',     'start'),
      edge('wm_cam',        'image',     'wm_resize',  'image'),
      edge('wm_resize',     'image',     'wm_colors',  'image'),
      edge('wm_sampleGrid', 'positions', 'wm_colors',  'points'),
      edge('wm_bg',         'color',     'wm_renderer','background'),
      // Drive the loop: one cell position + one pre-sampled colour per iteration
      edge('wm_drawGrid',   'positions', RID,          'p'),
      edge('wm_colors',     'colors',    RID,          'col'),

      // cell position (shared by both shapes)
      edge(RID,        'p',   'wm_split', 'v'),
      edge('wm_split', 'x',   'wm_pos',   'x'),
      edge('wm_split', 'y',   'wm_pos',   'y'),
      edge('wm_pos',   'vector', 'wm_circle', 'pos'),
      edge('wm_pos',   'vector', 'wm_square', 'pos'),

      // luminance = (r + g + b) / 3, then threshold
      edge(RID,        'col', 'wm_srgb', 'color'),
      edge('wm_srgb',  'r',   'wm_rg',   'a'),
      edge('wm_srgb',  'g',   'wm_rg',   'b'),
      edge('wm_rg',    'result', 'wm_rgb', 'a'),
      edge('wm_srgb',  'b',   'wm_rgb',  'b'),
      edge('wm_rgb',   'result', 'wm_lum', 'a'),
      edge('wm_lum',   'result', 'wm_isLight', 'a'),

      // build both shapes
      edge('wm_yCol',  'color', 'wm_yFill', 'color'),
      edge('wm_yFill', 'style', 'wm_circle', 'style'),
      edge('wm_bCol',  'color', 'wm_bFill', 'color'),
      edge('wm_bFill', 'style', 'wm_square', 'style'),

      // pick: light → circle, dark → square
      edge('wm_isLight', 'result', 'wm_pick', 'cond'),
      edge('wm_circle',  'shape',  'wm_pick', 'a'),
      edge('wm_square',  'shape',  'wm_pick', 'b'),
      edge('wm_pick',    'selected', RID,     'dot'),
      edge(RID,          'dot',   'wm_renderer', 'layers'),
    ],
  };
}


// ─── Sample: Body Tracking ─────────────────────────────────────────────────
// Live skeleton from the webcam via the input/landmarks (MediaPipe) node.
// One Toggle starts the camera + the tracker; the tracker emits landmark
// `points` plus parallel `connA`/`connB` segment-endpoint spreads. Two
// ForEach loops draw it: one a cyan LINE per connection pair, the other a
// yellow DOT per point. Switch the tracker dropdown (hands / face / pose) on
// the Landmarks node to track something else. Flip `start`, grant permission.
export function buildBodyTracking() {
  const doc = note('bt_doc', { x: -640, y: -200 },
    'BODY TRACKING (MediaPipe)\n\nToggle → starts the Camera AND the Landmarks tracker. Each detection gets a STABLE auto colour (the node\'s `color` / `connColor` outputs), so every person/hand entering the view is a different colour. One ForEach draws a coloured line per edge, another a coloured dot per point.\n\nSwitch the tracker dropdown (hands / face / pose / all) and raise `count` to track more people. Set `person` to 1/2/3 to isolate one.',
    { w: 560, h: 230 });

  const cam = node('bt_cam', 'input/camera', { x: -640, y: 60 }, { device: '', mirror: true });
  const tog = node('bt_tog', 'io/toggle', { x: -640, y: 220 }, { value: true });
  const lm  = node('bt_lm', 'input/landmarks', { x: -400, y: 60 }, { tracker: 'hands', mirror: true, count: 2, person: 0 });
  const bg  = node('bt_bg', 'color/rgb', { x: -640, y: 340 }, { r: 8, g: 10, b: 16, a: 255 });
  const renderer = node('bt_renderer', 'render/renderer', { x: 1180, y: 20 }, { width: 520, height: 520 });

  // ── Lines: one coloured draw/line per connection pair (connA[i]→connB[i]) ──
  const LRID = 'bt_lines';
  const lines: FloretNode = {
    id: LRID, type: 'floret', position: { x: 0, y: -120 },
    data: {
      defType: 'region/foreach',
      params: { width: 900, height: 320, label: 'ForEach edge' },
      ports: [
        { id: 'a', name: 'a', type: 'vec2', direction: 'input' },
        { id: 'b', name: 'b', type: 'vec2', direction: 'input' },
        { id: 'col', name: 'col', type: 'color', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 900, height: 320 },
  };
  const lstroke = node('bt_lstroke', 'style/stroke', { x: 220, y: 60 }, { width: 2 }, LRID);
  const line = node('bt_line', 'draw/line', { x: 460, y: 40 }, {}, LRID);

  // ── Points: one coloured dot per landmark ──
  const PRID = 'bt_points';
  const points: FloretNode = {
    id: PRID, type: 'floret', position: { x: 0, y: 280 },
    data: {
      defType: 'region/foreach',
      params: { width: 760, height: 300, label: 'ForEach point' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'col', name: 'col', type: 'color', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 760, height: 300 },
  };
  const pfill = node('bt_pfill', 'style/fill', { x: 220, y: 60 }, { mode: 'color' }, PRID);
  const dot = node('bt_dot', 'draw/ellipse', { x: 460, y: 40 }, { w: 8, h: 8 }, PRID);

  return {
    nodes: [doc, cam, tog, lm, bg, renderer, lines, lstroke, line, points, pfill, dot],
    edges: [
      // one toggle starts both the camera and the tracker
      edge('bt_tog', 'bool', 'bt_cam', 'start'),
      edge('bt_tog', 'bool', 'bt_lm', 'start'),
      edge('bt_cam', 'image', 'bt_lm', 'image'),
      edge('bt_bg', 'color', 'bt_renderer', 'background'),

      // lines region — coloured by the per-segment connColor (per person)
      edge('bt_lm', 'connA', LRID, 'a'),
      edge('bt_lm', 'connB', LRID, 'b'),
      edge('bt_lm', 'connColor', LRID, 'col'),
      edge(LRID, 'a', 'bt_line', 'a'),
      edge(LRID, 'b', 'bt_line', 'b'),
      edge(LRID, 'col', 'bt_lstroke', 'color'),
      edge('bt_lstroke', 'style', 'bt_line', 'style'),
      edge('bt_line', 'shape', LRID, 'layer'),

      // points region — coloured by the per-point color (per person)
      edge('bt_lm', 'points', PRID, 'p'),
      edge('bt_lm', 'color', PRID, 'col'),
      edge(PRID, 'p', 'bt_dot', 'pos'),
      edge(PRID, 'col', 'bt_pfill', 'color'),
      edge('bt_pfill', 'style', 'bt_dot', 'style'),
      edge('bt_dot', 'shape', PRID, 'layer'),

      // lines under points
      edge(LRID, 'layer', 'bt_renderer', 'layers'),
      edge(PRID, 'layer', 'bt_renderer', 'layers'),
    ],
  };
}
