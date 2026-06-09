import { useEffect, useState } from 'react';
import { Canvas } from '../editor/Canvas';
import { NodePalette } from '../editor/NodePalette';
import { HelpPanel } from '../editor/HelpPanel';
import { PreviewPanel } from '../preview/PreviewPanel';
import { PortValueEditor } from '../editor/PortValueEditor';
import { Toolbar } from './shell/Toolbar';
import { StatusBar } from './shell/StatusBar';
import { WelcomeModal } from './shell/WelcomeModal';
import { useGlobalShortcuts } from './shortcuts';
import { useAppStore } from './store';

export function App() {
  useGlobalShortcuts();
  const [narrowViewport, setNarrowViewport] = useState<boolean>(
    typeof window !== 'undefined' ? window.innerWidth <= 880 : false,
  );
  const previews = useAppStore((s) => s.previews);
  const setPreviewRenderer = useAppStore((s) => s.setPreviewRenderer);

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
          {/* Docked panels position themselves via flex `order` so left
              panels render before the canvas and right panels render
              after, regardless of declaration order here. */}
          <NodePalette />
          <Canvas />
          <HelpPanel />
          {previews.map((p) => (
            <PreviewPanel
              key={p.panelId}
              panelId={p.panelId}
              rendererId={p.rendererId}
              onSelectRenderer={(id) => setPreviewRenderer(p.panelId, id)}
              title={p.panelId === 'preview' ? 'Preview' : `Preview ${p.panelId.slice('preview-'.length)}`}
            />
          ))}
        </div>
      </div>
      <StatusBar />
      <PortValueEditor />
      <WelcomeModal />
    </div>
  );
}
