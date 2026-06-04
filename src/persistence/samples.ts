import type { FloretEdge, FloretNode } from '../editor/editor-state';

/**
 * Sample patches — under the regions-only model with frame-edge ports.
 * Each region declares its dynamic ports on `data.ports`; cross-boundary
 * wires connect to handles like `in-<portId>` / `out-<portId>` on the
 * region itself.
 */

export interface Sample {
  id: string;
  name: string;
  description: string;
  /**
   * Category bucket — controls which tab the sample appears under in
   * the Samples menu. Missing / empty falls back to "Misc".
   */
  category?: string;
  build: () => { nodes: FloretNode[]; edges: FloretEdge[] };
}

let _seq = 0;
function uid(prefix: string): string {
  _seq += 1;
  return `${prefix}_${_seq}`;
}

/**
 * Build a node and mirror every entry in `params` into `portValues`.
 *
 * Most node types in the runtime expose their configuration as INPUT
 * ports (e.g. `math/mul.b`, `spread/range.count`, `draw/rect.w`) — not
 * as params. Hand-writing a sample patch with constants for every
 * input would mean dropping a `value/number` node + wire for every
 * single numeric literal. So we sneak the same key/value pairs in via
 * `portValues`, which the runtime uses as the fallback for any
 * unwired input.
 *
 * Entries that don't match an input port (e.g. `{ type: 'sine' }` on
 * sound/oscillator — `type` is a param, not an input) are simply
 * ignored on the portValues side; the params copy still binds.
 */
function node(
  id: string,
  defType: string,
  pos: { x: number; y: number },
  params: Record<string, unknown> = {},
  parentId?: string,
  extra: Partial<FloretNode> = {},
): FloretNode {
  const n: FloretNode = {
    id,
    type: 'floret',
    position: pos,
    data: {
      defType,
      params,
      ...(Object.keys(params).length > 0 ? { portValues: { ...params } } : {}),
    },
    ...extra,
  };
  if (parentId) {
    n.parentId = parentId;
    n.extent = 'parent';
  }
  return n;
}

/** Backwards-compatible alias — the `node()` helper now mirrors params
 *  into portValues automatically. */
const nodeWithPorts = node;

/**
 * Post-process pass that scales node positions and region frame
 * dimensions so default samples have breathing room. Hand-written
 * sample positions were laid out tight; this gives each node enough
 * space that headers, pin labels and inline widgets don't overlap.
 *
 * Scales x by SAMPLE_SX, y by SAMPLE_SY. Renderer `width` / `height`
 * params represent CANVAS size, not layout — explicitly skipped. Same
 * for image/from-layer `w` / `h` and io/image-url, which are pixel
 * dimensions of the rendered surface, not editor layout.
 *
 * Y is scaled more aggressively than X because tight y-spacing was
 * the dominant overlap source — many samples stacked nodes at 60–80 px
 * y-steps, but typical node heights are 80–160 px. The x dimension
 * had more breathing room to start with.
 */
const SAMPLE_SX = 1.4;
const SAMPLE_SY = 2.5;

function spaced(result: { nodes: FloretNode[]; edges: FloretEdge[] }) {
  for (const n of result.nodes) {
    n.position = {
      x: n.position.x * SAMPLE_SX,
      y: n.position.y * SAMPLE_SY,
    };
    // Region frames have width/height in both `style` and `data.params` —
    // they describe the FRAME size in the editor canvas, not anything
    // pixel-rendered. Scale both so the frame keeps fitting its (now
    // spaced) children. render/renderer's width/height represent the
    // p5 CANVAS size and must NOT be touched; same for image/from-layer.
    if (n.data.defType.startsWith('region/') || n.data.defType === 'meta/subpatch') {
      const p = n.data.params as Record<string, unknown>;
      if (typeof p.width === 'number') p.width = p.width * SAMPLE_SX;
      if (typeof p.height === 'number') p.height = p.height * SAMPLE_SY;
      const s = n.style as { width?: number; height?: number } | undefined;
      if (s) {
        if (typeof s.width === 'number') s.width = s.width * SAMPLE_SX;
        if (typeof s.height === 'number') s.height = s.height * SAMPLE_SY;
      }
    }
  }
  return result;
}

function edge(
  source: string,
  sourcePort: string,
  target: string,
  targetPort: string,
): FloretEdge {
  return {
    id: uid('e'),
    source,
    target,
    sourceHandle: `out-${sourcePort}`,
    targetHandle: `in-${targetPort}`,
  };
}

