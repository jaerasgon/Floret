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
import IconButton from '@mui/material/IconButton';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import Tooltip from '@mui/material/Tooltip';
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

  const onPanelChange = (_e: React.MouseEvent<HTMLElement>, value: string[]) => {
    const next = new Set(value);
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
      <ToggleButtonGroup size="small" value={panelValue} onChange={onPanelChange} aria-label="Panels">
        <ToggleButton value="library" aria-label={libraryVisible ? 'Hide library' : 'Show library'}>
          <Tooltip title={libraryVisible ? 'Hide library' : 'Show library'}>
            <LibraryIcon size={ICON} strokeWidth={1.6} />
          </Tooltip>
        </ToggleButton>
        <ToggleButton value="nodes" aria-label={paletteVisible ? 'Hide nodes' : 'Show nodes'}>
          <Tooltip title={paletteVisible ? 'Hide nodes' : 'Show nodes'}>
            <NodesIcon size={ICON} strokeWidth={1.6} />
          </Tooltip>
        </ToggleButton>
        <ToggleButton value="help" aria-label={helpVisible ? 'Hide help' : 'Show help'}>
          <Tooltip title={helpVisible ? 'Hide help' : 'Show help'}>
            <HelpIcon size={ICON} strokeWidth={1.6} />
          </Tooltip>
        </ToggleButton>
      </ToggleButtonGroup>
      <Tooltip title={theme === 'light' ? 'Switch to dark theme' : 'Switch to light theme'}>
        <IconButton
          size="small"
          aria-label="Toggle colour theme"
          onClick={() => setPref('theme', theme === 'light' ? 'dark' : 'light')}
        >
          {theme === 'light' ? <Moon size={ICON} strokeWidth={1.6} /> : <Sun size={ICON} strokeWidth={1.6} />}
        </IconButton>
      </Tooltip>
      <div className="spacer" />
      <Tooltip title="Auto-arrange nodes  ⇧T">
        <IconButton size="small" aria-label="Auto-arrange nodes" onClick={tidy}>
          <LayoutGrid size={ICON} strokeWidth={1.6} />
        </IconButton>
      </Tooltip>
      <Tooltip title={wireFlow ? 'Hide wire flow' : 'Animate wires'}>
        <ToggleButton
          size="small"
          value="wire-flow"
          selected={wireFlow}
          aria-label={wireFlow ? 'Hide wire flow' : 'Animate wires'}
          onChange={() => setPref('wireFlowAnimation', !wireFlow)}
        >
          <Waves size={ICON} strokeWidth={1.6} />
        </ToggleButton>
      </Tooltip>
      <Tooltip title="Restart scene — reboot runtime state (particles respawn, trails clear)">
        <IconButton size="small" aria-label="Restart scene" onClick={refresh}>
          <RotateCcw size={ICON - 1} strokeWidth={1.8} />
        </IconButton>
      </Tooltip>
      <Tooltip title={firstRenderer ? 'Focus mode — present the art full-screen' : 'Add a Renderer node to use focus mode'}>
        {/* span keeps the Tooltip working while the button is disabled */}
        <span>
          <IconButton
            size="small"
            aria-label="Enter focus mode"
            disabled={!firstRenderer}
            onClick={() => firstRenderer && enterFocus(firstRenderer.id)}
          >
            <Maximize2 size={ICON} strokeWidth={1.6} />
          </IconButton>
        </span>
      </Tooltip>
      <Tooltip title="Settings">
        <IconButton size="small" aria-label="Settings" onClick={() => setSettingsOpen(true)}>
          <SettingsIcon size={ICON} strokeWidth={1.6} />
        </IconButton>
      </Tooltip>
      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
