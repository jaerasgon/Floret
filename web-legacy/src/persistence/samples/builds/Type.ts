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
  const RINGS = 3;
  const c = note('th_c', { x: -560, y: 0 },
    'Type halo\n\nThe phrase is tiled into 3 concentric RINGS. A ForEach lays each character around its ring: posInRing = i mod N, angle = posInRing·(2π/N) + spin (inner rings spin faster). Each letter’s size wiggles with sin(time + i), and hue cycles per character + time.',
    { w: 500, h: 260 });

  const str   = node('th_str',   'value/string',  { x: -560, y: 220 }, { value: 'GENERATIVE FLORET TYPE STUDIO ' });
  const split = node('th_split', 'string/split',  { x: -360, y: 180 });
  const len   = node('th_len',   'string/length', { x: -360, y: 280 });
  const total = node('th_total', 'math/mul',      { x: -180, y: 280 }, { b: RINGS }); // N · RINGS
  const range = node('th_range', 'spread/range',  { x:  -20, y: 280 });
  const tiled = node('th_tiled', 'spread/repeat', { x: -180, y: 180 }, { count: RINGS });
  const time  = node('th_time',  'time/seconds',  { x: -560, y: 360 });
  const bg    = node('th_bg',    'color/color',   { x: -360, y: 380 },
                     { mode: 'rgb', r: 12, g: 14, b: 24, a: 255 });

  const RID = 'th_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 120, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1480, height: 780, label: 'ForEach char' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1480, height: 780 },
  };

  const TAU = Math.PI * 2;
  // ring index + position within the ring (i / N, floor, i − ring·N)
  const step  = node('th_step',  'math/div',  { x:  40, y: 360 }, { a: TAU }, RID); // TAU / N
  const ringF = node('th_ringF', 'math/div',  { x:  40, y:  40 }, {},        RID); // i / N
  const ring  = node('th_ring',  'math/floor',{ x: 200, y:  40 }, {},        RID);
  const ringN = node('th_ringN', 'math/mul',  { x: 360, y:  40 }, {},        RID); // ring · N
  const posIn = node('th_posIn', 'math/sub',  { x: 520, y:  40 }, {},        RID); // i − ring·N
  const angB  = node('th_angB',  'math/mul',  { x: 680, y:  40 }, {},        RID); // posIn · step
  // differential spin — inner rings turn faster: spin = time·0.3 / (ring+1)
  const tBase = node('th_tBase', 'math/mul',  { x:  40, y: 200 }, { b: 0.3 }, RID);
  const r1    = node('th_r1',    'math/add',  { x: 360, y: 140 }, { b: 1 },   RID);
  const spin  = node('th_spin',  'math/div',  { x: 520, y: 160 }, {},         RID);
  const angle = node('th_angle', 'math/add',  { x: 840, y:  40 }, {},         RID);
  const cosN  = node('th_cos',   'math/cos',  { x:1000, y:  40 }, {},         RID);
  const sinN  = node('th_sin',   'math/sin',  { x:1000, y: 120 }, {},         RID);
  // radius = ring·70 + 90  → rings at 90 / 160 / 230
  const ringG = node('th_ringG', 'math/mul',  { x: 680, y: 200 }, { b: 70 }, RID);
  const radius= node('th_radius','math/add',  { x: 840, y: 200 }, { b: 90 }, RID);
  const xOut  = node('th_x',     'math/mul',  { x:1160, y:  40 }, {},        RID);
  const yOut  = node('th_y',     'math/mul',  { x:1160, y: 120 }, {},        RID);
  const pos   = node('th_pos',   'vec2/make', { x:1300, y:  60 }, {},        RID);
  // size wiggle: 26 + 7·sin(time·2.5 + i·0.7)
  const wT    = node('th_wT',    'math/mul',  { x:  40, y: 460 }, { b: 2.5 }, RID);
  const wI    = node('th_wI',    'math/mul',  { x:  40, y: 540 }, { b: 0.7 }, RID);
  const wPh   = node('th_wPh',   'math/add',  { x: 200, y: 500 }, {},        RID);
  const wSin  = node('th_wSin',  'math/sin',  { x: 360, y: 500 }, {},        RID);
  const wAmp  = node('th_wAmp',  'math/mul',  { x: 520, y: 500 }, { b: 7 },  RID);
  const size  = node('th_size',  'math/add',  { x: 680, y: 500 }, { b: 26 }, RID);
  // hue = i·16 + time·30
  const hueI  = node('th_hueI',  'math/mul',  { x:  40, y: 640 }, { b: 16 }, RID);
  const hueT  = node('th_hueT',  'math/mul',  { x:  40, y: 700 }, { b: 30 }, RID);
  const hue   = node('th_hue',   'math/add',  { x: 200, y: 660 }, {},       RID);
  const hsl   = node('th_hsl',   'color/color',{ x: 360, y: 640 },
                     { mode: 'hsl', h: 0, s: 90, l: 65, a: 255 }, RID);
  const fill  = node('th_fill',  'style/fill', { x: 520, y: 640 }, {}, RID);
  const tp    = node('th_tp',    'style/text-params', { x: 680, y: 640 },
                     { font: 'monospace', bold: true, italic: false,
                       align: 'center', baseline: 'middle' }, RID);
  const text  = node('th_text',  'draw/text', { x:1300, y: 320 },
                     { string: '?', size: 30, align: 'center' }, RID);

  const renderer = node('th_renderer', 'render/renderer', { x: 1680, y: 0 },
                        { width: 560, height: 560 });

  return {
    nodes: [c, str, split, len, total, range, tiled, time, bg, region,
            step, ringF, ring, ringN, posIn, angB, tBase, r1, spin, angle, cosN, sinN,
            ringG, radius, xOut, yOut, pos, wT, wI, wPh, wSin, wAmp, size,
            hueI, hueT, hue, hsl, fill, tp, text, renderer],
    edges: [
      edge('th_str', 'text', 'th_split', 'text'),
      edge('th_str', 'text', 'th_len',   'text'),
      edge('th_len', 'length', 'th_total', 'a'),
      edge('th_total', 'result', 'th_range', 'count'),
      edge('th_split', 'parts', 'th_tiled', 'source'),
      edge('th_tiled', 'values', RID, 'ch'),
      edge('th_range', 'indices', RID, 'i'),

      // N (= phrase length) broadcasts into the inner math
      edge('th_len', 'length', 'th_step',  'b'),
      edge('th_len', 'length', 'th_ringF', 'b'),
      edge('th_len', 'length', 'th_ringN', 'b'),
      // ring + position-in-ring → base angle
      edge(RID, 'i', 'th_ringF', 'a'),
      edge('th_ringF', 'result', 'th_ring', 'x'),
      edge('th_ring', 'result', 'th_ringN', 'a'),
      edge(RID, 'i', 'th_posIn', 'a'),
      edge('th_ringN', 'result', 'th_posIn', 'b'),
      edge('th_posIn', 'result', 'th_angB', 'a'),
      edge('th_step', 'result', 'th_angB', 'b'),
      // differential spin
      edge('th_time', 't', 'th_tBase', 'a'),
      edge('th_ring', 'result', 'th_r1', 'a'),
      edge('th_tBase', 'result', 'th_spin', 'a'),
      edge('th_r1', 'result', 'th_spin', 'b'),
      edge('th_angB', 'result', 'th_angle', 'a'),
      edge('th_spin', 'result', 'th_angle', 'b'),
      // (x, y) on the ring
      edge('th_angle', 'result', 'th_cos', 'x'),
      edge('th_angle', 'result', 'th_sin', 'x'),
      edge('th_ring', 'result', 'th_ringG', 'a'),
      edge('th_ringG', 'result', 'th_radius', 'a'),
      edge('th_cos', 'result', 'th_x', 'a'),
      edge('th_radius', 'result', 'th_x', 'b'),
      edge('th_sin', 'result', 'th_y', 'a'),
      edge('th_radius', 'result', 'th_y', 'b'),
      edge('th_x', 'result', 'th_pos', 'x'),
      edge('th_y', 'result', 'th_pos', 'y'),
      edge('th_pos', 'vector', 'th_text', 'pos'),
      // size wiggle
      edge('th_time', 't', 'th_wT', 'a'),
      edge(RID, 'i', 'th_wI', 'a'),
      edge('th_wT', 'result', 'th_wPh', 'a'),
      edge('th_wI', 'result', 'th_wPh', 'b'),
      edge('th_wPh', 'result', 'th_wSin', 'x'),
      edge('th_wSin', 'result', 'th_wAmp', 'a'),
      edge('th_wAmp', 'result', 'th_size', 'a'),
      edge('th_size', 'result', 'th_text', 'size'),
      // hue
      edge(RID, 'i', 'th_hueI', 'a'),
      edge('th_time', 't', 'th_hueT', 'a'),
      edge('th_hueI', 'result', 'th_hue', 'a'),
      edge('th_hueT', 'result', 'th_hue', 'b'),
      edge('th_hue', 'result', 'th_hsl', 'h'),
      // style + draw
      edge('th_hsl', 'color', 'th_fill', 'color'),
      edge('th_fill', 'style', 'th_tp', 'base'),
      edge('th_tp', 'style', 'th_text', 'style'),
      edge(RID, 'ch', 'th_text', 'string'),
      edge('th_text', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'th_renderer', 'layers'),
      edge('th_bg', 'color', 'th_renderer', 'background'),
    ],
  };
}




