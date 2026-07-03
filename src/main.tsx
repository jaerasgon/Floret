import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
// Bundled type system (self-hosted woff2 via fontsource — no CDN, works offline):
// Space Grotesk for display/brand, Hanken Grotesk for UI body, JetBrains Mono for
// values. Imported before styles.css so the --font-* stacks resolve to these.
import '@fontsource-variable/space-grotesk';
import '@fontsource-variable/hanken-grotesk';
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
try {
  if (JSON.parse(localStorage.getItem('floret.prefs.v1') || '{}').theme === 'light') {
    document.documentElement.setAttribute('data-theme', 'light');
  }
} catch { /* default dark */ }

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
