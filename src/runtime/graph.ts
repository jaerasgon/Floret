import type { FrameCtx, PortValue } from './types';
import type { RawComputeFn } from './spread';

export type NodeId = string;
export type ScopeKey = NodeId | null;

export interface RuntimeNode {
  id: NodeId;
  type: string;
  inArity: number;
  outArity: number;
  compute?: RawComputeFn;
  defaults: PortValue[];
  defaultOverride?: (portIdx: number) => PortValue | undefined;
  readFromContext?: (frame: FrameCtx) => PortValue[];
  parentId?: NodeId;
  regionKind?: 'foreach' | 'if' | 'cache';
  splicerRole?: 'splicer-in' | 'splicer-out' | 'accumulator-read' | 'accumulator-write';
  /** For accumulator-read / accumulator-write nodes: pairing slot (defaults 1). */
  accSlot?: number;
  /** Number of *static* input ports declared by the region NodeDef itself.
   *  Splicer-ins (and accumulator inits) occupy port indices AFTER these. */
  regionStaticInArity?: number;
  /**
   * Cross-frame state marker. When set, this node's `next` input is
   * excluded from topo and gathered in a post-eval pass; the current
   * value is stored on `runtimeState.value`.
   */
  stateful?: boolean;
  /** Which input port is the cross-frame `next`. Default 1 (matches
   *  state/value, state/spread). state/write uses 0. */
  statefulNextPort?: number;
  /** Set of input port indices that accept MULTIPLE incoming wires.
   *
   *  Contract — when port `i` is in this set, `gatherInputs`:
   *    1. Initialises `result[i]` to `[]` (the per-port default is
   *       NOT consulted; multi inputs are list-of-wires, not list-of-
   *       defaults).
   *    2. For each incoming edge into port `i`, appends every element
   *       of the upstream PortValue. Single-kind sources already arrive
   *       as length-1 spreads, so the append is uniform regardless of
   *       the source's own kind — single→multi adds one element,
   *       spread→multi concatenates.
   *
   *  Populated at compile from `PortSpec.kind === 'multi'`. Multi only
   *  applies on input ports; outputs declare `single` or `spread`.
   *  Order within the gather is edge-insertion order in
   *  `graph.edgesByTarget`. */
  multiInPorts?: Set<number>;
  /**
   * Set of input port indices marked `constant: true` on their PortSpec.
   *
   * Contract:
   *   - The compiler reads each constant input ONCE at compile time
   *     (after upstream nodes' compute have been evaluated for the
   *     compile-time topo pass) and stashes the value in `constants`.
   *   - `gatherInputs` SKIPS these indices entirely each tick — they're
   *     served from `constants[i]` instead of re-gathered from edges.
   *   - When the editor mutates a constant pin's source (edge change,
   *     param edit, portValue edit) the patch is marked for recompile,
   *     not just re-eval. See `editor-state.ts` recompile bumper.
   */
  constantInPorts?: Set<number>;
  /** Per-index constant values, populated at compile time. Index matches
   *  `inputs` array; entries for non-constant ports are undefined. */
  constants?: PortValue[];
  /**
   * True when this node's compute is a pure function of its inputs and
   * therefore safe to memoise across ticks. Set at compile time; the
   * tick loop honours it to skip the compute call when inputs are
   * structurally unchanged since last tick.
   *
   * Effectful nodes opt out: `audio`-typed ports (their compute mutates
   * persistent Web Audio nodes), stateful, splicers, regions, and
   * source nodes (readFromContext) all bypass the memo path.
   */
  memoisable?: boolean;
  /** Internal mutable runtime state. Used by Cache regions (cache hash +
   *  outputs), stateful nodes (the persisted value), and pure compute
   *  memoisation (per-iteration input/output cache). Rebuilt at each compile. */
  runtimeState?: {
    cache?: { hash: string; outputs: PortValue[] };
    /** Persisted state for stateful nodes — exposed as `current` next tick. */
    value?: PortValue;
    /**
     * Per-iteration memoisation for pure compute nodes. Keyed by iteration
     * index; -1 holds the root-scope entry. Stores both the previous
     * inputs (for shallow comparison against the current tick's inputs)
     * and the outputs to reuse on a hit.
     */
    memo?: Map<number, { inputs: PortValue[]; outputs: PortValue[] }>;
  };
}

