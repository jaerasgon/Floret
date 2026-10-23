import type { FloretEdge, FloretNode } from '../editor/editor-state';
import { CURRENT_PATCH_VERSION, type PatchFile, type SerializedEdge, type SerializedNode } from './schema';
import { nodeRegistry } from '../nodes/registry';
import { getResolvedInputs } from '../nodes/types';
import { parseHandle } from '../editor/connection-rules';

/**
 * Editor state → PatchFile (JSON-friendly).
 *
 * Source of truth for serialised values is `node.data.params` — which is
 * what gets committed on pointerup / blur from IO boxes. The ParamStore
 * is intentionally NOT consulted here; if it contains a mid-drag value
 * that hasn't been committed yet, that's a UX bug we'll address in M7
 * (force-commit on save).
 */

export interface SerializeInput {
  nodes: ReadonlyArray<FloretNode>;
  edges: ReadonlyArray<FloretEdge>;
  name?: string;
}

export function serializePatch(input: SerializeInput): PatchFile {
  const now = new Date().toISOString();
  const nodes: SerializedNode[] = input.nodes.map((n) => {
    const out: SerializedNode = {
      id: n.id,
      defType: n.data.defType,
      position: { x: n.position.x, y: n.position.y },
      params: { ...n.data.params },
    };
    if (n.parentId) out.parentId = n.parentId;
    if (n.data.portValues && Object.keys(n.data.portValues).length > 0) {
      out.portValues = { ...n.data.portValues };
    }
    if (n.data.ports && n.data.ports.length > 0) {
      out.ports = n.data.ports.map((p) => ({
        id: p.id, name: p.name, type: p.type, direction: p.direction,
      }));
    }
    const w = (n.style?.width ?? n.width) as number | undefined;
    const h = (n.style?.height ?? n.height) as number | undefined;
    if (typeof w === 'number') out.width = w;
    if (typeof h === 'number') out.height = h;
    return out;
  });
  const edges: SerializedEdge[] = input.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: e.sourceHandle ?? null,
    targetHandle: e.targetHandle ?? null,
  }));
  return {
    kind: 'floret.patch',
    version: CURRENT_PATCH_VERSION,
    appVersion: '0.0.1',
    meta: {
      name: input.name,
      createdAt: now,
      modifiedAt: now,
    },
    nodes,
    edges,
  };
}

/**
 * PatchFile → editor state (rehydrate-ready). Pure (no side effects);
 * the editor store's loadPatch action mounts the result and seeds the
 * ParamStore.
 */
export function deserializePatch(patch: PatchFile): { nodes: FloretNode[]; edges: FloretEdge[] } {
  const nodes: FloretNode[] = patch.nodes.map((n) => {
    const out: FloretNode = {
      id: n.id,
      type: 'floret',
      position: { ...n.position },
      data: {
        defType: n.defType,
        params: { ...n.params },
      },
    };
    if (n.parentId) {
      out.parentId = n.parentId;
      out.extent = 'parent';
    }
    if (n.portValues) out.data.portValues = { ...n.portValues };
    if (n.ports) {
      out.data.ports = n.ports.map((p) => ({
        id: p.id, name: p.name,
        type: p.type as import('../runtime/types').PortType,
        direction: p.direction,
      }));
    }
    if (typeof n.width === 'number' || typeof n.height === 'number') {
      out.style = {
        ...(typeof n.width === 'number' ? { width: n.width } : {}),
        ...(typeof n.height === 'number' ? { height: n.height } : {}),
      };
    }
    return out;
  });
  const edges: FloretEdge[] = patch.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: e.sourceHandle,
    targetHandle: e.targetHandle,
  }));
  // Handle renames first, then the structural draw x/y → pos (vec2) fix
  // (which adds vec2/make nodes), then prune anything still dangling.
  const renamed = migrateLegacyEdges(nodes, edges);
  const coords = migrateLegacyDrawCoords(nodes, renamed);
  return { nodes: coords.nodes, edges: pruneDeadEdges(coords.nodes, coords.edges) };
}

