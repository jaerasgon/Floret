import type {
  Affine,
  EllipseShape,
  GroupNode,
  ImageShape,
  LineShape,
  ParticleBatch,
  ParticleTrailBatch,
  PolygonShape,
  RectShape,
  RenderingContext2D,
  ResolvedStyle,
  SceneNode,
  TextShape,
} from './scene';
import { composeFontCss, resolveBox, catmullRomBeziers } from './scene';

/**
 * Walk a SceneNode tree, emit Canvas2D calls.
 *
 * The renderer is the IM-mode "output" of our retained-mode model.
 * Future SVG/PDF visitors walk the same tree but emit XML / vector
 * graphics ops instead — neither knows about the other.
 *
 * Design notes:
 *
 *  - **Statelessness across siblings.** Every shape carries its own
 *    `ResolvedStyle`. We don't try to deduce "the previous shape
 *    already set ctx.fillStyle to this; skip the assignment" — that
 *    optimisation belongs to a future batched renderer with a
 *    sort-by-style pass. Today's renderer assigns on every shape; the
 *    Canvas2D fast paths handle no-op assignments fine.
 *
 *  - **Transforms via save/restore.** Group nodes with a transform
 *    save the matrix, apply, recurse, restore. Same for clip rects
 *    and blend modes — anything affecting context state is bracketed.
 *
 *  - **Modes baked into resolution.** `rectMode` / `ellipseMode` are
 *    resolved INSIDE the shape's switch case, not via global state.
 *    The shape carries its mode field as a flat description, never as
 *    "set this mode then draw."
 */

export function renderScene(ctx: RenderingContext2D, node: SceneNode): void {
  switch (node.kind) {
    case 'group': return renderGroup(ctx, node);
    case 'rect': return renderRect(ctx, node);
    case 'ellipse': return renderEllipse(ctx, node);
    case 'line': return renderLine(ctx, node);
    case 'arc': return renderArc(ctx, node);
    case 'triangle': return renderTriangle(ctx, node);
    case 'quad': return renderQuad(ctx, node);
    case 'point': return renderPoint(ctx, node);
    case 'polygon': return renderPolygon(ctx, node);
    case 'text': return renderText(ctx, node);
    case 'image': return renderImage(ctx, node);
    case 'particle-batch': return renderParticleBatch(ctx, node);
    case 'particle-trail-batch': return renderParticleTrailBatch(ctx, node);
  }
}

/** Apply style to context for fill+stroke decisions on the next draw. */
function applyStyle(ctx: RenderingContext2D, s: ResolvedStyle): void {
  if (s.fill) ctx.fillStyle = s.fill;
  if (s.stroke) {
    ctx.strokeStyle = s.stroke;
    ctx.lineWidth = s.strokeWeight;
    ctx.lineCap = s.strokeCap;
    ctx.lineJoin = s.strokeJoin;
  }
}

/**
 * Stroke a closed shape honouring `strokeAlign`. Canvas2D only strokes
 * centred on the path; `inside` / `outside` are emulated by clipping to
 * (or away from) the shape and stroking at 2× width so only the desired
 * half of the doubled stroke survives. `buildPath` must begin its own
 * path (`beginPath`) and trace the closed outline.
 */
function strokeShape(ctx: RenderingContext2D, s: ResolvedStyle, buildPath: () => void): void {
  if (!s.stroke) return;
  const align = s.strokeAlign ?? 'center';
  if (align === 'center') {
    buildPath();
    ctx.stroke();
    return;
  }
  ctx.save();
  buildPath();
  if (align === 'inside') {
    ctx.clip();
  } else {
    // outside: even-odd clip of a huge rect XOR the shape → exterior only,
    // so the inner half of the doubled stroke is cut away.
    ctx.rect(-1e6, -1e6, 2e6, 2e6);
    ctx.clip('evenodd');
  }
  ctx.lineWidth = s.strokeWeight * 2;
  buildPath();
  ctx.stroke();
  ctx.restore();
}

function applyTextStyle(ctx: RenderingContext2D, s: ResolvedStyle): void {
  ctx.font = composeFontCss(s);
  ctx.textAlign = s.textAlign;
  ctx.textBaseline = s.textBaseline;
  // Canvas2D `letterSpacing` is supported in modern browsers as a string
  // ("2px"). Older runtimes ignore the assignment silently — guard with
  // an `in` check to avoid bumping a typed-array property by accident.
  if ('letterSpacing' in ctx) {
    (ctx as unknown as { letterSpacing: string }).letterSpacing = `${s.letterSpacing}px`;
  }
}

// ─── Group ──────────────────────────────────────────────────────────────

