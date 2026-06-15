import type { Connection, Edge, Node } from '@xyflow/react';
import { nodeRegistry } from '../nodes/registry';
import { getResolvedInputs } from '../nodes/types';
import { parseLoopVarHandle } from '../runtime/compile';
import type { PortType } from '../runtime/types';
import type { FloretNodeData, RegionPortDescriptor } from './editor-state';

type PortKind = 'single' | 'spread' | 'multi';

/**
 * Strict port types with narrow coercions:
 *   number ↔ boolean   (0/1 coercion)
 *   any    ↔ anything  (convert/* nodes use this on their input)
 * Used by both editor `isValidConnection` and the runtime compile pass.
 */
export function arePortsCompatible(a: PortType, b: PortType): boolean {
  if (a === b) return true;
  if (a === 'any' || b === 'any') return true;
  if ((a === 'number' && b === 'boolean') || (a === 'boolean' && b === 'number')) return true;
  return false;
}

/**
 * kind-compatibility (source kind → target kind):
 *   single → single   ✓
 *   spread → spread   ✓
 *   single → spread   ✓ (auto-wraps as a length-1 spread at the wire)
 *   single → multi    ✓ (one value added to the gathered spread)
 *   spread → multi    ✓ (spread elements concatenated)
 *   spread → single   ✗ (must cross a region boundary via a splicer)
 *   anything → multi via N wires: always ✓ (the runtime appends each
 *                                            wire's value to the gather)
 */
export function areKindsCompatible(from: PortKind, to: PortKind): boolean {
  if (from === to) return true;
  if (from === 'single' && to === 'spread') return true;
  if (to === 'multi' && (from === 'single' || from === 'spread')) return true;
  return false;
}

/**
 * True when `to` accepts MULTIPLE incoming wires on the same handle.
 * The editor's onConnect / addEdge use this to skip the "drop existing
 * edge on same target handle" rule for multi inputs.
 */
export function isMultiTargetPort(
  nodeId: string,
  targetHandle: string | null | undefined,
  getDefType: NodeTypeLookup,
  getRegionPorts: RegionPortLookup,
  getParams: NodeParamsLookup,
): boolean {
  const parsed = parseHandle(targetHandle);
  if (!parsed || parsed.kind !== 'in') return false;
  const port = resolvePort(nodeId, 'in', parsed.name, getDefType, getRegionPorts, getParams);
  return port?.kind === 'multi';
}

/**
 * Handle-id convention:
 *   Inputs:  `in-<name>`
 *   Outputs: `out-<name>`
 * The portion after the dash is the port `name` from the NodeDef.
 */
export function parseHandle(id: string | null | undefined): { kind: 'in' | 'out'; name: string } | null {
  if (!id) return null;
  if (id.startsWith('in-')) return { kind: 'in', name: id.slice(3) };
  if (id.startsWith('out-')) return { kind: 'out', name: id.slice(4) };
  return null;
}

export interface NodeTypeLookup {
  (nodeId: string): string | undefined;
}

export interface RegionPortLookup {
  (nodeId: string): RegionPortDescriptor[] | undefined;
}

export interface NodeParamsLookup {
  (nodeId: string): Record<string, unknown> | undefined;
}

export interface EdgesLookup {
  (): ReadonlyArray<Edge>;
}

/**
 * Edges into a stateful node's `next` input are cross-frame — the value
 * is captured at end-of-tick and emitted as `current` on the NEXT tick,
 * so the feedback doesn't form a within-frame cycle. Mirrors the
 * runtime's topo-sort exemption in graph.ts.
 */
function isStatefulNextHandle(
  nodeId: string,
  handleId: string | null | undefined,
  getDefType: NodeTypeLookup,
): boolean {
  const defType = getDefType(nodeId);
  if (!defType) return false;
  const def = nodeRegistry.get(defType);
  if (!def?.stateful) return false;
  const parsed = parseHandle(handleId);
  return parsed?.kind === 'in' && parsed.name === 'next';
}

/**
 * Returns true if adding `source → target` would create a directed cycle
 * in the editor's edge graph. Nodes (regions included) count as a single
 * vertex — so a region's pass-through self-edge (out-X → in-Y on the
 * same region) is NOT a cycle on its own, but adding A → region when
 * region → … → A already exists IS.
 *
 * Stateful nodes' `next` edges are excluded from the graph (they're
 * cross-frame, not within-frame), so feedback loops through `state/*`
 * nodes don't trip the check.
 *
 * Algorithm: BFS from `target`, follow existing edges, and look for
 * `source`. O(V + E) per check; runs once per drag attempt, fine for
 * patches up to thousands of nodes.
 */
