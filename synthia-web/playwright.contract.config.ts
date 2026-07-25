import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration dedicated to the contract-closure sub-suite.
 *
 * Runs ONLY synthia-web/tests/e2e/integration/contract-closure/
 * and assumes the synthia-server is already reachable at
 * http://localhost:8080 (either started manually or via make dev). It does
 * NOT start Vite. These tests hit the backend directly.
 *
 * Usage:
 *   - Local: npm run test:contract-closure
 *            (or npx playwright test --config=playwright.contract.config.ts)
 *   - CI:    see .github/workflows/contract-closure.yml
 */
export default defineConfig({
  testDir: './tests/e2e/integration/contract-closure',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [['github'], ['html']] : 'list',
  timeout: 30_000,
  expect: {
    timeout: 5_000,
  },
  use: {
    baseURL: 'http://localhost:8080',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    extraHTTPHeaders: {
      // ensure CORS preflight doesn't drop the request even if the dev proxy
      // isn't available
      Origin: 'http://localhost:8080',
    },
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