function renderGroup(ctx: RenderingContext2D, g: GroupNode): void {
  const hasTransform = !!g.transform;
  const hasBlend = !!g.blendMode;
  const hasClip = !!g.clip;
  const needsSave = hasTransform || hasBlend || hasClip;
  if (needsSave) ctx.save();
  if (hasTransform) applyTransform(ctx, g.transform!);
  if (hasBlend) ctx.globalCompositeOperation = g.blendMode!;
  if (hasClip) {
    ctx.beginPath();
    ctx.rect(g.clip!.x, g.clip!.y, g.clip!.w, g.clip!.h);
    ctx.clip();
  }
  for (const child of g.children) renderScene(ctx, child);
  if (needsSave) ctx.restore();
}

/** Multiply the current transform by `m` (Canvas2D `transform()`
 *  semantics — concatenates, doesn't replace). */
function applyTransform(ctx: RenderingContext2D, m: Affine): void {
  ctx.transform(m[0], m[1], m[2], m[3], m[4], m[5]);
}

// ─── Shape rendering ────────────────────────────────────────────────────

function renderRect(ctx: RenderingContext2D, r: RectShape): void {
  applyStyle(ctx, r.style);
  const [x, y, w, h] = resolveBox(r.x, r.y, r.w, r.h, r.mode);
  const build = () => { ctx.beginPath(); ctx.rect(x, y, w, h); };
  if (r.style.fill) { build(); ctx.fill(); }
  strokeShape(ctx, r.style, build);
}

function renderEllipse(ctx: RenderingContext2D, e: EllipseShape): void {
  applyStyle(ctx, e.style);
  const [x, y, w, h] = resolveBox(e.x, e.y, e.w, e.h, e.mode);
  // p5 default ellipseMode is CENTER (x,y is the centre). We resolve to
  // a top-left+w/h box above, then compute the centre + radii here.
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = w / 2;
  const ry = h / 2;
  const build = () => { ctx.beginPath(); ctx.ellipse(cx, cy, Math.abs(rx), Math.abs(ry), 0, 0, Math.PI * 2); };
  if (e.style.fill) { build(); ctx.fill(); }
  strokeShape(ctx, e.style, build);
}

function renderLine(ctx: RenderingContext2D, l: LineShape): void {
  if (!l.style.stroke) return;
  applyStyle(ctx, l.style);
  ctx.beginPath();
  ctx.moveTo(l.x1, l.y1);
  ctx.lineTo(l.x2, l.y2);
  ctx.stroke();
}

function renderArc(ctx: RenderingContext2D, a: import('./scene').ArcShape): void {
  applyStyle(ctx, a.style);
  const [x, y, w, h] = resolveBox(a.x, a.y, a.w, a.h, a.mode);
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = w / 2;
  const ry = h / 2;
  const build = () => { ctx.beginPath(); ctx.ellipse(cx, cy, Math.abs(rx), Math.abs(ry), 0, a.start, a.stop); };
  if (a.style.fill) { build(); ctx.fill(); }
  strokeShape(ctx, a.style, build);
}

function renderTriangle(ctx: RenderingContext2D, t: import('./scene').TriangleShape): void {
  applyStyle(ctx, t.style);
  const build = () => {
    ctx.beginPath();
    ctx.moveTo(t.x1, t.y1);
    ctx.lineTo(t.x2, t.y2);
    ctx.lineTo(t.x3, t.y3);
    ctx.closePath();
  };
  if (t.style.fill) { build(); ctx.fill(); }
  strokeShape(ctx, t.style, build);
}

function renderQuad(ctx: RenderingContext2D, q: import('./scene').QuadShape): void {
  applyStyle(ctx, q.style);
  const build = () => {
    ctx.beginPath();
    ctx.moveTo(q.x1, q.y1);
    ctx.lineTo(q.x2, q.y2);
    ctx.lineTo(q.x3, q.y3);
    ctx.lineTo(q.x4, q.y4);
    ctx.closePath();
  };
  if (q.style.fill) { build(); ctx.fill(); }
  strokeShape(ctx, q.style, build);
}

function renderPoint(ctx: RenderingContext2D, p: import('./scene').PointShape): void {
  // p5's `point()` uses the stroke colour. We render a tiny filled
  // rect for performance (single pixel via fillRect with strokeWeight
  // as the size). Slightly different geometry than p5's "round dot"
  // but visually equivalent at strokeWeight 1-2; user can switch to
  // ellipse for round dots if needed.
  const colour = p.style.stroke ?? p.style.fill;
  if (!colour) return;
  ctx.fillStyle = colour;
  const sz = Math.max(1, p.style.strokeWeight);
  ctx.fillRect(p.x - sz / 2, p.y - sz / 2, sz, sz);
}

