import type {
  ArcShape,
  EllipseShape,
  GroupNode,
  ImageShape,
  LineShape,
  PointShape,
  PolygonShape,
  QuadShape,
  RectShape,
  ResolvedStyle,
  SceneNode,
  TextShape,
  TriangleShape,
} from './scene';
import { resolveBox, catmullRomBeziers } from './scene';

/**
 * Walk a SceneNode tree, emit an SVG document.
 *
 * The companion to the GPU renderer (`gl/scene-to-batches.ts`): same visitor
 * shape, same shape vocabulary, different output. Where the GPU path emits an
 * SDF/fill batch, this visitor writes `fill="..."` and `<rect/>`.
 *
 * Design notes:
 *
 *  - **String building.** Push fragments into an array, join once at the
 *    end. Avoids the quadratic cost of repeated string concat in a hot
 *    path. The cost we care about is the FIRST export — users won't be
 *    running this 60fps — but tidy is tidy.
 *
 *  - **Transforms align with SVG by construction.** The scene model's
 *    affine `[a b c d e f]` IS SVG's `matrix(a b c d e f)`. No conversion. A
 *    group with `transform` becomes `<g transform="matrix(a b c d e f)">`.
 *
 *  - **Style as inline attributes.** Every shape carries its own
 *    ResolvedStyle so there's no benefit to <defs>+class indirection
 *    today. Inline keeps the output legible and editable in a vector tool.
 *
 *  - **Blend modes.** The scene model's blend-mode keywords overlap with
 *    CSS `mix-blend-mode` for the actual *blend* keywords (multiply,
 *    screen, overlay, …) but also include compositing operators
 *    (source-over, destination-in, …) that have no SVG analog. We pass
 *    the known blend keywords through and silently drop the rest — the
 *    visual difference is minor for the export use-case and over-
 *    engineering it for a v1 isn't worth it.
 *
 *  - **Images embed as base64.** Self-contained output is the whole
 *    point — users want one file they can email, post, print. We
 *    rasterise each ImageShape to a temporary canvas of the destination
 *    size, then `toDataURL` it into an `<image href="data:..."/>`. Tint
 *    rides as an overlay `<rect>` with `mix-blend-mode: multiply` to
 *    match the renderer's behaviour.
 *
 *  - **Modes resolved at emit.** `rectMode` / `ellipseMode` are baked
 *    into a uniform top-left + w/h via `resolveBox`, identical to the
 *    GPU path. No SVG-side mode tracking.
 */

/** Top-level entry point. Returns a complete, self-contained SVG document. */
export function sceneToSvg(
  layers: ReadonlyArray<SceneNode>,
  opts: SvgExportOptions,
): string {
  const state: EmitState = { out: [], clipCounter: 0 };
  const w = Math.max(1, Math.floor(opts.width));
  const h = Math.max(1, Math.floor(opts.height));
  state.out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" `,
    `viewBox="0 0 ${w} ${h}" width="${w}" height="${h}">`,
  );
  if (opts.background) {
    state.out.push(`<rect width="${w}" height="${h}" fill="${esc(opts.background)}"/>`);
  }
  // Scene coordinates are centre-origin (the canvas renderer translates by
  // (w/2, h/2) before drawing each frame). Mirror that here, otherwise the
  // whole composition lands in the top-left quadrant, half off-canvas.
  state.out.push(`<g transform="translate(${n(w / 2)},${n(h / 2)})">`);
  for (const layer of layers) {
    if (layer) emitNode(state, layer);
  }
  state.out.push(`</g></svg>`);
  return state.out.join('');
}

export interface SvgExportOptions {
  width: number;
  height: number;
  /** CSS colour for the background rect. Omit / pass null for transparent. */
  background?: string | null;
}

interface EmitState {
  out: string[];
  clipCounter: number;
}

// ─── Visitor ────────────────────────────────────────────────────────────

function emitNode(s: EmitState, n: SceneNode): void {
  switch (n.kind) {
    case 'group':    return emitGroup(s, n);
    case 'rect':     return emitRect(s, n);
    case 'ellipse':  return emitEllipse(s, n);
    case 'line':     return emitLine(s, n);
    case 'arc':      return emitArc(s, n);
    case 'triangle': return emitTriangle(s, n);
    case 'quad':     return emitQuad(s, n);
    case 'point':    return emitPoint(s, n);
    case 'polygon':  return emitPolygon(s, n);
    case 'text':     return emitText(s, n);
    case 'image':    return emitImage(s, n);
    case 'particle-batch': return emitParticleBatch(s, n);
    case 'particle-trail-batch': return emitParticleTrailBatch(s, n);
  }
}

