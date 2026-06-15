import { useRef, useSyncExternalStore } from 'react';
import { useDock } from './dockStore';
import { getDockMeta, subscribeDockMeta } from './dockMeta';
import { beginDockDrag } from './dockDragStore';
import type { DockNode, DockSplit, DockGroup } from './dockTree';

/**
 * Recursive renderer for the dock layout tree. A split lays its children
 * out in a row/column with draggable dividers between them; a group is a
 * tabbed container whose active panel's content lives in a slot that the
 * panel component portals into. Dragging a tab starts a dock-drag; the
 * DockDragLayer paints drop zones and commits the move.
 */
export function DockArea() {
  const root = useDock((s) => s.root);
  // Re-render when panel metadata (titles / header actions) changes.
  useSyncExternalStore(subscribeDockMeta, () => metaVersion, () => metaVersion);
  if (!root) return <div className="dock-area dock-empty" id="dock-area-root" />;
  return (
    <div className="dock-area" id="dock-area-root">
      <DockNodeView node={root} />
    </div>
  );
}

// Bump a counter on every metadata emit so useSyncExternalStore sees a
// changed snapshot. (Cheap: metadata changes are rare.)
let metaVersion = 0;
subscribeDockMeta(() => { metaVersion++; });

function DockNodeView({ node }: { node: DockNode }) {
  if (node.kind === 'split') return <SplitView node={node} />;
  return <GroupView node={node} />;
}

function SplitView({ node }: { node: DockSplit }) {
  const setSizes = useDock((s) => s.setSizes);
  const ref = useRef<HTMLDivElement | null>(null);

  const onDividerDown = (i: number) => (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const container = ref.current;
    if (!container) return;
    const isRow = node.dir === 'row';
    const total = isRow ? container.clientWidth : container.clientHeight;
    const start = isRow ? e.clientX : e.clientY;
    const a0 = node.sizes[i]!;
    const b0 = node.sizes[i + 1]!;
    const onMove = (ev: MouseEvent) => {
      const delta = ((isRow ? ev.clientX : ev.clientY) - start) / total;
      const a = Math.max(0.05, a0 + delta);
      const b = Math.max(0.05, b0 - delta);
      const sizes = [...node.sizes];
      // Preserve the pair's combined share; only redistribute between them.
      const sum = a0 + b0;
      sizes[i] = (a / (a + b)) * sum;
      sizes[i + 1] = (b / (a + b)) * sum;
      setSizes(node.id, sizes);
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };

  return (
    <div ref={ref} className={`dock-split dock-split-${node.dir}`}>
      {node.children.map((child, i) => (
        <div className="dock-split-child" key={child.id} style={{ flex: `0 1 ${node.sizes[i]! * 100}%` }}>
          <DockNodeView node={child} />
          {i < node.children.length - 1 && (
            <div
              className={`dock-divider dock-divider-${node.dir}`}
              onMouseDown={onDividerDown(i)}
            />
          )}
        </div>
      ))}
    </div>
  );
}

function GroupView({ node }: { node: DockGroup }) {
  const setActive = useDock((s) => s.setActive);
  // A group with a single panel reads as a plain header that fills the width;
  // tabs only appear once a second panel is docked onto it.
  const single = node.panels.length === 1;
  return (
    <div className="dock-group" data-group-id={node.id}>
      <div className={`dock-tabbar${single ? ' dock-tabbar-single' : ''}`}>
        {node.panels.map((pid, i) => {
          const m = getDockMeta(pid);
          const active = i === node.active;
          return (
            // A tab is a `div role="tab"`, not a <button>: the active tab
            // hosts the panel's header-action buttons, and a <button> may
            // not contain <button>s (invalid DOM + unreliable clicks).
            <div
              key={pid}
              role="tab"
              tabIndex={0}
              aria-selected={active}
              className={`dock-tab${active ? ' active' : ''}`}
              onClick={() => setActive(node.id, i)}
              onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setActive(node.id, i); } }}
              onMouseDown={(e) => {
                // Left-press on a tab can start a drag (threshold handled
                // in beginDockDrag's move loop).
                if (e.button === 0) beginDockDrag(pid, e.clientX, e.clientY);
              }}
              title={m?.title ?? pid}
            >
              <span className="dock-tab-grip" aria-hidden>⋮⋮</span>
              <span className="dock-tab-label">{m?.title ?? pid}</span>
              {m?.headerActions && active && (
                <span className="dock-tab-actions" onMouseDown={(e) => e.stopPropagation()}>
                  {m.headerActions}
                </span>
              )}
            </div>
          );
        })}
      </div>
      <div className="dock-group-body">
        {node.panels.map((pid, i) => (
          <div
            key={pid}
            className="dock-slot"
            data-dock-slot={pid}
            style={{ display: i === node.active ? 'flex' : 'none' }}
          />
        ))}
      </div>
    </div>
  );
}
