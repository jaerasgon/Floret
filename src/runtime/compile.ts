import { buildRuntimeGraph, type RuntimeEdge, type RuntimeGraph, type RuntimeNode } from './graph';
import { applyKinded, type PortKind, type RawComputeFn } from './spread';
import type { PortValue } from './types';
import { getResolvedInputs, type NodeDef, type NodeParamContext } from '../nodes/types';
import { paramStore } from './param-store';

/** Structural interface for the registry — decouples compile from the singleton. */
export interface RegistryLike {
  require(type: string): NodeDef;
  get(type: string): NodeDef | undefined;
}

/**
 * Editor → Runtime bridge.
 *
 * Scopes: each editor node has a `parentId` (undefined = root scope, else
 * the id of an enclosing region). compile groups nodes by parentId,
 * topo-sorts each scope independently, and rewrites cross-scope edges so
 * that:
 *   - "outer → splicer-in.outer-pin" becomes "outer → enclosing-region",
 *     with the port index = splicer-in's position in the region's
 *     splicer-in list
 *   - "splicer-out.outer-pin → outer" becomes "enclosing-region → outer",
 *     same indexing convention
 *
 * The buildRuntimeGraph step refuses any remaining cross-scope edges as
 * fatal — those should have been rewritten here.
 */

export interface EditorNodeRef {
  id: string;
  defType: string;
  params: Record<string, unknown>;
  parentId?: string;
  /**
   * Dynamic ports on a region. Compile expands these into virtual
   * splicer-in / splicer-out RuntimeNodes (one per port) and rewrites
   * any edges that reference the region's `in-<portId>` / `out-<portId>`
   * handles to target the synthetic splicer instead.
   *
   * Only present on region nodes.
   */
  ports?: ReadonlyArray<RegionPortRef>;
}

export interface RegionPortRef {
  id: string;
  name: string;
  type: import('./types').PortType;
  direction: 'input' | 'output';
}

export interface EditorEdgeRef {
  fromNode: string;
  fromPort: string;
  toNode: string;
  toPort: string;
}

export interface CompileOptions {
  registry: RegistryLike;
  paramReader: (nodeId: string) => Record<string, unknown>;
  portValueReader?: (nodeId: string, portName: string) => unknown | undefined;
}

/**
 * A compile-time warning. Carries the message plus optional references
 * to the offending nodes/edges so the editor can highlight them and
 * focus on click. Existing call sites use `warningRefs.push({...})` or
 * the `warn(...)` shorthand defined inside `compileGraph`.
 */
export interface CompileWarning {
  message: string;
  /** Ids of nodes implicated in the warning. Used for focus + chip. */
  nodeIds?: string[];
  /** Ids of edges implicated in the warning. */
  edgeIds?: string[];
}

export interface CompileResult {
  graph: RuntimeGraph;
  warnings: CompileWarning[];
}

