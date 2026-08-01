import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { List, LayoutGrid, Columns3, ChevronLeft, Folder, FolderOpen, BookOpen, Download } from 'lucide-react';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import Tooltip from '@mui/material/Tooltip';
import { Panel } from './panel/Panel';
import { useEditor } from './editor-state';
import { paramStore } from '../runtime/param-store';
import { history } from './history';
import { useLibraryUI } from '../app/libraryUI';
import {
  downloadLibrary,
  importLibrary,
  listLibraries,
  loadLibraryPatch,
  parseLibrary,
  removeLibrary,
  removeLibraryEntry,
} from '../persistence/library';
import { SaveToLibraryModal } from '../app/shell/SaveToLibraryModal';
import { loadJSON, saveJSON } from '../persistence/local-store';
import { deserializePatch } from '../persistence/serialize';
import { renderPatchThumbnail } from '../persistence/thumbnail';
import type { PatchFile } from '../persistence/schema';

/**
 * Library panel — Finder-style browser for built-in samples + user
 * libraries. Three view modes:
 *
 *   - **list**: collapsible tree of folders → libraries → patches (same
 *     UI the old dropdown shipped with). Best for keyboard scrolling /
 *     small panel widths.
 *   - **grid**: icon-tile view. Navigate one level at a time (root →
 *     folder → library), with a back button to step out. Drilling in
 *     swaps the tile grid below the header.
 *   - **column**: three-pane Mac Finder layout — folders+libs / libs
 *     inside selected folder / patches inside selected library. Click-
 *     through, no drill-state to manage.
 *
 * Visibility is controlled by `useLibraryUI` (toggled from the toolbar's
 * Library button); when off, the panel doesn't mount at all.
 */

const FOLDER_COLLAPSE_LS_KEY = 'floret.libraryFolders.v1';
const LIBRARY_COLLAPSE_LS_KEY = 'floret.libraryCollapse.v1';

function readCollapseMap(key: string): Record<string, boolean> {
  const parsed = loadJSON<Record<string, boolean>>(key, {});
  return parsed && typeof parsed === 'object' ? parsed : {};
}
function writeCollapseMap(key: string, map: Record<string, boolean>): void {
  saveJSON(key, map);
}

/** Pick a hue from the library name so each library tile has a
 *  consistent accent across views, with zero per-patch metadata. */
function hueFor(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return h % 360;
}

export function LibraryPanel() {
  const visible = useLibraryUI((s) => s.visible);
  if (!visible) return null;
  return <LibraryPanelInner />;
}

function LibraryPanelInner() {
  const viewMode = useLibraryUI((s) => s.viewMode);
  const setViewMode = useLibraryUI((s) => s.setViewMode);

  return (
    <Panel
      id="library"
      title="Library"
      defaults={{ dock: 'left', width: 320, height: 560, floatX: 80, floatY: 80 }}
      hideClose
      headerActions={
        <ToggleButtonGroup
          exclusive
          size="small"
          aria-label="Library view"
          value={viewMode}
          onChange={(_e, v) => { if (v) setViewMode(v as typeof viewMode); }}
        >
          <ToggleButton value="list" aria-label="List view">
            <Tooltip title="List view"><List size={12} strokeWidth={1.7} /></Tooltip>
          </ToggleButton>
          <ToggleButton value="grid" aria-label="Icon view">
            <Tooltip title="Icon view"><LayoutGrid size={12} strokeWidth={1.7} /></Tooltip>
          </ToggleButton>
          <ToggleButton value="column" aria-label="Column view">
            <Tooltip title="Column view"><Columns3 size={12} strokeWidth={1.7} /></Tooltip>
          </ToggleButton>
        </ToggleButtonGroup>
      }
    >
      <LibraryBody />
    </Panel>
  );
}

