// ─── Scene → GPU batches (shared, side-effect-free) ─────────────────────────
// Walks the SceneNode tree and routes every kind — rect/ellipse/line/point/arc/
// triangle/quad/polygon/path/text/image/particles — to the right GPU pipeline
// (SDF / stencil fill / textured quad), baking group affines (incl. shear),
// clip, blend, and blur. Used by both the live spike and the A/B diff harness.

import { resolveBox, composeFontCss, catmullRomBeziers, type Affine, type SceneNode, type ResolvedStyle } from '../scene';
import { VectorRenderer, FrameBuilder, SdfType, SdfAlign } from './gl-vector-renderer';

// ── colour ──
export type RGBA = [number, number, number, number];
const TRANSPARENT: RGBA = [0, 0, 0, 0];
const colorCache = new Map<string, RGBA>();
export function parseColor(css: string | null | undefined): RGBA {
  if (!css) return TRANSPARENT;
  const hit = colorCache.get(css); if (hit) return hit;
  let out: RGBA = [0, 0, 0, 1];
  if (css[0] === '#') {
    let h = css.slice(1);
    if (h.length === 3) h = h[0]! + h[0]! + h[1]! + h[1]! + h[2]! + h[2]!;
    const n = parseInt(h, 16);
    out = [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, 1];
  } else {
    const m = css.match(/rgba?\(([^)]+)\)/);
    if (m) { const p = m[1]!.split(',').map((s) => parseFloat(s)); out = [(p[0] ?? 0) / 255, (p[1] ?? 0) / 255, (p[2] ?? 0) / 255, p[3] ?? 1]; }
  }
  colorCache.set(css, out); return out;
}
const alignCode = (a: string) => a === 'inside' ? SdfAlign.Inside : a === 'outside' ? SdfAlign.Outside : SdfAlign.Center;
const BLEND: Record<string, number> = { 'source-over': 0, lighter: 1, multiply: 2, screen: 3, darken: 4, lighten: 5 };
const blendCode = (m: string): number => BLEND[m] ?? 0;

// ── affine ──
export const ID: Affine = [1, 0, 0, 1, 0, 0];
const mul = (m: Affine, n: Affine): Affine => [
  m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1],
  m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3],
  m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5],
];
const apply = (m: Affine, x: number, y: number): [number, number] => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
const scaleOf = (m: Affine) => (Math.hypot(m[0], m[1]) + Math.hypot(m[2], m[3])) / 2;

type Pt = { x: number; y: number };

function emitCapsule(fb: FrameBuilder, a: Pt, b: Pt, radius: number, col: RGBA) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const len = Math.hypot(dx, dy); if (len < 1e-4 && radius < 1e-3) return;
  const ang = Math.atan2(dy, dx), cs = Math.cos(ang), sn = Math.sin(ang);
  fb.pushSdf(SdfType.Capsule, (a.x + b.x) / 2, (a.y + b.y) / 2, cs, -sn, sn, cs, len / 2, Math.max(radius, 0.4), 0, 0, col, TRANSPARENT, SdfAlign.Center);
}

function polyWorld(points: ReadonlyArray<Pt>, closed: boolean, tension: number, m: Affine): Pt[] {
  const n = points.length;
  if (tension <= 0 || n < 3) return points.map((p) => { const [x, y] = apply(m, p.x, p.y); return { x, y }; });
  const out: Pt[] = []; const K = 14;
  let prev = points[0]!;
  const push = (x: number, y: number) => { const [wx, wy] = apply(m, x, y); out.push({ x: wx, y: wy }); };
  push(prev.x, prev.y);
  for (const s of catmullRomBeziers(points, closed, tension)) {
    const p0 = prev;
    for (let i = 1; i <= K; i++) {
      const t = i / K, u = 1 - t;
      const x = u * u * u * p0.x + 3 * u * u * t * s.c1x + 3 * u * t * t * s.c2x + t * t * t * s.ex;
      const y = u * u * u * p0.y + 3 * u * u * t * s.c1y + 3 * u * t * t * s.c2y + t * t * t * s.ey;
      push(x, y);
    }
    prev = { x: s.ex, y: s.ey };
  }
  return out;
}

