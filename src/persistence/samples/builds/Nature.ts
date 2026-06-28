import type { FloretNode, FloretEdge } from '../../../editor/editor-state';
import { node, edge } from '../helpers';






// ─── Sample: Pendulum ─────────────────────────────────────────────────────
// Classic single pendulum. θ is the angle from straight-down (vertical).
//   ω_next = (ω + (-g/L)·sin θ) · damp
//   θ_next = θ + ω_next
// The bob position is (L·sin θ, L·cos θ) — note +Y is down so cos points
// straight down at θ = 0.
export function buildPendulum() {
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
      edge('pe_initTheta', 'number','pe_theta','init'),
      edge('pe_initOmega', 'number','pe_omega','init'),
      // sinθ, cosθ
      edge('pe_theta','current','pe_sinTheta','x'),
      edge('pe_theta','current','pe_cosTheta','x'),
      // g/L
      edge('pe_gravity', 'number','pe_gOverL','a'),
      edge('pe_length', 'number', 'pe_gOverL','b'),
      // α_raw = (g/L) · sinθ;  α = -α_raw
      edge('pe_gOverL', 'quotient','pe_alphaRaw','a'),
      edge('pe_sinTheta', 'sine','pe_alphaRaw','b'),
      edge('pe_alphaRaw', 'product','pe_alpha','x'),
      // ω_next = (ω + α) · damp
      edge('pe_omega','current','pe_omegaSum','a'),
      edge('pe_alpha', 'negated','pe_omegaSum','b'),
      edge('pe_omegaSum', 'sum','pe_omegaNext','a'),
      edge('pe_damping', 'number','pe_omegaNext','b'),
      // θ_next = θ + ω_next
      edge('pe_theta','current','pe_thetaNext','a'),
      edge('pe_omegaNext', 'product','pe_thetaNext','b'),
      // Write back.
      edge('pe_thetaNext', 'sum','pe_theta','next'),
      edge('pe_omegaNext', 'product','pe_omega','next'),
      // Bob position.
      edge('pe_sinTheta', 'sine','pe_bobX','a'),
      edge('pe_length', 'number','pe_bobX','b'),
      edge('pe_cosTheta', 'cosine','pe_bobY','a'),
      edge('pe_length', 'number','pe_bobY','b'),
      edge('pe_bobY', 'product','pe_bobYAbs','a'),
      edge('pe_pivotY', 'number','pe_bobYAbs','b'),
      // Rod line from pivot (0, pivotY) → bob.
      edge('pe_pivotY', 'number','pe_rod','y1'),
      edge('pe_bobX', 'product','pe_rod','x2'),
      edge('pe_bobYAbs', 'sum','pe_rod','y2'),
      edge('pe_rodC', 'style','pe_rodW','base'),
      edge('pe_rodW', 'style','pe_rod','style'),
      // Bob disc.
      edge('pe_bobX', 'product','pe_bob','x'),
      edge('pe_bobYAbs', 'sum','pe_bob','y'),
      edge('pe_bobFill', 'style','pe_bob','style'),
      // Renderer.
      edge('pe_rod', 'shape','pe_group','s1'),
      edge('pe_bob', 'shape','pe_group','s2'),
      edge('pe_group', 'grouped','pe_renderer','layers'),
    ],
  };
}









