/**
 * Drag-driven dock coordination.
 *
 * Any Panel header drag publishes to this module-level store. The
 * <DropZoneOverlay> subscribes to render hover zones; the Panel itself
 * subscribes so it can re-render its drag preview if needed.
 *
 * A drop target is either:
 *   - `{ kind: 'edge', side: 'left' | 'right' }` — dock to that rail at
 *     the end of its stack.
 *   - `{ kind: 'panel', panelId, position: 'above' | 'below' | 'left' |
 *     'right' }` — dock relative to that panel.
 *   - `null` — drop in free space, float at the cursor.
 *
 * Implementation: a tiny pub/sub designed to drop straight into
 * `useSyncExternalStore`.
 */
export type DropTarget =
  | { kind: 'edge'; side: 'left' | 'right' }
  | { kind: 'panel'; panelId: string; position: 'above' | 'below' | 'left' | 'right' }
  | null;

export interface DragSession {
  /** Panel id being dragged. */
  id: string;
  /** Visible panel title — used by the drop overlay's preview chip. */
  title: string;
  /** Current cursor position; tracked here so the overlay re-renders. */
  x: number;
  y: number;
  /** Width of the floating preview (matches the panel's current size). */
  width: number;
  /** Which drop zone the pointer is currently over (visual highlight). */
  hover: DropTarget;
  /**
   * Callback the panel registered at drag start. The overlay calls this
   * on `mouseup` with the final drop target.
   */
  onDrop: (target: DropTarget, x: number, y: number) => void;
}

type Listener = () => void;

let session: DragSession | null = null;
const listeners = new Set<Listener>();

function emit(): void {
  for (const l of listeners) l();
}

export function getDragSession(): DragSession | null {
  return session;
}

export function subscribeDrag(fn: Listener): () => void {
  listeners.add(fn);
  return () => { listeners.delete(fn); };
}

export function beginDrag(s: DragSession): void {
  session = s;
  emit();
}

export function updateDrag(patch: Partial<Pick<DragSession, 'x' | 'y' | 'hover'>>): void {
  if (!session) return;
  session = { ...session, ...patch };
  emit();
}

export function endDrag(): void {
  session = null;
  emit();
}