export function compileGraph(
  nodesIn: ReadonlyArray<EditorNodeRef>,
  edgesIn: ReadonlyArray<EditorEdgeRef>,
  opts: CompileOptions,
): CompileResult {
  const warnings: CompileWarning[] = [];
  // Helper: push a structured warning. Pass nodeIds/edgeIds as needed.
  const warn = (message: string, nodeIds?: string[], edgeIds?: string[]) => {
    warnings.push({ message, nodeIds, edgeIds });
  };

  // ─── Pass -1: flatten subpatches ─────────────────────────────────────────
  // Subpatches are *editor-only* scope boundaries — they hide a sub-tree
  // behind manually-defined input/output ports for visual organisation,
  // but the runtime treats them as transparent: every port is just a
  // direct alias from supply to consumer.
  //
  // For each port `p` on subpatch `S`:
  //   supply-edges  = edges INTO  (S, p) — the source(s) of the value
  //   consumer-edges = edges OUT OF (S, p) — the consumer(s) of the value
  //   each (supply, consumer) pair becomes a direct edge from supply's
  //   source to consumer's target.
  // S's children get reparented to S's parent; S itself is removed.
  const flat = flattenSubpatches(nodesIn, edgesIn, opts.registry);

  // ─── Pass 0: expand region frame ports into synthetic splicer children ───
  const { nodes, edges } = expandRegionPorts(flat.nodes, flat.edges, opts.registry);

  // ─── Pass 1: index nodes and identify splicer children of each region ────
  const nodeById = new Map<string, EditorNodeRef>(nodes.map((n) => [n.id, n] as const));
  const defByNodeId = new Map<string, NodeDef>();
  for (const n of nodes) {
    const def = opts.registry.get(n.defType);
    if (!def) {
      warn(`Unknown node type "${n.defType}" — node skipped.`, [n.id]);
      continue;
    }
    defByNodeId.set(n.id, def);
  }

  // For each region: ordered splicer / accumulator children.
  interface RegionMeta {
    splicerInIds: string[];
    splicerOutIds: string[];
    accumulatorIds: string[];
    splicerInIndex: Map<string, number>;
    splicerOutIndex: Map<string, number>;
    accumulatorIndex: Map<string, number>;
  }
  const regionMeta = new Map<string, RegionMeta>();
  for (const n of nodes) {
    const def = defByNodeId.get(n.id);
    if (def?.regionKind) {
      regionMeta.set(n.id, {
        splicerInIds: [],
        splicerOutIds: [],
        accumulatorIds: [],
        splicerInIndex: new Map(),
        splicerOutIndex: new Map(),
        accumulatorIndex: new Map(),
      });
    }
  }
  for (const n of nodes) {
    if (!n.parentId) continue;
    const def = defByNodeId.get(n.id);
    if (!def) continue;
    const meta = regionMeta.get(n.parentId);
    if (!meta) continue;
    if (def.regionRole === 'splicer-in') {
      meta.splicerInIndex.set(n.id, meta.splicerInIds.length);
      meta.splicerInIds.push(n.id);
    } else if (def.regionRole === 'splicer-out') {
      meta.splicerOutIndex.set(n.id, meta.splicerOutIds.length);
      meta.splicerOutIds.push(n.id);
    }
  }
  // Second sweep: pair accumulator-read with accumulator-write by `slot`
  // within each region. Each (Read, Write) pair occupies ONE entry in
  // accumulatorIds (anchor = Read's id). Unmatched Reads/Writes get
  // warnings and are dropped from the index.
  for (const [regionId, meta] of regionMeta) {
    const readsBySlot = new Map<number, string>();
    const writesBySlot = new Map<number, string>();
    for (const n of nodes) {
      if (n.parentId !== regionId) continue;
      const d = defByNodeId.get(n.id);
      if (!d) continue;
      const slot = Number(n.params?.slot ?? 1);
      if (d.regionRole === 'accumulator-read') readsBySlot.set(slot, n.id);
      else if (d.regionRole === 'accumulator-write') writesBySlot.set(slot, n.id);
    }
    for (const [slot, readId] of readsBySlot) {
      const writeId = writesBySlot.get(slot);
      if (writeId === undefined) {
        warn(`Accumulator Read (slot ${slot}) in region "${regionId}" has no matching Write — pair dropped.`, [readId, regionId]);
        continue;
      }
      // The anchor index is shared by both Read (for init in) and Write
      // (for final out). Map both physical ids to the same accumIdx so
      // edge-rewrite can resolve either side.
      const idx = meta.accumulatorIds.length;
      meta.accumulatorIndex.set(readId, idx);
      meta.accumulatorIndex.set(writeId, idx);
      meta.accumulatorIds.push(readId);
    }
    for (const [slot, writeId] of writesBySlot) {
      if (!readsBySlot.has(slot)) {
        warn(`Accumulator Write (slot ${slot}) in region "${regionId}" has no matching Read — pair dropped.`, [writeId, regionId]);
      }
    }
  }

  // ─── Pass 2: build RuntimeNodes ──────────────────────────────────────────
  const runtimeNodes: RuntimeNode[] = [];
  for (const n of nodes) {
    const def = defByNodeId.get(n.id);
    if (!def) continue;
    runtimeNodes.push(defToRuntimeNode(def, n, opts.paramReader, opts.portValueReader, regionMeta.get(n.id)));
  }

  // ─── Pass 3: edges — rewrite cross-scope ones ────────────────────────────
  const runtimeEdges: RuntimeEdge[] = [];
  // Cache resolved inputs per node (dynamicInputs may be non-trivial).
  const inputsByNodeId = new Map<string, ReadonlyArray<{ name: string; type: string; kind?: string }>>();
  function resolvedInputsFor(nodeId: string): ReadonlyArray<{ name: string; type: string; kind?: string }> {
    let cached = inputsByNodeId.get(nodeId);
    if (cached) return cached;
    const n = nodeById.get(nodeId);
    const d = defByNodeId.get(nodeId);
    if (!n || !d) { cached = []; }
    else cached = getResolvedInputs(d, n.params);
    inputsByNodeId.set(nodeId, cached);
    return cached;
  }
  for (const e of edges) {
    const src = nodeById.get(e.fromNode);
    const dst = nodeById.get(e.toNode);
    if (!src) { warn(`Edge from missing node "${e.fromNode}" — wire dropped.`); continue; }
    if (!dst) { warn(`Edge to missing node "${e.toNode}" — wire dropped.`); continue; }
    const srcDef = defByNodeId.get(src.id);
    const dstDef = defByNodeId.get(dst.id);
    if (!srcDef || !dstDef) {
      warn(`Edge between nodes of unknown type — wire dropped.`, [src.id, dst.id]);
      continue;
    }
    const dstInputs = resolvedInputsFor(dst.id);

    const srcScope = src.parentId;
    const dstScope = dst.parentId;

    if (srcScope === dstScope) {
      // Intra-scope edge — resolve port names normally.
      const fromIdx = srcDef.outputs.findIndex((p) => p.name === e.fromPort);
      const toIdx = dstInputs.findIndex((p) => p.name === e.toPort);
      if (fromIdx < 0) { warn(`Unknown output port "${e.fromPort}" on "${e.fromNode}" — wire dropped.`, [src.id]); continue; }
      if (toIdx < 0) { warn(`Unknown input port "${e.toPort}" on "${e.toNode}" — wire dropped.`, [dst.id]); continue; }
      // Spread → single inside the same scope: the runtime's applyKinded
      // silently drops every slot but the first. Surface it so the user
      // knows to wrap the consumer in a ForEach.
      const srcPort = srcDef.outputs[fromIdx]!;
      const dstPort = dstInputs[toIdx]!;
      if ((srcPort.kind ?? 'single') === 'spread' && (dstPort.kind ?? 'single') === 'single') {
        const hint =
          srcPort.type === 'layer'
            ? 'Wrap the consumer in a ForEach, or feed the spread through a Group first.'
            : 'Wrap the consumer in a ForEach region to iterate.';
        warn(
          `Spread wired into single-value input "${e.toNode}.${e.toPort}" — only slot 0 reaches it. ${hint}`,
          [src.id, dst.id],
        );
        continue;
      }
      runtimeEdges.push({
        fromNode: e.fromNode, fromPort: fromIdx,
        toNode: e.toNode, toPort: toIdx,
      });
      continue;
    }

    // Cross-scope edge. Two legal cases under the R1 model:
    //   A) "outer → splicer-in" — dst is a splicer-in whose parent (region)
    //       sits in the outer scope.
    //   B) "splicer-out → outer" — src is a splicer-out whose parent (region)
    //       sits in the outer scope.
    if (dstDef.regionRole === 'splicer-in' && dst.parentId && nodeById.get(dst.parentId)?.parentId === srcScope) {
      const regionId = dst.parentId;
      const meta = regionMeta.get(regionId);
      if (!meta) { warn(`Splicer-in "${dst.id}" has no region meta — internal compile bug.`, [dst.id, regionId]); continue; }
      const fromIdx = srcDef.outputs.findIndex((p) => p.name === e.fromPort);
      if (fromIdx < 0) { warn(`Unknown output port "${e.fromPort}" on "${e.fromNode}" — wire dropped.`, [src.id]); continue; }
      const sIdx = meta.splicerInIndex.get(dst.id);
      if (sIdx === undefined) { warn(`Splicer-in "${dst.id}" not indexed — internal compile bug.`, [dst.id, regionId]); continue; }
      const toIdx = resolvedInputsFor(regionId).length + sIdx;
      runtimeEdges.push({
        fromNode: e.fromNode, fromPort: fromIdx,
        toNode: regionId, toPort: toIdx,
      });
      continue;
    }
    if (srcDef.regionRole === 'splicer-out' && src.parentId && nodeById.get(src.parentId)?.parentId === dstScope) {
      const regionId = src.parentId;
      const meta = regionMeta.get(regionId);
      if (!meta) { warn(`Splicer-out "${src.id}" has no region meta — internal compile bug.`, [src.id, regionId]); continue; }
      const fromIdx = meta.splicerOutIndex.get(src.id);
      if (fromIdx === undefined) { warn(`Splicer-out "${src.id}" not indexed — internal compile bug.`, [src.id, regionId]); continue; }
      const toIdx = dstInputs.findIndex((p) => p.name === e.toPort);
      if (toIdx < 0) { warn(`Unknown input port "${e.toPort}" on "${e.toNode}" — wire dropped.`, [dst.id]); continue; }
      runtimeEdges.push({
        fromNode: regionId, fromPort: fromIdx,
        toNode: e.toNode, toPort: toIdx,
      });
      continue;
    }
    // AccumulatorRead init: outer → accRead.init becomes outer → region.input[nSplicersIn + accumIdx]
    if (dstDef.regionRole === 'accumulator-read' && e.toPort === 'init' && dst.parentId && nodeById.get(dst.parentId)?.parentId === srcScope) {
      const regionId = dst.parentId;
      const meta = regionMeta.get(regionId);
      if (!meta) { warn(`AccRead "${dst.id}" has no region meta — internal compile bug.`, [dst.id, regionId]); continue; }
      const fromIdx = srcDef.outputs.findIndex((p) => p.name === e.fromPort);
      if (fromIdx < 0) { warn(`Unknown output port "${e.fromPort}" on "${e.fromNode}" — wire dropped.`, [src.id]); continue; }
      const accIdx = meta.accumulatorIndex.get(dst.id);
      if (accIdx === undefined) { warn(`AccRead "${dst.id}" not paired with a Write — wire dropped.`, [dst.id, regionId]); continue; }
      const toIdx = resolvedInputsFor(regionId).length + meta.splicerInIds.length + accIdx;
      runtimeEdges.push({
        fromNode: e.fromNode, fromPort: fromIdx,
        toNode: regionId, toPort: toIdx,
      });
      continue;
    }
    // AccumulatorWrite final: accWrite.final → outer becomes region.output[nSplicersOut + accumIdx] → outer
    if (srcDef.regionRole === 'accumulator-write' && e.fromPort === 'final' && src.parentId && nodeById.get(src.parentId)?.parentId === dstScope) {
      const regionId = src.parentId;
      const meta = regionMeta.get(regionId);
      if (!meta) { warn(`AccWrite "${src.id}" has no region meta — internal compile bug.`, [src.id, regionId]); continue; }
      const accIdx = meta.accumulatorIndex.get(src.id);
      if (accIdx === undefined) { warn(`AccWrite "${src.id}" not paired with a Read — wire dropped.`, [src.id, regionId]); continue; }
      const fromIdx = meta.splicerOutIds.length + accIdx;
      const toIdx = dstInputs.findIndex((p) => p.name === e.toPort);
      if (toIdx < 0) { warn(`Unknown input port "${e.toPort}" on "${e.toNode}" — wire dropped.`, [dst.id]); continue; }
      runtimeEdges.push({
        fromNode: regionId, fromPort: fromIdx,
        toNode: e.toNode, toPort: toIdx,
      });
      continue;
    }

    // Broadcast cross-scope edge: an outer source wired directly to an inner
    // non-splicer input. Allowed when going OUTER → INNER for `single`-kind
    // ports (the value is stable for every iteration). Disallowed inner →
    // outer (the inner value isn't stable post-iteration; the user must use
    // a splicer-out).
    const isOuterToInner =
      srcScope === undefined &&
      dst.parentId !== undefined; // dst lives inside a region (or deeper)
    const isInnerToOuter =
      src.parentId !== undefined &&
      dstScope === undefined;

    if (isOuterToInner) {
      const fromIdx = srcDef.outputs.findIndex((p) => p.name === e.fromPort);
      const toIdx = dstInputs.findIndex((p) => p.name === e.toPort);
      if (fromIdx < 0) { warn(`Unknown output port "${e.fromPort}" on "${e.fromNode}" — wire dropped.`, [src.id]); continue; }
      if (toIdx < 0) { warn(`Unknown input port "${e.toPort}" on "${e.toNode}" — wire dropped.`, [dst.id]); continue; }
      const srcPort = srcDef.outputs[fromIdx]!;
      const dstPort = dstInputs[toIdx]!;
      // Only `single` outer → `single` inner is a clean broadcast. A spread
      // OUT going to a single inner port without a splicer would silently
      // drop everything but slot 0 — surface a warning and skip.
      if ((srcPort.kind ?? 'single') === 'spread' && (dstPort.kind ?? 'single') === 'single') {
        warn(
          `Spread crossing into "${e.toNode}.${e.toPort}" needs a splicer-in inside the region.`,
          [src.id, dst.id],
        );
        continue;
      }
      runtimeEdges.push({
        fromNode: e.fromNode, fromPort: fromIdx,
        toNode: e.toNode, toPort: toIdx,
      });
      continue;
    }

    if (isInnerToOuter) {
      warn(
        `Wire from inside the region to "${e.toNode}" needs a splicer-out.`,
        [src.id, dst.id],
      );
      continue;
    }

    warn(
      `Cross-region wire from "${e.fromNode}" to "${e.toNode}" is not supported.`,
      [src.id, dst.id],
    );
  }

  // Every root-scope renderer is a render target. The graph keeps the
  // ordered list; `rendererId` is back-compat shorthand for the first.
  const rendererIds = nodes
    .filter((n) => n.defType === 'render/renderer' && !n.parentId)
    .map((n) => n.id);

  const graph = buildRuntimeGraph({
    nodes: runtimeNodes,
    edges: runtimeEdges,
    rendererIds,
  });

  // Cook-on-change scheduling: figure out whether this patch needs the
  // tick to fire every frame, or whether it can idle between events.
  // A node is "always live" if it genuinely changes value every frame
  // without any user input — time-based, cross-frame state, live MIDI/
  // mouse/keyboard, or anything that mutates Web Audio params per tick.
  // Sliders/toggles/colour-pickers are EXCLUDED — they only change on
  // user interaction, which the dirty bit catches via paramStore.
  const ALWAYS_LIVE_TYPES = new Set<string>([
    'time/seconds',
    'time/frame',
    'time/delta',
    // Motion integrators advance toward their target every frame, so the
    // patch must keep ticking even with no other live source.
    'motion/smooth',
    'motion/spring',
    'motion/trigger',
    'motion/delay-line',
    'state/value',
    'state/spread',
    'state/read',
    'state/write',
    'state/read-spread',
    'state/write-spread',
    'input/mouse',
    'input/keyboard',
    'input/midi-cc',
    'input/midi-note',
    // Camera draws the live video frame into its canvas every tick.
    'input/camera',
  ]);
  graph.alwaysAnimate = nodes.some((n) => {
    if (ALWAYS_LIVE_TYPES.has(n.defType)) return true;
    const def = opts.registry.get(n.defType);
    if (!def) return false;
    if (def.inputs.some((p) => p.type === 'audio')) return true;
    if (def.outputs.some((p) => p.type === 'audio')) return true;
    return false;
  });

  return { graph, warnings };
}

