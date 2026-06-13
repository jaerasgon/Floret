import type { FloretNode } from '../../../editor/editor-state';
import { node, edge, note } from '../helpers';


// ─── Kinetic Type ──────────────────────────────────────────────────────────
// Cavalry-style traveling-wave kinetic typography. Every character of every
// row gets its own x offset = sin(time·SPEED + charIdx·CHAR_PHASE − rowIdx·
// ROW_DELAY). The CHAR_PHASE makes the letters within a row ripple
// independently; the ROW_DELAY means each row plays the same animation
// slightly later in time, so the wave appears to flow downward through the
// rows. Rows are flattened into a single ForEach: Repeat tiles the chars to
// (rows × chars), Range(rows × chars) gives a flat index, and mod/floor
// recover row + char inside the region.
const KT_ROWS = 40;
export function buildKineticType() {
  const c = note(
    'kt_c',
    { x: -640, y: 0 },
    'Kinetic typography\n\nEach character of every row gets its own (x, y). The phrase is split into a Spread<string> then Repeat-tiled rows times — paired with a Range of the same length, ForEach iterates the 24 × 9 grid. Inside: floor / mod recover (rowIdx, charIdx); the column offset (charIdx − centre) gets multiplied by a row-envelope so middle rows fan out and edges stay tight. The result curves like the reference — a wave of letterforms.\n\nEdit the String node to change the phrase; the grid count rebuilds itself.',
    { w: 520, h: 320 },
  );

  // Outer scope.
  const str   = node('kt_str',   'value/string',  { x: -580, y: 200 }, { value: 'I LOVE FLORET' });
  const split = node('kt_split', 'string/split',  { x: -380, y: 160 }, { by: '' });
  const len   = node('kt_len',   'string/length', { x: -380, y: 260 });
  // Repeat tiles the char spread KT_ROWS times: length = chars × rows.
  const tile  = node('kt_tile',  'spread/repeat', { x: -180, y: 160 }, { count: KT_ROWS });
  // Range count = chars × rows so the index iterates in lockstep.
  const total = node('kt_total', 'math/mul',      { x: -180, y: 260 }, { b: KT_ROWS });
  const range = node('kt_range', 'spread/range',  { x:    0, y: 260 });
  const time  = node('kt_time',  'time/seconds',  { x: -580, y: 360 });

  // ── Per-word colour pipeline ──
  // Author 3 swatches. WordIndices tags each char with its word index;
  // PickSpread looks up palette[wordIdx] per char → Spread<Color> of length
  // = chars per row. Repeat tiles it KT_ROWS times so it iterates in
  // lockstep with the char + index spreads inside the region.
  const palette = node('kt_palette', 'color/palette', { x: -580, y:  20 },
                       { stopCount: 3, c0: '#ff5edb', c1: '#7afc6c', c2: '#ffd23b' });
  const widx    = node('kt_widx',    'string/word-indices', { x: -380, y:  60 }, { by: ' ' });
  const perChar = node('kt_perChar', 'spread/pick',         { x: -180, y:  60 });
  const tileC   = node('kt_tileC',   'spread/repeat',       { x:    0, y:  60 }, { count: KT_ROWS });

  // Background — your reference's electric blue.
  const bg = node('kt_bg', 'color/color', { x: -380, y: 420 },
                  { mode: 'rgb', r: 41, g: 70, b: 219, a: 255 });

  const RID = 'kt_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 220, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1100, height: 700, label: 'ForEach (row × char)' },
      ports: [
        { id: 'ch',  name: 'ch',  type: 'string', direction: 'input' },
        { id: 'i',   name: 'i',   type: 'number', direction: 'input' },
        { id: 'col', name: 'col', type: 'color',  direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1100, height: 700 },
  };

  // ── Recover (rowIdx, charIdx) from the flat index `i` and chars-per-row N.
  // i is broadcast from the region; chars-per-row (N) is broadcast from kt_len.
  const charIdxOff = node('kt_charIdxOff', 'math/mod',   { x:  40, y:  40 }, {}, RID); // i % N
  const rowDiv     = node('kt_rowDiv',     'math/div',   { x:  40, y: 120 }, {}, RID); // i / N
  const rowIdx     = node('kt_rowIdx',     'math/floor', { x: 200, y: 120 }, {}, RID); // floor(i/N)

  // ── y = (rowIdx − (ROWS-1)/2) · ROW_H
  const ROW_H = 22;
  const yCenter = ((KT_ROWS - 1) / 2) * ROW_H;
  const yMul = node('kt_yMul', 'math/mul', { x: 360, y: 120 }, { b: ROW_H },   RID);
  const yOff = node('kt_yOff', 'math/add', { x: 520, y: 120 }, { b: -yCenter }, RID);

  // ── Char column offset: (charIdx − centre).
  // centre = (N − 1) / 2 — derive from chars-per-row so it stays correct.
  const charCenter = node('kt_charCenter', 'math/sub', { x: 200, y:  40 }, { b: 1 }, RID); // N - 1
  const charCenter2 = node('kt_charCenter2', 'math/div', { x: 360, y:  40 }, { b: 2 }, RID); // (N - 1) / 2
  const charOff = node('kt_charOff', 'math/sub', { x: 520, y:  40 }, {}, RID); // charIdx − centre

  // ── Traveling wave: arg = time · SPEED + charIdx · CHAR_PHASE − rowIdx · ROW_DELAY.
  // CHAR_PHASE makes letters within a row ripple at different times;
  // ROW_DELAY makes each row lag behind the one above so the whole wave
  // travels down the page.
  const tSpeed    = node('kt_tSpeed',    'math/mul', { x: 360, y: 280 }, { b: 1.6 }, RID);
  const charPhase = node('kt_charPhase', 'math/mul', { x: 360, y: 200 }, { b: 0.55 }, RID);
  const rowDelay  = node('kt_rowDelay',  'math/mul', { x: 360, y: 360 }, { b: 0.22 }, RID);
  const waveAdd   = node('kt_waveAdd',   'math/add', { x: 520, y: 240 }, {}, RID); // tScaled + charPhase
  const waveArg   = node('kt_waveArg',   'math/sub', { x: 680, y: 280 }, {}, RID); // − rowDelay
  const waveSin   = node('kt_waveSin',   'math/sin', { x: 840, y: 280 }, {}, RID);
  const xWave     = node('kt_xWave',     'math/mul', { x: 1000, y: 280 }, { b: 26 }, RID); // wave amplitude

  // ── x = charOff · BASE_SPACING + xWave
  const xBase = node('kt_xBase', 'math/mul', { x: 680, y:  40 }, { b: 24 }, RID);
  const xOut  = node('kt_xOut',  'math/add', { x: 1000, y:  40 }, {}, RID);

  // ── Per-iteration colour comes from the region's `col` port (palette
  //    indexed by word index, tiled across rows). Fill takes it directly.
  const fill  = node('kt_fill',  'style/fill',  { x: 220, y: 460 }, {}, RID);
  const tp    = node('kt_tp',    'style/text-params', { x: 400, y: 460 },
                     { font: 'monospace', bold: true, italic: false,
                       align: 'center', baseline: 'middle',
                       kerning: 0, lineSpacing: 1.0 }, RID);

  const text = node('kt_text', 'draw/text', { x: 880, y:  40 },
                    { string: '?', x: 0, y: 0, size: 16, align: 'center' }, RID);

  const renderer = node('kt_renderer', 'render/renderer', { x: 1380, y: 0 },
                        { width: 380, height: 640 });

  return {
    nodes: [
      c, str, split, len, tile, total, range, time, bg, region,
      palette, widx, perChar, tileC,
      charIdxOff, rowDiv, rowIdx,
      charCenter, charCenter2, charOff,
      yMul, yOff,
      tSpeed, charPhase, rowDelay, waveAdd, waveArg, waveSin, xWave,
      xBase, xOut,
      fill, tp, text,
      renderer,
    ],
    edges: [
      // Split phrase → char spread; Length → drive tile count + Range count.
      edge('kt_str', 'text', 'kt_split', 'text'),
      edge('kt_str', 'text', 'kt_len',   'text'),
      edge('kt_split', 'parts', 'kt_tile',  'value'),
      edge('kt_len', 'length', 'kt_total', 'a'),
      edge('kt_total', 'product', 'kt_range', 'count'),

      // Per-word colour pipeline: phrase → WordIndices → Pick(palette) →
      // tile by rows; lockstep with chars + indices.
      edge('kt_str', 'text',    'kt_widx',    'text'),
      edge('kt_palette', 'colors', 'kt_perChar', 'source'),
      edge('kt_widx', 'indices',    'kt_perChar', 'indices'),
      edge('kt_perChar', 'values',    'kt_tileC',   'value'),

      // Three lockstep spreads (length = rows × chars) into the region.
      edge('kt_tile', 'values', RID, 'ch'),
      edge('kt_range', 'indices', RID, 'i'),
      edge('kt_tileC', 'values', RID, 'col'),

      // ── Recover rowIdx + charIdx ──
      // charIdx = i % N
      edge(RID, 'i', 'kt_charIdxOff', 'a'),
      edge('kt_len', 'length', 'kt_charIdxOff', 'b'),
      // rowIdx  = floor(i / N)
      edge(RID, 'i', 'kt_rowDiv', 'a'),
      edge('kt_len', 'length', 'kt_rowDiv', 'b'),
      edge('kt_rowDiv', 'quotient', 'kt_rowIdx', 'x'),

      // ── y = (rowIdx − centre) · ROW_H ──
      edge('kt_rowIdx', 'floored', 'kt_yMul', 'a'),
      edge('kt_yMul', 'product', 'kt_yOff', 'a'),
      edge('kt_yOff', 'sum', 'kt_text', 'y'),

      // ── Char centre: (N − 1) / 2 ──
      edge('kt_len', 'length', 'kt_charCenter',  'a'),
      edge('kt_charCenter', 'difference', 'kt_charCenter2', 'a'),
      edge('kt_charIdxOff', 'remainder', 'kt_charOff',     'a'),
      edge('kt_charCenter2', 'quotient', 'kt_charOff',     'b'),

      // ── Wave arg = time·SPEED + charIdx·CHAR_PHASE − rowIdx·ROW_DELAY ──
      edge('kt_time',        't',  'kt_tSpeed',    'a'),
      edge('kt_charIdxOff', 'remainder', 'kt_charPhase','a'),
      edge('kt_rowIdx', 'floored', 'kt_rowDelay', 'a'),
      edge('kt_tSpeed', 'product', 'kt_waveAdd',  'a'),
      edge('kt_charPhase', 'product', 'kt_waveAdd',  'b'),
      edge('kt_waveAdd', 'sum', 'kt_waveArg',  'a'),
      edge('kt_rowDelay', 'product', 'kt_waveArg',  'b'),
      edge('kt_waveArg', 'difference', 'kt_waveSin',  'x'),
      edge('kt_waveSin', 'sine', 'kt_xWave',    'a'),

      // ── x = charOff·BASE_SPACING + xWave ──
      edge('kt_charOff', 'difference', 'kt_xBase', 'a'),
      edge('kt_xBase', 'product', 'kt_xOut',  'a'),
      edge('kt_xWave', 'product', 'kt_xOut',  'b'),
      edge('kt_xOut', 'sum', 'kt_text',  'x'),

      // Style chain — per-iter colour from the region's `col` port.
      edge(RID, 'col', 'kt_fill', 'color'),
      edge('kt_fill', 'style', 'kt_tp',   'base'),
      edge('kt_tp',   'style', 'kt_text', 'style'),

      // The character to draw (broadcast through the region).
      edge(RID, 'ch', 'kt_text', 'string'),

      // Output + background.
      edge('kt_text', 'layer', RID, 'layer'),
      edge(RID, 'layer', 'kt_renderer', 'layers'),
      edge('kt_bg', 'color', 'kt_renderer', 'background'),
    ],
  };
}


