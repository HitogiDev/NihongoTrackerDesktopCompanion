import { defineConfig } from 'vitest/config';

// Unit tests for the frontend's pure logic and stores. Tauri isn't there: tests
// mock `../lib/ipc` and `@tauri-apps/api/event` themselves.
export default defineConfig({
  resolve: { dedupe: ['react', 'react-dom'] },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
});
