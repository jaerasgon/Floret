import type { FloretNode } from '../../../editor/editor-state';
import { node, edge } from '../helpers';


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
  // ── Live source ──
  const cam   = node('wm_cam',   'input/camera', { x: -620, y: 40 },  { device: '', mirror: true });
  const start = node('wm_start', 'io/toggle',    { x: -620, y: 180 }, { value: true });   // flip to open camera
  // Square the feed: smaller, fixed read surface + grid coords line up with pixels.
  const resize = node('wm_resize', 'image/resize', { x: -420, y: 40 }, { w: 500, h: 500 });

  // Draw grid (centred) and sample grid (pixel-space) — same 36×36 layout.
  const drawGrid   = node('wm_drawGrid',   'spread/grid', { x: -420, y: 200 }, { cols: 36, rows: 36, w: 500, h: 500 });
  const sampleGrid = node('wm_sampleGrid', 'spread/grid', { x: -420, y: 340 }, { cols: 36, rows: 36, w: 500, h: 500, center: { x: 250, y: 250 } });

  // ONE full-frame read → spread of colours (the whole performance fix).
  const colors = node('wm_colors', 'image/sample-spread', { x: -200, y: 120 });

  const bg   = node('wm_bg',   'color/rgb', { x: -620, y: 320 }, { r: 6, g: 7, b: 12, a: 255 });
  const renderer = node('wm_renderer', 'render/renderer', { x: 1140, y: 20 }, { width: 500, height: 500 });

  const RID = 'wm_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 0, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 1020, height: 560, label: 'ForEach' },
      ports: [
        { id: 'p',   name: 'p',   type: 'vec2',  direction: 'input' },
        { id: 'col', name: 'col', type: 'color', direction: 'input' },
        { id: 'dot', name: 'dot', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1020, height: 560 },
  };

  // ── Inside: pre-sampled colour → luminance → dot (no image reads here) ──
  const split = node('wm_split', 'vec2/split',      { x: 40,  y: 60 },  {},                                              RID);
  const srgb  = node('wm_srgb',  'color/split-rgb', { x: 220, y: 200 }, {},                                              RID);
  const rg    = node('wm_rg',    'math/add',        { x: 380, y: 180 }, {},                                              RID);
  const rgb   = node('wm_rgb',   'math/add',        { x: 380, y: 240 }, {},                                              RID);
  const lum   = node('wm_lum',   'math/mul',        { x: 500, y: 210 }, { b: 0.333 },                                    RID);
  const size  = node('wm_size',  'math/map',        { x: 620, y: 210 }, { inMin: 0, inMax: 255, outMin: 1.5, outMax: 15 }, RID);
  const fill  = node('wm_fill',  'style/fill',      { x: 620, y: 320 }, { mode: 'color' },                               RID);
  const dot   = node('wm_dot',   'draw/ellipse',    { x: 800, y: 120 }, {},                                              RID);

  return {
    nodes: [cam, start, resize, drawGrid, sampleGrid, colors, bg, renderer, region, split, srgb, rg, rgb, lum, size, fill, dot],
    edges: [
      // Camera → resize → ONE sample-spread → colour spread
      edge('wm_start',      'bool',      'wm_cam',     'start'),
      edge('wm_cam',        'image',     'wm_resize',  'image'),
      edge('wm_resize',     'image',     'wm_colors',  'image'),
      edge('wm_sampleGrid', 'positions', 'wm_colors',  'points'),
      edge('wm_bg',         'color',     'wm_renderer','background'),
      // Drive the loop: one dot position + one pre-sampled colour per iteration
      edge('wm_drawGrid',   'positions', RID,          'p'),
      edge('wm_colors',     'colors',    RID,          'col'),
      // Per cell — pure math, no image reads
      edge(RID,        'p',     'wm_split', 'v'),
      edge(RID,        'col',   'wm_srgb',  'color'),
      edge('wm_srgb',  'r',     'wm_rg',    'a'),
      edge('wm_srgb',  'g',     'wm_rg',    'b'),
      edge('wm_rg',    'sum',   'wm_rgb',   'a'),
      edge('wm_srgb',  'b',     'wm_rgb',   'b'),
      edge('wm_rgb',   'sum',   'wm_lum',   'a'),
      edge('wm_lum',   'product', 'wm_size','x'),
      // Colour the dot with its sampled pixel
      edge(RID,        'col',   'wm_fill',  'color'),
      edge('wm_fill',  'style', 'wm_dot',   'style'),
      // Draw at the centred coords; size by brightness
      edge('wm_split', 'x',     'wm_dot',   'x'),
      edge('wm_split', 'y',     'wm_dot',   'y'),
      edge('wm_size',  'mapped', 'wm_dot',  'w'),
      edge('wm_size',  'mapped', 'wm_dot',  'h'),
      edge('wm_dot',   'shape', RID,        'dot'),
      edge(RID,        'dot',   'wm_renderer', 'layers'),
    ],
  };
}
