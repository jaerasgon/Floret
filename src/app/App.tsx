import { useEffect, useMemo } from 'react';
import { Canvas } from '../editor/Canvas';
import { NodePalette } from '../editor/NodePalette';
import { HelpPanel } from '../editor/HelpPanel';
import { LibraryPanel } from '../editor/LibraryPanel';
import { PreviewPanel } from '../preview/PreviewPanel';
import { PortValueEditor } from '../editor/PortValueEditor';
import { Toolbar } from './shell/Toolbar';
import { StatusBar } from './shell/StatusBar';
import { WelcomeModal } from './shell/WelcomeModal';
import { DockArea } from '../editor/dock/DockArea';
import { DockDragLayer } from '../editor/dock/DockDragLayer';
import { DockClient } from '../editor/dock/DockClient';
import { useDock } from '../editor/dock/dockStore';
import { useGlobalShortcuts } from './shortcuts';
import { useEditor } from '../editor/editor-state';
import { useLibraryUI } from './libraryUI';
import { usePrefs } from './prefs';

export function App() {
  useGlobalShortcuts();
  // Preview panels are derived 1:1 from `render/renderer` nodes in the
  // patch — add a renderer node → its preview appears; delete the node →
  // the preview vanishes. The dock panel id IS the node id, so its
  // position in the layout sticks across edits.
  const rendererNodes = useEditor((s) => s.nodes);
  const renderers = useMemo(
    () => rendererNodes.filter((n) => n.data.defType === 'render/renderer'),
    [rendererNodes],
  );

  // Prune preview panels whose renderer node is gone (e.g. after a reload
  // where the saved layout outlived the node).
  const reconcile = useDock((s) => s.reconcile);
  useEffect(() => {
    // Keep the four fixed panels + every live renderer's preview; evict
    // any other panel (e.g. a preview whose node was deleted, or one left
    // over from a previously-loaded patch).
    const keep = new Set(['canvas', 'palette', 'help', 'library', ...renderers.map((n) => n.id)]);
    reconcile(keep);
  }, [renderers, reconcile]);

  // The Library is toggled from the toolbar; drop it from the dock layout
  // when hidden so it doesn't leave a dangling slot (it re-docks on show).
  // Event-driven (not unmount-driven) so it's robust under StrictMode.
  const libraryVisible = useLibraryUI((s) => s.visible);
  useEffect(() => {
    if (!libraryVisible) useDock.getState().removePanel('library');
  }, [libraryVisible]);

  // Same treatment for the Nodes palette and Help panel — both toggle from
  // the toolbar and drop out of the dock layout when hidden.
  const paletteVisible = usePrefs((s) => s.paletteVisible);
  useEffect(() => {
    if (!paletteVisible) useDock.getState().removePanel('palette');
  }, [paletteVisible]);
  const helpVisible = usePrefs((s) => s.helpVisible);
  useEffect(() => {
    if (!helpVisible) useDock.getState().removePanel('help');
  }, [helpVisible]);

  // Stop the BROWSER from page-zooming on trackpad pinch / ⌘-scroll anywhere
  // in the app. Pinch = `ctrl+wheel` (Chrome/Electron) or the WebKit
  // `gesture*` events (Safari). Components that want their own zoom (the node
  // canvas, the preview) read the same events and handle it themselves —
  // this only cancels the browser's default whole-page zoom.
  useEffect(() => {
    const onWheel = (e: WheelEvent) => { if (e.ctrlKey) e.preventDefault(); };
    const onGesture = (e: Event) => e.preventDefault();
    window.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('gesturestart', onGesture, { passive: false });
    window.addEventListener('gesturechange', onGesture, { passive: false });
    return () => {
      window.removeEventListener('wheel', onWheel);
      window.removeEventListener('gesturestart', onGesture);
      window.removeEventListener('gesturechange', onGesture);
    };
  }, []);

  return (
    <div className="app-shell">
      <Toolbar />
      <div className="split single-pane">
        <div className="pane editor-pane">
          {/* The dock area renders the recursive split-tree of panels;
              every panel (including the canvas) is a dock client that
              portals its content into the slot the tree lays out for it.
              Drop a panel on any region's edge to split it, or its centre
              to tab — at any depth. */}
          <DockArea />
          {/* Dock clients — order-independent; each finds its own slot. */}
          <DockClient id="canvas" title="Canvas" pinned placement={{ kind: 'root-edge', side: 'right' }}>
            <Canvas />
          </DockClient>
          <NodePalette />
          <HelpPanel />
          <LibraryPanel />
          {renderers.map((node, idx) => {
            const label =
              (node.data.params as Record<string, unknown> | undefined)?.label as string | undefined;
            const title = label && label.trim()
              ? label
              : renderers.length > 1 ? `Preview · #${idx + 1}` : 'Preview';
            return (
              <PreviewPanel
                key={node.id}
                panelId={node.id}
                rendererId={node.id}
                title={title}
              />
            );
          })}
          <DockDragLayer />
        </div>
      </div>
      <StatusBar />
      <PortValueEditor />
      <WelcomeModal />
    </div>
  );
}
