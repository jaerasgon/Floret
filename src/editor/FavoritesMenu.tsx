import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { nodeRegistry } from '../nodes/registry';
import type { NodeDef } from '../nodes/types';
import { useOverlay } from './overlay-store';
import { useFavorites } from './favorites';
import { placeNodeAtFlow } from './place-node';

/**
 * Favorites quick-menu — opened with the Q shortcut at the cursor. Lists the
 * user's starred node types for fast placement: arrows + Enter, or the 1–9
 * number keys for instant drop. Mirrors NodeSearch's popover shell (outside-
 * click + Esc to close, off-screen clamping) but without a text input, and
 * reuses the shared `placeNodeAtFlow` helper so placement + auto-parenting
 * match the search popover exactly.
 */
export function FavoritesMenu() {
  const open = useOverlay((s) => s.favoritesOpen);
  const screenPos = useOverlay((s) => s.favoritesScreenPosition);
  const flowPos = useOverlay((s) => s.favoritesFlowPosition);
  const close = useOverlay((s) => s.closeFavorites);

  const favTypes = useFavorites((s) => s.types);

  // Re-resolve when the registry changes (late node registration).
  useSyncExternalStore(
    (cb) => nodeRegistry.subscribe(cb),
    () => nodeRegistry.version(),
    () => 0,
  );

  const [highlight, setHighlight] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);

  // Resolve favorited types to defs, dropping stale + hidden ones.
  const defs = favTypes
    .map((t) => nodeRegistry.get(t))
    .filter((d): d is NodeDef => !!d && !d.hidden);

  // Reset highlight + focus the popover root on (re-)open so arrow/number
  // keys land without needing an input element.
  useEffect(() => {
    if (open) {
      setHighlight(0);
      requestAnimationFrame(() => rootRef.current?.focus());
    }
  }, [open]);

  // Keep highlight in range as the list changes.
  useEffect(() => {
    if (highlight >= defs.length) setHighlight(0);
  }, [defs.length, highlight]);

  // Close on outside click. Capture phase so we beat React Flow's handlers.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const popover = document.getElementById('floret-favorites-menu');
      if (popover && !popover.contains(e.target as Node)) close();
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  }, [open, close]);

  if (!open) return null;

  const pick = (def: NodeDef) => {
    placeNodeAtFlow(def, flowPos);
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
      setHighlight((h) => (defs.length === 0 ? 0 : (h + 1) % defs.length));
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight((h) => (defs.length === 0 ? 0 : (h - 1 + defs.length) % defs.length));
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const def = defs[highlight];
      if (def) pick(def);
      return;
    }
    // 1–9 → instant placement of that favorite.
    if (e.key >= '1' && e.key <= '9') {
      const i = Number(e.key) - 1;
      const def = defs[i];
      if (def) {
        e.preventDefault();
        pick(def);
      }
    }
  };

  // Clamp position so the popover doesn't run off-screen.
  const POPOVER_W = 240;
  const POPOVER_H = 320;
  const left = Math.min(screenPos.x, window.innerWidth - POPOVER_W - 8);
  const top = Math.min(screenPos.y, window.innerHeight - POPOVER_H - 8);

  return (
    <div
      id="floret-favorites-menu"
      className="node-search favorites-menu"
      style={{ left, top }}
      tabIndex={-1}
      ref={rootRef}
      onKeyDown={onKey}
    >
      <div className="node-search-header">
        <span className="node-search-item-cat">Favorites</span>
        <span className="node-search-hint">Q</span>
      </div>
      <div className="node-search-list">
        {defs.length === 0 ? (
          <div className="node-search-empty">
            No favorites yet — click the ★ on a node in the palette.
          </div>
        ) : (
          defs.map((def, i) => (
            <div
              key={def.type}
              data-idx={i}
              className={`node-search-item${i === highlight ? ' selected' : ''}`}
              onMouseEnter={() => setHighlight(i)}
              onMouseDown={(e) => {
                // mousedown (not click) so we beat the outside-click close.
                e.preventDefault();
                pick(def);
              }}
            >
              {i < 9 && <span className="favorites-hotkey">{i + 1}</span>}
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
