import { registerNode } from '../registry';
import type { SceneNode } from '../../runtime/scene';
import { composeFontCss } from '../../runtime/scene';
import type { Vec2 } from '../../runtime/types';

/**
 * particles/from-layer — produce a `Spread<vec2>` of sample positions
 * from any layer (rect, ellipse, polygon, text, group, …). Plug into an
 * Emitter's `positions` input to emit particles from that shape.
 *
 * Mode (CONSTANT):
 *   - `outline` — sample uniformly along the shape's outline
 *   - `fill`    — sample uniformly inside the bounding region
 *
 * `density` controls the number of samples (CONSTANT — affects spread
 * length so changing it would shift indices downstream).
 *
 * Strategy: walk the SceneNode tree, gather analytic shape primitives,
 * then sample. `text` shapes are rasterised to an OffscreenCanvas and
 * sampled by inside-pixel rejection — particles spawn within the actual
 * glyph paths, not the bounding box. The rasterisation result is
 * cached per-text-key so the cost is one-shot per unique (text, style)
 * combination.
 */
type SamplingMode = 'outline' | 'fill';

export const particlesFromLayerNode = registerNode({
  type: 'particles/from-layer',
  category: 'Particles',
  label: 'FromLayer',
  description:
    'Sample points from any shape — pipe into an Emitter to spawn particles from that shape.',
  inputs: [
    { name: 'layer',   type: 'layer',  default: null },
    { name: 'mode',    type: 'string', default: 'outline', param: 'mode',    constant: true },
    { name: 'density', type: 'number', default: 64,        param: 'density', constant: true, integer: true },
  ],
  outputs: [{ name: 'positions', type: 'vec2', kind: 'spread' }],
  defaultParams: { mode: 'outline' as SamplingMode, density: 64 },
  paramOptions: { mode: ['outline', 'fill'] },
  computeRaw: (inputs) => {
    const layer = inputs[0]?.[0] as SceneNode | null | undefined;
    const mode = (inputs[1]?.[0] === 'fill' ? 'fill' : 'outline') as SamplingMode;
    const density = Math.max(1, Math.floor(Number(inputs[2]?.[0] ?? 64)));
    if (!layer) return [[]];

    const out: Vec2[] = [];
    samplePositions(layer, mode, density, out);
    return [out];
  },
});

/** Walk the tree and collect samples. Each leaf shape contributes
 *  proportionally to its own bounding-box area. */
function samplePositions(
  node: SceneNode,
  mode: SamplingMode,
  density: number,
  out: Vec2[],
): void {
  switch (node.kind) {
    case 'group':
      for (const child of node.children) {
        samplePositions(child, mode, density, out);
      }
      return;
    case 'rect':       return sampleRect(node, mode, density, out);
    case 'ellipse':    return sampleEllipse(node, mode, density, out);
    case 'line':       return sampleLine(node, density, out);
    case 'polygon':    return samplePolygon(node, mode, density, out);
    case 'text':       return sampleText(node, mode, density, out);
    case 'triangle':   return sampleTriangle(node, mode, density, out);
    case 'point':      out.push({ x: node.x, y: node.y }); return;
    default: return; // arc, quad, image, particle batches — skipped for v1
  }
}

function sampleRect(
  r: import('../../runtime/scene').RectShape,
  mode: SamplingMode, density: number, out: Vec2[],
): void {
  const m = r.mode;
  let x = r.x, y = r.y, w = r.w, h = r.h;
  if (m === 'center')  { x -= w/2; y -= h/2; }
  else if (m === 'corners') { w = w - r.x; h = h - r.y; }
  else if (m === 'radius')  { x -= w; y -= h; w *= 2; h *= 2; }
  if (mode === 'fill') {
    for (let i = 0; i < density; i++) {
      out.push({ x: x + Math.random() * w, y: y + Math.random() * h });
    }
  } else {
    // Outline: distribute around perimeter.
    const perim = 2 * (w + h);
    for (let i = 0; i < density; i++) {
      const t = (i / density) * perim;
      if (t < w)            out.push({ x: x + t,     y });
      else if (t < w + h)   out.push({ x: x + w,     y: y + (t - w) });
      else if (t < 2*w + h) out.push({ x: x + (2*w + h - t), y: y + h });
      else                  out.push({ x,             y: y + (perim - t) });
    }
  }
}

