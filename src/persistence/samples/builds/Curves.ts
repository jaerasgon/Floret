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


// ─── Sample: Islamic Pattern ───────────────────────────────────────────────
// 8-fold rotational symmetry. A "petal" — three concentric ellipses at
// an offset from the centre — is rotated 8 times around (250, 250) by
// the standard kaleidoscope ForEach pattern. Time slowly oscillates
// the petal's distance, breathing the pattern in and out.
export function buildIslamicPattern() {
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
