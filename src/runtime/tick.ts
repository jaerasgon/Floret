import type { FrameCtx, Layer, PortValue } from './types';
import type { NodeId, RuntimeGraph, RuntimeNode, RuntimeScope } from './graph';
import { paramStore } from './param-store';
import { loopVarSplicerId } from './compile';
import { frameCtxStore } from './frame-ctx';

/**
 * Per-frame graph evaluation. Walks the root scope's topo order; when a
 * region node is reached, `runRegion` descends into its child scope and
 * runs it N times (per the iteration count derived from the region's
 * splicer-in inputs). Splicer-in / splicer-out nodes are special-cased
 * in `evalNode` — they don't have a `compute` of their own; their
 * outputs are populated from the iteration context (splicer-in) or
 * passed through unchanged (splicer-out).
 */
export interface TickResult {
  /** Outputs of root-scope nodes only (region inner outputs are scoped per-iteration). */
  outputs: Map<NodeId, PortValue[]>;
  /** Layers reaching the FIRST renderer node — back-compat shorthand
   *  for `layersByRenderer.get(rendererId)`. Single-renderer drivers
   *  read from here; multi-renderer ones use the map below. */
  layers: ReadonlyArray<Layer>;
  /** Per-renderer layer spread, keyed by renderer NodeId. Multiple
   *  Preview panels can each look up their own renderer here. */
  layersByRenderer: Map<NodeId, ReadonlyArray<Layer>>;
  /**
   * Per-region statistics from this tick — iteration count, per-splicer-in
   * lengths, which input limited iteration (if any). Used by the editor
   * to render the region's "× N · capped by [port]" badge.
   */
  regionStats: Map<NodeId, RegionTickStats>;
}

export interface RegionTickStats {
  /** Number of inner-scope iterations that actually ran this tick. */
  iterations: number;
  /** Length of each splicer-in's outer spread, in splicer-in order. */
  inputLengths: ReadonlyArray<number>;
  /**
   * Index (within `inputLengths`) of the splicer-in that limited iteration.
   * -1 when there are no splicer-ins, when all inputs share the same length,
   * or when the region isn't of a kind that iterates (if / cache).
   */
  bottleneckIdx: number;
}

/** Iteration context — bindings active while evaluating a single iteration of a region's inner scope. */
interface IterCtx {
  /** Map of `splicer-in id → the current iteration's slot value (already wrapped as length-1 PortValue)`. */
  splicerInValues: Map<NodeId, PortValue>;
  /** Map of `accumulator id → the accumulator's current value (this iteration's read)`. */
  accumulatorValues: Map<NodeId, unknown>;
  /** Zero-based iteration index — used to key per-iteration compute memo
   *  on each pure RuntimeNode's `runtimeState.memo`. Root scope uses -1. */
  iterIdx: number;
  /** Total iteration count N for this region run. Exposed via the
   *  built-in `count` loop variable; also handy for completeness. */
  count: number;
}

export function tick(graph: RuntimeGraph, frame: FrameCtx): TickResult {
  // Publish this frame to the ambient store so effectful nodes (motion/*)
  // can read dt/t without an input wire — computeRaw doesn't receive `frame`.
  frameCtxStore.update(frame);
  const outputs = new Map<NodeId, PortValue[]>();
  const regionStats = new Map<NodeId, RegionTickStats>();
  // Refresh the slot → state/write lookup so state/read can resolve its
  // paired writer this tick. Live param edits take effect immediately.
  graph.slotWriters = buildSlotWriters(graph);
  const rootScope = graph.scopes.get(null);
  if (rootScope) {
    evalScope(graph, rootScope, frame, outputs, null, regionStats);
    // Post-eval pass: capture each stateful node's `next` (port 1)
    // input and stash it on runtimeState.value. On the next tick,
    // evalNode reads it back and emits it as `current` (port 0).
    captureStatefulNext(graph, rootScope, outputs);
  }

  const layersByRenderer = new Map<NodeId, ReadonlyArray<Layer>>();
  for (const id of graph.rendererIds) {
    const out = outputs.get(id);
    if (out && out[0]) {
      layersByRenderer.set(id, out[0] as ReadonlyArray<Layer>);
    } else {
      layersByRenderer.set(id, []);
    }
  }
  const layers = graph.rendererId
    ? (layersByRenderer.get(graph.rendererId) ?? [])
    : [];
  return { outputs, layers, layersByRenderer, regionStats };
}