export interface RuntimeEdge {
  fromNode: NodeId;
  fromPort: number;
  toNode: NodeId;
  toPort: number;
}

/**
 * A scope (root or region inner). Topo order is computed only over
 * intra-scope edges plus the synthetic "outer-source → enclosing-region"
 * deps that we inject for cross-scope wires.
 */
export interface RuntimeScope {
  parentId: ScopeKey;
  order: NodeId[];
  splicerInIds: NodeId[];
  splicerOutIds: NodeId[];
  /** Accumulator entries for a region scope, in declaration order.
   *  Each entry is a (Read, Write) pair matched by `slot`; the Read's
   *  node id is the anchor used here. Order matches the region's
   *  outer-input / outer-output layout for accumulators (one init slot
   *  + one final slot per entry). */
  accumulatorIds: NodeId[];
  /** Pair lookup: anchor (Read) id → both physical node ids. */
  accumulatorPairs?: Map<NodeId, { readId: NodeId; writeId: NodeId }>;
}

export interface RuntimeGraph {
  nodes: Map<NodeId, RuntimeNode>;
  scopes: Map<ScopeKey, RuntimeScope>;
  /**
   * True when the graph contains at least one node whose output changes
   * every frame independent of any user input (time, state, audio source,
   * live MIDI/mouse/keyboard). Cook-on-change: when this is false, the
   * frame hook skips the tick + paint between events and lets the canvas
   * stand pat at zero CPU cost. Set in compile.
   */
  alwaysAnimate?: boolean;
  /**
   * GLOBAL edges-by-target. Includes intra-scope wires AND cross-scope
   * broadcast wires (outer-single → inner-single). Splicer-routed
   * cross-scope wires are rewritten by compile so they live entirely
   * within the outer scope's view of the region.
   *
   * `tick` reads from this map regardless of scope when gathering inputs.
   * Cross-scope reads work because the outer scope evaluates before any
   * region descends, so the outer outputs are already in the live outputs
   * map by the time inner nodes look for them.
   */
  edgesByTarget: Map<NodeId, RuntimeEdge[]>;
  /** First renderer id — back-compat shorthand for `rendererIds[0] ?? null`. */
  rendererId: NodeId | null;
  /** Every `render/renderer` node at the root scope, in declaration
   *  order. Multiple renderers each get their own Preview / output
   *  canvas; the runtime's tick stage produces a layer spread per id. */
  rendererIds: NodeId[];
  /**
   * Slot → state/write node lookup, refreshed at the start of each tick
   * by `tick()` via `buildSlotWriters`. `state/read` resolves its paired
   * writer through this map (`evalNode` reads `writer.runtimeState.value`
   * to emit `current`).
   *
   * Lives on the graph rather than per-scope because the lookup spans
   * scopes — a Read inside a region can match a Write in the root.
   *
   * Transient: rebuilt every tick so slot param edits take effect
   * immediately without recompile. Don't persist or serialise.
   *
   * Key shape is `${writeType}:${slot}` so the scalar Read/Write pair
   * (`state/write`) and the spread Read/Write pair
   * (`state/write-spread`) keep separate slot namespaces — a scalar
   * Read at slot 1 must not resolve to a spread writer at slot 1.
   */
  slotWriters?: Map<string, RuntimeNode>;
}

export interface RuntimeGraphInput {
  nodes: RuntimeNode[];
  edges: RuntimeEdge[];
  /** Either supply all renderer ids OR a single id via `rendererId`;
   *  the graph normalises one to the other so callers can pass whichever
   *  is convenient. */
  rendererIds?: NodeId[];
  rendererId?: NodeId | null;
}

