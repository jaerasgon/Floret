import type { Color, Vec2 } from './types';
import type { ParticleBuffer, ParticleTemplate } from './particles';

/**
 * Retained-mode scene description.
 *
 * Drawing nodes don't mutate a canvas directly. They return a `SceneNode`
 * — a plain data description of what to draw. The GPU renderer
 * (`gl/scene-to-batches.ts` → `gl/gl-vector-renderer.ts`) walks the tree
 * into WebGL2 batches; the SVG exporter (`scene-svg.ts`) walks the same
 * tree to emit vector output; the editor can introspect the tree for
 * hit-testing, bounds, selection, and "click a shape to edit" UX.
 *
 * Three reasons we picked retained-mode over immediate-mode functions:
 *
 *   1. **Vector export.** Once we have a tree of shapes, SVG/PDF
 *      output is a separate visitor — no per-node coupling, no
 *      double-implementation in every draw node.
 *   2. **Hit-testing.** "Which shape did the user click?" becomes a
 *      tree walk, not a hack.
 *   3. **Smart rendering.** Future GPU batching, worker offload,
 *      occlusion culling — all easier when the renderer sees the whole
 *      scene declaratively rather than as opaque mutator callbacks.
 *
 * Each `SceneNode` carries its OWN style (immutable). No implicit
 * `p.fill()` state-machine. Composition (transforms, groups) is via
 * `{ kind: 'group', children, transform }` rather than `p.push() /
 * p.translate() / ... / p.pop()`.
 */

export type RectMode = 'corner' | 'center' | 'corners' | 'radius';
export type EllipseMode = 'corner' | 'center' | 'corners' | 'radius';
export type StrokeCap = 'butt' | 'round' | 'square';
export type StrokeJoin = 'miter' | 'bevel' | 'round';
/** Where a stroke sits relative to the shape's outline. CSS/Canvas paint
 *  centred strokes only; this renderer also supports `inside` / `outside`
 *  (as SVG does) via clipping (closed shapes only — open paths have no
 *  interior). */
export type StrokeAlign = 'center' | 'inside' | 'outside';
export type TextAlign = 'left' | 'center' | 'right';
export type TextBaseline = 'alphabetic' | 'top' | 'middle' | 'bottom';
export type BlendMode = GlobalCompositeOperation;

/**
 * Fully-resolved drawing style. `fill: null` means "don't fill";
 * `stroke: null` means "don't stroke" — distinct from a transparent
 * colour so the renderer can short-circuit. Built by style nodes via
 * `chainStyle()` (see below) and attached to every shape SceneNode.
 */
export interface ResolvedStyle {
  fill: string | null;
  stroke: string | null;
  strokeWeight: number;
  strokeCap: StrokeCap;
  strokeJoin: StrokeJoin;
  /** Where the stroke sits relative to the outline (closed shapes only). */
  strokeAlign: StrokeAlign;
  /** Gaussian blur radius in pixels (0 = sharp). Applied via the canvas
   *  filter to the whole shape — fill + stroke + text alike. */
  blur: number;
  /** Font size in pixels. Composed into the CSS `font` shorthand at render
   *  time by `composeFontCss(style)`. */
  fontSize: number;
  /** CSS font-family stack (e.g. "sans-serif", "Inter, sans-serif"). */
  fontFamily: string;
  /** CSS font-weight (e.g. "normal", "bold", "600"). */
  fontWeight: string;
  /** CSS font-style ("normal" | "italic" | "oblique"). */
  fontStyle: string;
  /** Letter-spacing in pixels (CSS `letter-spacing` / SVG `letter-spacing`). */
  letterSpacing: number;
  /** Multiplier on fontSize for line spacing in multi-line text. */
  lineHeight: number;
  textAlign: TextAlign;
  textBaseline: TextBaseline;
  /** How (x,y,w,h) is interpreted for rects (and similar). The Rect
   *  node reads this off the resolved style so a chained `RectMode`
   *  style node can override the shape's default. */
  rectMode: RectMode;
  ellipseMode: EllipseMode;
}

/**
 * Sensible defaults — what an unstyled shape uses. White fill, no
 * stroke matches the common "give me this colour" expectation.
 * Lines override `fill: null` because lines are stroke-driven; see
 * `LINE_DEFAULT_STYLE` below.
 */
