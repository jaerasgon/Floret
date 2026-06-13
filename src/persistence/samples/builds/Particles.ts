import { node, edge, note } from '../helpers';


// ─── Particles / Fountain ─────────────────────────────────────────────────
// Simplest particle system — a single-point emitter, gravity from the
// World, drag for terminal velocity, and a wrap-around boundary so the
// canvas keeps cycling. The "hello, particles" patch.
export function buildParticlesFountain() {
  const c = note(
    'pf_c',
    { x: -540, y: 0 },
    'Particle fountain\n\nA Particles patch is four nodes at minimum: time/seconds → Emitter, an Emitter to spawn from, a World to integrate against, and a Render-As to draw the live buffer. Gravity lives on the World and applies to every Emitter wired to it.\n\nDrag tames terminal velocity; the wrap-bounds keep the canvas cycling.',
    { w: 500, h: 260 },
  );
  const time  = node('pf_time',  'time/seconds', { x: -540, y: 240 });
  const drag  = node('pf_drag',  'force/drag',   { x: -340, y: 240 }, { strength: 0.3 });
  const bnds  = node('pf_bnds',  'particles/bounds', { x: -340, y: 320 },
                     { size: { x: 520, y: 520 }, mode: 'wrap' });
  const world = node('pf_world', 'physics/world',{ x: -140, y: 240 },
                     { gravity: { x: 0, y: 380 }, drag: 0.05, integrator: 'euler' });
  const emit  = node('pf_em',    'particles/emitter', { x:  140, y:  40 },
                     { rate: 240, lifetime: 4, initialVel: { x: 0, y: -340 }, velSpread: 0.35,
                       color: { r: 1, g: 0.85, b: 0.4, a: 1 }, size: 5,
                       maxCount: 1500, overflowMode: 'drop', seed: 7 });
  const render= node('pf_render','particles/render-as', { x: 440, y: 40 },
                     { template: 'circle', useBufferColor: true });
  const bg    = node('pf_bg',    'color/color',  { x: 140, y: 220 },
                     { mode: 'rgb', r: 12, g: 12, b: 22, a: 255 });
  const renderer = node('pf_renderer','render/renderer', { x: 720, y: 0 },
                        { width: 540, height: 540 });
  return {
    nodes: [c, time, drag, bnds, world, emit, render, bg, renderer],
    edges: [
      edge('pf_drag', 'force',     'pf_world',   'forces'),
      edge('pf_bnds', 'collider',  'pf_world',   'colliders'),
      edge('pf_world','world',     'pf_em',      'world'),
      edge('pf_time', 't',         'pf_em',      'time'),
      edge('pf_em',   'particles', 'pf_render',  'particles'),
      edge('pf_render', 'layer',      'pf_renderer','layers'),
      edge('pf_bg', 'color',       'pf_renderer','background'),
    ],
  };
}


// ─── Particles / Text Dust ────────────────────────────────────────────────
// Showcases the text rasteriser in particles/from-layer — particles spawn
// inside the actual glyph paths of "FLORET" (not the bounding box). Slow
// upward drift + turbulence make the letters dissolve into smoke.
export function buildParticlesTextDust() {
  const c = note(
    'pt_c',
    { x: -560, y: 0 },
    'Text dust\n\nparticles/from-layer rasterises the wired text shape to an offscreen canvas and samples inside the alpha mask. The result is a Spread<vec2> of points INSIDE the glyphs — wire it into the Emitter\'s positions input and particles spawn from the letters themselves.\n\nTurbulence + slow upward drift turn the letters into smoke.',
    { w: 500, h: 260 },
  );
  const time  = node('pt_time',  'time/seconds', { x: -540, y: 240 });
  const str   = node('pt_str',   'value/string', { x: -540, y: 320 }, { value: 'FLORET' });
  const fillC = node('pt_fillC', 'color/color',  { x: -340, y: 320 },
                     { mode: 'rgb', r: 255, g: 255, b: 255, a: 255 });
  const fill  = node('pt_fill',  'style/fill',   { x: -140, y: 320 });
  const tp    = node('pt_tp',    'style/text-params', { x:  60, y: 320 },
                     { font: 'sans-serif', bold: true, align: 'center', baseline: 'middle' });
  const text  = node('pt_text',  'draw/text',    { x: 260, y: 320 },
                     { x: 0, y: 0, size: 120, align: 'center' });
  const fromL = node('pt_fromL', 'particles/from-layer', { x: 460, y: 320 },
                     { mode: 'fill', density: 320 });

  const turb  = node('pt_turb',  'force/turbulence', { x: -340, y: 460 },
                     { scale: 0.015, speed: 0.3, strength: 30, seed: 3 });
  const world = node('pt_world', 'physics/world', { x: -140, y: 460 },
                     { gravity: { x: 0, y: -12 }, drag: 0.5, integrator: 'euler' });

  const emit  = node('pt_em',    'particles/emitter', { x: 700, y: 100 },
                     { rate: 600, lifetime: 5,
                       initialVel: { x: 0, y: 0 }, velSpread: 0.04,
                       color: { r: 1, g: 0.92, b: 0.78, a: 0.9 }, size: 1.8,
                       maxCount: 3500, overflowMode: 'drop', seed: 11 });
  const render= node('pt_render','particles/render-as', { x: 1000, y: 100 },
                     { template: 'circle', useBufferColor: true });
  const bg    = node('pt_bg',    'color/color', { x: 700, y: 240 },
                     { mode: 'rgb', r: 8, g: 8, b: 16, a: 255 });
  const renderer = node('pt_renderer','render/renderer', { x: 1240, y: 0 },
                        { width: 560, height: 380 });
  return {
    nodes: [c, time, str, fillC, fill, tp, text, fromL, turb, world,
            emit, render, bg, renderer],
    edges: [
      edge('pt_str', 'text','pt_text','string'),
      edge('pt_fillC', 'color','pt_fill','color'),
      edge('pt_fill','style','pt_tp','base'),
      edge('pt_tp','style','pt_text','style'),
      edge('pt_text', 'layer','pt_fromL','layer'),
      edge('pt_fromL','positions','pt_em','positions'),
      edge('pt_turb','force','pt_world','forces'),
      edge('pt_world','world','pt_em','world'),
      edge('pt_time','t','pt_em','time'),
      edge('pt_em','particles','pt_render','particles'),
      edge('pt_render', 'layer','pt_renderer','layers'),
      edge('pt_bg', 'color','pt_renderer','background'),
    ],
  };
}


