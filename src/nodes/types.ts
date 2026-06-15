import type React from 'react';
import type { NodeProps } from '@xyflow/react';
import type { PortType, PortValue, FrameCtx } from '../runtime/types';
import type { RawComputeFn, ScalarFn } from '../runtime/spread';

export interface PortSpec {
  /** Internal name (used for handle ids: 'in-<name>', 'out-<name>'). */
  name: string;
  /** Display label (defaults to `name` if absent). */
  label?: string;
  type: PortType;
  /**
   * Wire shape on this port. Three flavours:
   *
   *   - 'single' (default): one wire, scalar value of `type`. Compute
   *                         sees the unwrapped scalar.
   *   - 'spread':            one wire, `Spread<type>` (a 1-D list).
   *                         Compute sees the spread.
   *   - 'multi'  (INPUT only): N wires, runtime gathers each wire's
   *                         value into a single `Spread<type>` before
   *                         compute. Renders as a taller "pill" handle.
   *
   * Compatibility (enforced by `connection-rules.ts areKindsCompatible`):
   *
   *   single → single   ✓
   *   spread → spread   ✓
   *   single → spread   ✓ (auto-wraps as length-1 spread)
   *   single → multi    ✓ (one element added to the gather)
   *   spread → multi    ✓ (all elements concatenated)
   *   multi  → anything ✗ (multi is an input-side semantic only —
   *                        outputs are always single or spread)
   *   spread → single   ✗ (must cross a region boundary via a splicer)
   *
   * Runtime contract for multi inputs: see `RuntimeNode.multiInPorts`
   * in `runtime/graph.ts` — the gather semantics live there.
   */
  kind?: 'single' | 'spread' | 'multi';
  /**
   * Default SCALAR value when the input port is unconnected. The runtime
   * wraps it as needed (length-1 spread for spread-kind ports).
   */
  default?: unknown;
  /**
   * If set, the runtime reads the named param value from `paramStore`
   * each tick when this input has no edge connected (and no portValues
   * override). Lets an IO node combine an inline slider with a wired
   * input — the wire wins when connected, otherwise the slider value
   * is used live with no recompile.
   */
  param?: string;
  /**
   * Soft hint that this `number` port expects an integer — the runtime
   * floors/rounds the value internally, but the editor shows a small
   * `⌊⌋` glyph next to the pin so the user can see the coercion. Has
   * no runtime effect on its own; it's an editor-facing marker.
   */
  integer?: boolean;
  /**
   * Constant pin — sampled ONCE at compile time (or whenever the upstream
   * source / edge / portValue changes) and stashed on
   * `RuntimeNode.constants[name]`. The tick-time gather phase skips
   * constant inputs entirely, so they cost zero per frame.
   *
   * Used for "set-once" parameters that shape topology or allocate
   * memory: buffer capacity, RNG seed, integrator choice, render
   * template selection, palette LUT size. Editing the upstream value
   * triggers a recompile (the buffer reallocates, the LUT rebuilds);
   * that's the cost of the per-frame win.
   *
   * Constant inputs MUST NOT participate in cycles — the compiler
   * evaluates them in topo order at compile time, so a cycle through a
   * constant pin is rejected.
   *
   * Editor renders the pin with a small `C` glyph and a dimmer ring so
   * authors can tell at a glance which pins are set-once. See
   * `GenericNode.tsx`.
   */
  constant?: boolean;
}

/**
 * Context passed to a node's `paramReader` — lets the node read its own
 * inline editor state (e.g. a Number node's `value` field) at tick time.
 */
export interface NodeParamContext {
  nodeId: string;
  /** Reads a named parameter from the editor's node.data. */
  param<T = unknown>(key: string, fallback?: T): T;
}

export interface NodeDef {
  /** Globally unique, used as React Flow node `type` and registry key. */
  type: string;
  /** Palette grouping. */
  category: string;
  /** Display name (shown on the node header). */
  label: string;
  /** Optional one-line tooltip. */
  description?: string;
  inputs: ReadonlyArray<PortSpec>;
  outputs: ReadonlyArray<PortSpec>;

