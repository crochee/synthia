import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for synthia-web end-to-end tests.
 *
 * Three layers are covered:
 *   - tests/e2e/ui/         – visual + DOM interactions
 *   - tests/e2e/integration – A2A / REST API contracts
 *   - tests/e2e/agent/      – agent functional logic (end-to-end conversations)
 *
 * The webServer block boots both Vite and cargo automatically
 * when a test is invoked (and reuses an already-running server
 * during local development).
 */
export default defineConfig({
  testDir: './tests/e2e',
  // Only pick up Playwright spec files (`*.spec.ts`). The
  // `sse-harness.test.ts` and similar unit tests under
  // `tests/e2e/integration/contract-closure/_helpers/` import from
  // `vitest` and are run via `npm run test:unit`; including them
  // here makes Playwright crash with
  // "Vitest failed to access its internal state".
  testMatch: /.*\.spec\.ts$/,
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? [['html'], ['github']] : 'list',
  timeout: 120_000,
  expect: {
    timeout: 5_000,
  },
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command:
        'cd /home/crochee/workspace/synthia && cargo run -p synthia-server -- --config config.yaml',
      port: 8080,
      reuseExistingServer: true,
      timeout: 60_000,
    },
    {
      command: 'npm run dev',
      port: 5173,
      reuseExistingServer: true,
      timeout: 30_000,
    },
  ],
});