function LibraryBody() {
  const loadPatch = useEditor((s) => s.loadPatch);
  const nodes = useEditor((s) => s.nodes);
  const viewMode = useLibraryUI((s) => s.viewMode);
  const [libraries, setLibraries] = useState(() => listLibraries());
  const [error, setError] = useState<string | null>(null);
  const [saveOpen, setSaveOpen] = useState(false);
  const [query, setQuery] = useState('');
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  // Flat cross-library search. When the query is non-empty it replaces the
  // active view with a single ranked result list — folder/library structure
  // doesn't matter when you're hunting for a patch by name.
  const searchResults = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return null;
    const hits: { slug: string; libName: string; patchId: string; name: string; description?: string }[] = [];
    for (const lib of libraries) {
      for (const p of lib.library.patches) {
        if (
          p.name.toLowerCase().includes(q) ||
          (p.description?.toLowerCase().includes(q) ?? false) ||
          lib.library.name.toLowerCase().includes(q)
        ) {
          hits.push({ slug: lib.slug, libName: lib.library.name, patchId: p.id, name: p.name, description: p.description });
        }
      }
    }
    return hits;
  }, [query, libraries]);

  // Group libraries by folder; `null`-folder libs render at root.
  const groups = useMemo(() => {
    const folders = new Map<string, typeof libraries>();
    const root: typeof libraries = [];
    for (const lib of libraries) {
      const folder = lib.library.folder;
      if (folder) {
        if (!folders.has(folder)) folders.set(folder, []);
        folders.get(folder)!.push(lib);
      } else {
        root.push(lib);
      }
    }
    return { folders: [...folders.entries()], root };
  }, [libraries]);

  const refresh = useCallback(() => setLibraries(listLibraries()), []);

  const onPickFile = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = '';
    if (!f) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const parsed = parseLibrary(JSON.parse(String(reader.result)));
        if (!parsed.ok) { setError(`Not a valid library: ${parsed.error}`); return; }
        importLibrary(parsed.library);
        refresh();
        setError(null);
      } catch (err) {
        setError(`Could not parse JSON: ${String(err).slice(0, 120)}`);
      }
    };
    reader.onerror = () => setError('Could not read file.');
    reader.readAsText(f);
  }, [refresh]);

  const onPatchClick = useCallback((slug: string, patchId: string) => {
    try {
      const { nodes: ns, edges: es } = loadLibraryPatch(slug, patchId);
      paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
      loadPatch(ns, es);
      history.reset();
    } catch (err) {
      setError(String(err).slice(0, 180));
    }
  }, [loadPatch]);

  return (
    <div className="library-panel-body">
      <div className="library-actions">
        <Tooltip title="Import a floret.library JSON file">
          <Button
            variant="outlined"
            size="small"
            color="inherit"
            onClick={() => fileInputRef.current?.click()}
          >
            ＋ Import…
          </Button>
        </Tooltip>
        <Tooltip title={nodes.length === 0 ? 'Add nodes to the canvas first' : 'Save the current patch as a library entry'}>
          <span>
            <Button
              variant="outlined"
              size="small"
              color="inherit"
              disabled={nodes.length === 0}
              onClick={() => setSaveOpen(true)}
            >
              ↓ Save patch…
            </Button>
          </span>
        </Tooltip>
        <input
          ref={fileInputRef}
          type="file"
          accept="application/json,.json"
          style={{ display: 'none' }}
          onChange={onPickFile}
        />
      </div>
      {libraries.length > 0 && (
        <div className="library-search">
          <TextField
            aria-label="Search all patches"
            size="small"
            fullWidth
            placeholder="Search all patches…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      )}
      {error && <div className="library-error">{error}</div>}
      {libraries.length === 0 && !error && (
        <div className="library-empty">No libraries yet. Use "Import…" above.</div>
      )}
      {searchResults !== null ? (
        <SearchResults results={searchResults} onPatchClick={onPatchClick} />
      ) : (
        <>
          {viewMode === 'list' && (
            <ListView
              groups={groups as ViewGroups}
              onPatchClick={onPatchClick}
              onSetError={setError}
              refresh={refresh}
            />
          )}
          {viewMode === 'grid' && (
            <GridView
              groups={groups as ViewGroups}
              onPatchClick={onPatchClick}
              onSetError={setError}
            />
          )}
          {viewMode === 'column' && (
            <ColumnView
              groups={groups as ViewGroups}
              onPatchClick={onPatchClick}
            />
          )}
        </>
      )}
      {saveOpen && (
        <SaveToLibraryModal onClose={() => { setSaveOpen(false); refresh(); }} />
      )}
    </div>
  );
}

type StoredLib = ReturnType<typeof listLibraries>[number];
type ViewGroups = { folders: [string, StoredLib[]][]; root: StoredLib[] };

interface SearchHit {
  slug: string; libName: string; patchId: string; name: string; description?: string;
}

/* ─────────────────────────────────────────────────────────────────────
 * Search results — flat cross-library list (replaces the active view
 * while a query is present)
 * ───────────────────────────────────────────────────────────────────── */

function SearchResults({ results, onPatchClick }: {
  results: SearchHit[];
  onPatchClick: (slug: string, patchId: string) => void;
}) {
  if (results.length === 0) {
    return <div className="library-empty">No patches match.</div>;
  }
  return (
    <div className="library-search-results">
      <div className="library-search-count">{results.length} {results.length === 1 ? 'result' : 'results'}</div>
      {results.map((r) => (
        <button
          key={`${r.slug}::${r.patchId}`}
          className="library-row library-patch"
          onClick={() => onPatchClick(r.slug, r.patchId)}
          title={r.description ?? ''}
        >
          <div className="sample-name">{r.name}</div>
          <div className="library-search-lib">{r.libName}</div>
          {r.description && <div className="sample-desc">{r.description}</div>}
        </button>
      ))}
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────────────
 * List view — collapsible tree
 * ───────────────────────────────────────────────────────────────────── */

function ListView({ groups, onPatchClick, onSetError, refresh }: {
  groups: ViewGroups;
  onPatchClick: (slug: string, patchId: string) => void;
  onSetError: (s: string | null) => void;
  refresh: () => void;
}) {
  const [folderCollapsed, setFolderCollapsed] = useState<Record<string, boolean>>(
    () => readCollapseMap(FOLDER_COLLAPSE_LS_KEY),
  );
  const [libraryCollapsed, setLibraryCollapsed] = useState<Record<string, boolean>>(
    () => readCollapseMap(LIBRARY_COLLAPSE_LS_KEY),
  );

  const toggleFolder = useCallback((folder: string) => {
    setFolderCollapsed((prev) => {
      const current = folder in prev ? prev[folder]! : true;
      const next = { ...prev, [folder]: !current };
      writeCollapseMap(FOLDER_COLLAPSE_LS_KEY, next);
      return next;
    });
  }, []);
  const toggleLibrary = useCallback((slug: string, defaultCollapsed: boolean) => {
    setLibraryCollapsed((prev) => {
      const current = slug in prev ? prev[slug]! : defaultCollapsed;
      const next = { ...prev, [slug]: !current };
      writeCollapseMap(LIBRARY_COLLAPSE_LS_KEY, next);
      return next;
    });
  }, []);
  const isFolderCollapsed = (f: string) => f in folderCollapsed ? folderCollapsed[f]! : true;
  const isLibraryCollapsed = (slug: string, inFolder: boolean) =>
    slug in libraryCollapsed ? libraryCollapsed[slug]! : inFolder;

  const renderLibrary = (entry: StoredLib, inFolder: boolean) => {
    const collapsed = isLibraryCollapsed(entry.slug, inFolder);
    const isBuiltIn = !!entry.library.builtIn;
    return (
      <div
        key={entry.slug}
        className={`library-block${inFolder ? ' library-block-nested' : ''}`}
      >
        <button
          className="library-header library-header-toggle"
          onClick={() => toggleLibrary(entry.slug, inFolder)}
          title={entry.library.description ?? ''}
        >
          <span className="library-disclosure">{collapsed ? '▸' : '▾'}</span>
          <span className="library-name">{entry.library.name}</span>
          <span className="library-count">{entry.library.patches.length}</span>
          {isBuiltIn && <span className="library-tag">built-in</span>}
          <span className="library-header-actions">
            <button
              className="library-remove"
              onClick={(e) => {
                e.stopPropagation();
                try { downloadLibrary(entry.slug); }
                catch (err) { onSetError(String(err).slice(0, 180)); }
              }}
              title="Download this library as a .json file"
            >
              ⬇
            </button>
            {!isBuiltIn && (
              <button
                className="library-remove"
                onClick={(e) => {
                  e.stopPropagation();
                  removeLibrary(entry.slug);
                  refresh();
                }}
                title={`Remove library "${entry.library.name}"`}
              >
                ✕
              </button>
            )}
          </span>
        </button>
        {!collapsed && entry.library.patches.length === 0 && (
          <div className="library-empty library-empty-inline">
            Empty — use "↓ Save patch" above to add an entry.
          </div>
        )}
        {!collapsed && entry.library.patches.map((p) => (
          <div key={p.id} className="library-patch-row">
            <button
              className="library-row library-patch"
              onClick={() => onPatchClick(entry.slug, p.id)}
              title={p.description ?? ''}
            >
              <div className="sample-name">{p.name}</div>
              {p.description && <div className="sample-desc">{p.description}</div>}
            </button>
            {!isBuiltIn && (
              <button
                className="library-remove library-patch-remove"
                onClick={(e) => {
                  e.stopPropagation();
                  removeLibraryEntry(entry.slug, p.id);
                  refresh();
                }}
                title={`Remove "${p.name}" from "${entry.library.name}"`}
              >
                ✕
              </button>
            )}
          </div>
        ))}
      </div>
    );
  };

  return (
    <div className="library-list-view">
      {groups.folders.map(([folder, libs]) => {
        const collapsed = isFolderCollapsed(folder);
        const totalPatches = libs.reduce((s, l) => s + l.library.patches.length, 0);
        return (
          <div key={`folder:${folder}`} className="library-folder">
            <button className="library-folder-header" onClick={() => toggleFolder(folder)}>
              <span className="library-disclosure">{collapsed ? '▸' : '▾'}</span>
              <span className="library-folder-name">{folder}</span>
              <span className="library-count">{totalPatches}</span>
            </button>
            {!collapsed && (
              <div className="library-folder-body">
                {libs.map((lib) => renderLibrary(lib, true))}
              </div>
            )}
          </div>
        );
      })}
      {groups.root.map((lib) => renderLibrary(lib, false))}
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────────────
 * Grid (icon) view — drill-down navigation
 * ───────────────────────────────────────────────────────────────────── */

function GridView({ groups, onPatchClick, onSetError }: {
  groups: ViewGroups;
  onPatchClick: (slug: string, patchId: string) => void;
  onSetError: (s: string | null) => void;
}) {
  const selection = useLibraryUI((s) => s.selection);
  const setSelection = useLibraryUI((s) => s.setSelection);
  const resetSelection = useLibraryUI((s) => s.resetSelection);

  // Resolve current location: root, folder, or library.
  const currentLibrary = useMemo(() => {
    if (!selection.library) return null;
    return [...groups.root, ...groups.folders.flatMap(([, ls]) => ls)]
      .find((l) => l.slug === selection.library) ?? null;
  }, [selection.library, groups]);
  const currentFolder = useMemo(() => {
    if (!selection.folder) return null;
    return groups.folders.find(([f]) => f === selection.folder) ?? null;
  }, [selection.folder, groups]);

  // Breadcrumb: root → folder → library.
  const renderCrumb = () => {
    const parts: { label: string; onClick?: () => void }[] = [
      { label: 'Library', onClick: () => resetSelection() },
    ];
    if (selection.folder) {
      parts.push({
        label: selection.folder,
        onClick: () => setSelection({ library: null }),
      });
    }
    if (currentLibrary) parts.push({ label: currentLibrary.library.name });
    return (
      <div className="library-crumb">
        {parts[0]!.onClick && parts.length > 1 && (
          <button
            type="button"
            className="library-crumb-back"
            onClick={() => {
              if (selection.library) setSelection({ library: null });
              else resetSelection();
            }}
            title="Back"
          >
            <ChevronLeft size={12} strokeWidth={1.7} />
          </button>
        )}
        {parts.map((p, i) => (
          <span key={i} className="library-crumb-part">
            {p.onClick && i < parts.length - 1 ? (
              <button type="button" className="library-crumb-link" onClick={p.onClick}>{p.label}</button>
            ) : (
              <span className="library-crumb-current">{p.label}</span>
            )}
            {i < parts.length - 1 && <span className="library-crumb-sep">›</span>}
          </span>
        ))}
      </div>
    );
  };

  // Tile content: folders, libraries, or patches depending on level.
  let tiles: React.ReactNode;
  if (currentLibrary) {
    const isBuiltIn = !!currentLibrary.library.builtIn;
    tiles = currentLibrary.library.patches.length === 0 ? (
      <div className="library-empty library-empty-inline">Empty — use "↓ Save patch" above.</div>
    ) : (
      <div className="library-tile-grid">
        {currentLibrary.library.patches.map((p) => (
          <PatchTile
            key={p.id}
            name={p.name}
            description={p.description}
            hue={hueFor(currentLibrary.library.name)}
            patch={p.patch}
            onClick={() => onPatchClick(currentLibrary.slug, p.id)}
          />
        ))}
        {isBuiltIn && null}
      </div>
    );
  } else if (currentFolder) {
    const [, libs] = currentFolder;
    tiles = (
      <div className="library-tile-grid">
        {libs.map((lib) => (
          <LibraryTile
            key={lib.slug}
            name={lib.library.name}
            count={lib.library.patches.length}
            hue={hueFor(lib.library.name)}
            builtIn={!!lib.library.builtIn}
            onClick={() => setSelection({ library: lib.slug })}
            onDownload={() => {
              try { downloadLibrary(lib.slug); } catch (err) { onSetError(String(err).slice(0, 180)); }
            }}
          />
        ))}
      </div>
    );
  } else {
    // Root: folder cards + root-level library cards.
    tiles = (
      <div className="library-tile-grid">
        {groups.folders.map(([f, libs]) => {
          const totalPatches = libs.reduce((s, l) => s + l.library.patches.length, 0);
          return (
            <FolderTile
              key={`folder:${f}`}
              name={f}
              count={totalPatches}
              onClick={() => setSelection({ folder: f, library: null })}
            />
          );
        })}
        {groups.root.map((lib) => (
          <LibraryTile
            key={lib.slug}
            name={lib.library.name}
            count={lib.library.patches.length}
            hue={hueFor(lib.library.name)}
            builtIn={!!lib.library.builtIn}
            onClick={() => setSelection({ folder: null, library: lib.slug })}
            onDownload={() => {
              try { downloadLibrary(lib.slug); } catch (err) { onSetError(String(err).slice(0, 180)); }
            }}
          />
        ))}
      </div>
    );
  }

  return (
    <div className="library-grid-view">
      {(selection.folder || selection.library) && renderCrumb()}
      {tiles}
    </div>
  );
}

function FolderTile({ name, count, onClick }: { name: string; count: number; onClick: () => void }) {
  return (
    <button type="button" className="library-tile library-tile-folder" onClick={onClick} title={name}>
      <div className="library-tile-icon">
        <Folder size={28} strokeWidth={1.3} />
      </div>
      <div className="library-tile-name">{name}</div>
      <div className="library-tile-meta">{count}</div>
    </button>
  );
}

function LibraryTile({ name, count, hue, builtIn, onClick, onDownload }: {
  name: string; count: number; hue: number; builtIn: boolean;
  onClick: () => void; onDownload: () => void;
}) {
  return (
    <div className="library-tile-wrap">
      <button
        type="button"
        className="library-tile library-tile-library"
        onClick={onClick}
        title={name}
        style={{
          // Tinted "thumbnail" stripe — placeholder until real canvas previews ship.
          ['--tile-hue' as string]: `${hue}`,
        }}
      >
        <div className="library-tile-thumb" />
        <div className="library-tile-icon-overlay">
          <BookOpen size={18} strokeWidth={1.3} />
        </div>
        <div className="library-tile-name">{name}</div>
        <div className="library-tile-meta">
          {count} {count === 1 ? 'patch' : 'patches'}
          {builtIn && <span className="library-tag library-tag-inline">built-in</span>}
        </div>
      </button>
      <button
        type="button"
        className="library-tile-action"
        onClick={onDownload}
        title="Download library"
      >
        <Download size={11} strokeWidth={1.7} />
      </button>
    </div>
  );
}

function PatchTile({ name, description, hue, patch, onClick }: {
  name: string; description?: string; hue: number; patch: PatchFile; onClick: () => void;
}) {
  // Lazily render a real thumbnail off the main path. The tinted gradient
  // shows until (and if) the image resolves — audio-only patches and any
  // that fail to render simply keep the placeholder.
  const [thumb, setThumb] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    const run = () => {
      try {
        const { nodes, edges } = deserializePatch(patch);
        void renderPatchThumbnail(nodes, edges).then((url) => {
          if (!cancelled && url) setThumb(url);
        });
      } catch { /* keep placeholder */ }
    };
    // Defer to idle so a grid of tiles doesn't block paint.
    const ric = (window as unknown as { requestIdleCallback?: (cb: () => void) => number }).requestIdleCallback;
    const id = ric ? ric(run) : window.setTimeout(run, 0);
    return () => {
      cancelled = true;
      const cic = (window as unknown as { cancelIdleCallback?: (h: number) => void }).cancelIdleCallback;
      if (ric && cic) cic(id as number); else window.clearTimeout(id as number);
    };
  }, [patch]);

  return (
    <button
      type="button"
      className="library-tile library-tile-patch"
      onClick={onClick}
      title={description ?? name}
      style={{ ['--tile-hue' as string]: `${hue}` }}
    >
      <div
        className="library-tile-thumb"
        style={thumb ? { backgroundImage: `url(${thumb})`, backgroundSize: 'cover', backgroundPosition: 'center' } : undefined}
      />
      <div className="library-tile-name">{name}</div>
      {description && <div className="library-tile-meta library-tile-desc">{description}</div>}
    </button>
  );
}

/* ─────────────────────────────────────────────────────────────────────
 * Column view — Finder three-pane
 * ───────────────────────────────────────────────────────────────────── */

function ColumnView({ groups, onPatchClick }: {
  groups: ViewGroups;
  onPatchClick: (slug: string, patchId: string) => void;
}) {
  const selection = useLibraryUI((s) => s.selection);
  const setSelection = useLibraryUI((s) => s.setSelection);

  const currentFolder = useMemo(
    () => groups.folders.find(([f]) => f === selection.folder) ?? null,
    [selection.folder, groups],
  );
  // Resolve selected library — could be inside a folder or at root.
  const currentLibrary = useMemo(() => {
    if (!selection.library) return null;
    return [...groups.root, ...groups.folders.flatMap(([, ls]) => ls)]
      .find((l) => l.slug === selection.library) ?? null;
  }, [selection.library, groups]);

  return (
    <div className="library-column-view">
      {/* Col 1: folders + root libs */}
      <div className="library-column">
        {groups.folders.map(([f, libs]) => {
          const total = libs.reduce((s, l) => s + l.library.patches.length, 0);
          const active = selection.folder === f;
          return (
            <button
              key={`folder:${f}`}
              type="button"
              className={`library-column-row${active ? ' active' : ''}`}
              onClick={() => setSelection({ folder: f, library: null })}
            >
              {active ? <FolderOpen size={12} strokeWidth={1.5} /> : <Folder size={12} strokeWidth={1.5} />}
              <span className="library-column-label">{f}</span>
              <span className="library-count">{total}</span>
              <span className="library-column-chevron">›</span>
            </button>
          );
        })}
        {groups.root.map((lib) => {
          const active = selection.library === lib.slug && !selection.folder;
          return (
            <button
              key={lib.slug}
              type="button"
              className={`library-column-row${active ? ' active' : ''}`}
              onClick={() => setSelection({ folder: null, library: lib.slug })}
            >
              <BookOpen size={12} strokeWidth={1.5} />
              <span className="library-column-label">{lib.library.name}</span>
              <span className="library-count">{lib.library.patches.length}</span>
              <span className="library-column-chevron">›</span>
            </button>
          );
        })}
      </div>
      {/* Col 2: libraries inside selected folder */}
      <div className="library-column">
        {currentFolder ? (
          currentFolder[1].map((lib) => {
            const active = selection.library === lib.slug;
            return (
              <button
                key={lib.slug}
                type="button"
                className={`library-column-row${active ? ' active' : ''}`}
                onClick={() => setSelection({ library: lib.slug })}
              >
                <BookOpen size={12} strokeWidth={1.5} />
                <span className="library-column-label">{lib.library.name}</span>
                <span className="library-count">{lib.library.patches.length}</span>
                <span className="library-column-chevron">›</span>
              </button>
            );
          })
        ) : (
          <div className="library-column-hint">Pick a folder →</div>
        )}
      </div>
      {/* Col 3: patches inside selected library */}
      <div className="library-column">
        {currentLibrary ? (
          currentLibrary.library.patches.length === 0 ? (
            <div className="library-column-hint">Empty library.</div>
          ) : (
            currentLibrary.library.patches.map((p) => (
              <button
                key={p.id}
                type="button"
                className="library-column-row"
                onClick={() => onPatchClick(currentLibrary.slug, p.id)}
                title={p.description ?? ''}
              >
                <span className="library-column-label">{p.name}</span>
              </button>
            ))
          )
        ) : (
          <div className="library-column-hint">Pick a library →</div>
        )}
      </div>
    </div>
  );
}