// ─── Type / Halo ──────────────────────────────────────────────────────────
// Phrase characters laid out evenly around a circle; the whole ring spins
// over time. Hue cycles per-character + per-time so the ring also runs a
// chromatic chase. One ForEach iteration per character — angle = i · (TAU /
// N) + time · SPIN; (cos, sin) · RADIUS gives (x, y).
export function buildTypeHalo() {
  const c = note('th_c', { x: -560, y: 0 },
    'Type halo\n\nThe phrase is split into characters, then a ForEach lays them around a circle. angle = i · (2π / N) + time · SPIN; (x, y) = (cos, sin) · RADIUS. Hue cycles per character + per time for a chromatic chase.',
    { w: 500, h: 240 });

  const str   = node('th_str',   'value/string',  { x: -560, y: 200 }, { value: 'GENERATIVE FLORET TYPE STUDIO ' });
  const split = node('th_split', 'string/split',  { x: -360, y: 160 });
  const len   = node('th_len',   'string/length', { x: -360, y: 260 });
  const range = node('th_range', 'spread/range',  { x: -160, y: 260 });
  const time  = node('th_time',  'time/seconds',  { x: -560, y: 320 });
  const bg    = node('th_bg',    'color/color',   { x: -360, y: 340 },
                     { mode: 'rgb', r: 12, g: 14, b: 24, a: 255 });

  const RID = 'th_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 80, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 920, height: 540, label: 'ForEach char' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 920, height: 540 },
  };

  const TAU = Math.PI * 2;
  const stepDiv = node('th_step',    'math/div', { x:  40, y:  40 }, { a: TAU }, RID); // TAU / N
  const angBase = node('th_angBase', 'math/mul', { x: 200, y:  40 }, {},          RID);
  const tSpin   = node('th_tSpin',   'math/mul', { x: 200, y: 120 }, { b: 0.35 }, RID);
  const angle   = node('th_angle',   'math/add', { x: 360, y:  40 }, {},          RID);
  const cosA    = node('th_cos',     'math/cos', { x: 520, y:  40 }, {},          RID);
  const sinA    = node('th_sin',     'math/sin', { x: 520, y: 120 }, {},          RID);
  const xOut    = node('th_x',       'math/mul', { x: 680, y:  40 }, { b: 150 }, RID);
  const yOut    = node('th_y',       'math/mul', { x: 680, y: 120 }, { b: 150 }, RID);
  // hue = i · 24 + time · 32
  const hueI    = node('th_hueI',    'math/mul', { x:  40, y: 240 }, { b: 24 }, RID);
  const hueT    = node('th_hueT',    'math/mul', { x:  40, y: 320 }, { b: 32 }, RID);
  const hue     = node('th_hue',     'math/add', { x: 200, y: 240 }, {},        RID);
  const hsl     = node('th_hsl',     'color/color', { x: 360, y: 240 },
                       { mode: 'hsl', h: 0, s: 90, l: 65, a: 255 }, RID);
  const fill    = node('th_fill',    'style/fill', { x: 520, y: 240 }, {}, RID);
  const tp      = node('th_tp',      'style/text-params', { x: 680, y: 240 },
                       { font: 'monospace', bold: true, italic: false,
                         align: 'center', baseline: 'middle' }, RID);
  const text    = node('th_text',    'draw/text', { x: 840, y:  40 },
                       { string: '?', size: 30, align: 'center' }, RID);

  const renderer = node('th_renderer', 'render/renderer', { x: 1080, y: 0 },
                        { width: 540, height: 540 });

  return {
    nodes: [c, str, split, len, range, time, bg, region,
            stepDiv, angBase, tSpin, angle, cosA, sinA, xOut, yOut,
            hueI, hueT, hue, hsl, fill, tp, text, renderer],
    edges: [
      edge('th_str', 'text', 'th_split', 'text'),
      edge('th_str', 'text', 'th_len',   'text'),
      edge('th_len', 'length', 'th_range', 'count'),
      edge('th_split', 'parts', RID, 'ch'),
      edge('th_range', 'indices', RID, 'i'),

      // angle
      edge('th_len', 'length', 'th_step',    'b'),
      edge(RID, 'i',          'th_angBase', 'a'),
      edge('th_step', 'quotient', 'th_angBase', 'b'),
      edge('th_time',   't',   'th_tSpin',   'a'),
      edge('th_angBase', 'product', 'th_angle',   'a'),
      edge('th_tSpin', 'product', 'th_angle',   'b'),

      // (x, y) on the ring
      edge('th_angle', 'sum', 'th_cos', 'x'),
      edge('th_angle', 'sum', 'th_sin', 'x'),
      edge('th_cos', 'cosine', 'th_x',   'a'),
      edge('th_sin', 'sine', 'th_y',   'a'),
      edge('th_x', 'product', 'th_text', 'x'),
      edge('th_y', 'product', 'th_text', 'y'),

      // hue
      edge(RID, 'i',          'th_hueI', 'a'),
      edge('th_time', 't',    'th_hueT', 'a'),
      edge('th_hueI', 'product',  'th_hue',  'a'),
      edge('th_hueT', 'product',  'th_hue',  'b'),
      edge('th_hue', 'sum',  'th_hsl',  'h'),

      // style + draw
      edge('th_hsl', 'color',   'th_fill', 'color'),
      edge('th_fill', 'style', 'th_tp',  'base'),
      edge('th_tp',   'style', 'th_text','style'),
      edge(RID, 'ch', 'th_text', 'string'),

      edge('th_text', 'layer', RID, 'layer'),
      edge(RID, 'layer', 'th_renderer', 'layers'),
      edge('th_bg', 'color', 'th_renderer', 'background'),
    ],
  };
}