// ─── NOC Ch 2 — Wind & Gravity Forces ─────────────────────────────────────
// Mouse-X-driven wind plus constant gravity push a ball around.
// F = m·a → acc = (wind + gravity·m)/m = wind/m + gravity. State<vec2>.
export function buildNocWindGravity() {
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
      edge('wg_bg', 'color', 'wg_renderer', 'background'),
      edge('wg_initPos', 'vector', 'wg_pos', 'init'),
      edge('wg_initVel', 'vector', 'wg_vel', 'init'),
      // wind vector from mouseX
      edge('wg_mouse', 'x', 'wg_windMul', 'a'),
      edge('wg_windMul', 'product', 'wg_windVec', 'x'),
      // wind / mass
      edge('wg_windVec', 'vector', 'wg_wScaled', 'v'),
      edge('wg_mass', 'number', 'wg_invMass', 'b'),
      edge('wg_invMass', 'quotient', 'wg_wScaled', 's'),
      // acc = wind/m + g
      edge('wg_wScaled', 'scaled', 'wg_accAdd', 'a'),
      edge('wg_gravity', 'vector', 'wg_accAdd', 'b'),
      // vel_next = vel + acc
      edge('wg_vel', 'current', 'wg_velAdd', 'a'),
      edge('wg_accAdd', 'vector', 'wg_velAdd', 'b'),
      // pos_tmp = pos + vel_next
      edge('wg_pos', 'current', 'wg_posAdd', 'a'),
      edge('wg_velAdd', 'vector', 'wg_posAdd', 'b'),
      // bounce
      edge('wg_posAdd', 'vector', 'wg_ps', 'v'),
      edge('wg_velAdd', 'vector', 'wg_vs', 'v'),
      edge('wg_ps', 'x', 'wg_absX', 'x'),
      edge('wg_ps', 'y', 'wg_absY', 'x'),
      edge('wg_absX', 'absolute', 'wg_hitX', 'a'),
      edge('wg_bounds', 'number', 'wg_hitX', 'b'),
      edge('wg_absY', 'absolute', 'wg_hitY', 'a'),
      edge('wg_bounds', 'number', 'wg_hitY', 'b'),
      edge('wg_vs', 'x', 'wg_vxNeg', 'x'),
      edge('wg_vs', 'y', 'wg_vyNeg', 'x'),
      edge('wg_hitX', 'result', 'wg_vxOut', 'cond'),
      edge('wg_vxNeg', 'negated', 'wg_vxOut', 'a'),
      edge('wg_vs', 'x', 'wg_vxOut', 'b'),
      edge('wg_hitY', 'result', 'wg_vyOut', 'cond'),
      edge('wg_vyNeg', 'negated', 'wg_vyOut', 'a'),
      edge('wg_vs', 'y', 'wg_vyOut', 'b'),
      edge('wg_vxOut', 'selected', 'wg_vFinal', 'x'),
      edge('wg_vyOut', 'selected', 'wg_vFinal', 'y'),
      edge('wg_posAdd', 'vector', 'wg_pos', 'next'),
      edge('wg_vFinal', 'vector', 'wg_vel', 'next'),
      // draw ball — size proportional to mass
      edge('wg_pos', 'current', 'wg_psDraw', 'v'),
      edge('wg_psDraw', 'x', 'wg_ball', 'x'),
      edge('wg_psDraw', 'y', 'wg_ball', 'y'),
      edge('wg_mass', 'number', 'wg_sizeMul', 'a'),
      edge('wg_sizeMul', 'product', 'wg_ball', 'w'),
      edge('wg_sizeMul', 'product', 'wg_ball', 'h'),
      edge('wg_fill', 'style', 'wg_ball', 'style'),
      edge('wg_ball', 'shape', 'wg_renderer', 'layers'),
    ],
  };
}



// ─── NOC Ch 2 — Drag / Fluid Resistance ───────────────────────────────────
// Gravity pulls the ball down; quadratic drag −c·|v|·v opposes motion
// across the whole canvas. The terminal-velocity hover is the visual:
// the ball accelerates then settles at a constant fall speed.
export function buildNocDrag() {
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
      edge('dr_bg', 'color', 'dr_renderer', 'background'),
      edge('dr_initPos', 'vector', 'dr_pos', 'init'),
      edge('dr_initVel', 'vector', 'dr_vel', 'init'),
      edge('dr_vel', 'current', 'dr_vMag', 'v'),
      edge('dr_vMag', 'length', 'dr_cv', 'a'),
      edge('dr_c', 'number', 'dr_cv', 'b'),
      edge('dr_cv', 'product', 'dr_cvN', 'x'),
      edge('dr_vel', 'current', 'dr_drag', 'v'),
      edge('dr_cvN', 'negated', 'dr_drag', 's'),
      edge('dr_gravity', 'vector', 'dr_acc', 'a'),
      edge('dr_drag', 'scaled', 'dr_acc', 'b'),
      edge('dr_vel', 'current', 'dr_velAdd', 'a'),
      edge('dr_acc', 'vector', 'dr_velAdd', 'b'),
      edge('dr_pos', 'current', 'dr_posAdd', 'a'),
      edge('dr_velAdd', 'vector', 'dr_posAdd', 'b'),
      edge('dr_posAdd', 'vector', 'dr_psN', 'v'),
      edge('dr_psN', 'y', 'dr_hitFloor', 'a'),
      edge('dr_bounds', 'number', 'dr_hitFloor', 'b'),
      edge('dr_hitFloor', 'result', 'dr_yClamp', 'cond'),
      edge('dr_bounds', 'number', 'dr_yClamp', 'a'),
      edge('dr_psN', 'y', 'dr_yClamp', 'b'),
      edge('dr_psN', 'x', 'dr_posFin', 'x'),
      edge('dr_yClamp', 'selected', 'dr_posFin', 'y'),
      edge('dr_velAdd', 'vector', 'dr_vsFin', 'v'),
      edge('dr_vsFin', 'y', 'dr_vyN2', 'x'),
      edge('dr_vyN2', 'negated', 'dr_vyDamp', 'a'),
      edge('dr_hitFloor', 'result', 'dr_vyOut2', 'cond'),
      edge('dr_vyDamp', 'product', 'dr_vyOut2', 'a'),
      edge('dr_vsFin', 'y', 'dr_vyOut2', 'b'),
      edge('dr_vsFin', 'x', 'dr_velFin', 'x'),
      edge('dr_vyOut2', 'selected', 'dr_velFin', 'y'),
      edge('dr_posFin', 'vector', 'dr_pos', 'next'),
      edge('dr_velFin', 'vector', 'dr_vel', 'next'),
      edge('dr_waterFill', 'style', 'dr_water', 'style'),
      edge('dr_pos', 'current', 'dr_psDraw', 'v'),
      edge('dr_psDraw', 'x', 'dr_ball', 'x'),
      edge('dr_psDraw', 'y', 'dr_ball', 'y'),
      edge('dr_ballFill', 'style', 'dr_ball', 'style'),
      edge('dr_water', 'shape', 'dr_grp', 's1'),
      edge('dr_ball', 'shape', 'dr_grp', 's2'),
      edge('dr_grp', 'grouped', 'dr_renderer', 'layers'),
    ],
  };
}


