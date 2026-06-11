import { useEffect, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { endDrag, getDragSession, subscribeDrag, updateDrag, type DropTarget } from './dragStore';
import { listPanels, subscribePanels } from './panelRegistry';

/**
 * Full-viewport overlay rendered while any panel is being dragged.
 *
 * Resolves the cursor position into a drop target:
 *
 *   1. If the cursor is over a *visible, non-dragged* panel, the panel's
 *      bounding rect is split into four quadrants — top, bottom, left,
 *      right. The dominant axis under the cursor wins (closest edge),
 *      so a single highlight rectangle covers exactly the half/edge the
 *      drop will land on.
 *   2. Otherwise, if the cursor is within EDGE_W px of the left/right
 *      viewport edge, the drop docks to that rail at the end of its
 *      stack.
 *   3. Otherwise, the drop floats at the cursor.
 *
 * The dragged panel itself follows the cursor live (handled by Panel —
 * the overlay just paints the destination preview).
 */
const EDGE_W = 60; // px — viewport edge band for whole-rail docking

interface ZoneRect {
  left: number;
  top: number;
  width: number;
  height: number;
  label: string;
}

function quadrantForPanel(rect: DOMRect, x: number, y: number): {
  position: 'above' | 'below' | 'left' | 'right';
  zone: ZoneRect;
} {
  // Pick the dominant axis: how far the cursor is from each edge
  // (normalised to the panel's dimensions). Smallest = closest edge.
  const topD = (y - rect.top) / rect.height;
  const botD = (rect.bottom - y) / rect.height;
  const leftD = (x - rect.left) / rect.width;
  const rightD = (rect.right - x) / rect.width;
  const min = Math.min(topD, botD, leftD, rightD);
  if (min === topD) {
    return {
      position: 'above',
      zone: { left: rect.left, top: rect.top, width: rect.width, height: rect.height / 2, label: 'Place above' },
    };
  }
  if (min === botD) {
    return {
      position: 'below',
      zone: { left: rect.left, top: rect.top + rect.height / 2, width: rect.width, height: rect.height / 2, label: 'Place below' },
    };
  }
  if (min === leftD) {
    return {
      position: 'left',
      zone: { left: rect.left, top: rect.top, width: rect.width / 2, height: rect.height, label: 'Dock left of' },
    };
  }
  return {
    position: 'right',
    zone: { left: rect.left + rect.width / 2, top: rect.top, width: rect.width / 2, height: rect.height, label: 'Dock right of' },
  };
}

function resolveTarget(x: number, y: number, draggingId: string): { target: DropTarget; zone: ZoneRect | null } {
  // 1. Per-panel quadrants take priority — they're more specific.
  const panels = listPanels();
  for (const p of panels) {
    if (p.id === draggingId) continue;
    if (p.dock === 'hidden' || p.dock === 'float') continue;
    const rect = p.getRect();
    if (!rect) continue;
    if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue;
    const { position, zone } = quadrantForPanel(rect, x, y);
    return {
      target: { kind: 'panel', panelId: p.id, position },
      zone,
    };
  }
  // 2. Viewport-edge fallback — docks to a rail's tail.
  if (x <= EDGE_W) {
    return {
      target: { kind: 'edge', side: 'left' },
      zone: { left: 0, top: 0, width: 220, height: window.innerHeight, label: 'Dock to left rail' },
    };
  }
  if (x >= window.innerWidth - EDGE_W) {
    return {
      target: { kind: 'edge', side: 'right' },
      zone: { left: window.innerWidth - 220, top: 0, width: 220, height: window.innerHeight, label: 'Dock to right rail' },
    };
  }
  // 3. Free space — drop floats at the cursor.
  return { target: null, zone: null };
}

export function DropZoneOverlay() {
  const session = useSyncExternalStore(subscribeDrag, getDragSession, getDragSession);
  // Subscribe to panel registry too so the overlay re-renders when a
  // panel mounts / unmounts mid-drag (rare, but cheap to handle).
  useSyncExternalStore(subscribePanels, () => 0, () => 0);

  useEffect(() => {
    if (!session) return;
    const onMove = (ev: MouseEvent) => {
      const { target } = resolveTarget(ev.clientX, ev.clientY, session.id);
      updateDrag({ x: ev.clientX, y: ev.clientY, hover: target });
    };
    const onUp = (ev: MouseEvent) => {
      const { target } = resolveTarget(ev.clientX, ev.clientY, session.id);
      const onDrop = session.onDrop;
      const x = ev.clientX;
      const y = ev.clientY;
      endDrag();
      onDrop(target, x, y);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
    return () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
  }, [session?.id]);

  if (!session) return null;

  // Resolve the active zone *now* for paint. The store carries `hover`
  // (the target descriptor), but the visual zone rect is recomputed
  // each frame from current panel positions — cheaper than stashing it
  // and avoids stale rects when panels resize.
  const { zone } = resolveTarget(session.x, session.y, session.id);

  return createPortal(
    <div className="dropzone-overlay" aria-hidden>
      {zone && (
        <div
          className="dropzone-active"
          style={{ left: zone.left, top: zone.top, width: zone.width, height: zone.height }}
        >
          <div className="dropzone-label">{zone.label}</div>
        </div>
      )}
      <div
        className="dropzone-chip"
        style={{ left: session.x + 12, top: session.y + 8 }}
      >
        {session.title}
      </div>
    </div>,
    document.body,
  );
}