// ─── Type / The Raven ──────────────────────────────────────────────────────
// A passage from Poe's "The Raven" (public domain) laid out as a serif grid.
// Each letter's brightness is its own slow Perlin walk — letters fade in and
// out independently in black & white — and the dimmer a letter gets, the more
// it blurs (alpha → blur via the new style/blur). An eerie breathing manuscript.
export function buildTypeRaven() {
  const LINES = [
    'Once upon a midnight dreary,',
    'while I pondered, weak and weary,',
    'over many a quaint and curious',
    'volume of forgotten lore —',
    'while I nodded, nearly napping,',
    'suddenly there came a tapping,',
  ];
  const COLS = Math.max(...LINES.map((l) => l.length)); // pad width
  const ROWS = LINES.length;
  const TOTAL = COLS * ROWS;
  const CHARW = 13;
  const LINEH = 38;
  // Pad every line to COLS so the flat char list is a clean ROWS×COLS grid;
  // padding spaces draw nothing, preserving the layout.
  const passage = LINES.map((l) => l.padEnd(COLS)).join('');

  const c = note('rv_c', { x: -560, y: 0 },
    'The Raven\n\nA passage laid out as a serif grid. Each letter\'s opacity is its own slow Perlin walk — noise(i, time) — so letters fade in and out independently in black & white, and the dimmer a letter gets, the more it blurs.',
    { w: 500, h: 240 });

  const str   = node('rv_str',   'value/string', { x: -560, y: 220 }, { value: passage });
  const split = node('rv_split', 'string/split', { x: -360, y: 200 });
  const range = node('rv_range', 'spread/range', { x: -360, y: 300 }, { count: TOTAL });
  const time  = node('rv_time',  'time/seconds', { x: -560, y: 360 });
  const bg    = node('rv_bg',    'color/color',  { x: -360, y: 380 }, { mode: 'rgb', r: 8, g: 8, b: 10, a: 255 });

  const RID = 'rv_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 120, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1480, height: 700, label: 'ForEach letter' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1480, height: 700 },
  };

  // grid position: row = floor(i / COLS), col = i − row·COLS
  const rowF  = node('rv_rowF',  'math/div',  { x:  40, y:  40 }, { b: COLS }, RID);
  const row   = node('rv_row',   'math/floor',{ x: 200, y:  40 }, {},         RID);
  const rowC  = node('rv_rowCol','math/mul',  { x: 360, y:  40 }, { b: COLS }, RID);
  const col   = node('rv_col',   'math/sub',  { x: 520, y:  40 }, {},         RID);
  const colCen= node('rv_colCen','math/sub',  { x: 680, y:  40 }, { b: (COLS - 1) / 2 }, RID);
  const xOut  = node('rv_x',     'math/mul',  { x: 840, y:  40 }, { b: CHARW }, RID);
  const rowCen= node('rv_rowCen','math/sub',  { x: 360, y: 140 }, { b: (ROWS - 1) / 2 }, RID);
  const yOut  = node('rv_y',     'math/mul',  { x: 520, y: 140 }, { b: LINEH }, RID);
  const pos   = node('rv_pos',   'vec2/make', { x:1000, y:  80 }, {},          RID);

  // per-letter opacity: a slow Perlin walk, mapped to the full 0..1 range
  const ni    = node('rv_ni',    'math/mul',  { x:  40, y: 260 }, { b: 9.1 },  RID); // i · 9.1 (decorrelate)
  const ts    = node('rv_ts',    'math/mul',  { x:  40, y: 340 }, { b: 0.16 }, RID); // time · 0.16 (slow drift)
  const n     = node('rv_noise', 'noise/perlin', { x: 200, y: 280 }, {},       RID);
  // Narrow low window → only the bottom tail of the noise dips below full
  // opacity, so just a few letters fade at once and the verse stays readable.
  const alpha = node('rv_alpha', 'math/map',  { x: 360, y: 280 }, { inMin: 0.22, inMax: 0.37, outMin: 0, outMax: 1 }, RID);
  const a255  = node('rv_a255',  'math/mul',  { x: 520, y: 280 }, { b: 255 },  RID);
  // blur grows as the letter fades: (1 − alpha) · 7
  const inv   = node('rv_inv',   'math/sub',  { x: 360, y: 380 }, { a: 1 },    RID);
  const blurA = node('rv_blurA', 'math/mul',  { x: 520, y: 380 }, { b: 7 },    RID);

  const white = node('rv_white', 'color/color', { x: 680, y: 280 }, { mode: 'rgb', r: 245, g: 245, b: 245, a: 255 }, RID);
  const fill  = node('rv_fill',  'style/fill', { x: 840, y: 280 }, {}, RID);
  const tp    = node('rv_tp',    'style/text-params', { x: 1000, y: 280 },
                     { font: 'Georgia, "Times New Roman", serif', bold: false, italic: false,
                       align: 'center', baseline: 'middle' }, RID);
  const blur  = node('rv_blur',  'style/blur', { x: 1160, y: 280 }, {}, RID);
  const text  = node('rv_text',  'draw/text',  { x: 1180, y:  80 }, { string: '?', size: 22, align: 'center' }, RID);

  const renderer = node('rv_renderer', 'render/renderer', { x: 1680, y: 0 }, { width: 560, height: 360 });

  return {
    nodes: [c, str, split, range, time, bg, region,
            rowF, row, rowC, col, colCen, xOut, rowCen, yOut, pos,
            ni, ts, n, alpha, a255, inv, blurA, white, fill, tp, blur, text, renderer],
    edges: [
      edge('rv_str', 'text', 'rv_split', 'text'),
      edge('rv_split', 'parts', RID, 'ch'),
      edge('rv_range', 'indices', RID, 'i'),
      edge('rv_bg', 'color', 'rv_renderer', 'background'),

      // grid position
      edge(RID, 'i', 'rv_rowF', 'a'),
      edge('rv_rowF', 'result', 'rv_row', 'x'),
      edge('rv_row', 'result', 'rv_rowCol', 'a'),
      edge(RID, 'i', 'rv_col', 'a'),
      edge('rv_rowCol', 'result', 'rv_col', 'b'),
      edge('rv_col', 'result', 'rv_colCen', 'a'),
      edge('rv_colCen', 'result', 'rv_x', 'a'),
      edge('rv_row', 'result', 'rv_rowCen', 'a'),
      edge('rv_rowCen', 'result', 'rv_y', 'a'),
      edge('rv_x', 'result', 'rv_pos', 'x'),
      edge('rv_y', 'result', 'rv_pos', 'y'),
      edge('rv_pos', 'vector', 'rv_text', 'pos'),

      // opacity walk
      edge(RID, 'i', 'rv_ni', 'a'),
      edge('rv_time', 't', 'rv_ts', 'a'),
      edge('rv_ni', 'result', 'rv_noise', 'x'),
      edge('rv_ts', 'result', 'rv_noise', 'y'),
      edge('rv_noise', 'noise', 'rv_alpha', 'x'),
      edge('rv_alpha', 'result', 'rv_a255', 'a'),
      edge('rv_a255', 'result', 'rv_white', 'a'),
      // blur from fade
      edge('rv_alpha', 'result', 'rv_inv', 'b'),
      edge('rv_inv', 'result', 'rv_blurA', 'a'),
      edge('rv_blurA', 'result', 'rv_blur', 'amount'),

      // style chain + draw
      edge('rv_white', 'color', 'rv_fill', 'color'),
      edge('rv_fill', 'style', 'rv_tp', 'base'),
      edge('rv_tp', 'style', 'rv_blur', 'base'),
      edge('rv_blur', 'style', 'rv_text', 'style'),
      edge(RID, 'ch', 'rv_text', 'string'),
      edge('rv_text', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'rv_renderer', 'layers'),
    ],
  };
}