// ─── Particles / Attractor Field ──────────────────────────────────────────
// A persistent cloud — turbulence keeps particles wandering, a central
// attractor with visible falloff ring keeps them from leaving. The
// attractor's `viz` output is wired into the renderer so you can SEE the
// influence region.
export function buildParticlesAttractor() {
  const c = note(
    'pa_c',
    { x: -540, y: 0 },
    'Attractor field\n\nThree forces wired into the World: turbulence (perlin field), drag (terminal velocity), and an attractor pulling everything toward (0, 0).\n\nThe attractor\'s `viz` output draws its influence ring — toggle `showViz` to debug placement.',
    { w: 500, h: 260 },
  );
  const time  = node('pa_time',  'time/seconds', { x: -540, y: 240 });
  const turb  = node('pa_turb',  'force/turbulence', { x: -340, y: 200 },
                     { scale: 0.005, speed: 0.4, strength: 140, seed: 5 });
  const drag  = node('pa_drag',  'force/drag',   { x: -340, y: 280 }, { strength: 1.6 });
  const attr  = node('pa_attr',  'force/attractor', { x: -340, y: 360 },
                     { center: { x: 0, y: 0 }, strength: 240, radius: 360,
                       falloff: 'linear', showViz: true });
  const world = node('pa_world', 'physics/world',{ x: -140, y: 280 },
                     { gravity: { x: 0, y: 0 }, drag: 0.0, integrator: 'euler' });
  // Spawn positions: a noisy ring so the cloud has spread to start.
  const ring  = node('pa_ring',  'spread/ring', { x: -140, y: 80 },
                     { count: 60, radius: 180 });
  const emit  = node('pa_em',    'particles/emitter', { x: 140, y: 80 },
                     { rate: 600, lifetime: 6,
                       initialVel: { x: 0, y: 0 }, velSpread: 0.0,
                       color: { r: 0.6, g: 0.9, b: 1, a: 0.9 }, size: 2.5,
                       maxCount: 2400, overflowMode: 'replace-oldest', seed: 13 });
  const render= node('pa_render','particles/render-as', { x: 440, y: 80 },
                     { template: 'circle', useBufferColor: true });
  const bg    = node('pa_bg',    'color/color', { x: 140, y: 220 },
                     { mode: 'rgb', r: 8, g: 10, b: 24, a: 255 });
  const renderer = node('pa_renderer','render/renderer', { x: 720, y: 0 },
                        { width: 540, height: 540 });
  return {
    nodes: [c, time, turb, drag, attr, world, ring, emit, render, bg, renderer],
    edges: [
      edge('pa_turb','force','pa_world','forces'),
      edge('pa_drag','force','pa_world','forces'),
      edge('pa_attr','force','pa_world','forces'),
      // Also draw the attractor's viz ring straight into the renderer.
      edge('pa_attr','viz','pa_renderer','layers'),
      edge('pa_ring','positions','pa_em','positions'),
      edge('pa_world','world','pa_em','world'),
      edge('pa_time','t','pa_em','time'),
      edge('pa_em','particles','pa_render','particles'),
      edge('pa_render', 'layer','pa_renderer','layers'),
      edge('pa_bg', 'color','pa_renderer','background'),
    ],
  };
}


