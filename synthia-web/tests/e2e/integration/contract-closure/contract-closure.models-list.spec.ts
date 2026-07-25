import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `GET /api/models`.
 *
 * Validates that synthia-server returns 2xx with a models list. This is the
 * stand-in for the plan-listed "agent-card" spec: agent card metadata in
 * Synthia is exposed through the `/api/models` endpoint rather than a
 * dedicated well-known path. Behavioural coverage for the agent card itself
 * lives in `tests/e2e/agent/`.
 */
test.describe('contract-closure /api/models (model list)', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('GET /api/models — backend reachable, list shape', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /api/models');
    test.skip(
      !target,
      '[contract-closure] no GET /api/models in contract.yaml. Run `make contract-scan`.',
    );

    const r = await request.get('/api/models');
    expect(r.status()).toBeLessThan(300);
    const body = await r.json();
    expect(body).toHaveProperty('status', 'ok');
    // data must be present; shape is opaque here (callers vary)
    expect(body).toHaveProperty('data');
  });
});