function captureStatefulNext(
  graph: RuntimeGraph,
  rootScope: RuntimeScope,
  outputs: Map<NodeId, PortValue[]>,
): void {
  for (const id of rootScope.order) {
    const node = graph.nodes.get(id)!;
    if (!node.stateful || !node.runtimeState) continue;
    const nextPort = node.statefulNextPort ?? 1;
    // Only capture if the `next` port actually has an upstream wire —
    // otherwise we'd overwrite the persisted value with the default
    // (empty / null) every frame.
    const hasNextWire = graph.edgesByTarget.get(id)?.some((e) => e.toPort === nextPort);
    if (!hasNextWire) continue;
    const inputs = gatherInputs(graph, node, outputs);
    const nextVal = inputs[nextPort];
    if (nextVal !== undefined) {
      node.runtimeState.value = nextVal;
    }
  }
}

/**
 * Build the slot → state/write node map for the current tick. Cheap;
 * rebuilt each tick so the user can edit slot params without recompiling.
 *
 * Two writer types share this map: `state/write` (scalar) and
 * `state/write-spread`. The key is `${writeType}:${slot}` so scalar
 * Read at slot 1 pairs only with scalar Write at slot 1, never with a
 * Write* at slot 1 — same slot number, separate namespaces.
 */
function buildSlotWriters(graph: RuntimeGraph): Map<string, RuntimeNode> {
  const writers = new Map<string, RuntimeNode>();
  for (const node of graph.nodes.values()) {
    if (node.type === 'state/write' || node.type === 'state/write-spread') {
      const slot = paramStore.getKey(node.id, 'slot');
      if (slot !== undefined) writers.set(`${node.type}:${slot}`, node);
    }
  }
  return writers;
}

/**
 * Evaluate a single scope. `iterCtx` is set when evaluating a region's
 * inner scope mid-iteration; null/undefined for the root scope and
 * between iterations.
 */
function evalScope(
  graph: RuntimeGraph,
  scope: RuntimeScope,
  frame: FrameCtx,
  outputs: Map<NodeId, PortValue[]>,
  iterCtx: IterCtx | null,
  regionStats: Map<NodeId, RegionTickStats>,
): void {
  for (const id of scope.order) {
    const node = graph.nodes.get(id)!;
    evalNode(graph, node, frame, outputs, iterCtx, regionStats);
  }
}

/**
 * Per-node error throttle. A node whose `compute` throws would do so on
 * every one of the ~60 frames per second; without throttling the console
 * floods and DevTools chokes. We warn once per (node, message) pair and
 * stay quiet until the error clears or changes. The map is module-level
 * and never explicitly cleared — node ids are stable and bounded by patch
 * size, and a recompile produces a fresh graph whose still-broken nodes
 * simply re-key on their first throw. Keeping a node out of the map after
 * recovery lets it warn again if it breaks a second time.
 */
const lastComputeError = new Map<NodeId, string>();

/**
 * Run a node's `compute`, but never let a thrown exception halt the
 * frame. A single bad node (divide-by-zero, out-of-range index, a typo
 * in a future scripting node) used to abort the whole tick — every
 * downstream node went un-evaluated and the canvas froze with no signal.
 *
 * On throw we emit the same empty-spread fallback the no-compute path
 * uses (one empty spread per output port), warn at most once per distinct
 * error, and return. The caller must NOT memoise this result, so the node
 * retries next frame and recovers on its own once the cause is fixed.
 */
function safeCompute(node: RuntimeNode, inputs: PortValue[]): PortValue[] {
  try {
    const out = node.compute!(inputs);
    // Recovered: allow a future failure to warn again.
    if (lastComputeError.has(node.id)) lastComputeError.delete(node.id);
    return out;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    if (lastComputeError.get(node.id) !== msg) {
      lastComputeError.set(node.id, msg);
      console.warn(`Node "${node.type}" (${node.id}) compute error — emitting empty output:`, err);
    }
    return Array.from({ length: node.outArity }, () => []);
  }
}