function renderPolygon(ctx: RenderingContext2D, pg: PolygonShape): void {
  const n = pg.points.length;
  if (n === 0) return;
  applyStyle(ctx, pg.style);
  const build = () => { tracePolygon(ctx, pg); };
  if (pg.closed) {
    if (pg.style.fill) { build(); ctx.fill(); }
    // strokeAlign (inside/outside) only makes sense for a closed region.
    strokeShape(ctx, pg.style, build);
  } else if (pg.style.stroke) {
    build();
    ctx.stroke();
  }
}

/** Trace a polygon's outline into the current path (begins a fresh path).
 *  Straight polyline by default; Catmull–Rom Béziers when `tension > 0`. */
function tracePolygon(ctx: RenderingContext2D, pg: PolygonShape): void {
  const n = pg.points.length;
  ctx.beginPath();
  const tension = pg.tension ?? 0;
  if (tension <= 0 || n < 3) {
    // Straight polyline — original behaviour.
    ctx.moveTo(pg.points[0]!.x, pg.points[0]!.y);
    for (let i = 1; i < n; i++) {
      ctx.lineTo(pg.points[i]!.x, pg.points[i]!.y);
    }
  } else {
    // Catmull–Rom → cubic Bézier conversion. For each segment p1 → p2 we
    // need a "previous" p0 and a "next" p3 to derive the two control
    // points. At the endpoints we mirror the neighbour so the curve
    // still passes through the first/last point cleanly.
    //
    // Bézier control points:
    //   c1 = p1 + (p2 - p0) * (tension / 6)
    //   c2 = p2 - (p3 - p1) * (tension / 6)
    // The /6 normalises so tension=1 reproduces the canonical
    // uniform Catmull–Rom (which is itself a tau=0.5 Cardinal spline).
    const pts = pg.points;
    ctx.moveTo(pts[0]!.x, pts[0]!.y);
    for (const seg of catmullRomBeziers(pts, pg.closed, tension)) {
      ctx.bezierCurveTo(seg.c1x, seg.c1y, seg.c2x, seg.c2y, seg.ex, seg.ey);
    }
  }
  if (pg.closed) ctx.closePath();
}

function renderText(ctx: RenderingContext2D, t: TextShape): void {
  applyStyle(ctx, t.style);
  applyTextStyle(ctx, t.style);
  const lines = t.text.includes('\n') ? t.text.split('\n') : [t.text];
  const dy = t.style.fontSize * t.style.lineHeight;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    const y = t.y + i * dy;
    if (t.style.fill) ctx.fillText(line, t.x, y);
    if (t.style.stroke) ctx.strokeText(line, t.x, y);
  }
}

function renderImage(ctx: RenderingContext2D, im: ImageShape): void {
  if (im.tint) {
    // Tint via composite: draw image, then multiply tint colour through.
    // Cheap path; for a true per-pixel tint we'd need an offscreen pass.
    if (im.src) {
      ctx.drawImage(
        im.source, im.src.x, im.src.y, im.src.w, im.src.h,
        im.dst.x, im.dst.y, im.dst.w, im.dst.h,
      );
    } else {
      ctx.drawImage(im.source, im.dst.x, im.dst.y, im.dst.w, im.dst.h);
    }
    ctx.save();
    ctx.globalCompositeOperation = 'multiply';
    ctx.fillStyle = im.tint;
    ctx.fillRect(im.dst.x, im.dst.y, im.dst.w, im.dst.h);
    ctx.restore();
    return;
  }
  if (im.src) {
    ctx.drawImage(
      im.source, im.src.x, im.src.y, im.src.w, im.src.h,
      im.dst.x, im.dst.y, im.dst.w, im.dst.h,
    );
  } else {
    ctx.drawImage(im.source, im.dst.x, im.dst.y, im.dst.w, im.dst.h);
  }
}

// ─── Particle batches ───────────────────────────────────────────────────

/**
 * Draw every alive particle of `b.buffer` with the given template. One
 * tight loop over the typed arrays — the CPU analogue of GPU instancing.
 *
 * Performance notes:
 * - We pull array references into locals so V8 can keep them in
 *   registers across iterations.
 * - For the `circle` template we reuse a single Path2D-style arc; only
 *   `fillStyle` and the transform change per particle.
 * - For the `layer` template we `save/translate/scale/restore` and walk
 *   the sub-tree per particle. Cheaper than allocating N translated
 *   copies of the SceneNode.
 */
