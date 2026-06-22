/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';
// @ts-expect-error — plain .mjs plugin, no types needed
import { einkRelay } from './eink-relay.vite.mjs';

export default defineConfig({
  plugins: [react(), einkRelay()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    // Polyfill OffscreenCanvas (and createImageBitmap) so rasterising
    // nodes run in the DOM-less Node test env instead of throwing. See
    // src/test/setup-canvas.ts.
    setupFiles: ['./src/test/setup-canvas.ts'],
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