function evalNode(
  graph: RuntimeGraph,
  node: RuntimeNode,
  frame: FrameCtx,
  outputs: Map<NodeId, PortValue[]>,
  iterCtx: IterCtx | null,
  regionStats: Map<NodeId, RegionTickStats>,
): void {
  if (node.readFromContext) {
    outputs.set(node.id, node.readFromContext(frame));
    return;
  }

  // Splicer-in delivers an outer spread's per-iteration slot; loop-var
  // delivers a built-in iteration variable (index / count / fraction).
  // Both read their value from the iteration context by node id.
  if (node.splicerRole === 'splicer-in' || node.splicerRole === 'loop-var') {
    if (iterCtx) {
      const v = iterCtx.splicerInValues.get(node.id);
      outputs.set(node.id, [v ?? []]);
    } else {
      outputs.set(node.id, [[]]);
    }
    return;
  }

  if (node.splicerRole === 'splicer-out') {
    const inputs = gatherInputs(graph, node, outputs);
    outputs.set(node.id, [inputs[0] ?? []]);
    return;
  }

  // AccumulatorRead: emits the anchor's current value as `read`. The
  // anchor key in accumulatorValues is the Read node's own id.
  if (node.splicerRole === 'accumulator-read') {
    const current = iterCtx ? iterCtx.accumulatorValues.get(node.id) : undefined;
    outputs.set(node.id, [[current]]);
    return;
  }

  // AccumulatorWrite: the `write` input is gathered by runRegion after
  // each iteration to advance the paired Read. `final` is published
  // once after the loop completes; during the loop it emits undefined.
  if (node.splicerRole === 'accumulator-write') {
    outputs.set(node.id, [[undefined]]);
    return;
  }

  if (node.regionKind) {
    runRegion(graph, node, frame, outputs, regionStats);
    return;
  }

  // state/read (and state/read-spread): look up the matching write by
  // slot+kind and emit its persisted value. Falls back to `init`
  // (port 0) on frame 0 / when no writer is paired, so simulations seed
  // cleanly.
  if (node.type === 'state/read' || node.type === 'state/read-spread') {
    const writeType = node.type === 'state/read' ? 'state/write' : 'state/write-spread';
    const slot = paramStore.getKey(node.id, 'slot');
    const writer = graph.slotWriters?.get(`${writeType}:${slot}`);
    const persisted = writer?.runtimeState?.value;
    if (persisted !== undefined) {
      outputs.set(node.id, [persisted]);
    } else {
      const inputs = gatherInputs(graph, node, outputs);
      outputs.set(node.id, [inputs[0] ?? []]);
    }
    return;
  }

  // Stateful node: emit `current` (port 0 of outputs) = persisted value
  // from last frame's post-eval, falling back to `init` (port 0 of
  // inputs) on tick zero. The `next` input is gathered separately by
  // captureStatefulNext after this scope finishes.
  if (node.stateful) {
    // state/write has no outputs and no `init` — it just stashes its
    // `next` input post-tick. Nothing to emit during the scope pass.
    if (node.outArity === 0) {
      outputs.set(node.id, []);
      return;
    }
    const persisted = node.runtimeState?.value;
    if (persisted !== undefined) {
      outputs.set(node.id, [persisted]);
    } else {
      const initInputs = gatherInputs(graph, node, outputs);
      const init = initInputs[0] ?? [];
      outputs.set(node.id, [init]);
      // Seed runtimeState so a sample that never wires `next` still
      // sees a stable value across frames.
      if (node.runtimeState) node.runtimeState.value = init;
    }
    return;
  }

  if (!node.compute) {
    outputs.set(node.id, Array.from({ length: node.outArity }, () => []));
    return;
  }
  const inputs = gatherInputs(graph, node, outputs);

  // ─── Pure-compute memoisation ──────────────────────────────────────────
  // When `memoisable` (compile time: pure compute, no audio ports, no
  // stateful / region / source flags), compare this tick's inputs against
  // the previous tick's at the same iteration index. If structurally
  // equal, reuse the cached outputs and skip the compute call. Foreach
  // bodies key by iterIdx so iteration 0's memo doesn't poison iteration
  // 35; the root scope uses -1.
  //
  // The leverage: static or rarely-updated chains become free across
  // ticks — the node graph lets us memoise where an imperative draw loop
  // would recompute every frame.
  if (node.memoisable && node.runtimeState) {
    const iterIdx = iterCtx ? iterCtx.iterIdx : -1;
    let memo = node.runtimeState.memo;
    const prev = memo?.get(iterIdx);
    if (prev && shallowInputsEqual(prev.inputs, inputs)) {
      outputs.set(node.id, prev.outputs);
      return;
    }
    const result = safeCompute(node, inputs);
    // Don't memoise a thrown-and-recovered result: it returns the empty
    // fallback, and caching that would pin the node to empty output until
    // its inputs happen to change. Leaving it unmemoised means we retry
    // next frame and recover the instant the underlying cause is fixed.
    if (!lastComputeError.has(node.id)) {
      if (!memo) {
        memo = new Map();
        node.runtimeState.memo = memo;
      }
      memo.set(iterIdx, { inputs, outputs: result });
    }
    outputs.set(node.id, result);
    return;
  }

  outputs.set(node.id, safeCompute(node, inputs));
}

