// ─── A/B parity harness: Canvas2D (scene-renderer) vs GL (VectorRenderer) ────
// For each visual sample: tick the graph N frames, then render the SAME layers
// through both backends and compute mean-abs-error. This isolates the RENDERER
// difference (not sim divergence) and is the gate for retiring Canvas2D.

import { SAMPLES } from '../persistence/samples';
import { tick } from '../runtime/tick';
import { renderScene } from '../runtime/scene-renderer';
import type { SceneNode } from '../runtime/scene';
import { VectorRenderer, FrameBuilder } from './gl-vector-renderer';
import { buildSample, emit, ID } from './scene-to-batches';

const SKIP = (defType: string) =>
  defType.startsWith('sound/') || defType === 'io/image-url' || defType === 'draw/image' ||
  defType === 'input/camera' || defType === 'input/landmarks';

interface Row { name: string; id: string; mae: number; w: number; h: number; c2d: string; gl: string }

function glToDataURL(data: Uint8Array, w: number, h: number): string {
  const c = document.createElement('canvas'); c.width = w; c.height = h;
  const ctx = c.getContext('2d')!; const img = ctx.createImageData(w, h);
  for (let y = 0; y < h; y++) {                       // flip GL (bottom-up) → top-down
    const src = (h - 1 - y) * w * 4, dst = y * w * 4;
    for (let i = 0; i < w * 4; i++) img.data[dst + i] = data[src + i]!;
  }
  ctx.putImageData(img, 0, 0); return c.toDataURL();
}

async function run() {
  const glCanvas = document.createElement('canvas');
  const renderer = new VectorRenderer(glCanvas);
  const fb = new FrameBuilder();
  const c2d = document.createElement('canvas');
  const rows: Row[] = [];
  const status = document.getElementById('status')!;

  for (const samp of SAMPLES) {
    let built; try { built = samp.build(); } catch { continue; }
    const hasRenderer = built.nodes.some((n) => n.data.defType === 'render/renderer');
    if (!hasRenderer || built.nodes.some((n) => SKIP(n.data.defType))) continue;
    status.textContent = `rendering ${samp.name}…`;
    await new Promise((r) => setTimeout(r, 0));

    let s; try { s = buildSample(samp.id); } catch { continue; }
    const W = s.w, H = s.h;
    const frame = { t: 0, dt: 1 / 60, frameCount: 0, width: W, height: H, mouseX: W / 2, mouseY: H / 2, mouseIsPressed: false, key: '', keyIsPressed: false };
    let layers: ReadonlyArray<unknown> | undefined;
    try { for (let i = 0; i < 30; i++) { frame.t += 1 / 60; frame.frameCount++; layers = tick(s.graph, frame).layers; } } catch { continue; }
    if (!layers) continue;
    const flat: SceneNode[] = [];
    for (const l of layers) { if (Array.isArray(l)) flat.push(...l as SceneNode[]); else flat.push(l as SceneNode); }

    // Canvas2D reference
    c2d.width = W; c2d.height = H;
    const ctx = c2d.getContext('2d', { willReadFrequently: true })!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = `rgb(${Math.round(s.bg[0] * 255)},${Math.round(s.bg[1] * 255)},${Math.round(s.bg[2] * 255)})`;
    ctx.fillRect(0, 0, W, H);
    ctx.translate(W / 2, H / 2);                       // centre origin (matches the runtime)
    for (const l of flat) try { renderScene(ctx, l); } catch { /* skip bad node */ }
    const c2dData = ctx.getImageData(0, 0, W, H).data;

    // GL
    glCanvas.width = W; glCanvas.height = H;
    fb.reset();
    for (const l of flat) emit(fb, renderer, l, ID);
    renderer.render(fb, W, H, s.bg);
    const gl = renderer.gl;
    const glData = new Uint8Array(W * H * 4);
    gl.readPixels(0, 0, W, H, gl.RGBA, gl.UNSIGNED_BYTE, glData);

    // MAE over RGB (GL bottom-up → flip)
    let sum = 0;
    for (let y = 0; y < H; y++) {
      const glRow = (H - 1 - y) * W * 4, c2dRow = y * W * 4;
      for (let x = 0; x < W; x++) {
        const go = glRow + x * 4, co = c2dRow + x * 4;
        sum += Math.abs(glData[go]! - c2dData[co]!) + Math.abs(glData[go + 1]! - c2dData[co + 1]!) + Math.abs(glData[go + 2]! - c2dData[co + 2]!);
      }
    }
    const mae = sum / (W * H * 3) / 255;
    rows.push({ name: samp.name, id: samp.id, mae, w: W, h: H, c2d: c2d.toDataURL(), gl: glToDataURL(glData, W, H) });
  }

  rows.sort((a, b) => b.mae - a.mae);
  (window as unknown as { __diff: unknown }).__diff = rows.map((r) => ({ name: r.name, id: r.id, mae: +(r.mae * 100).toFixed(2) }));
  status.textContent = `${rows.length} samples · sorted worst → best (MAE % of full-scale)`;

  const tbl = document.getElementById('rows')!;
  for (const r of rows) {
    const pct = (r.mae * 100).toFixed(2);
    const verdict = r.mae < 0.01 ? '✓' : r.mae < 0.03 ? '~' : '✗';
    const div = document.createElement('div'); div.className = 'row';
    div.innerHTML =
      `<div class="meta"><b>${verdict} ${r.name}</b><span>${pct}% · ${r.w}×${r.h}</span></div>` +
      `<img src="${r.c2d}" title="Canvas2D"><img src="${r.gl}" title="GL">`;
    tbl.appendChild(div);
  }
}

void run();
