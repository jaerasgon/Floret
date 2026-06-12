import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useDock, type DefaultPlacement } from './dockStore';
import { registerDockMeta, updateDockMeta } from './dockMeta';
import { beginDockDrag } from './dockDragStore';

/**
 * A dock client — one panel's worth of content placed in the dock layout.
 * Drop-in replacement for the old `<Panel>`: same id / title / children /
 * headerActions surface. The chrome (tab + drag handle) is owned by the
 * DockArea's group, so a docked panel just portals its body into its slot.
 * Floating panels render a self-contained card; hidden panels render
 * nothing.
 */
export interface DockClientProps {
  id: string;
  title: string;
  children: ReactNode;
  /** Where the panel lands the first time it appears. */
  placement?: DefaultPlacement;
  headerActions?: ReactNode;
  /** Pinned (the canvas) — can't be floated or hidden. */
  pinned?: boolean;
  hideClose?: boolean;
}

export function DockClient({
  id, title, children, placement, headerActions, pinned, hideClose,
}: DockClientProps) {
  const ensurePanel = useDock((s) => s.ensurePanel);
  const floatRect = useDock((s) => s.floating.find((f) => f.id === id));
  const hidden = useDock((s) => s.hidden.includes(id));

  // Register metadata for the tab bar; keep title / actions fresh.
  useEffect(() => registerDockMeta({ id, title, headerActions, pinned, hideClose }), [id]);
  useEffect(() => { updateDockMeta(id, { title, headerActions }); }, [id, title, headerActions]);

  // Ensure placement once on mount.
  useEffect(() => {
    ensurePanel(id, placement ?? { kind: 'root-edge', side: 'right' });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  if (hidden) return null;
  if (floatRect) {
    return <FloatingPanel id={id} title={title} headerActions={headerActions} pinned={pinned} hideClose={hideClose}>{children}</FloatingPanel>;
  }
  return <DockedSlot id={id}>{children}</DockedSlot>;
}

/** Portal the body into the group slot the DockArea rendered for this id. */
function DockedSlot({ id, children }: { id: string; children: ReactNode }) {
  // Re-resolve the slot whenever the layout tree changes (the panel may
  // have moved to a different group / been re-created).
  const root = useDock((s) => s.root);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    setSlot(document.querySelector<HTMLElement>(`[data-dock-slot="${CSS.escape(id)}"]`));
  }, [id, root]);
  if (!slot) return null;
  return createPortal(<div className="dock-panel-body">{children}</div>, slot);
}

/** A detached, draggable, resizable card for a floating panel. */
function FloatingPanel({
  id, title, children, headerActions, pinned, hideClose,
}: { id: string; title: string; children: ReactNode; headerActions?: ReactNode; pinned?: boolean; hideClose?: boolean }) {
  const rect = useDock((s) => s.floating.find((f) => f.id === id))!;
  const moveFloat = useDock((s) => s.moveFloat);
  const resizeFloat = useDock((s) => s.resizeFloat);
  const dropOnRootEdge = useDock((s) => s.dropOnRootEdge);
  const hidePanel = useDock((s) => s.hidePanel);
  const ref = useRef<HTMLDivElement | null>(null);

  const onHeaderDown = (e: React.MouseEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return;
    e.preventDefault();
    const sx = e.clientX, sy = e.clientY;
    const ox = rect.x, oy = rect.y;
    const onMove = (ev: MouseEvent) => moveFloat(id, ox + (ev.clientX - sx), oy + (ev.clientY - sy));
    const onUp = () => { document.removeEventListener('mousemove', onMove); document.removeEventListener('mouseup', onUp); };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };
  const onResizeDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault(); e.stopPropagation();
    const sx = e.clientX, sy = e.clientY, ow = rect.w, oh = rect.h;
    const onMove = (ev: MouseEvent) => resizeFloat(id, ow + (ev.clientX - sx), oh + (ev.clientY - sy));
    const onUp = () => { document.removeEventListener('mousemove', onMove); document.removeEventListener('mouseup', onUp); };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };

  return createPortal(
    <div ref={ref} className="panel panel-float dock-float" style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}>
      <header
        className="panel-header"
        // Press-drag the header to re-dock (start a dock-drag); a plain
        // drag without crossing into the dock area just repositions.
        onMouseDown={(e) => {
          if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return;
          beginDockDrag(id, e.clientX, e.clientY);
          onHeaderDown(e);
        }}
      >
        <span className="panel-title"><span className="panel-grip" aria-hidden>⋮⋮</span>{title}</span>
        <div className="panel-header-actions">
          {headerActions}
          <button type="button" className="panel-action" title="Dock to right" onClick={() => dropOnRootEdge(id, 'right')}>⇥</button>
          {!hideClose && !pinned && (
            <button type="button" className="panel-action panel-action-close" title="Hide" onClick={() => hidePanel(id)}>×</button>
          )}
        </div>
      </header>
      <div className="panel-body panel-body-float dock-panel-body">{children}</div>
      <div className="panel-resize-grip" onMouseDown={onResizeDown} title="Resize" />
    </div>,
    document.body,
  );
}

/** Subscribe helper kept for parity with the old API surface; unused now
 *  but exported so call sites that imported a panel-state hook don't break
 *  during migration. */
export function useDockClientReady(): boolean {
  return useSyncExternalStore(() => () => {}, () => true, () => true);
}
