import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `GET /api/v1/models`.
 *
 * Validates that synthia-server returns 2xx with a models list. This is the
 * stand-in for the plan-listed "agent-card" spec: agent card metadata in
 * Synthia is exposed through the `/api/v1/models` endpoint rather than a
 * dedicated well-known path. Behavioural coverage for the agent card itself
 * lives in `tests/e2e/agent/`.
 */
test.describe('contract-closure /api/v1/models (model list)', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('GET /api/v1/models — backend reachable, list shape', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /api/v1/models');
    test.skip(
      !target,
      '[contract-closure] no GET /api/v1/models in contract.yaml. Run `make contract-scan`.',
    );

    const r = await request.get('/api/v1/models');
    expect(r.status()).toBeLessThan(300);
    const body = await r.json();
    // v1 bare response: { models: [...], default_provider, default_model }.
    // No envelope `status` field.
    expect(Array.isArray(body.models), 'body.models must be an array').toBe(true);
  });
});
