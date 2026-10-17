import { defineConfig, devices } from '@playwright/test';
import process from 'node:process';

const previewPort = Number(process.env.MOD01_PREVIEW_PORT ?? 4173);
const previewUrl = `http://127.0.0.1:${String(previewPort)}`;

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,
  workers: 1,
  reporter: 'line',
  timeout: 240_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: previewUrl,
    serviceWorkers: 'allow',
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: {
    command: `pnpm --dir apps/studio exec vite preview --host 127.0.0.1 --port ${String(previewPort)} --strictPort`,
    url: previewUrl,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
