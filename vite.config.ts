import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        settings: resolve(__dirname, 'settings.html'),
      },
    },
  },
  clearScreen: false,
  server: {
    strictPort: true,
    port: 1420,
  },
  envPrefix: ['VITE_', 'TAURI_'],
});
