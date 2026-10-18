/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { configDefaults } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';
// @ts-expect-error — plain .mjs plugin, no types needed
import { floretBus } from './floret-bus.vite.mjs';

export default defineConfig({
  plugins: [react(), floretBus()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
    // Astryx (@astryxdesign/*) is a peer-dep consumer of React. Force a single
    // React/react-dom instance so its hooks share our dispatcher — without this,
    // Vite's dev pre-bundler can hand Astryx a second copy and every Astryx
    // component that calls a hook throws "Invalid hook call".
    dedupe: ['react', 'react-dom'],
  },
  test: {
    // Polyfill OffscreenCanvas (and createImageBitmap) so rasterising
    // nodes run in the DOM-less Node test env instead of throwing. See
    // src/test/setup-canvas.ts.
    setupFiles: ['./src/test/setup-canvas.ts'],
    // Agent worktrees and the native port contain copies / non-TS code.
    exclude: [...configDefaults.exclude, '.*/**', 'native/**'],
  },
  server: {
    port: 5173,
    strictPort: false,
    // Bind to all interfaces so the Pi can pull frames from the e-ink relay.
    host: true,
    watch: {
      // Heavy / unrelated trees in the project root that Vite shouldn't watch.
      // Saves ~hundreds of thousands of file handles and avoids EBUSY on
      // locked .pyc files inside Python venvs.
      ignored: [
        '**/ComfyUI/**',
        '**/.venv/**',
        '**/venv/**',
        '**/__pycache__/**',
        '**/node_modules/**',
        '**/dist/**',
        '**/src-tauri/target/**',
      ],
    },
  },
  clearScreen: false,
});