function sampleEllipse(
  e: import('../../runtime/scene').EllipseShape,
  mode: SamplingMode, density: number, out: Vec2[],
): void {
  let cx = e.x, cy = e.y, rx = e.w/2, ry = e.h/2;
  if (e.mode === 'corner') { cx += rx; cy += ry; }
  else if (e.mode === 'radius') { rx = e.w; ry = e.h; }
  const TAU = Math.PI * 2;
  if (mode === 'fill') {
    for (let i = 0; i < density; i++) {
      // Rejection-free disk sampling — sqrt for uniformity.
      const r = Math.sqrt(Math.random());
      const a = Math.random() * TAU;
      out.push({ x: cx + Math.cos(a) * r * rx, y: cy + Math.sin(a) * r * ry });
    }
  } else {
    for (let i = 0; i < density; i++) {
      const a = (i / density) * TAU;
      out.push({ x: cx + Math.cos(a) * rx, y: cy + Math.sin(a) * ry });
    }
  }
}

function sampleLine(
  l: import('../../runtime/scene').LineShape,
  density: number, out: Vec2[],
): void {
  for (let i = 0; i < density; i++) {
    const t = i / (density - 1 || 1);
    out.push({ x: l.x1 + (l.x2 - l.x1) * t, y: l.y1 + (l.y2 - l.y1) * t });
  }
}

function samplePolygon(
  p: import('../../runtime/scene').PolygonShape,
  mode: SamplingMode, density: number, out: Vec2[],
): void {
  const pts = p.points;
  if (pts.length < 2) return;
  if (mode === 'outline' || !p.closed) {
    // Perimeter walk.
    const lens: number[] = [];
    let total = 0;
    for (let i = 0; i < pts.length - 1; i++) {
      const dx = pts[i + 1]!.x - pts[i]!.x;
      const dy = pts[i + 1]!.y - pts[i]!.y;
      const l = Math.hypot(dx, dy);
      lens.push(l); total += l;
    }
    if (p.closed) {
      const dx = pts[0]!.x - pts[pts.length - 1]!.x;
      const dy = pts[0]!.y - pts[pts.length - 1]!.y;
      const l = Math.hypot(dx, dy);
      lens.push(l); total += l;
    }
    for (let i = 0; i < density; i++) {
      let t = (i / density) * total;
      for (let j = 0; j < lens.length; j++) {
        if (t <= lens[j]!) {
          const a = pts[j]!;
          const b = pts[(j + 1) % pts.length]!;
          const f = lens[j]! > 0 ? t / lens[j]! : 0;
          out.push({ x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f });
          break;
        }
        t -= lens[j]!;
      }
    }
    return;
  }
  // Fill: bounding-box rejection sample.
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const pt of pts) {
    if (pt.x < minX) minX = pt.x;
    if (pt.y < minY) minY = pt.y;
    if (pt.x > maxX) maxX = pt.x;
    if (pt.y > maxY) maxY = pt.y;
  }
  let tries = 0;
  while (out.length < density && tries < density * 40) {
    const x = minX + Math.random() * (maxX - minX);
    const y = minY + Math.random() * (maxY - minY);
    if (pointInPoly(x, y, pts)) out.push({ x, y });
    tries++;
  }
}

function pointInPoly(
  x: number, y: number,
  pts: ReadonlyArray<Vec2>,
): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const xi = pts[i]!.x, yi = pts[i]!.y;
    const xj = pts[j]!.x, yj = pts[j]!.y;
    const intersect = ((yi > y) !== (yj > y)) &&
      (x < (xj - xi) * (y - yi) / (yj - yi || 1e-9) + xi);
    if (intersect) inside = !inside;
  }
  return inside;
}