// ─── Particles / Polygon Pinball ──────────────────────────────────────────
// An octagonal "arena" built from a polygon shape doubles as a renderable
// outline AND a collider — collider/from-layer over the same polygon
// reflects particles off its edge normals (not centroid normals).
export function buildParticlesPinball() {
  const c = note(
    'pp_c',
    { x: -540, y: 0 },
    'Polygon pinball\n\ncollider/from-layer accepts any shape layer. Here a draw/polygon octagon is wired into BOTH the renderer (so you can see it) AND the collider (so it deflects particles). The collider precomputes edge normals at compile and uses them for proper bounce — not a centroid approximation.',
    { w: 500, h: 280 },
  );
  const time = node('pp_time', 'time/seconds', { x: -540, y: 260 });
  // Polygon arena — an octagon traced by spread/ring (8 Vec2 points on a
  // circle), wired straight into draw/polygon's `points` spread.
  const ring = node('pp_ring', 'spread/ring', { x: -540, y: 360 },
                    { count: 8, radius: 220, phase: Math.PI / 2 });
  const polyStroke = node('pp_polyC', 'color/color', { x: -340, y: 200 },
                          { mode: 'rgb', r: 220, g: 220, b: 240, a: 255 });
  const polyStr  = node('pp_polyS', 'style/stroke', { x: -140, y: 200 },
                        { width: 2 });
  const noFill   = node('pp_polyN', 'style/no-fill', { x:  60, y: 200 });
  const poly = node('pp_poly', 'draw/polygon', { x: 260, y: 260 }, { closed: true });
  const coll = node('pp_coll', 'collider/from-layer', { x: 480, y: 260 },
                    { mode: 'bounce', restitution: 0.85, inverted: true });
  const world= node('pp_world', 'physics/world', { x: 700, y: 260 },
                    { gravity: { x: 0, y: 280 }, drag: 0.02, integrator: 'euler' });
  // Per-spawn hue cycling — each ball gets a distinct colour, and its
  // trail (useBufferColor=true) inherits the same hue.
  const tHue = node('pp_tHue', 'math/mul', { x: 700, y: 200 }, { b: 540 });
  const hsl  = node('pp_hsl',  'color/color', { x: 880, y: 200 },
                    { mode: 'hsl', h: 0, s: 90, l: 60, a: 255 });
  const emit = node('pp_em',    'particles/emitter', { x: 920, y: 40 },
                    { rate: 60, lifetime: 12,
                      initialVel: { x: 200, y: -200 }, velSpread: 0.6,
                      size: 6,
                      maxCount: 220, overflowMode: 'drop', seed: 19 });
  const render = node('pp_render','particles/render-as', { x: 1180, y: 40 },
                      { template: 'circle', useBufferColor: true });
  const trailW = node('pp_trailW', 'style/stroke-weight', { x: 1000, y: 160 }, { w: 2.5 });
  const trail  = node('pp_trail', 'particles/trail', { x: 1180, y: 160 },
                      { segments: 18, fadeOut: true, useBufferColor: true });
  const bg = node('pp_bg', 'color/color', { x: 920, y: 200 },
                  { mode: 'rgb', r: 6, g: 10, b: 18, a: 255 });
  const renderer = node('pp_renderer', 'render/renderer', { x: 1420, y: 0 },
                        { width: 560, height: 560 });
  return {
    nodes: [c, time, ring, polyStroke, polyStr, noFill, poly, coll, world,
            tHue, hsl, emit, render, trailW, trail, bg, renderer],
    edges: [
      // Build the octagon: 8 Vec2 points → polygon
      edge('pp_ring','positions','pp_poly','points'),
      // Trail stroke weight chained into the trail's style input
      edge('pp_trailW','style','pp_trail','style'),
      // Per-spawn hue: time → mul → HSL → emitter.color
      edge('pp_time','t','pp_tHue','a'),
      edge('pp_tHue', 'product','pp_hsl','h'),
      edge('pp_hsl', 'color','pp_em','color'),
      // Outline style
      edge('pp_polyC', 'color','pp_polyS','color'),
      edge('pp_polyS','style','pp_polyN','base'),
      edge('pp_polyN','style','pp_poly','style'),
      // Polygon doubles as collider AND visible outline
      edge('pp_poly', 'shape','pp_coll','layer'),
      edge('pp_poly', 'shape','pp_renderer','layers'),
      edge('pp_coll','collider','pp_world','colliders'),
      edge('pp_world','world','pp_em','world'),
      edge('pp_time','t','pp_em','time'),
      edge('pp_em','particles','pp_trail','particles'),
      edge('pp_em','particles','pp_render','particles'),
      edge('pp_trail', 'layer','pp_renderer','layers'),
      edge('pp_render', 'layer','pp_renderer','layers'),
      edge('pp_bg', 'color','pp_renderer','background'),
    ],
  };
}


