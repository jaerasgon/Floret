import { useMemo, useState, useSyncExternalStore } from 'react';
import { SearchX } from 'lucide-react';
import TextField from '@mui/material/TextField';
import { nodeRegistry } from '../nodes/registry';
import type { NodeDef } from '../nodes/types';
import { Panel } from './panel/Panel';
import { useOverlay } from './overlay-store';
import { useFavorites } from './favorites';
import { usePrefs } from '../app/prefs';

/** Drawable region kinds (subpatches aren't drag-sized). */
function drawRegionKind(def: NodeDef): 'foreach' | 'if' | 'cache' | null {
  const k = def.regionKind;
  return k === 'foreach' || k === 'if' || k === 'cache' ? k : null;
}

/**
 * Node palette — search + category tree of every registered NodeDef.
 * Wrapped in a generic `Panel` so the user can dock, float, resize, or
 * collapse it from a single uniform chrome.
 */
export function NodePalette() {
  // Toggled from the toolbar; when off the panel doesn't mount at all
  // (App drops it from the dock layout — see the visibility effects there).
  const visible = usePrefs((s) => s.paletteVisible);
  if (!visible) return null;
  return (
    <Panel
      id="palette"
      title="Nodes"
      defaults={{ dock: 'left', width: 260, height: 600, floatX: 60, floatY: 80 }}
    >
      <PaletteBody />
    </Panel>
  );
}

function PaletteBody() {
  useSyncExternalStore(
    (cb) => nodeRegistry.subscribe(cb),
    () => nodeRegistry.version(),
    () => 0,
  );

  const armRegionDraw = useOverlay((s) => s.armRegionDraw);
  const armedRegionKind = useOverlay((s) => s.armedRegionKind);
  const favTypes = useFavorites((s) => s.types);
  const toggleFavorite = useFavorites((s) => s.toggleFavorite);
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  /** Categories the user has manually expanded. Search overrides this. */
  const [openCats, setOpenCats] = useState<Set<string>>(() => new Set());

  const filtered = useMemo(() => {
    const cats = nodeRegistry.categories();
    const entries: [string, NodeDef[]][] = [];
    for (const [cat, defs] of cats) {
      // Hide internal NodeDefs (splicer-in / splicer-out etc.) from
      // the palette — they're synthesised by compile, not user-added.
      const visible = defs.filter((d) => !d.hidden);
      const matched = q
        ? visible.filter(
            (d) =>
              d.label.toLowerCase().includes(q) ||
              d.type.toLowerCase().includes(q) ||
              (d.description?.toLowerCase().includes(q) ?? false) ||
              cat.toLowerCase().includes(q),
          )
        : visible;
      if (matched.length) entries.push([cat, matched.sort((a, b) => a.label.localeCompare(b.label))]);
    }
    return entries.sort(([a], [b]) => a.localeCompare(b));
  }, [q]);

  const onDragStart = (e: React.DragEvent, def: NodeDef) => {
    e.dataTransfer.setData('application/floret-def', def.type);
    e.dataTransfer.effectAllowed = 'move';
  };

  const toggleCat = (cat: string) => {
    setOpenCats((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  };

  /** A single palette row — shared by the category tree and the pinned
   *  Favorites section. Regions arm the draw tool; the ★ toggles favorites. */
  const renderItem = (def: NodeDef) => {
    const regionKind = drawRegionKind(def);
    const armed = regionKind !== null && armedRegionKind === regionKind;
    const fav = favTypes.includes(def.type);
    return (
      <div
        key={def.type}
        className={`palette-item${regionKind ? ' palette-item-region' : ''}${armed ? ' armed' : ''}`}
        draggable
        onDragStart={(e) => onDragStart(e, def)}
        // Regions arm the draw tool — by click or by drag-out. Either gesture
        // shows a crosshair on the canvas and a press-drag sizes the region.
        // Other nodes are drag-only.
        onClick={regionKind ? () => armRegionDraw(regionKind) : undefined}
        title={regionKind
          ? `${def.description ?? def.type}\n\nClick or drag out, then drag on the canvas to draw the region.`
          : def.description ?? def.type}
      >
        <span className="palette-item-label">{def.label}</span>
        <span className="palette-item-type">{armed ? 'draw on canvas…' : def.type}</span>
        <button
          className={`palette-item-star${fav ? ' on' : ''}`}
          aria-pressed={fav}
          title={fav ? 'Unfavorite' : 'Favorite (adds to Q menu)'}
          // Don't start a drag or arm a region when toggling the star.
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); toggleFavorite(def.type); }}
        >
          {fav ? '★' : '☆'}
        </button>
      </div>
    );
  };

  // Resolve favorites to defs, dropping stale + hidden types. Shown pinned at
  // the top unless a search is active (avoids duplicating the results).
  const favDefs = favTypes
    .map((t) => nodeRegistry.get(t))
    .filter((d): d is NodeDef => !!d && !d.hidden);

  return (
    <div className="palette-inner">
      <div className="palette-search-wrap">
        <TextField
          aria-label="Search nodes"
          size="small"
          fullWidth
          placeholder="Search…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="palette-scroll">
        {!q && favDefs.length > 0 && (
          <div className="palette-group" data-open={1} data-category="favorites">
            <div className="palette-group-title palette-group-title-static">
              <span className="palette-group-chevron">★</span>
              <span className="palette-group-dot" aria-hidden="true" />
              <span>Favorites</span>
              <span className="palette-group-count">{favDefs.length}</span>
            </div>
            {favDefs.map(renderItem)}
          </div>
        )}
        {filtered.length === 0 ? (
          <div className="palette-empty">
            <SearchX className="palette-empty-icon" size={22} strokeWidth={1.4} />
            <span>No nodes match</span>
          </div>
        ) : (
          filtered.map(([cat, defs]) => {
            const isOpen = q ? true : openCats.has(cat);
            return (
              <div
                key={cat}
                className="palette-group"
                data-open={isOpen ? 1 : 0}
                data-category={cat.toLowerCase()}
              >
                <button
                  className="palette-group-title"
                  onClick={() => toggleCat(cat)}
                  aria-expanded={isOpen}
                >
                  <span className="palette-group-chevron">{isOpen ? '▾' : '▸'}</span>
                  <span className="palette-group-dot" aria-hidden="true" />
                  <span>{cat}</span>
                  <span className="palette-group-count">{defs.length}</span>
                </button>
                {isOpen && defs.map(renderItem)}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
