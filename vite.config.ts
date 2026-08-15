import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    // `@nihongotracker/yomitan-content` is a linked path dependency with its
    // own dev-only React (for its tests). Without deduping, Rollup resolves the
    // JSX runtime from inside the package and the build fails with
    // `"jsx" is not exported by react/jsx-runtime`.
    dedupe: ['react', 'react-dom'],
  },

  // Tauri expects a fixed port and fails if it is not available.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
}));
