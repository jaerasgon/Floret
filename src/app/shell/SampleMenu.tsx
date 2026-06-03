import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useEditor } from '../../editor/editor-state';
import { paramStore } from '../../runtime/param-store';
import { history } from '../../editor/history';
import { SAMPLE_CATEGORIES, SAMPLES, type Sample } from '../../persistence/samples';

/**
 * Drop-down that loads a built-in demo patch. Samples are grouped into
 * category tabs along the top — clicking a tab filters the list below.
 *
 * Default tab is the first category that actually has samples. The
 * canonical tab order comes from `SAMPLE_CATEGORIES`; any sample whose
 * `category` isn't in that list is bucketed under "Misc" at the end.
 */
const MISC = 'Misc';

function categoryOf(s: Sample): string {
  const c = s.category?.trim();
  return c && c.length > 0 ? c : MISC;
}

export function SampleMenu() {
  const loadPatch = useEditor((s) => s.loadPatch);
  const nodes = useEditor((s) => s.nodes);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  // Group samples by category once.
  const { tabs, byCategory } = useMemo(() => {
    const map = new Map<string, Sample[]>();
    for (const s of SAMPLES) {
      const c = categoryOf(s);
      let bucket = map.get(c);
      if (!bucket) { bucket = []; map.set(c, bucket); }
      bucket.push(s);
    }
    // Tab order: canonical categories first, then any extras (incl. Misc).
    const seen = new Set<string>();
    const ordered: string[] = [];
    for (const c of SAMPLE_CATEGORIES) {
      if (map.has(c)) { ordered.push(c); seen.add(c); }
    }
    for (const c of map.keys()) {
      if (!seen.has(c)) ordered.push(c);
    }
    return { tabs: ordered, byCategory: map };
  }, []);

  const [activeTab, setActiveTab] = useState<string>(() => tabs[0] ?? MISC);

  // Close the dropdown when the user clicks outside.
  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onClick);
    return () => window.removeEventListener('mousedown', onClick);
  }, [open]);

  const loadSample = useCallback(
    (id: string) => {
      const sample = SAMPLES.find((s) => s.id === id);
      if (!sample) return;
      // No confirm — undo recovers the swap if it was unintended.
      const { nodes: ns, edges: es } = sample.build();
      paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
      loadPatch(ns, es);
      history.reset();
      setOpen(false);
    },
    [loadPatch, nodes.length],
  );

  const activeSamples = byCategory.get(activeTab) ?? [];

  return (
    <div ref={wrapRef} style={{ position: 'relative' }}>
      <button onClick={() => setOpen((v) => !v)} title="Load a built-in sample patch">
        Samples ▾
      </button>
      {open && (
        <div className="sample-menu sample-menu-tabbed">
          <div className="sample-tabs">
            {tabs.map((c) => (
              <button
                key={c}
                type="button"
                className="sample-tab"
                data-active={c === activeTab ? 1 : 0}
                onClick={() => setActiveTab(c)}
                title={`${byCategory.get(c)?.length ?? 0} samples`}
              >
                {c}
              </button>
            ))}
          </div>
          <div className="sample-tab-body">
            {activeSamples.map((s) => (
              <button
                key={s.id}
                className="sample-menu-item"
                onClick={() => loadSample(s.id)}
                title={s.description}
              >
                <div className="sample-name">{s.name}</div>
                <div className="sample-desc">{s.description}</div>
              </button>
            ))}
            {activeSamples.length === 0 && (
              <div className="library-empty">No samples in this category yet.</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
