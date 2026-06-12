import { useEffect, useMemo, useState } from 'react';
import { Canvas } from '../editor/Canvas';
import { NodePalette } from '../editor/NodePalette';
import { HelpPanel } from '../editor/HelpPanel';
import { LibraryPanel } from '../editor/LibraryPanel';
import { PreviewPanel } from '../preview/PreviewPanel';
import { PortValueEditor } from '../editor/PortValueEditor';
import { Toolbar } from './shell/Toolbar';
import { StatusBar } from './shell/StatusBar';
import { WelcomeModal } from './shell/WelcomeModal';
import { DropZoneOverlay } from '../editor/panel/DropZoneOverlay';
import { useGlobalShortcuts } from './shortcuts';
import { useEditor } from '../editor/editor-state';

export function App() {
  useGlobalShortcuts();
  const [narrowViewport, setNarrowViewport] = useState<boolean>(
    typeof window !== 'undefined' ? window.innerWidth <= 880 : false,
  );
  // Preview panels are derived 1:1 from `render/renderer` nodes in the
  // patch — there's no separate user-managed list. Add a renderer node
  // → its preview appears; delete the node → the preview vanishes. The
  // panel id IS the node id so layout state (dock side, size, float
  // position) sticks across edits via the existing panelState
  // localStorage. Future on-screen renderers can opt out by registering
  // a different defType.
  const rendererNodes = useEditor((s) => s.nodes);
  const renderers = useMemo(
    () => rendererNodes.filter((n) => n.data.defType === 'render/renderer'),
    [rendererNodes],
  );

  useEffect(() => {
    const mq = window.matchMedia('(max-width: 880px)');
    const update = () => setNarrowViewport(mq.matches);
    update();
    mq.addEventListener('change', update);
    return () => mq.removeEventListener('change', update);
  }, []);
  void narrowViewport;

  return (
    <div className="app-shell">
      <Toolbar />
      <div className="split single-pane">
        <div className="pane editor-pane">
          {/* The editor pane is a column: a horizontal main row (left
              rails | canvas | right rails) on top, and a full-width
              bottom rail below it. Rail containers are flex containers;
              docked panels portal themselves into the matching
              `panel-rail-<side>-<column>` based on their `dock` state.
              Left rails use negative flex-order (closer-to-canvas column
              = larger order → adjacent to canvas); right rails use
              positive flex-order. Empty rails collapse via CSS `:empty`,
              so we can over-provision a fixed count. */}
          <div className="editor-main-row">
            {Array.from({ length: 6 }).map((_, c) => (
              <div
                key={`l${c}`}
                id={`panel-rail-left-${c}`}
                className="panel-rail panel-rail-left"
                style={{ order: -1 - c }}
              />
            ))}
            <Canvas />
            {Array.from({ length: 6 }).map((_, c) => (
              <div
                key={`r${c}`}
                id={`panel-rail-right-${c}`}
                className="panel-rail panel-rail-right"
                style={{ order: 1 + c }}
              />
            ))}
          </div>
          {/* Bottom rail — a single full-width horizontal strip; bottom-
              docked panels sit side by side and share its height. */}
          <div id="panel-rail-bottom-0" className="panel-rail panel-rail-bottom" />
          {/* Panels still live in the React tree here — they portal
              into the rails (or to <body> when floating). */}
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
        </div>
      </div>
      <StatusBar />
      <PortValueEditor />
      <WelcomeModal />
      <DropZoneOverlay />
    </div>
  );
}
