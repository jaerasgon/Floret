import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
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

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
