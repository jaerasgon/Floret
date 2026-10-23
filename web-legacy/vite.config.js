import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';
import { floretBus } from './floret-bus.vite.mjs';
export default defineConfig({
    plugins: [react(), floretBus()],
  // The images, fonts and MediaPipe files live at the repository root, where
  // the native app reads them too.
  publicDir: '../public',
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
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