// ─── NOC Ch 2 — Mouse Attraction ──────────────────────────────────────────
// One mover is pulled toward the mouse via inverse-square-ish gravity.
// Distance clamped to avoid blow-ups. State<vec2> for pos & vel.
export function buildNocAttraction() {
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
      edge('at_bg', 'color', 'at_renderer', 'background'),
      edge('at_initPos', 'vector', 'at_pos', 'init'),
      edge('at_initVel', 'vector', 'at_vel', 'init'),
      edge('at_mouse', 'x', 'at_mv', 'x'),
      edge('at_mouse', 'y', 'at_mv', 'y'),
      edge('at_mv', 'vector', 'at_diff', 'a'),
      edge('at_pos', 'current', 'at_diff', 'b'),
      edge('at_diff', 'vector', 'at_dist', 'v'),
      edge('at_dist', 'length', 'at_distSq', 'x'),
      edge('at_distSq', 'squared', 'at_distClamp', 'x'),
      edge('at_G', 'number', 'at_F', 'a'),
      edge('at_distClamp', 'clamped', 'at_F', 'b'),
      edge('at_diff', 'vector', 'at_dir', 'v'),
      edge('at_dir', 'normalized', 'at_acc', 'v'),
      edge('at_F', 'quotient', 'at_acc', 's'),
      edge('at_vel', 'current', 'at_velAdd', 'a'),
      edge('at_acc', 'scaled', 'at_velAdd', 'b'),
      edge('at_velAdd', 'vector', 'at_velDamp', 'v'),
      edge('at_damp', 'number', 'at_velDamp', 's'),
      edge('at_pos', 'current', 'at_posAdd', 'a'),
      edge('at_velDamp', 'scaled', 'at_posAdd', 'b'),
      edge('at_posAdd', 'vector', 'at_pos', 'next'),
      edge('at_velDamp', 'scaled', 'at_vel', 'next'),
      edge('at_pos', 'current', 'at_psDraw', 'v'),
      edge('at_psDraw', 'x', 'at_ball', 'x'),
      edge('at_psDraw', 'y', 'at_ball', 'y'),
      edge('at_fill', 'style', 'at_ball', 'style'),
      edge('at_mouse', 'x', 'at_mMark', 'x'),
      edge('at_mouse', 'y', 'at_mMark', 'y'),
      edge('at_mFill', 'style', 'at_mMark', 'style'),
      edge('at_mMark', 'shape', 'at_grp', 's1'),
      edge('at_ball', 'shape', 'at_grp', 's2'),
      edge('at_grp', 'grouped', 'at_renderer', 'layers'),
    ],
  };
}




// ─── NOC Ch 3 — Spring (Hooke's Law) ──────────────────────────────────────
// A bob hanging from a spring: F = -k*(y - restY), with gravity + damping.
// State<number> for bobY and vy. Drag/release with mouse for fun? No —
// for simplicity, just let it oscillate from a perturbed start.
export function buildNocSpring() {
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
      edge('spr_bg', 'color', 'spr_renderer', 'background'),
      edge('spr_initY', 'number', 'spr_y', 'init'),
      edge('spr_initVy', 'number', 'spr_vy', 'init'),
      edge('spr_y', 'current', 'spr_stretch', 'a'),
      edge('spr_restY', 'number', 'spr_stretch', 'b'),
      edge('spr_stretch', 'difference', 'spr_kx', 'a'),
      edge('spr_k', 'number', 'spr_kx', 'b'),
      edge('spr_kx', 'product', 'spr_Fspring', 'x'),
      edge('spr_Fspring', 'negated', 'spr_acc', 'a'),
      edge('spr_gravity', 'number', 'spr_acc', 'b'),
      edge('spr_vy', 'current', 'spr_vyAdd', 'a'),
      edge('spr_acc', 'sum', 'spr_vyAdd', 'b'),
      edge('spr_vyAdd', 'sum', 'spr_vyDamp', 'a'),
      edge('spr_damp', 'number', 'spr_vyDamp', 'b'),
      edge('spr_y', 'current', 'spr_yNext', 'a'),
      edge('spr_vyDamp', 'product', 'spr_yNext', 'b'),
      edge('spr_yNext', 'sum', 'spr_y', 'next'),
      edge('spr_vyDamp', 'product', 'spr_vy', 'next'),
      // Rod from anchor (0, -240) to bob (0, y_current).
      edge('spr_strokeC', 'color', 'spr_stroke', 'color'),
      edge('spr_stroke', 'style', 'spr_rod', 'style'),
      edge('spr_anchorY', 'number', 'spr_rod', 'y1'),
      edge('spr_y', 'current', 'spr_rod', 'y2'),
      // Anchor disc at (0, -240).
      edge('spr_anchorFill', 'style', 'spr_anchor', 'style'),
      edge('spr_anchorY', 'number', 'spr_anchor', 'y'),
      // Bob at (0, y_current).
      edge('spr_y', 'current', 'spr_bob', 'y'),
      edge('spr_fill', 'style', 'spr_bob', 'style'),
      // Group: rod, anchor, bob.
      edge('spr_rod', 'shape', 'spr_grp', 's1'),
      edge('spr_anchor', 'shape', 'spr_grp', 's2'),
      edge('spr_bob', 'shape', 'spr_grp', 's3'),
      edge('spr_grp', 'grouped', 'spr_renderer', 'layers'),
    ],
  };
}