export function wouldCreateCycle(
  source: string,
  target: string,
  targetHandle: string | null | undefined,
  edges: ReadonlyArray<Edge>,
  getDefType: NodeTypeLookup,
  sourceHandle?: string | null | undefined,
): boolean {
  if (source === target) return false; // self-edges are not cycles on their own
  // The proposed edge itself, if it lands on a stateful `next`, doesn't
  // count as part of any within-frame cycle.
  if (isStatefulNextHandle(target, targetHandle, getDefType)) return false;
  // Region ports live in TWO scopes — each frame port is rewritten at
  // compile time into its own splicer-in / splicer-out node. The cycle
  // check has to mirror that, otherwise a legitimate
  //   region.out-X → … → inner-body → region.in-Y   (X is an input port,
  //                                                  Y is an output port)
  // looks like a self-loop through the region, even though the runtime
  // sees those as two unrelated splicer nodes. Virtualising each
  // region's port-name into its own graph key separates the splicer
  // mechanics, so only edges that actually share a splicer node count
  // as connected here.
  const virt = (nodeId: string, handle: string | null | undefined): string => {
    const t = getDefType(nodeId);
    if (!t) return nodeId;
    const def = nodeRegistry.get(t);
    if (!def?.regionKind) return nodeId;
    const p = parseHandle(handle);
    return p ? `${nodeId}#${p.name}` : nodeId;
  };
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (isStatefulNextHandle(e.target, e.targetHandle, getDefType)) continue;
    const vs = virt(e.source, e.sourceHandle);
    const vt = virt(e.target, e.targetHandle);
    let list = adj.get(vs);
    if (!list) { list = []; adj.set(vs, list); }
    list.push(vt);
  }
  const startV = virt(target, targetHandle);
  const goalV = virt(source, sourceHandle);
  const visited = new Set<string>();
  const stack: string[] = [startV];
  while (stack.length) {
    const cur = stack.pop()!;
    if (cur === goalV) return true;
    if (visited.has(cur)) continue;
    visited.add(cur);
    const next = adj.get(cur);
    if (next) for (const n of next) stack.push(n);
  }
  return false;
}

interface ResolvedPort {
  type: PortType;
  kind: PortKind;
}

/**
 * Resolve a handle reference to a {type, kind} pair, honoring both
 * NodeDef static ports AND dynamic ports (region frame ports and
 * NodeDef.dynamicInputs). Returns null if the handle can't be matched.
 */
function resolvePort(
  nodeId: string,
  side: 'in' | 'out',
  handleName: string,
  getDefType: NodeTypeLookup,
  getRegionPorts: RegionPortLookup,
  getParams: NodeParamsLookup,
): ResolvedPort | null {
  const defType = getDefType(nodeId);
  if (!defType) return null;
  const def = nodeRegistry.get(defType);
  if (!def) return null;

  if (def.regionKind) {
    // Built-in loop variables (ForEach/Cache) — number-typed inner sources.
    if ((def.regionKind === 'foreach' || def.regionKind === 'cache') &&
        side === 'out' && parseLoopVarHandle(handleName)) {
      return { type: 'number', kind: 'single' };
    }
    const dynamic = getRegionPorts(nodeId);
    if (dynamic) {
      const dp = dynamic.find((p) => p.id === handleName);
      if (dp) {
        const kind: PortKind =
          dp.direction === 'input'
            ? (side === 'in' ? 'spread' : 'single')
            : (side === 'in' ? 'single' : 'spread');
        return { type: dp.type, kind };
      }
    }
    // Fall through to static inputs (e.g. If.condition).
  }

  const portList = side === 'in'
    ? getResolvedInputs(def, getParams(nodeId) ?? {})
    : def.outputs;
  const found = portList.find((p) => p.name === handleName);
  if (!found) return null;
  return { type: found.type, kind: found.kind ?? 'single' };
}

/**
 * Build an isValidConnection callback for React Flow.
 */