function renderParticleBatch(ctx: RenderingContext2D, b: ParticleBatch): void {
  const buf = b.buffer;
  const cap = buf.capacity;
  const alive = buf.alive;
  const px = buf.posX, py = buf.posY;
  const sz = buf.size;
  const cr = buf.colorR, cg = buf.colorG, cb = buf.colorB, ca = buf.colorA;
  const useColor = b.useBufferColor;
  applyStyle(ctx, b.style);

  if (b.template.kind === 'circle') {
    for (let i = 0; i < cap; i++) {
      if (!alive[i]) continue;
      const r = sz[i]! * 0.5;
      if (r <= 0) continue;
      if (useColor) {
        ctx.fillStyle = packRgba(cr[i]!, cg[i]!, cb[i]!, ca[i]!);
      }
      ctx.beginPath();
      ctx.arc(px[i]!, py[i]!, r, 0, TAU);
      if (b.style.fill || useColor) ctx.fill();
      if (b.style.stroke) ctx.stroke();
    }
    return;
  }

  if (b.template.kind === 'rect') {
    for (let i = 0; i < cap; i++) {
      if (!alive[i]) continue;
      const s = sz[i]!;
      if (s <= 0) continue;
      if (useColor) {
        ctx.fillStyle = packRgba(cr[i]!, cg[i]!, cb[i]!, ca[i]!);
      }
      const x = px[i]! - s * 0.5;
      const y = py[i]! - s * 0.5;
      if (b.style.fill || useColor) ctx.fillRect(x, y, s, s);
      if (b.style.stroke) ctx.strokeRect(x, y, s, s);
    }
    return;
  }

  if (b.template.kind === 'layer') {
    const tpl = b.template.node;
    // Template is authored at size = 1 (unit shape); per-particle size
    // scales the whole template uniformly.
    for (let i = 0; i < cap; i++) {
      if (!alive[i]) continue;
      const s = sz[i]!;
      if (s <= 0) continue;
      ctx.save();
      ctx.translate(px[i]!, py[i]!);
      const rot = buf.rotation[i]!;
      if (rot !== 0) ctx.rotate(rot);
      if (s !== 1) ctx.scale(s, s);
      renderScene(ctx, tpl);
      ctx.restore();
    }
  }
}

/**
 * Draw a polyline trail per alive particle. `history` is interleaved
 * (x, y) per segment per slot. We walk segments backwards from
 * `headIdx` (most recent) so the line is drawn front-to-back.
 *
 * When `fadeOut` is true each segment's alpha decays linearly with its
 * age so the trail tail fades into the background.
 */
function renderParticleTrailBatch(
  ctx: RenderingContext2D,
  b: ParticleTrailBatch,
): void {
  const buf = b.buffer;
  const cap = buf.capacity;
  const alive = buf.alive;
  const segs = b.segments;
  const hist = b.history;
  const head = b.headIdx;
  const useColor = b.useBufferColor;
  const fadeOut = b.fadeOut;
  const tapered = b.tapered;
  const tailWidth = b.tailWidth;
  // Either modifier forces per-segment strokes; without either we can
  // do a single fast-path polyline per slot.
  const perSegment = fadeOut || tapered;
  const baseWidth = b.style.strokeWeight;
  const cr = buf.colorR, cg = buf.colorG, cbr = buf.colorB, ca = buf.colorA;
  applyStyle(ctx, b.style);

  for (let slot = 0; slot < cap; slot++) {
    if (!alive[slot]) continue;
    // Per-particle stroke colour pulled from the buffer when requested.
    // Setting strokeStyle once per slot is cheap; once per segment would
    // be wasted churn for the uniform-color path.
    if (useColor) {
      ctx.strokeStyle = packRgba(cr[slot]!, cg[slot]!, cbr[slot]!, ca[slot]!);
    }
    const baseHist = slot * segs * 2;
    let started = false;
    // Walk newest → oldest.
    for (let s = 0; s < segs; s++) {
      const idx = (head - s + segs) % segs;
      const x = hist[baseHist + idx * 2]!;
      const y = hist[baseHist + idx * 2 + 1]!;
      // Sentinel: uninitialised history slots are NaN — skip those.
      if (!Number.isFinite(x) || !Number.isFinite(y)) break;
      if (!started) {
        ctx.beginPath();
        ctx.moveTo(x, y);
        started = true;
      } else {
        if (perSegment) {
          // Segment travels from age (s-1) to age s. Width samples at
          // the average age so segments meet smoothly at vertices.
          const ageAvg = (s - 0.5) / segs;
          if (fadeOut) ctx.globalAlpha = 1 - ageAvg;
          if (tapered) {
            // 1 at head (ageAvg=0), `tailWidth` at tail (ageAvg=1).
            const w = baseWidth * (1 - ageAvg * (1 - tailWidth));
            ctx.lineWidth = w > 0 ? w : 0.001;
          }
          ctx.lineTo(x, y);
          ctx.stroke();
          ctx.beginPath();
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
      }
    }
    if (started && !perSegment) ctx.stroke();
  }
  if (fadeOut) ctx.globalAlpha = 1;
  if (tapered) ctx.lineWidth = baseWidth;
}

function packRgba(r: number, g: number, b: number, a: number): string {
  return `rgba(${(r * 255) | 0},${(g * 255) | 0},${(b * 255) | 0},${a})`;
}

const TAU = Math.PI * 2;
