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
import { FloretLogo } from './FloretLogo';

const ICON = 15;

/** Add/remove `key` from the active-panel set — the segmented toggles are
 *  independent, so each click flips just its own membership. */
function toggleIn(set: string[], key: string): string[] {
  return set.includes(key) ? set.filter((k) => k !== key) : [...set, key];
}

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

  // The three panel toggles are one connected family (which panels are open),
  // so they render as a single multi-select ToggleButtonGroup. onChange hands
  // back the full active set; diff it against each store flag and flip only the
  // one that actually changed.
  const panelValue = [
    libraryVisible ? 'library' : null,
    paletteVisible ? 'nodes' : null,
    helpVisible ? 'help' : null,
  ].filter(Boolean) as string[];

  const onPanelChange = (value: string | null | string[]) => {
    const next = new Set(Array.isArray(value) ? value : value ? [value] : []);
    if (next.has('library') !== libraryVisible) toggleLibrary();
    if (next.has('nodes') !== paletteVisible) setPref('paletteVisible', next.has('nodes'));
    if (next.has('help') !== helpVisible) setPref('helpVisible', next.has('help'));
  };

  return (
    <div className="toolbar">
      <span className="title" aria-label="Floret" title="Floret">
        <FloretLogo size={22} />
      </span>
      <span className="toolbar-divider" />
      <FileMenu />
      <span className="toolbar-divider" />
      {/* Panel view-toggles — one segmented pill (library / nodes / help). */}
      <div className="tb-seg" role="group" aria-label="Panels">
        <button
          type="button"
          className={`tb-seg-btn${libraryVisible ? ' active' : ''}`}
          aria-pressed={libraryVisible}
          title={libraryVisible ? 'Hide library' : 'Show library'}
          onClick={() => onPanelChange(toggleIn(panelValue, 'library'))}
        >
          <LibraryIcon size={ICON} strokeWidth={1.6} />
        </button>
        <button
          type="button"
          className={`tb-seg-btn${paletteVisible ? ' active' : ''}`}
          aria-pressed={paletteVisible}
          title={paletteVisible ? 'Hide nodes' : 'Show nodes'}
          onClick={() => onPanelChange(toggleIn(panelValue, 'nodes'))}
        >
          <NodesIcon size={ICON} strokeWidth={1.6} />
        </button>
        <button
          type="button"
          className={`tb-seg-btn${helpVisible ? ' active' : ''}`}
          aria-pressed={helpVisible}
          title={helpVisible ? 'Hide help' : 'Show help'}
          onClick={() => onPanelChange(toggleIn(panelValue, 'help'))}
        >
          <HelpIcon size={ICON} strokeWidth={1.6} />
        </button>
      </div>
      <button
        type="button"
        className="tb-icon-btn"
        aria-label="Toggle colour theme"
        title={theme === 'light' ? 'Switch to dark theme' : 'Switch to light theme'}
        onClick={() => setPref('theme', theme === 'light' ? 'dark' : 'light')}
      >
        {theme === 'light' ? <Moon size={ICON} strokeWidth={1.6} /> : <Sun size={ICON} strokeWidth={1.6} />}
      </button>
      <div className="spacer" />
      <button
        type="button"
        className="tb-icon-btn"
        aria-label="Auto-arrange nodes"
        title="Auto-arrange nodes  ⇧T"
        onClick={tidy}
      >
        <LayoutGrid size={ICON} strokeWidth={1.6} />
      </button>
      <button
        type="button"
        className={`tb-icon-btn tb-toggle${wireFlow ? ' active' : ''}`}
        aria-pressed={wireFlow}
        title={wireFlow ? 'Hide wire flow' : 'Animate wires'}
        onClick={() => setPref('wireFlowAnimation', !wireFlow)}
      >
        <Waves size={ICON} strokeWidth={1.6} />
      </button>
      <button
        type="button"
        className="tb-icon-btn"
        aria-label="Restart scene"
        title="Restart scene — reboot runtime state (particles respawn, trails clear)"
        onClick={refresh}
      >
        <RotateCcw size={ICON - 1} strokeWidth={1.8} />
      </button>
      <button
        type="button"
        className="tb-icon-btn"
        aria-label="Enter focus mode"
        title={firstRenderer ? 'Focus mode — present the art full-screen' : 'Add a Renderer node to use focus mode'}
        disabled={!firstRenderer}
        onClick={() => firstRenderer && enterFocus(firstRenderer.id)}
      >
        <Maximize2 size={ICON} strokeWidth={1.6} />
      </button>
      <button
        type="button"
        className="tb-icon-btn"
        aria-label="Settings"
        title="Settings"
        onClick={() => setSettingsOpen(true)}
      >
        <SettingsIcon size={ICON} strokeWidth={1.6} />
      </button>
      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