function sampleTriangle(
  t: import('../../runtime/scene').TriangleShape,
  mode: SamplingMode, density: number, out: Vec2[],
): void {
  if (mode === 'fill') {
    for (let i = 0; i < density; i++) {
      let r1 = Math.random(), r2 = Math.random();
      if (r1 + r2 > 1) { r1 = 1 - r1; r2 = 1 - r2; }
      const x = t.x1 + r1 * (t.x2 - t.x1) + r2 * (t.x3 - t.x1);
      const y = t.y1 + r1 * (t.y2 - t.y1) + r2 * (t.y3 - t.y1);
      out.push({ x, y });
    }
  } else {
    // Outline: walk three edges.
    const edges: [Vec2, Vec2][] = [
      [{ x: t.x1, y: t.y1 }, { x: t.x2, y: t.y2 }],
      [{ x: t.x2, y: t.y2 }, { x: t.x3, y: t.y3 }],
      [{ x: t.x3, y: t.y3 }, { x: t.x1, y: t.y1 }],
    ];
    const perDensity = Math.ceil(density / 3);
    for (const [a, b] of edges) {
      for (let i = 0; i < perDensity && out.length < density; i++) {
        const f = i / perDensity;
        out.push({ x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f });
      }
    }
  }
}

/**
 * Per-text rasterisation cache. Keyed by composed font + text +
 * alignment + letter spacing + size. When a `from-layer` over a static
 * text shape stays unchanged, we rasterise once and resample on every
 * tick from the cached pixel buffer.
 */
interface RasterEntry {
  /** Inside-pixel coordinates collected once at rasterise time.
   *  Resampling at tick time is `pickRandom(insidePixels)`. */
  insidePixels: Float32Array;
  /** Outline-pixel coordinates — pixels inside whose 4-neighbourhood has
   *  at least one outside pixel. Used for outline mode. */
  outlinePixels: Float32Array;
  /** World offset that converts raster pixel coords to world coords. */
  offsetX: number;
  offsetY: number;
}
const rasterCache = new Map<string, RasterEntry>();
const RASTER_PAD = 4;          // padding around the rendered text in pixels
const RASTER_ALPHA_CUT = 64;   // alpha threshold for "inside the glyph"

function sampleText(
  t: import('../../runtime/scene').TextShape,
  mode: SamplingMode, density: number, out: Vec2[],
): void {
  const text = String(t.text ?? '');
  if (text.length === 0) return;

  const fontCss = composeFontCss(t.style);
  const ls = t.style.letterSpacing;
  const key = `${fontCss}|${ls}|${t.style.textAlign}|${t.style.textBaseline}|${text}`;
  let entry: RasterEntry | null = rasterCache.get(key) ?? null;
  if (!entry) {
    entry = rasteriseText(t, fontCss);
    if (entry) {
      rasterCache.set(key, entry);
      // Hard cap the cache so a patch that animates text doesn't leak.
      // 64 entries × ~typical sizes is bounded by a few MB.
      if (rasterCache.size > 64) {
        const first = rasterCache.keys().next().value;
        if (first !== undefined) rasterCache.delete(first);
      }
    }
  }
  // Fall back to bounding-box rect if rasterisation failed (no
  // OffscreenCanvas in this runtime, etc.).
  if (!entry || (mode === 'fill' ? entry.insidePixels.length : entry.outlinePixels.length) === 0) {
    return sampleTextBBox(t, mode, density, out);
  }

  // Translate the text shape's origin into the cached raster's frame.
  // The rasteriser draws relative to its own padding origin; the
  // SceneNode wants samples in world coords offset by (t.x, t.y).
  const src = mode === 'fill' ? entry.insidePixels : entry.outlinePixels;
  const total = src.length / 2;
  const ox = t.x + entry.offsetX;
  const oy = t.y + entry.offsetY;
  for (let i = 0; i < density; i++) {
    const idx = (Math.random() * total) | 0;
    out.push({ x: ox + (src[idx * 2] ?? 0), y: oy + (src[idx * 2 + 1] ?? 0) });
  }
}

