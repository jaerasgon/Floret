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

/**
 * Walk a SceneNode tree, emit an SVG document.
 *
 * The companion to `scene-renderer.ts`: same visitor shape, same shape
 * vocabulary, different output. Where the renderer assigns `ctx.fillStyle`
 * and calls `ctx.fillRect`, this visitor writes `fill="..."` and `<rect/>`.
 *
 * Design notes:
 *
 *  - **String building.** Push fragments into an array, join once at the
 *    end. Avoids the quadratic cost of repeated string concat in a hot
 *    path. The cost we care about is the FIRST export — users won't be
 *    running this 60fps — but tidy is tidy.
 *
 *  - **Transforms align with SVG by construction.** Canvas2D's affine
 *    `[a b c d e f]` IS SVG's `matrix(a b c d e f)`. No conversion. A
 *    group with `transform` becomes `<g transform="matrix(a b c d e f)">`.
 *
 *  - **Style as inline attributes.** Every shape carries its own
 *    ResolvedStyle so there's no benefit to <defs>+class indirection
 *    today. Inline keeps the output legible and editable in a vector tool.
 *
 *  - **Blend modes.** Canvas2D's globalCompositeOperation overlaps with
 *    CSS `mix-blend-mode` for the actual *blend* keywords (multiply,
 *    screen, overlay, …) but also includes compositing operators
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
 *    Canvas2D renderer. No SVG-side mode tracking.
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
  for (const layer of layers) {
    if (layer) emitNode(state, layer);
  }
  state.out.push(`</svg>`);
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
  }
}

function emitGroup(s: EmitState, g: GroupNode): void {
  const attrs: string[] = [];
  if (g.transform) {
    const m = g.transform;
    attrs.push(`transform="matrix(${m[0]} ${m[1]} ${m[2]} ${m[3]} ${m[4]} ${m[5]})"`);
  }
  if (g.blendMode && SVG_BLEND_MODES.has(g.blendMode)) {
    attrs.push(`style="mix-blend-mode:${g.blendMode}"`);
  }
  let clipId: string | null = null;
  if (g.clip) {
    clipId = `clip${s.clipCounter++}`;
    s.out.push(
      `<defs><clipPath id="${clipId}">`,
      `<rect x="${g.clip.x}" y="${g.clip.y}" width="${g.clip.w}" height="${g.clip.h}"/>`,
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
  s.out.push(`<rect x="${x}" y="${y}" width="${w}" height="${h}" ${styleAttrs(r.style)}/>`);
}

function emitEllipse(s: EmitState, e: EllipseShape): void {
  const [x, y, w, h] = resolveBox(e.x, e.y, e.w, e.h, e.mode);
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = Math.abs(w / 2);
  const ry = Math.abs(h / 2);
  s.out.push(`<ellipse cx="${cx}" cy="${cy}" rx="${rx}" ry="${ry}" ${styleAttrs(e.style)}/>`);
}

function emitLine(s: EmitState, l: LineShape): void {
  if (!l.style.stroke) return;
  s.out.push(`<line x1="${l.x1}" y1="${l.y1}" x2="${l.x2}" y2="${l.y2}" ${strokeAttrs(l.style)} fill="none"/>`);
}

function emitArc(s: EmitState, a: ArcShape): void {
  const [x, y, w, h] = resolveBox(a.x, a.y, a.w, a.h, a.mode);
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = Math.abs(w / 2);
  const ry = Math.abs(h / 2);
  // Build an arc path. SVG arcs need start/end points on the ellipse plus
  // the large-arc and sweep flags. Canvas2D's ellipse(start, stop) sweeps
  // from start to stop counterclockwise-positive (in y-down screen space
  // that's clockwise visually). We mirror that by emitting sweep-flag=1.
  const sx = cx + rx * Math.cos(a.start);
  const sy = cy + ry * Math.sin(a.start);
  const ex = cx + rx * Math.cos(a.stop);
  const ey = cy + ry * Math.sin(a.stop);
  const span = a.stop - a.start;
  const large = Math.abs(span) > Math.PI ? 1 : 0;
  const sweep = span >= 0 ? 1 : 0;
  const d = `M ${sx} ${sy} A ${rx} ${ry} 0 ${large} ${sweep} ${ex} ${ey}`;
  s.out.push(`<path d="${d}" ${styleAttrs(a.style)}/>`);
}

function emitTriangle(s: EmitState, t: TriangleShape): void {
  s.out.push(`<polygon points="${t.x1},${t.y1} ${t.x2},${t.y2} ${t.x3},${t.y3}" ${styleAttrs(t.style)}/>`);
}

function emitQuad(s: EmitState, q: QuadShape): void {
  s.out.push(`<polygon points="${q.x1},${q.y1} ${q.x2},${q.y2} ${q.x3},${q.y3} ${q.x4},${q.y4}" ${styleAttrs(q.style)}/>`);
}

function emitPoint(s: EmitState, p: PointShape): void {
  // Renderer draws a tiny square in the stroke colour (falling back to
  // fill). We mirror that here for visual parity.
  const colour = p.style.stroke ?? p.style.fill;
  if (!colour) return;
  const sz = Math.max(1, p.style.strokeWeight);
  s.out.push(`<rect x="${p.x - sz / 2}" y="${p.y - sz / 2}" width="${sz}" height="${sz}" fill="${esc(colour)}"/>`);
}

function emitPolygon(s: EmitState, pg: PolygonShape): void {
  const n = pg.points.length;
  if (n === 0) return;
  const tension = pg.tension ?? 0;
  // Straight polyline / polygon — original fast path.
  if (tension <= 0 || n < 3) {
    const pts = pg.points.map(p => `${p.x},${p.y}`).join(' ');
    const tag = pg.closed ? 'polygon' : 'polyline';
    const extraFill = pg.closed ? '' : ' fill="none"';
    s.out.push(`<${tag} points="${pts}" ${styleAttrs(pg.style)}${extraFill}/>`);
    return;
  }
  // Curved variant — emit an SVG `<path>` with Catmull–Rom segments
  // expressed as cubic Béziers. Same control-point math as the canvas
  // renderer so raster and vector outputs agree pixel-for-pixel.
  const pts = pg.points;
  const k = tension / 6;
  const parts: string[] = [`M${pts[0]!.x},${pts[0]!.y}`];
  const last = pg.closed ? n : n - 1;
  for (let i = 0; i < last; i++) {
    const p0 = pts[pg.closed ? (i - 1 + n) % n : Math.max(0, i - 1)]!;
    const p1 = pts[i]!;
    const p2 = pts[pg.closed ? (i + 1) % n : i + 1]!;
    const p3 = pts[pg.closed ? (i + 2) % n : Math.min(n - 1, i + 2)]!;
    const c1x = p1.x + (p2.x - p0.x) * k;
    const c1y = p1.y + (p2.y - p0.y) * k;
    const c2x = p2.x - (p3.x - p1.x) * k;
    const c2y = p2.y - (p3.y - p1.y) * k;
    parts.push(`C${c1x},${c1y} ${c2x},${c2y} ${p2.x},${p2.y}`);
  }
  if (pg.closed) parts.push('Z');
  const extraFill = pg.closed ? '' : ' fill="none"';
  s.out.push(`<path d="${parts.join(' ')}" ${styleAttrs(pg.style)}${extraFill}/>`);
}

function emitText(s: EmitState, t: TextShape): void {
  const anchor = textAnchor(t.style.textAlign);
  const baseline = dominantBaseline(t.style.textBaseline);
  const fontAttr = ` style="font:${esc(t.style.font)}"`;
  s.out.push(
    `<text x="${t.x}" y="${t.y}" text-anchor="${anchor}" dominant-baseline="${baseline}"`,
    `${fontAttr} ${styleAttrs(t.style)}>${escText(t.text)}</text>`,
  );
}

function emitImage(s: EmitState, im: ImageShape): void {
  const dataUrl = imageSourceToDataUrl(im);
  if (!dataUrl) return;
  s.out.push(
    `<image x="${im.dst.x}" y="${im.dst.y}" width="${im.dst.w}" height="${im.dst.h}" `,
    `preserveAspectRatio="none" href="${dataUrl}"/>`,
  );
  if (im.tint) {
    s.out.push(
      `<rect x="${im.dst.x}" y="${im.dst.y}" width="${im.dst.w}" height="${im.dst.h}" `,
      `fill="${esc(im.tint)}" style="mix-blend-mode:multiply"/>`,
    );
  }
}

// ─── Style helpers ──────────────────────────────────────────────────────

function styleAttrs(s: ResolvedStyle): string {
  const parts: string[] = [];
  parts.push(s.fill ? `fill="${esc(s.fill)}"` : `fill="none"`);
  if (s.stroke) {
    parts.push(`stroke="${esc(s.stroke)}"`);
    parts.push(`stroke-width="${s.strokeWeight}"`);
    if (s.strokeCap !== 'butt') parts.push(`stroke-linecap="${s.strokeCap}"`);
    if (s.strokeJoin !== 'miter') parts.push(`stroke-linejoin="${s.strokeJoin}"`);
  }
  return parts.join(' ');
}

/** Stroke-only attrs — for shapes that never fill (lines). */
function strokeAttrs(s: ResolvedStyle): string {
  const parts: string[] = [];
  parts.push(`stroke="${esc(s.stroke!)}"`);
  parts.push(`stroke-width="${s.strokeWeight}"`);
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
 * Canvas2D `globalCompositeOperation` keywords that map 1:1 to CSS
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

/** Resolve mode-aware (x, y, w, h) into uniform top-left + w/h. Matches
 *  the renderer's resolveBox 1:1 so SVG and canvas frame identically. */
function resolveBox(x: number, y: number, w: number, h: number, mode: string): [number, number, number, number] {
  switch (mode) {
    case 'center':  return [x - w / 2, y - h / 2, w, h];
    case 'corners': return [Math.min(x, w), Math.min(y, h), Math.abs(w - x), Math.abs(h - y)];
    case 'radius':  return [x - w, y - h, w * 2, h * 2];
    case 'corner':
    default:        return [x, y, w, h];
  }
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
