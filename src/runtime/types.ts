/**
 * Every wire carries a Spread<T>. Scalars are spreads of length 1.
 * This eliminates "is this a scalar or a list?" branching from every node
 * and matches vvvv gamma's mental model: everything is a 1-D list.
 */
export type Spread<T> = ReadonlyArray<T>;

/** What flows on every edge between nodes. */
export type PortValue = Spread<unknown>;

export type PortType =
  | 'number'
  | 'boolean'
  | 'string'
  | 'vec2'
  | 'color'
  | 'mat3'
  | 'layer'
  | 'style'
  /** Persistent audio handle. Wires carry an `AudioHandle` referencing
   *  a long-lived audio node (oscillator, filter, …) kept alive in the
   *  audio registry across frames. Per-frame compute updates params on
   *  the existing node; structural changes (connect/disconnect, type
   *  swap) happen at compile / first-tick. */
  | 'audio'
  /** A loaded raster image. Wires carry an `HTMLImageElement` (or
   *  null while loading). Drawn via `p.image(...)` which accepts both
   *  HTMLImageElement and p5.Image. */
  | 'image'
  /** Physics world handle — gravity, drag, time-scale, and the registry
   *  of registered global forces / colliders. One World can feed many
   *  Emitters. See `runtime/particles.ts`. */
  | 'world'
  /** Opaque force descriptor. A Force mutates a particle buffer's
   *  per-particle force accumulators in-place — no per-particle JS
   *  allocations. See `Force` type in `runtime/particles.ts`. */
  | 'force'
  /** Opaque collider descriptor — analytic shape + collision mode. */
  | 'collider'
  /** A `ParticleBuffer` handle. Wires carry a length-1 spread of one
   *  buffer (Struct-of-Arrays storage). Never an array of Particle
   *  objects — that would defeat the perf model. */
  | 'particle'
  /** Universal — accepts any port type. Used by convert/* nodes whose
   *  job is to coerce one type to another. */
  | 'any';

/**
 * A Style transforms a `ResolvedStyle` — pure function, no mutation.
 * Chains compose by piping the base through each successive style.
 * See `runtime/scene.ts` for the full type + helpers.
 */
export type { Style, ResolvedStyle, SceneNode } from './scene';
export { DEFAULT_STYLE, LINE_DEFAULT_STYLE, resolveStyle, chainStyle, colorToCss, composeFontCss } from './scene';

export interface Vec2 {
  x: number;
  y: number;
}

export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

/**
 * 3x3 column-major matrix (xx, yx, _, xy, yy, _, tx, ty, _).
 * In M2 we only use the 2D affine subset, but the full 3x3 keeps the
 * shape stable for future shader/Stride-style work.
 */
export type Mat3 = readonly [number, number, number, number, number, number, number, number, number];

/**
 * A Layer is the unit of drawing. Drawing nodes return Layers, which
 * are now plain data — `SceneNode`s describing what to draw. The
 * scene renderer walks the tree and emits Canvas2D calls; a future
 * SVG/PDF exporter walks the same tree to emit vector output.
 */
import type { SceneNode as _SceneNode } from './scene';
export type Layer = _SceneNode;

/** Reserved for future use (target surface, blend mode). Currently empty. */
export interface DrawCtx {
  readonly tag: 'root';
}

export const DEFAULT_DRAW_CTX: DrawCtx = Object.freeze({ tag: 'root' });

export interface FrameCtx {
  /** Time in seconds since the runtime started. */
  readonly t: number;
  /** Delta in seconds since the previous frame. */
  readonly dt: number;
  /** Integer frame counter. */
  readonly frameCount: number;
  /** Canvas dimensions. */
  readonly width: number;
  readonly height: number;
  /** Mouse position in canvas coordinates. */
  readonly mouseX: number;
  readonly mouseY: number;
  readonly mouseIsPressed: boolean;
  /** Last key pressed (lowercase). Empty string if none. */
  readonly key: string;
  readonly keyIsPressed: boolean;
}

/**
 * A handle to a persistent audio object living in the audio registry.
 * Wires of type 'audio' carry these. The actual Web Audio node is kept
 * alive across frames keyed by `nodeId` + a per-node `slot` (for nodes
 * that own more than one — e.g. a synth's voice + envelope).
 *
 * `null` represents "no audio yet" — a still-loading SoundFile, a node
 * that has been removed, or an audio path whose source is unset.
 */
export interface AudioHandle {
  /** Editor node id that owns this audio object. */
  readonly nodeId: string;
  /** Slot key within the node. Most nodes use 'main'. */
  readonly slot: string;
  /**
   * The underlying Web Audio node (OscillatorNode, BiquadFilterNode,
   * GainNode, etc.). Untyped here because the specific shape varies
   * across kinds; consumers narrow via `kind`.
   */
  readonly node: AudioNode;
  /**
   * What kind of audio object this is — lets a sink (AudioOut) or a
   * processor (Filter, Delay) decide what to do when an audio wire
   * lands on its input.
   */
  readonly kind: AudioNodeKind;
}

export type AudioNodeKind =
  | 'oscillator'
  | 'noise'
  | 'pulse'
  | 'gain'
  | 'filter'
  | 'delay'
  | 'reverb'
  | 'distortion'
  | 'compressor'
  | 'envelope'
  | 'mono-synth'
  | 'sound-file'
  | 'audio-in'
  | 'amplitude'
  | 'fft'
  | 'peak-detect';

export const IDENTITY_MAT3: Mat3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];

export function scalar<T>(v: T): Spread<T> {
  return [v];
}

export const EMPTY_SPREAD: Spread<never> = [];