function emitParticleBatch(s: EmitState, b: import('./scene').ParticleBatch): void {
  // SVG export: one element per alive particle. Sized for fidelity, not
  // speed (SVG isn't the hot path — the GPU renderer is).
  const buf = b.buffer;
  const cap = buf.capacity;
  for (let i = 0; i < cap; i++) {
    if (!buf.alive[i]) continue;
    const sz = buf.size[i]!;
    if (sz <= 0) continue;
    // `fill="rgba(...)"` renders black in strict SVG parsers — emit
    // `fill="rgb(...)" fill-opacity="a"` instead (see `paint`).
    const fillAttr = b.useBufferColor
      ? `fill="rgb(${(buf.colorR[i]! * 255) | 0},${(buf.colorG[i]! * 255) | 0},${(buf.colorB[i]! * 255) | 0})" fill-opacity="${buf.colorA[i]}"`
      : paint('fill', b.style.fill ?? '#fff');
    const x = buf.posX[i]!;
    const y = buf.posY[i]!;
    if (!Number.isFinite(x) || !Number.isFinite(y)) continue; // skip diverged particles
    if (b.template.kind === 'rect') {
      s.out.push(`<rect x="${n(x - sz/2)}" y="${n(y - sz/2)}" width="${n(sz)}" height="${n(sz)}" ${fillAttr}/>`);
    } else {
      // circle or layer (layer falls back to circle in SVG for now)
      s.out.push(`<circle cx="${n(x)}" cy="${n(y)}" r="${n(sz/2)}" ${fillAttr}/>`);
    }
  }
}

function emitParticleTrailBatch(s: EmitState, b: import('./scene').ParticleTrailBatch): void {
  const buf = b.buffer;
  const segs = b.segments;
  const head = b.headIdx;
  const baseWidth = b.style.strokeWeight ?? 1;
  const { useBufferColor, tapered, fadeOut, tailWidth } = b;
  // A non-buffer stroke may itself be rgba(...) — split it so the trail
  // honours the style's alpha (and never emits rgba into a stroke attr,
  // which strict SVG parsers render black).
  const styleStrokeM = /^rgba\(\s*([\d.]+),\s*([\d.]+),\s*([\d.]+),\s*([\d.]+)\s*\)$/.exec(b.style.stroke ?? '');
  const styleStroke = styleStrokeM ? `rgb(${styleStrokeM[1]},${styleStrokeM[2]},${styleStrokeM[3]})` : (b.style.stroke ?? '#fff');
  const styleStrokeAlpha = styleStrokeM ? Number(styleStrokeM[4]) : 1;
  // Tapering / fading vary per segment, so they need one <line> per
  // segment; a plain trail collapses to a single fast <polyline>. Matches
  // the canvas renderer's per-segment width/alpha exactly.
  const perSegment = tapered || fadeOut;

  for (let slot = 0; slot < buf.capacity; slot++) {
    if (!buf.alive[slot]) continue;
    const stroke = useBufferColor
      ? `rgb(${(buf.colorR[slot]! * 255) | 0},${(buf.colorG[slot]! * 255) | 0},${(buf.colorB[slot]! * 255) | 0})`
      : styleStroke;
    const baseAlpha = useBufferColor ? buf.colorA[slot]! : styleStrokeAlpha;

    // Collect this slot's points (newest → oldest), stopping at the first
    // uninitialised (NaN) history sample.
    const base = slot * segs * 2;
    const pts: Array<[number, number]> = [];
    for (let seg = 0; seg < segs; seg++) {
      const idx = (head - seg + segs) % segs;
      const x = b.history[base + idx * 2]!;
      const y = b.history[base + idx * 2 + 1]!;
      if (!Number.isFinite(x) || !Number.isFinite(y)) break;
      pts.push([x, y]);
    }
    if (pts.length < 2) continue;

    if (!perSegment) {
      const op = baseAlpha < 1 ? ` stroke-opacity="${baseAlpha}"` : '';
      const pointStr = pts.map((p) => `${n(p[0])},${n(p[1])}`).join(' ');
      s.out.push(`<polyline fill="none" stroke="${stroke}" stroke-width="${n(baseWidth)}" stroke-linecap="round"${op} points="${pointStr}"/>`);
      continue;
    }
    // Per-segment: width tapers head→tail, opacity fades head→tail.
    for (let i = 1; i < pts.length; i++) {
      const ageAvg = (i - 0.5) / segs;
      let w = baseWidth;
      if (tapered) { w = baseWidth * (1 - ageAvg * (1 - tailWidth)); if (w <= 0) w = 0.001; }
      const op = fadeOut ? baseAlpha * (1 - ageAvg) : baseAlpha;
      const opAttr = op < 1 ? ` stroke-opacity="${op.toFixed(3)}"` : '';
      s.out.push(`<line x1="${n(pts[i - 1]![0])}" y1="${n(pts[i - 1]![1])}" x2="${n(pts[i]![0])}" y2="${n(pts[i]![1])}" stroke="${stroke}" stroke-width="${n(w)}" stroke-linecap="round"${opAttr}/>`);
    }
  }
}

