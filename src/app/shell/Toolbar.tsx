import { useEffect, useState } from 'react';
import {
  RotateCcw,
  Settings as SettingsIcon,
  LayoutGrid,
  Waves,
  Maximize2,
  Library as LibraryIcon,
  Boxes as NodesIcon,
  HelpCircle as HelpIcon,
  Keyboard as KeyboardIcon,
  Sun,
  Moon,
} from 'lucide-react';
import { useAppStore } from '../store';
import { usePrefs } from '../prefs';
import { useEditor } from '../../editor/editor-state';
import { useFocusMode } from '../../editor/focus-store';
import { autoLayoutPatch } from '../../editor/autoLayout';
import { useLibraryUI } from '../libraryUI';
import { FileMenu } from './FileMenu';
import { SettingsModal } from './SettingsModal';
import { ShortcutsModal } from './ShortcutsModal';
import { Tooltip } from './Tooltip';

export function Toolbar() {
  // Pause lives on the Preview panel header. Refresh is global here —
  // it reboots the entire running scene by recompiling, which discards
  // every node's hidden runtime state (particle buffers, trail history,
  // stateful values, memo caches). The patch itself isn't reloaded.
  const refresh = useAppStore((s) => s.refresh);
  const wireFlow = usePrefs((s) => s.wireFlowAnimation);
  const theme = usePrefs((s) => s.theme);
  const setPref = usePrefs((s) => s.set);
  const setNodes = useEditor((s) => s.setNodes);
  // Focus mode features the first renderer's preview full-screen. Disabled when
  // the patch has no renderer (nothing to present).
  const firstRenderer = useEditor((s) => s.nodes.find((n) => n.data.defType === 'render/renderer'));
  const enterFocus = useFocusMode((s) => s.enter);
  const libraryVisible = useLibraryUI((s) => s.visible);
  const toggleLibrary = useLibraryUI((s) => s.toggleVisible);
  const paletteVisible = usePrefs((s) => s.paletteVisible);
  const helpVisible = usePrefs((s) => s.helpVisible);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  // `?` opens the shortcuts reference from anywhere (shortcuts.ts dispatches
  // the event so it stays out of editable fields).
  useEffect(() => {
    const open = () => setShortcutsOpen(true);
    window.addEventListener('floret:show-shortcuts', open);
    return () => window.removeEventListener('floret:show-shortcuts', open);
  }, []);

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

  // Apply the colour theme on <html> so every var() re-skins (and body-portaled
  // panels inherit it too).
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  return (
    <div className="toolbar">
      <span className="title">FLORET</span>
      <span className="toolbar-divider" />
      <FileMenu />
      <span className="toolbar-divider" />
      {/* View toggles grouped as one segmented cluster — they're a family
          (which panels are open), not three unrelated buttons. */}
      <div className="tb-seg" role="group" aria-label="Panels">
        <Tooltip label={libraryVisible ? 'Hide library' : 'Show library'}>
          <button
            className={`tb-seg-btn${libraryVisible ? ' active' : ''}`}
            onClick={toggleLibrary}
            aria-pressed={libraryVisible}
            aria-label="Toggle library"
          >
            <LibraryIcon size={14} strokeWidth={1.6} />
          </button>
        </Tooltip>
        <Tooltip label={paletteVisible ? 'Hide nodes' : 'Show nodes'}>
          <button
            className={`tb-seg-btn${paletteVisible ? ' active' : ''}`}
            onClick={() => setPref('paletteVisible', !paletteVisible)}
            aria-pressed={paletteVisible}
            aria-label="Toggle nodes panel"
          >
            <NodesIcon size={14} strokeWidth={1.6} />
          </button>
        </Tooltip>
        <Tooltip label={helpVisible ? 'Hide help' : 'Show help'}>
          <button
            className={`tb-seg-btn${helpVisible ? ' active' : ''}`}
            onClick={() => setPref('helpVisible', !helpVisible)}
            aria-pressed={helpVisible}
            aria-label="Toggle help panel"
          >
            <HelpIcon size={14} strokeWidth={1.6} />
          </button>
        </Tooltip>
      </div>
      <div className="spacer" />
      {/* Canvas view actions — arrange the graph, animate wires. */}
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
      <span className="toolbar-divider" />
      {/* Scene actions — restart the running scene, present full-screen. */}
      <Tooltip label="Restart scene — reboot runtime state (particles respawn, trails clear)">
        <button
          className="toolbar-refresh-btn tb-icon-btn"
          onClick={refresh}
          aria-label="Refresh"
        >
          <RotateCcw size={13} strokeWidth={1.8} />
        </button>
      </Tooltip>
      <Tooltip label={firstRenderer ? 'Focus mode — present the art full-screen' : 'Add a Renderer node to use focus mode'}>
        <button
          className="tb-icon-btn"
          onClick={() => firstRenderer && enterFocus(firstRenderer.id)}
          disabled={!firstRenderer}
          aria-label="Enter focus mode"
        >
          <Maximize2 size={14} strokeWidth={1.6} />
        </button>
      </Tooltip>
      <span className="toolbar-divider" />
      {/* Appearance & app — theme, shortcuts, settings. */}
      <Tooltip label={theme === 'light' ? 'Switch to dark theme' : 'Switch to light theme'}>
        <button
          className="tb-icon-btn"
          onClick={() => setPref('theme', theme === 'light' ? 'dark' : 'light')}
          aria-label="Toggle colour theme"
        >
          {theme === 'light' ? <Moon size={14} strokeWidth={1.6} /> : <Sun size={14} strokeWidth={1.6} />}
        </button>
      </Tooltip>
      <Tooltip label="Keyboard shortcuts" shortcut="?">
        <button
          className="tb-icon-btn"
          onClick={() => setShortcutsOpen(true)}
          aria-label="Keyboard shortcuts"
        >
          <KeyboardIcon size={14} strokeWidth={1.6} />
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
      {shortcutsOpen && <ShortcutsModal onClose={() => setShortcutsOpen(false)} />}
    </div>
  );
}
