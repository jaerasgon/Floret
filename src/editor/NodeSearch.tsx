import { useEffect, useMemo, useRef, useState } from 'react';
import { nodeRegistry } from '../nodes/registry';
import { getResolvedInputs, type NodeDef } from '../nodes/types';
import { useOverlay } from './overlay-store';
import { useEditor, type FloretNode } from './editor-state';
import { arePortsCompatible } from './connection-rules';
import type { PortType } from '../runtime/types';

/**
 * Locate the smallest region whose bounds contain the given flow-coord
 * point and whose parent matches the current view scope. Used to
 * auto-parent newly-created nodes into the region the user dropped them in.
 */
function findRegionAtFlow(
  nodes: ReadonlyArray<FloretNode>,
  flowPoint: { x: number; y: number },
  newNodeId: string,
  scopeParent: string | null,
): { id: string; pos: { x: number; y: number } } | null {
  let best: { id: string; pos: { x: number; y: number } } | null = null;
  let bestArea = Infinity;
  for (const n of nodes) {
    if (n.id === newNodeId) continue;
    if ((n.parentId ?? null) !== scopeParent) continue;
    const def = nodeRegistry.get(n.data.defType);
    if (!def?.regionKind) continue;
    const w = (n.data.params?.width as number | undefined) ?? 480;
    const h = (n.data.params?.height as number | undefined) ?? 280;
    if (
      flowPoint.x >= n.position.x && flowPoint.x <= n.position.x + w &&
      flowPoint.y >= n.position.y && flowPoint.y <= n.position.y + h
    ) {
      const area = w * h;
      if (area < bestArea) {
        best = { id: n.id, pos: n.position };
        bestArea = area;
      }
    }
  }
  return best;
}

/**
 * Command-palette-style node picker. Opens at a screen position; when an
 * item is selected, adds the node at the captured flow position (and
 * creates the auto-edge if it was opened from a wire drop).
 *
 * Keyboard: ↑/↓ navigate, Enter to commit, Esc to dismiss.
 */
