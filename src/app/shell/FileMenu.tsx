import { useCallback, useEffect, useRef, useState } from 'react';
import { useEditor } from '../../editor/editor-state';
import { paramStore } from '../../runtime/param-store';
import { autosave, clearAutosave, loadAutosave, loadPatch, savePatch } from '../../persistence/fs';
import { deserializePatch, serializePatch } from '../../persistence/serialize';
import { starterPatch } from '../../persistence/starter';
import { buildShareUrl, consumeSharedPatch, hasSharedPatch } from '../../persistence/share';

/**
 * Toolbar's File controls — New, Open, Save, Save As, plus a "live autosave"
 * indicator. Autosave fires (debounced) whenever the editor changes; load
 * on next launch hydrates from localStorage if present.
 */
export function FileMenu() {
  const nodes = useEditor((s) => s.nodes);
  const edges = useEditor((s) => s.edges);
  const loadEditor = useEditor((s) => s.loadPatch);
  const [handleRef] = useState<{ current: FileSystemFileHandle | null }>({ current: null });
  const [fileName, setFileName] = useState<string>('untitled');
  const [status, setStatus] = useState<string>('');
  const debounceRef = useRef<number | null>(null);

  // Autosave: debounce 400ms after the last graph mutation.
  useEffect(() => {
    if (debounceRef.current != null) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(() => {
      autosave(serializePatch({ nodes, edges, name: fileName }));
    }, 400);
    return () => {
      if (debounceRef.current != null) window.clearTimeout(debounceRef.current);
    };
  }, [nodes, edges, fileName]);

  // Ctrl+S keyboard shortcut → dispatched via window event from shortcuts.ts
  useEffect(() => {
    const handler = () => { void onSave(); };
    window.addEventListener('floret:save', handler);
    return () => window.removeEventListener('floret:save', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges, fileName]);

  // Restore from autosave on first mount. If there's no autosave at all
  // (first-ever launch on this profile) drop in the starter patch so the
  // user sees a moving thing instead of EMPTY CANVAS. A deliberate "New"
  // via the toolbar still leaves the canvas blank — that's a different
  // intent than "I'm opening the app for the first time".
  useEffect(() => {
    // A shared link (#patch=…) wins over autosave — someone opened this URL
    // specifically to see that patch.
    if (hasSharedPatch()) {
      void (async () => {
        try {
          const shared = await consumeSharedPatch();
          if (shared) {
            const { nodes: ns, edges: es } = deserializePatch(shared);
            paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
            loadEditor(ns, es);
            if (shared.meta?.name) setFileName(shared.meta.name);
            setStatus('shared patch loaded');
            return;
          }
        } catch (err) {
          setStatus(`shared link failed: ${(err as Error).message.slice(0, 60)}`);
        }
        restoreAutosaveOrStarter();
      })();
      return;
    }
    restoreAutosaveOrStarter();

    function restoreAutosaveOrStarter() {
      const restored = loadAutosave();
      if (restored && restored.nodes.length > 0) {
        const { nodes: ns, edges: es } = deserializePatch(restored);
        // Hydrate ParamStore from saved params so IO-box reads land correctly.
        paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
        loadEditor(ns, es);
        if (restored.meta?.name) setFileName(restored.meta.name);
        setStatus('autosave restored');
      } else if (!restored) {
        const { nodes: ns, edges: es } = starterPatch();
        paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
        loadEditor(ns, es);
        setStatus('starter patch');
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onNew = useCallback(() => {
    // Native confirm() blocks the JS thread and trips test drivers;
    // rely on undo to recover an accidental wipe instead.
    loadEditor([], []);
    handleRef.current = null;
    setFileName('untitled');
    clearAutosave();
    setStatus('new patch');
  }, [handleRef, loadEditor, nodes.length]);

  const onSave = useCallback(async () => {
    const patch = serializePatch({ nodes, edges, name: fileName });
    try {
      const result = await savePatch(patch, handleRef.current);
      handleRef.current = result.handle;
      setFileName(result.name.replace(/\.floret\.json$|\.json$/i, ''));
      setStatus(`saved ${result.name}`);
    } catch (err) {
      if ((err as Error).name !== 'AbortError') {
        setStatus(`save failed: ${(err as Error).message}`);
      }
    }
  }, [edges, fileName, handleRef, nodes]);

  const onSaveAs = useCallback(async () => {
    handleRef.current = null;
    await onSave();
  }, [handleRef, onSave]);

  const onShare = useCallback(async () => {
    if (nodes.length === 0) { setStatus('nothing to share — canvas is empty'); return; }
    let url: string;
    try {
      url = await buildShareUrl(serializePatch({ nodes, edges, name: fileName }));
    } catch (err) {
      setStatus(`share failed: ${(err as Error).message.slice(0, 60)}`);
      return;
    }
    try {
      await navigator.clipboard.writeText(url);
      setStatus(`link copied (${(url.length / 1024).toFixed(1)} KB)`);
    } catch {
      // Clipboard can be blocked (no focus, insecure context, permissions).
      // Put the link in the address bar so it's always copyable by hand.
      window.history.replaceState(null, '', url);
      setStatus('link in address bar — copy it (clipboard blocked)');
    }
  }, [nodes, edges, fileName]);

  const onOpen = useCallback(async () => {
    try {
      const result = await loadPatch();
      const { nodes: ns, edges: es } = deserializePatch(result.patch);
      paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
      loadEditor(ns, es);
      handleRef.current = result.handle;
      setFileName(result.name.replace(/\.floret\.json$|\.json$/i, ''));
      setStatus(`opened ${result.name}`);
    } catch (err) {
      if ((err as Error).name !== 'AbortError') {
        setStatus(`open failed: ${(err as Error).message}`);
      }
    }
  }, [handleRef, loadEditor]);

  return (
    <>
      <button onClick={onNew} title="New empty patch">New</button>
      <button onClick={onOpen} title="Open a .floret.json file">Open</button>
      <button onClick={onSave} title="Save (Ctrl+S)">Save</button>
      <button onClick={onSaveAs} title="Save under a new name">Save As</button>
      <button onClick={onShare} title="Copy a shareable link to this patch">Share</button>
      <span style={{ color: 'var(--fg-2)', fontFamily: 'var(--font-mono)', marginLeft: 8 }}>
        {fileName}
      </span>
      {status && (
        <span style={{ color: 'var(--accent-dim)', fontFamily: 'var(--font-mono)', marginLeft: 8 }}>
          · {status}
        </span>
      )}
    </>
  );
}