/**
 * Shallow structural equality across two input-arrays. Each entry is a
 * `PortValue` (a spread array of unknowns). Two ports are equal iff:
 *  - Same array reference (cheap upstream-memoised path), OR
 *  - Same length AND every element pair is `===`.
 *
 * Reference equality on elements is the right call for our value
 * lattice: primitives (number/string/boolean) compare by value;
 * objects (Color, Vec2, Layer/SceneNode, image surfaces) preserve identity
 * when the upstream is also memoised. The first tick after a graph
 * change has no memo → recompute → fresh outputs stored → subsequent
 * ticks hit the memo chain all the way down.
 *
 * Costs O(total input elements) per node per tick — negligible against
 * the savings (skipping a 200ms compute beats spending 5µs comparing).
 */
function shallowInputsEqual(a: PortValue[], b: PortValue[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const x = a[i];
    const y = b[i];
    if (x === y) continue;
    if (!x || !y) return false;
    if (x.length !== y.length) return false;
    for (let j = 0; j < x.length; j++) {
      if (x[j] !== y[j]) return false;
    }
  }
  return true;
}

/**
 * Execute a region: gather outer inputs (one per splicer-in child),
 * compute N = min input spread length, loop N times feeding slot i to
 * each splicer-in's inner output, and aggregate splicer-out values into
 * spreads written to the region's own `outputs` slots.
 *
 * Region input/output order is established at compile time:
 *   - region.inputs[i]  ←→ splicerInIds[i]
 *   - region.outputs[i] ←→ splicerOutIds[i]
 *
 * Cross-scope edges have been rewritten by compile to target the region
 * (not the splicer children) — so `gatherInputs` here looks just like
 * any other node.
 */