export const DEFAULT_STYLE: ResolvedStyle = Object.freeze({
  fill: '#ffffff',
  stroke: null,
  strokeWeight: 1,
  strokeCap: 'butt',
  strokeJoin: 'miter',
  strokeAlign: 'center',
  blur: 0,
  fontSize: 14,
  fontFamily: 'sans-serif',
  fontWeight: 'normal',
  fontStyle: 'normal',
  letterSpacing: 0,
  lineHeight: 1.2,
  textAlign: 'left',
  textBaseline: 'alphabetic',
  rectMode: 'corner',
  ellipseMode: 'center',
});

/** Lines / open polygons default to a thin black stroke, no fill. */
export const LINE_DEFAULT_STYLE: ResolvedStyle = Object.freeze({
  ...DEFAULT_STYLE,
  fill: null,
  stroke: '#000000',
});

/**
 * Neutral base a WIRED style chain resolves against — no paint at all, but
 * keeps the structural defaults (stroke weight, caps, font, modes). A wired
 * style fully defines the look: only what the chain sets gets painted, so a
 * lone Stroke yields an outline with no fill, and a lone Fill yields a fill
 * with no stroke. The shape's own default (white fill, line stroke, …) only
 * applies when NOTHING is wired. See `resolveStyle`.
 */
export const NEUTRAL_STYLE: ResolvedStyle = Object.freeze({
  ...DEFAULT_STYLE,
  fill: null,
  stroke: null,
});

/**
 * A 2D affine transform `[a, b, c, d, e, f]` following the CSS/SVG
 * `matrix(a,b,c,d,e,f)` convention:
 *   x' = a·x + c·y + e
 *   y' = b·x + d·y + f
 * Identity = `[1, 0, 0, 1, 0, 0]`.
 */
export type Affine = readonly [number, number, number, number, number, number];

export const IDENTITY: Affine = Object.freeze([1, 0, 0, 1, 0, 0]) as Affine;

/**
 * Drawing context — accepts either main-thread Canvas2D or the
 * OffscreenCanvas variant (worker + derived-image-registry both use the
 * latter). The two have an almost identical surface; callers use only
 * methods present on both.
 */
export type RenderingContext2D =
  | CanvasRenderingContext2D
  | OffscreenCanvasRenderingContext2D;

/** ─── Shape variants ─────────────────────────────────────────────── */

export interface RectShape {
  kind: 'rect';
  x: number;
  y: number;
  w: number;
  h: number;
  mode: RectMode;
  style: ResolvedStyle;
}

export interface EllipseShape {
  kind: 'ellipse';
  x: number;
  y: number;
  w: number;
  h: number;
  mode: EllipseMode;
  style: ResolvedStyle;
}

export interface LineShape {
  kind: 'line';
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  style: ResolvedStyle;
}

export interface ArcShape {
  kind: 'arc';
  x: number;
  y: number;
  w: number;
  h: number;
  start: number;
  stop: number;
  mode: EllipseMode;
  style: ResolvedStyle;
}

export interface TriangleShape {
  kind: 'triangle';
  x1: number; y1: number;
  x2: number; y2: number;
  x3: number; y3: number;
  style: ResolvedStyle;
}

export interface QuadShape {
  kind: 'quad';
  x1: number; y1: number;
  x2: number; y2: number;
  x3: number; y3: number;
  x4: number; y4: number;
  style: ResolvedStyle;
}

export interface PointShape {
  kind: 'point';
  x: number;
  y: number;
  style: ResolvedStyle;
}

export interface PolygonShape {
  kind: 'polygon';
  points: ReadonlyArray<Vec2>;
  closed: boolean;
  style: ResolvedStyle;
  /**
   * Optional curve smoothness, 0..1.
   *   0 (or omitted) → straight `lineTo` between points (classic polyline).
   *   >0             → Catmull–Rom spline converted to cubic Béziers, so
   *                    the curve passes through every input point with
   *                    progressively rounder corners as tension rises.
   * The Path draw node exposes this; existing polygon callers (triangles,
   * etc.) leave it undefined and get the original straight-line render.
   */
  tension?: number;
}

