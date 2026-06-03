import { useState } from 'react';
import { Canvas } from '../editor/Canvas';
import { NodePalette } from '../editor/NodePalette';
import { Preview } from '../preview/Preview';
import { PortValueEditor } from '../editor/PortValueEditor';
import { Toolbar } from './shell/Toolbar';
import { StatusBar } from './shell/StatusBar';
import { Resizer } from './shell/Resizer';
import { useGlobalShortcuts } from './shortcuts';

export function App() {
  useGlobalShortcuts();
  const [previewWidth, setPreviewWidth] = useState<number>(480);
  const [paletteCollapsed, setPaletteCollapsed] = useState<boolean>(false);

  return (
    <div className="app-shell">
      <Toolbar />
      <div
        className="split"
        style={{ ['--preview-width' as string]: `${previewWidth}px` }}
      >
        <div className="pane editor-pane">
          <div
            className="editor-with-palette"
            data-palette-collapsed={paletteCollapsed ? 1 : 0}
          >
            <NodePalette
              collapsed={paletteCollapsed}
              onToggleCollapse={() => setPaletteCollapsed((c) => !c)}
            />
            <Canvas />
          </div>
        </div>
        <Resizer onResize={(dx) => setPreviewWidth((w) => Math.max(240, Math.min(1200, w - dx)))} />
        <div className="pane preview-pane">
          <Preview />
        </div>
      </div>
      <StatusBar />
      <PortValueEditor />
    </div>
  );
}