export function buildIsValidConnection(
  getDefType: NodeTypeLookup,
  getRegionPorts: RegionPortLookup,
  getParams: NodeParamsLookup,
  getEdges: EdgesLookup,
) {
  return (conn: Connection): boolean => {
    if (!conn.source || !conn.target) return false;

    const srcHandle = parseHandle(conn.sourceHandle);
    const dstHandle = parseHandle(conn.targetHandle);
    if (!srcHandle || srcHandle.kind !== 'out') return false;
    if (!dstHandle || dstHandle.kind !== 'in') return false;

    if (conn.source === conn.target) {
      // Self-wires are only legal on regions (frame-port to frame-port),
      // and the two ports must be different.
      const defType = getDefType(conn.source);
      const def = defType ? nodeRegistry.get(defType) : undefined;
      if (!def?.regionKind) return false;
      if (srcHandle.name === dstHandle.name) return false;
    }

    const srcPort = resolvePort(conn.source, 'out', srcHandle.name, getDefType, getRegionPorts, getParams);
    const dstPort = resolvePort(conn.target, 'in', dstHandle.name, getDefType, getRegionPorts, getParams);
    if (!srcPort || !dstPort) return false;

    if (!arePortsCompatible(srcPort.type, dstPort.type)) return false;
    if (!areKindsCompatible(srcPort.kind, dstPort.kind)) return false;

    // No cycles. Strictly left-to-right dataflow, regions included.
    if (wouldCreateCycle(conn.source, conn.target, conn.targetHandle, getEdges(), getDefType, conn.sourceHandle)) return false;

    return true;
  };
}

/**
 * Mini-map accent colour per category. Mirrors the `--cat-accent` CSS
 * tokens in styles.css — keep these two in sync when adding categories.
 */
export function categoryColor(category: string | undefined): string {
  switch ((category ?? '').toLowerCase()) {
    case 'math':       return '#c4a85f';
    case 'time':       return '#d5a566';
    case 'draw':       return '#5fa6ff';
    case 'style':      return '#c97bd5';
    case 'transform':  return '#a880ff';
    case 'value':      return '#6cb6ff';
    case 'render':     return '#ff9a4a';
    case 'io':         return '#d4a574';
    case 'input':      return '#ff8a6c';
    case 'spread':     return '#e5d05f';
    case 'region':     return '#5fd5c4';
    case 'logic':      return '#ff7fb5';
    case 'random':     return '#7d92ff';
    case 'vec2':       return '#4ed8d8';
    case 'color':      return '#ff6b6b';
    case 'convert':    return '#9aa4b8';
    case 'meta':       return '#f0d896';
    case 'sound':      return '#ff9cd9';
    default:           return '#6b7080';
  }
}

/**
 * Build the `upstreamTypes` map a node's `dynamicOutputs` hook expects.
 * Walks the edge list, finds each incoming wire targeting `nodeId`, and
 * resolves the source port's PortType from the source node's registry
 * def. Region nodes carry their port type on the node data instead.
 *
 * Returned map is keyed by INPUT port name on the target node. Missing
 * keys = no wire connected.
 */
export function getUpstreamTypes(
  nodeId: string,
  edges: ReadonlyArray<Edge>,
  nodes: ReadonlyArray<Node>,
): Record<string, PortType | undefined> {
  const out: Record<string, PortType | undefined> = {};
  for (const e of edges) {
    if (e.target !== nodeId) continue;
    const targetParsed = parseHandle(e.targetHandle);
    if (!targetParsed || targetParsed.kind !== 'in') continue;
    const srcNode = nodes.find((n) => n.id === e.source);
    if (!srcNode) continue;
    const srcParsed = parseHandle(e.sourceHandle);
    if (!srcParsed) continue;
    const data = srcNode.data as FloretNodeData | undefined;
    const def = data ? nodeRegistry.get(data.defType) : undefined;
    // Region/subpatch nodes carry runtime-derived ports on data.ports.
    if (def?.regionKind) {
      // Built-in loop-var pins are always number-typed.
      if (parseLoopVarHandle(srcParsed.name)) {
        out[targetParsed.name] = 'number';
        continue;
      }
      const port = data?.ports?.find((p) => p.id === srcParsed.name);
      if (port?.type) out[targetParsed.name] = port.type as PortType;
      continue;
    }
    const port = def?.outputs.find((p) => p.name === srcParsed.name);
    if (port) out[targetParsed.name] = port.type;
  }
  return out;
}

/** Visual colour per port type, sourced from CSS vars in styles.css. */
export function portColorVar(t: PortType): string {
  switch (t) {
    case 'number':  return 'var(--port-number)';
    case 'boolean': return 'var(--port-bool)';
    case 'vec2':    return 'var(--port-vec2)';
    case 'color':   return 'var(--port-color)';
    case 'layer':   return 'var(--port-layer)';
    case 'mat3':    return 'var(--port-vec2)';
    case 'string':  return 'var(--fg-1)';
    case 'style':   return 'var(--port-style)';
    case 'audio':   return 'var(--port-audio)';
    case 'image':   return 'var(--port-image)';
    case 'world':   return 'var(--port-world)';
    case 'force':   return 'var(--port-force)';
    case 'collider':return 'var(--port-collider)';
    case 'particle':return 'var(--port-particle)';
    case 'any':     return 'var(--port-any)';
    default:        return 'var(--fg-1)';
  }
}
