import { useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { getDockDrag, subscribeDockDrag, type DockDropTarget } from './dockDragStore';
import { getDockMeta } from './dockMeta';

/**
 * Overlay painted during a dock drag. Shows a translucent preview of the
 * region the panel will occupy — the dropped half of a group, the whole
 * group (centre = new tab), or an edge strip of the dock area — plus a
 * label chip that follows the cursor.
 */
export function DockDragLayer() {
  const session = useSyncExternalStore(subscribeDockDrag, getDockDrag, getDockDrag);
  if (!session || !session.started) return null;
  const zone = zoneRect(session.hover);
  const title = getDockMeta(session.panelId)?.title ?? session.panelId;
  return createPortal(
    <div className="dock-drag-layer" aria-hidden>
      {zone && (
        <div className="dock-drop-zone" style={{ left: zone.left, top: zone.top, width: zone.width, height: zone.height }}>
          <span className="dock-drop-label">{zone.label}</span>
        </div>
      )}
      <div className="dock-drag-chip" style={{ left: session.x + 12, top: session.y + 8 }}>{title}</div>
    </div>,
    document.body,
  );
}

function zoneRect(t: DockDropTarget): { left: number; top: number; width: number; height: number; label: string } | null {
  if (!t) return null;
  if (t.kind === 'float') return null;
  const r = t.rect;
  if (t.kind === 'group') {
    if (t.side === 'center') return { left: r.left, top: r.top, width: r.width, height: r.height, label: 'Tab here' };
    if (t.side === 'left') return { left: r.left, top: r.top, width: r.width / 2, height: r.height, label: 'Split left' };
    if (t.side === 'right') return { left: r.left + r.width / 2, top: r.top, width: r.width / 2, height: r.height, label: 'Split right' };
    if (t.side === 'top') return { left: r.left, top: r.top, width: r.width, height: r.height / 2, label: 'Split top' };
    return { left: r.left, top: r.top + r.height / 2, width: r.width, height: r.height / 2, label: 'Split bottom' };
  }
  // root edge
  const band = 220;
  if (t.side === 'left') return { left: r.left, top: r.top, width: band, height: r.height, label: 'Dock left' };
  if (t.side === 'right') return { left: r.right - band, top: r.top, width: band, height: r.height, label: 'Dock right' };
  if (t.side === 'top') return { left: r.left, top: r.top, width: r.width, height: band, label: 'Dock top' };
  return { left: r.left, top: r.bottom - band, width: r.width, height: band, label: 'Dock bottom' };
}
