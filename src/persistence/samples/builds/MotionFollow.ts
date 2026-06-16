import { node, edge } from '../helpers';

// ─── Sample: Motion Follow ─────────────────────────────────────────────────
// A target orbits on a circle; three followers chase it with different Motion
// nodes — Smooth (exponential lag), Spring (overshoot + settle), and DelayLine
// (a time-delayed echo). All three are the adaptive motion nodes running on
// vec2. Current port names + real editor coordinates (no spaced()).
export function buildMotionFollow() {
  const time = node('mf_time', 'time/seconds', { x: -240, y: 40 });
  const ang = node('mf_ang', 'math/mul', { x: -60, y: 40 }, { slotCount: 2, b: 0.9 });   // t · 0.9
  const rel = node('mf_rel', 'vec2/from-angle', { x: 120, y: 40 }, { mag: 150 });          // polar → vec2
  const center = node('mf_center', 'vec2/make', { x: 120, y: 200 }, { x: 250, y: 250 });
  const target = node('mf_target', 'math/add', { x: 320, y: 80 }, { slotCount: 2, valueType: 'vec2' });

  const smooth = node('mf_smooth', 'motion/smooth', { x: 320, y: 240 }, { valueType: 'vec2', rate: 2.5 });
  const spring = node('mf_spring', 'motion/spring', { x: 320, y: 360 }, { valueType: 'vec2', stiffness: 90, damping: 8 });
  const echo = node('mf_echo', 'motion/delay-line', { x: 320, y: 480 }, { valueType: 'vec2', delay: 0.5 });

  // Target dot — default white fill, no style.
  const dotT = node('mf_dotT', 'draw/ellipse', { x: 560, y: 80 }, { w: 14, h: 14 });
  // Smooth (blue), Spring (orange), Echo (dim) — each Color → Fill → Ellipse.
  const colS = node('mf_colS', 'color/hex', { x: 560, y: 220 }, { hex: '#4aa8ff' });
  const fillS = node('mf_fillS', 'style/fill', { x: 720, y: 220 });
  const dotS = node('mf_dotS', 'draw/ellipse', { x: 880, y: 220 }, { w: 26, h: 26 });
  const colP = node('mf_colP', 'color/hex', { x: 560, y: 360 }, { hex: '#ff8a3d' });
  const fillP = node('mf_fillP', 'style/fill', { x: 720, y: 360 });
  const dotP = node('mf_dotP', 'draw/ellipse', { x: 880, y: 360 }, { w: 26, h: 26 });
  const colE = node('mf_colE', 'color/hex', { x: 560, y: 500 }, { hex: '#6a7079' });
  const fillE = node('mf_fillE', 'style/fill', { x: 720, y: 500 });
  const dotE = node('mf_dotE', 'draw/ellipse', { x: 880, y: 500 }, { w: 10, h: 10 });

  const group = node('mf_group', 'transform/group', { x: 1080, y: 220 }, { slotCount: 4 });
  const renderer = node('mf_renderer', 'render/renderer', { x: 1280, y: 120 }, { width: 500, height: 500 });

  return {
    nodes: [
      time, ang, rel, center, target, smooth, spring, echo,
      dotT, colS, fillS, dotS, colP, fillP, dotP, colE, fillE, dotE,
      group, renderer,
    ],
    edges: [
      edge('mf_time', 't', 'mf_ang', 'a'),
      edge('mf_ang', 'result', 'mf_rel', 'angle'),
      edge('mf_rel', 'vector', 'mf_target', 'a'),
      edge('mf_center', 'vector', 'mf_target', 'b'),
      // Three adaptive motion followers off the same vec2 target.
      edge('mf_target', 'result', 'mf_smooth', 'value'),
      edge('mf_target', 'result', 'mf_spring', 'value'),
      edge('mf_target', 'result', 'mf_echo', 'value'),
      // Dots
      edge('mf_target', 'result', 'mf_dotT', 'pos'),
      edge('mf_smooth', 'result', 'mf_dotS', 'pos'),
      edge('mf_spring', 'result', 'mf_dotP', 'pos'),
      edge('mf_echo', 'result', 'mf_dotE', 'pos'),
      edge('mf_colS', 'color', 'mf_fillS', 'color'),
      edge('mf_fillS', 'style', 'mf_dotS', 'style'),
      edge('mf_colP', 'color', 'mf_fillP', 'color'),
      edge('mf_fillP', 'style', 'mf_dotP', 'style'),
      edge('mf_colE', 'color', 'mf_fillE', 'color'),
      edge('mf_fillE', 'style', 'mf_dotE', 'style'),
      edge('mf_dotT', 'shape', 'mf_group', 's1'),
      edge('mf_dotS', 'shape', 'mf_group', 's2'),
      edge('mf_dotP', 'shape', 'mf_group', 's3'),
      edge('mf_dotE', 'shape', 'mf_group', 's4'),
      edge('mf_group', 'grouped', 'mf_renderer', 'layers'),
    ],
  };
}