function fillPolygon(fb: FrameBuilder, pts: Pt[], col: RGBA) {
  if (col[3] <= 0 || pts.length < 3) return;
  fb.pushPolyFill(pts, col);
}
function strokePolygon(fb: FrameBuilder, pts: Pt[], closed: boolean, radius: number, col: RGBA) {
  if (col[3] <= 0) return;
  const n = pts.length;
  for (let i = 0; i < (closed ? n : n - 1); i++) emitCapsule(fb, pts[i]!, pts[(i + 1) % n]!, radius, col);
}

// ── text rasterisation (glyph canvas → texture) ──
// Works on the main thread (HTMLCanvasElement) AND in a worker (OffscreenCanvas),
// so the same code drives the Stage-1 worker render path.
type AnyCanvas = HTMLCanvasElement | OffscreenCanvas;
type Any2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
function makeCanvas(w: number, h: number): AnyCanvas {
  if (typeof document !== 'undefined') { const c = document.createElement('canvas'); c.width = w; c.height = h; return c; }
  return new OffscreenCanvas(w, h);
}
interface GlyphRec { canvas: AnyCanvas; w: number; h: number; asc: number; neutral: boolean }
// Bounded insertion-ordered cache: tint-keyed glyphs (fill-only text) stay tiny,
// but ever-changing CONTENT (a frame counter, a clock) would otherwise mint a new
// key every frame and leak a glyph canvas forever. Evict oldest past the cap.
const GLYPH_CACHE_CAP = 1024;
const textCanvasCache = new Map<string, GlyphRec>();
// Rasterise a text run to a glyph canvas. Fill-only text (no stroke) is drawn
// in neutral WHITE and keyed by text+font alone — its colour and opacity are
// applied as a per-quad tint at draw time, so a letter animating its opacity
// (or many same-letter cells) reuses ONE cached glyph instead of re-rasterising
// every frame. Text with a stroke needs both colours baked, so it keeps the
// colour in its key.
function rasterizeText(text: string, style: ResolvedStyle): GlyphRec {
  const neutral = !style.stroke;
  const colourKey = neutral ? 'N' : `${style.fill}|${style.stroke}|${style.strokeWeight}`;
  const key = `${text}|${composeFontCss(style)}|${style.lineHeight}|${colourKey}`;
  const hit = textCanvasCache.get(key); if (hit) return hit;
  const s = 2;
  const lines = text.includes('\n') ? text.split('\n') : [text];
  const meas = makeCanvas(1, 1).getContext('2d') as Any2D;
  meas.font = composeFontCss(style);
  let w = 1; for (const l of lines) w = Math.max(w, meas.measureText(l).width);
  const dy = style.fontSize * style.lineHeight;
  const W = Math.ceil(w) + 6, H = Math.ceil(lines.length * dy) + 6;
  const canvas = makeCanvas(W * s, H * s);
  const ctx = canvas.getContext('2d') as Any2D; ctx.scale(s, s);
  ctx.font = composeFontCss(style); ctx.textAlign = 'left'; ctx.textBaseline = 'top';
  ctx.lineWidth = style.strokeWeight;
  for (let i = 0; i < lines.length; i++) {
    if (neutral) { ctx.fillStyle = '#fff'; ctx.fillText(lines[i]!, 1, i * dy + 1); }
    else {
      if (style.fill) { ctx.fillStyle = style.fill; ctx.fillText(lines[i]!, 1, i * dy + 1); }
      if (style.stroke) { ctx.strokeStyle = style.stroke; ctx.strokeText(lines[i]!, 1, i * dy + 1); }
    }
  }
  const rec: GlyphRec = { canvas, w: W, h: H, asc: style.fontSize * 0.8, neutral };
  if (textCanvasCache.size >= GLYPH_CACHE_CAP) {
    // Drop the oldest entry; its GPU texture is keyed by the canvas in a
    // WeakMap, so it's reclaimable once this last strong ref goes.
    textCanvasCache.delete(textCanvasCache.keys().next().value!);
  }
  textCanvasCache.set(key, rec); return rec;
}

