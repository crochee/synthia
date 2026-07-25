import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `GET /health`.
 *
 * Validates that synthia-server exposes a `/health` endpoint that responds
 * 2xx with a body containing a `status` field. Loose on shape because the
 * contract sub-suite enforces wiring, not feature behavior (the latter
 * lives in tests/e2e/{agent,ui}/).
 *
 * If `contract.yaml` has no `GET /health` entry, this spec is skipped —
 * actionable via `make contract-scan` rather than silently passing.
 */
test.describe('contract-closure /health', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('GET /health endpoint reachable with status field', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /health');
    test.skip(
      !target,
      '[contract-closure] no GET /health in contract.yaml. Run `make contract-scan`.',
    );

    const r = await request.get('/health');
    expect(r.status(), 'health must respond 2xx').toBeLessThan(300);
    const body = await r.json();
    expect(body).toHaveProperty('status');
  });
});
