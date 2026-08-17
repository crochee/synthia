import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `GET /api/v1/tasks` (REST list).
 *
 * Validates that:
 *   - the frontend calls `GET /api/v1/tasks` (via `api.get('/api/v1/tasks')`)
 *   - synthia-server returns 2xx with a v1 bare `List<T>` body.
 *
 * Auto-skips when contract.yaml doesn't have this entry yet (run
 * `make contract-scan` to refresh).
 */
test.describe('contract-closure /api/v1/tasks (REST list)', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('GET /api/v1/tasks — backend reachable, list shape', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /api/v1/tasks');
    test.skip(
      !target,
      '[contract-closure] no GET /api/v1/tasks in contract.yaml. Run `make contract-scan`.',
    );

    const r = await request.get('/api/v1/tasks');
    expect(
      r.status(),
      `${target!.source_files?.backend?.join(',') ?? ''} must respond 2xx`,
    ).toBeLessThan(300);
    const body = await r.json();
    // v1 bare response: List<T> = { data: [...], next_cursor?, total? }.
    // No envelope `status` field.
    expect(Array.isArray(body.data), 'body.data must be an array').toBe(true);
  });
});
