import React from 'react';
import ReactDOM from 'react-dom/client';
import { Theme } from '@astryxdesign/core/theme';
import { neutralTheme } from '@astryxdesign/theme-neutral/built';
import { App } from './app/App';
import { usePrefs, resolveTheme } from './app/prefs';
// The instrument's single voice: Commit Mono — a neutral, engineered monospace
// used across the whole app (chrome, nodes, numerics). Self-hosted woff2 via
// fontsource (no CDN, works offline), imported before styles.css so the
// --font-* stacks and the Astryx --font-family-* overrides resolve to it.
// Weights: 400 body · 500 emphasis · 600 headers · 700 strong; italic for hints.
import '@fontsource/commit-mono/400.css';
import '@fontsource/commit-mono/500.css';
import '@fontsource/commit-mono/600.css';
import '@fontsource/commit-mono/700.css';
import '@fontsource/commit-mono/400-italic.css';
// Astryx design-system stylesheets. The @layer cascade is
// reset → astryx-base → astryx-theme, so import order matters. These are
// layered; Floret's own (unlayered) styles.css below still wins during the
// migration, and cedes ground surface-by-surface as chrome moves to Astryx.
import '@astryxdesign/core/reset.css';
import '@astryxdesign/core/astryx.css';
import '@astryxdesign/theme-neutral/theme.css';
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
const prefersLight = () => window.matchMedia('(prefers-color-scheme: light)').matches;
try {
  const t = JSON.parse(localStorage.getItem('floret.prefs.v1') || '{}').theme;
  if (t === 'light') initialMode = 'light';
  else if (t === 'system') initialMode = prefersLight() ? 'light' : 'dark';
} catch { /* default dark */ }
// Set data-theme for BOTH modes before first paint. This drives `color-scheme`
// (via Astryx's reset.css), so Astryx's `light-dark()` tokens resolve correctly
// from the very first frame — including in the top-layer <dialog> — with no
// flash of the wrong mode while React mounts and the Theme provider syncs.
document.documentElement.setAttribute('data-theme', initialMode);

/**
 * Bridges Floret's persisted light/dark preference into Astryx's Theme
 * `mode`. Astryx drives `color-scheme` from `mode`, and Floret's node/app
 * colours now resolve through Astryx's `light-dark()` tokens — so `mode`
 * MUST track the pref reactively, otherwise toggling the theme flips
 * `data-theme` but leaves `color-scheme` (and every bridged token) stuck at
 * the mode captured on load. Subscribing here keeps the two in lock-step.
 */
function Root() {
  const theme = usePrefs((s) => s.theme);
  return (
    <Theme theme={neutralTheme} mode={resolveTheme(theme)}>
      <App />
    </Theme>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