/**
 * Drop edges whose handle no longer resolves to a real port — the
 * invisible-dead-wire case. After a port rename (e.g. a node's output
 * `out` → `grouped`, or an input `x`/`y` → `pos`), old saved patches keep
 * the stale edge. React Flow won't render it (so the user can't select or
 * delete it), but the runtime still tries to compile it and surfaces an
 * "Unknown port" warning as an error chip. Pruning on load clears those.
 *
 * Conservative on purpose: only drops when we're CERTAIN the port is gone.
 * Region nodes (dynamic ports) and nodes with `dynamicOutputs` are left
 * untouched, since their port set can't be resolved from static defs here.
 */
export function pruneDeadEdges(
  nodes: ReadonlyArray<FloretNode>,
  edges: ReadonlyArray<FloretEdge>,
): FloretEdge[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));

  const portExists = (
    nodeId: string,
    handle: string | null | undefined,
    side: 'out' | 'in',
  ): boolean => {
    const n = byId.get(nodeId);
    if (!n) return true; // missing node — compile reports it separately
    const def = nodeRegistry.get(n.data.defType);
    if (!def) return true; // unknown def — don't touch
    if (def.regionKind) return true; // regions carry dynamic ports
    const parsed = parseHandle(handle);
    if (!parsed) return true;
    if (side === 'out') {
      if (def.dynamicOutputs) return true; // can't resolve statically
      return def.outputs.some((p) => p.name === parsed.name);
    }
    // `getResolvedInputs` already expands dynamic inputs (e.g. group slots).
    return getResolvedInputs(def, n.data.params ?? {}).some((p) => p.name === parsed.name);
  };

  return edges.filter(
    (e) => portExists(e.source, e.sourceHandle, 'out') && portExists(e.target, e.targetHandle, 'in'),
  );
}

/** Explicit input-port renames, keyed by defType: `{ oldName: newName }`.
 *  For pure renames only (same shape) — structural changes (e.g. draw nodes'
 *  x/y numbers → a single `pos` vec2) can't be auto-migrated and live in the
 *  sample/build sources instead. */
const INPUT_RENAMES: Record<string, Record<string, string>> = {
  'spread/randomize': { value: 'source' },
  'spread/repeat': { value: 'source' },
};

/**
 * Per-load handle migration for legacy port names. Saved patches and samples
 * authored before the great pin-rename reference outdated handles; React Flow
 * silently drops edges with unknown handles, so we remap them here. Covers:
 *
 *   - **Outputs** — the `out → semantic name` sweep (math `sum`/`product`/… →
 *     `result`, draw `layer` → `shape`, style `out` → `style`, etc.). Any
 *     source handle that doesn't match a CURRENT output AND whose node has
 *     exactly one output is remapped to that output. (Every renamed output is
 *     on a single-output node.)
 *   - **Inputs** — explicit per-type renames (see INPUT_RENAMES).
 *
 * Pure — returns a new edge list. Safe to call multiple times (idempotent:
 * once a handle matches the current port it's left alone).
 */
export function migrateLegacyEdges(
  nodes: ReadonlyArray<FloretNode>,
  edges: ReadonlyArray<FloretEdge>,
): FloretEdge[] {
  const defTypeById = new Map<string, string>();
  for (const n of nodes) {
    const t = (n.data as { defType?: string } | undefined)?.defType;
    if (t) defTypeById.set(n.id, t);
  }
  const defFor = (id: string) => {
    const t = defTypeById.get(id);
    return t ? nodeRegistry.get(t) : undefined;
  };

  return edges.map((e) => {
    let sourceHandle = e.sourceHandle;
    let targetHandle = e.targetHandle;

    // ── Output remap ──
    const src = parseHandle(sourceHandle);
    if (src && src.kind === 'out') {
      const def = defFor(e.source);
      // Region outputs are dynamic (frame ports) — never remap those.
      if (def && !def.regionKind && def.outputs.length > 0) {
        const known = def.outputs.some((p) => p.name === src.name);
        if (!known && def.outputs.length === 1) {
          sourceHandle = `out-${def.outputs[0]!.name}`;
        }
      }
    }

    // ── Input remap (explicit renames) ──
    const dst = parseHandle(targetHandle);
    if (dst && dst.kind === 'in') {
      const t = defTypeById.get(e.target);
      const rename = t ? INPUT_RENAMES[t]?.[dst.name] : undefined;
      if (rename) targetHandle = `in-${rename}`;
    }

    return sourceHandle === e.sourceHandle && targetHandle === e.targetHandle
      ? e
      : { ...e, sourceHandle, targetHandle };
  });
}

