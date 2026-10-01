// Regenerate the reference renders the floret-render tests diff against.
//
// Renders scenes with the web app's real WebGL2 renderer (src/runtime/gl/,
// bundled from harness.ts) in headless Chromium, and writes:
//   scenes/<name>.json   synthetic scenes (TS SceneNode JSON, styles expanded)
//   png/<name>.png       the TS renderer's output for each synthetic scene
//   png/<sample>-f<i>.png  …and for recorded trace scenes (fixtures/traces)
//
// Usage (from the repo root, after `npm install`; `ONLY=name,…` limits it):
//   PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers \
//   PLAYWRIGHT_MODULE=/path/to/node_modules/playwright/index.mjs \
//   node native/crates/floret-render/reference/generate.mjs
//
// PLAYWRIGHT_MODULE defaults to the `playwright` package resolvable from here.
// REFERENCE_ONLY=<name,...> renders just those scenes.

import { build } from 'esbuild';
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const traces = join(here, '../../../fixtures/traces');
mkdirSync(join(here, 'scenes'), { recursive: true });
mkdirSync(join(here, 'png'), { recursive: true });

// ── Synthetic scenes ────────────────────────────────────────────────────────

const DEFAULT_STYLE = {
  fill: '#ffffff', stroke: null, strokeWeight: 1, strokeCap: 'butt', strokeJoin: 'miter',
  strokeAlign: 'center', blur: 0, fontSize: 14, fontFamily: 'sans-serif', fontWeight: 'normal',
  fontStyle: 'normal', letterSpacing: 0, lineHeight: 1.2, textAlign: 'left',
  textBaseline: 'alphabetic', rectMode: 'corner', ellipseMode: 'center',
};
const st = (s = {}) => ({ ...DEFAULT_STYLE, ...s });
const rect = (x, y, w, h, s, mode = 'corner') => ({ kind: 'rect', x, y, w, h, mode, style: st(s) });
const ellipse = (x, y, w, h, s, mode = 'center') => ({ kind: 'ellipse', x, y, w, h, mode, style: st(s) });
const line = (x1, y1, x2, y2, s) => ({ kind: 'line', x1, y1, x2, y2, style: st({ fill: null, stroke: '#000000', ...s }) });
const poly = (pts, closed, s, tension) => ({
  kind: 'polygon', points: pts.map(([x, y]) => ({ x, y })), closed, style: st(s),
  ...(tension === undefined ? {} : { tension }),
});
const text = (t, x, y, s) => ({ kind: 'text', text: t, x, y, style: st(s) });
const group = (children, extra = {}) => ({ kind: 'group', children, ...extra });
// An image source as inline pixels (harness.ts turns it into a canvas):
// a 16×16 gradient whose bottom-right quadrant is half transparent.
const gradient = (() => {
  const rgba = [];
  for (let y = 0; y < 16; y++) for (let x = 0; x < 16; x++) {
    rgba.push(x * 16, y * 16, 255 - x * 8, x >= 8 && y >= 8 ? 128 : 255);
  }
  return { w: 16, h: 16, rgba };
})();
const image = (src, dst, tint = null) => ({ kind: 'image', source: gradient, src, dst, tint });
const star = (cx, cy, r1, r2, n) => Array.from({ length: n * 2 }, (_, i) => {
  const a = (i / (n * 2)) * Math.PI * 2 - Math.PI / 2, r = i % 2 ? r2 : r1;
  return [cx + Math.cos(a) * r, cy + Math.sin(a) * r];
});

// A particle buffer as plain arrays (the TS renderer only indexes them).
// `at(i)` → { x, y, size, rgba } or null for a dead slot.
function particleBuffer(capacity, at) {
  const b = { capacity, count: 0, alive: [], posX: [], posY: [], size: [], colorR: [], colorG: [], colorB: [], colorA: [] };
  for (let i = 0; i < capacity; i++) {
    const p = at(i);
    b.alive.push(p ? 1 : 0);
    b.posX.push(p?.x ?? 0); b.posY.push(p?.y ?? 0); b.size.push(p?.size ?? 0);
    const [r, g, bl, a] = p?.rgba ?? [0, 0, 0, 0];
    b.colorR.push(r); b.colorG.push(g); b.colorB.push(bl); b.colorA.push(a);
    if (p) b.count++;
  }
  return b;
}
const particleBatch = (buffer, s, useBufferColor = true, template = 'circle') =>
  ({ kind: 'particle-batch', buffer, count: buffer.count, template: { kind: template }, style: st(s), useBufferColor });
