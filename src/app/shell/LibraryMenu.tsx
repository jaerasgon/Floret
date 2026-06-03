import { useCallback, useEffect, useRef, useState } from 'react';
import { useEditor } from '../../editor/editor-state';
import { paramStore } from '../../runtime/param-store';
import { history } from '../../editor/history';
import {
  downloadLibrary,
  importLibrary,
  listLibraries,
  loadLibraryPatch,
  parseLibrary,
  removeLibrary,
  removeLibraryEntry,
} from '../../persistence/library';
import { SaveToLibraryModal } from './SaveToLibraryModal';

/**
 * Library browser — toolbar dropdown showing every imported library
 * and the patches inside each. Click a patch to load it (same effect as
 * a sample). "Import library…" at the top opens a file picker that
 * accepts a `nodule.library` JSON file.
 *
 * Libraries persist in localStorage; the menu pulls a fresh snapshot
 * each time it opens, so import / remove side effects don't need to
 * thread through a store.
 */
export function LibraryMenu() {
  const loadPatch = useEditor((s) => s.loadPatch);
  const nodes = useEditor((s) => s.nodes);
  const [open, setOpen] = useState(false);
  const [libraries, setLibraries] = useState(() => listLibraries());
  const [error, setError] = useState<string | null>(null);
  const [saveOpen, setSaveOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    setLibraries(listLibraries());
    setError(null);
    const onClick = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onClick);
    return () => window.removeEventListener('mousedown', onClick);
  }, [open]);

  const onPickFile = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const f = e.target.files?.[0];
      e.target.value = '';
      if (!f) return;
      const reader = new FileReader();
      reader.onload = () => {
        try {
          const parsed = parseLibrary(JSON.parse(String(reader.result)));
          if (!parsed.ok) {
            setError(`Not a valid library: ${parsed.error}`);
            return;
          }
          importLibrary(parsed.library);
          setLibraries(listLibraries());
          setError(null);
        } catch (err) {
          setError(`Could not parse JSON: ${String(err).slice(0, 120)}`);
        }
      };
      reader.onerror = () => setError('Could not read file.');
      reader.readAsText(f);
    },
    [],
  );

  const onPatchClick = useCallback(
    (slug: string, patchId: string) => {
      try {
        // No confirm — undo recovers the swap if it was unintended.
        const { nodes: ns, edges: es } = loadLibraryPatch(slug, patchId);
        paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
        loadPatch(ns, es);
        history.reset();
        setOpen(false);
      } catch (err) {
        setError(String(err).slice(0, 180));
      }
    },
    [loadPatch, nodes.length],
  );

  return (
    <div ref={wrapRef} style={{ position: 'relative' }}>
      <button onClick={() => setOpen((v) => !v)} title="Browse imported patch libraries">
        Library ▾
      </button>
      {open && (
        <div className="sample-menu library-menu">
          <button
            className="sample-menu-item library-import"
            onClick={() => fileInputRef.current?.click()}
            title="Import a nodule.library JSON file"
          >
            <div className="sample-name">＋ Import library…</div>
            <div className="sample-desc">Pick a `.json` library file from disk.</div>
          </button>
          <button
            className="sample-menu-item library-import"
            onClick={() => { setSaveOpen(true); setOpen(false); }}
            disabled={nodes.length === 0}
            title={nodes.length === 0 ? 'Add nodes to the canvas first' : 'Save the current patch as a library entry'}
          >
            <div className="sample-name">↓ Save current patch…</div>
            <div className="sample-desc">Add the canvas as an entry in an existing or new library.</div>
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept="application/json,.json"
            style={{ display: 'none' }}
            onChange={onPickFile}
          />
          {error && <div className="library-error">{error}</div>}
          {libraries.length === 0 && !error && (
            <div className="library-empty">
              No libraries yet. Use “Import library…” above.
            </div>
          )}
          {libraries.length > 0 && (
            <div className="library-separator" />
          )}
          {libraries.map((entry) => (
            <div key={entry.slug} className="library-block">
              <div className="library-header">
                <span className="library-name" title={entry.library.description ?? ''}>
                  {entry.library.name}
                </span>
                <span className="library-header-actions">
                  <button
                    className="library-remove"
                    onClick={(e) => {
                      e.stopPropagation();
                      try {
                        downloadLibrary(entry.slug);
                      } catch (err) {
                        setError(String(err).slice(0, 180));
                      }
                    }}
                    title="Download this library as a .json file"
                  >
                    ⬇
                  </button>
                  <button
                    className="library-remove"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeLibrary(entry.slug);
                      setLibraries(listLibraries());
                    }}
                    title={`Remove library "${entry.library.name}"`}
                  >
                    ✕
                  </button>
                </span>
              </div>
              {entry.library.patches.length === 0 && (
                <div className="library-empty library-empty-inline">
                  Empty — use “↓ Save current patch” above to add an entry.
                </div>
              )}
              {entry.library.patches.map((p) => (
                <div key={p.id} className="library-patch-row">
                  <button
                    className="sample-menu-item library-patch"
                    onClick={() => onPatchClick(entry.slug, p.id)}
                    title={p.description ?? ''}
                  >
                    <div className="sample-name">{p.name}</div>
                    {p.description && <div className="sample-desc">{p.description}</div>}
                  </button>
                  <button
                    className="library-remove library-patch-remove"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeLibraryEntry(entry.slug, p.id);
                      setLibraries(listLibraries());
                    }}
                    title={`Remove "${p.name}" from "${entry.library.name}"`}
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
      {saveOpen && (
        <SaveToLibraryModal
          onClose={() => {
            setSaveOpen(false);
            // The user may have created a new library or added an entry — refresh.
            setLibraries(listLibraries());
          }}
        />
      )}
    </div>
  );
}