// ─── Sample: Lissajous Threads ─────────────────────────────────────────────
// 240 dots traced along (cos(a·t+φ), sin(b·t+φ)), where t = i/N. Two sliders
// pick the integer ratio (a, b); time drifts the phase. Hue cycles around
// the curve via HSL(i/N·360).
function buildLissajous() {
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
      edge('lj_range', 'out', RID, 'i'),
      // t = i * tStep
      edge(RID, 'i', 'lj_tMul', 'a'),
      edge('lj_tStep', 'out', 'lj_tMul', 'b'),
      // angleX = t*a + time
      edge('lj_tMul', 'out', 'lj_axMul', 'a'),
      edge('lj_a', 'out', 'lj_axMul', 'b'),
      edge('lj_axMul', 'out', 'lj_axAdd', 'a'),
      edge('lj_time', 't', 'lj_axAdd', 'b'),
      // angleY = t*b + time
      edge('lj_tMul', 'out', 'lj_ayMul', 'a'),
      edge('lj_b', 'out', 'lj_ayMul', 'b'),
      edge('lj_ayMul', 'out', 'lj_ayAdd', 'a'),
      edge('lj_time', 't', 'lj_ayAdd', 'b'),
      // cos/sin then scale by R then add center
      edge('lj_axAdd', 'out', 'lj_cosX', 'x'),
      edge('lj_ayAdd', 'out', 'lj_sinY', 'x'),
      edge('lj_cosX', 'out', 'lj_rX', 'a'),
      edge('lj_radius', 'out', 'lj_rX', 'b'),
      edge('lj_sinY', 'out', 'lj_rY', 'a'),
      edge('lj_radius', 'out', 'lj_rY', 'b'),
      edge('lj_rX', 'out', 'lj_xFin', 'a'),
      edge('lj_cx', 'out', 'lj_xFin', 'b'),
      edge('lj_rY', 'out', 'lj_yFin', 'a'),
      edge('lj_cy', 'out', 'lj_yFin', 'b'),
      // Hue + fill
      edge(RID, 'i', 'lj_hueMul', 'a'),
      edge('lj_hueMul', 'out', 'lj_hsl', 'h'),
      edge('lj_hsl', 'out', 'lj_fill', 'color'),
      // Dot
      edge('lj_xFin', 'out', 'lj_dot', 'x'),
      edge('lj_yFin', 'out', 'lj_dot', 'y'),
      edge('lj_dotSize', 'out', 'lj_dot', 'w'),
      edge('lj_dotSize', 'out', 'lj_dot', 'h'),
      edge('lj_fill', 'out', 'lj_dot', 'style'),
      edge('lj_dot', 'out', RID, 'p'),
      edge(RID, 'p', 'lj_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Kaleidoscope ──────────────────────────────────────────────────
// A base triangle is offset by the mouse position from center, then a ForEach
// rotates it through 12 angles around the canvas centre. Time cycles the hue.
function buildKaleidoscope() {
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

  const angles = nodeWithPorts('ks_angles', 'spread/circular', { x: -560, y: 220 }, { count: N });
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
      edge('ks_bg', 'out', 'ks_renderer', 'background'),
      // Time → sin/cos
      edge('ks_time', 't', 'ks_tScale', 'a'),
      edge('ks_tScale', 'out', 'ks_sinT', 'x'),
      edge('ks_tScale', 'out', 'ks_cosT', 'x'),
      // Offset × 120
      edge('ks_sinT', 'out', 'ks_wOff', 'a'),
      edge('ks_cosT', 'out', 'ks_hOff', 'a'),
      // + 200 → centred-ish source coords
      edge('ks_wOff', 'out', 'ks_sxAnim', 'a'),
      edge('ks_hOff', 'out', 'ks_syAnim', 'a'),
      // Image region: wire image + animated sx/sy
      edge('ks_url', 'image', 'ks_imgRegion', 'image'),
      edge('ks_sxAnim', 'out', 'ks_imgRegion', 'sx'),
      edge('ks_syAnim', 'out', 'ks_imgRegion', 'sy'),
      // ForEach: angle + i
      edge('ks_angles', 'out', RID, 'angle'),
      edge('ks_idxs', 'out', RID, 'i'),
      // Sign = 1 - 2 * (i % 2)
      edge(RID, 'i', 'ks_iMod', 'a'),
      edge('ks_iMod', 'out', 'ks_iMul', 'a'),
      edge('ks_iMul', 'out', 'ks_sign', 'a'),
      // scale(sign, 1) the image wedge (broadcast in from outer scope)
      edge('ks_imgRegion', 'out', 'ks_scaled', 'layer'),
      edge('ks_sign', 'out', 'ks_scaled', 'sx'),
      // rotate(angle) the scaled wedge
      edge('ks_scaled', 'out', 'ks_rotated', 'layer'),
      edge(RID, 'angle', 'ks_rotated', 'angle'),
      // Out
      edge('ks_rotated', 'out', RID, 'out'),
      edge(RID, 'out', 'ks_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Ripple Field ──────────────────────────────────────────────────
// A 25×25 grid of dots. Each dot's size and hue are modulated by
// sin(dist_from_center · 0.04 − time · 3) — concentric ripples radiating
// from the canvas centre.
function buildRippleField() {
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

// ─── Sample: Starfield Pulse ───────────────────────────────────────────────
// 80 randomly-placed stars. Each star pulses size + alpha with sin(time +
// i·0.3), so they twinkle out of phase. Two seeded randomize spreads give
// x/y; a Range gives the per-star phase index.
function buildStarfield() {
  const N = 80;
  const time = node('sf_time', 'time/seconds', { x: -420, y: 20 });
  const xs = nodeWithPorts('sf_xs', 'spread/randomize', { x: -420, y: 80 }, { count: N, min: 20, max: 480, seed: 7 });
  const ys = nodeWithPorts('sf_ys', 'spread/randomize', { x: -420, y: 160 }, { count: N, min: 20, max: 480, seed: 13 });
  const idxs = nodeWithPorts('sf_idxs', 'spread/range', { x: -420, y: 240 }, { count: N });
  const phaseScale = node('sf_phaseScale', 'value/number', { x: -420, y: 300 }, { value: 0.3 });

  const RID = 'sf_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -120, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 860, height: 460, label: 'ForEach' },
      ports: [
        { id: 'x', name: 'x', type: 'number', direction: 'input' },
        { id: 'y', name: 'y', type: 'number', direction: 'input' },
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 860, height: 460 },
  };

  // Inside: phase = i * 0.3, wave = sin(time + phase) → -1..1
  const phaseMul = node('sf_phaseMul', 'math/mul', { x: 60, y: 60 }, {}, RID);
  const wavePhase = node('sf_wavePhase', 'math/add', { x: 220, y: 60 }, {}, RID);
  const wave = node('sf_wave', 'math/sin', { x: 380, y: 60 }, {}, RID);
  // size = 2.5 + wave * 2 → 0.5..4.5
  const sizeMul = node('sf_sizeMul', 'math/mul', { x: 540, y: 60 }, { b: 2 }, RID);
  const sizeBase = node('sf_sizeBase', 'value/number', { x: 540, y: 120 }, { value: 2.5 });
  const sizeAdd = node('sf_sizeAdd', 'math/add', { x: 700, y: 60 }, {}, RID);
  // alpha = (wave * 0.5 + 0.5) * 255 → 0..255
  const alpScale = node('sf_alpScale', 'math/mul', { x: 60, y: 200 }, { b: 0.5 }, RID);
  const alpShift = node('sf_alpShift', 'math/add', { x: 220, y: 200 }, { b: 0.5 }, RID);
  const alpMul = node('sf_alpMul', 'math/mul', { x: 380, y: 200 }, { b: 255 }, RID);
  // Hue varies slightly per star — mostly warm whites, a few blue-tinted.
  const hueMul = node('sf_hueMul', 'math/mul', { x: 60, y: 300 }, { b: 4 }, RID);
  const hueBase = node('sf_hueBase', 'value/number', { x: 60, y: 360 }, { value: 40 });
  const hueAdd = node('sf_hueAdd', 'math/add', { x: 220, y: 300 }, {}, RID);
  const hsl = node('sf_hsl', 'color/hsl', { x: 380, y: 300 }, { s: 35, l: 95 }, RID);
  const fill = node('sf_fill', 'style/fill', { x: 540, y: 300 }, { mode: 'color' }, RID);
  const star = node('sf_star', 'draw/ellipse', { x: 700, y: 200 }, {}, RID);

  // Night-sky black background. RGB(8, 10, 22) — very dark indigo.
  const bgColor = node('sf_bg', 'color/rgb', { x: -420, y: 380 }, { r: 8, g: 10, b: 22, a: 255 });
  const renderer = node('sf_renderer', 'render/renderer', { x: 900, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, xs, ys, idxs, phaseScale,
      region,
      phaseMul, wavePhase, wave,
      sizeMul, sizeBase, sizeAdd,
      alpScale, alpShift, alpMul,
      hueMul, hueBase, hueAdd, hsl, fill, star,
      bgColor, renderer,
    ],
    edges: [
      edge('sf_bg', 'out', 'sf_renderer', 'background'),
      edge('sf_xs', 'out', RID, 'x'),
      edge('sf_ys', 'out', RID, 'y'),
      edge('sf_idxs', 'out', RID, 'i'),
      // wave = sin(time + i*0.3)
      edge(RID, 'i', 'sf_phaseMul', 'a'),
      edge('sf_phaseScale', 'out', 'sf_phaseMul', 'b'),
      edge('sf_time', 't', 'sf_wavePhase', 'a'),
      edge('sf_phaseMul', 'out', 'sf_wavePhase', 'b'),
      edge('sf_wavePhase', 'out', 'sf_wave', 'x'),
      // size
      edge('sf_wave', 'out', 'sf_sizeMul', 'a'),
      edge('sf_sizeBase', 'out', 'sf_sizeAdd', 'a'),
      edge('sf_sizeMul', 'out', 'sf_sizeAdd', 'b'),
      // alpha
      edge('sf_wave', 'out', 'sf_alpScale', 'a'),
      edge('sf_alpScale', 'out', 'sf_alpShift', 'a'),
      edge('sf_alpShift', 'out', 'sf_alpMul', 'a'),
      // hue variation per star
      edge(RID, 'i', 'sf_hueMul', 'a'),
      edge('sf_hueBase', 'out', 'sf_hueAdd', 'a'),
      edge('sf_hueMul', 'out', 'sf_hueAdd', 'b'),
      edge('sf_hueAdd', 'out', 'sf_hsl', 'h'),
      edge('sf_alpMul', 'out', 'sf_hsl', 'a'),
      edge('sf_hsl', 'out', 'sf_fill', 'color'),
      // Star ellipse
      edge(RID, 'x', 'sf_star', 'x'),
      edge(RID, 'y', 'sf_star', 'y'),
      edge('sf_sizeAdd', 'out', 'sf_star', 'w'),
      edge('sf_sizeAdd', 'out', 'sf_star', 'h'),
      edge('sf_fill', 'out', 'sf_star', 'style'),
      edge('sf_star', 'out', RID, 'layer'),
      edge(RID, 'layer', 'sf_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Tone Surface ──────────────────────────────────────────────────
// Mouse X picks pitch (110..880 Hz), mouse Y picks amp (0..0.4). A 20×20
// background grid colours each dot by distance to the mouse, so the field
// "lights up" around the cursor. Click → audio resumes (browser autoplay).
function buildToneSurface() {
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
      edge('ts_freqMap', 'out', 'ts_osc', 'freq'),
      edge('ts_ampMap', 'out', 'ts_osc', 'amp'),
      edge('ts_osc', 'audio', 'ts_out', 'in'),
      // Grid → ForEach; mouse broadcasts directly into the inner dist node.
      edge('ts_grid', 'out', RID, 'p'),
      edge(RID, 'p', 'ts_split', 'v'),
      edge('ts_split', 'x', 'ts_xOff', 'a'),
      edge('ts_split', 'y', 'ts_yOff', 'a'),
      edge('ts_xOff', 'out', 'ts_dist', 'x1'),
      edge('ts_yOff', 'out', 'ts_dist', 'y1'),
      edge('ts_mouse', 'x', 'ts_dist', 'x2'),
      edge('ts_mouse', 'y', 'ts_dist', 'y2'),
      // hue
      edge('ts_dist', 'out', 'ts_distScaled', 'a'),
      edge('ts_hueBase', 'out', 'ts_hue', 'a'),
      edge('ts_distScaled', 'out', 'ts_hue', 'b'),
      edge('ts_hue', 'out', 'ts_hsl', 'h'),
      // lightness
      edge('ts_dist', 'out', 'ts_lScaled', 'a'),
      edge('ts_lBase', 'out', 'ts_lit', 'a'),
      edge('ts_lScaled', 'out', 'ts_lit', 'b'),
      edge('ts_lit', 'out', 'ts_lClamp', 'x'),
      edge('ts_lClamp', 'out', 'ts_hsl', 'l'),
      edge('ts_hsl', 'out', 'ts_fill', 'color'),
      // Dot
      edge('ts_xOff', 'out', 'ts_dot', 'x'),
      edge('ts_yOff', 'out', 'ts_dot', 'y'),
      edge('ts_dotSize', 'out', 'ts_dot', 'w'),
      edge('ts_dotSize', 'out', 'ts_dot', 'h'),
      edge('ts_fill', 'out', 'ts_dot', 'style'),
      edge('ts_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'ts_renderer', 'layers'),
    ],
  };
}

// ─── Sample: FFT Bloom ─────────────────────────────────────────────────────
// An oscillator (whose freq is driven by mouse Y) feeds an FFT. The 64-bin
// spectrum is plotted polar-style: each bin becomes a dot on a ring whose
// radius is 80 + magnitude · 0.7. Hue cycles around the ring.
function buildFFTBloom() {
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
      edge('fb_bg', 'out', 'fb_renderer', 'background'),
      // Mouse Y → freq → osc → output + FFT
      edge('fb_mouse', 'y', 'fb_freqMap', 'x'),
      edge('fb_freqMap', 'out', 'fb_osc', 'freq'),
      edge('fb_amp', 'out', 'fb_osc', 'amp'),
      edge('fb_osc', 'audio', 'fb_out', 'in'),
      edge('fb_osc', 'audio', 'fb_fft', 'in'),
      // ForEach over (i, spectrum)
      edge('fb_idxs', 'out', RID, 'i'),
      edge('fb_fft', 'spectrum', RID, 'v'),
      // angle = i * step
      edge(RID, 'i', 'fb_angleMul', 'a'),
      // radius = 80 + v * 0.7
      edge(RID, 'v', 'fb_vScaled', 'a'),
      edge('fb_vScaled', 'out', 'fb_radius', 'a'),
      edge('fb_angleMul', 'out', 'fb_fromAng', 'angle'),
      edge('fb_radius', 'out', 'fb_fromAng', 'mag'),
      edge('fb_fromAng', 'out', 'fb_splitPos', 'v'),
      edge('fb_splitPos', 'x', 'fb_xCenter', 'a'),
      edge('fb_splitPos', 'y', 'fb_yCenter', 'a'),
      // hue + size
      edge(RID, 'i', 'fb_hueMul', 'a'),
      edge('fb_hueMul', 'out', 'fb_hsl', 'h'),
      edge('fb_hsl', 'out', 'fb_fill', 'color'),
      edge(RID, 'v', 'fb_sScaled', 'a'),
      edge('fb_sScaled', 'out', 'fb_sAdd', 'a'),
      // Dot
      edge('fb_xCenter', 'out', 'fb_dot', 'x'),
      edge('fb_yCenter', 'out', 'fb_dot', 'y'),
      edge('fb_sAdd', 'out', 'fb_dot', 'w'),
      edge('fb_sAdd', 'out', 'fb_dot', 'h'),
      edge('fb_fill', 'out', 'fb_dot', 'style'),
      edge('fb_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'fb_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Random Walk Trail ─────────────────────────────────────────────
// A walker drifts via hashed (time, seed) angles. A length-N vec2 trail
// is stored in a State Read/Write slot pair (slot 1) — each tick the
// walker reads the current trail, drops the oldest point, appends a new
// one, and writes it back. Read on the left, Write on the right —
// strictly left-to-right with no back-edge in the graph.
function buildRandomWalkTrail() {
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
      edge('rw_bg', 'out', 'rw_renderer', 'background'),
      // Seed: a length-N spread of (centre, centre) feeds Read*.init for frame 0.
      edge('rw_center', 'out', 'rw_init', 'value'),
      edge('rw_init', 'out', 'rw_read', 'init'),
      edge('rw_read', 'current', 'rw_walk', 'trail'),
      edge('rw_time', 't', 'rw_walk', 'time'),
      edge('rw_step', 'out', 'rw_walk', 'step'),
      edge('rw_seed', 'out', 'rw_walk', 'seed'),
      edge('rw_walk', 'trail', 'rw_write', 'next'),
      // Render trail — the freshly written value also drives this frame's render.
      edge('rw_walk', 'trail', RID, 'p'),
      edge('rw_idx', 'out', RID, 'i'),
      edge(RID, 'p', 'rw_split', 'v'),
      edge(RID, 'i', 'rw_hueMul', 'a'),
      edge(RID, 'i', 'rw_alpMul', 'a'),
      edge(RID, 'i', 'rw_sizeMul', 'a'),
      edge('rw_sizeMul', 'out', 'rw_sizeAdd', 'a'),
      edge('rw_hueMul', 'out', 'rw_hsl', 'h'),
      edge('rw_alpMul', 'out', 'rw_hsl', 'a'),
      edge('rw_hsl', 'out', 'rw_fill', 'color'),
      edge('rw_split', 'x', 'rw_dot', 'x'),
      edge('rw_split', 'y', 'rw_dot', 'y'),
      edge('rw_sizeAdd', 'out', 'rw_dot', 'w'),
      edge('rw_sizeAdd', 'out', 'rw_dot', 'h'),
      edge('rw_fill', 'out', 'rw_dot', 'style'),
      edge('rw_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'rw_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Game of Life ──────────────────────────────────────────────────
// 32×32 toroidal Game of Life. state<boolean*> holds the grid; each
// tick, gol-step computes the next generation. ForEach over a Range
// renders one rectangle per cell — alive cells fill, dead cells
// collapse to size 0.
//
// Grid sizing: 32×32 = 1024 cells gives stable patterns (blinkers,
// blocks, occasional gliders) room to form and persist for hundreds
// of generations — small enough to read, large enough to last. The
// default density of 0.32 sits in the GoL sweet spot where simulations
// neither crash from undercrowding nor smother from overcrowding.
function buildGameOfLife() {
  const COLS = 32;
  const ROWS = 32;
  const N = COLS * ROWS;
  const cols = node('gol_cols', 'value/number', { x: -480, y: 20 }, { value: COLS });
  const rows = node('gol_rows', 'value/number', { x: -480, y: 80 }, { value: ROWS });
  const density = node('gol_density', 'io/slider', { x: -480, y: 140 }, { value: 0.32, min: 0.15, max: 0.5 });
  const seed = node('gol_seed', 'io/slider', { x: -480, y: 220 }, { value: 7, min: 1, max: 99 });
  const init = node('gol_init', 'spread/gol-seed', { x: -260, y: 100 });
  // Read* on the left (slot 1), Write* on the right — strict L→R, no back-edge.
  const read = node('gol_read', 'state/read-spread', { x: 120, y: 100 }, { slot: 1 });
  const stepNode = node('gol_step', 'spread/gol-step', { x: 320, y: 100 });
  const write = node('gol_write', 'state/write-spread', { x: 540, y: 100 }, { slot: 1 });
  const idx = node('gol_idx', 'spread/range', { x: -480, y: 300 }, { count: N });

  const RID = 'gol_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 200, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 820, height: 520, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 820, height: 520 },
  };

  // Inside: look up grid[i], compute (x, y) = (i%cols, i/cols) * cellSize.
  // Grid spans cols * cellSize = 32 * 14 = 448 px. Centre on the
  // 500-wide centred-origin canvas by offsetting the corner of cell 0
  // to -(cols-1)*cellSize/2.
  const cellSize = node('gol_cellSize', 'value/number', { x: 60, y: 320 }, { value: 14 });
  const colsInner = node('gol_colsInner', 'value/number', { x: 60, y: 380 }, { value: COLS });
  const offset = node('gol_offset', 'value/number', { x: 60, y: 440 }, { value: -217 });
  // Lookup
  const cell = node('gol_cell', 'spread/at', { x: 60, y: 60 }, {}, RID);
  // x = (i % cols) * cellSize + offset
  const xMod = node('gol_xMod', 'math/mod', { x: 60, y: 140 }, {}, RID);
  const xPos = node('gol_xPos', 'math/mul', { x: 240, y: 140 }, {}, RID);
  const xFin = node('gol_xFin', 'math/add', { x: 420, y: 140 }, {}, RID);
  // y = floor(i / cols) * cellSize + offset
  const yDiv = node('gol_yDiv', 'math/div', { x: 60, y: 220 }, {}, RID);
  const yFloor = node('gol_yFloor', 'math/floor', { x: 240, y: 220 }, {}, RID);
  const yPos = node('gol_yPos', 'math/mul', { x: 420, y: 220 }, {}, RID);
  const yFin = node('gol_yFin', 'math/add', { x: 600, y: 220 }, {}, RID);
  // size = cellSize - 2 if alive, else 0. Multiply by cell value (true=1).
  const sizeBase = node('gol_sizeBase', 'value/number', { x: 60, y: 460 }, { value: 12 });
  const sizeMul = node('gol_sizeMul', 'math/mul', { x: 240, y: 280 }, {}, RID);
  // Color: alive = HSL(160, 75, 60); dead = invisible via size=0
  const hsl = node('gol_hsl', 'color/hsl', { x: 60, y: 540 }, { h: 160, s: 75, l: 60, a: 255 });
  const fill = node('gol_fill', 'style/fill', { x: 240, y: 540 }, { mode: 'color' });
  const rect = node('gol_rect', 'draw/rect', { x: 760, y: 140 }, {}, RID);

  const bg = node('gol_bg', 'color/rgb', { x: -480, y: 380 }, { r: 18, g: 22, b: 28, a: 255 });
  const renderer = node('gol_renderer', 'render/renderer', { x: 1080, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      cols, rows, density, seed, init, read, stepNode, write, idx,
      region,
      cellSize, colsInner, offset,
      cell, xMod, xPos, xFin, yDiv, yFloor, yPos, yFin, sizeBase, sizeMul,
      hsl, fill, rect,
      bg, renderer,
    ],
    edges: [
      edge('gol_bg', 'out', 'gol_renderer', 'background'),
      // Seed → init → Read*.init (frame 0 only)
      edge('gol_cols', 'out', 'gol_init', 'cols'),
      edge('gol_rows', 'out', 'gol_init', 'rows'),
      edge('gol_density', 'out', 'gol_init', 'density'),
      edge('gol_seed', 'out', 'gol_init', 'seed'),
      edge('gol_init', 'grid', 'gol_read', 'init'),
      edge('gol_read', 'current', 'gol_step', 'grid'),
      edge('gol_cols', 'out', 'gol_step', 'cols'),
      edge('gol_rows', 'out', 'gol_step', 'rows'),
      edge('gol_step', 'next', 'gol_write', 'next'),
      // ForEach over cell indices
      edge('gol_idx', 'out', RID, 'i'),
      // Render this frame's grid (the freshly stepped one).
      edge('gol_step', 'next', 'gol_cell', 'spread'),
      edge(RID, 'i', 'gol_cell', 'index'),
      // x = (i % cols) * cellSize + offset
      edge(RID, 'i', 'gol_xMod', 'a'),
      edge('gol_colsInner', 'out', 'gol_xMod', 'b'),
      edge('gol_xMod', 'out', 'gol_xPos', 'a'),
      edge('gol_cellSize', 'out', 'gol_xPos', 'b'),
      edge('gol_xPos', 'out', 'gol_xFin', 'a'),
      edge('gol_offset', 'out', 'gol_xFin', 'b'),
      // y = floor(i / cols) * cellSize + offset
      edge(RID, 'i', 'gol_yDiv', 'a'),
      edge('gol_colsInner', 'out', 'gol_yDiv', 'b'),
      edge('gol_yDiv', 'out', 'gol_yFloor', 'x'),
      edge('gol_yFloor', 'out', 'gol_yPos', 'a'),
      edge('gol_cellSize', 'out', 'gol_yPos', 'b'),
      edge('gol_yPos', 'out', 'gol_yFin', 'a'),
      edge('gol_offset', 'out', 'gol_yFin', 'b'),
      // size = cellAlive * sizeBase  (true*24=24, false*24=0 — invisible)
      edge('gol_cell', 'out', 'gol_sizeMul', 'a'),
      edge('gol_sizeBase', 'out', 'gol_sizeMul', 'b'),
      // Rect (fill broadcast from outside)
      edge('gol_hsl', 'out', 'gol_fill', 'color'),
      edge('gol_xFin', 'out', 'gol_rect', 'x'),
      edge('gol_yFin', 'out', 'gol_rect', 'y'),
      edge('gol_sizeMul', 'out', 'gol_rect', 'w'),
      edge('gol_sizeMul', 'out', 'gol_rect', 'h'),
      edge('gol_fill', 'out', 'gol_rect', 'style'),
      edge('gol_rect', 'out', RID, 'layer'),
      edge(RID, 'layer', 'gol_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Noise Wave ────────────────────────────────────────────────────
// A grid of colored squares whose positions and hues are bent by aggressive
// Perlin noise at multiple octaves. Cells slide off their natural lattice
// positions; bright RGB-glitch colours emerge where the noise spikes.
// Pure function of time, no state.
function buildNoiseWave() {
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
function buildDatamosh() {
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
      edge('imd_bg', 'out', 'imd_renderer', 'background'),
      // Time → offset
      edge('imd_time', 't', 'imd_tSpeed', 'a'),
      edge('imd_tSpeed', 'out', 'imd_sinT', 'x'),
      edge('imd_sinT', 'out', 'imd_offsetMul', 'a'),
      edge('imd_offsetMul', 'out', 'imd_negOffset', 'a'),
      // xShifted = baseX + offset, xShiftedNeg = baseX + (-offset)
      edge('imd_baseX', 'out', 'imd_xShifted', 'a'),
      edge('imd_offsetMul', 'out', 'imd_xShifted', 'b'),
      edge('imd_baseX', 'out', 'imd_xShiftedNeg', 'a'),
      edge('imd_negOffset', 'out', 'imd_xShiftedNeg', 'b'),
      // Image feeds all four draws (broadcast)
      edge('imd_img', 'image', 'imd_drawBase', 'image'),
      edge('imd_img', 'image', 'imd_drawR', 'image'),
      edge('imd_img', 'image', 'imd_drawG', 'image'),
      edge('imd_img', 'image', 'imd_drawB', 'image'),
      // Base draw — no tint, no offset
      edge('imd_baseX', 'out', 'imd_drawBase', 'x'),
      edge('imd_baseY', 'out', 'imd_drawBase', 'y'),
      edge('imd_drawW', 'out', 'imd_drawBase', 'w'),
      edge('imd_drawH', 'out', 'imd_drawBase', 'h'),
      // R-tint, +offset
      edge('imd_xShifted', 'out', 'imd_drawR', 'x'),
      edge('imd_baseY', 'out', 'imd_drawR', 'y'),
      edge('imd_drawW', 'out', 'imd_drawR', 'w'),
      edge('imd_drawH', 'out', 'imd_drawR', 'h'),
      edge('imd_redTint', 'out', 'imd_drawR', 'tint'),
      // G-tint, no shift
      edge('imd_baseX', 'out', 'imd_drawG', 'x'),
      edge('imd_baseY', 'out', 'imd_drawG', 'y'),
      edge('imd_drawW', 'out', 'imd_drawG', 'w'),
      edge('imd_drawH', 'out', 'imd_drawG', 'h'),
      edge('imd_grnTint', 'out', 'imd_drawG', 'tint'),
      // B-tint, -offset
      edge('imd_xShiftedNeg', 'out', 'imd_drawB', 'x'),
      edge('imd_baseY', 'out', 'imd_drawB', 'y'),
      edge('imd_drawW', 'out', 'imd_drawB', 'w'),
      edge('imd_drawH', 'out', 'imd_drawB', 'h'),
      edge('imd_bluTint', 'out', 'imd_drawB', 'tint'),
      // Wrap each in a blend
      edge('imd_drawBase', 'out', 'imd_blendBase', 'layer'),
      edge('imd_drawR', 'out', 'imd_blendR', 'layer'),
      edge('imd_drawG', 'out', 'imd_blendG', 'layer'),
      edge('imd_drawB', 'out', 'imd_blendB', 'layer'),
      // Stack 4 blended layers → group → renderer
      edge('imd_blendBase', 'out', 'imd_s1', 'a'),
      edge('imd_blendR', 'out', 'imd_s1', 'b'),
      edge('imd_s1', 'out', 'imd_s2', 'a'),
      edge('imd_blendG', 'out', 'imd_s2', 'b'),
      edge('imd_s2', 'out', 'imd_s3', 'a'),
      edge('imd_blendB', 'out', 'imd_s3', 'b'),
      edge('imd_s3', 'out', 'imd_finalGroup', 's1'),
      edge('imd_finalGroup', 'out', 'imd_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Force Field Arrows ────────────────────────────────────────────
// A 14×14 grid of arrows pointing along a Perlin-noise angle field that
// rotates slowly with time. Each arrow is drawn as a line — its end
// point is computed by `vec2/from-angle`.
function buildForceField() {
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

// ─── Sample: CSV Bar Chart ─────────────────────────────────────────────────
// Inline CSV parsed into a number spread; bars rendered per value
// with HSL hue along the index axis. Edit the `data` param on the
// CSVInline node to load a different series.
function buildCSVBarChart() {
  const csv = node('csv_csv', 'data/csv-inline', { x: -480, y: 60 }, {
    data: '12, 45, 22, 67, 38, 91, 54, 73, 29, 60, 84, 19, 41, 77, 33, 55',
  });
  const count = node('csv_count', 'spread/count', { x: -260, y: 60 });
  const idx = node('csv_idx', 'spread/range', { x: -480, y: 160 }, { count: 16 });
  // Bar width = canvasW / count; we hardcode 16 bars → 28px wide with 4px gap
  const barW = node('csv_barW', 'value/number', { x: -480, y: 240 }, { value: 26 });
  const gap = node('csv_gap', 'value/number', { x: -480, y: 300 }, { value: 6 });
  // baseY = 220 puts the bars sitting near the bottom edge (canvas spans -250..250).
  const baseY = node('csv_baseY', 'value/number', { x: -480, y: 360 }, { value: 220 });
  const heightScale = node('csv_heightScale', 'value/number', { x: -480, y: 420 }, { value: 4 });

  const RID = 'csv_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: 200, y: 60 },
    data: {
      defType: 'region/foreach',
      params: { width: 880, height: 480, label: 'ForEach' },
      ports: [
        { id: 'v', name: 'v', type: 'number', direction: 'input' },
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 880, height: 480 },
  };

  // Inside: x = i * (barW + gap) + 20; height = v * heightScale; y = baseY - height
  const stride = node('csv_stride', 'math/add', { x: 60, y: 140 }, {}, RID);
  const xMul = node('csv_xMul', 'math/mul', { x: 240, y: 60 }, {}, RID);
  // Start the first bar near the left edge: -240 (canvas spans -250..250).
  const xFin = node('csv_xFin', 'math/add', { x: 420, y: 60 }, { b: -240 }, RID);
  const hMul = node('csv_hMul', 'math/mul', { x: 240, y: 140 }, {}, RID);
  const yFin = node('csv_yFin', 'math/sub', { x: 420, y: 140 }, {}, RID);
  // Hue across the spread
  const hueMul = node('csv_hueMul', 'math/mul', { x: 60, y: 260 }, { b: 22 }, RID);
  const hsl = node('csv_hsl', 'color/hsl', { x: 240, y: 260 }, { s: 80, l: 60, a: 255 }, RID);
  const fill = node('csv_fill', 'style/fill', { x: 420, y: 260 }, { mode: 'color' }, RID);
  const rect = node('csv_rect', 'draw/rect', { x: 600, y: 60 }, {}, RID);

  const bg = node('csv_bg', 'color/rgb', { x: -480, y: 500 }, { r: 24, g: 24, b: 30, a: 255 });
  const renderer = node('csv_renderer', 'render/renderer', { x: 1140, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [
      csv, count, idx, barW, gap, baseY, heightScale,
      region,
      stride, xMul, xFin, hMul, yFin,
      hueMul, hsl, fill, rect,
      bg, renderer,
    ],
    edges: [
      edge('csv_bg', 'out', 'csv_renderer', 'background'),
      edge('csv_csv', 'out', 'csv_count', 'spread'),
      edge('csv_csv', 'out', RID, 'v'),
      edge('csv_idx', 'out', RID, 'i'),
      // stride = barW + gap
      edge('csv_barW', 'out', 'csv_stride', 'a'),
      edge('csv_gap', 'out', 'csv_stride', 'b'),
      // x = i * stride + 20
      edge(RID, 'i', 'csv_xMul', 'a'),
      edge('csv_stride', 'out', 'csv_xMul', 'b'),
      edge('csv_xMul', 'out', 'csv_xFin', 'a'),
      // height = v * heightScale
      edge(RID, 'v', 'csv_hMul', 'a'),
      edge('csv_heightScale', 'out', 'csv_hMul', 'b'),
      // y = baseY - height (bar sits on baseY line)
      edge('csv_baseY', 'out', 'csv_yFin', 'a'),
      edge('csv_hMul', 'out', 'csv_yFin', 'b'),
      // Hue
      edge(RID, 'i', 'csv_hueMul', 'a'),
      edge('csv_hueMul', 'out', 'csv_hsl', 'h'),
      edge('csv_hsl', 'out', 'csv_fill', 'color'),
      // Rect
      edge('csv_xFin', 'out', 'csv_rect', 'x'),
      edge('csv_yFin', 'out', 'csv_rect', 'y'),
      edge('csv_barW', 'out', 'csv_rect', 'w'),
      edge('csv_hMul', 'out', 'csv_rect', 'h'),
      edge('csv_fill', 'out', 'csv_rect', 'style'),
      edge('csv_rect', 'out', RID, 'layer'),
      edge(RID, 'layer', 'csv_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Islamic Pattern ───────────────────────────────────────────────
// 8-fold rotational symmetry. A "petal" — three concentric ellipses at
// an offset from the centre — is rotated 8 times around (250, 250) by
// the standard kaleidoscope ForEach pattern. Time slowly oscillates
// the petal's distance, breathing the pattern in and out.
function buildIslamicPattern() {
  // Concentric symmetric geometric pattern, in the spirit of classic
  // Islamic tile work: an outer ring of six overlapping black-outlined
  // circles forming a flower-of-life ring, six teal "petal" ellipses
  // riding on the inner ring, an 8-pointed red star at the centre
  // (built as a single polygon with 16 alternating-radius vertices),
  // and six small red dots at the innermost ring.
  //
  // Four ForEach regions, one per layer; outputs concatenated via
  // stacked Group nodes into a single Layer spread the Renderer reads.

  // ── Shared styling ─────────────────────────────────────────────────────
  const outlineColor = node('ip_outlineColor', 'color/rgb', { x: -540, y: 20 }, { r: 26, g: 30, b: 40, a: 255 });
  const outlineNoFill = node('ip_outlineNoFill', 'style/no-fill', { x: -360, y: 20 });
  const outlineStroke = node('ip_outlineStroke', 'style/stroke', { x: -180, y: 20 });
  const outlineWeight = node('ip_outlineWeight', 'style/stroke-weight', { x: 0, y: 20 }, { w: 2 });

  const tealColor = node('ip_tealColor', 'color/rgb', { x: -540, y: 100 }, { r: 90, g: 150, b: 165, a: 255 });
  const tealFill = node('ip_tealFill', 'style/fill', { x: -360, y: 100 }, { mode: 'color' });

  const redColor = node('ip_redColor', 'color/rgb', { x: -540, y: 160 }, { r: 175, g: 55, b: 80, a: 255 });
  const redFill = node('ip_redFill', 'style/fill', { x: -360, y: 160 }, { mode: 'color' });

  // ── Layer 1: 6 outer outlined circles (flower-of-life ring) ────────────
  const outerAngles = nodeWithPorts('ip_outerAngles', 'spread/circular', { x: -540, y: 240 }, { count: 6 });
  const OUTER_RID = 'ip_outerRegion';
  const outerRegion: FloretNode = {
    id: OUTER_RID, type: 'floret', position: { x: -260, y: 240 },
    data: { defType: 'region/foreach',
      params: { width: 700, height: 200, label: 'Outer Circles' },
      ports: [
        { id: 'a', name: 'angle', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 700, height: 200 },
  };
  const outerFa = node('ip_outerFa', 'vec2/from-angle', { x: 40, y: 40 }, { mag: 110 }, OUTER_RID);
  const outerSplit = node('ip_outerSplit', 'vec2/split', { x: 200, y: 40 }, {}, OUTER_RID);
  const outerCircle = node('ip_outerCircle', 'draw/ellipse', { x: 380, y: 40 }, { w: 220, h: 220 }, OUTER_RID);

  // ── Layer 2: 6 teal filled inner-ring petals ───────────────────────────
  const midAngles = nodeWithPorts('ip_midAngles', 'spread/circular', { x: -540, y: 480 }, { count: 6 });
  const MID_RID = 'ip_midRegion';
  const midRegion: FloretNode = {
    id: MID_RID, type: 'floret', position: { x: -260, y: 480 },
    data: { defType: 'region/foreach',
      params: { width: 700, height: 180, label: 'Mid Petals' },
      ports: [
        { id: 'a', name: 'angle', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 700, height: 180 },
  };
  const midFa = node('ip_midFa', 'vec2/from-angle', { x: 40, y: 40 }, { mag: 110 }, MID_RID);
  const midSplit = node('ip_midSplit', 'vec2/split', { x: 200, y: 40 }, {}, MID_RID);
  const midRot = node('ip_midRot', 'math/add', { x: 40, y: 100 }, {}, MID_RID); // rotation angle per petal = angle
  const midEll = node('ip_midEll', 'draw/ellipse', { x: 380, y: 40 }, { w: 90, h: 56 }, MID_RID);

  // ── Layer 3: 16-vertex 8-pointed star (single polygon, filled red) ─────
  // Build vertices via a ForEach over range(16) → outputs a vec2 spread.
  // Vertex i: angle = i * (TAU/16); radius = outer if i even, inner if odd.
  const starIdxs = nodeWithPorts('ip_starIdxs', 'spread/range', { x: -540, y: 700 }, { count: 16 });
  const starStep = node('ip_starStep', 'value/number', { x: -540, y: 760 }, { value: (Math.PI * 2) / 16 });
  const STAR_RID = 'ip_starRegion';
  const starRegion: FloretNode = {
    id: STAR_RID, type: 'floret', position: { x: -260, y: 700 },
    data: { defType: 'region/foreach',
      params: { width: 720, height: 220, label: 'Star Vertices' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'v', name: 'v', type: 'vec2', direction: 'output' },
      ],
    },
    style: { width: 720, height: 220 },
  };
  const starAng = node('ip_starAng', 'math/mul', { x: 40, y: 40 }, {}, STAR_RID);
  const starMod = node('ip_starMod', 'math/mod', { x: 40, y: 100 }, { b: 2 }, STAR_RID);
  // r = 80 - (i%2) * 48 → 80 when even, 32 when odd
  const starRMul = node('ip_starRMul', 'math/mul', { x: 200, y: 100 }, { b: -48 }, STAR_RID);
  const starR = node('ip_starR', 'math/add', { x: 360, y: 100 }, { b: 80 }, STAR_RID);
  const starFa = node('ip_starFa', 'vec2/from-angle', { x: 520, y: 40 }, {}, STAR_RID);

  // Filled polygon star (closed=true → applies fillStyle from input).
  const starPoly = node('ip_starPoly', 'draw/polygon', { x: 520, y: 700 }, { closed: true });

  // ── Layer 4: 6 small red dots at innermost ring ────────────────────────
  const innerAngles = nodeWithPorts('ip_innerAngles', 'spread/circular', { x: -540, y: 940 }, { count: 6 });
  const INNER_RID = 'ip_innerRegion';
  const innerRegion: FloretNode = {
    id: INNER_RID, type: 'floret', position: { x: -260, y: 940 },
    data: { defType: 'region/foreach',
      params: { width: 700, height: 160, label: 'Inner Dots' },
      ports: [
        { id: 'a', name: 'angle', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 700, height: 160 },
  };
  const innerFa = node('ip_innerFa', 'vec2/from-angle', { x: 40, y: 40 }, { mag: 105 }, INNER_RID);
  const innerSplit = node('ip_innerSplit', 'vec2/split', { x: 200, y: 40 }, {}, INNER_RID);
  const innerDot = node('ip_innerDot', 'draw/ellipse', { x: 380, y: 40 }, { w: 16, h: 16 }, INNER_RID);

  // ── Combine into one Group ─────────────────────────────────────────────
  const stack1 = node('ip_stack1', 'spread/stack', { x: 480, y: 280 });
  const stack2 = node('ip_stack2', 'spread/stack', { x: 480, y: 380 });
  const stack3 = node('ip_stack3', 'spread/stack', { x: 480, y: 480 });
  // slotCount: 1 — the single slot is kind:'spread' so it accepts the
  // whole stack3 spread (concatenated outer + mid + star + inner layers).
  const grp = node('ip_grp', 'transform/group', { x: 700, y: 380 }, { slotCount: 1 });

  const bg = node('ip_bg', 'color/rgb', { x: -540, y: 1100 }, { r: 245, g: 232, b: 215, a: 255 });
  const renderer = node('ip_renderer', 'render/renderer', { x: 920, y: 280 }, { width: 500, height: 500 });

  return {
    nodes: [
      outlineColor, outlineNoFill, outlineStroke, outlineWeight,
      tealColor, tealFill, redColor, redFill,
      outerAngles, outerRegion, outerFa, outerSplit, outerCircle,
      midAngles, midRegion, midFa, midSplit, midRot, midEll,
      starIdxs, starStep, starRegion, starAng, starMod, starRMul, starR, starFa,
      starPoly,
      innerAngles, innerRegion, innerFa, innerSplit, innerDot,
      stack1, stack2, stack3, grp,
      bg, renderer,
    ],
    edges: [
      edge('ip_bg', 'out', 'ip_renderer', 'background'),
      // Outline style chain: no-fill → stroke(color) → stroke-weight
      edge('ip_outlineColor', 'out', 'ip_outlineStroke', 'color'),
      edge('ip_outlineNoFill', 'out', 'ip_outlineStroke', 'base'),
      edge('ip_outlineStroke', 'out', 'ip_outlineWeight', 'base'),
      // Fill styles
      edge('ip_tealColor', 'out', 'ip_tealFill', 'color'),
      edge('ip_redColor', 'out', 'ip_redFill', 'color'),

      // ── Outer circles ────────────────────────────────────────
      edge('ip_outerAngles', 'out', OUTER_RID, 'a'),
      edge(OUTER_RID, 'a', 'ip_outerFa', 'angle'),
      edge('ip_outerFa', 'out', 'ip_outerSplit', 'v'),
      edge('ip_outerSplit', 'x', 'ip_outerCircle', 'x'),
      edge('ip_outerSplit', 'y', 'ip_outerCircle', 'y'),
      // Outline style: no-fill → stroke(dark) → stroke-weight(2).
      edge('ip_outlineWeight', 'out', 'ip_outerCircle', 'style'),
      edge('ip_outerCircle', 'out', OUTER_RID, 'layer'),

      // ── Mid teal petals ──────────────────────────────────────
      edge('ip_midAngles', 'out', MID_RID, 'a'),
      edge(MID_RID, 'a', 'ip_midFa', 'angle'),
      edge('ip_midFa', 'out', 'ip_midSplit', 'v'),
      edge('ip_midSplit', 'x', 'ip_midEll', 'x'),
      edge('ip_midSplit', 'y', 'ip_midEll', 'y'),
      // (petals are wide ellipses; aligning them tangentially would
      //  need a rotation transform around each petal's centre; for now
      //  they sit horizontal which still reads as a six-petal flower.)
      edge('ip_tealFill', 'out', 'ip_midEll', 'style'),
      edge('ip_midEll', 'out', MID_RID, 'layer'),

      // ── 8-point star polygon vertices ────────────────────────
      edge('ip_starIdxs', 'out', STAR_RID, 'i'),
      // angle = i * (TAU/16)
      edge(STAR_RID, 'i', 'ip_starAng', 'a'),
      edge('ip_starStep', 'out', 'ip_starAng', 'b'),
      // r = 80 + (i mod 2)*-48 → 80 even, 32 odd
      edge(STAR_RID, 'i', 'ip_starMod', 'a'),
      edge('ip_starMod', 'out', 'ip_starRMul', 'a'),
      edge('ip_starRMul', 'out', 'ip_starR', 'a'),
      // vec = from-angle(angle, r)
      edge('ip_starAng', 'out', 'ip_starFa', 'angle'),
      edge('ip_starR', 'out', 'ip_starFa', 'mag'),
      edge('ip_starFa', 'out', STAR_RID, 'v'),
      // Build polygon from the vec2 spread, filled red.
      edge(STAR_RID, 'v', 'ip_starPoly', 'points'),
      edge('ip_redFill', 'out', 'ip_starPoly', 'style'),

      // ── Inner dots ───────────────────────────────────────────
      edge('ip_innerAngles', 'out', INNER_RID, 'a'),
      edge(INNER_RID, 'a', 'ip_innerFa', 'angle'),
      edge('ip_innerFa', 'out', 'ip_innerSplit', 'v'),
      edge('ip_innerSplit', 'x', 'ip_innerDot', 'x'),
      edge('ip_innerSplit', 'y', 'ip_innerDot', 'y'),
      edge('ip_redFill', 'out', 'ip_innerDot', 'style'),
      edge('ip_innerDot', 'out', INNER_RID, 'layer'),

      // ── Combine layers via three Stacks ──────────────────────
      // stack1 = outer ++ mid
      edge(OUTER_RID, 'layer', 'ip_stack1', 'a'),
      edge(MID_RID, 'layer', 'ip_stack1', 'b'),
      // stack2 = stack1 ++ [starPoly]
      edge('ip_stack1', 'out', 'ip_stack2', 'a'),
      edge('ip_starPoly', 'out', 'ip_stack2', 'b'),
      // stack3 = stack2 ++ inner-dots
      edge('ip_stack2', 'out', 'ip_stack3', 'a'),
      edge(INNER_RID, 'layer', 'ip_stack3', 'b'),
      // Group all into a single Layer for the renderer.
      edge('ip_stack3', 'out', 'ip_grp', 's1'),
      edge('ip_grp', 'out', 'ip_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Photo Mosaic ──────────────────────────────────────────────────
// Load a photo, sample every pixel in a 32×32 grid with image/sample, draw
// each point as a circle whose size is proportional to the pixel's luminance.
// The result is a pointillist / Warhol dot-matrix of the source image that
// breathes gently with sin(time).
function buildPhotoMosaic() {
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
      edge('pm_bg',   'out',    'pm_renderer', 'background'),
      edge('pm_grid', 'out', RID,           'p'),
      // Inner: split point → sample → lum → size
      edge(RID,          'p',     'pm_split',   'v'),
      edge('pm_split',   'x',     'pm_sample',  'x'),
      edge('pm_split',   'y',     'pm_sample',  'y'),
      edge('pm_img',     'image', 'pm_sample',  'image'),   // broadcast from outer
      edge('pm_sample',  'color', 'pm_srgb',    'color'),
      edge('pm_srgb',    'r',     'pm_rg',      'a'),
      edge('pm_srgb',    'g',     'pm_rg',      'b'),
      edge('pm_rg',      'out',   'pm_rgb',     'a'),
      edge('pm_srgb',    'b',     'pm_rgb',     'b'),
      edge('pm_rgb',     'out',   'pm_lum',     'a'),
      edge('pm_lum',     'out',   'pm_sizeBase','x'),
      // Time wobble (time broadcast from outer)
      edge('pm_time',    't',     'pm_tMul',    'a'),
      edge('pm_tMul',    'out',   'pm_sinT',    'x'),
      edge('pm_sinT',    'out',   'pm_wobble',  'x'),
      edge('pm_sizeBase','out',   'pm_dotSize', 'a'),
      edge('pm_wobble',  'out',   'pm_dotSize', 'b'),
      // Fill and draw
      edge('pm_sample',  'color', 'pm_fill',    'color'),
      edge('pm_fill',    'out',   'pm_ellipse', 'style'),
      edge('pm_split',   'x',     'pm_ellipse', 'x'),
      edge('pm_split',   'y',     'pm_ellipse', 'y'),
      edge('pm_dotSize', 'out',   'pm_ellipse', 'w'),
      edge('pm_dotSize', 'out',   'pm_ellipse', 'h'),
      edge('pm_ellipse', 'out',   RID,          'dot'),
      edge(RID,          'dot',   'pm_renderer','layers'),
    ],
  };
}

// ─── Sample: Chromatic Threshold ──────────────────────────────────────────
// A time-driven threshold level sweeps through the image. The thresholded
// version is blended (difference) against the original, creating an animated
// glitch / solarisation effect. Three nodes are all you need: filter, blend,
// draw.
function buildChromaticThreshold() {
  const img      = node('ct_img',    'io/image-url', { x: -400, y: 20  }, { url: 'https://picsum.photos/seed/threshold/500/500' });
  const time     = node('ct_time',   'time/seconds', { x: -400, y: 140 });
  const tMul     = node('ct_tMul',   'math/mul',     { x: -220, y: 140 }, { b: 0.8 });
  const sinT     = node('ct_sinT',   'math/sin',     { x:  -60, y: 140 });
  const level    = node('ct_level',  'math/map',     { x:  120, y: 140 }, { inMin: -1, inMax: 1, outMin: 0.05, outMax: 0.95 });
  const filter   = node('ct_filter', 'image/filter', { x:   20, y:  20 }, { mode: 'threshold', level: 0.5 });
  const blend    = node('ct_blend',  'image/blend',  { x:  300, y:  20 }, { mode: 'difference' });
  const draw     = node('ct_draw',   'draw/image',   { x:  580, y:  20 }, { x: -250, y: -250, w: 500, h: 500 });
  const bg       = node('ct_bg',     'color/rgb',    { x:  580, y: 140 }, { r: 0, g: 0, b: 0, a: 255 });
  const renderer = node('ct_renderer','render/renderer',{ x: 780, y:  20 }, { width: 500, height: 500 });

  return {
    nodes: [img, time, tMul, sinT, level, filter, blend, draw, bg, renderer],
    edges: [
      // Threshold level is sin(t*0.8) mapped 0..1
      edge('ct_time',   't',     'ct_tMul',    'a'),
      edge('ct_tMul',   'out',   'ct_sinT',    'x'),
      edge('ct_sinT',   'out',   'ct_level',   'x'),
      edge('ct_level',  'out',   'ct_filter',  'level'),
      edge('ct_img',    'image', 'ct_filter',  'image'),
      // Difference blend: original XOR threshold = solarisation
      edge('ct_img',    'image', 'ct_blend',   'a'),
      edge('ct_filter', 'image', 'ct_blend',   'b'),
      edge('ct_blend',  'image', 'ct_draw',    'image'),
      edge('ct_draw',   'out',   'ct_renderer','layers'),
      edge('ct_bg',     'out',   'ct_renderer','background'),
    ],
  };
}

// ─── Sample: Pixel Sort ───────────────────────────────────────────────────
// Classic glitch art: for each row of a photo, identify runs of pixels whose
// brightness falls in the (lo, hi) range and sort them. Pixels outside the
// mask stay put — that's what gives pixel-sort its streaked, "leaking" look
// rather than a smooth gradient. Sliders drive the mask bounds live.
function buildPixelSort() {
  const img      = node('ps_img',    'io/image-url', { x: -400, y:  20 },
                        { url: 'https://picsum.photos/seed/pixel-sort/500/500' });
  const time     = node('ps_time',   'time/seconds', { x: -400, y: 100 });
  const lo       = node('ps_lo',     'io/slider',    { x: -400, y: 180 }, { value: 0.0,  min: 0, max: 1 });
  const hi       = node('ps_hi',     'io/slider',    { x: -400, y: 240 }, { value: 0.85, min: 0, max: 1 });
  // Gentle wobble on the upper bound — `hiFinal = hi + sin(t * 0.6) * 0.05`.
  // Keeps the streaks shimmering AND keeps the patch ticking after the image
  // loads (which is async; without a time-varying input the patch settles
  // before the image arrives and the canvas stays black until a slider moves).
  const tMul     = node('ps_tMul',   'math/mul',     { x: -240, y: 100 }, { b: 0.6 });
  const sinT     = node('ps_sinT',   'math/sin',     { x:  -80, y: 100 });
  const wobMul   = node('ps_wobMul', 'math/mul',     { x:  -80, y: 160 }, { b: 0.05 });
  const hiFinal  = node('ps_hiFinal','math/add',     { x:   80, y: 200 });
  const sort     = node('ps_sort',   'pixel/sort', { x: 240, y: 40 },
                        { axis: 'rows', criterion: 'brightness', order: 'ascending' });
  const draw     = node('ps_draw',   'draw/image',   { x: 460, y: 40 },
                        { x: -250, y: -250, w: 500, h: 500 });
  const bg       = node('ps_bg',     'color/rgb',    { x: 460, y: 160 }, { r: 12, g: 10, b: 16, a: 255 });
  const renderer = node('ps_renderer','render/renderer',{ x: 720, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [img, time, lo, hi, tMul, sinT, wobMul, hiFinal, sort, draw, bg, renderer],
    edges: [
      edge('ps_img',  'image', 'ps_sort',  'image'),
      // hiFinal = hi + sin(time * 0.6) * 0.05
      edge('ps_time', 't',     'ps_tMul',  'a'),
      edge('ps_tMul', 'out',   'ps_sinT',  'x'),
      edge('ps_sinT', 'out',   'ps_wobMul','a'),
      edge('ps_hi',   'out',   'ps_hiFinal','a'),
      edge('ps_wobMul','out',  'ps_hiFinal','b'),
      edge('ps_lo',   'out',   'ps_sort',  'lo'),
      edge('ps_hiFinal','out', 'ps_sort',  'hi'),
      edge('ps_sort', 'image', 'ps_draw',  'image'),
      edge('ps_draw', 'out',   'ps_renderer', 'layers'),
      edge('ps_bg',   'out',   'ps_renderer', 'background'),
    ],
  };
}

// ─── Sample: Hue Sort ─────────────────────────────────────────────────────
// Pixel sort along COLUMNS (vertical) using HUE as the criterion — produces
// rainbow-streaked vertical bands where the mask catches mid-saturation
// regions of the image.
function buildHueSort() {
  const img      = node('hs_img',    'io/image-url', { x: -400, y:  20 },
                        { url: 'https://picsum.photos/seed/hue-sort/500/500' });
  const time     = node('hs_time',   'time/seconds', { x: -400, y: 100 });
  const lo       = node('hs_lo',     'io/slider',    { x: -400, y: 180 }, { value: 0.0,  min: 0, max: 1 });
  const hi       = node('hs_hi',     'io/slider',    { x: -400, y: 240 }, { value: 0.9,  min: 0, max: 1 });
  // Same time-driven wobble pattern as Pixel Sort — keeps the streaks alive
  // AND keeps the patch ticking after the async image load completes.
  const tMul     = node('hs_tMul',   'math/mul',     { x: -240, y: 100 }, { b: 0.5 });
  const sinT     = node('hs_sinT',   'math/sin',     { x:  -80, y: 100 });
  const wobMul   = node('hs_wobMul', 'math/mul',     { x:  -80, y: 160 }, { b: 0.06 });
  const hiFinal  = node('hs_hiFinal','math/add',     { x:   80, y: 200 });
  const sort     = node('hs_sort',   'pixel/sort',   { x:  240, y:  40 },
                        { axis: 'cols', criterion: 'hue', order: 'ascending' });
  const draw     = node('hs_draw',   'draw/image',   { x:  460, y:  40 },
                        { x: -250, y: -250, w: 500, h: 500 });
  const bg       = node('hs_bg',     'color/rgb',    { x:  460, y: 160 }, { r: 10, g: 12, b: 18, a: 255 });
  const renderer = node('hs_renderer','render/renderer',{ x: 720, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [img, time, lo, hi, tMul, sinT, wobMul, hiFinal, sort, draw, bg, renderer],
    edges: [
      edge('hs_img',  'image', 'hs_sort',  'image'),
      edge('hs_time', 't',     'hs_tMul',  'a'),
      edge('hs_tMul', 'out',   'hs_sinT',  'x'),
      edge('hs_sinT', 'out',   'hs_wobMul','a'),
      edge('hs_hi',   'out',   'hs_hiFinal','a'),
      edge('hs_wobMul','out',  'hs_hiFinal','b'),
      edge('hs_lo',   'out',   'hs_sort',  'lo'),
      edge('hs_hiFinal','out', 'hs_sort',  'hi'),
      edge('hs_sort', 'image', 'hs_draw',  'image'),
      edge('hs_draw', 'out',   'hs_renderer', 'layers'),
      edge('hs_bg',   'out',   'hs_renderer', 'background'),
    ],
  };
}

// ─── Sample: Channel Sort ─────────────────────────────────────────────────
// Pixel sort along rows using the RED channel only, descending — pixels
// stack with the reddest on the LEFT, fading toward green-blue toward the
// right of each masked run. Produces angry red horizontal bands.
function buildChannelSort() {
  const img      = node('cs_img',    'io/image-url', { x: -400, y:  20 },
                        { url: 'https://picsum.photos/seed/channel-sort/500/500' });
  const time     = node('cs_time',   'time/seconds', { x: -400, y: 100 });
  const lo       = node('cs_lo',     'io/slider',    { x: -400, y: 180 }, { value: 0.0,  min: 0, max: 1 });
  const hi       = node('cs_hi',     'io/slider',    { x: -400, y: 240 }, { value: 0.9,  min: 0, max: 1 });
  const tMul     = node('cs_tMul',   'math/mul',     { x: -240, y: 100 }, { b: 0.7 });
  const sinT     = node('cs_sinT',   'math/sin',     { x:  -80, y: 100 });
  const wobMul   = node('cs_wobMul', 'math/mul',     { x:  -80, y: 160 }, { b: 0.04 });
  const hiFinal  = node('cs_hiFinal','math/add',     { x:   80, y: 200 });
  const sort     = node('cs_sort',   'pixel/sort',   { x:  240, y:  40 },
                        { axis: 'rows', criterion: 'red', order: 'descending' });
  const draw     = node('cs_draw',   'draw/image',   { x:  460, y:  40 },
                        { x: -250, y: -250, w: 500, h: 500 });
  const bg       = node('cs_bg',     'color/rgb',    { x:  460, y: 160 }, { r: 14, g: 10, b: 10, a: 255 });
  const renderer = node('cs_renderer','render/renderer',{ x: 720, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [img, time, lo, hi, tMul, sinT, wobMul, hiFinal, sort, draw, bg, renderer],
    edges: [
      edge('cs_img',  'image', 'cs_sort',  'image'),
      edge('cs_time', 't',     'cs_tMul',  'a'),
      edge('cs_tMul', 'out',   'cs_sinT',  'x'),
      edge('cs_sinT', 'out',   'cs_wobMul','a'),
      edge('cs_hi',   'out',   'cs_hiFinal','a'),
      edge('cs_wobMul','out',  'cs_hiFinal','b'),
      edge('cs_lo',   'out',   'cs_sort',  'lo'),
      edge('cs_hiFinal','out', 'cs_sort',  'hi'),
      edge('cs_sort', 'image', 'cs_draw',  'image'),
      edge('cs_draw', 'out',   'cs_renderer', 'layers'),
      edge('cs_bg',   'out',   'cs_renderer', 'background'),
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
function buildPaintTrail() {
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
      edge('pt_hueMul',   'out',     'pt_hsl',      'h'),
      edge('pt_hsl',      'out',     'pt_fill',     'color'),
      edge('pt_fill',     'out',     'pt_brush',    'style'),
      edge('pt_mouse',    'x',       'pt_brush',    'x'),
      edge('pt_mouse',    'y',       'pt_brush',    'y'),
      edge('pt_size',     'out',     'pt_brush',    'w'),
      edge('pt_size',     'out',     'pt_brush',    'h'),
      // Stack: [prev layer, brush layer] → from-layer
      edge('pt_drawPrev', 'out',     'pt_stack',    'a'),
      edge('pt_brush',    'out',     'pt_stack',    'b'),
      edge('pt_stack',    'out',     'pt_fromLayer', 'layer'),
      // from-layer → Write (feedback) + display. Both end in L→R nodes
      // sitting to the right of from-layer — no back-edge.
      edge('pt_fromLayer','image',   'pt_write',    'next'),
      edge('pt_fromLayer','image',   'pt_display',  'image'),
      edge('pt_display',  'out',     'pt_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Subpatch Petals ──────────────────────────────────────────────
// A flower of 8 colored petals orbiting the canvas centre. Mouse X controls
// the orbit radius (drag right = petals push out); mouse Y controls the
// breathing speed. The per-petal math (orbit position, breathing radius,
// HSL hue from angle + time, ellipse) is wrapped inside a single "Petal"
// subpatch with inputs (angle, t, orbitR, beatSpeed) → output (layer).
// The outer patch is just a foreach over a CircularSpread that drops each
// angle into the subpatch. Demonstrates a subpatch with both inputs and
// outputs, hiding ~12 inner nodes behind one tidy box. Press I (with the
// subpatch selected) to dive in and see what is inside.
function buildSubpatchPetals() {
  const N = 8;
  const mouse    = node('sp_mouse',    'input/mouse',     { x: -460, y:  20 });
  const time     = node('sp_time',     'time/seconds',    { x: -460, y:  90 });
  const circular = nodeWithPorts('sp_circular', 'spread/circular',
                                 { x: -460, y: 160 }, { count: N, phase: 0 });
  // Map mouse position into useful ranges. Defaults keep the patch
  // visible even when the cursor isn't over the canvas yet.
  const orbitR     = node('sp_orbitR',     'math/map', { x: -260, y:  20 }, { inMin: -250, inMax: 250, outMin: 50, outMax: 180 });
  const beatSpeed  = node('sp_beatSpeed',  'math/map', { x: -260, y:  90 }, { inMin: -250, inMax: 250, outMin: 1, outMax: 6 });

  // ForEach region — fan the 8 angles through the petal subpatch.
  const RID = 'sp_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -200, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 740, height: 440, label: 'ForEach Petal' },
      ports: [
        { id: 'angle', name: 'angle', type: 'number', direction: 'input' },
        { id: 'petal', name: 'petal', type: 'layer',  direction: 'output' },
      ],
    },
    style: { width: 740, height: 440 },
  };

  // Subpatch lives inside the region — runs once per iteration.
  const SID = 'sp_petal';
  const subpatch: FloretNode = {
    id: SID,
    type: 'floret',
    position: { x: 100, y: 40 },
    data: {
      defType: 'meta/subpatch',
      params: { label: 'Petal', width: 620, height: 380 },
      ports: [
        { id: 'p_angle',  name: 'angle',  type: 'number', direction: 'input'  },
        { id: 'p_t',      name: 't',      type: 'number', direction: 'input'  },
        { id: 'p_orbitR', name: 'orbitR', type: 'number', direction: 'input'  },
        { id: 'p_speed',  name: 'speed',  type: 'number', direction: 'input'  },
        { id: 'p_layer',  name: 'layer',  type: 'layer',  direction: 'output' },
      ],
    },
    parentId: RID,
    extent: 'parent',
  };

  // Inside the subpatch: petal center = (250 + cos(angle)·orbitR,
  //                                       250 + sin(angle)·orbitR).
  const cosA       = node('sp_cosA',       'math/cos', {  x:  40, y:  40 }, {}, SID);
  const sinA       = node('sp_sinA',       'math/sin', {  x:  40, y: 100 }, {}, SID);
  const offsetX    = node('sp_offsetX',    'math/mul', { x: 180, y:  40 }, {}, SID);
  const offsetY    = node('sp_offsetY',    'math/mul', { x: 180, y: 100 }, {}, SID);
  const px         = node('sp_px',         'math/add', { x: 320, y:  40 }, { b: 0 }, SID);
  const py         = node('sp_py',         'math/add', { x: 320, y: 100 }, { b: 0 }, SID);
  // Breath: radius pulses with sin(t·speed + angle·4).
  const tBeat      = node('sp_tBeat',      'math/mul', { x:  40, y: 160 }, {}, SID);
  const aBeat      = node('sp_aBeat',      'math/mul', { x:  40, y: 210 }, { b: 4 }, SID);
  const beatSum    = node('sp_beatSum',    'math/add', { x: 180, y: 185 }, {}, SID);
  const beat       = node('sp_beat',       'math/sin', { x: 320, y: 185 }, {}, SID);
  const beatScaled = node('sp_beatScaled', 'math/mul', { x: 460, y: 185 }, { b: 28 }, SID);
  const radius     = node('sp_radius',     'math/add', { x: 460, y: 245 }, { b: 64 }, SID);
  const radiusHalf = node('sp_radiusHalf', 'math/mul', { x: 460, y: 295 }, { b: 0.55 }, SID);
  // Hue = angle·(360/TAU) + t·40, then HSL → fill.
  const hueA       = node('sp_hueA',       'math/mul', { x:  40, y: 270 }, { b: 360 / (Math.PI * 2) }, SID);
  const hueT       = node('sp_hueT',       'math/mul', { x:  40, y: 320 }, { b: 40 }, SID);
  const hue        = node('sp_hue',        'math/add', { x: 180, y: 295 }, {}, SID);
  const hsl        = node('sp_hsl',        'color/hsl',{ x: 320, y: 270 }, { s: 85, l: 60, a: 210 }, SID);
  const fill       = node('sp_fill',       'style/fill',{ x: 320, y: 330 }, { mode: 'color' }, SID);
  // Ellipse(px, py, radius, radius·0.55) styled with fill.
  const ellipse    = node('sp_ellipse',    'draw/ellipse', { x: 580, y: 200 }, {}, SID);

  const renderer = node('sp_renderer', 'render/renderer', { x: 600, y: 20 },
                        { width: 500, height: 500, background: '#08081a' });

  return {
    nodes: [
      mouse, time, circular, orbitR, beatSpeed,
      region, subpatch,
      cosA, sinA, offsetX, offsetY, px, py,
      tBeat, aBeat, beatSum, beat, beatScaled, radius, radiusHalf,
      hueA, hueT, hue, hsl, fill, ellipse,
      renderer,
    ],
    edges: [
      // Mouse → control mappings
      edge('sp_mouse', 'x', 'sp_orbitR',    'x'),
      edge('sp_mouse', 'y', 'sp_beatSpeed', 'x'),

      // Outer: fan circular spread into region; region petal → renderer.
      edge('sp_circular', 'out', RID, 'angle'),
      edge(RID, 'petal', 'sp_renderer', 'layers'),
      // Cross-scope broadcasts into the subpatch (which lives in region scope).
      // Note: subpatch ports use port IDs as handle names — `p_angle` etc.
      edge(RID,            'angle', SID, 'p_angle'),
      edge('sp_time',      't',     SID, 'p_t'),
      edge('sp_orbitR',    'out',   SID, 'p_orbitR'),
      edge('sp_beatSpeed', 'out',   SID, 'p_speed'),
      edge(SID,            'p_layer', RID, 'petal'),

      // ── Inside the subpatch ──
      // angle → cos / sin
      edge(SID, 'p_angle', 'sp_cosA', 'x'),
      edge(SID, 'p_angle', 'sp_sinA', 'x'),
      // offsets = cos/sin * orbitR
      edge('sp_cosA',  'out', 'sp_offsetX', 'a'),
      edge('sp_sinA',  'out', 'sp_offsetY', 'a'),
      edge(SID, 'p_orbitR', 'sp_offsetX', 'b'),
      edge(SID, 'p_orbitR', 'sp_offsetY', 'b'),
      // petal center = (250 + offsetX, 250 + offsetY) — `b: 250` provides the centre default
      edge('sp_offsetX', 'out', 'sp_px', 'a'),
      edge('sp_offsetY', 'out', 'sp_py', 'a'),
      // breath = sin(t·speed + angle·4)
      edge(SID, 'p_t',     'sp_tBeat', 'a'),
      edge(SID, 'p_speed', 'sp_tBeat', 'b'),
      edge(SID, 'p_angle', 'sp_aBeat', 'a'),
      edge('sp_tBeat', 'out', 'sp_beatSum', 'a'),
      edge('sp_aBeat', 'out', 'sp_beatSum', 'b'),
      edge('sp_beatSum', 'out', 'sp_beat', 'x'),
      // radius
      edge('sp_beat', 'out', 'sp_beatScaled', 'a'),
      edge('sp_beatScaled', 'out', 'sp_radius', 'a'),
      edge('sp_radius', 'out', 'sp_radiusHalf', 'a'),
      // hue
      edge(SID, 'p_angle', 'sp_hueA', 'a'),
      edge(SID, 'p_t',     'sp_hueT', 'a'),
      edge('sp_hueA', 'out', 'sp_hue', 'a'),
      edge('sp_hueT', 'out', 'sp_hue', 'b'),
      // hsl → fill
      edge('sp_hue', 'out', 'sp_hsl', 'h'),
      edge('sp_hsl', 'out', 'sp_fill', 'color'),
      // ellipse
      edge('sp_px',         'out', 'sp_ellipse', 'x'),
      edge('sp_py',         'out', 'sp_ellipse', 'y'),
      edge('sp_radius',     'out', 'sp_ellipse', 'w'),
      edge('sp_radiusHalf', 'out', 'sp_ellipse', 'h'),
      edge('sp_fill',       'out', 'sp_ellipse', 'style'),
      // ellipse → subpatch output
      edge('sp_ellipse', 'out', SID, 'p_layer'),
    ],
  };
}

// ─── Sample: Slicer Drift ─────────────────────────────────────────────────
// Loads a photo, slices it into a 6×6 grid of 36 sub-images, and lets
// every slice drift independently on its own Perlin-noise wanderer.
// Each slice keeps the bitmap it was cut from, so the image stays
// "readable" even as the pieces scatter — like a photo cut apart and
// blown into the wind. Demonstrates wiring a Spread<image> through a
// ForEach via an `image` splicer-in.
function buildSlicerDrift() {
  const COLS = 6;
  const ROWS = 6;
  const N = COLS * ROWS;
  const SLICE = 84; // displayed slice size (px)

  const img    = node('sl_img',   'io/image-url', { x: -540, y:  20 },
                      { url: 'https://picsum.photos/seed/floret-slicer/600/600' });
  const time   = node('sl_time',  'time/seconds', { x: -540, y: 100 });
  const slicer = nodeWithPorts('sl_slicer', 'image/slicer',
                               { x: -340, y:  20 }, { cols: COLS, rows: ROWS });
  // Seed spread = [0..N-1]; gives each slice a unique Perlin trajectory.
  const seeds  = nodeWithPorts('sl_seeds', 'spread/range',
                               { x: -540, y: 180 }, { count: N });

  // ForEach region carries one image + one seed per iteration and emits
  // one positioned layer.
  const RID = 'sl_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -100, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 760, height: 360, label: 'ForEach Slice' },
      ports: [
        { id: 'image',  name: 'image',  type: 'image',  direction: 'input'  },
        { id: 'seed',   name: 'seed',   type: 'number', direction: 'input'  },
        { id: 'layer',  name: 'layer',  type: 'layer',  direction: 'output' },
      ],
    },
    style: { width: 760, height: 360 },
  };

  // Inside the region: drift each slice on its own wanderer.
  const wander = node('sl_wander', 'random/wanderer2d', { x:  40, y:  40 },
                      { speed: 0.25, area: 180 }, RID);
  const split  = node('sl_split',  'vec2/split',       { x: 220, y:  40 }, {}, RID);
  const px     = node('sl_px',     'math/add',         { x: 360, y:  40 }, { b: -SLICE / 2 }, RID);
  const py     = node('sl_py',     'math/add',         { x: 360, y: 100 }, { b: -SLICE / 2 }, RID);
  const draw   = node('sl_draw',   'draw/image',       { x: 540, y:  60 }, { w: SLICE, h: SLICE }, RID);

  const renderer = node('sl_renderer', 'render/renderer', { x: 720, y: 20 },
                        { width: 500, height: 500, background: '#08081a' });

  return {
    nodes: [img, time, slicer, seeds, region, wander, split, px, py, draw, renderer],
    edges: [
      // Outer: image → slicer; slicer + seeds fan into region; layer → renderer.
      edge('sl_img', 'image', 'sl_slicer', 'image'),
      edge('sl_slicer', 'images', RID, 'image'),
      edge('sl_seeds',  'out', RID, 'seed'),
      edge(RID, 'layer', 'sl_renderer', 'layers'),

      // Time is broadcast into the region (single → all iterations).
      edge('sl_time', 't', 'sl_wander', 't'),
      // Per-iteration seed → wanderer
      edge(RID, 'seed', 'sl_wander', 'seed'),
      // wanderer → split → (px, py) anchored at canvas centre
      edge('sl_wander', 'out', 'sl_split', 'v'),
      edge('sl_split', 'x', 'sl_px', 'a'),
      edge('sl_split', 'y', 'sl_py', 'a'),
      // draw the slice at the drifted position
      edge(RID, 'image', 'sl_draw', 'image'),
      edge('sl_px', 'out', 'sl_draw', 'x'),
      edge('sl_py', 'out', 'sl_draw', 'y'),
      edge('sl_draw', 'out', RID, 'layer'),
    ],
  };
}

// ─── Sample: Mixed Media ─────────────────────────────────────────────────
// Combines two layer sources into one renderer:
//   1) the photo slicer-drift (a Spread<layer> from a ForEach)
//   2) a ring of pulsing dots arranged on a CircularSpread + ForEach
//   3) a single stroke crosshair centred on the canvas
// All three feed `transform/group` (multi pin = accepts many wires of
// any kind) → renderer.layers. Draw order = wire order, so the photo
// goes in first (background), then the ring, then the crosshair on top.
function buildMixedMedia() {
  const COLS = 6, ROWS = 6, N = COLS * ROWS;
  const SLICE = 70;
  const RING_N = 12;

  // ── Slicer drift (background) ─────────────────────────────
  const img    = node('mm_img',   'io/image-url', { x: -560, y:  20 },
                      { url: 'https://picsum.photos/seed/floret-mix/600/600' });
  const time   = node('mm_time',  'time/seconds', { x: -560, y: 100 });
  const slicer = nodeWithPorts('mm_slicer', 'image/slicer',
                               { x: -340, y:  20 }, { cols: COLS, rows: ROWS });
  const seeds  = nodeWithPorts('mm_seeds', 'spread/range',
                               { x: -560, y: 180 }, { count: N });

  const SLICE_R = 'mm_slice_region';
  const sliceRegion: FloretNode = {
    id: SLICE_R,
    type: 'floret',
    position: { x: -100, y: 20 },
    data: {
      defType: 'region/foreach',
      params: { width: 720, height: 280, label: 'ForEach Slice' },
      ports: [
        { id: 'image', name: 'image', type: 'image',  direction: 'input'  },
        { id: 'seed',  name: 'seed',  type: 'number', direction: 'input'  },
        { id: 'layer', name: 'layer', type: 'layer',  direction: 'output' },
      ],
    },
    style: { width: 720, height: 280 },
  };
  const wander = node('mm_wander', 'random/wanderer2d', { x:  60, y:  40 },
                      { speed: 0.2, area: 150 }, SLICE_R);
  const split  = node('mm_split',  'vec2/split',       { x: 220, y:  40 }, {}, SLICE_R);
  const px     = node('mm_px',     'math/add',         { x: 360, y:  40 }, { b: -SLICE / 2 }, SLICE_R);
  const py     = node('mm_py',     'math/add',         { x: 360, y: 100 }, { b: -SLICE / 2 }, SLICE_R);
  const drawIm = node('mm_drawIm', 'draw/image',       { x: 540, y:  60 }, { w: SLICE, h: SLICE }, SLICE_R);

  // ── Pulsing ring of dots (foreground) ─────────────────────
  const angles = nodeWithPorts('mm_angles', 'spread/circular',
                               { x: -560, y: 380 }, { count: RING_N, phase: 0 });

  const RING_R = 'mm_ring_region';
  const ringRegion: FloretNode = {
    id: RING_R,
    type: 'floret',
    position: { x: -100, y: 360 },
    data: {
      defType: 'region/foreach',
      params: { width: 720, height: 260, label: 'ForEach Dot' },
      ports: [
        { id: 'angle', name: 'angle', type: 'number', direction: 'input'  },
        { id: 'layer', name: 'layer', type: 'layer',  direction: 'output' },
      ],
    },
    style: { width: 720, height: 260 },
  };
  const cosA   = node('mm_cosA',   'math/cos', {  x: 60,  y:  40 }, {}, RING_R);
  const sinA   = node('mm_sinA',   'math/sin', {  x: 60,  y: 100 }, {}, RING_R);
  const ringX  = node('mm_ringX',  'math/mul', { x: 200, y:  40 }, { b: 200 }, RING_R);
  const ringY  = node('mm_ringY',  'math/mul', { x: 200, y: 100 }, { b: 200 }, RING_R);
  const dotPx  = node('mm_dotPx',  'math/add', { x: 340, y:  40 }, { b: 0 }, RING_R);
  const dotPy  = node('mm_dotPy',  'math/add', { x: 340, y: 100 }, { b: 0 }, RING_R);
  // Pulse: size = 14 + sin(t*3 + angle*2) * 8
  const tPulse = node('mm_tPulse', 'math/mul', {  x: 60, y: 160 }, { b: 3 }, RING_R);
  const aPulse = node('mm_aPulse', 'math/mul', {  x: 60, y: 210 }, { b: 2 }, RING_R);
  const pSum   = node('mm_pSum',   'math/add', { x: 200, y: 185 }, {}, RING_R);
  const pulse  = node('mm_pulse',  'math/sin', { x: 340, y: 185 }, {}, RING_R);
  const pScale = node('mm_pScale', 'math/mul', { x: 480, y: 185 }, { b: 8 }, RING_R);
  const dotSz  = node('mm_dotSz',  'math/add', { x: 480, y: 235 }, { b: 14 }, RING_R);
  // Style: hue cycles around the ring + time
  const hueA   = node('mm_hueA',   'math/mul', {  x: 60, y: 260 }, { b: 360 / (Math.PI * 2) }, RING_R);
  const hueT   = node('mm_hueT',   'math/mul', {  x: 60, y: 310 }, { b: 60 }, RING_R);
  const hueAdd = node('mm_hueAdd', 'math/add', { x: 200, y: 285 }, {}, RING_R);
  const hsl    = node('mm_hsl',    'color/hsl', { x: 340, y: 260 }, { s: 90, l: 65, a: 255 }, RING_R);
  const fill   = node('mm_fill',   'style/fill', { x: 340, y: 320 }, { mode: 'color' }, RING_R);
  const dot    = node('mm_dot',    'draw/ellipse', { x: 540, y: 180 }, {}, RING_R);

  // ── Crosshair (single layer in front) ─────────────────────
  // 4 short lines drawn from canvas centre using one draw/line per side
  // would be 4 nodes — instead use a Polygon with stroke for compactness.
  const xhStyle = node('mm_xhStyle', 'style/stroke-weight', { x: -100, y: 700 }, { w: 2 });
  const xhFill  = node('mm_xhFill',  'style/no-fill',       { x:  80, y: 700 });
  const xhColor = node('mm_xhColor', 'style/fill',          { x: -100, y: 770 }, { mode: 'rgb', r: 255, g: 255, b: 255, a: 200 });
  const xhV     = node('mm_xhV',     'draw/line',           { x: 260, y: 700 },
                       { x1: 0, y1: -230, x2: 0, y2: 230 });
  const xhH     = node('mm_xhH',     'draw/line',           { x: 260, y: 780 },
                       { x1: -230, y1: 0, x2: 230, y2: 0 });

  // ── Group + renderer ──────────────────────────────────────
  // 4 slot pins: slice region, ring region, vertical crosshair, horizontal.
  const group    = node('mm_group',    'transform/group',  { x: 720, y: 200 }, { slotCount: 4 });
  const renderer = node('mm_renderer', 'render/renderer',  { x: 920, y: 200 },
                        { width: 500, height: 500, background: '#08081a' });

  return {
    nodes: [
      img, time, slicer, seeds, sliceRegion,
      wander, split, px, py, drawIm,
      angles, ringRegion,
      cosA, sinA, ringX, ringY, dotPx, dotPy,
      tPulse, aPulse, pSum, pulse, pScale, dotSz,
      hueA, hueT, hueAdd, hsl, fill, dot,
      xhStyle, xhFill, xhColor, xhV, xhH,
      group, renderer,
    ],
    edges: [
      // ── Slicer drift wiring ──
      edge('mm_img',    'image',  'mm_slicer', 'image'),
      edge('mm_slicer', 'images', SLICE_R, 'image'),
      edge('mm_seeds',  'out', SLICE_R, 'seed'),
      edge('mm_time',   't',      'mm_wander', 't'),
      edge(SLICE_R,     'seed',   'mm_wander', 'seed'),
      edge('mm_wander', 'out',    'mm_split', 'v'),
      edge('mm_split',  'x',      'mm_px', 'a'),
      edge('mm_split',  'y',      'mm_py', 'a'),
      edge(SLICE_R,     'image',  'mm_drawIm', 'image'),
      edge('mm_px',     'out',    'mm_drawIm', 'x'),
      edge('mm_py',     'out',    'mm_drawIm', 'y'),
      edge('mm_drawIm', 'out',    SLICE_R, 'layer'),

      // ── Ring wiring ──
      edge('mm_angles', 'out', RING_R, 'angle'),
      edge(RING_R, 'angle', 'mm_cosA', 'x'),
      edge(RING_R, 'angle', 'mm_sinA', 'x'),
      edge('mm_cosA', 'out', 'mm_ringX', 'a'),
      edge('mm_sinA', 'out', 'mm_ringY', 'a'),
      edge('mm_ringX', 'out', 'mm_dotPx', 'a'),
      edge('mm_ringY', 'out', 'mm_dotPy', 'a'),
      // pulse
      edge('mm_time',   't', 'mm_tPulse', 'a'),
      edge(RING_R, 'angle', 'mm_aPulse', 'a'),
      edge('mm_tPulse', 'out', 'mm_pSum', 'a'),
      edge('mm_aPulse', 'out', 'mm_pSum', 'b'),
      edge('mm_pSum', 'out', 'mm_pulse', 'x'),
      edge('mm_pulse', 'out', 'mm_pScale', 'a'),
      edge('mm_pScale', 'out', 'mm_dotSz', 'a'),
      // hue
      edge(RING_R, 'angle', 'mm_hueA', 'a'),
      edge('mm_time', 't', 'mm_hueT', 'a'),
      edge('mm_hueA', 'out', 'mm_hueAdd', 'a'),
      edge('mm_hueT', 'out', 'mm_hueAdd', 'b'),
      edge('mm_hueAdd', 'out', 'mm_hsl', 'h'),
      edge('mm_hsl', 'out', 'mm_fill', 'color'),
      edge('mm_fill', 'out', 'mm_dot', 'style'),
      // ellipse
      edge('mm_dotPx', 'out', 'mm_dot', 'x'),
      edge('mm_dotPy', 'out', 'mm_dot', 'y'),
      edge('mm_dotSz', 'out', 'mm_dot', 'w'),
      edge('mm_dotSz', 'out', 'mm_dot', 'h'),
      edge('mm_dot', 'out', RING_R, 'layer'),

      // ── Crosshair wiring ──
      edge('mm_xhColor', 'out', 'mm_xhStyle', 'base'),
      edge('mm_xhStyle', 'out', 'mm_xhFill',  'base'),
      edge('mm_xhFill',  'out', 'mm_xhV', 'style'),
      edge('mm_xhFill',  'out', 'mm_xhH', 'style'),

      // ── Each layer source into its own Group slot → renderer ──
      // Slots draw in order, top to bottom: slot 1 first (background),
      // slot 4 last (on top). Use the +/− buttons on the Group header
      // to add or remove slots; each pin accepts ONE wire.
      edge(SLICE_R,    'layer', 'mm_group', 's1'),
      edge(RING_R,     'layer', 'mm_group', 's2'),
      edge('mm_xhV',   'out',   'mm_group', 's3'),
      edge('mm_xhH',   'out',   'mm_group', 's4'),
      edge('mm_group', 'out',   'mm_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Gravity Drop ─────────────────────────────────────────────────
// A single ball under constant gravity, bouncing off the floor with an
// elastic damping coefficient. Position and velocity are each kept in
// a state/value node — the most compact way to demonstrate Nature of
// Code's "Forces and motion" loop on a single body.
//
//   each tick:
//     v_next = v + gravity
//     p_next = p + v_next
//     if p_next.y > floor:
//        p_next.y = floor
//        v_next.y = -v_next.y * restitution
//        v_next.x *= floorFriction   (so it doesn't roll forever)
function buildGravityDrop() {
  const time = node('gd_time', 'time/seconds', { x: -560, y:  20 });
  // Constants: gravity vector, floor y, restitution, ground friction.
  const gravity = node('gd_gravity', 'vec2/make', { x: -560, y: 100 }, { x: 0, y: 0.45 });
  const floorY  = node('gd_floor',   'value/number', { x: -560, y: 180 }, { value: 220 });
  const rest    = node('gd_rest',    'io/slider',    { x: -560, y: 260 }, { value: 0.86, min: 0.1, max: 1 });
  const friction= node('gd_friction','io/slider',    { x: -560, y: 340 }, { value: 0.992, min: 0.5, max: 1 });

  // Position state — init at (0, -220) so the ball starts at the top edge.
  const initPos = node('gd_initPos', 'vec2/make', { x: -340, y:  20 }, { x: 0, y: -220 });
  const initVel = node('gd_initVel', 'vec2/make', { x: -340, y: 100 }, { x: 0, y: 0 });
  const pos     = node('gd_pos', 'state/value', { x: -160, y:  20 });
  const vel     = node('gd_vel', 'state/value', { x: -160, y: 100 });

  // Tentative next velocity / position.
  const vAccum  = node('gd_vAccum', 'vec2/add', { x:   40, y: 100 });
  const pAccum  = node('gd_pAccum', 'vec2/add', { x:   40, y:  20 });

  // Bounce decision — needs p_next.y and v_next.y separately.
  const pSplit  = node('gd_pSplit', 'vec2/split', { x:  220, y:  20 });
  const vSplit  = node('gd_vSplit', 'vec2/split', { x:  220, y: 100 });
  const hit     = node('gd_hit',    'logic/gt',   { x:  380, y:  60 });
  // y after bounce: clamp to floor; x stays.
  const yClamp  = node('gd_yClamp', 'logic/select', { x:  540, y:  20 });
  // vy after bounce: -vy * rest; otherwise unchanged.
  const vyNeg   = node('gd_vyNeg',  'math/neg',   { x:  380, y: 140 });
  const vyDamp  = node('gd_vyDamp', 'math/mul',   { x:  540, y: 140 });
  const vyOut   = node('gd_vyOut',  'logic/select', { x:  700, y: 140 });
  // vx after bounce: vx * friction (only on impact); otherwise vx.
  const vxDamp  = node('gd_vxDamp', 'math/mul',   { x:  540, y: 220 });
  const vxOut   = node('gd_vxOut',  'logic/select', { x:  700, y: 220 });

  const nextPos = node('gd_nextPos','vec2/make', { x:  860, y:  20 });
  const nextVel = node('gd_nextVel','vec2/make', { x:  860, y: 180 });

  // Render: a single ellipse plus a floor line.
  const fill    = node('gd_fill',    'style/fill', { x:  220, y: 320 }, { mode: 'rgb', r: 240, g: 180, b: 80, a: 255 });
  const ball    = node('gd_ball',    'draw/ellipse', { x: 380, y: 320 }, { w: 36, h: 36 });
  const ballSplit = node('gd_ballSplit','vec2/split', { x: 540, y: 320 });
  // Floor line.
  const floorStrokeC = node('gd_fStrokeC','style/stroke', { x: 220, y: 420 });
  const floorStrokeW = node('gd_fStrokeW','style/stroke-weight', { x: 400, y: 420 }, { w: 1 });
  const floorLine = node('gd_floorLine','draw/line', { x: 580, y: 420 },
                         { x1: -250, x2: 250, y2: 0 });

  const group   = node('gd_group',   'transform/group', { x: 1060, y: 320 }, { slotCount: 2 });
  const renderer = node('gd_renderer', 'render/renderer', { x: 1240, y: 320 },
                        { width: 500, height: 500, background: '#0a0a14' });

  return {
    nodes: [
      time, gravity, floorY, rest, friction,
      initPos, initVel, pos, vel,
      vAccum, pAccum, pSplit, vSplit, hit,
      yClamp, vyNeg, vyDamp, vyOut, vxDamp, vxOut,
      nextPos, nextVel,
      fill, ball, ballSplit,
      floorStrokeC, floorStrokeW, floorLine,
      group, renderer,
    ],
    edges: [
      // time wire kept solely to defeat dead-code elim (everything's stateful).
      edge('gd_time', 't', 'gd_vxDamp', 'b'), // overwritten below, but ensures gd_time is referenced
      // State init seeds.
      edge('gd_initPos', 'out', 'gd_pos', 'init'),
      edge('gd_initVel', 'out', 'gd_vel', 'init'),
      // v_next = v + gravity
      edge('gd_vel',     'current', 'gd_vAccum', 'a'),
      edge('gd_gravity', 'out',     'gd_vAccum', 'b'),
      // p_next = p + v_next
      edge('gd_pos',    'current', 'gd_pAccum', 'a'),
      edge('gd_vAccum', 'out',     'gd_pAccum', 'b'),
      // Split for the bounce check.
      edge('gd_pAccum', 'out', 'gd_pSplit', 'v'),
      edge('gd_vAccum', 'out', 'gd_vSplit', 'v'),
      // hit = p_next.y > floorY
      edge('gd_pSplit', 'y',  'gd_hit', 'a'),
      edge('gd_floor',  'out','gd_hit', 'b'),
      // y after bounce: select(hit, floorY, p_next.y)
      edge('gd_hit',    'out','gd_yClamp', 'cond'),
      edge('gd_floor',  'out','gd_yClamp', 'a'),
      edge('gd_pSplit', 'y',  'gd_yClamp', 'b'),
      // vy after bounce: select(hit, -v.y * rest, v.y)
      edge('gd_vSplit', 'y',  'gd_vyNeg', 'x'),
      edge('gd_vyNeg',  'out','gd_vyDamp', 'a'),
      edge('gd_rest',   'out','gd_vyDamp', 'b'),
      edge('gd_hit',    'out','gd_vyOut', 'cond'),
      edge('gd_vyDamp', 'out', 'gd_vyOut', 'a'),
      edge('gd_vSplit', 'y',  'gd_vyOut', 'b'),
      // vx after bounce: select(hit, v.x * friction, v.x)
      edge('gd_vSplit', 'x',  'gd_vxDamp', 'a'),
      edge('gd_friction','out','gd_vxDamp', 'b'),
      edge('gd_hit',    'out','gd_vxOut', 'cond'),
      edge('gd_vxDamp', 'out', 'gd_vxOut', 'a'),
      edge('gd_vSplit', 'x',  'gd_vxOut', 'b'),
      // Reassemble next pos / vel.
      edge('gd_pSplit', 'x',  'gd_nextPos', 'x'),
      edge('gd_yClamp', 'out', 'gd_nextPos', 'y'),
      edge('gd_vxOut',  'out','gd_nextVel', 'x'),
      edge('gd_vyOut',  'out','gd_nextVel', 'y'),
      // Write back into state.
      edge('gd_nextPos','out','gd_pos', 'next'),
      edge('gd_nextVel','out','gd_vel', 'next'),
      // Draw the ball at the current position.
      edge('gd_pos',    'current', 'gd_ballSplit', 'v'),
      edge('gd_ballSplit','x','gd_ball','x'),
      edge('gd_ballSplit','y','gd_ball','y'),
      edge('gd_fill',   'out','gd_ball','style'),
      // Floor line styled white.
      edge('gd_floor',     'out','gd_floorLine', 'y1'),
      edge('gd_floor',     'out','gd_floorLine', 'y2'),
      edge('gd_fStrokeC', 'out', 'gd_fStrokeW', 'base'),
      edge('gd_fStrokeW', 'out', 'gd_floorLine', 'style'),
      // Group both into the renderer.
      edge('gd_ball',     'out','gd_group','s1'),
      edge('gd_floorLine','out','gd_group','s2'),
      edge('gd_group',    'out','gd_renderer','layers'),
    ],
  };
}

// ─── Sample: Pendulum ─────────────────────────────────────────────────────
// Classic single pendulum. θ is the angle from straight-down (vertical).
//   ω_next = (ω + (-g/L)·sin θ) · damp
//   θ_next = θ + ω_next
// The bob position is (L·sin θ, L·cos θ) — note +Y is down so cos points
// straight down at θ = 0.
function buildPendulum() {
  const length    = node('pe_length',    'io/slider', { x: -560, y:  20 }, { value: 200, min: 80, max: 240 });
  const gravity   = node('pe_gravity',   'io/slider', { x: -560, y: 100 }, { value: 0.35, min: 0.1, max: 1 });
  const damping   = node('pe_damping',   'io/slider', { x: -560, y: 180 }, { value: 0.999, min: 0.96, max: 1 });

  // Initial angle: ~45 degrees (~0.78 rad) so it starts swinging immediately.
  const initTheta = node('pe_initTheta', 'value/number', { x: -340, y:  20 }, { value: 0.8 });
  const initOmega = node('pe_initOmega', 'value/number', { x: -340, y: 100 }, { value: 0 });
  const theta     = node('pe_theta',     'state/value', { x: -160, y:  20 });
  const omega     = node('pe_omega',     'state/value', { x: -160, y: 100 });

  // α = -(g / L) · sin θ
  const gOverL    = node('pe_gOverL',    'math/div', { x:  40, y: 200 });
  const sinTheta  = node('pe_sinTheta',  'math/sin', { x:  40, y:  20 });
  const alphaRaw  = node('pe_alphaRaw',  'math/mul', { x: 220, y:  60 });
  const alpha     = node('pe_alpha',     'math/neg', { x: 380, y:  60 });
  // ω_next = (ω + α) · damp
  const omegaSum  = node('pe_omegaSum',  'math/add', { x: 540, y: 100 });
  const omegaNext = node('pe_omegaNext', 'math/mul', { x: 700, y: 100 });
  // θ_next = θ + ω_next
  const thetaNext = node('pe_thetaNext', 'math/add', { x: 700, y:  20 });

  // Bob coords: (L·sin θ, L·cos θ).
  const cosTheta  = node('pe_cosTheta',  'math/cos', { x: 220, y: 240 });
  const bobX      = node('pe_bobX',      'math/mul', { x: 380, y: 200 });
  const bobY      = node('pe_bobY',      'math/mul', { x: 380, y: 280 });
  // Pivot is fixed at (0, -150) so the bob hangs into the lower half of the canvas.
  const pivotY    = node('pe_pivotY',    'value/number', { x: 540, y: 200 }, { value: -150 });
  const bobYAbs   = node('pe_bobYAbs',   'math/add', { x: 700, y: 280 });

  // Draw: rod from pivot to bob, plus a disc.
  const rodStrokeC = node('pe_rodC', 'style/stroke', { x: -160, y: 380 });
  const rodStrokeW = node('pe_rodW', 'style/stroke-weight', { x: 40, y: 380 }, { w: 2 });
  const rod        = node('pe_rod',  'draw/line', { x: 240, y: 380 }, { x1: 0 });
  const bobFill    = node('pe_bobFill', 'style/fill', { x: -160, y: 460 }, { mode: 'rgb', r: 220, g: 100, b: 80, a: 255 });
  const bob        = node('pe_bob',  'draw/ellipse', { x: 240, y: 460 }, { w: 32, h: 32 });

  const group    = node('pe_group',   'transform/group', { x: 900, y: 420 }, { slotCount: 2 });
  const renderer = node('pe_renderer','render/renderer', { x: 1100, y: 420 },
                        { width: 500, height: 500, background: '#0a0a14' });

  return {
    nodes: [
      length, gravity, damping,
      initTheta, initOmega, theta, omega,
      gOverL, sinTheta, alphaRaw, alpha, omegaSum, omegaNext, thetaNext,
      cosTheta, bobX, bobY, pivotY, bobYAbs,
      rodStrokeC, rodStrokeW, rod,
      bobFill, bob,
      group, renderer,
    ],
    edges: [
      // State seeds.
      edge('pe_initTheta','out','pe_theta','init'),
      edge('pe_initOmega','out','pe_omega','init'),
      // sinθ, cosθ
      edge('pe_theta','current','pe_sinTheta','x'),
      edge('pe_theta','current','pe_cosTheta','x'),
      // g/L
      edge('pe_gravity','out','pe_gOverL','a'),
      edge('pe_length', 'out', 'pe_gOverL','b'),
      // α_raw = (g/L) · sinθ;  α = -α_raw
      edge('pe_gOverL','out','pe_alphaRaw','a'),
      edge('pe_sinTheta','out','pe_alphaRaw','b'),
      edge('pe_alphaRaw','out','pe_alpha','x'),
      // ω_next = (ω + α) · damp
      edge('pe_omega','current','pe_omegaSum','a'),
      edge('pe_alpha','out','pe_omegaSum','b'),
      edge('pe_omegaSum','out','pe_omegaNext','a'),
      edge('pe_damping','out','pe_omegaNext','b'),
      // θ_next = θ + ω_next
      edge('pe_theta','current','pe_thetaNext','a'),
      edge('pe_omegaNext','out','pe_thetaNext','b'),
      // Write back.
      edge('pe_thetaNext','out','pe_theta','next'),
      edge('pe_omegaNext','out','pe_omega','next'),
      // Bob position.
      edge('pe_sinTheta','out','pe_bobX','a'),
      edge('pe_length',  'out','pe_bobX','b'),
      edge('pe_cosTheta','out','pe_bobY','a'),
      edge('pe_length',  'out','pe_bobY','b'),
      edge('pe_bobY',    'out','pe_bobYAbs','a'),
      edge('pe_pivotY',  'out','pe_bobYAbs','b'),
      // Rod line from pivot (0, pivotY) → bob.
      edge('pe_pivotY','out','pe_rod','y1'),
      edge('pe_bobX',  'out','pe_rod','x2'),
      edge('pe_bobYAbs','out','pe_rod','y2'),
      edge('pe_rodC',  'out','pe_rodW','base'),
      edge('pe_rodW',  'out','pe_rod','style'),
      // Bob disc.
      edge('pe_bobX',   'out','pe_bob','x'),
      edge('pe_bobYAbs','out','pe_bob','y'),
      edge('pe_bobFill','out','pe_bob','style'),
      // Renderer.
      edge('pe_rod','out','pe_group','s1'),
      edge('pe_bob','out','pe_group','s2'),
      edge('pe_group','out','pe_renderer','layers'),
    ],
  };
}


// ─── Sample: Particle Fountain ────────────────────────────────────────────
// 120 particles. Each particle's trajectory is a closed-form parabola
// in `phase`, where phase cycles 0..lifetime — so when phase wraps to
// 0 the particle "respawns" automatically without any state. ForEach
// drives the per-particle math. This is NOC §2's "Forces & motion"
// kinematics, but expressed declaratively instead of incrementally.
function buildParticleFountain() {
  const N = 120;
  const LIFETIME = 4.0;     // seconds per particle cycle
  const GRAVITY = 80;       // px per phase² (positive = downward)
  const ORIGIN_Y = 220;     // emit point — bottom of canvas (Y+ is down)
  const seeds = nodeWithPorts('pf_seeds', 'spread/range', { x: -560, y:  60 }, { count: N });
  const time  = node('pf_time', 'time/seconds', { x: -560, y: 140 });

  const RID = 'pf_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -340, y: 60 },
    data: {
      defType: 'region/foreach',
      params: { width: 820, height: 460, label: 'ForEach Particle' },
      // Only `seed` iterates — `time` is a single value broadcast into
      // every iteration directly from the outer scope. Putting `t` in
      // here as a port would cap the iteration count at 1.
      ports: [
        { id: 'seed', name: 'seed', type: 'number', direction: 'input'  },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 820, height: 460 },
  };

  // Per-particle phase = (t + seed · 0.07) mod LIFETIME
  const stagger = node('pf_stagger', 'math/mul', { x:  40, y:  40 }, { b: 0.07 }, RID);
  const tPlus   = node('pf_tPlus',   'math/add', { x: 220, y:  40 }, {}, RID);
  const phase   = node('pf_phase',   'math/mod', { x: 380, y:  40 }, { b: LIFETIME }, RID);

  // Random burst angle & speed per seed (live inside the region — per-iteration).
  const angle = node('pf_angle', 'random/uniform', { x:  40, y: 120 }, { min: -1.0, max: 1.0 }, RID);
  const speed = node('pf_speed', 'random/uniform', { x:  40, y: 180 }, { min: 150, max: 200 }, RID);

  // Initial velocity (sin·s, -cos·s) — angle measured from straight up.
  // angle = 0 ⇒ straight up; range [-1.3, 1.3] gives a wide upward fan.
  const cosA   = node('pf_cosA',   'math/cos', { x: 200, y: 180 }, {}, RID);
  const sinA   = node('pf_sinA',   'math/sin', { x: 200, y: 120 }, {}, RID);
  const negCos = node('pf_negCos', 'math/neg', { x: 360, y: 180 }, {}, RID);
  const vx0    = node('pf_vx0',    'math/mul', { x: 360, y: 120 }, {}, RID);
  const vy0    = node('pf_vy0',    'math/mul', { x: 520, y: 180 }, {}, RID);

  // Closed-form integration:
  //   px = vx0 · phase
  //   py = vy0 · phase + ½ · g · phase² + originY
  const px      = node('pf_px',      'math/mul', { x: 540, y: 120 }, {}, RID);
  const phaseSq = node('pf_phaseSq', 'math/sq',  { x: 540, y:  40 }, {}, RID);
  const gravTerm= node('pf_gravTerm','math/mul', { x: 700, y:  40 }, { b: 0.5 * GRAVITY }, RID);
  const vyTerm  = node('pf_vyTerm',  'math/mul', { x: 700, y: 180 }, {}, RID);
  const pyBase  = node('pf_pyBase',  'math/add', { x: 860, y:  40 }, {}, RID);
  const py      = node('pf_py',      'math/add', { x: 860, y: 100 }, { b: ORIGIN_Y }, RID);

  // Fade-out alpha: 255 → 0 across the lifetime so each particle dims as it falls.
  const tNorm = node('pf_tNorm', 'math/div', { x:  40, y: 240 }, { b: LIFETIME }, RID);
  const alphaFade = node('pf_alphaFade', 'math/mul', { x: 200, y: 240 }, { b: -255 }, RID);
  const alpha = node('pf_alpha', 'math/add', { x: 360, y: 240 }, { b: 255 }, RID);
  // Hue cycles by seed.
  const hue = node('pf_hue', 'math/mul', { x: 40, y: 300 }, { b: 3 }, RID);
  const hsl = node('pf_hsl', 'color/hsl', { x: 200, y: 300 }, { s: 85, l: 65 }, RID);
  const fill = node('pf_fill', 'style/fill', { x: 380, y: 300 }, { mode: 'color' }, RID);
  const dot  = node('pf_dot', 'draw/ellipse', { x: 700, y: 300 }, { w: 9, h: 9 }, RID);

  const renderer = node('pf_renderer', 'render/renderer', { x: 540, y: 60 },
                        { width: 500, height: 500, background: '#06060e' });

  return {
    nodes: [
      seeds, time, region,
      stagger, tPlus, phase,
      angle, speed, cosA, sinA, negCos, vx0, vy0,
      px, phaseSq, gravTerm, vyTerm, pyBase, py,
      tNorm, alphaFade, alpha, hue, hsl, fill, dot,
      renderer,
    ],
    edges: [
      // ForEach driver — only seed iterates.
      edge('pf_seeds', 'out', RID, 'seed'),
      // phase = (t + seed·0.07) mod LIFETIME — staggers particles in time.
      // Time is broadcast across the boundary into the inner pf_tPlus.
      edge(RID, 'seed', 'pf_stagger', 'a'),
      edge('pf_time',  't',      'pf_tPlus',   'a'),
      edge('pf_stagger', 'out', 'pf_tPlus', 'b'),
      edge('pf_tPlus',   'out', 'pf_phase', 'a'),
      // burst direction (seed only — angle / speed are stable per particle).
      edge(RID, 'seed', 'pf_angle', 'seed'),
      edge(RID, 'seed', 'pf_speed', 'seed'),
      // velocity components.
      edge('pf_angle', 'out', 'pf_cosA', 'x'),
      edge('pf_angle', 'out', 'pf_sinA', 'x'),
      edge('pf_cosA',  'out', 'pf_negCos', 'x'),
      edge('pf_sinA',  'out', 'pf_vx0', 'a'),
      edge('pf_speed', 'out', 'pf_vx0', 'b'),
      edge('pf_negCos','out', 'pf_vy0', 'a'),
      edge('pf_speed', 'out', 'pf_vy0', 'b'),
      // position.
      edge('pf_vx0',   'out', 'pf_px', 'a'),
      edge('pf_phase', 'out', 'pf_px', 'b'),
      edge('pf_phase', 'out', 'pf_phaseSq', 'x'),
      edge('pf_phaseSq','out','pf_gravTerm','a'),
      edge('pf_vy0',   'out', 'pf_vyTerm', 'a'),
      edge('pf_phase', 'out', 'pf_vyTerm', 'b'),
      edge('pf_vyTerm','out', 'pf_pyBase', 'a'),
      edge('pf_gravTerm','out','pf_pyBase','b'),
      edge('pf_pyBase','out', 'pf_py', 'a'),
      // alpha + hue
      edge('pf_phase', 'out', 'pf_tNorm', 'a'),
      edge('pf_tNorm', 'out', 'pf_alphaFade', 'a'),
      edge('pf_alphaFade', 'out', 'pf_alpha', 'a'),
      edge(RID, 'seed', 'pf_hue', 'a'),
      edge('pf_hue',   'out', 'pf_hsl', 'h'),
      edge('pf_alpha', 'out', 'pf_hsl', 'a'),
      edge('pf_hsl',   'out', 'pf_fill', 'color'),
      // draw
      edge('pf_px',  'out', 'pf_dot', 'x'),
      edge('pf_py',  'out', 'pf_dot', 'y'),
      edge('pf_fill','out', 'pf_dot', 'style'),
      edge('pf_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'pf_renderer', 'layers'),
    ],
  };
}

// ─── Sample: Movers in a Noise Field ──────────────────────────────────────
// 60 dots each drift around a fixed "home" position via Perlin noise.
// The home positions scatter randomly across the canvas; per-dot
// Perlin wanderers nudge them around. Each particle is stateless —
// position = home + wanderer2d(time, seed). NOC §1 — building motion
// from noise.
function buildMoversInNoise() {
  const N = 60;
  const seeds = nodeWithPorts('mn_seeds', 'spread/range', { x: -560, y:  60 }, { count: N });
  const time  = node('mn_time', 'time/seconds', { x: -560, y: 140 });

  const RID = 'mn_region';
  const region: FloretNode = {
    id: RID,
    type: 'floret',
    position: { x: -340, y: 60 },
    data: {
      defType: 'region/foreach',
      params: { width: 820, height: 380, label: 'ForEach Mover' },
      // Only seed iterates — time is broadcast in directly. See the
      // Particle Fountain comment for why a `t` port would cap iter to 1.
      ports: [
        { id: 'seed', name: 'seed', type: 'number', direction: 'input'  },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 820, height: 380 },
  };

  // Per-mover home position: random in [-200, 200], seeded.
  const homeX = node('mn_homeX', 'random/uniform', { x:  40, y:  40 }, { min: -200, max: 200 }, RID);
  const homeY = node('mn_homeY', 'random/uniform', { x:  40, y: 100 }, { min: -200, max: 200 }, RID);
  // Drift via wanderer2d(t, speed, area, seed).
  const wander = node('mn_wander', 'random/wanderer2d', { x: 220, y:  60 },
                      { speed: 0.3, area: 60 }, RID);
  const wSplit = node('mn_wSplit', 'vec2/split', { x: 400, y:  60 }, {}, RID);
  // pos = home + wanderer
  const px = node('mn_px', 'math/add', { x: 560, y:  40 }, {}, RID);
  const py = node('mn_py', 'math/add', { x: 560, y: 100 }, {}, RID);

  // Hue by seed, light alpha for a soft swarm look.
  const hue = node('mn_hue', 'math/mul', { x: 40, y: 200 }, { b: 6 }, RID);
  const hsl = node('mn_hsl', 'color/hsl', { x: 220, y: 200 }, { s: 75, l: 60, a: 220 }, RID);
  const fill = node('mn_fill','style/fill', { x: 400, y: 200 }, { mode: 'color' }, RID);
  // Use the wanderer's bearing to tweak dot size a little — anything
  // works here; this just shows that the drift is non-trivial.
  const dot = node('mn_dot', 'draw/ellipse', { x: 700, y: 100 }, { w: 9, h: 9 }, RID);

  const renderer = node('mn_renderer', 'render/renderer', { x: 540, y: 60 },
                        { width: 500, height: 500, background: '#070710' });

  return {
    nodes: [
      seeds, time, region,
      homeX, homeY, wander, wSplit, px, py,
      hue, hsl, fill, dot, renderer,
    ],
    edges: [
      edge('mn_seeds', 'out', RID, 'seed'),

      edge(RID, 'seed', 'mn_homeX', 'seed'),
      edge(RID, 'seed', 'mn_homeY', 'seed'),
      // Wanderer per-particle: seed shifts so each mover has its own trajectory.
      // Time broadcasts in from the outer scope.
      edge('mn_time', 't',    'mn_wander', 't'),
      edge(RID, 'seed', 'mn_wander', 'seed'),
      edge('mn_wander', 'out', 'mn_wSplit', 'v'),
      edge('mn_homeX',  'out', 'mn_px', 'a'),
      edge('mn_wSplit', 'x',   'mn_px', 'b'),
      edge('mn_homeY',  'out', 'mn_py', 'a'),
      edge('mn_wSplit', 'y',   'mn_py', 'b'),

      edge(RID, 'seed', 'mn_hue', 'a'),
      edge('mn_hue', 'out', 'mn_hsl', 'h'),
      edge('mn_hsl', 'out', 'mn_fill', 'color'),

      edge('mn_px',  'out', 'mn_dot', 'x'),
      edge('mn_py',  'out', 'mn_dot', 'y'),
      edge('mn_fill','out', 'mn_dot', 'style'),
      edge('mn_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'mn_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 0 — Perlin Noise Walker ───────────────────────────────────────
// A 60-dot trail drifting smoothly via wanderer2d, each dot at a slightly
// different time offset so they form a comet-like trail behind the head.
// Pure function of time — no state. NOC §0 — building motion from noise.
function buildNocRandomWalker() {
  const N = 60;
  const time = node('rwk_time', 'time/seconds', { x: -440, y: 20 });
  const speedS = node('rwk_speed', 'io/slider', { x: -440, y: 100 }, { value: 0.4, min: 0.05, max: 2 });
  const areaS = node('rwk_area', 'io/slider', { x: -440, y: 180 }, { value: 200, min: 50, max: 240 });
  const seedS = node('rwk_seed', 'io/slider', { x: -440, y: 260 }, { value: 7, min: 1, max: 99 });
  const idxs = node('rwk_idxs', 'spread/range', { x: -440, y: 340 }, { count: N });

  const RID = 'rwk_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -160, y: 20 },
    data: { defType: 'region/foreach',
      params: { width: 800, height: 400, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'p', name: 'p', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 800, height: 400 },
  };
  // tOffset = i * -0.05  → newest dot at i=0 has tOffset 0, oldest trails behind.
  const tOffMul = node('rwk_tOffMul', 'math/mul', { x: 40, y: 40 }, { b: -0.05 }, RID);
  const tSamp = node('rwk_tSamp', 'math/add', { x: 220, y: 40 }, {}, RID);
  const wanderer = node('rwk_wanderer', 'random/wanderer2d', { x: 380, y: 60 }, {}, RID);
  const split = node('rwk_split', 'vec2/split', { x: 540, y: 60 }, {}, RID);
  // Size & alpha fade with trail age — older = smaller / more transparent.
  const sizeMul = node('rwk_sizeMul', 'math/mul', { x: 40, y: 140 }, { b: -0.32 }, RID);
  const sizeAdd = node('rwk_sizeAdd', 'math/add', { x: 220, y: 140 }, { b: 22 }, RID);
  const alpMul = node('rwk_alpMul', 'math/mul', { x: 40, y: 220 }, { b: -4 }, RID);
  const alpAdd = node('rwk_alpAdd', 'math/add', { x: 220, y: 220 }, { b: 255 }, RID);
  // Warm hue with slight per-dot variation.
  const hueMul = node('rwk_hueMul', 'math/mul', { x: 40, y: 300 }, { b: 1.5 }, RID);
  const hueAdd = node('rwk_hueAdd', 'math/add', { x: 220, y: 300 }, { b: 45 }, RID);
  const hsl = node('rwk_hsl', 'color/hsl', { x: 380, y: 300 }, { s: 80, l: 60 }, RID);
  const fill = node('rwk_fill', 'style/fill', { x: 540, y: 300 }, { mode: 'color' }, RID);
  const dot = node('rwk_dot', 'draw/ellipse', { x: 700, y: 200 }, {}, RID);

  const bg = node('rwk_bg', 'color/rgb', { x: -440, y: 400 }, { r: 10, g: 10, b: 18, a: 255 });
  const renderer = node('rwk_renderer', 'render/renderer', { x: 700, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [time, speedS, areaS, seedS, idxs, region, tOffMul, tSamp, wanderer, split,
            sizeMul, sizeAdd, alpMul, alpAdd, hueMul, hueAdd, hsl, fill, dot, bg, renderer],
    edges: [
      edge('rwk_bg', 'out', 'rwk_renderer', 'background'),
      edge('rwk_idxs', 'out', RID, 'i'),
      // Sampled time = time + i * -0.05.
      edge(RID, 'i', 'rwk_tOffMul', 'a'),
      edge('rwk_tOffMul', 'out', 'rwk_tSamp', 'a'),
      edge('rwk_time', 't', 'rwk_tSamp', 'b'),
      // Wanderer per-dot.
      edge('rwk_tSamp', 'out', 'rwk_wanderer', 't'),
      edge('rwk_speed', 'out', 'rwk_wanderer', 'speed'),
      edge('rwk_area', 'out', 'rwk_wanderer', 'area'),
      edge('rwk_seed', 'out', 'rwk_wanderer', 'seed'),
      edge('rwk_wanderer', 'out', 'rwk_split', 'v'),
      // Size: 22 - i*0.32 → 22 down to ~3.
      edge(RID, 'i', 'rwk_sizeMul', 'a'),
      edge('rwk_sizeMul', 'out', 'rwk_sizeAdd', 'a'),
      // Alpha: 255 - i*4 → 255 down to ~15.
      edge(RID, 'i', 'rwk_alpMul', 'a'),
      edge('rwk_alpMul', 'out', 'rwk_alpAdd', 'a'),
      // Hue: 45 + i*1.5 → warm shift along trail.
      edge(RID, 'i', 'rwk_hueMul', 'a'),
      edge('rwk_hueMul', 'out', 'rwk_hueAdd', 'a'),
      edge('rwk_hueAdd', 'out', 'rwk_hsl', 'h'),
      edge('rwk_alpAdd', 'out', 'rwk_hsl', 'a'),
      edge('rwk_hsl', 'out', 'rwk_fill', 'color'),
      // Dot.
      edge('rwk_split', 'x', 'rwk_dot', 'x'),
      edge('rwk_split', 'y', 'rwk_dot', 'y'),
      edge('rwk_sizeAdd', 'out', 'rwk_dot', 'w'),
      edge('rwk_sizeAdd', 'out', 'rwk_dot', 'h'),
      edge('rwk_fill', 'out', 'rwk_dot', 'style'),
      edge('rwk_dot', 'out', RID, 'p'),
      edge(RID, 'p', 'rwk_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 0 — Gaussian Distribution Cloud ───────────────────────────────
// 1500 points scattered around the canvas centre via random/gaussian.
// Stateless — pure function of the per-point seed index. Inside the
// foreach, we use `i` for the x-gaussian seed and `i + 9999` for the
// y-gaussian seed so the two axes are decorrelated.
function buildNocGaussian() {
  const N = 1500;
  const sd = node('gau_sd', 'io/slider', { x: -440, y: 20 }, { value: 70, min: 10, max: 200 });
  const seeds = node('gau_xSeeds', 'spread/range', { x: -440, y: 100 }, { count: N });

  const RID = 'gau_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -60, y: 20 },
    data: { defType: 'region/foreach', params: { width: 700, height: 360, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 700, height: 360 },
  };
  // Per-point seeds. y-seed = i + 9999 so it's a different stream from x.
  const ySeed = node('gau_ySeed', 'math/add', { x: 60, y: 60 }, { b: 9999 }, RID);
  const gx = node('gau_gx', 'random/gaussian', { x: 220, y: 40 }, { mean: 0 }, RID);
  const gy = node('gau_gy', 'random/gaussian', { x: 220, y: 120 }, { mean: 0 }, RID);
  const fill = node('gau_fill', 'style/fill', { x: 60, y: 200 }, { mode: 'rgb', r: 255, g: 200, b: 120, a: 90 });
  const dot = node('gau_dot', 'draw/ellipse', { x: 380, y: 80 }, { w: 4, h: 4 }, RID);
  const bg = node('gau_bg', 'color/rgb', { x: -440, y: 260 }, { r: 10, g: 8, b: 14, a: 255 });
  const renderer = node('gau_renderer', 'render/renderer', { x: 760, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [sd, seeds, region, ySeed, gx, gy, fill, dot, bg, renderer],
    edges: [
      edge('gau_bg', 'out', 'gau_renderer', 'background'),
      edge('gau_xSeeds', 'out', RID, 'i'),
      edge(RID, 'i', 'gau_gx', 'seed'),
      edge(RID, 'i', 'gau_ySeed', 'a'),
      edge('gau_ySeed', 'out', 'gau_gy', 'seed'),
      edge('gau_sd', 'out', 'gau_gx', 'sd'),
      edge('gau_sd', 'out', 'gau_gy', 'sd'),
      edge('gau_gx', 'out', 'gau_dot', 'x'),
      edge('gau_gy', 'out', 'gau_dot', 'y'),
      edge('gau_fill', 'out', 'gau_dot', 'style'),
      edge('gau_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'gau_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 1 — Bouncing Ball with Vectors ────────────────────────────────
// One ball with vec2 position & velocity. Bounces off all four walls.
// State<vec2> for pos and vel. NOC §1 — vectors made tangible.
function buildNocBouncingBall() {
  const initPos = node('bb_initPos', 'vec2/make', { x: -400, y: 20 }, { x: -200, y: -150 });
  const initVel = node('bb_initVel', 'vec2/make', { x: -400, y: 100 }, { x: 2.5, y: 2 });
  const bounds = node('bb_bounds', 'value/number', { x: -400, y: 180 }, { value: 230 });

  const pos = node('bb_pos', 'state/value', { x: -200, y: 20 });
  const vel = node('bb_vel', 'state/value', { x: -200, y: 100 });
  const next = node('bb_next', 'vec2/add', { x: 0, y: 60 });
  const ns = node('bb_ns', 'vec2/split', { x: 160, y: 60 });
  const vs = node('bb_vs', 'vec2/split', { x: 160, y: 140 });
  // X bounce: |nx| > bounds → flip vx
  const absX = node('bb_absX', 'math/abs', { x: 320, y: 40 });
  const hitX = node('bb_hitX', 'logic/gt', { x: 480, y: 40 });
  const vxNeg = node('bb_vxNeg', 'math/neg', { x: 320, y: 140 });
  const vxNext = node('bb_vxNext', 'logic/select', { x: 640, y: 100 });
  // Y bounce
  const absY = node('bb_absY', 'math/abs', { x: 320, y: 80 });
  const hitY = node('bb_hitY', 'logic/gt', { x: 480, y: 80 });
  const vyNeg = node('bb_vyNeg', 'math/neg', { x: 320, y: 180 });
  const vyNext = node('bb_vyNext', 'logic/select', { x: 640, y: 180 });
  const vNext = node('bb_vNext', 'vec2/make', { x: 800, y: 140 });

  const fill = node('bb_fill', 'style/fill', { x: -200, y: 260 }, { mode: 'rgb', r: 230, g: 130, b: 80, a: 255 });
  const split = node('bb_split', 'vec2/split', { x: 0, y: 260 });
  const ball = node('bb_ball', 'draw/ellipse', { x: 160, y: 260 }, { w: 36, h: 36 });

  const bg = node('bb_bg', 'color/rgb', { x: -400, y: 320 }, { r: 12, g: 12, b: 22, a: 255 });
  const renderer = node('bb_renderer', 'render/renderer', { x: 480, y: 260 }, { width: 500, height: 500 });
  return {
    nodes: [initPos, initVel, bounds, pos, vel, next, ns, vs, absX, hitX, vxNeg, vxNext,
            absY, hitY, vyNeg, vyNext, vNext, fill, split, ball, bg, renderer],
    edges: [
      edge('bb_bg', 'out', 'bb_renderer', 'background'),
      edge('bb_initPos', 'out', 'bb_pos', 'init'),
      edge('bb_initVel', 'out', 'bb_vel', 'init'),
      // next = pos + vel
      edge('bb_pos', 'current', 'bb_next', 'a'),
      edge('bb_vel', 'current', 'bb_next', 'b'),
      edge('bb_next', 'out', 'bb_ns', 'v'),
      edge('bb_vel', 'current', 'bb_vs', 'v'),
      // X
      edge('bb_ns', 'x', 'bb_absX', 'x'),
      edge('bb_absX', 'out', 'bb_hitX', 'a'),
      edge('bb_bounds', 'out', 'bb_hitX', 'b'),
      edge('bb_vs', 'x', 'bb_vxNeg', 'x'),
      edge('bb_hitX', 'out', 'bb_vxNext', 'cond'),
      edge('bb_vxNeg', 'out', 'bb_vxNext', 'a'),
      edge('bb_vs', 'x', 'bb_vxNext', 'b'),
      // Y
      edge('bb_ns', 'y', 'bb_absY', 'x'),
      edge('bb_absY', 'out', 'bb_hitY', 'a'),
      edge('bb_bounds', 'out', 'bb_hitY', 'b'),
      edge('bb_vs', 'y', 'bb_vyNeg', 'x'),
      edge('bb_hitY', 'out', 'bb_vyNext', 'cond'),
      edge('bb_vyNeg', 'out', 'bb_vyNext', 'a'),
      edge('bb_vs', 'y', 'bb_vyNext', 'b'),
      // reassemble velocity
      edge('bb_vxNext', 'out', 'bb_vNext', 'x'),
      edge('bb_vyNext', 'out', 'bb_vNext', 'y'),
      // write back
      edge('bb_next', 'out', 'bb_pos', 'next'),
      edge('bb_vNext', 'out', 'bb_vel', 'next'),
      // draw
      edge('bb_pos', 'current', 'bb_split', 'v'),
      edge('bb_split', 'x', 'bb_ball', 'x'),
      edge('bb_split', 'y', 'bb_ball', 'y'),
      edge('bb_fill', 'out', 'bb_ball', 'style'),
      edge('bb_ball', 'out', 'bb_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 1 — Acceleration Toward Mouse ─────────────────────────────────
// A mover accelerates toward the mouse pointer at a constant magnitude.
// State<vec2> for position and velocity. The classic "follow the cursor".
function buildNocAccelMouse() {
  const mouse = node('am_mouse', 'input/mouse', { x: -440, y: 20 });
  const maxSpeed = node('am_maxSpeed', 'io/slider', { x: -440, y: 100 }, { value: 7, min: 1, max: 20 });
  const accMag = node('am_accMag', 'io/slider', { x: -440, y: 180 }, { value: 0.2, min: 0.01, max: 0.5 });

  const initPos = node('am_initPos', 'vec2/make', { x: -440, y: 260 }, { x: 0, y: 0 });
  const initVel = node('am_initVel', 'vec2/make', { x: -440, y: 340 }, { x: 0, y: 0 });
  const pos = node('am_pos', 'state/value', { x: -220, y: 60 });
  const vel = node('am_vel', 'state/value', { x: -220, y: 140 });

  // acc = normalize(mouse - pos) * accMag
  const mouseVec = node('am_mouseVec', 'vec2/make', { x: -220, y: 220 });
  const diff = node('am_diff', 'vec2/sub', { x: 0, y: 100 });
  const dir = node('am_dir', 'vec2/normalize', { x: 160, y: 100 });
  const acc = node('am_acc', 'vec2/scale', { x: 320, y: 100 });
  // vel_next = limit(vel + acc, maxSpeed)
  const velAdd = node('am_velAdd', 'vec2/add', { x: 480, y: 140 });
  const velLim = node('am_velLim', 'vec2/limit', { x: 640, y: 140 });
  // pos_next = pos + vel_next
  const posAdd = node('am_posAdd', 'vec2/add', { x: 800, y: 60 });

  const fill = node('am_fill', 'style/fill', { x: -220, y: 380 }, { mode: 'rgb', r: 120, g: 220, b: 240, a: 255 });
  const split = node('am_split', 'vec2/split', { x: 0, y: 380 });
  const ball = node('am_ball', 'draw/ellipse', { x: 160, y: 380 }, { w: 28, h: 28 });

  const bg = node('am_bg', 'color/rgb', { x: -440, y: 440 }, { r: 12, g: 14, b: 22, a: 255 });
  const renderer = node('am_renderer', 'render/renderer', { x: 540, y: 380 }, { width: 500, height: 500 });
  return {
    nodes: [mouse, maxSpeed, accMag, initPos, initVel, pos, vel, mouseVec, diff, dir, acc,
            velAdd, velLim, posAdd, fill, split, ball, bg, renderer],
    edges: [
      edge('am_bg', 'out', 'am_renderer', 'background'),
      edge('am_initPos', 'out', 'am_pos', 'init'),
      edge('am_initVel', 'out', 'am_vel', 'init'),
      edge('am_mouse', 'x', 'am_mouseVec', 'x'),
      edge('am_mouse', 'y', 'am_mouseVec', 'y'),
      // diff = mouseVec - pos
      edge('am_mouseVec', 'out', 'am_diff', 'a'),
      edge('am_pos', 'current', 'am_diff', 'b'),
      edge('am_diff', 'out', 'am_dir', 'v'),
      edge('am_dir', 'out', 'am_acc', 'v'),
      edge('am_accMag', 'out', 'am_acc', 's'),
      // vel + acc
      edge('am_vel', 'current', 'am_velAdd', 'a'),
      edge('am_acc', 'out', 'am_velAdd', 'b'),
      edge('am_velAdd', 'out', 'am_velLim', 'v'),
      edge('am_maxSpeed', 'out', 'am_velLim', 'max'),
      // pos + vel_next
      edge('am_pos', 'current', 'am_posAdd', 'a'),
      edge('am_velLim', 'out', 'am_posAdd', 'b'),
      // write
      edge('am_posAdd', 'out', 'am_pos', 'next'),
      edge('am_velLim', 'out', 'am_vel', 'next'),
      // draw
      edge('am_pos', 'current', 'am_split', 'v'),
      edge('am_split', 'x', 'am_ball', 'x'),
      edge('am_split', 'y', 'am_ball', 'y'),
      edge('am_fill', 'out', 'am_ball', 'style'),
      edge('am_ball', 'out', 'am_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 2 — Wind & Gravity Forces ─────────────────────────────────────
// Mouse-X-driven wind plus constant gravity push a ball around.
// F = m·a → acc = (wind + gravity·m)/m = wind/m + gravity. State<vec2>.
function buildNocWindGravity() {
  const mouse = node('wg_mouse', 'input/mouse', { x: -440, y: 20 });
  const massS = node('wg_mass', 'io/slider', { x: -440, y: 100 }, { value: 1, min: 0.5, max: 3 });
  // Wind = (mouseX / 250) * 0.6  — left = blow left, right = blow right.
  const windMul = node('wg_windMul', 'math/mul', { x: -240, y: 20 }, { b: 0.003 });
  const windVec = node('wg_windVec', 'vec2/make', { x: -80, y: 20 });
  const gravity = node('wg_gravity', 'vec2/make', { x: -240, y: 100 }, { x: 0, y: 0.2 });

  const initPos = node('wg_initPos', 'vec2/make', { x: -440, y: 180 }, { x: 0, y: -150 });
  const initVel = node('wg_initVel', 'vec2/make', { x: -440, y: 260 }, { x: 0, y: 0 });
  const pos = node('wg_pos', 'state/value', { x: -240, y: 200 });
  const vel = node('wg_vel', 'state/value', { x: -240, y: 280 });

  // wind / mass
  const wScaled = node('wg_wScaled', 'vec2/scale', { x: 80, y: 20 });
  const invMass = node('wg_invMass', 'math/div', { x: 80, y: 80 }, { a: 1 });
  // acc = wind/mass + gravity
  const accAdd = node('wg_accAdd', 'vec2/add', { x: 240, y: 60 });
  // vel + acc
  const velAdd = node('wg_velAdd', 'vec2/add', { x: 400, y: 200 });
  // pos + vel
  const posAdd = node('wg_posAdd', 'vec2/add', { x: 560, y: 200 });

  // Wall bounce (all four)
  const ps = node('wg_ps', 'vec2/split', { x: 720, y: 200 });
  const vs = node('wg_vs', 'vec2/split', { x: 720, y: 280 });
  const bounds = node('wg_bounds', 'value/number', { x: -440, y: 340 }, { value: 230 });
  const absX = node('wg_absX', 'math/abs', { x: 880, y: 160 });
  const hitX = node('wg_hitX', 'logic/gt', { x: 1040, y: 160 });
  const absY = node('wg_absY', 'math/abs', { x: 880, y: 220 });
  const hitY = node('wg_hitY', 'logic/gt', { x: 1040, y: 220 });
  const vxNeg = node('wg_vxNeg', 'math/neg', { x: 880, y: 280 });
  const vyNeg = node('wg_vyNeg', 'math/neg', { x: 880, y: 340 });
  const vxOut = node('wg_vxOut', 'logic/select', { x: 1200, y: 280 });
  const vyOut = node('wg_vyOut', 'logic/select', { x: 1200, y: 340 });
  const vFinal = node('wg_vFinal', 'vec2/make', { x: 1360, y: 280 });

  const fill = node('wg_fill', 'style/fill', { x: -240, y: 380 }, { mode: 'rgb', r: 200, g: 220, b: 90, a: 255 });
  const psDraw = node('wg_psDraw', 'vec2/split', { x: -80, y: 380 });
  const ball = node('wg_ball', 'draw/ellipse', { x: 80, y: 380 }, { w: 36, h: 36 });
  const sizeMul = node('wg_sizeMul', 'math/mul', { x: -240, y: 460 }, { b: 24 });
  const bg = node('wg_bg', 'color/rgb', { x: -440, y: 420 }, { r: 14, g: 14, b: 24, a: 255 });
  const renderer = node('wg_renderer', 'render/renderer', { x: 240, y: 380 }, { width: 500, height: 500 });
  return {
    nodes: [mouse, massS, windMul, windVec, gravity, initPos, initVel, pos, vel,
            wScaled, invMass, accAdd, velAdd, posAdd, ps, vs, bounds,
            absX, hitX, absY, hitY, vxNeg, vyNeg, vxOut, vyOut, vFinal,
            fill, psDraw, ball, sizeMul, bg, renderer],
    edges: [
      edge('wg_bg', 'out', 'wg_renderer', 'background'),
      edge('wg_initPos', 'out', 'wg_pos', 'init'),
      edge('wg_initVel', 'out', 'wg_vel', 'init'),
      // wind vector from mouseX
      edge('wg_mouse', 'x', 'wg_windMul', 'a'),
      edge('wg_windMul', 'out', 'wg_windVec', 'x'),
      // wind / mass
      edge('wg_windVec', 'out', 'wg_wScaled', 'v'),
      edge('wg_mass', 'out', 'wg_invMass', 'b'),
      edge('wg_invMass', 'out', 'wg_wScaled', 's'),
      // acc = wind/m + g
      edge('wg_wScaled', 'out', 'wg_accAdd', 'a'),
      edge('wg_gravity', 'out', 'wg_accAdd', 'b'),
      // vel_next = vel + acc
      edge('wg_vel', 'current', 'wg_velAdd', 'a'),
      edge('wg_accAdd', 'out', 'wg_velAdd', 'b'),
      // pos_tmp = pos + vel_next
      edge('wg_pos', 'current', 'wg_posAdd', 'a'),
      edge('wg_velAdd', 'out', 'wg_posAdd', 'b'),
      // bounce
      edge('wg_posAdd', 'out', 'wg_ps', 'v'),
      edge('wg_velAdd', 'out', 'wg_vs', 'v'),
      edge('wg_ps', 'x', 'wg_absX', 'x'),
      edge('wg_ps', 'y', 'wg_absY', 'x'),
      edge('wg_absX', 'out', 'wg_hitX', 'a'),
      edge('wg_bounds', 'out', 'wg_hitX', 'b'),
      edge('wg_absY', 'out', 'wg_hitY', 'a'),
      edge('wg_bounds', 'out', 'wg_hitY', 'b'),
      edge('wg_vs', 'x', 'wg_vxNeg', 'x'),
      edge('wg_vs', 'y', 'wg_vyNeg', 'x'),
      edge('wg_hitX', 'out', 'wg_vxOut', 'cond'),
      edge('wg_vxNeg', 'out', 'wg_vxOut', 'a'),
      edge('wg_vs', 'x', 'wg_vxOut', 'b'),
      edge('wg_hitY', 'out', 'wg_vyOut', 'cond'),
      edge('wg_vyNeg', 'out', 'wg_vyOut', 'a'),
      edge('wg_vs', 'y', 'wg_vyOut', 'b'),
      edge('wg_vxOut', 'out', 'wg_vFinal', 'x'),
      edge('wg_vyOut', 'out', 'wg_vFinal', 'y'),
      edge('wg_posAdd', 'out', 'wg_pos', 'next'),
      edge('wg_vFinal', 'out', 'wg_vel', 'next'),
      // draw ball — size proportional to mass
      edge('wg_pos', 'current', 'wg_psDraw', 'v'),
      edge('wg_psDraw', 'x', 'wg_ball', 'x'),
      edge('wg_psDraw', 'y', 'wg_ball', 'y'),
      edge('wg_mass', 'out', 'wg_sizeMul', 'a'),
      edge('wg_sizeMul', 'out', 'wg_ball', 'w'),
      edge('wg_sizeMul', 'out', 'wg_ball', 'h'),
      edge('wg_fill', 'out', 'wg_ball', 'style'),
      edge('wg_ball', 'out', 'wg_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 2 — Friction ──────────────────────────────────────────────────
// A ball sliding right with friction opposing its velocity, gravity pulling
// down, walls bouncing it back. Friction slider tunes how quickly it stops.
function buildNocFriction() {
  const muS = node('fr_mu', 'io/slider', { x: -440, y: 20 }, { value: 0.06, min: 0.0, max: 0.2 });
  const gravity = node('fr_gravity', 'vec2/make', { x: -440, y: 100 }, { x: 0, y: 0.25 });
  const initPos = node('fr_initPos', 'vec2/make', { x: -440, y: 180 }, { x: -220, y: -200 });
  const initVel = node('fr_initVel', 'vec2/make', { x: -440, y: 260 }, { x: 5, y: 0 });
  const bounds = node('fr_bounds', 'value/number', { x: -440, y: 340 }, { value: 230 });

  const pos = node('fr_pos', 'state/value', { x: -240, y: 60 });
  const vel = node('fr_vel', 'state/value', { x: -240, y: 140 });

  // friction = -normalize(vel) * mu
  const frDir = node('fr_dir', 'vec2/normalize', { x: -80, y: 140 });
  const frNeg = node('fr_negMu', 'math/neg', { x: -240, y: 220 });
  const frForce = node('fr_force', 'vec2/scale', { x: 80, y: 140 });
  // acc = gravity + friction
  const acc = node('fr_acc', 'vec2/add', { x: 240, y: 100 });
  const velAdd = node('fr_velAdd', 'vec2/add', { x: 400, y: 140 });
  const posAdd = node('fr_posAdd', 'vec2/add', { x: 560, y: 60 });

  // bounce both axes
  const ps = node('fr_ps', 'vec2/split', { x: 720, y: 60 });
  const vs = node('fr_vs', 'vec2/split', { x: 720, y: 140 });
  const absX = node('fr_absX', 'math/abs', { x: 880, y: 40 });
  const hitX = node('fr_hitX', 'logic/gt', { x: 1040, y: 40 });
  const absY = node('fr_absY', 'math/abs', { x: 880, y: 100 });
  const hitY = node('fr_hitY', 'logic/gt', { x: 1040, y: 100 });
  const vxN = node('fr_vxN', 'math/neg', { x: 880, y: 160 });
  const vyN = node('fr_vyN', 'math/neg', { x: 880, y: 220 });
  const vxOut = node('fr_vxOut', 'logic/select', { x: 1200, y: 160 });
  const vyOut = node('fr_vyOut', 'logic/select', { x: 1200, y: 220 });
  const vFin = node('fr_vFin', 'vec2/make', { x: 1360, y: 160 });

  const fill = node('fr_fill', 'style/fill', { x: -240, y: 320 }, { mode: 'rgb', r: 240, g: 180, b: 80, a: 255 });
  const psDraw = node('fr_psDraw', 'vec2/split', { x: -80, y: 320 });
  const ball = node('fr_ball', 'draw/ellipse', { x: 80, y: 320 }, { w: 30, h: 30 });
  const bg = node('fr_bg', 'color/rgb', { x: -440, y: 420 }, { r: 12, g: 14, b: 22, a: 255 });
  const renderer = node('fr_renderer', 'render/renderer', { x: 240, y: 320 }, { width: 500, height: 500 });
  return {
    nodes: [muS, gravity, initPos, initVel, bounds, pos, vel, frDir, frNeg, frForce,
            acc, velAdd, posAdd, ps, vs, absX, hitX, absY, hitY, vxN, vyN, vxOut, vyOut, vFin,
            fill, psDraw, ball, bg, renderer],
    edges: [
      edge('fr_bg', 'out', 'fr_renderer', 'background'),
      edge('fr_initPos', 'out', 'fr_pos', 'init'),
      edge('fr_initVel', 'out', 'fr_vel', 'init'),
      // -mu * dir(vel)
      edge('fr_vel', 'current', 'fr_dir', 'v'),
      edge('fr_mu', 'out', 'fr_negMu', 'x'),
      edge('fr_dir', 'out', 'fr_force', 'v'),
      edge('fr_negMu', 'out', 'fr_force', 's'),
      // acc = g + friction
      edge('fr_gravity', 'out', 'fr_acc', 'a'),
      edge('fr_force', 'out', 'fr_acc', 'b'),
      edge('fr_vel', 'current', 'fr_velAdd', 'a'),
      edge('fr_acc', 'out', 'fr_velAdd', 'b'),
      edge('fr_pos', 'current', 'fr_posAdd', 'a'),
      edge('fr_velAdd', 'out', 'fr_posAdd', 'b'),
      edge('fr_posAdd', 'out', 'fr_ps', 'v'),
      edge('fr_velAdd', 'out', 'fr_vs', 'v'),
      edge('fr_ps', 'x', 'fr_absX', 'x'),
      edge('fr_ps', 'y', 'fr_absY', 'x'),
      edge('fr_absX', 'out', 'fr_hitX', 'a'),
      edge('fr_bounds', 'out', 'fr_hitX', 'b'),
      edge('fr_absY', 'out', 'fr_hitY', 'a'),
      edge('fr_bounds', 'out', 'fr_hitY', 'b'),
      edge('fr_vs', 'x', 'fr_vxN', 'x'),
      edge('fr_vs', 'y', 'fr_vyN', 'x'),
      edge('fr_hitX', 'out', 'fr_vxOut', 'cond'),
      edge('fr_vxN', 'out', 'fr_vxOut', 'a'),
      edge('fr_vs', 'x', 'fr_vxOut', 'b'),
      edge('fr_hitY', 'out', 'fr_vyOut', 'cond'),
      edge('fr_vyN', 'out', 'fr_vyOut', 'a'),
      edge('fr_vs', 'y', 'fr_vyOut', 'b'),
      edge('fr_vxOut', 'out', 'fr_vFin', 'x'),
      edge('fr_vyOut', 'out', 'fr_vFin', 'y'),
      edge('fr_posAdd', 'out', 'fr_pos', 'next'),
      edge('fr_vFin', 'out', 'fr_vel', 'next'),
      edge('fr_pos', 'current', 'fr_psDraw', 'v'),
      edge('fr_psDraw', 'x', 'fr_ball', 'x'),
      edge('fr_psDraw', 'y', 'fr_ball', 'y'),
      edge('fr_fill', 'out', 'fr_ball', 'style'),
      edge('fr_ball', 'out', 'fr_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 2 — Drag / Fluid Resistance ───────────────────────────────────
// Gravity pulls the ball down; quadratic drag −c·|v|·v opposes motion
// across the whole canvas. The terminal-velocity hover is the visual:
// the ball accelerates then settles at a constant fall speed.
function buildNocDrag() {
  const c = node('dr_c', 'io/slider', { x: -440, y: 20 }, { value: 0.02, min: 0, max: 0.1 });
  const gravity = node('dr_gravity', 'vec2/make', { x: -440, y: 100 }, { x: 0, y: 0.4 });
  const initPos = node('dr_initPos', 'vec2/make', { x: -440, y: 180 }, { x: 0, y: -220 });
  const initVel = node('dr_initVel', 'vec2/make', { x: -440, y: 260 }, { x: 0, y: 0 });
  const bounds = node('dr_bounds', 'value/number', { x: -440, y: 340 }, { value: 220 });
  const pos = node('dr_pos', 'state/value', { x: -240, y: 60 });
  const vel = node('dr_vel', 'state/value', { x: -240, y: 140 });

  // drag = scale(v, -c*|v|)
  const vMag = node('dr_vMag', 'vec2/mag', { x: -80, y: 140 });
  const cv = node('dr_cv', 'math/mul', { x: 80, y: 140 });
  const cvN = node('dr_cvN', 'math/neg', { x: 240, y: 140 });
  const drag = node('dr_drag', 'vec2/scale', { x: 400, y: 140 });

  // acc = g + drag
  const acc = node('dr_acc', 'vec2/add', { x: 560, y: 100 });
  const velAdd = node('dr_velAdd', 'vec2/add', { x: 720, y: 140 });
  const posAdd = node('dr_posAdd', 'vec2/add', { x: 880, y: 60 });

  // floor bounce
  const psN = node('dr_psN', 'vec2/split', { x: 1040, y: 60 });
  const hitFloor = node('dr_hitFloor', 'logic/gt', { x: 1200, y: 60 });
  const yClamp = node('dr_yClamp', 'logic/select', { x: 1360, y: 60 });
  const posFin = node('dr_posFin', 'vec2/make', { x: 1520, y: 60 });
  const vsFin = node('dr_vsFin', 'vec2/split', { x: 1040, y: 140 });
  const vyN2 = node('dr_vyN2', 'math/neg', { x: 1200, y: 140 });
  const vyDamp = node('dr_vyDamp', 'math/mul', { x: 1360, y: 140 }, { b: 0.7 });
  const vyOut2 = node('dr_vyOut2', 'logic/select', { x: 1520, y: 140 });
  const velFin = node('dr_velFin', 'vec2/make', { x: 1680, y: 140 });

  // visual: tinted water rect for atmosphere
  const waterFill = node('dr_waterFill', 'style/fill', { x: -440, y: 400 }, { mode: 'rgb', r: 40, g: 60, b: 120, a: 90 });
  const water = node('dr_water', 'draw/rect', { x: -240, y: 400 }, { x: -250, y: -250, w: 500, h: 500 });
  const ballFill = node('dr_ballFill', 'style/fill', { x: -240, y: 460 }, { mode: 'rgb', r: 240, g: 180, b: 80, a: 255 });
  const psDraw = node('dr_psDraw', 'vec2/split', { x: -80, y: 460 });
  const ball = node('dr_ball', 'draw/ellipse', { x: 80, y: 460 }, { w: 28, h: 28 });
  const grp = node('dr_grp', 'transform/group', { x: 320, y: 400 }, { slotCount: 2 });

  const bg = node('dr_bg', 'color/rgb', { x: -440, y: 520 }, { r: 12, g: 14, b: 22, a: 255 });
  const renderer = node('dr_renderer', 'render/renderer', { x: 560, y: 400 }, { width: 500, height: 500 });
  return {
    nodes: [c, gravity, initPos, initVel, bounds, pos, vel, vMag, cv, cvN, drag,
            acc, velAdd, posAdd, psN, hitFloor, yClamp, posFin, vsFin, vyN2, vyDamp, vyOut2, velFin,
            waterFill, water, ballFill, psDraw, ball, grp, bg, renderer],
    edges: [
      edge('dr_bg', 'out', 'dr_renderer', 'background'),
      edge('dr_initPos', 'out', 'dr_pos', 'init'),
      edge('dr_initVel', 'out', 'dr_vel', 'init'),
      edge('dr_vel', 'current', 'dr_vMag', 'v'),
      edge('dr_vMag', 'out', 'dr_cv', 'a'),
      edge('dr_c', 'out', 'dr_cv', 'b'),
      edge('dr_cv', 'out', 'dr_cvN', 'x'),
      edge('dr_vel', 'current', 'dr_drag', 'v'),
      edge('dr_cvN', 'out', 'dr_drag', 's'),
      edge('dr_gravity', 'out', 'dr_acc', 'a'),
      edge('dr_drag', 'out', 'dr_acc', 'b'),
      edge('dr_vel', 'current', 'dr_velAdd', 'a'),
      edge('dr_acc', 'out', 'dr_velAdd', 'b'),
      edge('dr_pos', 'current', 'dr_posAdd', 'a'),
      edge('dr_velAdd', 'out', 'dr_posAdd', 'b'),
      edge('dr_posAdd', 'out', 'dr_psN', 'v'),
      edge('dr_psN', 'y', 'dr_hitFloor', 'a'),
      edge('dr_bounds', 'out', 'dr_hitFloor', 'b'),
      edge('dr_hitFloor', 'out', 'dr_yClamp', 'cond'),
      edge('dr_bounds', 'out', 'dr_yClamp', 'a'),
      edge('dr_psN', 'y', 'dr_yClamp', 'b'),
      edge('dr_psN', 'x', 'dr_posFin', 'x'),
      edge('dr_yClamp', 'out', 'dr_posFin', 'y'),
      edge('dr_velAdd', 'out', 'dr_vsFin', 'v'),
      edge('dr_vsFin', 'y', 'dr_vyN2', 'x'),
      edge('dr_vyN2', 'out', 'dr_vyDamp', 'a'),
      edge('dr_hitFloor', 'out', 'dr_vyOut2', 'cond'),
      edge('dr_vyDamp', 'out', 'dr_vyOut2', 'a'),
      edge('dr_vsFin', 'y', 'dr_vyOut2', 'b'),
      edge('dr_vsFin', 'x', 'dr_velFin', 'x'),
      edge('dr_vyOut2', 'out', 'dr_velFin', 'y'),
      edge('dr_posFin', 'out', 'dr_pos', 'next'),
      edge('dr_velFin', 'out', 'dr_vel', 'next'),
      edge('dr_waterFill', 'out', 'dr_water', 'style'),
      edge('dr_pos', 'current', 'dr_psDraw', 'v'),
      edge('dr_psDraw', 'x', 'dr_ball', 'x'),
      edge('dr_psDraw', 'y', 'dr_ball', 'y'),
      edge('dr_ballFill', 'out', 'dr_ball', 'style'),
      edge('dr_water', 'out', 'dr_grp', 's1'),
      edge('dr_ball', 'out', 'dr_grp', 's2'),
      edge('dr_grp', 'out', 'dr_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 2 — Mouse Attraction ──────────────────────────────────────────
// One mover is pulled toward the mouse via inverse-square-ish gravity.
// Distance clamped to avoid blow-ups. State<vec2> for pos & vel.
function buildNocAttraction() {
  const mouse = node('at_mouse', 'input/mouse', { x: -440, y: 20 });
  const G = node('at_G', 'io/slider', { x: -440, y: 100 }, { value: 800, min: 50, max: 4000 });
  const damp = node('at_damp', 'io/slider', { x: -440, y: 180 }, { value: 0.985, min: 0.9, max: 1 });

  const initPos = node('at_initPos', 'vec2/make', { x: -440, y: 260 }, { x: -200, y: -200 });
  const initVel = node('at_initVel', 'vec2/make', { x: -440, y: 340 }, { x: 3, y: 0 });
  const pos = node('at_pos', 'state/value', { x: -240, y: 60 });
  const vel = node('at_vel', 'state/value', { x: -240, y: 140 });

  // diff = mouse - pos
  const mv = node('at_mv', 'vec2/make', { x: -240, y: 220 });
  const diff = node('at_diff', 'vec2/sub', { x: -80, y: 100 });
  const dist = node('at_dist', 'vec2/mag', { x: 80, y: 100 });
  const distSq = node('at_distSq', 'math/sq', { x: 240, y: 100 });
  const distClamp = node('at_distClamp', 'math/clamp', { x: 400, y: 100 }, { min: 100, max: 40000 });
  const F = node('at_F', 'math/div', { x: 560, y: 100 });
  // acc = normalize(diff) * F
  const dir = node('at_dir', 'vec2/normalize', { x: 80, y: 180 });
  const acc = node('at_acc', 'vec2/scale', { x: 720, y: 140 });
  // vel = (vel + acc) * damp
  const velAdd = node('at_velAdd', 'vec2/add', { x: 880, y: 140 });
  const velDamp = node('at_velDamp', 'vec2/scale', { x: 1040, y: 140 });
  const posAdd = node('at_posAdd', 'vec2/add', { x: 1200, y: 60 });

  const fill = node('at_fill', 'style/fill', { x: -240, y: 320 }, { mode: 'rgb', r: 240, g: 90, b: 130, a: 255 });
  const psDraw = node('at_psDraw', 'vec2/split', { x: -80, y: 320 });
  const ball = node('at_ball', 'draw/ellipse', { x: 80, y: 320 }, { w: 28, h: 28 });
  // Mouse marker
  const mFill = node('at_mFill', 'style/fill', { x: -240, y: 400 }, { mode: 'rgb', r: 90, g: 220, b: 240, a: 200 });
  const mMark = node('at_mMark', 'draw/ellipse', { x: 80, y: 400 }, { w: 16, h: 16 });
  const grp = node('at_grp', 'transform/group', { x: 240, y: 360 }, { slotCount: 2 });
  const bg = node('at_bg', 'color/rgb', { x: -440, y: 460 }, { r: 8, g: 8, b: 16, a: 255 });
  const renderer = node('at_renderer', 'render/renderer', { x: 480, y: 360 }, { width: 500, height: 500 });
  return {
    nodes: [mouse, G, damp, initPos, initVel, pos, vel, mv, diff, dist, distSq, distClamp,
            F, dir, acc, velAdd, velDamp, posAdd, fill, psDraw, ball, mFill, mMark, grp, bg, renderer],
    edges: [
      edge('at_bg', 'out', 'at_renderer', 'background'),
      edge('at_initPos', 'out', 'at_pos', 'init'),
      edge('at_initVel', 'out', 'at_vel', 'init'),
      edge('at_mouse', 'x', 'at_mv', 'x'),
      edge('at_mouse', 'y', 'at_mv', 'y'),
      edge('at_mv', 'out', 'at_diff', 'a'),
      edge('at_pos', 'current', 'at_diff', 'b'),
      edge('at_diff', 'out', 'at_dist', 'v'),
      edge('at_dist', 'out', 'at_distSq', 'x'),
      edge('at_distSq', 'out', 'at_distClamp', 'x'),
      edge('at_G', 'out', 'at_F', 'a'),
      edge('at_distClamp', 'out', 'at_F', 'b'),
      edge('at_diff', 'out', 'at_dir', 'v'),
      edge('at_dir', 'out', 'at_acc', 'v'),
      edge('at_F', 'out', 'at_acc', 's'),
      edge('at_vel', 'current', 'at_velAdd', 'a'),
      edge('at_acc', 'out', 'at_velAdd', 'b'),
      edge('at_velAdd', 'out', 'at_velDamp', 'v'),
      edge('at_damp', 'out', 'at_velDamp', 's'),
      edge('at_pos', 'current', 'at_posAdd', 'a'),
      edge('at_velDamp', 'out', 'at_posAdd', 'b'),
      edge('at_posAdd', 'out', 'at_pos', 'next'),
      edge('at_velDamp', 'out', 'at_vel', 'next'),
      edge('at_pos', 'current', 'at_psDraw', 'v'),
      edge('at_psDraw', 'x', 'at_ball', 'x'),
      edge('at_psDraw', 'y', 'at_ball', 'y'),
      edge('at_fill', 'out', 'at_ball', 'style'),
      edge('at_mouse', 'x', 'at_mMark', 'x'),
      edge('at_mouse', 'y', 'at_mMark', 'y'),
      edge('at_mFill', 'out', 'at_mMark', 'style'),
      edge('at_mMark', 'out', 'at_grp', 's1'),
      edge('at_ball', 'out', 'at_grp', 's2'),
      edge('at_grp', 'out', 'at_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 3 — Static Sine Wave ──────────────────────────────────────────
// A horizontal row of dots oscillating vertically. y_i = sin(i·dx + time)·amp.
function buildNocSineWave() {
  const N = 80;
  const time = node('sw_time', 'time/seconds', { x: -440, y: 20 });
  const ampS = node('sw_amp', 'io/slider', { x: -440, y: 100 }, { value: 120, min: 20, max: 220 });
  const periodS = node('sw_period', 'io/slider', { x: -440, y: 180 }, { value: 0.12, min: 0.02, max: 0.6 });
  const speedS = node('sw_speed', 'io/slider', { x: -440, y: 260 }, { value: 3, min: 0, max: 8 });
  const idxs = node('sw_idxs', 'spread/range', { x: -440, y: 340 }, { count: N });
  const xStep = node('sw_xStep', 'value/number', { x: -440, y: 400 }, { value: 500 / N });
  const xOffset = node('sw_xOffset', 'value/number', { x: -440, y: 460 }, { value: -250 });

  const RID = 'sw_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -100, y: 20 },
    data: { defType: 'region/foreach',
      params: { width: 760, height: 460, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 760, height: 460 },
  };
  // angle = i*period + time*speed
  const iPer = node('sw_iPer', 'math/mul', { x: 60, y: 60 }, {}, RID);
  const tSpd = node('sw_tSpd', 'math/mul', { x: 60, y: 120 }, {}, RID);
  const ang = node('sw_ang', 'math/add', { x: 220, y: 80 }, {}, RID);
  const sinA = node('sw_sinA', 'math/sin', { x: 380, y: 80 }, {}, RID);
  const y = node('sw_y', 'math/mul', { x: 540, y: 80 }, {}, RID);
  // x = i*xStep + xOffset
  const xMul = node('sw_xMul', 'math/mul', { x: 60, y: 200 }, {}, RID);
  const x = node('sw_x', 'math/add', { x: 220, y: 200 }, {}, RID);
  // hue cycles along the wave
  const hue = node('sw_hue', 'math/mul', { x: 60, y: 280 }, { b: 360 / N }, RID);
  const hsl = node('sw_hsl', 'color/hsl', { x: 220, y: 280 }, { s: 80, l: 60, a: 255 }, RID);
  const fill = node('sw_fill', 'style/fill', { x: 380, y: 280 }, { mode: 'color' }, RID);
  const dot = node('sw_dot', 'draw/ellipse', { x: 540, y: 200 }, { w: 18, h: 18 }, RID);

  const bg = node('sw_bg', 'color/rgb', { x: -440, y: 520 }, { r: 10, g: 12, b: 22, a: 255 });
  const renderer = node('sw_renderer', 'render/renderer', { x: 760, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [time, ampS, periodS, speedS, idxs, xStep, xOffset, region,
            iPer, tSpd, ang, sinA, y, xMul, x, hue, hsl, fill, dot, bg, renderer],
    edges: [
      edge('sw_bg', 'out', 'sw_renderer', 'background'),
      edge('sw_idxs', 'out', RID, 'i'),
      edge(RID, 'i', 'sw_iPer', 'a'),
      edge('sw_period', 'out', 'sw_iPer', 'b'),
      edge('sw_time', 't', 'sw_tSpd', 'a'),
      edge('sw_speed', 'out', 'sw_tSpd', 'b'),
      edge('sw_iPer', 'out', 'sw_ang', 'a'),
      edge('sw_tSpd', 'out', 'sw_ang', 'b'),
      edge('sw_ang', 'out', 'sw_sinA', 'x'),
      edge('sw_sinA', 'out', 'sw_y', 'a'),
      edge('sw_amp', 'out', 'sw_y', 'b'),
      edge(RID, 'i', 'sw_xMul', 'a'),
      edge('sw_xStep', 'out', 'sw_xMul', 'b'),
      edge('sw_xMul', 'out', 'sw_x', 'a'),
      edge('sw_xOffset', 'out', 'sw_x', 'b'),
      edge(RID, 'i', 'sw_hue', 'a'),
      edge('sw_hue', 'out', 'sw_hsl', 'h'),
      edge('sw_hsl', 'out', 'sw_fill', 'color'),
      edge('sw_x', 'out', 'sw_dot', 'x'),
      edge('sw_y', 'out', 'sw_dot', 'y'),
      edge('sw_fill', 'out', 'sw_dot', 'style'),
      edge('sw_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'sw_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 3 — Additive Wave ─────────────────────────────────────────────
// Sum of two sine waves of different period & amplitude. Stateless.
function buildNocAdditiveWave() {
  const N = 100;
  const time = node('aw_time', 'time/seconds', { x: -440, y: 20 });
  const a1 = node('aw_a1', 'io/slider', { x: -440, y: 100 }, { value: 70, min: 0, max: 150 });
  const p1 = node('aw_p1', 'io/slider', { x: -440, y: 180 }, { value: 0.10, min: 0.02, max: 0.4 });
  const a2 = node('aw_a2', 'io/slider', { x: -440, y: 260 }, { value: 50, min: 0, max: 150 });
  const p2 = node('aw_p2', 'io/slider', { x: -440, y: 340 }, { value: 0.28, min: 0.02, max: 0.6 });
  const idxs = node('aw_idxs', 'spread/range', { x: -440, y: 420 }, { count: N });
  const xStep = node('aw_xStep', 'value/number', { x: -440, y: 480 }, { value: 500 / N });
  const xOffset = node('aw_xOffset', 'value/number', { x: -440, y: 540 }, { value: -250 });

  const RID = 'aw_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -100, y: 20 },
    data: { defType: 'region/foreach',
      params: { width: 800, height: 520, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 800, height: 520 },
  };
  // angle1, sin1*a1
  const ang1 = node('aw_ang1', 'math/mul', { x: 60, y: 40 }, {}, RID);
  const ang1t = node('aw_ang1t', 'math/add', { x: 220, y: 40 }, {}, RID);
  const sin1 = node('aw_sin1', 'math/sin', { x: 380, y: 40 }, {}, RID);
  const y1 = node('aw_y1', 'math/mul', { x: 540, y: 40 }, {}, RID);
  const ang2 = node('aw_ang2', 'math/mul', { x: 60, y: 120 }, {}, RID);
  const ang2t = node('aw_ang2t', 'math/add', { x: 220, y: 120 }, {}, RID);
  const sin2 = node('aw_sin2', 'math/sin', { x: 380, y: 120 }, {}, RID);
  const y2 = node('aw_y2', 'math/mul', { x: 540, y: 120 }, {}, RID);
  const ySum = node('aw_ySum', 'math/add', { x: 700, y: 80 }, {}, RID);

  const xMul = node('aw_xMul', 'math/mul', { x: 60, y: 220 }, {}, RID);
  const x = node('aw_x', 'math/add', { x: 220, y: 220 }, {}, RID);
  const hue = node('aw_hue', 'math/mul', { x: 60, y: 300 }, { b: 360 / N }, RID);
  const hsl = node('aw_hsl', 'color/hsl', { x: 220, y: 300 }, { s: 85, l: 60, a: 255 }, RID);
  const fill = node('aw_fill', 'style/fill', { x: 380, y: 300 }, { mode: 'color' }, RID);
  const dot = node('aw_dot', 'draw/ellipse', { x: 700, y: 220 }, { w: 16, h: 16 }, RID);

  const bg = node('aw_bg', 'color/rgb', { x: -440, y: 600 }, { r: 10, g: 10, b: 22, a: 255 });
  const renderer = node('aw_renderer', 'render/renderer', { x: 760, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [time, a1, p1, a2, p2, idxs, xStep, xOffset, region,
            ang1, ang1t, sin1, y1, ang2, ang2t, sin2, y2, ySum,
            xMul, x, hue, hsl, fill, dot, bg, renderer],
    edges: [
      edge('aw_bg', 'out', 'aw_renderer', 'background'),
      edge('aw_idxs', 'out', RID, 'i'),
      edge(RID, 'i', 'aw_ang1', 'a'),
      edge('aw_p1', 'out', 'aw_ang1', 'b'),
      edge('aw_ang1', 'out', 'aw_ang1t', 'a'),
      edge('aw_time', 't', 'aw_ang1t', 'b'),
      edge('aw_ang1t', 'out', 'aw_sin1', 'x'),
      edge('aw_sin1', 'out', 'aw_y1', 'a'),
      edge('aw_a1', 'out', 'aw_y1', 'b'),
      edge(RID, 'i', 'aw_ang2', 'a'),
      edge('aw_p2', 'out', 'aw_ang2', 'b'),
      edge('aw_ang2', 'out', 'aw_ang2t', 'a'),
      edge('aw_time', 't', 'aw_ang2t', 'b'),
      edge('aw_ang2t', 'out', 'aw_sin2', 'x'),
      edge('aw_sin2', 'out', 'aw_y2', 'a'),
      edge('aw_a2', 'out', 'aw_y2', 'b'),
      edge('aw_y1', 'out', 'aw_ySum', 'a'),
      edge('aw_y2', 'out', 'aw_ySum', 'b'),
      edge(RID, 'i', 'aw_xMul', 'a'),
      edge('aw_xStep', 'out', 'aw_xMul', 'b'),
      edge('aw_xMul', 'out', 'aw_x', 'a'),
      edge('aw_xOffset', 'out', 'aw_x', 'b'),
      edge(RID, 'i', 'aw_hue', 'a'),
      edge('aw_hue', 'out', 'aw_hsl', 'h'),
      edge('aw_hsl', 'out', 'aw_fill', 'color'),
      edge('aw_x', 'out', 'aw_dot', 'x'),
      edge('aw_ySum', 'out', 'aw_dot', 'y'),
      edge('aw_fill', 'out', 'aw_dot', 'style'),
      edge('aw_dot', 'out', RID, 'layer'),
      edge(RID, 'layer', 'aw_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 3 — Spring (Hooke's Law) ──────────────────────────────────────
// A bob hanging from a spring: F = -k*(y - restY), with gravity + damping.
// State<number> for bobY and vy. Drag/release with mouse for fun? No —
// for simplicity, just let it oscillate from a perturbed start.
function buildNocSpring() {
  const k = node('spr_k', 'io/slider', { x: -440, y: 20 }, { value: 0.12, min: 0.02, max: 0.4 });
  const dampS = node('spr_damp', 'io/slider', { x: -440, y: 100 }, { value: 0.98, min: 0.9, max: 1 });
  const gravity = node('spr_gravity', 'value/number', { x: -440, y: 180 }, { value: 0.5 });
  const restY = node('spr_restY', 'value/number', { x: -440, y: 240 }, { value: 0 });
  const initY = node('spr_initY', 'value/number', { x: -440, y: 300 }, { value: -180 });
  const initVy = node('spr_initVy', 'value/number', { x: -440, y: 360 }, { value: 0 });

  const y = node('spr_y', 'state/value', { x: -220, y: 100 });
  const vy = node('spr_vy', 'state/value', { x: -220, y: 180 });

  // F = -k * (y - restY)
  const stretch = node('spr_stretch', 'math/sub', { x: -60, y: 60 });
  const kx = node('spr_kx', 'math/mul', { x: 100, y: 60 });
  const Fspring = node('spr_Fspring', 'math/neg', { x: 260, y: 60 });
  // acc = F + gravity
  const acc = node('spr_acc', 'math/add', { x: 420, y: 100 });
  // vy_next = (vy + acc) * damp
  const vyAdd = node('spr_vyAdd', 'math/add', { x: 580, y: 140 });
  const vyDamp = node('spr_vyDamp', 'math/mul', { x: 740, y: 140 });
  // y_next = y + vy_next
  const yNext = node('spr_yNext', 'math/add', { x: 900, y: 100 });

  // Anchor at (0, -240). Rod = line from anchor down to bob's y.
  const anchorY = node('spr_anchorY', 'value/number', { x: -440, y: 440 }, { value: -240 });
  const strokeC = node('spr_strokeC', 'color/rgb', { x: -440, y: 500 }, { r: 200, g: 200, b: 200, a: 200 });
  const stroke = node('spr_stroke', 'style/stroke', { x: -260, y: 500 });
  const rod = node('spr_rod', 'draw/line', { x: -80, y: 500 }, { x1: 0, x2: 0 });
  // Anchor disc + bob.
  const anchorFill = node('spr_anchorFill', 'style/fill', { x: -440, y: 580 }, { mode: 'rgb', r: 180, g: 180, b: 180, a: 255 });
  const anchorDisc = node('spr_anchor', 'draw/ellipse', { x: -260, y: 580 }, { x: 0, w: 14, h: 14 });
  const fill = node('spr_fill', 'style/fill', { x: -440, y: 660 }, { mode: 'rgb', r: 240, g: 130, b: 60, a: 255 });
  const bob = node('spr_bob', 'draw/ellipse', { x: -260, y: 660 }, { x: 0, w: 36, h: 36 });
  const grp = node('spr_grp', 'transform/group', { x: 100, y: 600 }, { slotCount: 3 });
  const bg = node('spr_bg', 'color/rgb', { x: -440, y: 720 }, { r: 10, g: 10, b: 18, a: 255 });
  const renderer = node('spr_renderer', 'render/renderer', { x: 440, y: 540 }, { width: 500, height: 500 });

  return {
    nodes: [k, dampS, gravity, restY, initY, initVy, y, vy, stretch, kx, Fspring, acc, vyAdd, vyDamp, yNext,
            anchorY, strokeC, stroke, rod, anchorFill, anchorDisc, fill, bob, grp, bg, renderer],
    edges: [
      edge('spr_bg', 'out', 'spr_renderer', 'background'),
      edge('spr_initY', 'out', 'spr_y', 'init'),
      edge('spr_initVy', 'out', 'spr_vy', 'init'),
      edge('spr_y', 'current', 'spr_stretch', 'a'),
      edge('spr_restY', 'out', 'spr_stretch', 'b'),
      edge('spr_stretch', 'out', 'spr_kx', 'a'),
      edge('spr_k', 'out', 'spr_kx', 'b'),
      edge('spr_kx', 'out', 'spr_Fspring', 'x'),
      edge('spr_Fspring', 'out', 'spr_acc', 'a'),
      edge('spr_gravity', 'out', 'spr_acc', 'b'),
      edge('spr_vy', 'current', 'spr_vyAdd', 'a'),
      edge('spr_acc', 'out', 'spr_vyAdd', 'b'),
      edge('spr_vyAdd', 'out', 'spr_vyDamp', 'a'),
      edge('spr_damp', 'out', 'spr_vyDamp', 'b'),
      edge('spr_y', 'current', 'spr_yNext', 'a'),
      edge('spr_vyDamp', 'out', 'spr_yNext', 'b'),
      edge('spr_yNext', 'out', 'spr_y', 'next'),
      edge('spr_vyDamp', 'out', 'spr_vy', 'next'),
      // Rod from anchor (0, -240) to bob (0, y_current).
      edge('spr_strokeC', 'out', 'spr_stroke', 'color'),
      edge('spr_stroke', 'out', 'spr_rod', 'style'),
      edge('spr_anchorY', 'out', 'spr_rod', 'y1'),
      edge('spr_y', 'current', 'spr_rod', 'y2'),
      // Anchor disc at (0, -240).
      edge('spr_anchorFill', 'out', 'spr_anchor', 'style'),
      edge('spr_anchorY', 'out', 'spr_anchor', 'y'),
      // Bob at (0, y_current).
      edge('spr_y', 'current', 'spr_bob', 'y'),
      edge('spr_fill', 'out', 'spr_bob', 'style'),
      // Group: rod, anchor, bob.
      edge('spr_rod', 'out', 'spr_grp', 's1'),
      edge('spr_anchor', 'out', 'spr_grp', 's2'),
      edge('spr_bob', 'out', 'spr_grp', 's3'),
      edge('spr_grp', 'out', 'spr_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 3 — Wave Clock ────────────────────────────────────────────────
// 16 hue-coded dots orbit the canvas centre. Static angles (i * TAU/N)
// with a single time-driven angular offset broadcast in. Mirrors the exact
// node-port arrangement of Lissajous Threads — the proven foreach + cos/sin
// pattern in this codebase.
function buildNocWaveClock() {
  const N = 16;
  const time = node('wc_time', 'time/seconds', { x: -380, y: 20 });
  const lenN = node('wc_len', 'value/number', { x: -380, y: 100 }, { value: 200 });
  const idxs = node('wc_idxs', 'spread/range', { x: -380, y: 180 }, { count: N });
  const stepN = node('wc_step', 'value/number', { x: -380, y: 240 }, { value: (Math.PI * 2) / N });

  const RID = 'wc_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -120, y: 20 },
    data: { defType: 'region/foreach',
      params: { width: 880, height: 460, label: 'ForEach' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'p', name: 'p', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 880, height: 460 },
  };

  // Inside: ang = i*step + t
  const angMul = node('wc_angMul', 'math/mul', { x: 40, y: 60 }, {}, RID);
  const angAdd = node('wc_angAdd', 'math/add', { x: 220, y: 60 }, {}, RID);
  // x = cos(ang)*len; y = sin(ang)*len
  const cosA = node('wc_cosA', 'math/cos', { x: 380, y: 40 }, {}, RID);
  const sinA = node('wc_sinA', 'math/sin', { x: 380, y: 100 }, {}, RID);
  const rX = node('wc_rX', 'math/mul', { x: 540, y: 40 }, {}, RID);
  const rY = node('wc_rY', 'math/mul', { x: 540, y: 100 }, {}, RID);
  // hue = i*step (just reuse step as hue scale)
  const hueMul = node('wc_hueMul', 'math/mul', { x: 40, y: 200 }, { b: 360 / N }, RID);
  const hsl = node('wc_hsl', 'color/hsl', { x: 220, y: 200 }, { s: 75, l: 60, a: 255 }, RID);
  const fill = node('wc_fill', 'style/fill', { x: 380, y: 200 }, { mode: 'color' }, RID);
  const dotSize = node('wc_dotSize', 'value/number', { x: 540, y: 200 }, { value: 26 });
  const dot = node('wc_dot', 'draw/ellipse', { x: 700, y: 100 }, {}, RID);

  const bg = node('wc_bg', 'color/rgb', { x: -380, y: 320 }, { r: 10, g: 12, b: 22, a: 255 });
  const renderer = node('wc_renderer', 'render/renderer', { x: 800, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [time, lenN, idxs, stepN, region, angMul, angAdd, cosA, sinA, rX, rY,
            hueMul, hsl, fill, dotSize, dot, bg, renderer],
    edges: [
      edge('wc_bg', 'out', 'wc_renderer', 'background'),
      edge('wc_idxs', 'out', RID, 'i'),
      edge(RID, 'i', 'wc_angMul', 'a'),
      edge('wc_step', 'out', 'wc_angMul', 'b'),
      edge('wc_angMul', 'out', 'wc_angAdd', 'a'),
      edge('wc_time', 't', 'wc_angAdd', 'b'),
      edge('wc_angAdd', 'out', 'wc_cosA', 'x'),
      edge('wc_angAdd', 'out', 'wc_sinA', 'x'),
      edge('wc_cosA', 'out', 'wc_rX', 'a'),
      edge('wc_len', 'out', 'wc_rX', 'b'),
      edge('wc_sinA', 'out', 'wc_rY', 'a'),
      edge('wc_len', 'out', 'wc_rY', 'b'),
      edge(RID, 'i', 'wc_hueMul', 'a'),
      edge('wc_hueMul', 'out', 'wc_hsl', 'h'),
      edge('wc_hsl', 'out', 'wc_fill', 'color'),
      edge('wc_rX', 'out', 'wc_dot', 'x'),
      edge('wc_rY', 'out', 'wc_dot', 'y'),
      edge('wc_dotSize', 'out', 'wc_dot', 'w'),
      edge('wc_dotSize', 'out', 'wc_dot', 'h'),
      edge('wc_fill', 'out', 'wc_dot', 'style'),
      edge('wc_dot', 'out', RID, 'p'),
      edge(RID, 'p', 'wc_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 5 — Seek ──────────────────────────────────────────────────────
// Vehicle steers toward the mouse using Reynolds' classic seek formula:
//   desired = normalize(target - pos) * maxSpeed
//   steer   = limit(desired - vel, maxForce)
//   vel    += steer
// State<vec2> for pos & vel.
function buildNocSeek() {
  const mouse = node('sk_mouse', 'input/mouse', { x: -440, y: 20 });
  const maxSpeed = node('sk_maxSpeed', 'io/slider', { x: -440, y: 100 }, { value: 6, min: 1, max: 15 });
  const maxForce = node('sk_maxForce', 'io/slider', { x: -440, y: 180 }, { value: 0.4, min: 0.05, max: 1.5 });
  const initPos = node('sk_initPos', 'vec2/make', { x: -440, y: 260 }, { x: -100, y: -100 });
  const initVel = node('sk_initVel', 'vec2/make', { x: -440, y: 340 }, { x: 0, y: 0 });

  const pos = node('sk_pos', 'state/value', { x: -220, y: 60 });
  const vel = node('sk_vel', 'state/value', { x: -220, y: 140 });

  const mv = node('sk_mv', 'vec2/make', { x: -220, y: 240 });
  // desired = normalize(target - pos) * maxSpeed
  const diff = node('sk_diff', 'vec2/sub', { x: -60, y: 100 });
  const dir = node('sk_dir', 'vec2/normalize', { x: 100, y: 100 });
  const desired = node('sk_desired', 'vec2/scale', { x: 260, y: 100 });
  // steer = limit(desired - vel, maxForce)
  const steerRaw = node('sk_steerRaw', 'vec2/sub', { x: 420, y: 140 });
  const steer = node('sk_steer', 'vec2/limit', { x: 580, y: 140 });
  // vel = vel + steer (clamped)
  const velAdd = node('sk_velAdd', 'vec2/add', { x: 740, y: 140 });
  const velCap = node('sk_velCap', 'vec2/limit', { x: 900, y: 140 });
  // pos = pos + vel
  const posAdd = node('sk_posAdd', 'vec2/add', { x: 1060, y: 60 });

  // Vehicle = triangle pointing in velocity direction.
  const head = node('sk_head', 'vec2/heading', { x: -60, y: 380 });
  // Group: dot at target, triangle at vehicle.
  const tFill = node('sk_tFill', 'style/fill', { x: -220, y: 420 }, { mode: 'rgb', r: 80, g: 200, b: 240, a: 255 });
  const target = node('sk_target', 'draw/ellipse', { x: -60, y: 460 }, { w: 18, h: 18 });
  const vFill = node('sk_vFill', 'style/fill', { x: -220, y: 500 }, { mode: 'rgb', r: 240, g: 200, b: 90, a: 255 });
  const splitP = node('sk_splitP', 'vec2/split', { x: -60, y: 540 });
  const vehicle = node('sk_vehicle', 'draw/ellipse', { x: 100, y: 540 }, { w: 22, h: 22 });
  const grp = node('sk_grp', 'transform/group', { x: 280, y: 480 }, { slotCount: 2 });

  const bg = node('sk_bg', 'color/rgb', { x: -440, y: 560 }, { r: 10, g: 10, b: 18, a: 255 });
  const renderer = node('sk_renderer', 'render/renderer', { x: 520, y: 480 }, { width: 500, height: 500 });
  return {
    nodes: [mouse, maxSpeed, maxForce, initPos, initVel, pos, vel, mv, diff, dir, desired,
            steerRaw, steer, velAdd, velCap, posAdd, head,
            tFill, target, vFill, splitP, vehicle, grp, bg, renderer],
    edges: [
      edge('sk_bg', 'out', 'sk_renderer', 'background'),
      edge('sk_initPos', 'out', 'sk_pos', 'init'),
      edge('sk_initVel', 'out', 'sk_vel', 'init'),
      edge('sk_mouse', 'x', 'sk_mv', 'x'),
      edge('sk_mouse', 'y', 'sk_mv', 'y'),
      edge('sk_mv', 'out', 'sk_diff', 'a'),
      edge('sk_pos', 'current', 'sk_diff', 'b'),
      edge('sk_diff', 'out', 'sk_dir', 'v'),
      edge('sk_dir', 'out', 'sk_desired', 'v'),
      edge('sk_maxSpeed', 'out', 'sk_desired', 's'),
      edge('sk_desired', 'out', 'sk_steerRaw', 'a'),
      edge('sk_vel', 'current', 'sk_steerRaw', 'b'),
      edge('sk_steerRaw', 'out', 'sk_steer', 'v'),
      edge('sk_maxForce', 'out', 'sk_steer', 'max'),
      edge('sk_vel', 'current', 'sk_velAdd', 'a'),
      edge('sk_steer', 'out', 'sk_velAdd', 'b'),
      edge('sk_velAdd', 'out', 'sk_velCap', 'v'),
      edge('sk_maxSpeed', 'out', 'sk_velCap', 'max'),
      edge('sk_pos', 'current', 'sk_posAdd', 'a'),
      edge('sk_velCap', 'out', 'sk_posAdd', 'b'),
      edge('sk_posAdd', 'out', 'sk_pos', 'next'),
      edge('sk_velCap', 'out', 'sk_vel', 'next'),
      // draw
      edge('sk_vel', 'current', 'sk_head', 'v'),
      edge('sk_mouse', 'x', 'sk_target', 'x'),
      edge('sk_mouse', 'y', 'sk_target', 'y'),
      edge('sk_tFill', 'out', 'sk_target', 'style'),
      edge('sk_pos', 'current', 'sk_splitP', 'v'),
      edge('sk_splitP', 'x', 'sk_vehicle', 'x'),
      edge('sk_splitP', 'y', 'sk_vehicle', 'y'),
      edge('sk_vFill', 'out', 'sk_vehicle', 'style'),
      edge('sk_target', 'out', 'sk_grp', 's1'),
      edge('sk_vehicle', 'out', 'sk_grp', 's2'),
      edge('sk_grp', 'out', 'sk_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 5 — Flow Field ────────────────────────────────────────────────
// A Perlin-driven flow field of arrows + many stateless dots that drift
// along it. Each dot's position is wanderer2d(time, seed) shifted by a
// random home — same as Movers but visualised over a vector field grid.
function buildNocFlowField() {
  const N = 16;        // grid resolution
  const time = node('flo_time', 'time/seconds', { x: -440, y: 20 });
  const noiseScale = node('flo_noiseScale', 'io/slider', { x: -440, y: 100 }, { value: 0.012, min: 0.002, max: 0.05 });
  const len = node('flo_len', 'value/number', { x: -440, y: 180 }, { value: 18 });
  const grid = node('flo_grid', 'spread/grid', { x: -440, y: 260 }, { cols: N, rows: N, w: 460, h: 460 });

  const RID = 'flo_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -120, y: 20 },
    data: { defType: 'region/foreach',
      params: { width: 880, height: 480, label: 'ForEach' },
      ports: [
        { id: 'p', name: 'p', type: 'vec2', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 880, height: 480 },
  };
  const split = node('flo_split', 'vec2/split', { x: 60, y: 60 }, {}, RID);
  // Perlin samples scaled by noiseScale (broadcast time into seed for drift)
  const xs = node('flo_xs', 'math/mul', { x: 220, y: 40 }, {}, RID);
  const ys = node('flo_ys', 'math/mul', { x: 220, y: 100 }, {}, RID);
  const yt = node('flo_yt', 'math/add', { x: 380, y: 100 }, {}, RID);
  const per = node('flo_per', 'noise/perlin', { x: 540, y: 60 }, { octaves: 2 }, RID);
  // angle = perlin * TAU * 2  (a couple of full rotations)
  const ang = node('flo_ang', 'math/mul', { x: 700, y: 60 }, { b: Math.PI * 2 * 2 }, RID);
  const fa = node('flo_fa', 'vec2/from-angle', { x: 60, y: 200 }, {}, RID);
  const fs = node('flo_fs', 'vec2/split', { x: 220, y: 200 }, {}, RID);
  // line endpoints (x, y) -> (x+dx, y+dy)
  const x2 = node('flo_x2', 'math/add', { x: 380, y: 200 }, {}, RID);
  const y2 = node('flo_y2', 'math/add', { x: 380, y: 260 }, {}, RID);
  // hue from angle
  const hueMul = node('flo_hueMul', 'math/mul', { x: 540, y: 320 }, { b: 50 }, RID);
  const hsl = node('flo_hsl', 'color/hsl', { x: 700, y: 320 }, { s: 70, l: 55, a: 220 }, RID);
  const strokeC = node('flo_strokeC', 'style/stroke', { x: 60, y: 320 }, {}, RID);
  const line = node('flo_line', 'draw/line', { x: 540, y: 200 }, {}, RID);

  const bg = node('flo_bg', 'color/rgb', { x: -440, y: 340 }, { r: 10, g: 10, b: 18, a: 255 });
  const renderer = node('flo_renderer', 'render/renderer', { x: 820, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [time, noiseScale, len, grid, region, split, xs, ys, yt, per, ang, fa, fs, x2, y2,
            hueMul, hsl, strokeC, line, bg, renderer],
    edges: [
      edge('flo_bg', 'out', 'flo_renderer', 'background'),
      edge('flo_grid', 'out', RID, 'p'),
      edge(RID, 'p', 'flo_split', 'v'),
      edge('flo_split', 'x', 'flo_xs', 'a'),
      edge('flo_noiseScale', 'out', 'flo_xs', 'b'),
      edge('flo_split', 'y', 'flo_ys', 'a'),
      edge('flo_noiseScale', 'out', 'flo_ys', 'b'),
      edge('flo_ys', 'out', 'flo_yt', 'a'),
      edge('flo_time', 't', 'flo_yt', 'b'),
      edge('flo_xs', 'out', 'flo_per', 'x'),
      edge('flo_yt', 'out', 'flo_per', 'y'),
      edge('flo_per', 'out', 'flo_ang', 'a'),
      edge('flo_ang', 'out', 'flo_fa', 'angle'),
      edge('flo_len', 'out', 'flo_fa', 'mag'),
      edge('flo_fa', 'out', 'flo_fs', 'v'),
      edge('flo_split', 'x', 'flo_x2', 'a'),
      edge('flo_fs', 'x', 'flo_x2', 'b'),
      edge('flo_split', 'y', 'flo_y2', 'a'),
      edge('flo_fs', 'y', 'flo_y2', 'b'),
      edge('flo_per', 'out', 'flo_hueMul', 'a'),
      edge('flo_hueMul', 'out', 'flo_hsl', 'h'),
      edge('flo_hsl', 'out', 'flo_strokeC', 'color'),
      edge('flo_strokeC', 'out', 'flo_line', 'style'),
      edge('flo_split', 'x', 'flo_line', 'x1'),
      edge('flo_split', 'y', 'flo_line', 'y1'),
      edge('flo_x2', 'out', 'flo_line', 'x2'),
      edge('flo_y2', 'out', 'flo_line', 'y2'),
      edge('flo_line', 'out', RID, 'layer'),
      edge(RID, 'layer', 'flo_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 7 — Wolfram Rule 90 (Sierpinski Triangle) ─────────────────────
// Accumulates 80 generations of a 1D CA into a flat boolean grid
// (COLS × ROWS), revealing the classic Sierpinski triangle pattern under
// rule 90 from a single seed cell. The grid lives in state<boolean*>;
// each frame `spread/last-row` extracts the most recent generation,
// `spread/elementary-ca` steps it, and `spread/scroll-up` shifts the
// grid + appends the new row. ForEach over cell indices renders each.
function buildNocRule90() {
  const COLS = 80;
  const ROWS = 80;
  const CELL_W = 6;
  const ruleS = node('r90_rule', 'io/slider', { x: -480, y: 20 }, { value: 90, min: 0, max: 255 });
  const colsN = node('r90_cols', 'value/number', { x: -480, y: 100 }, { value: COLS });
  const rowsN = node('r90_rows', 'value/number', { x: -480, y: 160 }, { value: ROWS });
  const idxN = node('r90_idxIn', 'value/number', { x: -480, y: 220 }, { value: Math.floor(COLS / 2) });
  // Seed grid: single live cell in the bottom row.
  const seed = node('r90_seed', 'spread/ca-grid-seed', { x: -280, y: 120 });
  // Read* on the left holds the history grid; chain steps it, Write* on the
  // right stashes the new grid for the next tick. Strict left-to-right flow.
  const read = node('r90_read', 'state/read-spread', { x: -80, y: 120 }, { slot: 1 });
  const lastRow = node('r90_lastRow', 'spread/last-row', { x: 120, y: 40 });
  const step = node('r90_step', 'spread/elementary-ca', { x: 320, y: 60 });
  const scroll = node('r90_scroll', 'spread/scroll-up', { x: 520, y: 120 });
  const write = node('r90_write', 'state/write-spread', { x: 720, y: 120 }, { slot: 1 });
  const idx = node('r90_idx', 'spread/range', { x: -480, y: 280 }, { count: COLS * ROWS });

  const RID = 'r90_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: 200, y: 280 },
    data: { defType: 'region/foreach',
      params: { width: 880, height: 400, label: 'ForEach Cell' },
      ports: [
        { id: 'i', name: 'i', type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 880, height: 400 },
  };
  // (col, row) = (i % COLS, floor(i / COLS))
  const colMod = node('r90_colMod', 'math/mod', { x: 60, y: 40 }, { b: COLS }, RID);
  const rowDiv = node('r90_rowDiv', 'math/div', { x: 60, y: 100 }, { b: COLS }, RID);
  const rowFloor = node('r90_rowFloor', 'math/floor', { x: 220, y: 100 }, {}, RID);
  // x = col * CELL_W - (COLS*CELL_W)/2 + CELL_W/2
  const xMul = node('r90_xMul', 'math/mul', { x: 220, y: 40 }, { b: CELL_W }, RID);
  const xPos = node('r90_xPos', 'math/add', { x: 380, y: 40 }, { b: -(COLS * CELL_W) / 2 + CELL_W / 2 }, RID);
  // y = row * CELL_W - (ROWS*CELL_W)/2 + CELL_W/2
  const yMul = node('r90_yMul', 'math/mul', { x: 380, y: 100 }, { b: CELL_W }, RID);
  const yPos = node('r90_yPos', 'math/add', { x: 540, y: 100 }, { b: -(ROWS * CELL_W) / 2 + CELL_W / 2 }, RID);
  // cell = grid[i]; size = cell * CELL_W
  const cell = node('r90_cell', 'spread/at', { x: 60, y: 180 }, {}, RID);
  const sz = node('r90_sz', 'math/mul', { x: 220, y: 180 }, { b: CELL_W }, RID);
  // Hue varies by row index for a gradient effect (newest rows hotter).
  const hueMul = node('r90_hueMul', 'math/mul', { x: 380, y: 180 }, { b: 360 / ROWS }, RID);
  const hsl = node('r90_hsl', 'color/hsl', { x: 540, y: 180 }, { s: 80, l: 60, a: 255 }, RID);
  const fill = node('r90_fill', 'style/fill', { x: 700, y: 180 }, { mode: 'color' }, RID);
  const rect = node('r90_rect', 'draw/rect', { x: 700, y: 60 }, {}, RID);

  const bg = node('r90_bg', 'color/rgb', { x: -480, y: 360 }, { r: 6, g: 6, b: 12, a: 255 });
  const renderer = node('r90_renderer', 'render/renderer', { x: 880, y: 20 }, { width: 500, height: 500 });
  return {
    nodes: [ruleS, colsN, rowsN, idxN, seed, read, lastRow, step, scroll, write, idx, region,
            colMod, rowDiv, rowFloor, xMul, xPos, yMul, yPos,
            cell, sz, hueMul, hsl, fill, rect, bg, renderer],
    edges: [
      edge('r90_bg', 'out', 'r90_renderer', 'background'),
      // Seed: cols × rows grid with one live cell in bottom row.
      edge('r90_cols', 'out', 'r90_seed', 'cols'),
      edge('r90_rows', 'out', 'r90_seed', 'rows'),
      edge('r90_idxIn', 'out', 'r90_seed', 'index'),
      edge('r90_seed', 'grid', 'r90_read', 'init'),
      // last-row → elementary-ca → scroll-up → Write*. Strict L→R.
      edge('r90_read', 'current', 'r90_lastRow', 'grid'),
      edge('r90_cols', 'out', 'r90_lastRow', 'cols'),
      edge('r90_lastRow', 'row', 'r90_step', 'row'),
      edge('r90_rule', 'out', 'r90_step', 'rule'),
      edge('r90_read', 'current', 'r90_scroll', 'grid'),
      edge('r90_step', 'next', 'r90_scroll', 'newRow'),
      edge('r90_cols', 'out', 'r90_scroll', 'cols'),
      edge('r90_scroll', 'grid', 'r90_write', 'next'),
      // ForEach over cell indices in the full grid.
      edge('r90_idx', 'out', RID, 'i'),
      // col, row from i
      edge(RID, 'i', 'r90_colMod', 'a'),
      edge(RID, 'i', 'r90_rowDiv', 'a'),
      edge('r90_rowDiv', 'out', 'r90_rowFloor', 'x'),
      // x, y from col, row
      edge('r90_colMod', 'out', 'r90_xMul', 'a'),
      edge('r90_xMul', 'out', 'r90_xPos', 'a'),
      edge('r90_rowFloor', 'out', 'r90_yMul', 'a'),
      edge('r90_yMul', 'out', 'r90_yPos', 'a'),
      // Render the FRESH grid (post-scroll), so visualisation matches state.
      edge('r90_scroll', 'grid', 'r90_cell', 'spread'),
      edge(RID, 'i', 'r90_cell', 'index'),
      edge('r90_cell', 'out', 'r90_sz', 'a'),
      // Hue from row.
      edge('r90_rowFloor', 'out', 'r90_hueMul', 'a'),
      edge('r90_hueMul', 'out', 'r90_hsl', 'h'),
      edge('r90_hsl', 'out', 'r90_fill', 'color'),
      // Rect.
      edge('r90_xPos', 'out', 'r90_rect', 'x'),
      edge('r90_yPos', 'out', 'r90_rect', 'y'),
      edge('r90_sz', 'out', 'r90_rect', 'w'),
      edge('r90_sz', 'out', 'r90_rect', 'h'),
      edge('r90_fill', 'out', 'r90_rect', 'style'),
      edge('r90_rect', 'out', RID, 'layer'),
      edge(RID, 'layer', 'r90_renderer', 'layers'),
    ],
  };
}

// ─── NOC Ch 8 — Fractal Tree ──────────────────────────────────────────────
// Recursively-generated tree, unrolled at build time into N draw/line
// nodes (one per branch). The classic binary-branching tree: each branch
// spawns two children at ±angle, length scaled by 0.7. 4 levels yields
// 1 + 2 + 4 + 8 = 15 branches. All branches share a single Style → Group →
// Renderer chain. Static — no swaying — to keep the structure crisp.
function buildNocFractalTree() {
  const LEVELS = 4;
  const ANGLE = 0.5;       // ~28.6° branch fan
  const SCALE = 0.7;       // length ratio per level
  const TRUNK_LEN = 140;
  const ROOT_Y = 220;      // canvas-bottom anchor

  type Branch = { x1: number; y1: number; x2: number; y2: number; level: number };
  const branches: Branch[] = [];
  function grow(x: number, y: number, angle: number, len: number, level: number) {
    if (level <= 0) return;
    const x2 = x + Math.sin(angle) * len;
    const y2 = y - Math.cos(angle) * len; // canvas +y is down, branches grow up
    branches.push({ x1: x, y1: y, x2, y2, level });
    grow(x2, y2, angle - ANGLE, len * SCALE, level - 1);
    grow(x2, y2, angle + ANGLE, len * SCALE, level - 1);
  }
  grow(0, ROOT_Y, 0, TRUNK_LEN, LEVELS);

  // Shared style: brown stroke, weighted by branch level (thicker near trunk).
  const strokeC = node('ft_strokeC', 'color/rgb', { x: -440, y: 20 }, { r: 230, g: 160, b: 80, a: 255 });
  const strokeStyle = node('ft_stroke', 'style/stroke', { x: -260, y: 20 });
  // Background — leafy dark green.
  const bg = node('ft_bg', 'color/rgb', { x: -440, y: 100 }, { r: 16, g: 22, b: 16, a: 255 });
  const renderer = node('ft_renderer', 'render/renderer', { x: 600, y: 20 }, { width: 500, height: 500 });

  // Per-branch line + stroke-weight chain. Weight = level (4 for trunk, 1 for tips).
  const nodes: FloretNode[] = [strokeC, strokeStyle, bg, renderer];
  const edges: FloretEdge[] = [
    edge('ft_bg', 'out', 'ft_renderer', 'background'),
    edge('ft_strokeC', 'out', 'ft_stroke', 'color'),
  ];
  const grp = node('ft_grp', 'transform/group', { x: 420, y: 20 }, { slotCount: branches.length });
  nodes.push(grp);
  edges.push(edge('ft_grp', 'out', 'ft_renderer', 'layers'));

  branches.forEach((b, i) => {
    const swId = `ft_sw_${i}`;
    const lnId = `ft_ln_${i}`;
    const xs = 80 + (i % 6) * 60;
    const ys = 200 + Math.floor(i / 6) * 80;
    const swN = node(swId, 'style/stroke-weight', { x: xs, y: ys }, { w: Math.max(1, b.level) });
    const lnN = node(lnId, 'draw/line', { x: xs + 180, y: ys },
                     { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2 });
    nodes.push(swN, lnN);
    edges.push(edge('ft_stroke', 'out', swId, 'base'));
    edges.push(edge(swId, 'out', lnId, 'style'));
    edges.push(edge(lnId, 'out', 'ft_grp', `s${i + 1}`));
  });

  return { nodes, edges };
}

// ─── Sample: Grid Art ─────────────────────────────────────────────────────
// 6×6 grid of stochastic walker-cluster cells. Each cell is independently
// seeded (seedBase slider + iteration index), so the 36 compositions all
// share the palette and motion language but vary in detail. A black-
// outlined border rect frames each cell. Port of a generative sketch by
// Sergi Calabuig — the heavy lifting (spawning walkers, stepping them
// 200× with decaying stroke weight, picking line-vs-circle drawing) all
// lives inside the draw/walker-cluster node.
function buildGridArt() {
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
  // Grid points are already centred on origin (−300..+300). The p5 driver
  // applies `translate(width/2, height/2)` before invoking each layer, so
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
      edge('ga_bg', 'out', 'ga_renderer', 'background'),
      // External → ForEach ports (spreads — drive the 36-iteration loop).
      edge('ga_grid', 'out', RID, 'p'),
      edge('ga_idx', 'out', RID, 'i'),
      // Inside: split the iteration's vec2 into x, y.
      edge(RID, 'p', 'ga_split', 'v'),
      // Per-cell seed = i + seedBase. Slider re-rolls all cells together.
      // Slider edge bypasses the region boundary as a broadcast.
      edge(RID, 'i', 'ga_seedSum', 'a'),
      edge('ga_seedBase', 'out', 'ga_seedSum', 'b'),
      // Walker — coords go straight through (driver centres the origin).
      edge('ga_split', 'x', 'ga_walker', 'x'),
      edge('ga_split', 'y', 'ga_walker', 'y'),
      edge('ga_seedSum', 'out', 'ga_walker', 'seed'),
      // Border style chain.
      edge('ga_noFill', 'out', 'ga_stroke', 'base'),
      edge('ga_stroke', 'out', 'ga_rectMode', 'base'),
      edge('ga_split', 'x', 'ga_border', 'x'),
      edge('ga_split', 'y', 'ga_border', 'y'),
      edge('ga_rectMode', 'out', 'ga_border', 'style'),
      // Group → ForEach output → Cache → Renderer. Runtime memoisation
      // keeps the upstream layer refs stable across ticks; Cache uses
      // that stability to skip rasterising and just blit the bitmap.
      edge('ga_walker', 'out', 'ga_group', 's1'),
      edge('ga_border', 'out', 'ga_group', 's2'),
      edge('ga_group', 'out', RID, 'layer'),
      edge(RID, 'layer', 'ga_cache', 'layer'),
      edge('ga_cache', 'out', 'ga_renderer', 'layers'),
    ],
  };
}

/**
 * Sample categories — order here is the order the tabs appear in the
 * Samples menu. The first non-empty category is selected by default.
 */
export const SAMPLE_CATEGORIES = [
  'Curves',
  'Fields',
  'Nature',
  'Image',
  'Pixel',
  'Sound',
  'Feedback',
  'Data',
  'Mixed',
] as const;

export const SAMPLES: Sample[] = [
  // ── Curves ─────────────────────────────────────────────
  {
    id: 'lissajous',
    name: 'Lissajous Threads',
    category: 'Curves',
    description: 'A 240-dot Lissajous curve. Two sliders pick the X/Y frequency ratio; time drifts the phase.',
    build: () => spaced(buildLissajous()),
  },
  {
    id: 'kaleidoscope',
    name: 'Kaleidoscope',
    category: 'Curves',
    description: 'A mouse-controlled triangle mirrored 12 times around the centre. Hue cycles with time.',
    build: () => spaced(buildKaleidoscope()),
  },
  {
    id: 'islamic-pattern',
    name: 'Islamic Pattern',
    category: 'Curves',
    description: '8-fold rotational symmetry. A 3-ellipse petal rotated around the centre 8 times; the petal radius breathes with sin(time).',
    build: () => spaced(buildIslamicPattern()),
  },
  {
    id: 'grid-art',
    name: 'Grid Art',
    category: 'Curves',
    description: '6×6 grid of stochastic walker-cluster cells, each independently seeded — 36 unique compositions sharing one palette. Slider re-rolls the whole grid. The walker algorithm (spawn, drift, tapering trails) lives inside draw/walker-cluster.',
    build: () => spaced(buildGridArt()),
  },

  // ── Fields ─────────────────────────────────────────────
  {
    id: 'ripple-field',
    name: 'Ripple Field',
    category: 'Fields',
    description: 'A 25×25 grid of dots, each pulsed by sin(dist_from_center − time). Concentric ripples.',
    build: () => spaced(buildRippleField()),
  },
  {
    id: 'starfield',
    name: 'Starfield Pulse',
    category: 'Fields',
    description: '80 randomly-placed stars twinkling out of phase. Two seeded randomize spreads + per-star phase.',
    build: () => spaced(buildStarfield()),
  },
  {
    id: 'noise-wave',
    name: 'Noise Wave',
    category: 'Fields',
    description: 'A 32×32 grid of cells displaced and hue-shifted by aggressive multi-octave Perlin noise. Pure function of time.',
    build: () => spaced(buildNoiseWave()),
  },
  {
    id: 'force-field',
    name: 'Force Field Arrows',
    category: 'Fields',
    description: 'A 14×14 grid of arrows pointing along a Perlin angle field that drifts with time. Each tip dot is hue-coded.',
    build: () => spaced(buildForceField()),
  },

  // ── Nature (Nature of Code) ────────────────────────────
  {
    id: 'gravity-drop',
    name: 'Gravity Drop',
    category: 'Nature',
    description: 'A ball falls under constant gravity and bounces off the floor with elastic damping. State<vec2> for position and velocity. NOC §2.',
    build: () => spaced(buildGravityDrop()),
  },
  {
    id: 'pendulum',
    name: 'Pendulum',
    category: 'Nature',
    description: 'Classic pendulum — angular acceleration = −(g/L)·sin(θ), velocity damped each tick. State<number> for θ and ω. NOC §3.',
    build: () => spaced(buildPendulum()),
  },
  {
    id: 'particle-fountain',
    name: 'Particle Fountain',
    category: 'Nature',
    description: '120 particles erupt upward with random burst angles, fall under gravity, then recycle when their lifetime expires. Each trajectory is a closed-form parabola in `phase` — no state required. ForEach drives the per-particle math. NOC §2 — Forces & motion.',
    build: () => spaced(buildParticleFountain()),
  },
  {
    id: 'movers-noise',
    name: 'Movers in a Noise Field',
    category: 'Nature',
    description: '60 dots drift around random home positions via per-particle Perlin wanderers. Stateless — each position is wanderer2d(time, seed) offset from a seeded home point. NOC §1 — motion from noise.',
    build: () => spaced(buildMoversInNoise()),
  },
  {
    id: 'noc-random-walker',
    name: 'Perlin Walker',
    category: 'Nature',
    description: 'A single dot drifts smoothly via random/wanderer2d — Perlin-based wandering. Stateless. NOC §0 — building motion from noise.',
    build: () => spaced(buildNocRandomWalker()),
  },
  {
    id: 'noc-gaussian',
    name: 'Gaussian Cloud',
    category: 'Nature',
    description: '1500 stateless dots scattered around the canvas centre by random/gaussian — bell-shaped density visible as a soft glowing core. Slider tunes the standard deviation. NOC §0.',
    build: () => spaced(buildNocGaussian()),
  },
  {
    id: 'noc-bouncing-ball',
    name: 'Bouncing Ball (Vectors)',
    category: 'Nature',
    description: 'One ball with vec2 position & velocity bounces off all four walls. State<vec2> for both pos and vel — the canonical NOC §1 introduction to vectors.',
    build: () => spaced(buildNocBouncingBall()),
  },
  {
    id: 'noc-accel-mouse',
    name: 'Acceleration Toward Mouse',
    category: 'Nature',
    description: 'A mover accelerates toward the mouse with constant magnitude. velocity = limit(vel + dir(mouse - pos)·a, maxSpeed). Two sliders tune accel and top speed. NOC §1.',
    build: () => spaced(buildNocAccelMouse()),
  },
  {
    id: 'noc-wind-gravity',
    name: 'Wind & Gravity',
    category: 'Nature',
    description: 'Mouse-X drives wind; gravity pulls down. F = m·a, so heavier masses respond less to wind. Slide the mass to feel the asymmetry. NOC §2 — forces.',
    build: () => spaced(buildNocWindGravity()),
  },
  {
    id: 'noc-friction',
    name: 'Friction',
    category: 'Nature',
    description: 'A ball slides under gravity with friction force −μ·normalize(v) opposing motion. Slider tunes μ. NOC §2.',
    build: () => spaced(buildNocFriction()),
  },
  {
    id: 'noc-drag',
    name: 'Drag',
    category: 'Nature',
    description: 'Quadratic fluid drag (−c·|v|·v) opposes motion as the ball falls. The terminal-velocity hover is the punchline — gravity and drag balance at constant speed. NOC §2.',
    build: () => spaced(buildNocDrag()),
  },
  {
    id: 'noc-attraction',
    name: 'Mouse Attraction',
    category: 'Nature',
    description: 'A mover is pulled toward the mouse cursor via a gravity-like inverse-square force, clamped at close distances. Damping keeps it from blowing up. NOC §2.',
    build: () => spaced(buildNocAttraction()),
  },
  {
    id: 'noc-sine-wave',
    name: 'Sine Wave',
    category: 'Nature',
    description: 'A row of dots oscillating vertically: y_i = sin(i·period + t·speed)·amp. Three sliders for amp / period / speed. NOC §3 — oscillation basics.',
    build: () => spaced(buildNocSineWave()),
  },
  {
    id: 'noc-additive-wave',
    name: 'Additive Wave',
    category: 'Nature',
    description: 'Two sine waves of different amplitude and period summed into one. The beat patterns appear as the two frequencies drift in and out of phase. NOC §3.',
    build: () => spaced(buildNocAdditiveWave()),
  },
  {
    id: 'noc-spring',
    name: 'Spring',
    category: 'Nature',
    description: "Hooke's law: F = −k·(y − restY), plus gravity and damping. State<number> for y and v_y. A bob oscillates beneath a hanging point. NOC §3.",
    build: () => spaced(buildNocSpring()),
  },
  {
    id: 'noc-wave-clock',
    name: 'Wave Clock',
    category: 'Nature',
    description: '16 hue-coded dots orbit the canvas centre at varying angular rates. Pure function of time. NOC §3 — composed oscillation.',
    build: () => spaced(buildNocWaveClock()),
  },
  {
    id: 'noc-seek',
    name: 'Seek',
    category: 'Nature',
    description: "Reynolds' classic seek: desired = normalize(target - pos)·maxSpeed, steer = limit(desired − vel, maxForce), vel += steer. Two sliders tune the agent. NOC §5 — autonomous agents.",
    build: () => spaced(buildNocSeek()),
  },
  {
    id: 'noc-flow-field',
    name: 'Flow Field',
    category: 'Nature',
    description: 'A 16×16 grid of arrows whose angles come from Perlin noise that drifts with time. Slider tunes noise scale. NOC §5 — fields driving motion.',
    build: () => spaced(buildNocFlowField()),
  },
  {
    id: 'noc-rule90',
    name: 'Wolfram Rule 90',
    category: 'Nature',
    description: 'A 1D elementary cellular automaton stepping each frame, rendered as a row that scrolls down the canvas. Default rule 90 produces a Sierpinski triangle from a single seed cell. NOC §7 — drag the slider to explore rules 0–255.',
    build: () => spaced(buildNocRule90()),
  },
  {
    id: 'noc-fractal-tree',
    name: 'Fractal Tree',
    category: 'Nature',
    description: 'A 2-level Pythagoras-style tree built from three lines (trunk + 2 branches), with the trunk rotated by sin(time) to give a gentle sway. NOC §8.',
    build: () => spaced(buildNocFractalTree()),
  },

  // ── Image ──────────────────────────────────────────────
  {
    id: 'slicer-drift',
    name: 'Slicer Drift',
    category: 'Image',
    description: 'A 6×6 image slicer feeds 36 sub-images into a ForEach. Each slice drifts on its own Perlin wanderer — the photo gets cut apart and blown around. Uses an image-type splicer-in on the region.',
    build: () => spaced(buildSlicerDrift()),
  },
  {
    id: 'datamosh',
    name: 'Datamosh',
    category: 'Image',
    description: 'Load an image, layer it 4× with RGB-tinted copies at time-driven offsets and blend modes (SCREEN / DIFFERENCE) for a chromatic-aberration glitch.',
    build: () => spaced(buildDatamosh()),
  },
  {
    id: 'photo-mosaic',
    name: 'Photo Mosaic',
    category: 'Image',
    description: 'Samples a 32×32 grid of pixels from a loaded image and draws each as a dot sized by luminance. Pointillist / Warhol effect that breathes with time.',
    build: () => spaced(buildPhotoMosaic()),
  },
  {
    id: 'chromatic-threshold',
    name: 'Chromatic Threshold',
    category: 'Image',
    description: 'A time-driven threshold level sweeps through the image, then the thresholded version is blended (difference) against the original for an animated glitch / solarisation look.',
    build: () => spaced(buildChromaticThreshold()),
  },

  // ── Pixel ──────────────────────────────────────────────
  {
    id: 'pixel-sort',
    name: 'Pixel Sort',
    category: 'Pixel',
    description: 'Classic glitch art: sorts maximal runs of pixels along each row by brightness, masked by a (lo, hi) range. Drag the sliders to shift the streaks.',
    build: () => spaced(buildPixelSort()),
  },
  {
    id: 'hue-sort',
    name: 'Hue Sort',
    category: 'Pixel',
    description: 'Pixel sort along columns using hue as the criterion. Produces rainbow-streaked vertical bands wherever the mask catches mid-saturation regions.',
    build: () => spaced(buildHueSort()),
  },
  {
    id: 'channel-sort',
    name: 'Channel Sort',
    category: 'Pixel',
    description: 'Pixel sort by the red channel only, descending — pixels stack reddest-first inside each masked run, producing angry red horizontal bands.',
    build: () => spaced(buildChannelSort()),
  },

  // ── Sound ──────────────────────────────────────────────
  {
    id: 'tone-surface',
    name: 'Tone Surface',
    category: 'Sound',
    description: 'Move the mouse — X picks pitch, Y picks amplitude. A 20×20 grid lights up around the cursor.',
    build: () => spaced(buildToneSurface()),
  },
  {
    id: 'fft-bloom',
    name: 'FFT Bloom',
    category: 'Sound',
    description: 'An oscillator → FFT → polar plot. Each of 64 bins becomes a dot on a ring; mouse Y drives the source pitch.',
    build: () => spaced(buildFFTBloom()),
  },

  // ── Feedback (state-driven) ────────────────────────────
  {
    id: 'random-walk',
    name: 'Random Walk Trail',
    category: 'Feedback',
    description: 'A walker drifts via hashed (time, seed) angles, leaving a 200-point colored trail. Hue cycles along the trail; oldest points fade. Uses state/spread.',
    build: () => spaced(buildRandomWalkTrail()),
  },
  {
    id: 'game-of-life',
    name: 'Game of Life',
    category: 'Feedback',
    description: "Conway's Game of Life on a 16×16 toroidal grid. State<boolean*> + gol-step. Tweak density/seed to reset.",
    build: () => spaced(buildGameOfLife()),
  },
  {
    id: 'paint-trail',
    name: 'Paint Trail',
    category: 'Feedback',
    description: 'Move the mouse to paint. image/from-layer + state/value accumulate each brush stroke on an offscreen canvas — infinite paint with a hue that cycles over time.',
    build: () => spaced(buildPaintTrail()),
  },

  // ── Data ───────────────────────────────────────────────
  {
    id: 'csv-bar-chart',
    name: 'CSV Bar Chart',
    category: 'Data',
    description: 'Inline CSV → number spread → 16 colored bars. Edit the `data` param on the CSVInline node to load a new series.',
    build: () => spaced(buildCSVBarChart()),
  },

  // ── Mixed (showcase / multi-feature) ───────────────────
  {
    id: 'mixed-media',
    name: 'Mixed Media',
    category: 'Mixed',
    description: 'Combines two ForEach regions (drifting photo slices + a pulsing ring of dots) and two hand-drawn lines (crosshair) into one Renderer via transform/group. Shows how to merge spread + single layer sources of any mix.',
    build: () => spaced(buildMixedMedia()),
  },
  {
    id: 'subpatch-petals',
    name: 'Subpatch Petals',
    category: 'Mixed',
    description: '8 colored petals orbit and breathe around the canvas centre. Mouse X drives the orbit radius (drag right = push out), mouse Y the breathing speed. The per-petal math is hidden inside a "Petal" subpatch with inputs (angle, t, orbitR, speed) → output (layer). Press I (with the subpatch selected) to dive in.',
    build: () => spaced(buildSubpatchPetals()),
  },
];