/**
 * Convert a NodeDef + nodeId into a RuntimeNode. Region orchestrators
 * have their in/out arity expanded based on their splicer children
 * (passed in via `regionMeta`). Splicer nodes carry `splicerRole` so
 * tick can dispatch to the iteration-context special-case.
 */
export function defToRuntimeNode(
  def: NodeDef,
  ref: EditorNodeRef,
  paramReader: (id: string) => Record<string, unknown>,
  portValueReader?: (nodeId: string, portName: string) => unknown | undefined,
  regionMeta?: { splicerInIds: string[]; splicerOutIds: string[]; accumulatorIds: string[] },
): RuntimeNode {
  const nodeId = ref.id;

  const resolvedInputs = getResolvedInputs(def, ref.params);

  // Region orchestrator. Layout:
  //   inputs  = static inputs (from def.inputs) ++ splicer-ins ++ accumulator inits
  //   outputs = splicer-outs ++ accumulator finals
  if (def.regionKind) {
    const staticIn = resolvedInputs.length;
    const nSplicersIn = regionMeta?.splicerInIds.length ?? 0;
    const nSplicersOut = regionMeta?.splicerOutIds.length ?? 0;
    const nAccums = regionMeta?.accumulatorIds.length ?? 0;
    const inArity = staticIn + nSplicersIn + nAccums;
    const outArity = nSplicersOut + nAccums;
    // Defaults: static inputs get their NodeDef defaults; dynamic slots are empty.
    const defaults: PortValue[] = new Array(inArity).fill([]) as PortValue[];
    for (let i = 0; i < staticIn; i++) {
      const p = resolvedInputs[i]!;
      defaults[i] = defaultForPort(p);
    }
    // Subpatches are flattened away before this path runs — `def.regionKind`
    // here is always one of the iterating kinds. Narrow the type explicitly.
    if (def.regionKind === 'subpatch') {
      throw new Error(`subpatch ${nodeId} should have been flattened pre-compile`);
    }
    return {
      id: nodeId,
      type: def.type,
      inArity,
      outArity,
      defaults,
      parentId: ref.parentId,
      regionKind: def.regionKind,
      regionStaticInArity: staticIn,
      runtimeState: {},
    };
  }

  const inArity = resolvedInputs.length;
  const outArity = def.outputs.length;
  const defaults: PortValue[] = resolvedInputs.map((p) => defaultForPort(p));

  const paramCtx: NodeParamContext = {
    nodeId,
    param<T>(key: string, fallback?: T): T {
      const live = paramStore.getKey<T>(nodeId, key);
      if (live !== undefined) return live;
      const ps = paramReader(nodeId);
      const v = ps[key];
      return v !== undefined ? (v as T) : (fallback as T);
    },
  };

  let compute: RawComputeFn | undefined;

  if (
    def.regionRole === 'splicer-in' ||
    def.regionRole === 'splicer-out' ||
    def.regionRole === 'accumulator-read' ||
    def.regionRole === 'accumulator-write' ||
    def.regionRole === 'loop-var'
  ) {
    // No compute — tick.ts evalNode handles these explicitly.
  } else if (def.stateful) {
    // No compute — tick.ts evalNode emits the persisted value (or init).
  } else if (def.type === 'state/read' || def.type === 'state/read-spread') {
    // No compute — tick.ts evalNode looks up the paired writer by slot
    // (matched on kind) and emits its persisted value (or falls back to
    // `init` on frame 0).
  } else if (def.readFromContext) {
    // Source node: tick() uses readFromContext directly, no compute.
  } else if (def.computeRaw) {
    const raw = def.computeRaw;
    compute = (inputs) => raw(inputs, paramCtx);
  } else if (def.compute) {
    const inputKinds: PortKind[] = resolvedInputs.map((p) => p.kind ?? 'single');
    const outputKinds: PortKind[] = def.outputs.map((p) => p.kind ?? 'single');
    compute = applyKinded(
      def.compute,
      { in: inArity, out: outArity },
      inputKinds,
      outputKinds,
    );
  } else if (def.inputs.length === 0 && def.outputs.length === 0) {
    // Editor-only annotation node (e.g. meta/comment). Produces nothing,
    // consumes nothing — the runtime treats it as a no-op so it can sit
    // in a patch without breaking compile.
    compute = () => [];
  } else {
    throw new Error(
      `NodeDef ${def.type} has no compute / computeRaw / readFromContext / regionKind / regionRole`,
    );
  }

  const node: RuntimeNode = {
    id: nodeId,
    type: def.type,
    inArity,
    outArity,
    defaults,
    compute,
    parentId: ref.parentId,
  };

  if (def.regionRole === 'splicer-in') node.splicerRole = 'splicer-in';
  else if (def.regionRole === 'splicer-out') node.splicerRole = 'splicer-out';
  else if (def.regionRole === 'loop-var') node.splicerRole = 'loop-var';
  else if (def.regionRole === 'accumulator-read') {
    node.splicerRole = 'accumulator-read';
    const slot = Number(ref.params?.slot ?? 1);
    node.accSlot = Number.isFinite(slot) ? slot : 1;
  } else if (def.regionRole === 'accumulator-write') {
    node.splicerRole = 'accumulator-write';
    const slot = Number(ref.params?.slot ?? 1);
    node.accSlot = Number.isFinite(slot) ? slot : 1;
  }

  if (def.stateful) {
    node.stateful = true;
    node.runtimeState = {};
    if (def.statefulNextPort !== undefined) {
      node.statefulNextPort = def.statefulNextPort;
    }
  }

  // Pure-compute memoisation eligibility. The tick loop skips recompute
  // when this node's gathered inputs are structurally unchanged since
  // last tick. Set true only for nodes whose compute is a pure function
  // of inputs:
  //
  //   - Has a `compute` (excludes splicers, accumulators, stateful,
  //     state/read, source nodes via readFromContext, region containers).
  //   - No `audio`-typed input or output — audio nodes mutate persistent
  //     Web Audio handles every tick; skipping compute would freeze the
  //     parameter updates downstream.
  //   - No `image`-typed input or output — the image registry hands out a
  //     stable canvas reference whose PIXEL CONTENT mutates when an async
  //     load completes. Reference-equality memoisation would freeze any
  //     downstream node whose other inputs are static (e.g. a draw/image
  //     with constant x/y/w/h) on its "image not ready" output forever.
  //
  // Cache regions keep their own per-region cache and don't go through
  // this path (they short-circuit before evalNode's compute branch).
  if (compute && !def.stateful && !def.effectful && !def.regionKind && !def.regionRole && !def.readFromContext) {
    const hasAudioPort =
      resolvedInputs.some((p) => p.type === 'audio') ||
      def.outputs.some((p) => p.type === 'audio');
    const hasImagePort =
      resolvedInputs.some((p) => p.type === 'image') ||
      def.outputs.some((p) => p.type === 'image');
    if (!hasAudioPort && !hasImagePort) {
      node.memoisable = true;
      if (!node.runtimeState) node.runtimeState = {};
    }
  }

  // Flag multi-kind input ports so the runtime gathers all incoming
  // wires into a spread instead of letting later edges overwrite earlier.
  const multiPorts = new Set<number>();
  resolvedInputs.forEach((p, i) => { if (p.kind === 'multi') multiPorts.add(i); });
  if (multiPorts.size > 0) node.multiInPorts = multiPorts;

  // Flag constant input ports — set-once values stashed on the node at
  // compile time and never re-gathered per tick. The actual values are
  // populated AFTER `defaultOverride` is set up below, since constants
  // are read via the same priority chain (portValues → param → default).
  const constantPorts = new Set<number>();
  resolvedInputs.forEach((p, i) => { if (p.constant === true) constantPorts.add(i); });
  if (constantPorts.size > 0) node.constantInPorts = constantPorts;

  // defaultOverride is consulted by tick() each frame for inputs with no
  // edge feeding them. Two fallback sources, in priority order:
  //   1) portValues override (from `Right-click → Set constant`)
  //   2) PortSpec.param — live read from paramStore, so an inline slider
  //      can act as a wire fallback without triggering a recompile.
  const anyPortNeedsOverride =
    !!portValueReader || resolvedInputs.some((p) => p.param !== undefined);
  if (anyPortNeedsOverride) {
    node.defaultOverride = (i: number): PortValue | undefined => {
      const port = resolvedInputs[i];
      if (!port) return undefined;
      if (portValueReader) {
        const v = portValueReader(nodeId, port.name);
        if (v !== undefined) return [v];
      }
      if (port.param) {
        const pv = paramStore.getKey(nodeId, port.param);
        if (pv !== undefined) return [pv];
      }
      return undefined;
    };
  }

  if (def.readFromContext) {
    const fn = def.readFromContext;
    node.readFromContext = (f) => fn(f, paramCtx);
  }

  // Populate constant input values once, now. Pulls from the same
  // priority chain `gatherInputs` uses for unwired ports: portValues
  // override → PortSpec.param → PortSpec.default. Wires into constant
  // pins are NOT honoured by this initial pass; a deferred compile-time
  // pass (post node setup, before topo-sort yields ticks) will resolve
  // simple upstream chains and overwrite these defaults — see
  // `resolveWiredConstants` in the caller.
  if (constantPorts.size > 0) {
    const constants: PortValue[] = new Array(node.inArity);
    for (const i of constantPorts) {
      const override = node.defaultOverride?.(i);
      constants[i] = override ?? node.defaults[i] ?? [];
    }
    node.constants = constants;
  }

  return node;
}

