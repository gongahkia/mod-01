import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    exclude: ['**/dist/**', '**/node_modules/**'],
    maxWorkers: 4,
    testTimeout: 15_000,
  },
});