export function buildRuntimeGraph(input: RuntimeGraphInput): RuntimeGraph {
  const nodes = new Map<NodeId, RuntimeNode>();
  for (const n of input.nodes) {
    if (nodes.has(n.id)) throw new Error(`duplicate node id ${n.id}`);
    nodes.set(n.id, n);
  }

  // Group nodes by scope.
  const byScope = new Map<ScopeKey, NodeId[]>();
  for (const n of nodes.values()) {
    const key: ScopeKey = n.parentId ?? null;
    let bucket = byScope.get(key);
    if (!bucket) {
      bucket = [];
      byScope.set(key, bucket);
    }
    bucket.push(n.id);
  }
  if (!byScope.has(null)) byScope.set(null, []);

  // Global edges-by-target — all real edges, regardless of scope.
  const edgesByTarget = new Map<NodeId, RuntimeEdge[]>();
  for (const e of input.edges) {
    if (!nodes.has(e.fromNode)) throw new Error(`edge from unknown node ${e.fromNode}`);
    if (!nodes.has(e.toNode)) throw new Error(`edge to unknown node ${e.toNode}`);
    let bucket = edgesByTarget.get(e.toNode);
    if (!bucket) {
      bucket = [];
      edgesByTarget.set(e.toNode, bucket);
    }
    bucket.push(e);
  }

  // Per-scope adjacency for topo. For each scope, intra-scope edges go in
  // directly. Cross-scope edges contribute a synthetic dep in the OUTER
  // scope (from outer source to the inner side's enclosing region) so that
  // the region runs after everything it reads from on the outside.
  const scopeAdj = new Map<ScopeKey, Map<NodeId, NodeId[]>>();
  const scopeInDeg = new Map<ScopeKey, Map<NodeId, number>>();
  for (const [key, ids] of byScope) {
    const adj = new Map<NodeId, NodeId[]>();
    const inDeg = new Map<NodeId, number>();
    for (const id of ids) {
      adj.set(id, []);
      inDeg.set(id, 0);
    }
    scopeAdj.set(key, adj);
    scopeInDeg.set(key, inDeg);
  }

  function ancestorInScope(startId: NodeId, targetScope: ScopeKey): NodeId | null {
    let cur: RuntimeNode | undefined = nodes.get(startId);
    while (cur) {
      const curScope: ScopeKey = cur.parentId ?? null;
      if (curScope === targetScope) return cur.id;
      if (cur.parentId === undefined) return null;
      cur = nodes.get(cur.parentId);
    }
    return null;
  }

  for (const e of input.edges) {
    const src = nodes.get(e.fromNode)!;
    const dst = nodes.get(e.toNode)!;
    const srcScope: ScopeKey = src.parentId ?? null;
    const dstScope: ScopeKey = dst.parentId ?? null;

    if (srcScope === dstScope) {
      // Edges targeting an accumulator's `write` (or `init`) port within
      // the region scope are NOT topo deps — that would create a cycle
      // with consumers of the accum's `read` output. The edge still
      // exists in `edgesByTarget` so runRegion can gather the write
      // value after each iteration.
      // accumulator-write.write is the per-iteration feedback into its
      // paired accumulator-read — exclude from topo so the inner-scope
      // chain Read → … → Write doesn't form a cycle.
      if (dst.splicerRole === 'accumulator-write') continue;
      // Same idea for stateful nodes: the edge feeding `next` is the
      // cross-frame feedback loop — it depends on `current`, so
      // including it in topo would create a cycle. tick() gathers it
      // in a post-eval pass. Most stateful nodes use port 1 (after
      // `init`); state/write uses port 0 — see statefulNextPort.
      if (dst.stateful && e.toPort === (dst.statefulNextPort ?? 1)) continue;
      const adj = scopeAdj.get(srcScope)!;
      const inDeg = scopeInDeg.get(srcScope)!;
      adj.get(src.id)!.push(dst.id);
      inDeg.set(dst.id, (inDeg.get(dst.id) ?? 0) + 1);
      continue;
    }

    // Cross-scope. Determine "which scope does the topo dep live in?"
    // The dep is: in the outer scope of the two, the outer source must
    // precede the inner side's enclosing region.
    const dstAncestor = ancestorInScope(dst.id, srcScope);
    const srcAncestor = ancestorInScope(src.id, dstScope);

    if (dstAncestor) {
      // src is in the outer scope; dst lives inside a region that's in src's scope.
      const adj = scopeAdj.get(srcScope)!;
      const inDeg = scopeInDeg.get(srcScope)!;
      // Avoid self-edges (region → itself).
      if (dstAncestor !== src.id && !adj.get(src.id)!.includes(dstAncestor)) {
        adj.get(src.id)!.push(dstAncestor);
        inDeg.set(dstAncestor, (inDeg.get(dstAncestor) ?? 0) + 1);
      }
    } else if (srcAncestor) {
      // src is inside a region that's in dst's scope; dst is in the outer.
      // (Splicer-out → outer). compile should have rewritten these so
      // src.id is the region itself in dst's scope — falls into the
      // intra-scope branch above. Reaching here is a compile-step bug.
      // Best effort: encode as a region → outer dep.
      const adj = scopeAdj.get(dstScope)!;
      const inDeg = scopeInDeg.get(dstScope)!;
      if (!adj.get(srcAncestor)!.includes(dst.id)) {
        adj.get(srcAncestor)!.push(dst.id);
        inDeg.set(dst.id, (inDeg.get(dst.id) ?? 0) + 1);
      }
    }
    // else: nodes in unrelated scopes (e.g. two distinct regions at the
    // same level) — the edge will still be honoured at gather time,
    // but topology between them is opaque.
  }

  // Topo-sort each scope.
  const scopes = new Map<ScopeKey, RuntimeScope>();
  for (const [key, ids] of byScope) {
    const adj = scopeAdj.get(key)!;
    const inDeg = scopeInDeg.get(key)!;
    const order: NodeId[] = [];
    const queue: NodeId[] = [];
    for (const id of ids) if ((inDeg.get(id) ?? 0) === 0) queue.push(id);
    while (queue.length) {
      const id = queue.shift()!;
      order.push(id);
      for (const next of adj.get(id) ?? []) {
        const d = (inDeg.get(next) ?? 0) - 1;
        inDeg.set(next, d);
        if (d === 0) queue.push(next);
      }
    }
    if (order.length !== ids.length) {
      const remaining = ids.filter((id) => !order.includes(id));
      throw new Error(`cycle in scope ${String(key)}: ${remaining.join(', ')}`);
    }

    const splicerInIds: NodeId[] = [];
    const splicerOutIds: NodeId[] = [];
    const accumulatorIds: NodeId[] = [];
    // Pair Read with Write by `accSlot` for the split-form accumulator.
    // The Read node id is used as the anchor in `accumulatorIds`; the
    // pair's Write id is recorded in `accumulatorPairs` for runRegion
    // to gather from + publish final to.
    let accumulatorPairs: Map<NodeId, { readId: NodeId; writeId: NodeId }> | undefined;
    if (key !== null) {
      const readsBySlot = new Map<number, NodeId>();
      const writesBySlot = new Map<number, NodeId>();
      for (const id of ids) {
        const n = nodes.get(id)!;
        if (n.splicerRole === 'splicer-in') splicerInIds.push(id);
        else if (n.splicerRole === 'splicer-out') splicerOutIds.push(id);
        else if (n.splicerRole === 'accumulator-read') readsBySlot.set(n.accSlot ?? 1, id);
        else if (n.splicerRole === 'accumulator-write') writesBySlot.set(n.accSlot ?? 1, id);
      }
      // Match by slot. Unmatched Reads / Writes are silently dropped from
      // pair list — compile surfaces a warning earlier when expected.
      for (const [slot, readId] of readsBySlot) {
        const writeId = writesBySlot.get(slot);
        if (writeId === undefined) continue;
        if (!accumulatorPairs) accumulatorPairs = new Map();
        accumulatorIds.push(readId);
        accumulatorPairs.set(readId, { readId, writeId });
      }
    }

    scopes.set(key, {
      parentId: key,
      order,
      splicerInIds,
      splicerOutIds,
      accumulatorIds,
      ...(accumulatorPairs ? { accumulatorPairs } : {}),
    });
  }

  const rendererIds = input.rendererIds
    ?? (input.rendererId ? [input.rendererId] : []);
  return {
    nodes,
    scopes,
    edgesByTarget,
    rendererId: rendererIds[0] ?? null,
    rendererIds,
  };
}