// Trails as TS's GL path draws them: flat (no fade / taper), no gaps.
function trailBatch(buffer, segments, headIdx, path, s, useBufferColor) {
  const history = [];
  for (let slot = 0; slot < buffer.capacity; slot++) {
    for (let seg = 0; seg < segments; seg++) {
      const age = (headIdx - seg + segments) % segments; // 0 = newest
      const [x, y] = path(slot, age);
      history.push(x, y);
    }
  }
  return { kind: 'particle-trail-batch', buffer, history, segments, headIdx, style: st(s), fadeOut: false, useBufferColor, tapered: false, tailWidth: 0.1 };
}
const spiral = particleBuffer(240, (i) => {
  if (i % 7 === 3) return null;
  const a = i * 0.21, r = 8 + i * 0.58;
  return {
    x: Math.cos(a) * r, y: Math.sin(a) * r, size: i % 23 === 0 ? 0 : 1 + (i % 5),
    rgba: [0.5 + 0.5 * Math.sin(i * 0.1), 0.5 + 0.5 * Math.sin(i * 0.1 + 2), 0.5 + 0.5 * Math.sin(i * 0.1 + 4), 0.35 + (i % 3) * 0.3],
  };
});
const grid = particleBuffer(36, (i) => ({ x: (i % 6) * 12 - 30, y: Math.floor(i / 6) * 12 - 30, size: 3, rgba: [1, 0, 0, 1] }));
const glow = particleBuffer(12, (i) => ({ x: Math.cos(i * 0.52) * 40, y: Math.sin(i * 0.52) * 40, size: 14, rgba: [0.2, 0.4, 1, 0.5] }));
const movers = particleBuffer(24, (i) => (i === 5 ? null : {
  x: 0, y: 0, size: 2, rgba: [1, 0.3 + (i % 4) * 0.2, 0.2, i % 2 ? 1 : 0.6],
}));
const wave = (slot, age) => [-140 + age * 9 + (slot % 4) * 4, -130 + slot * 11 + Math.sin(age * 0.6 + slot) * 6];

