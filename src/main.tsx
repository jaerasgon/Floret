import React from 'react';
import ReactDOM from 'react-dom/client';
import { Theme } from '@astryxdesign/core/theme';
import { butterTheme } from '@astryxdesign/theme-butter/built';
import { App } from './app/App';
// Bundled type system (self-hosted woff2 via fontsource — no CDN, works offline):
// Space Grotesk for display/brand, Hanken Grotesk for UI body, JetBrains Mono for
// values. Imported before styles.css so the --font-* stacks resolve to these.
import '@fontsource-variable/space-grotesk';
import '@fontsource-variable/hanken-grotesk';
import '@fontsource-variable/jetbrains-mono';
// Astryx design-system stylesheets. The @layer cascade is
// reset → astryx-base → astryx-theme, so import order matters. These are
// layered; Floret's own (unlayered) styles.css below still wins during the
// migration, and cedes ground surface-by-surface as chrome moves to Astryx.
import '@astryxdesign/core/reset.css';
import '@astryxdesign/core/astryx.css';
import '@astryxdesign/theme-butter/theme.css';
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
// Floret and Astryx share the same `data-theme` attribute on <html>, so the
// two theme systems stay in lock-step — we seed Astryx's mode from the same
// persisted pref below.
let initialMode: 'light' | 'dark' = 'dark';
try {
  if (JSON.parse(localStorage.getItem('floret.prefs.v1') || '{}').theme === 'light') {
    initialMode = 'light';
  }
} catch { /* default dark */ }
// Set data-theme for BOTH modes before first paint. This drives `color-scheme`
// (via Astryx's reset.css), so Astryx's `light-dark()` tokens resolve correctly
// from the very first frame — including in the top-layer <dialog> — with no
// flash of the wrong mode while React mounts and the Theme provider syncs.
document.documentElement.setAttribute('data-theme', initialMode);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Theme theme={butterTheme} mode={initialMode}>
      <App />
    </Theme>
  </React.StrictMode>,
);