export function NodeSearch() {
  const open = useOverlay((s) => s.searchOpen);
  const screenPos = useOverlay((s) => s.searchScreenPosition);
  const flowPos = useOverlay((s) => s.searchFlowPosition);
  const connection = useOverlay((s) => s.searchConnection);
  const insertion = useOverlay((s) => s.searchInsertion);
  const close = useOverlay((s) => s.closeSearch);

  const addNode = useEditor((s) => s.addNode);
  const addEdge = useEditor((s) => s.addEdge);
  const removeEdge = useEditor((s) => s.removeEdge);
  const setNodeParent = useEditor((s) => s.setNodeParent);

  const [query, setQuery] = useState('');
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  // Reset transient state on (re-)open and focus the input.
  useEffect(() => {
    if (open) {
      setQuery('');
      setHighlight(0);
      // Defer so the input is mounted before .focus().
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Close on outside click. Capture phase so we beat React Flow's own handlers.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const popover = document.getElementById('floret-node-search');
      if (popover && !popover.contains(e.target as Node)) close();
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  }, [open, close]);

  /** Filter + connection-compatibility test. */
  const results = useMemo<NodeDef[]>(() => {
    if (!open) return [];
    const all = nodeRegistry.all().filter((d) => !d.hidden);
    const compat = (def: NodeDef): boolean => {
      if (insertion) {
        // Insert-into-edge: need BOTH a compatible input (to receive
        // the upstream value) AND a compatible output (to feed the
        // original downstream target).
        const ins = getResolvedInputs(def, def.defaultParams ?? {});
        const hasIn = ins.some((p) =>
          arePortsCompatible(insertion.sourcePortType as PortType, p.type),
        );
        const hasOut = def.outputs.some((p) =>
          arePortsCompatible(p.type, insertion.targetPortType as PortType),
        );
        return hasIn && hasOut;
      }
      if (!connection) return true;
      const candidatePorts =
        connection.handleType === 'source' ? def.inputs : def.outputs;
      return candidatePorts.some((p) =>
        arePortsCompatible(connection.portType as never, p.type),
      );
    };
    const q = query.trim().toLowerCase();
    const matches = all.filter((d) => {
      if (!compat(d)) return false;
      if (!q) return true;
      return (
        d.label.toLowerCase().includes(q) ||
        d.type.toLowerCase().includes(q) ||
        d.category.toLowerCase().includes(q) ||
        (d.description?.toLowerCase().includes(q) ?? false)
      );
    });
    // Sort: prefix matches first, then includes, then alphabetic.
    matches.sort((a, b) => {
      if (q) {
        const ap = a.label.toLowerCase().startsWith(q) ? 0 : 1;
        const bp = b.label.toLowerCase().startsWith(q) ? 0 : 1;
        if (ap !== bp) return ap - bp;
      }
      return a.label.localeCompare(b.label);
    });
    return matches.slice(0, 40);
  }, [open, query, connection, insertion]);

  // Keep highlight in bounds when results change.
  useEffect(() => {
    if (highlight >= results.length) setHighlight(0);
  }, [results.length, highlight]);

  // Scroll highlight into view.
  useEffect(() => {
    if (!open) return;
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-idx="${highlight}"]`,
    );
    el?.scrollIntoView({ block: 'nearest' });
  }, [highlight, open]);

  if (!open) return null;

  const commit = (def: NodeDef) => {
    const id = addNode(def.type, flowPos);

    // Auto-parent the new node into a containing region (if any).
    // addNode already handles the subpatch case via viewStack; this
    // covers regions that live at the current view's scope.
    const st = useEditor.getState();
    const created = st.nodes.find((n) => n.id === id);
    const currentView = st.viewStack[st.viewStack.length - 1] ?? null;
    const newParentScope = created?.parentId ?? null;
    // Only auto-parent when the new node is at the current view scope
    // (i.e. addNode didn't already parent it elsewhere, e.g. inside a
    // subpatch — that's already handled by viewStack).
    if (created && newParentScope === currentView) {
      const region = findRegionAtFlow(st.nodes, flowPos, id, currentView);
      if (region && region.id !== currentView) {
        setNodeParent(id, region.id, {
          x: flowPos.x - region.pos.x,
          y: flowPos.y - region.pos.y,
        });
      }
    }

    if (insertion) {
      // Split the original edge: source → new node → original target.
      const ins = getResolvedInputs(def, def.defaultParams ?? {});
      const inPort = ins.find((p) =>
        arePortsCompatible(insertion.sourcePortType as PortType, p.type),
      );
      const outPort = def.outputs.find((p) =>
        arePortsCompatible(p.type, insertion.targetPortType as PortType),
      );
      if (inPort && outPort) {
        removeEdge(insertion.edgeId);
        addEdge(insertion.source, insertion.sourceHandle, id, `in-${inPort.name}`);
        addEdge(id, `out-${outPort.name}`, insertion.target, insertion.targetHandle);
      }
      close();
      return;
    }

    // If we opened from a dragged wire, auto-create the edge to the first
    // type-compatible port on the new node.
    if (connection) {
      const candidatePorts =
        connection.handleType === 'source' ? def.inputs : def.outputs;
      const target = candidatePorts.find((p) =>
        arePortsCompatible(connection.portType as never, p.type),
      );
      if (target) {
        if (connection.handleType === 'source') {
          addEdge(connection.nodeId, `out-${connection.handleName}`, id, `in-${target.name}`);
        } else {
          addEdge(id, `out-${target.name}`, connection.nodeId, `in-${connection.handleName}`);
        }
      }
    }
    close();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlight((h) => (results.length === 0 ? 0 : (h + 1) % results.length));
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight((h) =>
        results.length === 0 ? 0 : (h - 1 + results.length) % results.length,
      );
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const def = results[highlight];
      if (def) commit(def);
      return;
    }
  };

  // Clamp position so the popover doesn't run off-screen.
  const POPOVER_W = 280;
  const POPOVER_H = 320;
  const left = Math.min(screenPos.x, window.innerWidth - POPOVER_W - 8);
  const top = Math.min(screenPos.y, window.innerHeight - POPOVER_H - 8);

  return (
    <div
      id="floret-node-search"
      className="node-search"
      style={{ left, top }}
      onKeyDown={onKey}
    >
      <div className="node-search-header">
        <input
          ref={inputRef}
          className="node-search-input nodrag"
          type="search"
          placeholder={connection ? 'add compatible node…' : 'add node…'}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setHighlight(0);
          }}
        />
        <span className="node-search-hint">Esc</span>
      </div>
      <div className="node-search-list" ref={listRef}>
        {results.length === 0 ? (
          <div className="node-search-empty">no matches</div>
        ) : (
          results.map((def, i) => (
            <div
              key={def.type}
              data-idx={i}
              className={`node-search-item${i === highlight ? ' selected' : ''}`}
              onMouseEnter={() => setHighlight(i)}
              onMouseDown={(e) => {
                // mousedown (not click) so we beat the outside-click close.
                e.preventDefault();
                commit(def);
              }}
            >
              <div className="node-search-item-main">
                <span className="node-search-item-cat">{def.category}</span>
                <span className="node-search-item-label">{def.label}</span>
              </div>
              <span className="node-search-item-type">{def.type}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
