import React from 'react';
import ReactDOM from 'react-dom/client';
import { ThemeProvider } from '@mui/material/styles';
import GlobalStyles from '@mui/material/GlobalStyles';
import { App } from './app/App';
import { usePrefs } from './app/prefs';
import { makeM3Theme } from './app/theme/muiTheme';
import { m3Light, m3Dark, mdVars } from './app/theme/m3Tokens';
// Roboto — Material 3's type family — self-hosted via fontsource (no CDN).
import '@fontsource/roboto/300.css';
import '@fontsource/roboto/400.css';
import '@fontsource/roboto/500.css';
import '@fontsource/roboto/700.css';
// JetBrains Mono is still used for node/value monospace (--font-mono).
import '@fontsource-variable/jetbrains-mono';
import './app/styles.css';
import '@xyflow/react/dist/style.css';

// Side-effect import: registers every built-in NodeDef.
import './nodes';

// Side-effect import: instantiates the history stack and wires it into the
// editor store. Must come AFTER ./nodes so the registry is populated.
import './editor/history';

// Browser autoplay policy: resume the AudioContext on first user gesture.
import { installAudioStartHook } from './runtime/audio';
installAudioStartHook();

// Request Web MIDI access at boot. Resolves silently (Chromium) or
// fails silently (Safari); MIDI nodes return 0 until a value arrives.
import { installMidiHook } from './runtime/midi';
installMidiHook();

// Register the built-in sample libraries with the library system. Must
// come AFTER ./nodes (samples reference NodeDefs by type) and BEFORE
// the App mounts (so the Library dropdown sees them on first open).
import { registerBuiltInLibraries } from './persistence/library';
import { buildBuiltInSampleLibraries } from './persistence/samples';
registerBuiltInLibraries(buildBuiltInSampleLibraries());

// Seed the default dock layout on first run (no-op if a layout is saved).
import { seedDefaultLayout } from './editor/dock/dockStore';
seedDefaultLayout();

// Apply the persisted colour theme before first paint so light-theme users
// don't get a flash of dark. The Toolbar keeps data-theme in sync afterwards.
// `data-theme` on <html> selects which set of --md-sys-color-* vars applies
// (see the GlobalStyles below), so seeding it here avoids a first-frame flash.
let initialMode: 'light' | 'dark' = 'dark';
try {
  if (JSON.parse(localStorage.getItem('floret.prefs.v1') || '{}').theme === 'light') {
    initialMode = 'light';
  }
} catch { /* default dark */ }
document.documentElement.setAttribute('data-theme', initialMode);

/**
 * Emits the Material 3 colour roles as `--md-sys-color-*` CSS variables for
 * both schemes. `data-theme` (owned by Floret's prefs) selects which set is
 * live, and Floret's node token bridge in styles.css consumes these vars — so
 * the node graph tracks the same Material palette as the MUI chrome. The MUI
 * theme is built from the very same m3Tokens, keeping a single source of truth.
 */
const MD_VAR_STYLES = {
  ':root': { colorScheme: 'light', ...mdVars(m3Light) },
  ":root[data-theme='dark']": { colorScheme: 'dark', ...mdVars(m3Dark) },
} as const;

/**
 * Floret owns the light/dark preference (persisted in its prefs store, mirrored
 * onto <html data-theme>). The MUI theme is recomputed from that pref so its
 * components resolve to the matching Material 3 role values.
 */
function Root() {
  const mode = usePrefs((s) => s.theme);
  const theme = React.useMemo(() => makeM3Theme(mode), [mode]);
  return (
    <ThemeProvider theme={theme}>
      <GlobalStyles styles={MD_VAR_STYLES} />
      <App />
    </ThemeProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