export interface TextShape {
  kind: 'text';
  text: string;
  x: number;
  y: number;
  style: ResolvedStyle;
}

/**
 * Particle batch — the CPU "instancing" SceneNode. Each frame the
 * renderer walks one of these with a single tight loop over the typed
 * arrays inside `buffer`, drawing `count` particles using `template`.
 *
 * The buffer is shared by reference with the upstream Emitter — there
 * is NO copy per frame. `count` is snapshotted at the end of the
 * Emitter's tick so the renderer reads a stable value even if the
 * Emitter is later advanced again before the draw call lands.
 *
 * See `runtime/particles.ts` for the buffer layout and lifecycle.
 */
export interface ParticleBatch {
  kind: 'particle-batch';
  buffer: ParticleBuffer;
  count: number;
  template: ParticleTemplate;
  style: ResolvedStyle;
  /** When true, use per-particle colour from the buffer; otherwise the
   *  resolved style's fill is used uniformly across all particles. */
  useBufferColor: boolean;
}

/**
 * Per-particle trail batch. `history` is a ring buffer of past
 * positions: `history[(slot * segments + seg) * 2 + 0..1]` is the
 * (x, y) of segment `seg` for particle slot `slot`. `headIdx` is the
 * latest write position; segments walk backwards from there.
 */
export interface ParticleTrailBatch {
  kind: 'particle-trail-batch';
  buffer: ParticleBuffer;
  history: Float32Array;
  segments: number;
  /** Tail-most write index; segments walk backwards modulo `segments`. */
  headIdx: number;
  style: ResolvedStyle;
  /** When true, segment opacity decays linearly from head → tail. */
  fadeOut: boolean;
  /** When true, use the particle's current colour from the buffer as
   *  the stroke colour for its trail (so a fading particle takes its
   *  trail with it). When false, use `style.stroke`. */
  useBufferColor: boolean;
  /** When true, segment line width tapers linearly from
   *  `style.strokeWeight` at the head down to
   *  `style.strokeWeight × tailWidth` at the tail. Combines cleanly
   *  with `fadeOut` — opacity AND width can both decay. */
  tapered: boolean;
  /** Tail width as a fraction of head width (0..1). 0.1 leaves a thin
   *  whisker; 0 vanishes the tail entirely. Only used when `tapered`. */
  tailWidth: number;
}

export interface ImageShape {
  kind: 'image';
  source: CanvasImageSource;
  /** Optional source-rect — `image/region` uses this; null = draw whole image. */
  src: { x: number; y: number; w: number; h: number } | null;
  /** Destination rect on the canvas. */
  dst: { x: number; y: number; w: number; h: number };
  /** Multiplicative tint colour (CSS string) or null for untinted. */
  tint: string | null;
}

/** A uniform value resolved from the graph and pushed to the shader. */
export type ShaderUniformValue =
  | { t: 'number'; v: number }
  | { t: 'vec2'; v: [number, number] }
  | { t: 'color'; v: [number, number, number, number] };

/** Which offscreen buffer a pass writes to. 'image' is the final, composited pass. */
export type ShaderBufferId = 'A' | 'B' | 'C' | 'D' | 'image';
/** What an iChannel samples: the wired input layer, or another buffer's output. */
export type ShaderChannel = { kind: 'input' } | { kind: 'buffer'; buf: ShaderBufferId } | null;
/** One fragment-shader pass: its source + how its four iChannels are routed. */
export interface ShaderPass {
  buf: ShaderBufferId;
  source: string;
  channels: readonly [ShaderChannel, ShaderChannel, ShaderChannel, ShaderChannel];
}

/**
 * A GPU fragment-shader layer (the `gpu/shader` node). Rendered by the GL
 * renderer, NOT the tick: it runs every frame with time + feedback. `passes`
 * run in order (present buffers A..D, then the Image pass); each writes its
 * buffer and samples its routed iChannels (the wired `input` layer, sibling
 * buffers' outputs, or its own previous frame via ping-pong). `nodeId` keys the
 * renderer's persistent per-buffer feedback FBOs. Single-pass shaders are just
 * one Image pass.
 */