  /**
   * Optional: per-instance input override. When set, the runtime,
   * connection-rules, and the rendering component all use the result
   * of this function INSTEAD of `inputs`. Used for nodes whose input
   * shape depends on a mode param (e.g. Fill: RGB / HSL / hex / Color).
   *
   * The function MUST return ports in a deterministic order — compile
   * uses array index for runtime port slots.
   */
  dynamicInputs?: (params: Record<string, unknown>) => ReadonlyArray<PortSpec>;

  /**
   * Optional: per-instance output override that can react to what's
   * currently wired into the node's inputs. Mirrors `dynamicInputs`, but
   * receives an extra `upstreamTypes` map (input port name → source
   * port type) so the node can declare polymorphic outputs.
   *
   * Used by Repeat / Sort / Reverse / Slice — anything whose output
   * type SHOULD echo its input rather than read `any`. When no upstream
   * is connected the lookup yields undefined and the implementation
   * should fall back to the static type.
   */
  dynamicOutputs?: (
    params: Record<string, unknown>,
    upstreamTypes: Record<string, PortType | undefined>,
  ) => ReadonlyArray<PortSpec>;

  /**
   * The common path: pure scalar function. Auto-lifted to spreads by the
   * runtime. Use this unless the node needs spread-aware logic.
   */
  compute?: ScalarFn;

  /**
   * Escape hatch: receive whole spreads, return whole spreads.
   * Used by Renderer (passthrough), LinearSpread (generates a spread),
   * spread ops (Resample, Count). Bypasses liftCyclic.
   *
   * If `paramReader` is also set, computeRaw is called with the params
   * applied as a final argument — see compile.ts.
   */
  computeRaw?: (inputs: PortValue[], params: NodeParamContext) => PortValue[];

  /**
   * For source nodes (Time, Mouse, Slider): read from frame/editor state
   * instead of upstream inputs. Bypasses input gathering.
   */
  readFromContext?: (frame: FrameCtx, params: NodeParamContext) => PortValue[];

  /**
   * Optional custom React Flow node component. Default: GenericNode.
   * Receives React Flow's full NodeProps — cast `data` to `FloretNodeData`
   * inside the component.
   */
  component?: React.ComponentType<NodeProps>;

  /**
   * Default editor `data.params` payload when a node of this type is created.
   * IO boxes and Number nodes set this; pure compute nodes (Add, Sin) leave it empty.
   */
  defaultParams?: Record<string, unknown>;

  /**
   * Optional fixed option list per param key. When set, GenericNode
   * renders a `<select>` instead of a text input for that param,
   * limited to the supplied options. Used for enum-style mode
   * switching (e.g. Fill mode: rgb / hsl / hex / color).
   */
  paramOptions?: Record<string, ReadonlyArray<string>>;

  /**
   * Live-resolved option list per param key. Use when the choices
   * change at runtime (connected MIDI devices, available audio
   * outputs, …) — GenericNode re-renders the dropdown whenever
   * `subscribe` fires.
   *
   *   getOptions(): the current list as `{value, label}` pairs.
   *                 `value` is what's stored in the param; `label` is
   *                 what the user sees. For most cases value === label.
   *   subscribe(fn): register a listener; return an unsubscribe fn.
   *                  GenericNode wires this into useSyncExternalStore
   *                  so the dropdown re-renders on every change.
   *
   * `dynamicOptions` takes precedence over `paramOptions` for the
   * same key — they're not meant to coexist on one param.
   */
  dynamicOptions?: Record<
    string,
    {
      getOptions: () => ReadonlyArray<{ value: string; label: string }>;
      subscribe: (fn: () => void) => () => void;
    }
  >;

  /**
   * Hide this NodeDef from the palette. The runtime still knows
   * about it — used for internal NodeDefs (splicer-in / splicer-out)
   * that compile synthesises from a region's frame ports.
   */
  hidden?: boolean;

  /**
   * Adaptive (polymorphic) arithmetic node — its `valueType` param
   * ('any' | 'number' | 'vec2' | 'color') latches to the type of the first
   * wire attached (operand OR result) and resets to 'any' when the last wire
   * is removed. The editor maintains the latch (see editor-state.ts); pins
   * and compute follow `valueType`. See nodes/math/_adaptive.ts.
   */
  adaptiveType?: boolean;