// ─── Type / Spiral ────────────────────────────────────────────────────────
// Phrase tiled K times → laid out on a logarithmic spiral. Each character's
// radius grows with i; angle steps a small amount per character + drifts in
// time. The spiral as a whole rotates slowly while individual characters
// trace outward.
export function buildTypeSpiral() {
  const TILE = 6;
  const c = note('tsp_c', { x: -560, y: 0 },
    'Type spiral\n\nThe phrase is RepeatSpread-tiled so we have plenty of characters. They march outward: r = R0 + i · dR, angle = i · dAngle − time · SPIN. (cos, sin) · r places them on a logarithmic spiral that rotates as a whole.',
    { w: 500, h: 240 });

  const str   = node('tsp_str',   'value/string',  { x: -560, y: 200 }, { value: 'FLORET TYPE ' });
  const split = node('tsp_split', 'string/split',  { x: -360, y: 160 });
  const tile  = node('tsp_tile',  'spread/repeat', { x: -180, y: 160 }, { count: TILE });
  const len   = node('tsp_len',   'string/length', { x: -360, y: 260 });
  const total = node('tsp_total', 'math/mul',      { x: -180, y: 260 }, { b: TILE });
  const range = node('tsp_range', 'spread/range',  { x:    0, y: 260 });
  const time  = node('tsp_time',  'time/seconds',  { x: -560, y: 320 });
  const bg    = node('tsp_bg',    'color/color',   { x: -360, y: 340 },
                     { mode: 'rgb', r: 8, g: 8, b: 16, a: 255 });

  const RID = 'tsp_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 120, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 920, height: 540, label: 'ForEach char' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 920, height: 540 },
  };

  // angle = i · dAngle − time · SPIN
  const iAng    = node('tsp_iAng',  'math/mul', { x:  40, y:  40 }, { b: 0.42 }, RID);
  const tSpin   = node('tsp_tSpin', 'math/mul', { x:  40, y: 100 }, { b: 0.25 }, RID);
  const angle   = node('tsp_angle', 'math/sub', { x: 200, y:  40 }, {}, RID);
  // r = R0 + i · dR
  const rDelta  = node('tsp_rDelta','math/mul', { x:  40, y: 180 }, { b: 4 },   RID);
  const radius  = node('tsp_r',     'math/add', { x: 200, y: 180 }, { b: 28 }, RID);

  const cosA    = node('tsp_cos',   'math/cos', { x: 360, y:  40 }, {}, RID);
  const sinA    = node('tsp_sin',   'math/sin', { x: 360, y: 120 }, {}, RID);
  const xOut    = node('tsp_x',     'math/mul', { x: 520, y:  40 }, {}, RID);
  const yOut    = node('tsp_y',     'math/mul', { x: 520, y: 120 }, {}, RID);

  // hue = i · 16 + time · 40
  const hueI    = node('tsp_hueI',  'math/mul', { x:  40, y: 260 }, { b: 16 }, RID);
  const hueT    = node('tsp_hueT',  'math/mul', { x:  40, y: 320 }, { b: 40 }, RID);
  const hue     = node('tsp_hue',   'math/add', { x: 200, y: 260 }, {}, RID);
  const hsl     = node('tsp_hsl',   'color/color', { x: 360, y: 260 },
                       { mode: 'hsl', h: 0, s: 90, l: 65, a: 255 }, RID);
  const fill    = node('tsp_fill',  'style/fill', { x: 520, y: 260 }, {}, RID);
  const tp      = node('tsp_tp',    'style/text-params', { x: 680, y: 260 },
                       { font: 'monospace', bold: true, italic: false,
                         align: 'center', baseline: 'middle' }, RID);
  const text    = node('tsp_text',  'draw/text', { x: 700, y:  40 },
                       { string: '?', size: 20, align: 'center' }, RID);

  const renderer = node('tsp_renderer', 'render/renderer', { x: 940, y: 0 },
                        { width: 540, height: 540 });

  return {
    nodes: [c, str, split, tile, len, total, range, time, bg, region,
            iAng, tSpin, angle, rDelta, radius,
            cosA, sinA, xOut, yOut,
            hueI, hueT, hue, hsl, fill, tp, text, renderer],
    edges: [
      edge('tsp_str', 'text', 'tsp_split', 'text'),
      edge('tsp_str', 'text', 'tsp_len',   'text'),
      edge('tsp_split', 'parts', 'tsp_tile',  'value'),
      edge('tsp_len', 'length', 'tsp_total', 'a'),
      edge('tsp_total', 'product', 'tsp_range', 'count'),
      edge('tsp_tile', 'values', RID, 'ch'),
      edge('tsp_range', 'indices', RID, 'i'),

      // angle
      edge(RID, 'i',           'tsp_iAng',  'a'),
      edge('tsp_time', 't',    'tsp_tSpin', 'a'),
      edge('tsp_iAng', 'product',  'tsp_angle', 'a'),
      edge('tsp_tSpin', 'product',  'tsp_angle', 'b'),
      // r
      edge(RID, 'i',           'tsp_rDelta','a'),
      edge('tsp_rDelta', 'product', 'tsp_r',     'a'),

      edge('tsp_angle', 'difference', 'tsp_cos', 'x'),
      edge('tsp_angle', 'difference', 'tsp_sin', 'x'),
      edge('tsp_cos', 'cosine', 'tsp_x',   'a'),
      edge('tsp_r', 'sum', 'tsp_x',   'b'),
      edge('tsp_sin', 'sine', 'tsp_y',   'a'),
      edge('tsp_r', 'sum', 'tsp_y',   'b'),
      edge('tsp_x', 'product', 'tsp_text','x'),
      edge('tsp_y', 'product', 'tsp_text','y'),

      // hue
      edge(RID, 'i',           'tsp_hueI', 'a'),
      edge('tsp_time', 't',    'tsp_hueT', 'a'),
      edge('tsp_hueI', 'product',  'tsp_hue',  'a'),
      edge('tsp_hueT', 'product',  'tsp_hue',  'b'),
      edge('tsp_hue', 'sum',  'tsp_hsl',  'h'),

      // style + draw
      edge('tsp_hsl', 'color',   'tsp_fill', 'color'),
      edge('tsp_fill', 'style', 'tsp_tp',   'base'),
      edge('tsp_tp',   'style', 'tsp_text', 'style'),
      edge(RID, 'ch', 'tsp_text', 'string'),

      edge('tsp_text', 'layer', RID, 'layer'),
      edge(RID, 'layer', 'tsp_renderer', 'layers'),
      edge('tsp_bg', 'color', 'tsp_renderer', 'background'),
    ],
  };
}


