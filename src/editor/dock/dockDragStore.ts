import type { Side } from './dockTree';
import { useDock } from './dockStore';

/**
 * Dock drag session. Started by pressing a tab; the DockDragLayer paints
 * a preview of where the panel will land and commits the move on mouseup.
 *
 * Resolution is done against live DOM rects (`.dock-group[data-group-id]`
 * and the `#dock-area-root` bounds) so it works at any nesting depth with
 * no rect registry to keep in sync.
 */
export type DockDropTarget =
  | { kind: 'group'; groupId: string; side: Side; rect: DOMRect }
  | { kind: 'root'; side: Exclude<Side, 'center'>; rect: DOMRect }
  | { kind: 'float'; x: number; y: number }
  | null;

export interface DockDragSession {
  panelId: string;
  x: number;
  y: number;
  started: boolean;
  hover: DockDropTarget;
}

let session: DockDragSession | null = null;
const listeners = new Set<() => void>();
function emit(): void { for (const l of listeners) l(); }

export function subscribeDockDrag(fn: () => void): () => void {
  listeners.add(fn);
  return () => { listeners.delete(fn); };
}
export function getDockDrag(): DockDragSession | null { return session; }

const ROOT_EDGE = 36; // px band around the dock area for whole-edge docks
const DRAG_THRESHOLD = 5;

/** Resolve a cursor position to a drop target against current DOM rects. */
export function resolveDockTarget(x: number, y: number, draggingId: string): DockDropTarget {
  const area = document.getElementById('dock-area-root');
  const areaRect = area?.getBoundingClientRect();

  // 1. Innermost group under the cursor wins. Iterate groups; pick the
  //    smallest-area one containing the point (deepest in the tree).
  const groups = Array.from(document.querySelectorAll<HTMLElement>('.dock-group[data-group-id]'));
  let best: { el: HTMLElement; rect: DOMRect } | null = null;
  for (const el of groups) {
    const rect = el.getBoundingClientRect();
    if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue;
    if (!best || rect.width * rect.height < best.rect.width * best.rect.height) best = { el, rect };
  }
  if (best) {
    const groupId = best.el.getAttribute('data-group-id')!;
    const side = quadrant(best.rect, x, y);
    // Centre drop onto the panel's own lone group is a no-op — treat as null.
    return { kind: 'group', groupId, side, rect: best.rect };
  }

  // 2. Root edge band.
  if (areaRect) {
    if (x - areaRect.left <= ROOT_EDGE) return { kind: 'root', side: 'left', rect: areaRect };
    if (areaRect.right - x <= ROOT_EDGE) return { kind: 'root', side: 'right', rect: areaRect };
    if (y - areaRect.top <= ROOT_EDGE) return { kind: 'root', side: 'top', rect: areaRect };
    if (areaRect.bottom - y <= ROOT_EDGE) return { kind: 'root', side: 'bottom', rect: areaRect };
  }

  // 3. Outside the dock area → float at the cursor.
  return { kind: 'float', x, y };
  void draggingId;
}

/** Centre band → tab; otherwise the nearest edge → split. */
function quadrant(rect: DOMRect, x: number, y: number): Side {
  const fx = (x - rect.left) / rect.width;
  const fy = (y - rect.top) / rect.height;
  // Central 40% box → tab (centre).
  if (fx > 0.3 && fx < 0.7 && fy > 0.3 && fy < 0.7) return 'center';
  const left = fx, right = 1 - fx, top = fy, bottom = 1 - fy;
  const min = Math.min(left, right, top, bottom);
  if (min === left) return 'left';
  if (min === right) return 'right';
  if (min === top) return 'top';
  return 'bottom';
}

export function beginDockDrag(panelId: string, x: number, y: number): void {
  const startX = x, startY = y;
  session = { panelId, x, y, started: false, hover: null };
  const onMove = (ev: MouseEvent) => {
    if (!session) return;
    if (!session.started) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD) return;
      session.started = true;
    }
    session = {
      ...session,
      x: ev.clientX,
      y: ev.clientY,
      hover: resolveDockTarget(ev.clientX, ev.clientY, panelId),
    };
    emit();
  };
  const onUp = (ev: MouseEvent) => {
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
    const s = session;
    session = null;
    emit();
    if (!s || !s.started) return;
    const target = resolveDockTarget(ev.clientX, ev.clientY, panelId);
    commitDrop(panelId, target);
  };
  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
  emit();
}

function commitDrop(panelId: string, target: DockDropTarget): void {
  const dock = useDock.getState();
  if (!target) return;
  if (target.kind === 'group') {
    dock.dropOnGroup(panelId, target.groupId, target.side);
  } else if (target.kind === 'root') {
    dock.dropOnRootEdge(panelId, target.side);
  } else if (target.kind === 'float') {
    dock.floatPanel(panelId, { x: target.x - 60, y: target.y - 14, w: 360, h: 320 });
  }
}