/**
 * Per-port default. The runtime treats spread / multi inputs as
 * "carries a LIST of T". An unwired spread port without an explicit
 * `default` should be the empty list, not `[<some auto-default>]` — a
 * `walker-cluster.colors` pin with nothing connected means "no palette
 * supplied", not "palette is [white]". Single-kind ports keep the
 * historical behaviour: wrap the per-type fallback in a length-1
 * spread so compute reads see `inputs[i][0] = <default scalar>`.
 */
function defaultForPort(p: { kind?: 'single' | 'spread' | 'multi'; type: string; default?: unknown }): PortValue {
  const kind = p.kind ?? 'single';
  const hasExplicit = p.default !== undefined;
  if (kind === 'spread' || kind === 'multi') {
    return hasExplicit ? [p.default] : [];
  }
  // Distinguish "no default declared" (use the type's fallback — white for
  // color, 0 for number, …) from "explicit null default" (the node author
  // wants compute to see null so it can branch into a hex/picker fallback
  // path; see color/palette c0..cN, style/fill 'color' mode input). The
  // previous `p.default ?? defaultByType(p.type)` collapsed both into the
  // type fallback, which made palette stops always emit white because the
  // compute never saw an unwired-null signal.
  return [hasExplicit ? p.default : defaultByType(p.type)];
}