// ─── Type / Drift ─────────────────────────────────────────────────────────
// A grid of repeated characters, each cell displaced by 2D Perlin noise.
// Two seeds for x and y → cells drift independently; the field flows over
// time. Classic noise-field-meets-text composition.
export function buildTypeDrift() {
  const COLS = 10, ROWS = 14, CELLS = COLS * ROWS;
  const c = note('tdr_c', { x: -560, y: 0 },
    'Type drift\n\nA grid of repeated characters (10 × 14) drifts through a 2D Perlin field. Per cell: (baseX, baseY) from row/col; noise samples at (baseX·s, baseY·s + time·v) for two seeds give independent x/y jitter. The field flows.',
    { w: 500, h: 240 });

  const str   = node('tdr_str',   'value/string',  { x: -560, y: 200 }, { value: 'FLORET ' });
  const split = node('tdr_split', 'string/split',  { x: -360, y: 160 });
  // Tile by CELLS so the spread is long enough to feed every cell.
  const tile  = node('tdr_tile',  'spread/repeat', { x: -180, y: 160 }, { count: CELLS });
  const range = node('tdr_range', 'spread/range',  { x: -180, y: 260 }, { count: CELLS });
  const time  = node('tdr_time',  'time/seconds',  { x: -560, y: 320 });
  const bg    = node('tdr_bg',    'color/color',   { x: -360, y: 340 },
                     { mode: 'rgb', r: 18, g: 22, b: 30, a: 255 });

  const RID = 'tdr_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 120, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1000, height: 580, label: 'ForEach cell' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1000, height: 580 },
  };

  // colIdx = i mod COLS; rowIdx = floor(i / COLS)
  const colIdx = node('tdr_colIdx', 'math/mod',   { x:  40, y:  40 }, { b: COLS }, RID);
  const rowDiv = node('tdr_rowDiv', 'math/div',   { x:  40, y: 100 }, { b: COLS }, RID);
  const rowIdx = node('tdr_rowIdx', 'math/floor', { x: 200, y: 100 }, {}, RID);

  // baseX = (colIdx − (COLS-1)/2) · COL_W
  const COL_W = 38, ROW_H = 30;
  const colOff = node('tdr_colOff', 'math/sub', { x: 200, y:  40 }, { b: (COLS - 1) / 2 }, RID);
  const baseX  = node('tdr_baseX',  'math/mul', { x: 360, y:  40 }, { b: COL_W }, RID);
  const rowOff = node('tdr_rowOff', 'math/sub', { x: 360, y: 100 }, { b: (ROWS - 1) / 2 }, RID);
  const baseY  = node('tdr_baseY',  'math/mul', { x: 520, y: 100 }, { b: ROW_H }, RID);

  // Noise inputs: scale baseX, baseY for noise space; add time·SPEED to baseY.
  const nxS    = node('tdr_nxS',    'math/mul', { x:  40, y: 180 }, { b: 0.012 }, RID);
  const nyS    = node('tdr_nyS',    'math/mul', { x:  40, y: 240 }, { b: 0.012 }, RID);
  const tDrift = node('tdr_tDrift', 'math/mul', { x:  40, y: 300 }, { b: 0.6 },   RID);
  const nyT    = node('tdr_nyT',    'math/add', { x: 200, y: 240 }, {},           RID);

  const nx = node('tdr_nx', 'noise/perlin', { x: 360, y: 180 }, { seed: 1, octaves: 2, falloff: 0.5 }, RID);
  const ny = node('tdr_ny', 'noise/perlin', { x: 360, y: 280 }, { seed: 7, octaves: 2, falloff: 0.5 }, RID);
  // jitter = (noise − 0.5) · AMP
  const nxSub  = node('tdr_nxSub', 'math/sub', { x: 520, y: 180 }, { b: 0.5 }, RID);
  const nySub  = node('tdr_nySub', 'math/sub', { x: 520, y: 280 }, { b: 0.5 }, RID);
  const xJit   = node('tdr_xJit',  'math/mul', { x: 680, y: 180 }, { b: 80 }, RID);
  const yJit   = node('tdr_yJit',  'math/mul', { x: 680, y: 280 }, { b: 80 }, RID);
  const xOut   = node('tdr_x',     'math/add', { x: 840, y:  60 }, {}, RID);
  const yOut   = node('tdr_y',     'math/add', { x: 840, y: 140 }, {}, RID);

  // hue from the noise itself — gives the field a colour wash.
  const hueScale = node('tdr_hueScale', 'math/mul', { x: 520, y: 380 }, { b: 220 }, RID);
  const hueBase  = node('tdr_hueBase',  'math/add', { x: 680, y: 380 }, { b: 200 }, RID);
  const hsl   = node('tdr_hsl', 'color/color', { x: 840, y: 380 },
                     { mode: 'hsl', h: 0, s: 70, l: 70, a: 255 }, RID);
  const fill  = node('tdr_fill','style/fill',  { x: 1000, y: 380 }, {}, RID);
  const tp    = node('tdr_tp',  'style/text-params', { x: 1160, y: 380 },
                     { font: 'monospace', bold: true, italic: false,
                       align: 'center', baseline: 'middle' }, RID);
  const text  = node('tdr_text','draw/text',   { x: 1000, y:  80 },
                     { string: '?', size: 20, align: 'center' }, RID);

  const renderer = node('tdr_renderer', 'render/renderer', { x: 1380, y: 0 },
                        { width: 600, height: 540 });

  return {
    nodes: [c, str, split, tile, range, time, bg, region,
            colIdx, rowDiv, rowIdx, colOff, baseX, rowOff, baseY,
            nxS, nyS, tDrift, nyT, nx, ny, nxSub, nySub, xJit, yJit, xOut, yOut,
            hueScale, hueBase, hsl, fill, tp, text, renderer],
    edges: [
      edge('tdr_str', 'text', 'tdr_split', 'text'),
      edge('tdr_split', 'parts', 'tdr_tile',  'value'),
      edge('tdr_tile', 'values', RID, 'ch'),
      edge('tdr_range', 'indices', RID, 'i'),

      // colIdx, rowIdx
      edge(RID, 'i',          'tdr_colIdx', 'a'),
      edge(RID, 'i',          'tdr_rowDiv', 'a'),
      edge('tdr_rowDiv', 'quotient','tdr_rowIdx', 'x'),

      // baseX, baseY
      edge('tdr_colIdx', 'remainder','tdr_colOff', 'a'),
      edge('tdr_colOff', 'difference','tdr_baseX',  'a'),
      edge('tdr_rowIdx', 'floored','tdr_rowOff', 'a'),
      edge('tdr_rowOff', 'difference','tdr_baseY',  'a'),

      // noise inputs
      edge('tdr_baseX', 'product', 'tdr_nxS', 'a'),
      edge('tdr_baseY', 'product', 'tdr_nyS', 'a'),
      edge('tdr_time',  't',   'tdr_tDrift', 'a'),
      edge('tdr_nyS', 'product', 'tdr_nyT', 'a'),
      edge('tdr_tDrift', 'product', 'tdr_nyT', 'b'),

      // noise sample
      edge('tdr_nxS', 'product', 'tdr_nx', 'x'),
      edge('tdr_nyT', 'sum', 'tdr_nx', 'y'),
      edge('tdr_nxS', 'product', 'tdr_ny', 'x'),
      edge('tdr_nyT', 'sum', 'tdr_ny', 'y'),

      // jitter
      edge('tdr_nx', 'noise', 'tdr_nxSub', 'a'),
      edge('tdr_ny', 'noise', 'tdr_nySub', 'a'),
      edge('tdr_nxSub', 'difference', 'tdr_xJit',  'a'),
      edge('tdr_nySub', 'difference', 'tdr_yJit',  'a'),

      // x, y
      edge('tdr_baseX', 'product', 'tdr_x', 'a'),
      edge('tdr_xJit', 'product', 'tdr_x', 'b'),
      edge('tdr_baseY', 'product', 'tdr_y', 'a'),
      edge('tdr_yJit', 'product', 'tdr_y', 'b'),
      edge('tdr_x', 'sum', 'tdr_text', 'x'),
      edge('tdr_y', 'sum', 'tdr_text', 'y'),

      // hue
      edge('tdr_nx', 'noise', 'tdr_hueScale', 'a'),
      edge('tdr_hueScale', 'product', 'tdr_hueBase',  'a'),
      edge('tdr_hueBase', 'sum', 'tdr_hsl',      'h'),

      // style + draw
      edge('tdr_hsl', 'color',   'tdr_fill', 'color'),
      edge('tdr_fill','style', 'tdr_tp',   'base'),
      edge('tdr_tp',  'style', 'tdr_text', 'style'),
      edge(RID, 'ch', 'tdr_text', 'string'),

      edge('tdr_text', 'layer', RID, 'layer'),
      edge(RID, 'layer', 'tdr_renderer', 'layers'),
      edge('tdr_bg', 'color', 'tdr_renderer', 'background'),
    ],
  };
}