const SYNTHETIC = {
  particles: {
    width: 320, height: 320, background: [0.04, 0.04, 0.06],
    layers: [
      particleBatch(spiral, {}),
      group([particleBatch(grid, { fill: 'rgba(255,200,80,0.8)' }, false)], { transform: [1.5, 0, 0, 1.5, 100, -100] }),
      group([particleBatch(glow, {})], { blendMode: 'lighter', transform: [1, 0, 0, 1, -90, 100] }),
    ],
  },
  'particle-trails': {
    width: 320, height: 320, background: [0.04, 0.04, 0.06],
    layers: [
      trailBatch(movers, 12, 5, wave, { stroke: '#40c0ff', strokeWeight: 3 }, false),
      group([trailBatch(movers, 8, 2, wave, { stroke: '#ffffff', strokeWeight: 2 }, true)], { transform: [1, 0, 0, 1, 150, 20] }),
    ],
  },

  shapes: {
    width: 320, height: 320, background: [0.08, 0.08, 0.1],
    layers: [
      rect(-150, -150, 80, 50, { fill: '#e04040', stroke: '#ffffff', strokeWeight: 4 }),
      rect(-50, -150, 80, 50, { fill: '#40a0e0', stroke: '#ffd040', strokeWeight: 6, strokeAlign: 'inside' }),
      rect(50, -150, 80, 50, { fill: '#40e080', stroke: '#ffd040', strokeWeight: 6, strokeAlign: 'outside' }),
      ellipse(-110, -50, 80, 50, { fill: '#a060ff', stroke: '#ffffff', strokeWeight: 3 }),
      ellipse(-10, -50, 60, 60, { fill: null, stroke: '#ff8020', strokeWeight: 5, strokeAlign: 'inside' }),
      rect(90, -50, -40, -30, { fill: '#ffffff' }, 'center'),
      { kind: 'arc', x: -110, y: 50, w: 80, h: 80, start: 0, stop: 4, mode: 'center', style: st({ fill: '#2080ff', stroke: '#ffffff', strokeWeight: 3 }) },
      { kind: 'arc', x: -10, y: 50, w: 80, h: 60, start: -2, stop: 1.5, mode: 'center', style: st({ fill: null, stroke: '#ff4080', strokeWeight: 6 }) },
      { kind: 'triangle', x1: 50, y1: 90, x2: 140, y2: 90, x3: 95, y3: 10, style: st({ fill: '#f0c020', stroke: '#802020', strokeWeight: 4 }) },
      { kind: 'quad', x1: -150, y1: 110, x2: -60, y2: 100, x3: -70, y3: 150, x4: -140, y4: 145, style: st({ fill: '#30c0c0', stroke: null }) },
      { kind: 'point', x: -20, y: 130, style: st({ stroke: '#ffffff', strokeWeight: 8 }) },
      { kind: 'point', x: 0, y: 130, style: st({ stroke: '#ff0000', strokeWeight: 1 }) },
      { kind: 'point', x: 20, y: 130, style: st({ fill: '#00ff00', stroke: null, strokeWeight: 4 }) },
      line(40, 120, 150, 150, { stroke: '#ffffff', strokeWeight: 7 }),
      line(40, 140, 150, 110, { stroke: 'rgba(80,160,255,0.6)', strokeWeight: 3 }),
    ],
  },
  polygons: {
    width: 320, height: 320, background: [0.1, 0.1, 0.12],
    layers: [
      poly(star(-80, -80, 70, 28, 5), true, { fill: '#f0a020', stroke: '#ffffff', strokeWeight: 2 }),
      // Self-intersecting pentagram: nonzero winding fills the centre too.
      poly([0, 1, 2, 3, 4].map((i) => {
        const a = (i * 4 / 5) * Math.PI - Math.PI / 2;
        return [80 + Math.cos(a) * 70, -80 + Math.sin(a) * 70];
      }), true, { fill: '#40c0ff', stroke: null }),
      poly([[-150, 30], [-30, 50], [-80, 140], [-120, 90]], true, { fill: '#a0e060', stroke: '#204010', strokeWeight: 5 }, 1),
      poly([[10, 140], [40, 30], [80, 120], [110, 40], [150, 140]], false, { fill: null, stroke: 'rgba(255,90,90,0.6)', strokeWeight: 14 }),
      poly([[10, 20], [60, 0], [110, 20], [150, 0]], false, { fill: null, stroke: '#ffffff', strokeWeight: 2 }, 0.8),
    ],
  },
  blend: {
    width: 320, height: 320, background: [0.0, 0.0, 0.0],
    layers: [
      rect(-160, 0, 320, 160, { fill: '#808080' }),
      ...['source-over', 'lighter', 'multiply', 'screen', 'darken'].map((mode, i) => group([
        ellipse(-15, -5, 50, 50, { fill: 'rgb(255,60,60)' }),
        ellipse(15, -5, 50, 50, { fill: 'rgba(60,255,60,0.8)' }),
        ellipse(0, 20, 50, 50, { fill: 'rgb(60,60,255)' }),
      ], { transform: [1, 0, 0, 1, -128 + i * 64, (i % 2 ? 60 : -60)], blendMode: mode })),
    ],
  },
  clip: {
    width: 320, height: 320, background: [0.05, 0.05, 0.08],
    layers: [
      group([ellipse(-80, -80, 140, 140, { fill: '#ff8040' })], { clip: { x: -140, y: -140, w: 80, h: 100 } }),
      group([
        group([rect(0, 0, 200, 200, { fill: '#40a0ff' }, 'center')], { clip: { x: 20, y: -150, w: 120, h: 60 } }),
      ], { clip: { x: 60, y: -160, w: 100, h: 120 } }),
      group([ellipse(0, 0, 40, 40, { fill: '#80ff80', stroke: '#ffffff', strokeWeight: 3 })],
        { transform: [2, 0, 0, 1.5, -60, 80], clip: { x: -20, y: -20, w: 30, h: 40 } }),
      line(-160, 0, 160, 0, { stroke: '#444444', strokeWeight: 1 }),
    ],
  },
  // TS scissors a rotated clip's AABB; floret-render clips exactly.
  'clip-rotated': {
    width: 320, height: 320, background: [0.05, 0.05, 0.08],
    layers: [
      group([ellipse(0, 0, 220, 220, { fill: '#ff60a0' })],
        { transform: [Math.cos(0.5), Math.sin(0.5), -Math.sin(0.5), Math.cos(0.5), 0, 0], clip: { x: -60, y: -80, w: 120, h: 160 } }),
    ],
  },
  blur: {
    width: 320, height: 320, background: [0.06, 0.06, 0.08],
    layers: [
      ...[0, 2, 6, 14].map((b, i) => ellipse(-120 + i * 80, -90, 50, 50, { fill: '#ffd040', blur: b })),
      rect(-140, -10, 120, 40, { fill: '#40c0ff', stroke: '#ffffff', strokeWeight: 4, blur: 4 }),
      group([rect(0, 0, 30, 30, { fill: '#ff5050', blur: 3 }, 'center')], { transform: [3, 0, 0, 3, 80, 10] }),
      line(-140, 80, 140, 120, { stroke: '#a0ff80', strokeWeight: 8, blur: 5 }),
      poly(star(0, 120, 30, 14, 6), true, { fill: '#c080ff', blur: 2 }),
    ],
  },
  alpha: {
    width: 320, height: 320, background: [0.9, 0.9, 0.9],
    layers: [
      ellipse(-30, -30, 160, 160, { fill: 'rgba(255,0,0,0.5)' }),
      ellipse(30, -30, 160, 160, { fill: 'rgba(0,160,0,0.5)' }),
      ellipse(0, 30, 160, 160, { fill: 'rgba(0,0,255,0.5)' }),
      rect(-140, 100, 280, 40, { fill: 'rgba(0,0,0,0.25)', stroke: 'rgba(0,0,0,0.6)', strokeWeight: 6 }),
      poly([[-150, -150], [150, -150], [0, -60]], true, { fill: 'rgba(255,200,0,0.4)', stroke: null }),
    ],
  },
  shear: {
    width: 320, height: 320, background: [0.08, 0.08, 0.1],
    layers: [
      group([
        rect(-40, -40, 80, 80, { fill: '#30a0ff', stroke: '#ffffff', strokeWeight: 4 }),
        ellipse(0, 0, 50, 50, { fill: '#ff8040' }),
        line(-60, 0, 60, 0, { stroke: '#ffffff', strokeWeight: 3 }),
      ], { transform: [1.2, 0.3, 0.6, 1.1, -40, -20] }),
      group([group([rect(0, 0, 40, 20, { fill: '#a0ff60' }, 'center')], { transform: [Math.cos(1), Math.sin(1), -Math.sin(1), Math.cos(1), 0, 0] })],
        { transform: [2, 0, 0, 2, 90, 100] }),
    ],
  },
  text: {
    width: 320, height: 320, background: [0.05, 0.05, 0.07],
    layers: [
      line(-160, -120, 160, -120, { stroke: '#404040' }),
      line(0, -160, 0, 160, { stroke: '#404040' }),
      text('Left', 0, -120, { fill: '#ffffff', fontSize: 18 }),
      text('Center', 0, -90, { fill: '#ffd040', fontSize: 18, textAlign: 'center' }),
      text('Right', 0, -60, { fill: '#40c0ff', fontSize: 18, textAlign: 'right' }),
      text('top', -150, -30, { fill: '#ffffff', textBaseline: 'top' }),
      text('middle', -100, -30, { fill: '#ffffff', textBaseline: 'middle' }),
      text('bottom', -40, -30, { fill: '#ffffff', textBaseline: 'bottom' }),
      text('Bold', -150, 20, { fill: '#ff8080', fontSize: 24, fontWeight: 'bold' }),
      text('Italic', -60, 20, { fill: '#80ff80', fontSize: 24, fontStyle: 'italic' }),
      text('Mono', 40, 20, { fill: '#8080ff', fontSize: 24, fontFamily: 'monospace' }),
      text('Two\nlines', -150, 60, { fill: '#ffffff', fontSize: 16 }),
      text('Stroked', -60, 80, { fill: '#202020', stroke: '#ffd040', strokeWeight: 2, fontSize: 30 }),
      group([text('Rotated', 0, 0, { fill: 'rgba(255,255,255,0.7)', fontSize: 20, textAlign: 'center' })],
        { transform: [Math.cos(-0.6), Math.sin(-0.6), -Math.sin(-0.6), Math.cos(-0.6), 90, 120] }),
    ],
  },
  image: {
    width: 320, height: 320, background: [0.1, 0.1, 0.12],
    layers: [
      image(null, { x: -150, y: -150, w: 128, h: 128 }),
      image({ x: 4, y: 2, w: 8, h: 10 }, { x: 10, y: -150, w: 100, h: 120 }, 'rgba(255,160,0,0.8)'),
      group([image(null, { x: -40, y: -40, w: 80, h: 80 })],
        { transform: [Math.cos(0.4), Math.sin(0.4), -Math.sin(0.4), Math.cos(0.4), -80, 80] }),
      rect(20, 20, 130, 130, { fill: '#808080' }),
      group([image(null, { x: 40, y: 40, w: 90, h: 90 })], { blendMode: 'multiply' }),
      group([image(null, { x: 0, y: 0, w: 160, h: 160 })], { clip: { x: 30, y: 30, w: 40, h: 40 } }),
    ],
  },
};
const only = process.env.REFERENCE_ONLY?.split(',');
const wanted = (name) => !only || only.includes(name);