function runRegion(
  graph: RuntimeGraph,
  region: RuntimeNode,
  frame: FrameCtx,
  outputs: Map<NodeId, PortValue[]>,
  regionStats: Map<NodeId, RegionTickStats>,
): void {
  const innerScope = graph.scopes.get(region.id);
  if (!innerScope) {
    outputs.set(region.id, Array.from({ length: region.outArity }, () => []));
    regionStats.set(region.id, { iterations: 0, inputLengths: [], bottleneckIdx: -1 });
    return;
  }

  // Gather the outer inputs. Layout:
  //   inputs[0 .. staticIn-1]                                        = static region inputs (e.g. If.condition)
  //   inputs[staticIn .. +splicerInCount-1]                          = outer spreads for splicer-ins
  //   inputs[staticIn + splicerInCount .. +accumCount-1]             = outer init values for accumulators
  const outerInputs = gatherInputs(graph, region, outputs);
  const staticIn = region.regionStaticInArity ?? 0;
  // ForEach `delay` (static input 0): rewinds each iteration's time source
  // by `iter * delay` seconds so time-driven inner nodes stagger. Only the
  // time SOURCE is shifted — stateful inner nodes keep their real-time
  // state (see foreach.ts node description).
  let delay = 0;
  if (region.regionKind === 'foreach') {
    const d = (outerInputs[0] ?? [])[0];
    if (typeof d === 'number' && Number.isFinite(d)) delay = d;
  }
  const nSplicersIn = innerScope.splicerInIds.length;
  const nAccums = innerScope.accumulatorIds.length;
  const nSplicersOut = innerScope.splicerOutIds.length;

  // Record per-splicer-in lengths up front so the stats reflect the real
  // outer inputs even when the region short-circuits (If false, Cache hit).
  const inputLengths: number[] = [];
  for (let i = 0; i < nSplicersIn; i++) {
    inputLengths.push((outerInputs[staticIn + i] ?? []).length);
  }

  // ─── Cache region: short-circuit if inputs haven't changed ───────────────
  if (region.regionKind === 'cache' && region.runtimeState) {
    const hash = hashInputs(outerInputs);
    const cached = region.runtimeState.cache;
    if (cached && cached.hash === hash) {
      outputs.set(region.id, cached.outputs);
      regionStats.set(region.id, {
        iterations: Math.min(...(inputLengths.length ? inputLengths : [1])),
        inputLengths,
        bottleneckIdx: -1,
      });
      return;
    }
  }

  // ─── If region: skip iteration when condition is false ───────────────────
  if (region.regionKind === 'if') {
    const condRaw = (outerInputs[0] ?? [])[0];
    const cond = typeof condRaw === 'boolean' ? condRaw : Boolean(condRaw);
    if (!cond) {
      // Empty splicer-out spreads + accumulator inits forwarded as finals.
      const empties: PortValue[] = innerScope.splicerOutIds.map(() => []);
      for (let i = 0; i < nAccums; i++) {
        const initSpread = outerInputs[staticIn + nSplicersIn + i] ?? [];
        empties.push([initSpread[0]]);
      }
      outputs.set(region.id, empties);
      regionStats.set(region.id, { iterations: 0, inputLengths, bottleneckIdx: -1 });
      return;
    }
  }

  // Iteration count.
  // - foreach: min over splicer-in lengths.
  // - if (true here): exactly one iteration.
  // - cache: same as foreach.
  let N = Infinity;
  let bottleneckIdx = -1;
  if (region.regionKind === 'if') {
    N = 1;
  } else if (nSplicersIn === 0) {
    N = region.regionKind === 'foreach' ? 0 : 1;
  } else {
    for (let i = 0; i < nSplicersIn; i++) {
      const v = outerInputs[staticIn + i] ?? [];
      if (v.length < N) {
        N = v.length;
        bottleneckIdx = i;
      }
    }
    if (!Number.isFinite(N)) N = 0;
    // Only call it a bottleneck if at least one input exceeds the min.
    const allEqual = inputLengths.every((l) => l === N);
    if (allEqual) bottleneckIdx = -1;
  }

  // Initialize accumulator values from their outer init slots.
  const accumValues = new Map<NodeId, unknown>();
  for (let i = 0; i < nAccums; i++) {
    const accumId = innerScope.accumulatorIds[i]!;
    const initSpread = outerInputs[staticIn + nSplicersIn + i] ?? [];
    accumValues.set(accumId, initSpread[0]);
  }

  const outBuffers: unknown[][] = innerScope.splicerOutIds.map(() => []);
  // Per-iteration trace of each splicer-in's emitted slot. Same shape as
  // outBuffers; used by the inspector to expose the inner per-iter view
  // when the user hovers a region's inner-source pin.
  const inBuffers: unknown[][] = innerScope.splicerInIds.map(() => []);
  // Per-iteration trace of each accumulator's `read` value (the value
  // visible inside the iteration body, BEFORE that iteration's `write`
  // updates it). Length matches iteration count.
  const accumBuffers: unknown[][] = innerScope.accumulatorIds.map(() => []);
  // Per-iteration trace of each ordinary inner-body node's outputs, so
  // the inspector tooltip on an inner pin shows the spread of values
  // across iterations instead of nothing. Allocated lazily on first
  // sighting of each node so multi-port shapes are captured correctly.
  const splicerInSet = new Set(innerScope.splicerInIds);
  const splicerOutSet = new Set(innerScope.splicerOutIds);
  const accumSet = new Set(innerScope.accumulatorIds);
  // Pair members (read + write physical ids) also belong to the
  // "accumulator" bucket for buffer / overwrite purposes.
  if (innerScope.accumulatorPairs) {
    for (const { readId, writeId } of innerScope.accumulatorPairs.values()) {
      accumSet.add(readId);
      accumSet.add(writeId);
    }
  }
  const bodyBuffers = new Map<NodeId, unknown[][]>();

  for (let iter = 0; iter < N; iter++) {
    const splicerInValues = new Map<NodeId, PortValue>();
    for (let i = 0; i < nSplicersIn; i++) {
      const splicerInId = innerScope.splicerInIds[i]!;
      const outer = outerInputs[staticIn + i] ?? [];
      const slot = outer[iter];
      splicerInValues.set(splicerInId, [slot]);
      inBuffers[i]!.push(slot);
    }
    // Snapshot the accumulator read values BEFORE this iteration runs.
    for (let i = 0; i < nAccums; i++) {
      accumBuffers[i]!.push(accumValues.get(innerScope.accumulatorIds[i]!));
    }
    // Built-in loop variables, keyed by deterministic synthetic id so the
    // `region/loop-var` source nodes (if wired) resolve them here. Each is
    // a single-kind value, so wrap as a length-1 spread (matches splicers).
    splicerInValues.set(loopVarSplicerId(region.id, 'index'), [iter]);
    splicerInValues.set(loopVarSplicerId(region.id, 'count'), [N]);
    splicerInValues.set(loopVarSplicerId(region.id, 'fraction'), [N > 1 ? iter / (N - 1) : 0]);
    const iterCtx: IterCtx = { splicerInValues, accumulatorValues: accumValues, iterIdx: iter, count: N };

    // Stagger the time source per iteration when a delay is set.
    const iterFrame: FrameCtx = delay !== 0 ? { ...frame, t: frame.t - iter * delay } : frame;
    const innerOutputs = new Map<NodeId, PortValue[]>(outputs);
    evalScope(graph, innerScope, iterFrame, innerOutputs, iterCtx, regionStats);

    for (let i = 0; i < nSplicersOut; i++) {
      const id = innerScope.splicerOutIds[i]!;
      const v = innerOutputs.get(id);
      outBuffers[i]!.push(v?.[0]?.[0]);
    }

    // Capture each ordinary body node's outputs for the inspector. We
    // store one slot per (node, port, iteration) so hovering an inner
    // pin reveals the spread of values that pin emitted across the run.
    // Splicer/accumulator IDs are published below with their own traces.
    for (const nodeId of innerScope.order) {
      if (splicerInSet.has(nodeId) || splicerOutSet.has(nodeId) || accumSet.has(nodeId)) continue;
      const v = innerOutputs.get(nodeId);
      if (!v) continue;
      let buf = bodyBuffers.get(nodeId);
      if (!buf) {
        buf = v.map(() => [] as unknown[]);
        bodyBuffers.set(nodeId, buf);
      }
      for (let p = 0; p < v.length; p++) {
        // Each port value is itself a spread; capture slot 0 — that's
        // what a tooltip on a single-kind pin would display naturally,
        // and matches the semantics of `getRuntimeValue`'s unwrap rule.
        const portBuf = buf[p];
        if (portBuf !== undefined) portBuf.push(v[p]?.[0]);
      }
    }

    // After the inner scope runs, gather each accumulator pair's Write
    // input (port 0) and stash it as the next iteration's read value.
    for (const accumId of innerScope.accumulatorIds) {
      const pair = innerScope.accumulatorPairs?.get(accumId);
      if (!pair) continue;
      const writeNode = graph.nodes.get(pair.writeId);
      if (!writeNode) continue;
      const writeInputs = gatherInputs(graph, writeNode, innerOutputs);
      const writeVal = (writeInputs[0] ?? [])[0];
      if (writeVal !== undefined) accumValues.set(accumId, writeVal);
    }
  }

  // Publish each inner-body node's per-iteration outputs to the global
  // outputs map. Each port becomes a spread of length N — the inspector
  // unwraps length-1 spreads automatically and stringifies longer ones
  // as `[v0, v1, …×N]` via formatValue.
  for (const [nodeId, buf] of bodyBuffers) {
    outputs.set(nodeId, buf as PortValue[]);
  }

  // Publish per-iteration traces under each splicer / accumulator's
  // synthetic id. The editor inspector reads these to power tooltips on
  // region inner pins and accumulator reads.
  for (let i = 0; i < nSplicersIn; i++) {
    outputs.set(innerScope.splicerInIds[i]!, [inBuffers[i]!]);
  }
  for (let i = 0; i < nSplicersOut; i++) {
    outputs.set(innerScope.splicerOutIds[i]!, [outBuffers[i]!]);
  }
  for (let i = 0; i < nAccums; i++) {
    const accumId = innerScope.accumulatorIds[i]!;
    const pair = innerScope.accumulatorPairs?.get(accumId);
    if (!pair) continue;
    // Read anchor: outputs[0] = per-iteration trace of `read`.
    outputs.set(pair.readId, [accumBuffers[i]!]);
    // Write: outputs[0] = `final` (the last accumulator value).
    outputs.set(pair.writeId, [[accumValues.get(accumId)]]);
  }

  // Region outputs: splicer-out spreads first, then accumulator finals.
  const finalOutputs: PortValue[] = [...outBuffers];
  for (const accumId of innerScope.accumulatorIds) {
    finalOutputs.push([accumValues.get(accumId)]);
  }
  outputs.set(region.id, finalOutputs);
  regionStats.set(region.id, { iterations: N, inputLengths, bottleneckIdx });

  // Cache region: stash the computed outputs against the input hash.
  if (region.regionKind === 'cache' && region.runtimeState) {
    region.runtimeState.cache = { hash: hashInputs(outerInputs), outputs: finalOutputs };
  }
}