// ─── Type / Rain ──────────────────────────────────────────────────────────
// Matrix-style falling characters. Each column scrolls at a per-column
// speed (noise-seeded so columns slip independently), with chars wrapping
// at the bottom of the canvas. Brightness fades from head (bright) to tail
// (dim) via a column-relative row factor.
export function buildTypeRain() {
  const COLS = 18, ROWS = 22, CELLS = COLS * ROWS;
  const CANVAS_H = 600;
  const c = note('trn_c', { x: -540, y: 0 },
    'Type rain\n\nA 18 × 22 grid where each column scrolls vertically at a unique noise-seeded speed. y = (rowIdx · ROW_H + time · speed[col]) mod CANVAS_H − CANVAS_H/2. Characters appear to fall like Matrix code.',
    { w: 500, h: 240 });

  const str   = node('trn_str',   'value/string',  { x: -540, y: 200 }, { value: 'FLORET01' });
  const split = node('trn_split', 'string/split',  { x: -360, y: 160 });
  const tile  = node('trn_tile',  'spread/repeat', { x: -180, y: 160 }, { count: CELLS });
  const range = node('trn_range', 'spread/range',  { x: -180, y: 260 }, { count: CELLS });
  const time  = node('trn_time',  'time/seconds',  { x: -540, y: 320 });
  const bg    = node('trn_bg',    'color/color',   { x: -360, y: 340 },
                     { mode: 'rgb', r: 4, g: 8, b: 6, a: 255 });

  const RID = 'trn_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 120, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1100, height: 600, label: 'ForEach cell' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1100, height: 600 },
  };

  const COL_W = 26, ROW_H = 26;
  // colIdx, rowIdx
  const colIdx = node('trn_colIdx', 'math/mod',   { x:  40, y:  40 }, { b: COLS }, RID);
  const rowDiv = node('trn_rowDiv', 'math/div',   { x:  40, y: 100 }, { b: COLS }, RID);
  const rowIdx = node('trn_rowIdx', 'math/floor', { x: 200, y: 100 }, {}, RID);

  // x = (colIdx − (COLS-1)/2) · COL_W
  const colOff = node('trn_colOff', 'math/sub', { x: 200, y:  40 }, { b: (COLS - 1) / 2 }, RID);
  const xOut   = node('trn_x',      'math/mul', { x: 360, y:  40 }, { b: COL_W }, RID);

  // Per-column speed: noise(colIdx · 0.27, 0) · 90 + 60   (60..150 px/s)
  const nArg   = node('trn_nArg',  'math/mul',     { x:  40, y: 180 }, { b: 0.27 }, RID);
  const speedN = node('trn_speedN','noise/perlin', { x: 200, y: 180 }, { seed: 13 }, RID);
  const speedM = node('trn_speedM','math/mul', { x: 360, y: 180 }, { b: 90 }, RID);
  const speed  = node('trn_speed', 'math/add', { x: 520, y: 180 }, { b: 60 }, RID);

  // scroll = time · speed
  const scroll = node('trn_scroll','math/mul', { x: 680, y: 180 }, {}, RID);
  // rowYRaw = rowIdx · ROW_H
  const rowY   = node('trn_rowY',  'math/mul', { x: 360, y: 260 }, { b: ROW_H }, RID);
  // ySum = rowY + scroll, then mod CANVAS_H, then − CANVAS_H/2
  const ySum   = node('trn_ySum',  'math/add', { x: 840, y: 220 }, {}, RID);
  const yWrap  = node('trn_yWrap', 'math/mod', { x: 1000, y: 220 }, { b: CANVAS_H }, RID);
  const yOut   = node('trn_y',     'math/sub', { x: 1160, y: 220 }, { b: CANVAS_H / 2 }, RID);

  // Brightness fade by yWrap / CANVAS_H (top = bright, bottom = dim).
  const yNorm  = node('trn_yNorm', 'math/div', { x: 1000, y: 320 }, { b: CANVAS_H }, RID);
  const dimSub = node('trn_dimSub','math/sub', { x: 1160, y: 320 }, { a: 1 }, RID); // 1 − yNorm
  const lOut   = node('trn_lOut',  'math/mul', { x: 1320, y: 320 }, { b: 70 }, RID); // 0..70

  const hsl   = node('trn_hsl', 'color/color', { x: 1480, y: 320 },
                     { mode: 'hsl', h: 140, s: 90, l: 60, a: 255 }, RID);
  const fill  = node('trn_fill','style/fill',  { x: 1640, y: 320 }, {}, RID);
  const tp    = node('trn_tp',  'style/text-params', { x: 1800, y: 320 },
                     { font: 'monospace', bold: true, italic: false,
                       align: 'center', baseline: 'middle' }, RID);
  const text  = node('trn_text','draw/text',   { x: 1320, y:  80 },
                     { string: '?', size: 18, align: 'center' }, RID);

  const renderer = node('trn_renderer', 'render/renderer', { x: 2000, y: 0 },
                        { width: 520, height: 600 });

  return {
    nodes: [c, str, split, tile, range, time, bg, region,
            colIdx, rowDiv, rowIdx, colOff, xOut,
            nArg, speedN, speedM, speed, scroll, rowY, ySum, yWrap, yOut,
            yNorm, dimSub, lOut, hsl, fill, tp, text, renderer],
    edges: [
      edge('trn_str', 'text', 'trn_split', 'text'),
      edge('trn_split', 'parts', 'trn_tile',  'value'),
      edge('trn_tile', 'values', RID, 'ch'),
      edge('trn_range', 'indices', RID, 'i'),

      // col/row
      edge(RID, 'i',           'trn_colIdx', 'a'),
      edge(RID, 'i',           'trn_rowDiv', 'a'),
      edge('trn_rowDiv', 'quotient', 'trn_rowIdx', 'x'),

      // x
      edge('trn_colIdx', 'remainder', 'trn_colOff', 'a'),
      edge('trn_colOff', 'difference', 'trn_x',      'a'),
      edge('trn_x', 'product', 'trn_text',   'x'),

      // per-col speed
      edge('trn_colIdx', 'remainder', 'trn_nArg',   'a'),
      edge('trn_nArg', 'product', 'trn_speedN', 'x'),
      edge('trn_speedN', 'noise', 'trn_speedM', 'a'),
      edge('trn_speedM', 'product', 'trn_speed',  'a'),

      // scroll
      edge('trn_time',  't',   'trn_scroll', 'a'),
      edge('trn_speed', 'sum', 'trn_scroll', 'b'),

      // y
      edge('trn_rowIdx', 'floored', 'trn_rowY',  'a'),
      edge('trn_rowY', 'product', 'trn_ySum',  'a'),
      edge('trn_scroll', 'product', 'trn_ySum',  'b'),
      edge('trn_ySum', 'sum', 'trn_yWrap', 'a'),
      edge('trn_yWrap', 'remainder', 'trn_y',     'a'),
      edge('trn_y', 'difference', 'trn_text',  'y'),

      // brightness from y
      edge('trn_yWrap', 'remainder', 'trn_yNorm', 'a'),
      edge('trn_yNorm', 'quotient', 'trn_dimSub','b'),
      edge('trn_dimSub', 'difference', 'trn_lOut',  'a'),
      edge('trn_lOut', 'product', 'trn_hsl',   'l'),

      // style + draw
      edge('trn_hsl', 'color',   'trn_fill', 'color'),
      edge('trn_fill', 'style', 'trn_tp',   'base'),
      edge('trn_tp',   'style', 'trn_text', 'style'),
      edge(RID, 'ch', 'trn_text', 'string'),

      edge('trn_text', 'layer', RID, 'layer'),
      edge(RID, 'layer', 'trn_renderer', 'layers'),
      edge('trn_bg', 'color', 'trn_renderer', 'background'),
    ],
  };
}