// ── Trace scenes ────────────────────────────────────────────────────────────

const SUPPORTED = new Set(['rect', 'ellipse', 'line', 'arc', 'triangle', 'quad', 'point', 'polygon', 'text', 'group']);

function resolveRefs(outputs) {
  const refs = new Map();
  const dec = (j) => {
    if (Array.isArray(j)) return j.map(dec);
    if (j && typeof j === 'object') {
      if ('$ref' in j) return refs.get(j.$ref) ?? null;
      if ('$u' in j && Object.keys(j).length === 1) return null;
      if ('$num' in j) return j.$num === 'Infinity' ? Infinity : j.$num === '-Infinity' ? -Infinity : NaN;
      const tagged = Object.keys(j).some((k) => k.startsWith('$') && k !== '$id' && k !== '$arr' && k !== '$class');
      let out;
      if ('$arr' in j) out = j.$arr.map(dec);
      else if (tagged) out = null;
      else out = Object.fromEntries(Object.entries(j).filter(([k]) => !k.startsWith('$')).map(([k, v]) => [k, dec(v)]));
      if ('$id' in j) refs.set(j.$id, out);
      return out;
    }
    return j;
  };
  const res = {};
  for (const [k, v] of Object.entries(outputs)) res[k] = dec(v);
  return res;
}