function emitGroup(s: EmitState, g: GroupNode): void {
  const attrs: string[] = [];
  if (g.transform) {
    const m = g.transform;
    attrs.push(`transform="matrix(${n(m[0])} ${n(m[1])} ${n(m[2])} ${n(m[3])} ${n(m[4])} ${n(m[5])})"`);
  }
  if (g.blendMode && SVG_BLEND_MODES.has(g.blendMode)) {
    attrs.push(`style="mix-blend-mode:${g.blendMode}"`);
  }
  let clipId: string | null = null;
  if (g.clip) {
    clipId = `clip${s.clipCounter++}`;
    s.out.push(
      `<defs><clipPath id="${clipId}">`,
      `<rect x="${n(g.clip.x)}" y="${n(g.clip.y)}" width="${n(g.clip.w)}" height="${n(g.clip.h)}"/>`,
      `</clipPath></defs>`,
    );
    attrs.push(`clip-path="url(#${clipId})"`);
  }
  s.out.push(attrs.length ? `<g ${attrs.join(' ')}>` : `<g>`);
  for (const child of g.children) emitNode(s, child);
  s.out.push(`</g>`);
}

function emitRect(s: EmitState, r: RectShape): void {
  const [x, y, w, h] = resolveBox(r.x, r.y, r.w, r.h, r.mode);
  s.out.push(`<rect x="${n(x)}" y="${n(y)}" width="${n(w)}" height="${n(h)}" ${styleAttrs(r.style)}/>`);
}

function emitEllipse(s: EmitState, e: EllipseShape): void {
  const [x, y, w, h] = resolveBox(e.x, e.y, e.w, e.h, e.mode);
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = Math.abs(w / 2);
  const ry = Math.abs(h / 2);
  s.out.push(`<ellipse cx="${n(cx)}" cy="${n(cy)}" rx="${n(rx)}" ry="${n(ry)}" ${styleAttrs(e.style)}/>`);
}

function emitLine(s: EmitState, l: LineShape): void {
  if (!l.style.stroke) return;
  s.out.push(`<line x1="${n(l.x1)}" y1="${n(l.y1)}" x2="${n(l.x2)}" y2="${n(l.y2)}" ${strokeAttrs(l.style)} fill="none"/>`);
}

function emitArc(s: EmitState, a: ArcShape): void {
  const [x, y, w, h] = resolveBox(a.x, a.y, a.w, a.h, a.mode);
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = Math.abs(w / 2);
  const ry = Math.abs(h / 2);
  // Build an arc path. SVG arcs need start/end points on the ellipse plus
  // the large-arc and sweep flags. The scene model's arc(start, stop) sweeps
  // from start to stop counterclockwise-positive (in y-down screen space
  // that's clockwise visually). We mirror that by emitting sweep-flag=1.
  const sx = cx + rx * Math.cos(a.start);
  const sy = cy + ry * Math.sin(a.start);
  const ex = cx + rx * Math.cos(a.stop);
  const ey = cy + ry * Math.sin(a.stop);
  const span = a.stop - a.start;
  const large = Math.abs(span) > Math.PI ? 1 : 0;
  const sweep = span >= 0 ? 1 : 0;
  const d = `M ${n(sx)} ${n(sy)} A ${n(rx)} ${n(ry)} 0 ${large} ${sweep} ${n(ex)} ${n(ey)}`;
  s.out.push(`<path d="${d}" ${styleAttrs(a.style)}/>`);
}