function defaultByType(t: string): unknown {
  switch (t) {
    case 'number': return 0;
    case 'boolean': return false;
    case 'string': return '';
    case 'vec2': return { x: 0, y: 0 };
    case 'color': return { r: 1, g: 1, b: 1, a: 1 };
    case 'mat3': return [1, 0, 0, 0, 1, 0, 0, 0, 1] as const;
    case 'layer': return null;
    case 'style': return null;
    case 'audio': return null;
    case 'image': return null;
    case 'any': return null;
    default: return undefined;
  }
}

/**
 * Region-frame port preprocessor.
 *
 * For every region with `ports: [...]`, synthesize a splicer-in or
 * splicer-out child node per port and rewrite any edges that referenced
 * the region's `in-<portId>` / `out-<portId>` handles to target the
 * synthetic splicer instead. The rest of compile then runs as if those
 * splicers had been explicit editor nodes all along.
 *
 * The synthetic ids use `__sin_` / `__sout_` separators; user-generated
 * ids never contain these.
 */
export function syntheticSplicerId(
  regionId: string,
  portId: string,
  direction: 'input' | 'output',
): string {
  return `${regionId}__${direction === 'input' ? 'sin' : 'sout'}_${portId}`;
}

/**
 * ForEach/Cache built-in loop variables. Rendered as inner-source pins on
 * the region (`out-__loopvar_<name>` handles) and synthesised by
 * `expandRegionPorts` into `region/loop-var` source nodes that the runtime
 * populates from the iteration counter.
 */
