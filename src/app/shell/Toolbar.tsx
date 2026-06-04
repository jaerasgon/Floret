import { useEffect, useState } from 'react';
import { useAppStore } from '../store';
import { usePrefs } from '../prefs';
import { useEditor } from '../../editor/editor-state';
import { autoLayoutPatch } from '../../editor/autoLayout';
import { FileMenu } from './FileMenu';
import { SampleMenu } from './SampleMenu';
import { LibraryMenu } from './LibraryMenu';
import { SettingsModal } from './SettingsModal';

export function Toolbar() {
  const running = useAppStore((s) => s.running);
  const toggleRunning = useAppStore((s) => s.toggleRunning);
  const refresh = useAppStore((s) => s.refresh);
  const wireFlow = usePrefs((s) => s.wireFlowAnimation);
  const proMode = usePrefs((s) => s.proMode);
  const setPref = usePrefs((s) => s.set);
  const setNodes = useEditor((s) => s.setNodes);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const tidy = () => {
    const s = useEditor.getState();
    setNodes(autoLayoutPatch(s.nodes, s.edges));
  };

  // Mirror the wireFlow toggle to a body attribute so the wire-animation
  // CSS rules can scope-match without prop drilling.
  useEffect(() => {
    if (wireFlow) document.body.setAttribute('data-wire-flow', '1');
    else document.body.removeAttribute('data-wire-flow');
  }, [wireFlow]);

  // Same pattern for Pro mode — collapses every node down to title +
  // pins (no row contents, no type subtitle), and lets the category
  // gradient fill the whole node.
  useEffect(() => {
    if (proMode) document.body.setAttribute('data-pro-mode', '1');
    else document.body.removeAttribute('data-pro-mode');
  }, [proMode]);

  return (
    <div className="toolbar">
      <span className="title">FLORET</span>
      <span className="toolbar-divider" />
      <FileMenu />
      <span className="toolbar-divider" />
      <SampleMenu />
      <span className="toolbar-divider" />
      <LibraryMenu />
      <div className="spacer" />
      <button onClick={tidy} title="Auto-arrange nodes left-to-right by data flow">
        Tidy
      </button>
      <button
        onClick={() => setPref('wireFlowAnimation', !wireFlow)}
        title={wireFlow ? 'Hide wire flow animation' : 'Animate dashes along wires'}
        aria-pressed={wireFlow}
      >
        Flow {wireFlow ? '●' : '○'}
      </button>
      <button
        className={`toolbar-run-btn${running ? ' running' : ''}`}
        onClick={toggleRunning}
        title={running ? 'Pause the runtime' : 'Resume the runtime'}
      >
        {running ? '❚❚ Pause' : '▶ Play'}
      </button>
      <button
        className="toolbar-refresh-btn"
        onClick={refresh}
        title="Restart the scene — resets time to 0 and re-mounts the canvas"
      >
        ↻ Refresh
      </button>
      <button
        className={`toolbar-pro-btn${proMode ? ' active' : ''}`}
        onClick={() => setPref('proMode', !proMode)}
        title={proMode ? 'Exit Pro mode — show full node UI' : 'Enter Pro mode — collapse nodes to title + pins'}
        aria-pressed={proMode}
      >
        Pro
      </button>
      <button
        className="toolbar-settings-btn"
        onClick={() => setSettingsOpen(true)}
        title="Open Settings"
      >
        ⚙
      </button>
      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