export interface ShaderShape {
  kind: 'shader';
  nodeId: string;
  passes: ReadonlyArray<ShaderPass>;
  /** Detected user uniforms (name + glsl type) so the renderer knows what to set. */
  uniformDecls: ReadonlyArray<{ name: string; glslType: string }>;
  uniforms: Record<string, ShaderUniformValue>;
  /** Primary input subtree → the 'input' channel (rasterised to a texture). */
  input: SceneNode | null;
}

/**
 * A Group composites N children with an optional transform and blend
 * mode. The transform multiplies into the current matrix; the blend
 * mode is set (and restored) around the children draws. Group is also
 * how `transform/translate`, `transform/rotate`, `transform/scale`,
 * `transform/blend` express themselves — each wraps its inner layer in
 * a Group with the appropriate transform / blendMode set.
 */
export interface GroupNode {
  kind: 'group';
  children: ReadonlyArray<SceneNode>;
  transform?: Affine;
  blendMode?: BlendMode;
  /** Clip subsequent children to this rect (in local coords). For
   *  region cells, walker-cluster cells, anything that needs hard edges. */
  clip?: { x: number; y: number; w: number; h: number };
}

export type ShapeNode =
  | RectShape
  | EllipseShape
  | LineShape
  | ArcShape
  | TriangleShape
  | QuadShape
  | PointShape
  | PolygonShape
  | TextShape
  | ImageShape
  | ShaderShape
  | ParticleBatch
  | ParticleTrailBatch;

export type SceneNode = ShapeNode | GroupNode;

/**
 * A Style is a pure function: given a base ResolvedStyle, return a new
 * one with whatever overrides this style applies. Chains compose by
 * piping the base through each successive style.
 *
 *   FillRed → StrokeBlue → StrokeWeight(2) → shape
 *   = shape.style = StrokeWeight(2)(StrokeBlue(FillRed(DEFAULT_STYLE)))
 *
 * No mutation, no leakage between sibling shapes — every shape gets
 * its own resolved style snapshot.
 */
export type Style = (base: ResolvedStyle) => ResolvedStyle;

export const IDENTITY_STYLE: Style = (s) => s;

/**
 * Resolve a Style for a shape. A WIRED style replaces the shape's default
 * rather than layering on top of it: the chain resolves against
 * `NEUTRAL_STYLE` (no paint), so it fully defines what gets drawn — a lone
 * Stroke is outline-only, a lone Fill is fill-only. The `fallback` (the
 * shape's own default, e.g. white fill / line stroke) is used ONLY when no
 * style is wired.
 */
export function resolveStyle(
  style: Style | null | undefined,
  fallback: ResolvedStyle = DEFAULT_STYLE,
): ResolvedStyle {
  if (!style) return fallback;
  return style(NEUTRAL_STYLE);
}

/** Compose two styles: applying `outer` after `inner`. */
export function chainStyle(inner: Style | null | undefined, outer: Style): Style {
  if (!inner) return outer;
  return (base) => outer(inner(base));
}

/**
 * Helper: build a CSS colour string from a Color (0..1 channels) with
 * fast paths for opaque colours. The renderer assigns this string to
 * `ctx.fillStyle` / `ctx.strokeStyle` directly — no per-frame parsing.
 */
export function colorToCss(c: Color): string {
  const r = Math.round(clamp01(c.r) * 255);
  const g = Math.round(clamp01(c.g) * 255);
  const b = Math.round(clamp01(c.b) * 255);
  const a = clamp01(c.a);
  return a >= 1
    ? `rgb(${r},${g},${b})`
    : `rgba(${r},${g},${b},${a.toFixed(3)})`;
}

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

/** Affine builders — used by transform nodes to construct Group.transform. */
export function translateMat(tx: number, ty: number): Affine {
  return [1, 0, 0, 1, tx, ty];
}
export function scaleMat(sx: number, sy: number): Affine {
  return [sx, 0, 0, sy, 0, 0];
}
export function rotateMat(angleRad: number): Affine {
  const c = Math.cos(angleRad);
  const s = Math.sin(angleRad);
  return [c, s, -s, c, 0, 0];
}
export function shearXMat(angleRad: number): Affine {
  return [1, 0, Math.tan(angleRad), 1, 0, 0];
}
export function shearYMat(angleRad: number): Affine {
  return [1, Math.tan(angleRad), 0, 1, 0, 0];
}
export function affine(a: number, b: number, c: number, d: number, e: number, f: number): Affine {
  return [a, b, c, d, e, f];
}

