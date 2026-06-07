import type {
  Affine,
  EllipseShape,
  GroupNode,
  ImageShape,
  LineShape,
  PolygonShape,
  RectShape,
  RenderingContext2D,
  ResolvedStyle,
  SceneNode,
  TextShape,
} from './scene';

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

function applyTextStyle(ctx: RenderingContext2D, s: ResolvedStyle): void {
  ctx.font = s.font;
  ctx.textAlign = s.textAlign;
  ctx.textBaseline = s.textBaseline;
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
  if (r.style.fill) ctx.fillRect(x, y, w, h);
  if (r.style.stroke) ctx.strokeRect(x, y, w, h);
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
  ctx.beginPath();
  ctx.ellipse(cx, cy, Math.abs(rx), Math.abs(ry), 0, 0, Math.PI * 2);
  if (e.style.fill) ctx.fill();
  if (e.style.stroke) ctx.stroke();
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
  ctx.beginPath();
  ctx.ellipse(cx, cy, Math.abs(rx), Math.abs(ry), 0, a.start, a.stop);
  if (a.style.fill) ctx.fill();
  if (a.style.stroke) ctx.stroke();
}

function renderTriangle(ctx: RenderingContext2D, t: import('./scene').TriangleShape): void {
  applyStyle(ctx, t.style);
  ctx.beginPath();
  ctx.moveTo(t.x1, t.y1);
  ctx.lineTo(t.x2, t.y2);
  ctx.lineTo(t.x3, t.y3);
  ctx.closePath();
  if (t.style.fill) ctx.fill();
  if (t.style.stroke) ctx.stroke();
}

function renderQuad(ctx: RenderingContext2D, q: import('./scene').QuadShape): void {
  applyStyle(ctx, q.style);
  ctx.beginPath();
  ctx.moveTo(q.x1, q.y1);
  ctx.lineTo(q.x2, q.y2);
  ctx.lineTo(q.x3, q.y3);
  ctx.lineTo(q.x4, q.y4);
  ctx.closePath();
  if (q.style.fill) ctx.fill();
  if (q.style.stroke) ctx.stroke();
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
    const t = tension;
    const k = t / 6;
    const pts = pg.points;
    ctx.moveTo(pts[0]!.x, pts[0]!.y);
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
      ctx.bezierCurveTo(c1x, c1y, c2x, c2y, p2.x, p2.y);
    }
  }
  if (pg.closed) {
    ctx.closePath();
    if (pg.style.fill) ctx.fill();
  }
  if (pg.style.stroke) ctx.stroke();
}

function renderText(ctx: RenderingContext2D, t: TextShape): void {
  applyStyle(ctx, t.style);
  applyTextStyle(ctx, t.style);
  if (t.style.fill) ctx.fillText(t.text, t.x, t.y);
  if (t.style.stroke) ctx.strokeText(t.text, t.x, t.y);
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

// ─── Helpers ────────────────────────────────────────────────────────────

/**
 * Resolve the `(x, y, w, h)` quartet under a given rect/ellipse mode
 * into a uniform top-left `(x, y)` + width/height. Matches p5's mode
 * semantics:
 *   - corner: (x,y)=top-left, (w,h)=size
 *   - center: (x,y)=centre, (w,h)=size
 *   - corners: (x,y)=top-left, (w,h)=bottom-right corner
 *   - radius: (x,y)=centre, (w,h)=half-extents
 */
function resolveBox(x: number, y: number, w: number, h: number, mode: string): [number, number, number, number] {
  switch (mode) {
    case 'center': return [x - w / 2, y - h / 2, w, h];
    case 'corners': return [Math.min(x, w), Math.min(y, h), Math.abs(w - x), Math.abs(h - y)];
    case 'radius': return [x - w, y - h, w * 2, h * 2];
    case 'corner':
    default: return [x, y, w, h];
  }
}