export const LOOP_VAR_NAMES = ['index', 'count', 'fraction'] as const;
export type LoopVarName = (typeof LOOP_VAR_NAMES)[number];

/** Editor handle suffix (after `out-`) for a loop-variable pin. */
export function loopVarHandle(name: LoopVarName): string {
  return `__loopvar_${name}`;
}

/** Stable synthetic node id for a region's loop-variable source. Derived
 *  identically by compile (when expanding) and tick (when populating). */
export function loopVarSplicerId(regionId: string, name: LoopVarName): string {
  return `${regionId}__loopvar_${name}`;
}

/** Parse a handle suffix back to a loop-var name, or null if not one. */
export function parseLoopVarHandle(handle: string): LoopVarName | null {
  const prefix = '__loopvar_';
  if (!handle.startsWith(prefix)) return null;
  const name = handle.slice(prefix.length);
  return (LOOP_VAR_NAMES as readonly string[]).includes(name) ? (name as LoopVarName) : null;
}

/** True for region kinds that iterate and therefore expose loop variables. */
function regionKindHasLoopVars(kind: string | undefined): boolean {
  return kind === 'foreach' || kind === 'cache';
}

function expandRegionPorts(
  nodes: ReadonlyArray<EditorNodeRef>,
  edges: ReadonlyArray<EditorEdgeRef>,
  registry: RegistryLike,
): { nodes: EditorNodeRef[]; edges: EditorEdgeRef[] } {
  const outNodes: EditorNodeRef[] = [...nodes];
  const outEdges: EditorEdgeRef[] = [];

  // regionId → portId → port descriptor
  const portsByRegion = new Map<string, Map<string, RegionPortRef>>();
  // Regions (foreach/cache) that expose built-in loop variables.
  const loopVarRegions = new Set<string>();
  for (const n of nodes) {
    const def = registry.get(n.defType);
    if (!def?.regionKind) continue;
    if (regionKindHasLoopVars(def.regionKind)) loopVarRegions.add(n.id);
    const ports = n.ports ?? [];
    if (ports.length === 0) continue;
    const map = new Map<string, RegionPortRef>();
    for (const p of ports) {
      map.set(p.id, p);
      outNodes.push({
        id: syntheticSplicerId(n.id, p.id, p.direction),
        defType:
          p.direction === 'input'
            ? `region/splicer-in/${p.type}`
            : `region/splicer-out/${p.type}`,
        params: {},
        parentId: n.id,
      });
    }
    portsByRegion.set(n.id, map);
  }

  // regionId → set of loop-var names actually wired this compile. We
  // synthesise a source node only for referenced pins (unwired pins cost
  // nothing). The runtime fills these by deterministic id regardless.
  const loopVarRefs = new Map<string, Set<LoopVarName>>();

  for (const e of edges) {
    let from = e.fromNode;
    let fromPort = e.fromPort;
    let to = e.toNode;
    let toPort = e.toPort;

    // Loop-variable source: `region.out-__loopvar_<name>` → the synthetic
    // `region/loop-var` node's `inner` output. Mirrors the splicer-in
    // inner-source rewrite below.
    if (loopVarRegions.has(from)) {
      const lv = parseLoopVarHandle(fromPort);
      if (lv) {
        let set = loopVarRefs.get(from);
        if (!set) { set = new Set(); loopVarRefs.set(from, set); }
        set.add(lv);
        from = loopVarSplicerId(e.fromNode, lv);
        fromPort = 'inner';
      }
    }

    const srcPorts = portsByRegion.get(from);
    if (srcPorts) {
      const port = srcPorts.get(fromPort);
      if (port) {
        from = syntheticSplicerId(e.fromNode, port.id, port.direction);
        // Input port's inner pin = splicer-in.inner (the source emitted
        // per-iteration). Output port's outer pin = splicer-out.outer.
        fromPort = port.direction === 'input' ? 'inner' : 'outer';
      }
    }

    const dstPorts = portsByRegion.get(to);
    if (dstPorts) {
      const port = dstPorts.get(toPort);
      if (port) {
        to = syntheticSplicerId(e.toNode, port.id, port.direction);
        // Input port's outer pin = splicer-in.outer (the outer spread
        // sink). Output port's inner pin = splicer-out.inner.
        toPort = port.direction === 'input' ? 'outer' : 'inner';
      }
    }

    outEdges.push({ fromNode: from, fromPort, toNode: to, toPort });
  }

  // Synthesise a loop-var source node per referenced (region, name) pair.
  for (const [regionId, names] of loopVarRefs) {
    for (const name of names) {
      outNodes.push({
        id: loopVarSplicerId(regionId, name),
        defType: 'region/loop-var',
        params: {},
        parentId: regionId,
      });
    }
  }

  return { nodes: outNodes, edges: outEdges };
}