function rasteriseText(
  t: import('../../runtime/scene').TextShape,
  fontCss: string,
): RasterEntry | null {
  // OffscreenCanvas is required. If the host runtime doesn't provide
  // it (some test environments), bail out and the caller falls back.
  if (typeof OffscreenCanvas === 'undefined') return null;

  // Measure first with a 1px scratch canvas so we know how big to make
  // the real rasteriser.
  const scratch = new OffscreenCanvas(1, 1);
  const sctx = scratch.getContext('2d');
  if (!sctx) return null;
  sctx.font = fontCss;
  // Honour letterSpacing on engines that support it (modern Chrome /
  // Safari TP). Older runtimes ignore the assignment silently.
  if ('letterSpacing' in sctx) {
    (sctx as unknown as { letterSpacing: string }).letterSpacing = `${t.style.letterSpacing}px`;
  }
  const metrics = sctx.measureText(t.text);
  // actualBoundingBoxLeft/Right are positive in their respective
  // directions on most engines — we sum them to get the full width.
  const w = Math.ceil(metrics.actualBoundingBoxLeft + metrics.actualBoundingBoxRight) || 1;
  const ascent = Math.ceil(metrics.actualBoundingBoxAscent) || t.style.fontSize;
  const descent = Math.ceil(metrics.actualBoundingBoxDescent) || 0;
  const h = ascent + descent || t.style.fontSize;
  const pad = RASTER_PAD;
  const totalW = w + pad * 2;
  const totalH = h + pad * 2;

  const canvas = new OffscreenCanvas(totalW, totalH);
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.font = fontCss;
  if ('letterSpacing' in ctx) {
    (ctx as unknown as { letterSpacing: string }).letterSpacing = `${t.style.letterSpacing}px`;
  }
  ctx.textAlign = t.style.textAlign;
  ctx.textBaseline = t.style.textBaseline;
  ctx.fillStyle = '#fff';
  // Anchor origin: we draw at (anchorX, anchorY) in raster space; the
  // returned offset translates raster-(0,0) to world-(t.x, t.y).
  // For alphabetic baseline the descent sits below the baseline line.
  let anchorX = pad + metrics.actualBoundingBoxLeft;
  const anchorY = pad + ascent;
  // textAlign already shifts the actual fill anchor — we need to push
  // the raster origin to match: left/right alignment shift anchorX.
  if (t.style.textAlign === 'center') anchorX = totalW * 0.5;
  else if (t.style.textAlign === 'right') anchorX = totalW - pad;
  ctx.fillText(t.text, anchorX, anchorY);

  const imgData = ctx.getImageData(0, 0, totalW, totalH).data;
  // Inside set: pixels whose alpha clears the threshold.
  // Outline set: inside pixels with at least one outside neighbour.
  const insideXY: number[] = [];
  const outlineXY: number[] = [];
  const isInside = (x: number, y: number): boolean => {
    if (x < 0 || y < 0 || x >= totalW || y >= totalH) return false;
    return (imgData[(y * totalW + x) * 4 + 3] ?? 0) >= RASTER_ALPHA_CUT;
  };
  for (let y = 0; y < totalH; y++) {
    for (let x = 0; x < totalW; x++) {
      if (!isInside(x, y)) continue;
      // Adjust into local glyph coords — the offset below converts
      // raster origin into world space relative to (t.x, t.y). In raster
      // space, the baseline sits at y = anchorY (ascent + pad), so
      // raster-y = anchorY corresponds to world-y = 0.
      const localX = x - anchorX;
      const localY = y - anchorY;
      insideXY.push(localX, localY);
      if (!isInside(x - 1, y) || !isInside(x + 1, y) ||
          !isInside(x, y - 1) || !isInside(x, y + 1)) {
        outlineXY.push(localX, localY);
      }
    }
  }
  return {
    insidePixels: new Float32Array(insideXY),
    outlinePixels: new Float32Array(outlineXY),
    offsetX: 0,
    offsetY: 0,
  };
}

function sampleTextBBox(
  t: import('../../runtime/scene').TextShape,
  mode: SamplingMode, density: number, out: Vec2[],
): void {
  // Fallback when rasterisation isn't available (no OffscreenCanvas).
  // Same body as the v1 implementation.
  const sz = t.style.fontSize;
  const text = String(t.text ?? '');
  const w = text.length * sz * 0.6;
  const h = sz;
  let x = t.x;
  let y = t.y - h * 0.7;
  if (t.style.textAlign === 'center')      x -= w / 2;
  else if (t.style.textAlign === 'right')  x -= w;
  if (mode === 'fill') {
    for (let i = 0; i < density; i++) {
      out.push({ x: x + Math.random() * w, y: y + Math.random() * h });
    }
  } else {
    const perim = 2 * (w + h);
    for (let i = 0; i < density; i++) {
      const u = (i / density) * perim;
      if (u < w)            out.push({ x: x + u,     y });
      else if (u < w + h)   out.push({ x: x + w,     y: y + (u - w) });
      else if (u < 2*w + h) out.push({ x: x + (2*w + h - u), y: y + h });
      else                  out.push({ x,             y: y + (perim - u) });
    }
  }
}
