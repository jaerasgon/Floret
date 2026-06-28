// ─── Spike v2 harness: live preview of a sample through the GL vector renderer ──
// The scene→batches walk now lives in scene-to-batches.ts (shared with the A/B
// diff harness). This file is just the canvas + RAF loop + HUD.

import { tick } from '../runtime/tick';
import type { SceneNode } from '../runtime/scene';
import { VectorRenderer, FrameBuilder } from '../runtime/gl/gl-vector-renderer';
import { emit, ID } from '../runtime/gl/scene-to-batches';
import { buildSample } from './scene-samples';

const canvas = document.getElementById('gl') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLDivElement;
const renderer = new VectorRenderer(canvas);
const fb = new FrameBuilder();
const params = new URLSearchParams(location.search);
let current = buildSample(params.get('sample') ?? 'grid-ripple');

function sizeCanvas() {
  const dpr = Math.min(devicePixelRatio || 1, 2);
  const disp = Math.min(window.innerWidth - 40, window.innerHeight - 130, 820);
  canvas.style.width = canvas.style.height = `${disp}px`;
  canvas.width = Math.round(disp * dpr); canvas.height = Math.round(disp * dpr);
}
window.addEventListener('resize', sizeCanvas); sizeCanvas();

const frame = { t: 0, dt: 1 / 60, frameCount: 0, width: current.w, height: current.h, mouseX: current.w / 2, mouseY: current.h / 2, mouseIsPressed: false, key: '', keyIsPressed: false };
let fps = 0, last = performance.now(), acc = 0, frames = 0;

function step(dt: number) {
  frame.t += dt; frame.dt = dt; frame.frameCount++;
  const out = tick(current.graph, frame) as { layers: ReadonlyArray<unknown> };
  fb.reset();
  for (const l of out.layers) {
    if (Array.isArray(l)) for (const inner of l) emit(fb, renderer, inner as SceneNode, ID);
    else emit(fb, renderer, l as SceneNode, ID);
  }
  renderer.render(fb, current.w, current.h, current.bg);
}

function loop(now: number) {
  const dt = Math.min((now - last) / 1000, 0.05); last = now;
  acc += dt; frames++; if (acc >= 0.5) { fps = frames / acc; acc = 0; frames = 0; }
  step(dt);
  hud.textContent =
    `${current.name}  ·  ${fb.sdfCount.toLocaleString()} sdf · ${fb.fillVerts.toLocaleString()} fill-v · ${fb.texQuads.length} tex` +
    `  ·  ${fps.toFixed(0)} fps` + (fb.skipped ? `  ·  ${fb.skipped} skipped` : '') +
    (current.warnings.length ? `  ·  ${current.warnings.length} warnings` : '');
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
(window as unknown as { __spike: unknown }).__spike = {
  buildSample,
  set sample(id: string) { current = buildSample(id); },
  // Drive N frames synchronously — lets a hidden/throttled tab accumulate
  // time-based sims (particles) for screenshots.
  pump(n: number, dt = 1 / 60) { for (let i = 0; i < n; i++) step(dt); },
};
