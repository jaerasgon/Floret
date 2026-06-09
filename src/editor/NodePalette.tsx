import { useMemo, useState, useSyncExternalStore } from 'react';
import { nodeRegistry } from '../nodes/registry';
import type { NodeDef } from '../nodes/types';
import { Panel } from './panel/Panel';

/**
 * Node palette — search + category tree of every registered NodeDef.
 * Wrapped in a generic `Panel` so the user can dock, float, resize, or
 * collapse it from a single uniform chrome.
 */
export function NodePalette() {
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

  return (
    <div className="palette-inner">
      <div className="palette-search-wrap">
        <input
          className="palette-search"
          type="search"
          placeholder="Search…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="palette-scroll">
        {filtered.length === 0 ? (
          <div className="palette-empty">no nodes match</div>
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
                  <span>{cat}</span>
                  <span className="palette-group-count">{defs.length}</span>
                </button>
                {isOpen && defs.map((def) => (
                  <div
                    key={def.type}
                    className="palette-item"
                    draggable
                    onDragStart={(e) => onDragStart(e, def)}
                    title={def.description ?? def.type}
                  >
                    <span className="palette-item-label">{def.label}</span>
                    <span className="palette-item-type">{def.type}</span>
                  </div>
                ))}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
