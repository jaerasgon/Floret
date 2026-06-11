import type { Color, Vec2 } from './types';
import type { ParticleBuffer, ParticleTemplate } from './particles';

/**
 * Retained-mode scene description.
 *
 * Drawing nodes no longer return a `(p: p5) => void` thunk that mutates
 * the canvas. They return a `SceneNode` — a plain data description of
 * what to draw. The scene renderer (`scene-renderer.ts`) walks the tree
 * and emits Canvas2D calls; a future SVG/PDF exporter walks the same
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
  /** Font size in pixels. Composed into the CSS `font` shorthand at render
   *  time by `composeFontCss(style)`. */
  fontSize: number;
  /** CSS font-family stack (e.g. "sans-serif", "Inter, sans-serif"). */
  fontFamily: string;
  /** CSS font-weight (e.g. "normal", "bold", "600"). */
  fontWeight: string;
  /** CSS font-style ("normal" | "italic" | "oblique"). */
  fontStyle: string;
  /** Letter-spacing in pixels (Canvas2D `letterSpacing`, SVG `letter-spacing`). */
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
 * stroke matches p5's typical "give me this colour" expectation.
 * Lines override `fill: null` because lines are stroke-driven; see
 * `LINE_DEFAULT_STYLE` below.
 */
export const DEFAULT_STYLE: ResolvedStyle = Object.freeze({
  fill: '#ffffff',
  stroke: null,
  strokeWeight: 1,
  strokeCap: 'butt',
  strokeJoin: 'miter',
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
 * A 2D affine transform `[a, b, c, d, e, f]` matching the Canvas2D
 * `setTransform` convention:
 *   x' = a·x + c·y + e
 *   y' = b·x + d·y + f
 * Identity = `[1, 0, 0, 1, 0, 0]`.
 */
export type Affine = readonly [number, number, number, number, number, number];

export const IDENTITY: Affine = Object.freeze([1, 0, 0, 1, 0, 0]) as Affine;

/**
 * Drawing context — accepts either main-thread Canvas2D or the
 * OffscreenCanvas variant (worker + derived-image-registry both use the
 * latter). The two have an almost identical surface; the scene renderer
 * uses only methods present on both.
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
 * Resolve a Style (possibly null/undefined) against a base default.
 * Used by every draw node when materialising the shape.
 */
export function resolveStyle(
  style: Style | null | undefined,
  base: ResolvedStyle = DEFAULT_STYLE,
): ResolvedStyle {
  if (!style) return base;
  return style(base);
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
 * ResolvedStyle. Used by the Canvas2D renderer (assigns to `ctx.font`)
 * and the SVG emitter (writes it into `style="font:..."`). Order matters
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