// ─── Particles / Galactic Spiral ──────────────────────────────────────────
// Verlet integrator showcase — a vortex + central pull keeps particles in
// long stable spirals that Euler would visibly drift on. Per-particle
// trails carry the buffer colour so each filament keeps its hue.
export function buildParticlesSpiral() {
  const c = note(
    'ps_c',
    { x: -540, y: 0 },
    'Galactic spiral\n\nphysics/world.integrator is set to VERLET — two-pass integration that preserves long-running orbits much better than Euler. Vortex + central attractor produce stable spirals; trails (per-particle buffer colour, fadeOut) drag the colour through space.\n\nFlip the integrator to euler and watch the particles drift outward over time.',
    { w: 500, h: 280 },
  );
  const time = node('ps_time', 'time/seconds', { x: -540, y: 280 });
  const vortex = node('ps_vortex', 'force/vortex', { x: -340, y: 240 },
                      { center: { x: 0, y: 0 }, axisStrength: 900, pull: 80,
                        radius: 400, falloff: 'constant', showViz: false });
  const attr = node('ps_attr', 'force/attractor', { x: -340, y: 340 },
                    { center: { x: 0, y: 0 }, strength: 600, radius: 400,
                      falloff: 'linear', showViz: false });
  const world = node('ps_world', 'physics/world', { x: -140, y: 300 },
                     { gravity: { x: 0, y: 0 }, drag: 0.02, integrator: 'verlet' });
  // Per-spawn colour: time × 720 means hue cycles twice per second, so
  // successive spawns ~16 ms apart get hues 11° apart — distinct enough
  // that adjacent filaments read as different colours.
  const tHue = node('ps_tHue', 'math/mul', { x: -540, y: 200 }, { b: 720 });
  const hsl  = node('ps_hsl',  'color/color', { x: -340, y: 160 },
                    { mode: 'hsl', h: 0, s: 90, l: 60, a: 255 });
  // Spawn from a ring around the centre + velocity jitter so particles
  // distribute across the spiral instead of stacking on one path.
  const ring = node('ps_ring', 'spread/ring', { x: -140, y: 60 },
                    { count: 80, radius: 220 });
  const emit = node('ps_em',   'particles/emitter', { x: 140, y: 40 },
                    { rate: 240, lifetime: 9,
                      initialVel: { x: 0, y: 0 }, velSpread: 0.2,
                      size: 2.0,
                      maxCount: 1800, overflowMode: 'replace-oldest', seed: 23 });
  const render = node('ps_render', 'particles/render-as', { x: 440, y: 40 },
                      { template: 'circle', useBufferColor: true });
  const trailW = node('ps_trailW', 'style/stroke-weight', { x: 280, y: 160 }, { w: 3 });
  const trail  = node('ps_trail',  'particles/trail',     { x: 440, y: 160 },
                      { segments: 32, fadeOut: true, useBufferColor: true,
                        tapered: true, tailWidth: 0.05 });
  const bg = node('ps_bg', 'color/color', { x: 140, y: 200 },
                  { mode: 'rgb', r: 4, g: 4, b: 12, a: 255 });
  const renderer = node('ps_renderer', 'render/renderer', { x: 720, y: 0 },
                        { width: 600, height: 600 });
  // Remove the unused ring placeholder above; just leave the other nodes.
  return {
    nodes: [c, time, vortex, attr, world, tHue, hsl, ring, emit, render, trailW, trail, bg, renderer],
    edges: [
      edge('ps_time','t','ps_tHue','a'),
      edge('ps_tHue', 'product','ps_hsl','h'),
      edge('ps_hsl', 'color','ps_em','color'),
      edge('ps_vortex','force','ps_world','forces'),
      edge('ps_attr','force','ps_world','forces'),
      edge('ps_world','world','ps_em','world'),
      edge('ps_ring','positions','ps_em','positions'),
      edge('ps_time','t','ps_em','time'),
      edge('ps_em','particles','ps_trail','particles'),
      edge('ps_em','particles','ps_render','particles'),
      edge('ps_trailW','style','ps_trail','style'),
      edge('ps_trail', 'layer','ps_renderer','layers'),
      edge('ps_render', 'layer','ps_renderer','layers'),
      edge('ps_bg', 'color','ps_renderer','background'),
    ],
  };
}