// Keep only node kinds the Rust SceneNode has, flattening lists like the driver.
function cleanLayers(spread) {
  const out = [];
  const walk = (n) => {
    if (Array.isArray(n)) { n.forEach(walk); return; }
    if (!n || typeof n !== 'object' || !SUPPORTED.has(n.kind)) return;
    if (n.kind === 'group') { const children = []; n.children.forEach((c) => { const sub = cleanLayers([c]); children.push(...sub); }); out.push({ ...n, children }); return; }
    out.push(n);
  };
  walk(spread);
  return out;
}

const byte = (v) => Math.round(Math.min(1, Math.max(0, v)) * 255) / 255;

function traceScene(sample, fi) {
  const t = JSON.parse(gunzipSync(readFileSync(join(traces, `${sample}.json.gz`))).toString());
  const rid = t.compile.rendererIds[0];
  const fr = t.frames[fi];
  if (!rid || !fr) return null;
  const out = resolveRefs(fr.outputs)[rid];
  if (!out) return null;
  const bg = out[1]?.[0];
  const background = bg && typeof bg === 'object' && 'r' in bg ? [byte(bg.r), byte(bg.g), byte(bg.b)] : [0, 0, 0];
  return { width: fr.frame.width, height: fr.frame.height, background, layers: cleanLayers(out[0] ?? []) };
}

// ── Render ──────────────────────────────────────────────────────────────────

const bundle = await build({
  entryPoints: [join(here, 'harness.ts')], bundle: true, write: false, format: 'iife', target: 'es2022',
});
const html = `<!doctype html><html><body style="margin:0;background:#000"><script>${bundle.outputFiles[0].text}</script></body></html>`;

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage();
await page.setContent(html);

async function renderTo(name, scene) {
  if (!wanted(name)) return;
  const url = await page.evaluate((s) => window.renderScene(s), scene);
  writeFileSync(join(here, 'png', `${name}.png`), Buffer.from(url.split(',')[1], 'base64'));
  console.log('rendered', name);
}

for (const [name, scene] of Object.entries(SYNTHETIC)) {
  if (!wanted(name)) continue;
  writeFileSync(join(here, 'scenes', `${name}.json`), JSON.stringify(scene, null, 1) + '\n');
  await renderTo(name, scene);
}

// Every recorded sample whose last frame draws something the renderer supports.
const FRAME = 5; // frames 0, 1, 2, 10, 30, 60 → index 5 = frame 60
for (const f of readdirSync(traces).filter((f) => f.endsWith('.json.gz')).sort()) {
  const sample = f.slice(0, -'.json.gz'.length);
  if (!wanted(`${sample}-f${FRAME}`)) continue;
  const scene = traceScene(sample, FRAME);
  if (!scene || scene.layers.length === 0) continue;
  await renderTo(`${sample}-f${FRAME}`, scene);
}

await browser.close();