// ── walk one SceneNode ──
// Wrapper: a shape with style.blur renders into its own layer, gets a separable
// Gaussian, then composites — a CSS-style per-shape `filter: blur()`.
export function emit(fb: FrameBuilder, renderer: VectorRenderer, node: SceneNode, m: Affine): void {
  const st = (node as { style?: ResolvedStyle }).style;
  const blur = st && node.kind !== 'group' ? st.blur : 0;
  if (blur > 0) {
    fb.pushLayer();
    emitInner(fb, renderer, node, m);
    fb.popLayerBlur(blur * scaleOf(m));
    return;
  }
  emitInner(fb, renderer, node, m);
}

function emitInner(fb: FrameBuilder, renderer: VectorRenderer, node: SceneNode, m: Affine): void {
  switch (node.kind) {
    case 'group': {
      const gm = node.transform ? mul(m, node.transform) : m;
      let pc = false, pb = false;
      if (node.clip) {
        const cl = node.clip;
        const corners: [number, number][] = [[cl.x, cl.y], [cl.x + cl.w, cl.y], [cl.x, cl.y + cl.h], [cl.x + cl.w, cl.y + cl.h]];
        let minx = Infinity, miny = Infinity, maxx = -Infinity, maxy = -Infinity;
        for (const [px, py] of corners) { const [wx, wy] = apply(gm, px, py); minx = Math.min(minx, wx); miny = Math.min(miny, wy); maxx = Math.max(maxx, wx); maxy = Math.max(maxy, wy); }
        const parent = fb.curClip();
        fb.pushClip(parent
          ? [Math.max(minx, parent[0]), Math.max(miny, parent[1]), Math.min(maxx, parent[2]), Math.min(maxy, parent[3])]
          : [minx, miny, maxx, maxy]);
        pc = true;
      }
      if (node.blendMode) { fb.pushLayer(); pb = true; }
      for (const c of node.children) emit(fb, renderer, c, gm);
      if (pb) fb.popLayer(blendCode(node.blendMode ?? 'source-over'));
      if (pc) fb.popClip();
      return;
    }
    case 'rect':
    case 'ellipse': {
      const st = node.style;
      const [left, top, w, h] = resolveBox(node.x, node.y, node.w, node.h, node.mode);
      const [cx, cy] = apply(m, left + w / 2, top + h / 2);
      // |w|,|h|: negative dims (e.g. bars drawn upward) have a valid centre but
      // need a positive half-extent or the SDF never goes inside (blank shape).
      fb.pushSdf(node.kind === 'rect' ? SdfType.Rect : SdfType.Ellipse, cx, cy, m[0], m[2], m[1], m[3],
        Math.abs(w) / 2, Math.abs(h) / 2, 0, st.strokeWeight, parseColor(st.fill), parseColor(st.stroke), alignCode(st.strokeAlign));
      return;
    }
    case 'line': {
      if (!node.style.stroke) return;
      const a = { x: 0, y: 0 }, b = { x: 0, y: 0 };
      [a.x, a.y] = apply(m, node.x1, node.y1); [b.x, b.y] = apply(m, node.x2, node.y2);
      emitCapsule(fb, a, b, node.style.strokeWeight * scaleOf(m) / 2, parseColor(node.style.stroke));
      return;
    }
    case 'point': {
      const col = parseColor(node.style.stroke ?? node.style.fill); if (col[3] <= 0) return;
      const [cx, cy] = apply(m, node.x, node.y);
      const r = Math.max(node.style.strokeWeight, 1) * scaleOf(m) / 2;
      fb.pushSdf(SdfType.Rect, cx, cy, 1, 0, 0, 1, r, r, 0, 0, col, TRANSPARENT, SdfAlign.Center);
      return;
    }
    case 'triangle': {
      const pts = [{ x: node.x1, y: node.y1 }, { x: node.x2, y: node.y2 }, { x: node.x3, y: node.y3 }]
        .map((p) => { const [x, y] = apply(m, p.x, p.y); return { x, y }; });
      fillPolygon(fb, pts, parseColor(node.style.fill));
      strokePolygon(fb, pts, true, node.style.strokeWeight * scaleOf(m) / 2, parseColor(node.style.stroke));
      return;
    }
    case 'quad': {
      const pts = [{ x: node.x1, y: node.y1 }, { x: node.x2, y: node.y2 }, { x: node.x3, y: node.y3 }, { x: node.x4, y: node.y4 }]
        .map((p) => { const [x, y] = apply(m, p.x, p.y); return { x, y }; });
      fillPolygon(fb, pts, parseColor(node.style.fill));
      strokePolygon(fb, pts, true, node.style.strokeWeight * scaleOf(m) / 2, parseColor(node.style.stroke));
      return;
    }
    case 'polygon': {
      const pts = polyWorld(node.points, node.closed, node.tension ?? 0, m);
      if (node.closed) fillPolygon(fb, pts, parseColor(node.style.fill));
      strokePolygon(fb, pts, node.closed, node.style.strokeWeight * scaleOf(m) / 2, parseColor(node.style.stroke));
      return;
    }
    case 'arc': {
      const [left, top, w, h] = resolveBox(node.x, node.y, node.w, node.h, node.mode);
      const cx = left + w / 2, cy = top + h / 2, rx = w / 2, ry = h / 2;
      const N = 28, pts: Pt[] = [];
      for (let i = 0; i <= N; i++) {
        const a = node.start + (node.stop - node.start) * (i / N);
        const [x, y] = apply(m, cx + Math.cos(a) * rx, cy + Math.sin(a) * ry); pts.push({ x, y });
      }
      fillPolygon(fb, pts, parseColor(node.style.fill));
      strokePolygon(fb, pts, false, node.style.strokeWeight * scaleOf(m) / 2, parseColor(node.style.stroke));
      return;
    }
    case 'text': {
      if (!node.text) return;
      const r = rasterizeText(node.text, node.style);
      // Glyph canvases never change once rasterised → immutable: upload once,
      // skip the per-frame re-upload. Neutral (white) glyphs carry their colour
      // and opacity as the quad tint; baked glyphs draw at full white tint.
      const tint = r.neutral ? parseColor(node.style.fill) : [1, 1, 1, 1] as [number, number, number, number];
      if (tint[3] <= 0) return;   // fully transparent → nothing to draw
      const tex = renderer.texture(r.canvas, r.canvas, true);
      const W = r.w, H = r.h;
      const ax = node.style.textAlign === 'center' ? -W / 2 : node.style.textAlign === 'right' ? -W : 0;
      const ay = node.style.textBaseline === 'top' ? 0 : node.style.textBaseline === 'middle' ? -H / 2
        : node.style.textBaseline === 'bottom' ? -H : -r.asc;
      const lx = node.x + ax, ly = node.y + ay;
      const tl = apply(m, lx, ly), tr = apply(m, lx + W, ly), bl = apply(m, lx, ly + H), br = apply(m, lx + W, ly + H);
      fb.pushTexQuad({ c: [tl[0], tl[1], tr[0], tr[1], bl[0], bl[1], br[0], br[1]], u0: 0, v0: 0, u1: 1, v1: 1, tint, tex });
      return;
    }
    case 'image': {
      const src = node.source as (CanvasImageSource & { width?: number; height?: number; naturalWidth?: number; naturalHeight?: number; complete?: boolean });
      if (src.complete === false) { fb.skipped++; return; }
      const sw = src.naturalWidth || src.width || 1, sh = src.naturalHeight || src.height || 1;
      let tex: WebGLTexture;
      try { tex = renderer.texture(src as object, src as TexImageSource); } catch { fb.skipped++; return; }
      const d = node.dst;
      const u0 = node.src ? node.src.x / sw : 0, v0 = node.src ? node.src.y / sh : 0;
      const u1 = node.src ? (node.src.x + node.src.w) / sw : 1, v1 = node.src ? (node.src.y + node.src.h) / sh : 1;
      const tl = apply(m, d.x, d.y), tr = apply(m, d.x + d.w, d.y), bl = apply(m, d.x, d.y + d.h), br = apply(m, d.x + d.w, d.y + d.h);
      const tint: RGBA = node.tint ? parseColor(node.tint) : [1, 1, 1, 1];
      fb.pushTexQuad({ c: [tl[0], tl[1], tr[0], tr[1], bl[0], bl[1], br[0], br[1]], u0, v0, u1, v1, tint, tex });
      return;
    }
    case 'shader': {
      const uniforms = node.uniformDecls
        .filter((d) => d.glslType !== 'sampler2D')
        .map((d) => {
          const uv = node.uniforms[d.name];
          const value = !uv ? [0] : uv.t === 'number' ? [uv.v] : uv.v;
          return { name: d.name, glslType: d.glslType, value };
        });
      const payload = { nodeId: node.nodeId, passes: node.passes, uniforms, hasInput: !!node.input };
      // The input subtree renders into a pushed FBO first → becomes the 'input' channel.
      if (node.input) { fb.pushLayer(); emit(fb, renderer, node.input, m); fb.pushShader(payload); }
      else fb.pushShader(payload);
      return;
    }
    case 'particle-batch': {
      try {
        const buf = node.buffer, cap = buf.capacity, alive = buf.alive, sc = scaleOf(m);
        const useCol = node.useBufferColor, sFill = parseColor(node.style.fill);
        for (let i = 0; i < cap; i++) {
          if (!alive[i]) continue;
          const [cx, cy] = apply(m, buf.posX[i]!, buf.posY[i]!);
          const r = Math.max(buf.size[i]!, 0.4) * sc;
          const col: RGBA = useCol ? [buf.colorR[i]!, buf.colorG[i]!, buf.colorB[i]!, buf.colorA[i]!] : sFill;
          fb.pushSdf(SdfType.Ellipse, cx, cy, 1, 0, 0, 1, r, r, 0, 0, col, TRANSPARENT, SdfAlign.Center);
        }
      } catch { fb.skipped++; }
      return;
    }
    case 'particle-trail-batch': {
      try {
        const buf = node.buffer, cap = buf.capacity, alive = buf.alive, hist = node.history, segs = node.segments, head = node.headIdx;
        const useCol = node.useBufferColor, sStroke = parseColor(node.style.stroke), w = node.style.strokeWeight * scaleOf(m) / 2;
        for (let slot = 0; slot < cap; slot++) {
          if (!alive[slot]) continue;
          const col: RGBA = useCol ? [buf.colorR[slot]!, buf.colorG[slot]!, buf.colorB[slot]!, buf.colorA[slot]!] : sStroke;
          let prev: Pt | null = null;
          for (let s = 0; s < segs; s++) {
            const idx = (((head - s) % segs) + segs) % segs, base = (slot * segs + idx) * 2;
            const [wx, wy] = apply(m, hist[base]!, hist[base + 1]!);
            if (prev) emitCapsule(fb, prev, { x: wx, y: wy }, w, col);
            prev = { x: wx, y: wy };
          }
        }
      } catch { fb.skipped++; }
      return;
    }
    default:
      fb.skipped++;
  }
}