// ─── NOC Ch 5 — Seek ──────────────────────────────────────────────────────
// Vehicle steers toward the mouse using Reynolds' classic seek formula:
//   desired = normalize(target - pos) * maxSpeed
//   steer   = limit(desired - vel, maxForce)
//   vel    += steer
// State<vec2> for pos & vel.
export function buildNocSeek() {
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
      edge('sk_bg', 'color', 'sk_renderer', 'background'),
      edge('sk_initPos', 'vector', 'sk_pos', 'init'),
      edge('sk_initVel', 'vector', 'sk_vel', 'init'),
      edge('sk_mouse', 'x', 'sk_mv', 'x'),
      edge('sk_mouse', 'y', 'sk_mv', 'y'),
      edge('sk_mv', 'vector', 'sk_diff', 'a'),
      edge('sk_pos', 'current', 'sk_diff', 'b'),
      edge('sk_diff', 'vector', 'sk_dir', 'v'),
      edge('sk_dir', 'normalized', 'sk_desired', 'v'),
      edge('sk_maxSpeed', 'number', 'sk_desired', 's'),
      edge('sk_desired', 'scaled', 'sk_steerRaw', 'a'),
      edge('sk_vel', 'current', 'sk_steerRaw', 'b'),
      edge('sk_steerRaw', 'vector', 'sk_steer', 'v'),
      edge('sk_maxForce', 'number', 'sk_steer', 'max'),
      edge('sk_vel', 'current', 'sk_velAdd', 'a'),
      edge('sk_steer', 'vector', 'sk_velAdd', 'b'),
      edge('sk_velAdd', 'vector', 'sk_velCap', 'v'),
      edge('sk_maxSpeed', 'number', 'sk_velCap', 'max'),
      edge('sk_pos', 'current', 'sk_posAdd', 'a'),
      edge('sk_velCap', 'vector', 'sk_posAdd', 'b'),
      edge('sk_posAdd', 'vector', 'sk_pos', 'next'),
      edge('sk_velCap', 'vector', 'sk_vel', 'next'),
      // draw
      edge('sk_vel', 'current', 'sk_head', 'v'),
      edge('sk_mouse', 'x', 'sk_target', 'x'),
      edge('sk_mouse', 'y', 'sk_target', 'y'),
      edge('sk_tFill', 'style', 'sk_target', 'style'),
      edge('sk_pos', 'current', 'sk_splitP', 'v'),
      edge('sk_splitP', 'x', 'sk_vehicle', 'x'),
      edge('sk_splitP', 'y', 'sk_vehicle', 'y'),
      edge('sk_vFill', 'style', 'sk_vehicle', 'style'),
      edge('sk_target', 'shape', 'sk_grp', 's1'),
      edge('sk_vehicle', 'shape', 'sk_grp', 's2'),
      edge('sk_grp', 'grouped', 'sk_renderer', 'layers'),
    ],
  };
}


