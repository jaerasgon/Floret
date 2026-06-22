import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';
import { einkRelay } from './eink-relay.vite.mjs';
export default defineConfig({
    plugins: [react(), einkRelay()],
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
    },
    server: {
        port: 5173,
        strictPort: false,
        // Bind to all interfaces so a Raspberry Pi on the LAN can pull
        // /__floret/frame.png from the e-ink relay above.
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