function emitTriangle(s: EmitState, t: TriangleShape): void {
  s.out.push(`<polygon points="${n(t.x1)},${n(t.y1)} ${n(t.x2)},${n(t.y2)} ${n(t.x3)},${n(t.y3)}" ${styleAttrs(t.style)}/>`);
}

function emitQuad(s: EmitState, q: QuadShape): void {
  s.out.push(`<polygon points="${n(q.x1)},${n(q.y1)} ${n(q.x2)},${n(q.y2)} ${n(q.x3)},${n(q.y3)} ${n(q.x4)},${n(q.y4)}" ${styleAttrs(q.style)}/>`);
}

function emitPoint(s: EmitState, p: PointShape): void {
  // Renderer draws a tiny square in the stroke colour (falling back to
  // fill). We mirror that here for visual parity.
  const colour = p.style.stroke ?? p.style.fill;
  if (!colour) return;
  const sz = Math.max(1, p.style.strokeWeight);
  s.out.push(`<rect x="${n(p.x - sz / 2)}" y="${n(p.y - sz / 2)}" width="${n(sz)}" height="${n(sz)}" ${paint('fill', colour)}/>`);
}

function emitPolygon(s: EmitState, pg: PolygonShape): void {
  const count = pg.points.length;
  if (count === 0) return;
  const tension = pg.tension ?? 0;
  // Straight polyline / polygon — original fast path.
  if (tension <= 0 || count < 3) {
    const pts = pg.points.map(p => `${n(p.x)},${n(p.y)}`).join(' ');
    const tag = pg.closed ? 'polygon' : 'polyline';
    const extraFill = pg.closed ? '' : ' fill="none"';
    s.out.push(`<${tag} points="${pts}" ${styleAttrs(pg.style)}${extraFill}/>`);
    return;
  }
  // Curved variant — emit an SVG `<path>` with Catmull–Rom segments
  // expressed as cubic Béziers. Same control-point math as the canvas
  // renderer so raster and vector outputs agree pixel-for-pixel.
  const pts = pg.points;
  const parts: string[] = [`M${n(pts[0]!.x)},${n(pts[0]!.y)}`];
  for (const seg of catmullRomBeziers(pts, pg.closed, tension)) {
    parts.push(`C${n(seg.c1x)},${n(seg.c1y)} ${n(seg.c2x)},${n(seg.c2y)} ${n(seg.ex)},${n(seg.ey)}`);
  }
  if (pg.closed) parts.push('Z');
  const extraFill = pg.closed ? '' : ' fill="none"';
  s.out.push(`<path d="${parts.join(' ')}" ${styleAttrs(pg.style)}${extraFill}/>`);
}

function emitText(s: EmitState, t: TextShape): void {
  const anchor = textAnchor(t.style.textAlign);
  const baseline = dominantBaseline(t.style.textBaseline);
  // Emit explicit font-* presentation attributes rather than the CSS `font`
  // shorthand: Illustrator / Figma / many SVG viewers don't honour the
  // shorthand and silently fall back to a default face.
  const fontAttrs = [
    `font-family="${esc(t.style.fontFamily || 'sans-serif')}"`,
    `font-size="${n(t.style.fontSize)}"`,
    t.style.fontWeight && t.style.fontWeight !== 'normal' ? `font-weight="${esc(t.style.fontWeight)}"` : '',
    t.style.fontStyle && t.style.fontStyle !== 'normal' ? `font-style="${esc(t.style.fontStyle)}"` : '',
    t.style.letterSpacing ? `letter-spacing="${n(t.style.letterSpacing)}"` : '',
  ].filter(Boolean).join(' ');
  const lines = t.text.includes('\n') ? t.text.split('\n') : [t.text];
  const dy = t.style.fontSize * t.style.lineHeight;
  if (lines.length === 1) {
    s.out.push(
      `<text x="${n(t.x)}" y="${n(t.y)}" text-anchor="${anchor}" dominant-baseline="${baseline}"`,
      ` ${fontAttrs} ${styleAttrs(t.style)}>${escText(lines[0] ?? '')}</text>`,
    );
    return;
  }
  s.out.push(
    `<text x="${n(t.x)}" y="${n(t.y)}" text-anchor="${anchor}" dominant-baseline="${baseline}"`,
    ` ${fontAttrs} ${styleAttrs(t.style)}>`,
  );
  for (let i = 0; i < lines.length; i++) {
    const yOff = i === 0 ? 0 : dy;
    s.out.push(`<tspan x="${n(t.x)}" dy="${n(yOff)}">${escText(lines[i] ?? '')}</tspan>`);
  }
  s.out.push('</text>');
}