  /**
   * Region marker. Present on `region/foreach` (and later If/Cache).
   * Compile treats these as orchestrator nodes — they have dynamic
   * in/out arity (one port per splicer child) and `tick` dispatches into
   * `runRegion` rather than calling `compute`.
   */
  regionKind?: 'foreach' | 'if' | 'cache' | 'subpatch';

  /**
   * Role inside an enclosing region. Splicer / accumulator nodes use this.
   * Compile sets `RuntimeNode.splicerRole` / `accumulatorRole` from this;
   * `tick` special-cases their eval (no `compute` invocation).
   *
   * `loop-var` is a synthetic source — the ForEach/Cache built-in
   * `index` / `count` / `fraction` pins. Like a splicer-in's inner pin it
   * reads from the iteration context (no outer spread feeds it), but it is
   * NOT counted among the region's splicer-ins, so it never affects input
   * arity or the iteration count.
   */
  regionRole?: 'splicer-in' | 'splicer-out' | 'accumulator-read' | 'accumulator-write' | 'loop-var';

  /**
   * Cross-frame state node. Inputs are `init` (port 0, used on the
   * first tick) and `next` (port 1, captured at the end of every tick
   * and exposed as `current` on the NEXT tick). The output is the
   * value from the previous tick — or `init` on tick zero.
   *
   * The edge feeding `next` is excluded from topo-sort so the
   * feedback loop doesn't deadlock. The runtime gathers it in a
   * post-eval pass after the rest of the graph has computed for the
   * frame, then stashes the value on `RuntimeNode.runtimeState.value`.
   */
  stateful?: boolean;
  /**
   * Marks a node whose `compute` / `computeRaw` mutates internal state
   * that persists across ticks (e.g. a ring-buffer accumulator like
   * `particles/trail`). The runtime's per-tick memo would otherwise
   * cache the first output and return it unchanged when the input set
   * (often a single stable buffer reference + constants) doesn't shift.
   *
   * Setting this to `true` opts out of memoisation so the compute runs
   * every tick. Use sparingly — most nodes don't need it. Use it when
   * the same `compute(inputs)` produces a DIFFERENT output on the
   * second call because the node has internal hidden state.
   */
  effectful?: boolean;

  /**
   * For stateful nodes only: index of the input port that carries the
   * cross-frame `next` value. The runtime:
   *   1. Excludes edges into this port from topo (so the feedback loop
   *      doesn't deadlock).
   *   2. Gathers it in a post-eval pass and stashes the value on
   *      `runtimeState.value` for next tick.
   *
   * Default 1, matching the conventional `[init, next]` shape on
   * `state/value` and `state/spread`. `state/write` declares its own
   * shape `[next]` and sets `statefulNextPort: 0`.
   */
  statefulNextPort?: number;
}

/**
 * Resolve a node's active input ports — uses `dynamicInputs(params)`
 * when present, falling back to the static `inputs` array. Called from
 * compile.ts, connection-rules.ts, and node components so they all see
 * the same shape.
 */
export function getResolvedInputs(
  def: NodeDef,
  params: Record<string, unknown>,
): ReadonlyArray<PortSpec> {
  return def.dynamicInputs ? def.dynamicInputs(params) : def.inputs;
}

/**
 * Resolve a node's active output ports — uses `dynamicOutputs(params,
 * upstreamTypes)` when present so output types can echo whatever's wired
 * into the inputs. Falls back to the static `outputs` array.
 *
 * `upstreamTypes` maps input-port-name → the source port's PortType (or
 * undefined for unconnected inputs). The editor builds this from the
 * current edge set just before calling here.
 */
export function getResolvedOutputs(
  def: NodeDef,
  params: Record<string, unknown>,
  upstreamTypes: Record<string, PortType | undefined>,
): ReadonlyArray<PortSpec> {
  return def.dynamicOutputs
    ? def.dynamicOutputs(params, upstreamTypes)
    : def.outputs;
}

/** Re-export common types so node files only need to import from './types'. */
export type { PortType, PortValue, RawComputeFn, ScalarFn };