/**
 * Subpatch flatten pass. Subpatches (`regionKind: 'subpatch'`) are
 * editor-only abstractions — at runtime they vanish. We iteratively
 * pick any subpatch S, then:
 *
 *   1. Group every edge touching S by port id:
 *        supply  = edge whose toNode = S, toPort = portId
 *        consumer = edge whose fromNode = S, fromPort = portId
 *      (which side is "supply" vs "consumer" matches whichever physical
 *      pin is the source vs sink — input port: outer is supply, inner
 *      is consumer; output port: inner is supply, outer is consumer.)
 *
 *   2. For each port: cross-product supply × consumer → emit direct
 *      edges from the supply's source to the consumer's target.
 *
 *   3. Reparent every child of S to S.parentId, then remove S.
 *
 * Iterating handles nested subpatches naturally — the order doesn't
 * matter because each flatten is local. The loop terminates when no
 * subpatches remain.
 */
function flattenSubpatches(
  nodesIn: ReadonlyArray<EditorNodeRef>,
  edgesIn: ReadonlyArray<EditorEdgeRef>,
  registry: RegistryLike,
): { nodes: EditorNodeRef[]; edges: EditorEdgeRef[] } {
  let curNodes: EditorNodeRef[] = [...nodesIn];
  let curEdges: EditorEdgeRef[] = [...edgesIn];
  // Safety bound — more than 10k iterations would mean a registry bug.
  for (let guard = 0; guard < 10000; guard++) {
    const sIdx = curNodes.findIndex(
      (n) => registry.get(n.defType)?.regionKind === 'subpatch',
    );
    if (sIdx < 0) break;
    const sub = curNodes[sIdx]!;
    const ports = sub.ports ?? [];
    const portIds = new Set(ports.map((p) => p.id));

    const supplies = new Map<string, EditorEdgeRef[]>();
    const consumers = new Map<string, EditorEdgeRef[]>();
    for (const pid of portIds) {
      supplies.set(pid, []);
      consumers.set(pid, []);
    }
    const otherEdges: EditorEdgeRef[] = [];
    for (const e of curEdges) {
      if (e.toNode === sub.id && portIds.has(e.toPort)) {
        supplies.get(e.toPort)!.push(e);
      } else if (e.fromNode === sub.id && portIds.has(e.fromPort)) {
        consumers.get(e.fromPort)!.push(e);
      } else if (e.fromNode === sub.id || e.toNode === sub.id) {
        // Dangling edge — references a port that no longer exists. Drop it.
      } else {
        otherEdges.push(e);
      }
    }

    const newEdges: EditorEdgeRef[] = [];
    for (const pid of portIds) {
      const ss = supplies.get(pid)!;
      const cc = consumers.get(pid)!;
      for (const s of ss) {
        for (const c of cc) {
          newEdges.push({
            fromNode: s.fromNode,
            fromPort: s.fromPort,
            toNode: c.toNode,
            toPort: c.toPort,
          });
        }
      }
    }

    curEdges = [...otherEdges, ...newEdges];
    curNodes = curNodes
      .filter((n) => n.id !== sub.id)
      .map((n) => (n.parentId === sub.id ? { ...n, parentId: sub.parentId } : n));
  }
  return { nodes: curNodes, edges: curEdges };
}