function emitImage(s: EmitState, im: ImageShape): void {
  const dataUrl = imageSourceToDataUrl(im);
  if (!dataUrl) return;
  s.out.push(
    `<image x="${n(im.dst.x)}" y="${n(im.dst.y)}" width="${n(im.dst.w)}" height="${n(im.dst.h)}" `,
    `preserveAspectRatio="none" href="${dataUrl}"/>`,
  );
  if (im.tint) {
    s.out.push(
      `<rect x="${n(im.dst.x)}" y="${n(im.dst.y)}" width="${n(im.dst.w)}" height="${n(im.dst.h)}" `,
      `${paint('fill', im.tint)} style="mix-blend-mode:multiply"/>`,
    );
  }
}

// ─── Style helpers ──────────────────────────────────────────────────────

function styleAttrs(s: ResolvedStyle): string {
  const parts: string[] = [];
  parts.push(s.fill ? paint('fill', s.fill) : `fill="none"`);
  if (s.stroke) {
    parts.push(paint('stroke', s.stroke));
    parts.push(`stroke-width="${n(s.strokeWeight)}"`);
    if (s.strokeCap !== 'butt') parts.push(`stroke-linecap="${s.strokeCap}"`);
    if (s.strokeJoin !== 'miter') parts.push(`stroke-linejoin="${s.strokeJoin}"`);
  }
  return parts.join(' ');
}

/** Stroke-only attrs — for shapes that never fill (lines). */
function strokeAttrs(s: ResolvedStyle): string {
  const parts: string[] = [];
  parts.push(paint('stroke', s.stroke!));
  parts.push(`stroke-width="${n(s.strokeWeight)}"`);
  if (s.strokeCap !== 'butt') parts.push(`stroke-linecap="${s.strokeCap}"`);
  if (s.strokeJoin !== 'miter') parts.push(`stroke-linejoin="${s.strokeJoin}"`);
  return parts.join(' ');
}

function textAnchor(a: string): string {
  return a === 'center' ? 'middle' : a === 'right' ? 'end' : 'start';
}

function dominantBaseline(b: string): string {
  switch (b) {
    case 'top':    return 'hanging';
    case 'middle': return 'middle';
    case 'bottom': return 'text-after-edge';
    default:       return 'alphabetic';
  }
}

/**
 * Scene blend-mode keywords (the CSS/SVG blend-mode set) that map 1:1 to CSS
 * `mix-blend-mode`. Anything else (source-over, destination-in, …) we
 * silently drop — those are compositing operators, not blend modes, and
 * SVG has no portable equivalent.
 */
const SVG_BLEND_MODES = new Set<string>([
  'normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten',
  'color-dodge', 'color-burn', 'hard-light', 'soft-light', 'difference',
  'exclusion', 'hue', 'saturation', 'color', 'luminosity', 'plus-lighter',
]);

// ─── Helpers ────────────────────────────────────────────────────────────


/**
 * Emit a `fill`/`stroke` paint attribute plus a matching opacity attribute.
 * Floret's colours arrive as `rgba(r,g,b,a)` whenever alpha < 1, but SVG's
 * presentation attributes don't reliably accept `rgba()` — strict parsers
 * (Illustrator, Figma, many standalone viewers) fail to parse it and fall
 * back to the default paint, which is BLACK. So `rgba(...)` is split into
 * `role="rgb(r,g,b)" role-opacity="a"`; hex / named / `rgb()` colours pass
 * through unchanged with no opacity attr.
 */
