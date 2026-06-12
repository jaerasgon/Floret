import { useEffect, useState } from 'react';
import {
  RotateCcw,
  Settings as SettingsIcon,
  LayoutGrid,
  Waves,
  Sparkles,
  Library as LibraryIcon,
} from 'lucide-react';
import { useAppStore } from '../store';
import { usePrefs } from '../prefs';
import { useEditor } from '../../editor/editor-state';
import { autoLayoutPatch } from '../../editor/autoLayout';
import { useLibraryUI } from '../libraryUI';
import { FileMenu } from './FileMenu';
import { SettingsModal } from './SettingsModal';
import { Tooltip } from './Tooltip';

export function Toolbar() {
  // Pause lives on the Preview panel header. Refresh is global here —
  // it reboots the entire running scene by recompiling, which discards
  // every node's hidden runtime state (particle buffers, trail history,
  // stateful values, memo caches). The patch itself isn't reloaded.
  const refresh = useAppStore((s) => s.refresh);
  const wireFlow = usePrefs((s) => s.wireFlowAnimation);
  const proMode = usePrefs((s) => s.proMode);
  const setPref = usePrefs((s) => s.set);
  const setNodes = useEditor((s) => s.setNodes);
  const libraryVisible = useLibraryUI((s) => s.visible);
  const toggleLibrary = useLibraryUI((s) => s.toggleVisible);
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
      <Tooltip label={libraryVisible ? 'Hide library' : 'Show library'}>
        <button
          className={`tb-icon-btn tb-toggle${libraryVisible ? ' active' : ''}`}
          onClick={toggleLibrary}
          aria-pressed={libraryVisible}
          aria-label="Toggle library"
        >
          <LibraryIcon size={14} strokeWidth={1.6} />
        </button>
      </Tooltip>
      <div className="spacer" />
      <Tooltip label="Auto-arrange nodes" shortcut="⇧T">
        <button className="tb-icon-btn" onClick={tidy} aria-label="Tidy">
          <LayoutGrid size={14} strokeWidth={1.6} />
        </button>
      </Tooltip>
      <Tooltip
        label={wireFlow ? 'Hide wire flow' : 'Animate wires'}
      >
        <button
          className={`tb-icon-btn tb-toggle${wireFlow ? ' active' : ''}`}
          onClick={() => setPref('wireFlowAnimation', !wireFlow)}
          aria-pressed={wireFlow}
          aria-label="Toggle wire flow"
        >
          <Waves size={14} strokeWidth={1.6} />
        </button>
      </Tooltip>
      <Tooltip label="Restart scene — reboot runtime state (particles respawn, trails clear)">
        <button
          className="toolbar-refresh-btn tb-icon-btn"
          onClick={refresh}
          aria-label="Refresh"
        >
          <RotateCcw size={13} strokeWidth={1.8} />
        </button>
      </Tooltip>
      <Tooltip
        label={proMode ? 'Exit Pro mode' : 'Enter Pro mode'}
      >
        <button
          className={`toolbar-pro-btn tb-pill${proMode ? ' active' : ''}`}
          onClick={() => setPref('proMode', !proMode)}
          aria-pressed={proMode}
          aria-label="Toggle Pro mode"
        >
          <Sparkles size={12} strokeWidth={1.6} />
          <span>Pro</span>
        </button>
      </Tooltip>
      <Tooltip label="Settings">
        <button
          className="toolbar-settings-btn tb-icon-btn"
          onClick={() => setSettingsOpen(true)}
          aria-label="Settings"
        >
          <SettingsIcon size={14} strokeWidth={1.6} />
        </button>
      </Tooltip>
      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