/**
 * Flatten a spread of SceneNodes into a single Group — useful for
 * Layer-spread outputs from ForEach regions. Empty arrays become an
 * empty group (no-op at render time).
 */
export function groupOf(children: ReadonlyArray<SceneNode>): GroupNode {
  return { kind: 'group', children };
}

/**
 * Build a CSS `font` shorthand from the structured fields on a
 * ResolvedStyle. Used by the batch builder for text measurement (assigns
 * to `ctx.font`) and the SVG emitter (writes it into `style="font:..."`).
 * Order matters
 * — CSS expects `style weight size family`.
 */
export function composeFontCss(s: ResolvedStyle): string {
  const parts: string[] = [];
  if (s.fontStyle && s.fontStyle !== 'normal') parts.push(s.fontStyle);
  if (s.fontWeight && s.fontWeight !== 'normal') parts.push(s.fontWeight);
  parts.push(`${s.fontSize}px`);
  parts.push(s.fontFamily || 'sans-serif');
  return parts.join(' ');
}

/**
 * Shared draw geometry — used by BOTH the GL batch builder and the SVG
 * emitter so raster and vector output frame identically. Keeping these
 * here (rather than duplicated per backend) means the mode math and the
 * spline control points can't silently drift apart.
 */

/**
 * Resolve a mode-aware (x, y, w, h) into a uniform top-left + w/h.
 *   - corner:  (x,y)=top-left, (w,h)=size
 *   - center:  (x,y)=centre,   (w,h)=size
 *   - corners: (x,y) and (w,h) are two opposite corners
 *   - radius:  (x,y)=centre,   (w,h)=half-extents
 */
export function resolveBox(
  x: number, y: number, w: number, h: number, mode: string,
): [number, number, number, number] {
  switch (mode) {
    case 'center':  return [x - w / 2, y - h / 2, w, h];
    case 'corners': return [Math.min(x, w), Math.min(y, h), Math.abs(w - x), Math.abs(h - y)];
    case 'radius':  return [x - w, y - h, w * 2, h * 2];
    case 'corner':
    default:        return [x, y, w, h];
  }
}

export interface BezierSegment {
  c1x: number; c1y: number;   // first control point
  c2x: number; c2y: number;   // second control point
  ex: number; ey: number;     // segment end point
}

/**
 * Convert a point list into uniform Catmull–Rom cubic-Bézier segments.
 * Endpoints mirror their neighbour so the curve still passes through the
 * first/last point. `tension` 1 reproduces the canonical uniform
 * Catmull–Rom (a τ=0.5 Cardinal spline); the /6 normalises it.
 *
 * Returns one segment per (closed ? n : n-1) span — the caller emits the
 * leading moveTo to `points[0]` itself, then feeds each segment to
 * `bezierCurveTo` (canvas) or a path `C` command (SVG).
 */
export function catmullRomBeziers(
  points: ReadonlyArray<{ x: number; y: number }>,
  closed: boolean,
  tension: number,
): BezierSegment[] {
  const n = points.length;
  const k = tension / 6;
  const segs: BezierSegment[] = [];
  const last = closed ? n : n - 1;
  for (let i = 0; i < last; i++) {
    const p0 = points[closed ? (i - 1 + n) % n : Math.max(0, i - 1)]!;
    const p1 = points[i]!;
    const p2 = points[closed ? (i + 1) % n : i + 1]!;
    const p3 = points[closed ? (i + 2) % n : Math.min(n - 1, i + 2)]!;
    segs.push({
      c1x: p1.x + (p2.x - p0.x) * k,
      c1y: p1.y + (p2.y - p0.y) * k,
      c2x: p2.x - (p3.x - p1.x) * k,
      c2y: p2.y - (p3.y - p1.y) * k,
      ex: p2.x,
      ey: p2.y,
    });
  }
  return segs;
}