function paint(role: 'fill' | 'stroke', css: string): string {
  const m = /^rgba\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*\)$/.exec(css);
  if (m) return `${role}="rgb(${m[1]},${m[2]},${m[3]})" ${role}-opacity="${m[4]}"`;
  return `${role}="${esc(css)}"`;
}

/**
 * Format a coordinate / length for an SVG attribute. Two safeguards that
 * keep exports robust across arbitrary scenes:
 *   - Round to 3dp. Raw scene floats carry 15-digit / exponential noise
 *     (e.g. `1.347e-14`) that bloats the file and trips stricter parsers;
 *     sub-pixel precision is irrelevant for vector output anyway.
 *   - Collapse non-finite values to 0. A diverged sim (physics blow-up,
 *     normalise-of-zero, /0 position) can emit NaN/Infinity — the GPU
 *     renderer silently skips those, but `cx="NaN"` breaks the element (and,
 *     in strict parsers, the whole document). Degrading to a harmless point
 *     at the origin matches the renderer's "drew nothing" outcome closely.
 */
function n(v: number): string {
  if (!Number.isFinite(v)) return '0';
  return (Math.round(v * 1000) / 1000).toString();
}

/** Escape for attribute values. */
function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** Escape for text content (no need to touch quotes). */
function escText(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * Materialise an ImageShape's source into a data URL. Strategy depends on
 * the source type:
 *
 *  - HTMLCanvasElement / OffscreenCanvas: read pixels via toDataURL.
 *  - HTMLImageElement with `src` already a data URL: pass it through.
 *  - Anything else (raw bitmap, image element with blob URL): copy the
 *    needed region onto a temporary canvas, then toDataURL. This is the
 *    only branch that respects `src` rect cropping; the canvas branches
 *    crop by drawing onto a sized scratch surface too.
 *
 * Returns null if conversion isn't possible (e.g. an HTMLImageElement
 * that hasn't loaded yet, a cross-origin image that taints the canvas).
 * The visitor skips drawing in that case rather than emitting a broken
 * `<image>` tag.
 */
function imageSourceToDataUrl(im: ImageShape): string | null {
  const src = im.source;
  // Determine the source rect (the slice we copy out of the source).
  const srcRect = im.src ?? sourceFullRect(src);
  if (!srcRect) return null;

  // Fast path: HTMLImageElement whose src IS already a data URL AND we're
  // drawing the whole image. Skip the canvas round-trip — significantly
  // smaller output for large images already encoded once.
  if (
    !im.src &&
    typeof HTMLImageElement !== 'undefined' &&
    src instanceof HTMLImageElement &&
    src.src.startsWith('data:')
  ) {
    return src.src;
  }

  // General path: rasterise onto a scratch canvas sized to the source rect.
  try {
    const scratch = document.createElement('canvas');
    scratch.width = Math.max(1, Math.floor(srcRect.w));
    scratch.height = Math.max(1, Math.floor(srcRect.h));
    const sctx = scratch.getContext('2d');
    if (!sctx) return null;
    sctx.drawImage(
      src, srcRect.x, srcRect.y, srcRect.w, srcRect.h,
      0, 0, scratch.width, scratch.height,
    );
    return scratch.toDataURL('image/png');
  } catch {
    // SecurityError on tainted canvas, or any other failure — skip the image.
    return null;
  }
}

/** Best-effort full-source rect for a CanvasImageSource. */
function sourceFullRect(src: CanvasImageSource): { x: number; y: number; w: number; h: number } | null {
  if (typeof HTMLCanvasElement !== 'undefined' && src instanceof HTMLCanvasElement) {
    return { x: 0, y: 0, w: src.width, h: src.height };
  }
  if (typeof HTMLImageElement !== 'undefined' && src instanceof HTMLImageElement) {
    const w = src.naturalWidth || src.width;
    const h = src.naturalHeight || src.height;
    return w && h ? { x: 0, y: 0, w, h } : null;
  }
  if (typeof OffscreenCanvas !== 'undefined' && src instanceof OffscreenCanvas) {
    return { x: 0, y: 0, w: src.width, h: src.height };
  }
  if (typeof ImageBitmap !== 'undefined' && src instanceof ImageBitmap) {
    return { x: 0, y: 0, w: src.width, h: src.height };
  }
  return null;
}
