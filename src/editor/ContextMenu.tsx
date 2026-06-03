import { useEffect } from 'react';
import { useOverlay } from './overlay-store';

/**
 * Right-click context menu. Items are supplied at open time so the
 * same component handles pane / node / edge contexts.
 */
export function ContextMenu() {
  const open = useOverlay((s) => s.contextOpen);
  const screenPos = useOverlay((s) => s.contextScreenPosition);
  const items = useOverlay((s) => s.contextItems);
  const close = useOverlay((s) => s.closeContext);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const popover = document.getElementById('nodule-context-menu');
      if (popover && !popover.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('keydown', onKey);
    };
  }, [open, close]);

  if (!open) return null;

  // Off-screen clamp.
  const MENU_W = 220;
  const MENU_H = Math.min(items.length * 26 + 8, 400);
  const left = Math.min(screenPos.x, window.innerWidth - MENU_W - 4);
  const top = Math.min(screenPos.y, window.innerHeight - MENU_H - 4);

  return (
    <div
      id="nodule-context-menu"
      className="context-menu"
      style={{ left, top, minWidth: MENU_W }}
    >
      {items.map((it, i) => {
        if (it.separator) {
          return <div key={`sep-${i}`} className="context-sep" />;
        }
        return (
          <button
            key={`${i}-${it.label}`}
            type="button"
            className={`context-item${it.danger ? ' danger' : ''}`}
            disabled={it.disabled}
            onMouseDown={(e) => {
              e.preventDefault();
              if (it.disabled) return;
              close();
              it.onSelect();
            }}
          >
            <span>{it.label}</span>
            {it.shortcut && <span className="context-shortcut">{it.shortcut}</span>}
          </button>
        );
      })}
    </div>
  );
}