// ─── NOC Ch 5 — Flow Field ────────────────────────────────────────────────
// A Perlin-driven flow field of arrows + many stateless dots that drift
// along it. Each dot's position is wanderer2d(time, seed) shifted by a
// random home — same as Movers but visualised over a vector field grid.
export function buildNocFlowField() {
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
      edge('flo_bg', 'color', 'flo_renderer', 'background'),
      edge('flo_grid', 'positions', RID, 'p'),
      edge(RID, 'p', 'flo_split', 'v'),
      edge('flo_split', 'x', 'flo_xs', 'a'),
      edge('flo_noiseScale', 'number', 'flo_xs', 'b'),
      edge('flo_split', 'y', 'flo_ys', 'a'),
      edge('flo_noiseScale', 'number', 'flo_ys', 'b'),
      edge('flo_ys', 'product', 'flo_yt', 'a'),
      edge('flo_time', 't', 'flo_yt', 'b'),
      edge('flo_xs', 'product', 'flo_per', 'x'),
      edge('flo_yt', 'sum', 'flo_per', 'y'),
      edge('flo_per', 'noise', 'flo_ang', 'a'),
      edge('flo_ang', 'product', 'flo_fa', 'angle'),
      edge('flo_len', 'number', 'flo_fa', 'mag'),
      edge('flo_fa', 'vector', 'flo_fs', 'v'),
      edge('flo_split', 'x', 'flo_x2', 'a'),
      edge('flo_fs', 'x', 'flo_x2', 'b'),
      edge('flo_split', 'y', 'flo_y2', 'a'),
      edge('flo_fs', 'y', 'flo_y2', 'b'),
      edge('flo_per', 'noise', 'flo_hueMul', 'a'),
      edge('flo_hueMul', 'product', 'flo_hsl', 'h'),
      edge('flo_hsl', 'color', 'flo_strokeC', 'color'),
      edge('flo_strokeC', 'style', 'flo_line', 'style'),
      edge('flo_split', 'x', 'flo_line', 'x1'),
      edge('flo_split', 'y', 'flo_line', 'y1'),
      edge('flo_x2', 'sum', 'flo_line', 'x2'),
      edge('flo_y2', 'sum', 'flo_line', 'y2'),
      edge('flo_line', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'flo_renderer', 'layers'),
    ],
  };
}



// ─── NOC Ch 8 — Fractal Tree ──────────────────────────────────────────────
// Recursively-generated tree, unrolled at build time into N draw/line
// nodes (one per branch). The classic binary-branching tree: each branch
// spawns two children at ±angle, length scaled by 0.7. 4 levels yields
// 1 + 2 + 4 + 8 = 15 branches. All branches share a single Style → Group →
// Renderer chain. Static — no swaying — to keep the structure crisp.
export function buildNocFractalTree() {
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
    edge('ft_bg', 'color', 'ft_renderer', 'background'),
    edge('ft_strokeC', 'color', 'ft_stroke', 'color'),
  ];
  const grp = node('ft_grp', 'transform/group', { x: 420, y: 20 }, { slotCount: branches.length });
  nodes.push(grp);
  edges.push(edge('ft_grp', 'grouped', 'ft_renderer', 'layers'));

  branches.forEach((b, i) => {
    const swId = `ft_sw_${i}`;
    const lnId = `ft_ln_${i}`;
    const xs = 80 + (i % 6) * 60;
    const ys = 200 + Math.floor(i / 6) * 80;
    const swN = node(swId, 'style/stroke-weight', { x: xs, y: ys }, { w: Math.max(1, b.level) });
    const lnN = node(lnId, 'draw/line', { x: xs + 180, y: ys },
                     { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2 });
    nodes.push(swN, lnN);
    edges.push(edge('ft_stroke', 'style', swId, 'base'));
    edges.push(edge(swId, 'style', lnId, 'style'));
    edges.push(edge(lnId, 'shape', 'ft_grp', `s${i + 1}`));
  });

  return { nodes, edges };
}