// ─── Type / Noise Drift ─────────────────────────────────────────────────────
// One long passage laid out as a monospace grid. A slow, drifting Perlin
// "mask" field selects CLUSTERS of letters (smoothstep threshold); inside a
// cluster the letters are shoved around by a second noise displacement field
// and tinted, while everywhere else the text sits still and readable. The
// clusters wander over time, so the disturbance roams across the passage.
export function buildTypeDrift() {
  const LINES = [
    'It was the best of times,',
    'it was the worst of times,',
    'it was the age of wisdom,',
    'it was the age of foolishness,',
    'it was the epoch of belief,',
    'it was the epoch of incredulity,',
    'it was the season of Light,',
    'it was the season of Darkness.',
  ];
  const COLS = Math.max(...LINES.map((l) => l.length));
  const ROWS = LINES.length;
  const TOTAL = COLS * ROWS;
  const CHARW = 12;
  const LINEH = 30;
  const AMP = 26; // max cluster displacement (px)
  const passage = LINES.map((l) => l.padEnd(COLS)).join('');

  const c = note('tdr_c', { x: -560, y: 0 },
    'Noise drift\n\nThe passage sits as a monospace grid. A slow Perlin “mask” (smoothstep threshold) picks drifting CLUSTERS; inside them a second noise field shoves the letters around and tints them, while the rest stays still and readable. The clusters wander over time.',
    { w: 500, h: 240 });

  const str   = node('tdr_str',   'value/string', { x: -560, y: 220 }, { value: passage });
  const split = node('tdr_split', 'string/split', { x: -360, y: 200 });
  const range = node('tdr_range', 'spread/range', { x: -360, y: 300 }, { count: TOTAL });
  const time  = node('tdr_time',  'time/seconds', { x: -560, y: 360 });
  const bg    = node('tdr_bg',    'color/color',  { x: -360, y: 380 }, { mode: 'rgb', r: 10, g: 11, b: 16, a: 255 });

  const RID = 'tdr_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 120, y: 0 },
    data: {
      defType: 'region/foreach',
      params: { width: 1560, height: 760, label: 'ForEach letter' },
      ports: [
        { id: 'ch', name: 'ch', type: 'string', direction: 'input' },
        { id: 'i',  name: 'i',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 1560, height: 760 },
  };

  // grid position
  const rowF  = node('tdr_rowF',  'math/div',  { x:  40, y:  40 }, { b: COLS }, RID);
  const row   = node('tdr_row',   'math/floor',{ x: 200, y:  40 }, {},         RID);
  const rowCol= node('tdr_rowCol','math/mul',  { x: 360, y:  40 }, { b: COLS }, RID);
  const col   = node('tdr_col',   'math/sub',  { x: 520, y:  40 }, {},         RID);
  const colCen= node('tdr_colCen','math/sub',  { x: 680, y:  40 }, { b: (COLS - 1) / 2 }, RID);
  const gx    = node('tdr_gx',    'math/mul',  { x: 840, y:  40 }, { b: CHARW }, RID);
  const rowCen= node('tdr_rowCen','math/sub',  { x: 360, y: 140 }, { b: (ROWS - 1) / 2 }, RID);
  const gy    = node('tdr_gy',    'math/mul',  { x: 520, y: 140 }, { b: LINEH }, RID);

  // scaled noise coordinates (+ slow time drift on one axis)
  const gsx   = node('tdr_gsx',   'math/mul',  { x:  40, y: 240 }, { b: 0.012 }, RID);
  const gsy   = node('tdr_gsy',   'math/mul',  { x:  40, y: 300 }, { b: 0.012 }, RID);
  const tDisp = node('tdr_tDisp', 'math/mul',  { x:  40, y: 360 }, { b: 0.30 }, RID);
  const gsyT  = node('tdr_gsyT',  'math/add',  { x: 200, y: 320 }, {},          RID);
  const msx   = node('tdr_msx',   'math/mul',  { x:  40, y: 440 }, { b: 0.022 }, RID);
  const msy   = node('tdr_msy',   'math/mul',  { x:  40, y: 500 }, { b: 0.022 }, RID);
  const tMask = node('tdr_tMask', 'math/mul',  { x:  40, y: 560 }, { b: 0.15 }, RID);
  const msyT  = node('tdr_msyT',  'math/add',  { x: 200, y: 520 }, {},          RID);

  // noise fields: displacement (x, y — different seeds) + cluster mask
  const nX    = node('tdr_nX',    'noise/perlin', { x: 360, y: 240 }, { seed: 7 },  RID);
  const nY    = node('tdr_nY',    'noise/perlin', { x: 360, y: 320 }, { seed: 23 }, RID);
  const mN    = node('tdr_mN',    'noise/perlin', { x: 360, y: 480 }, { seed: 99 }, RID);
  const mask  = node('tdr_mask',  'math/smoothstep', { x: 540, y: 480 }, { edge0: 0.55, edge1: 0.78 }, RID);

  // displacement = map(noise → ±AMP) · mask
  const dxM   = node('tdr_dxM',   'math/map',  { x: 540, y: 240 }, { inMin: 0.3, inMax: 0.7, outMin: -AMP, outMax: AMP }, RID);
  const dyM   = node('tdr_dyM',   'math/map',  { x: 540, y: 320 }, { inMin: 0.3, inMax: 0.7, outMin: -AMP, outMax: AMP }, RID);
  const dxg   = node('tdr_dxg',   'math/mul',  { x: 720, y: 240 }, {}, RID);
  const dyg   = node('tdr_dyg',   'math/mul',  { x: 720, y: 320 }, {}, RID);
  const fx    = node('tdr_fx',    'math/add',  { x: 900, y: 120 }, {}, RID);
  const fy    = node('tdr_fy',    'math/add',  { x: 900, y: 200 }, {}, RID);
  const pos   = node('tdr_pos',   'vec2/make', { x:1060, y: 160 }, {}, RID);

  // colour: clusters tint (hue + sat rise with the mask); rest stays near-white
  const hueM  = node('tdr_hueM',  'math/mul',  { x: 720, y: 480 }, { b: 220 }, RID);
  const satM  = node('tdr_satM',  'math/mul',  { x: 720, y: 540 }, { b: 90 },  RID);
  const hsl   = node('tdr_hsl',   'color/color', { x: 900, y: 480 }, { mode: 'hsl', h: 0, s: 0, l: 82, a: 255 }, RID);
  const fill  = node('tdr_fill',  'style/fill', { x:1060, y: 480 }, {}, RID);
  const tp    = node('tdr_tp',    'style/text-params', { x:1220, y: 480 },
                     { font: 'Menlo, Consolas, monospace', bold: false, italic: false,
                       align: 'center', baseline: 'middle' }, RID);
  const text  = node('tdr_text',  'draw/text', { x:1240, y: 160 }, { string: '?', size: 18, align: 'center' }, RID);

  const renderer = node('tdr_renderer', 'render/renderer', { x: 1760, y: 0 }, { width: 560, height: 340 });

  return {
    nodes: [c, str, split, range, time, bg, region,
            rowF, row, rowCol, col, colCen, gx, rowCen, gy,
            gsx, gsy, tDisp, gsyT, msx, msy, tMask, msyT,
            nX, nY, mN, mask, dxM, dyM, dxg, dyg, fx, fy, pos,
            hueM, satM, hsl, fill, tp, text, renderer],
    edges: [
      edge('tdr_str', 'text', 'tdr_split', 'text'),
      edge('tdr_split', 'parts', RID, 'ch'),
      edge('tdr_range', 'indices', RID, 'i'),
      edge('tdr_bg', 'color', 'tdr_renderer', 'background'),

      // grid position
      edge(RID, 'i', 'tdr_rowF', 'a'),
      edge('tdr_rowF', 'result', 'tdr_row', 'x'),
      edge('tdr_row', 'result', 'tdr_rowCol', 'a'),
      edge(RID, 'i', 'tdr_col', 'a'),
      edge('tdr_rowCol', 'result', 'tdr_col', 'b'),
      edge('tdr_col', 'result', 'tdr_colCen', 'a'),
      edge('tdr_colCen', 'result', 'tdr_gx', 'a'),
      edge('tdr_row', 'result', 'tdr_rowCen', 'a'),
      edge('tdr_rowCen', 'result', 'tdr_gy', 'a'),

      // scaled coords + time drift
      edge('tdr_gx', 'result', 'tdr_gsx', 'a'),
      edge('tdr_gy', 'result', 'tdr_gsy', 'a'),
      edge('tdr_time', 't', 'tdr_tDisp', 'a'),
      edge('tdr_gsy', 'result', 'tdr_gsyT', 'a'),
      edge('tdr_tDisp', 'result', 'tdr_gsyT', 'b'),
      edge('tdr_gx', 'result', 'tdr_msx', 'a'),
      edge('tdr_gy', 'result', 'tdr_msy', 'a'),
      edge('tdr_time', 't', 'tdr_tMask', 'a'),
      edge('tdr_msy', 'result', 'tdr_msyT', 'a'),
      edge('tdr_tMask', 'result', 'tdr_msyT', 'b'),

      // noise fields
      edge('tdr_gsx', 'result', 'tdr_nX', 'x'),
      edge('tdr_gsyT', 'result', 'tdr_nX', 'y'),
      edge('tdr_gsx', 'result', 'tdr_nY', 'x'),
      edge('tdr_gsyT', 'result', 'tdr_nY', 'y'),
      edge('tdr_msx', 'result', 'tdr_mN', 'x'),
      edge('tdr_msyT', 'result', 'tdr_mN', 'y'),
      edge('tdr_mN', 'noise', 'tdr_mask', 'x'),

      // displacement gated by mask, added to grid position
      edge('tdr_nX', 'noise', 'tdr_dxM', 'x'),
      edge('tdr_nY', 'noise', 'tdr_dyM', 'x'),
      edge('tdr_dxM', 'result', 'tdr_dxg', 'a'),
      edge('tdr_mask', 'result', 'tdr_dxg', 'b'),
      edge('tdr_dyM', 'result', 'tdr_dyg', 'a'),
      edge('tdr_mask', 'result', 'tdr_dyg', 'b'),
      edge('tdr_gx', 'result', 'tdr_fx', 'a'),
      edge('tdr_dxg', 'result', 'tdr_fx', 'b'),
      edge('tdr_gy', 'result', 'tdr_fy', 'a'),
      edge('tdr_dyg', 'result', 'tdr_fy', 'b'),
      edge('tdr_fx', 'result', 'tdr_pos', 'x'),
      edge('tdr_fy', 'result', 'tdr_pos', 'y'),
      edge('tdr_pos', 'vector', 'tdr_text', 'pos'),

      // colour tint from mask
      edge('tdr_mask', 'result', 'tdr_hueM', 'a'),
      edge('tdr_mask', 'result', 'tdr_satM', 'a'),
      edge('tdr_hueM', 'result', 'tdr_hsl', 'h'),
      edge('tdr_satM', 'result', 'tdr_hsl', 's'),
      edge('tdr_hsl', 'color', 'tdr_fill', 'color'),
      edge('tdr_fill', 'style', 'tdr_tp', 'base'),
      edge('tdr_tp', 'style', 'tdr_text', 'style'),
      edge(RID, 'ch', 'tdr_text', 'string'),
      edge('tdr_text', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'tdr_renderer', 'layers'),
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