/**
 * Legacy draw-coordinate migration. Draw nodes used to take separate number
 * `x`/`y` inputs; they now take a single `pos` vec2 (and Line takes `a`/`b`
 * vec2 instead of `x1,y1`/`x2,y2`). This can't be a handle rename — two number
 * inputs collapse into one vec2 — so for any node still wired (or carrying
 * constant portValues) on the legacy coord inputs we synthesise a `vec2/make`
 * that feeds the new vec2 input.
 *
 * Returns a new {nodes, edges}; safe to run multiple times (a node already on
 * the vec2 input with no legacy coord signal is left alone).
 */
const LEGACY_COORD_GROUPS: Record<string, ReadonlyArray<{ out: string; x: string; y: string }>> = {
  'draw/rect': [{ out: 'pos', x: 'x', y: 'y' }],
  'draw/ellipse': [{ out: 'pos', x: 'x', y: 'y' }],
  'draw/image': [{ out: 'pos', x: 'x', y: 'y' }],
  'draw/text': [{ out: 'pos', x: 'x', y: 'y' }],
  'draw/walker-cluster': [{ out: 'pos', x: 'x', y: 'y' }],
  'draw/line': [{ out: 'a', x: 'x1', y: 'y1' }, { out: 'b', x: 'x2', y: 'y2' }],
};

export function migrateLegacyDrawCoords(
  nodes: ReadonlyArray<FloretNode>,
  edges: ReadonlyArray<FloretEdge>,
): { nodes: FloretNode[]; edges: FloretEdge[] } {
  const outNodes: FloretNode[] = [...nodes];
  const outEdges: FloretEdge[] = edges.map((e) => ({ ...e }));
  let seq = 0;

  const constCoord = (n: FloretNode, key: string): number | undefined => {
    const pv = (n.data as { portValues?: Record<string, unknown> }).portValues;
    if (pv && typeof pv[key] === 'number') return pv[key] as number;
    const p = n.data.params as Record<string, unknown> | undefined;
    if (p && typeof p[key] === 'number') return p[key] as number;
    return undefined;
  };

  for (const n of nodes) {
    const groups = LEGACY_COORD_GROUPS[(n.data as { defType?: string }).defType ?? ''];
    if (!groups) continue;
    for (const g of groups) {
      const posWired = outEdges.some((e) => e.target === n.id && e.targetHandle === `in-${g.out}`);
      const xi = outEdges.findIndex((e) => e.target === n.id && e.targetHandle === `in-${g.x}`);
      const yi = outEdges.findIndex((e) => e.target === n.id && e.targetHandle === `in-${g.y}`);
      const cx = constCoord(n, g.x);
      const cy = constCoord(n, g.y);
      // Nothing legacy to migrate, or the new vec2 input is already wired.
      if (posWired || (xi < 0 && yi < 0 && cx === undefined && cy === undefined)) continue;

      const makeId = `${n.id}-xy${seq++}`;
      const cxv = cx ?? 0;
      const cyv = cy ?? 0;
      outNodes.push({
        id: makeId,
        type: 'floret',
        position: { x: (n.position?.x ?? 0) - 60, y: n.position?.y ?? 0 },
        data: {
          defType: 'vec2/make',
          params: { x: cxv, y: cyv },
          portValues: { x: cxv, y: cyv },
        },
        ...(n.parentId ? { parentId: n.parentId, extent: 'parent' as const } : {}),
      } as FloretNode);

      if (xi >= 0) outEdges[xi] = { ...outEdges[xi]!, target: makeId, targetHandle: 'in-x' };
      if (yi >= 0) outEdges[yi] = { ...outEdges[yi]!, target: makeId, targetHandle: 'in-y' };
      outEdges.push({
        id: `e_xy_${makeId}`,
        source: makeId,
        sourceHandle: 'out-vector',
        target: n.id,
        targetHandle: `in-${g.out}`,
      });
    }
  }
  return { nodes: outNodes, edges: outEdges };
}
