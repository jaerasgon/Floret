import { nodeRegistry } from '../../nodes/registry';
import type { PortType } from '../../runtime/types';
import { paramStore } from '../../runtime/param-store';
import { parseHandle } from '../connection-rules';
import type { EditorGet, EditorSet, EditorState, FloretNode, RegionPortDescriptor } from './types';
import { newId, sortNodesParentFirst } from './util';
import { pushHistory } from './history-bridge';

type RegionPortSlice = Pick<
  EditorState,
  'addRegion' | 'addRegionPort' | 'removeRegionPort' | 'renameRegionPort' | 'moveRegionPort' | 'setRegionPortY'
>;

/**
 * Return a port name that doesn't collide with any existing port on the
 * same region. Starts from the type-derived base name and adds a
 * numeric suffix until it's unique.
 */
function uniquePortName(
  existing: ReadonlyArray<RegionPortDescriptor>,
  type: PortType,
  _direction: 'input' | 'output',
): string {
  const base = type;
  const taken = new Set(existing.map((p) => p.name));
  if (!taken.has(base)) return base;
  let i = 2;
  while (taken.has(`${base}${i}`)) i += 1;
  return `${base}${i}`;
}

/** Region creation + the dynamic frame-port editing actions. */
export function createRegionPortSlice(
  set: EditorSet,
  _get: EditorGet,
  deps: { compile: () => void },
): RegionPortSlice {
  const { compile } = deps;
  return {
    addRegion: (kind, position, options) => {
      const id = newId(`region_${kind}`);
      const regionDefType = `region/${kind}`;
      const regionDef = nodeRegistry.require(regionDefType);
      const regionParams = { ...(regionDef.defaultParams ?? {}) };
      const w = options?.width ?? (regionParams.width as number) ?? 480;
      const h = options?.height ?? (regionParams.height as number) ?? 280;
      regionParams.width = w;
      regionParams.height = h;
      const region: FloretNode = {
        id,
        type: 'floret',
        position,
        data: { defType: regionDefType, params: regionParams, ports: [] },
        style: { width: w, height: h },
      };
      paramStore.setNode(id, regionParams);

      set((s) => {
        let nodes = [...s.nodes, region];

        // Auto-reparent: any root node whose centre is inside the new
        // region's bounds becomes a child of it. Used by draw-mode
        // creation so the user can lasso existing nodes into a region.
        if (options?.autoReparent) {
          nodes = nodes.map((n) => {
            if (n.id === id || n.parentId) return n;
            const def = nodeRegistry.get(n.data.defType);
            if (def?.regionKind) return n; // don't nest regions
            const nw = (n.style?.width as number | undefined) ?? n.width ?? 160;
            const nh = (n.style?.height as number | undefined) ?? n.height ?? 60;
            const cx = n.position.x + nw / 2;
            const cy = n.position.y + nh / 2;
            if (cx < position.x || cx > position.x + w) return n;
            if (cy < position.y || cy > position.y + h) return n;
            return {
              ...n,
              parentId: id,
              extent: 'parent' as const,
              position: {
                x: n.position.x - position.x,
                y: n.position.y - position.y,
              },
            };
          });
          nodes = sortNodesParentFirst(nodes);
        }

        return { nodes };
      });
      compile();
      pushHistory();
      return id;
    },

    addRegionPort: (regionId, spec) => {
      const portId = `p_${newId('rp')}`;
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = n.data.ports ?? [];
          const name = spec.name ?? uniquePortName(ports, spec.type, spec.direction);
          const newPort: RegionPortDescriptor = { id: portId, name, type: spec.type, direction: spec.direction };
          if (spec.y !== undefined) newPort.y = spec.y;
          return {
            ...n,
            data: {
              ...n.data,
              ports: [...ports, newPort],
            },
          };
        }),
      }));
      compile();
      pushHistory();
      return portId;
    },

    removeRegionPort: (regionId, portId) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = (n.data.ports ?? []).filter((p) => p.id !== portId);
          return { ...n, data: { ...n.data, ports } };
        }),
        edges: s.edges.filter((e) => {
          if (e.source === regionId) {
            const parsed = parseHandle(e.sourceHandle);
            if (parsed?.name === portId) return false;
          }
          if (e.target === regionId) {
            const parsed = parseHandle(e.targetHandle);
            if (parsed?.name === portId) return false;
          }
          return true;
        }),
      }));
      compile();
      pushHistory();
    },

    renameRegionPort: (regionId, portId, name) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = (n.data.ports ?? []).map((p) =>
            p.id === portId ? { ...p, name } : p,
          );
          return { ...n, data: { ...n.data, ports } };
        }),
      }));
      pushHistory();
    },

    moveRegionPort: (regionId, portId, delta) => {
      if (delta === 0) return;
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const all = n.data.ports ?? [];
          const target = all.find((p) => p.id === portId);
          if (!target) return n;
          const dir = target.direction;
          const sameDir = all.filter((p) => p.direction === dir);
          const subsetIdx = sameDir.findIndex((p) => p.id === portId);
          const newSubsetIdx = Math.max(0, Math.min(sameDir.length - 1, subsetIdx + delta));
          if (newSubsetIdx === subsetIdx) return n;
          // Reorder within the same-direction subset; thread the result
          // back into the global array, preserving non-matching slots.
          const reordered = [...sameDir];
          const [moved] = reordered.splice(subsetIdx, 1);
          reordered.splice(newSubsetIdx, 0, moved!);
          let cur = 0;
          const next = all.map((p) => (p.direction === dir ? reordered[cur++]! : p));
          return { ...n, data: { ...n.data, ports: next } };
        }),
      }));
      compile();
      pushHistory();
    },

    setRegionPortY: (regionId, portId, y) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = (n.data.ports ?? []).map((p) =>
            p.id === portId ? { ...p, ...(y === undefined ? { y: undefined } : { y }) } : p,
          );
          return { ...n, data: { ...n.data, ports } };
        }),
      }));
      // Drag-driven calls fire on every mousemove; skip pushHistory to
      // avoid a 60Hz undo stack. The drag-end pushHistory in the
      // dragger's mouseup handler covers the final state.
    },
  };
}