// ─── NOC Ch 8 — Fractal Tree (spread-driven) ───────────────────────────────
// Same tree, no node duplication. The recursion still runs at build time, but
// instead of emitting one draw/line per branch it bakes the branch geometry
// into five `data/csv-inline` number spreads (x1, y1, x2, y2, weight) and zips
// them through a SINGLE ForEach → one draw/line. 30 nodes → 14, and per-branch
// stroke weight survives (the trunk stays thick, tips thin). This is the
// idiomatic Floret answer to "stop duplicating nodes": compute the data once,
// stream it as a spread.
export function buildFractalTreeSpread() {
  const LEVELS = 4, ANGLE = 0.5, SCALE = 0.7, TRUNK_LEN = 140, ROOT_Y = 220;
  interface Branch { x1: number; y1: number; x2: number; y2: number; level: number }
  const branches: Branch[] = [];
  function grow(x: number, y: number, angle: number, len: number, level: number): void {
    if (level <= 0) return;
    const x2 = x + Math.sin(angle) * len;
    const y2 = y - Math.cos(angle) * len;
    branches.push({ x1: x, y1: y, x2, y2, level });
    grow(x2, y2, angle - ANGLE, len * SCALE, level - 1);
    grow(x2, y2, angle + ANGLE, len * SCALE, level - 1);
  }
  grow(0, ROOT_Y, 0, TRUNK_LEN, LEVELS);
  const csv = (sel: (b: Branch) => number): string =>
    branches.map(sel).map((n) => Math.round(n * 100) / 100).join(', ');

  const x1 = node('fts_x1', 'data/csv-inline', { x: -560, y: -40 }, { data: csv((b) => b.x1) });
  const y1 = node('fts_y1', 'data/csv-inline', { x: -560, y:  40 }, { data: csv((b) => b.y1) });
  const x2 = node('fts_x2', 'data/csv-inline', { x: -560, y: 120 }, { data: csv((b) => b.x2) });
  const y2 = node('fts_y2', 'data/csv-inline', { x: -560, y: 200 }, { data: csv((b) => b.y2) });
  const wv = node('fts_w',  'data/csv-inline', { x: -560, y: 280 }, { data: csv((b) => Math.max(1, b.level)) });
  const strokeC = node('fts_strokeC', 'color/rgb',   { x: -560, y: -140 }, { r: 230, g: 160, b: 80, a: 255 });
  const stroke  = node('fts_stroke',  'style/stroke', { x: -360, y: -140 });
  const bg       = node('fts_bg', 'color/rgb', { x: -560, y: 360 }, { r: 16, g: 22, b: 16, a: 255 });
  const renderer = node('fts_renderer', 'render/renderer', { x: 760, y: 0 }, { width: 500, height: 500 });

  const RID = 'fts_region';
  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -120, y: -20 },
    data: {
      defType: 'region/foreach',
      params: { width: 780, height: 360, label: 'ForEach branch' },
      ports: [
        { id: 'x1', name: 'x1', type: 'number', direction: 'input' },
        { id: 'y1', name: 'y1', type: 'number', direction: 'input' },
        { id: 'x2', name: 'x2', type: 'number', direction: 'input' },
        { id: 'y2', name: 'y2', type: 'number', direction: 'input' },
        { id: 'w',  name: 'w',  type: 'number', direction: 'input' },
        { id: 'layer', name: 'layer', type: 'layer', direction: 'output' },
      ],
    },
    style: { width: 780, height: 360 },
  };
  const aMake = node('fts_a',  'vec2/make', { x: 60, y: 40 }, {}, RID);
  const bMake = node('fts_b',  'vec2/make', { x: 60, y: 160 }, {}, RID);
  const sw    = node('fts_sw', 'style/stroke-weight', { x: 280, y: 120 }, {}, RID);
  const line  = node('fts_line', 'draw/line', { x: 500, y: 70 }, {}, RID);

  return {
    nodes: [x1, y1, x2, y2, wv, strokeC, stroke, bg, renderer, region, aMake, bMake, sw, line],
    edges: [
      edge('fts_bg', 'color', 'fts_renderer', 'background'),
      edge('fts_strokeC', 'color', 'fts_stroke', 'color'),
      edge('fts_x1', 'values', RID, 'x1'),
      edge('fts_y1', 'values', RID, 'y1'),
      edge('fts_x2', 'values', RID, 'x2'),
      edge('fts_y2', 'values', RID, 'y2'),
      edge('fts_w',  'values', RID, 'w'),
      edge(RID, 'x1', 'fts_a', 'x'),
      edge(RID, 'y1', 'fts_a', 'y'),
      edge(RID, 'x2', 'fts_b', 'x'),
      edge(RID, 'y2', 'fts_b', 'y'),
      edge('fts_stroke', 'style', 'fts_sw', 'base'),
      edge(RID, 'w', 'fts_sw', 'w'),
      edge('fts_a', 'vector', 'fts_line', 'a'),
      edge('fts_b', 'vector', 'fts_line', 'b'),
      edge('fts_sw', 'style', 'fts_line', 'style'),
      edge('fts_line', 'shape', RID, 'layer'),
      edge(RID, 'layer', 'fts_renderer', 'layers'),
    ],
  };
}


