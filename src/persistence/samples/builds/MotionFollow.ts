import { node, edge } from '../helpers';

// ─── Sample: Motion Lab ────────────────────────────────────────────────────
// The target X sways side to side (a sine). Three followers track it in their
// own lanes against a shared vertical guide; because each Motion node responds
// differently, the dots stay visibly spread apart at every instant:
//   • Smooth     — lags behind, eased
//   • Spring     — overshoots and resonates (widest swing)
//   • DelayLine  — the same motion, phase-shifted later in time
// The top dot is the instant target for reference.
// Current port names + real editor coordinates (no spaced()).
export function buildMotionFollow() {
  // ── Swaying target: x = sin(t·2.0)·165  (range −165…165, center origin) ──
  const time = node('mf_time', 'time/seconds', { x: -460, y: 40 });
  const tscale = node('mf_tscale', 'math/mul', { x: -300, y: 40 }, { b: 2.0 });   // t·2.0
  const sinT = node('mf_sin', 'math/sin', { x: -150, y: 40 });
  const targetX = node('mf_targetX', 'math/mul', { x: -10, y: 40 }, { b: 165 });  // ·165

  // ── Shared vertical guide at the target X (renderer uses a CENTER origin,
  //    so the whole layout is built around 0,0) ──
  const gTop = node('mf_gTop', 'vec2/make', { x: 460, y: -40 }, { y: -200 });
  const gBot = node('mf_gBot', 'vec2/make', { x: 460, y: 30 }, { y: 200 });
  const gCol = node('mf_gCol', 'color/hex', { x: 460, y: 120 }, { hex: '#555a66' });
  const gStroke = node('mf_gStroke', 'style/stroke', { x: 620, y: 80 }, { width: 1 });
  const gLine = node('mf_gLine', 'draw/line', { x: 780, y: -20 });

  // ── Reference dot (instant target) ──
  const refPos = node('mf_refPos', 'vec2/make', { x: 460, y: 220 }, { y: -150 });
  const refDot = node('mf_refDot', 'draw/ellipse', { x: 780, y: 200 }, { w: 16, h: 16 });

  // ── Smooth lane ──
  const smooth = node('mf_smooth', 'motion/smooth', { x: 460, y: 320 }, { rate: 2.5 });
  const smPos = node('mf_smPos', 'vec2/make', { x: 620, y: 320 }, { y: -50 });
  const smCol = node('mf_smCol', 'color/hex', { x: 620, y: 400 }, { hex: '#4aa8ff' });
  const smFill = node('mf_smFill', 'style/fill', { x: 780, y: 400 });
  const smDot = node('mf_smDot', 'draw/ellipse', { x: 940, y: 320 }, { w: 24, h: 24 });

  // ── Spring lane ──
  const spring = node('mf_spring', 'motion/spring', { x: 460, y: 500 }, { stiffness: 65, damping: 5 });
  const spPos = node('mf_spPos', 'vec2/make', { x: 620, y: 500 }, { y: 50 });
  const spCol = node('mf_spCol', 'color/hex', { x: 620, y: 580 }, { hex: '#ff8a3d' });
  const spFill = node('mf_spFill', 'style/fill', { x: 780, y: 580 });
  const spDot = node('mf_spDot', 'draw/ellipse', { x: 940, y: 500 }, { w: 24, h: 24 });

  // ── DelayLine lane ──
  const delay = node('mf_delay', 'motion/delay-line', { x: 460, y: 680 }, { delay: 0.5 });
  const dlPos = node('mf_dlPos', 'vec2/make', { x: 620, y: 680 }, { y: 150 });
  const dlCol = node('mf_dlCol', 'color/hex', { x: 620, y: 760 }, { hex: '#54d08a' });
  const dlFill = node('mf_dlFill', 'style/fill', { x: 780, y: 760 });
  const dlDot = node('mf_dlDot', 'draw/ellipse', { x: 940, y: 680 }, { w: 24, h: 24 });

  const group = node('mf_group', 'transform/group', { x: 1140, y: 320 }, { slotCount: 5 });
  const renderer = node('mf_renderer', 'render/renderer', { x: 1340, y: 200 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, tscale, sinT, targetX,
      gTop, gBot, gCol, gStroke, gLine,
      refPos, refDot,
      smooth, smPos, smCol, smFill, smDot,
      spring, spPos, spCol, spFill, spDot,
      delay, dlPos, dlCol, dlFill, dlDot,
      group, renderer,
    ],
    edges: [
      // swaying target
      edge('mf_time', 't', 'mf_tscale', 'a'),
      edge('mf_tscale', 'result', 'mf_sin', 'x'),
      edge('mf_sin', 'result', 'mf_targetX', 'a'),
      // guide line at target X
      edge('mf_targetX', 'result', 'mf_gTop', 'x'),
      edge('mf_targetX', 'result', 'mf_gBot', 'x'),
      edge('mf_gTop', 'vector', 'mf_gLine', 'a'),
      edge('mf_gBot', 'vector', 'mf_gLine', 'b'),
      edge('mf_gCol', 'color', 'mf_gStroke', 'color'),
      edge('mf_gStroke', 'style', 'mf_gLine', 'style'),
      // reference dot
      edge('mf_targetX', 'result', 'mf_refPos', 'x'),
      edge('mf_refPos', 'vector', 'mf_refDot', 'pos'),
      // smooth
      edge('mf_targetX', 'result', 'mf_smooth', 'value'),
      edge('mf_smooth', 'result', 'mf_smPos', 'x'),
      edge('mf_smPos', 'vector', 'mf_smDot', 'pos'),
      edge('mf_smCol', 'color', 'mf_smFill', 'color'),
      edge('mf_smFill', 'style', 'mf_smDot', 'style'),
      // spring
      edge('mf_targetX', 'result', 'mf_spring', 'value'),
      edge('mf_spring', 'result', 'mf_spPos', 'x'),
      edge('mf_spPos', 'vector', 'mf_spDot', 'pos'),
      edge('mf_spCol', 'color', 'mf_spFill', 'color'),
      edge('mf_spFill', 'style', 'mf_spDot', 'style'),
      // delay
      edge('mf_targetX', 'result', 'mf_delay', 'value'),
      edge('mf_delay', 'result', 'mf_dlPos', 'x'),
      edge('mf_dlPos', 'vector', 'mf_dlDot', 'pos'),
      edge('mf_dlCol', 'color', 'mf_dlFill', 'color'),
      edge('mf_dlFill', 'style', 'mf_dlDot', 'style'),
      // collect
      edge('mf_gLine', 'shape', 'mf_group', 's1'),
      edge('mf_refDot', 'shape', 'mf_group', 's2'),
      edge('mf_smDot', 'shape', 'mf_group', 's3'),
      edge('mf_spDot', 'shape', 'mf_group', 's4'),
      edge('mf_dlDot', 'shape', 'mf_group', 's5'),
      edge('mf_group', 'grouped', 'mf_renderer', 'layers'),
    ],
  };
}
