import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `GET /api/tasks` (REST list).
 *
 * Validates that:
 *   - the frontend calls `GET /api/tasks` (via `api.get('/api/tasks')`)
 *   - synthia-server returns 2xx with a list-shaped JSON body.
 *
 * Auto-skips when contract.yaml doesn't have this entry yet (run
 * `make contract-scan` to refresh).
 */
test.describe('contract-closure /api/tasks (REST list)', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('GET /api/tasks — backend reachable, list shape', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /api/tasks');
    test.skip(
      !target,
      '[contract-closure] no GET /api/tasks in contract.yaml. Run `make contract-scan`.',
    );

    const r = await request.get('/api/tasks');
    expect(
      r.status(),
      `${target!.source_files?.backend?.join(',') ?? ''} must respond 2xx`,
    ).toBeLessThan(300);
    const body = await r.json();
    // Envelope: { status, data: { tasks: [...] } } — see Server REST envelope helper.
    expect(body).toHaveProperty('status', 'ok');
    expect(body).toHaveProperty('data');
  });
});