// ─── NOC Ch 8 — Fractal Tree (generative) ──────────────────────────────────
// The recursion lives IN THE GRAPH — no branch coordinates baked anywhere.
// A single ForEach walks 32 steps (8 leaf-paths × 4 segments). Three scalar
// accumulators (angle, x, y) carry the turtle's state; at the start of each
// leaf (seg 0) a lerp-gate resets it to the trunk base, then each step turns by
// ±ANGLE (decoded from the leaf index's bits), advances by len·(sin,−cos), and
// draws one line. Change ANGLE / SCALE / LEVELS and the whole tree reshapes —
// the price of that is node count (Floret has no vectorised math, so every
// trig/accumulate step is its own node: ~45 here vs 14 for the spread version).
export function buildFractalTreeGenerative() {
  const LEVELS = 4;          // trunk + 3 turning tiers
  const SEG = LEVELS;        // segments per leaf path
  const LEAVES = 2 ** (LEVELS - 1);
  const STEPS = LEAVES * SEG; // 32
  const ANGLE = 0.5, SCALE = 0.7, TRUNK = 140, ROOT_Y = 220;
  const RID = 'gt_region';
  const I = (id: string, t: string, x: number, y: number, p: Record<string, unknown> = {}) =>
    node(id, t, { x, y }, p, RID);   // inner-scope node helper

  // ── outer scope ──
  const nN     = node('gt_n', 'value/number', { x: -720, y: 20 }, { value: STEPS });
  const range  = node('gt_range', 'spread/range', { x: -560, y: 20 }, { count: STEPS });
  const strokeC= node('gt_strokeC', 'color/rgb', { x: -720, y: -80 }, { r: 230, g: 160, b: 80, a: 255 });
  const stroke = node('gt_stroke', 'style/stroke', { x: -560, y: -80 });
  const bg     = node('gt_bg', 'color/rgb', { x: -720, y: 120 }, { r: 16, g: 22, b: 16, a: 255 });
  const renderer = node('gt_renderer', 'render/renderer', { x: 1700, y: 0 }, { width: 500, height: 500 });

  const region: FloretNode = {
    id: RID, type: 'floret', position: { x: -320, y: -40 },
    data: { defType: 'region/foreach', params: { width: 1960, height: 620, label: 'ForEach step' }, ports: [] },
    style: { width: 1960, height: 620 },
  };

  // ── inner scope: turtle walk ──
  const kin   = I('gt_kin', 'region/splicer-in/number', 40, 40);        // inner = k (0..31)
  const seg   = I('gt_seg', 'math/mod', 200, 40, { b: SEG });           // k mod 4
  const leafd = I('gt_leafd', 'math/div', 200, 120, { b: SEG });
  const leaf  = I('gt_leaf', 'math/floor', 360, 120);                   // floor(k/4)
  const stepG = I('gt_stepG', 'math/step', 360, 40, { edge: 0.5 });     // seg>=1 ? 1 : 0
  const gate0 = I('gt_gate0', 'math/sub', 520, 40, { a: 1 });           // 1 at seg 0
  // bit-decode the turn: bit_{seg-1} of leaf
  const expN  = I('gt_exp', 'math/sub', 520, 160, { b: 1 });            // seg-1
  const powB  = I('gt_powB', 'math/pow', 680, 160, { a: 2 });           // 2^(seg-1)
  const div2  = I('gt_div2', 'math/div', 840, 160);                     // leaf / 2^(seg-1)
  const flr2  = I('gt_flr2', 'math/floor', 1000, 160);
  const bit   = I('gt_bit', 'math/mod', 1160, 160, { b: 2 });           // 0 or 1
  const sgn1  = I('gt_sgn1', 'math/mul', 1160, 240, { b: 2 });
  const sgn   = I('gt_sgn', 'math/sub', 1320, 240, { b: 1 });           // -1 or +1
  const trnR  = I('gt_trnR', 'math/mul', 1320, 320, { b: ANGLE });
  const turn  = I('gt_turn', 'math/mul', 1480, 320);                    // × segGate → 0 at trunk
  // accumulators (slot 1 angle, 2 x, 3 y) + per-leaf reset
  const ra    = I('gt_ra', 'region/accumulator-read/number', 680, 380, { slot: 1 });
  const rx    = I('gt_rx', 'region/accumulator-read/number', 680, 460, { slot: 2 });
  const ry    = I('gt_ry', 'region/accumulator-read/number', 680, 540, { slot: 3 });
  const pAng  = I('gt_pAng', 'math/lerp', 900, 380, { b: 0 });          // reset angle→0 at seg0
  const pX    = I('gt_pX', 'math/lerp', 900, 460, { b: 0 });            // reset x→0
  const pY    = I('gt_pY', 'math/lerp', 900, 540, { b: ROOT_Y });       // reset y→ROOT_Y
  const nAng  = I('gt_nAng', 'math/add', 1640, 380);                    // prevAngle + turn
  const lpow  = I('gt_lpow', 'math/pow', 1060, 620, { a: SCALE });      // SCALE^seg
  const len   = I('gt_len', 'math/mul', 1220, 620, { a: TRUNK });
  const sinA  = I('gt_sin', 'math/sin', 1800, 360);
  const cosA  = I('gt_cos', 'math/cos', 1800, 440);
  const dX    = I('gt_dx', 'math/mul', 1960, 360);                      // len·sin
  const ncos  = I('gt_ncos', 'math/neg', 1960, 480);
  const dY    = I('gt_dy', 'math/mul', 2120, 480);                      // len·(−cos)
  const nX    = I('gt_nX', 'math/add', 2280, 440);                      // prevX + dX
  const nY    = I('gt_nY', 'math/add', 2280, 540);                      // prevY + dY
  const wa    = I('gt_wa', 'region/accumulator-write/number', 1820, 300, { slot: 1 });
  const wx    = I('gt_wx', 'region/accumulator-write/number', 2440, 440, { slot: 2 });
  const wy    = I('gt_wy', 'region/accumulator-write/number', 2440, 540, { slot: 3 });
  const wsub  = I('gt_wsub', 'math/sub', 1640, 660, { a: LEVELS });     // taper: thick trunk
  const wmax  = I('gt_wmax', 'math/max', 1800, 660, { b: 1 });
  const swt   = I('gt_swt', 'style/stroke-weight', 2000, 660);
  const aMk   = I('gt_a', 'vec2/make', 2440, 360);                      // prev point
  const bMk   = I('gt_b', 'vec2/make', 2600, 480);                      // new point
  const line  = I('gt_line', 'draw/line', 2760, 420);
  const out   = I('gt_out', 'region/splicer-out/layer', 2920, 420);

  return {
    nodes: [nN, range, strokeC, stroke, bg, renderer, region,
      kin, seg, leafd, leaf, stepG, gate0, expN, powB, div2, flr2, bit, sgn1, sgn, trnR, turn,
      ra, rx, ry, pAng, pX, pY, nAng, lpow, len, sinA, cosA, dX, ncos, dY, nX, nY,
      wa, wx, wy, wsub, wmax, swt, aMk, bMk, line, out],
    edges: [
      edge('gt_n', 'number', 'gt_range', 'count'),
      edge('gt_strokeC', 'color', 'gt_stroke', 'color'),
      edge('gt_bg', 'color', 'gt_renderer', 'background'),
      edge('gt_range', 'indices', 'gt_kin', 'outer'),
      // index → seg / leaf
      edge('gt_kin', 'inner', 'gt_seg', 'a'),
      edge('gt_kin', 'inner', 'gt_leafd', 'a'),
      edge('gt_leafd', 'result', 'gt_leaf', 'x'),
      edge('gt_seg', 'result', 'gt_stepG', 'x'),
      edge('gt_stepG', 'result', 'gt_gate0', 'b'),
      // bit decode → turn
      edge('gt_seg', 'result', 'gt_exp', 'a'),
      edge('gt_exp', 'result', 'gt_powB', 'b'),
      edge('gt_leaf', 'result', 'gt_div2', 'a'),
      edge('gt_powB', 'result', 'gt_div2', 'b'),
      edge('gt_div2', 'result', 'gt_flr2', 'x'),
      edge('gt_flr2', 'result', 'gt_bit', 'a'),
      edge('gt_bit', 'result', 'gt_sgn1', 'a'),
      edge('gt_sgn1', 'result', 'gt_sgn', 'a'),
      edge('gt_sgn', 'result', 'gt_trnR', 'a'),
      edge('gt_trnR', 'result', 'gt_turn', 'a'),
      edge('gt_stepG', 'result', 'gt_turn', 'b'),
      // reset gate on the three accumulators
      edge('gt_ra', 'read', 'gt_pAng', 'a'), edge('gt_gate0', 'result', 'gt_pAng', 't'),
      edge('gt_rx', 'read', 'gt_pX', 'a'),   edge('gt_gate0', 'result', 'gt_pX', 't'),
      edge('gt_ry', 'read', 'gt_pY', 'a'),   edge('gt_gate0', 'result', 'gt_pY', 't'),
      // new angle = prevAngle + turn
      edge('gt_pAng', 'result', 'gt_nAng', 'a'),
      edge('gt_turn', 'result', 'gt_nAng', 'b'),
      // length = TRUNK · SCALE^seg
      edge('gt_seg', 'result', 'gt_lpow', 'b'),
      edge('gt_lpow', 'result', 'gt_len', 'b'),
      // displacement
      edge('gt_nAng', 'result', 'gt_sin', 'x'),
      edge('gt_nAng', 'result', 'gt_cos', 'x'),
      edge('gt_len', 'result', 'gt_dx', 'a'), edge('gt_sin', 'result', 'gt_dx', 'b'),
      edge('gt_cos', 'result', 'gt_ncos', 'x'),
      edge('gt_len', 'result', 'gt_dy', 'a'), edge('gt_ncos', 'result', 'gt_dy', 'b'),
      // advance
      edge('gt_pX', 'result', 'gt_nX', 'a'), edge('gt_dx', 'result', 'gt_nX', 'b'),
      edge('gt_pY', 'result', 'gt_nY', 'a'), edge('gt_dy', 'result', 'gt_nY', 'b'),
      // thread accumulators forward
      edge('gt_nAng', 'result', 'gt_wa', 'write'),
      edge('gt_nX', 'result', 'gt_wx', 'write'),
      edge('gt_nY', 'result', 'gt_wy', 'write'),
      // taper width = max(1, LEVELS - seg)
      edge('gt_seg', 'result', 'gt_wsub', 'b'),
      edge('gt_wsub', 'result', 'gt_wmax', 'a'),
      edge('gt_stroke', 'style', 'gt_swt', 'base'),
      edge('gt_wmax', 'result', 'gt_swt', 'w'),
      // draw line prev → new
      edge('gt_pX', 'result', 'gt_a', 'x'), edge('gt_pY', 'result', 'gt_a', 'y'),
      edge('gt_nX', 'result', 'gt_b', 'x'), edge('gt_nY', 'result', 'gt_b', 'y'),
      edge('gt_a', 'vector', 'gt_line', 'a'),
      edge('gt_b', 'vector', 'gt_line', 'b'),
      edge('gt_swt', 'style', 'gt_line', 'style'),
      edge('gt_line', 'shape', 'gt_out', 'inner'),
      edge('gt_out', 'outer', 'gt_renderer', 'layers'),
    ],
  };
}