/**
 * Hash a region's outer inputs for cache-region comparison. Stringifies
 * structurally; functions (Layers) hash to `function`, which means cache
 * regions over Layer inputs won't be useful — by design, since Layer
 * closures capture state and aren't comparable.
 */
function hashInputs(inputs: PortValue[]): string {
  return JSON.stringify(inputs, (_k, v) => {
    if (typeof v === 'function') return 'function';
    return v;
  });
}

function gatherInputs(
  graph: RuntimeGraph,
  node: RuntimeNode,
  outputs: Map<NodeId, PortValue[]>,
): PortValue[] {
  const result: PortValue[] = new Array(node.inArity);
  const multi = node.multiInPorts;
  const constants = node.constantInPorts;
  const constValues = node.constants;
  for (let i = 0; i < node.inArity; i++) {
    if (constants?.has(i)) {
      // Constant — served from the compile-time cache, never re-gathered.
      result[i] = constValues?.[i] ?? node.defaults[i] ?? [];
    } else if (multi?.has(i)) {
      // Multi-kind starts empty; each incoming wire below appends to it.
      result[i] = [];
    } else {
      const override = node.defaultOverride?.(i);
      result[i] = override ?? node.defaults[i] ?? [];
    }
  }

  // Global edges (cross-scope broadcasts come for free this way).
  const edges = graph.edgesByTarget.get(node.id);
  if (!edges) return result;

  for (const e of edges) {
    // Constant ports are pinned to their compile-time value — wires into
    // them are honoured at compile, not here.
    if (constants?.has(e.toPort)) continue;
    const upstream = outputs.get(e.fromNode);
    if (!upstream) continue;
    const v = upstream[e.fromPort];
    if (v === undefined) continue;
    if (multi?.has(e.toPort)) {
      // Append every element of the upstream value (treating it as a
      // Spread). Single-kind sources already arrive as length-1 spreads
      // from the runtime, so this is a flat concat in both cases.
      const existing = result[e.toPort] as unknown[];
      for (const item of v) existing.push(item);
    } else {
      result[e.toPort] = v;
    }
  }
  return result;
}
