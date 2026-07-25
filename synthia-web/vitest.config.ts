import { defineConfig } from 'vitest/config';

/**
 * Vitest configuration for unit tests living under
 * `tests/e2e/integration/contract-closure/_helpers/`.
 *
 * Scope is intentionally narrow: only the helper unit tests (sse-harness,
 * contract-yaml parser, etc.). The Playwright spec files continue to run
 * via `playwright test` and are NOT picked up here.
 */
export default defineConfig({
  test: {
    include: [
      'tests/e2e/integration/contract-closure/_helpers/**/*.test.ts',
    ],
    environment: 'node',
  },
});
