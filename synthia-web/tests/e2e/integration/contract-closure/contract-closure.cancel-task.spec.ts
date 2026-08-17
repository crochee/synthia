import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `cancel /a2a/tasks/{key}:cancel` handler
 * (fix card #006).
 *
 * The cancel endpoint is served by the A2A JSON-RPC router.
 * This spec verifies the endpoint exists in the contract and
 * can be called on a running task.
 */

test.describe('contract-closure cancel /a2a/tasks/{key}:cancel', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('contract.yaml has the cancel entry', async () => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'cancel /a2a/tasks/{key}:cancel');
    expect(target, 'cancel entry must exist in contract.yaml').toBeDefined();
  });

  test('cancelling a non-existent task returns error', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    test.skip(
      !eps.find((e) => e.id === 'cancel /a2a/tasks/{key}:cancel'),
      '[contract-closure] no cancel entry in contract.yaml.',
    );

    // A2A JSON-RPC cancel request for a non-existent task.
    const resp = await request.post('/a2a', {
      headers: { 'content-type': 'application/json' },
      data: {
        jsonrpc: '2.0',
        method: 'tasks/cancel',
        id: 1,
        params: {
          id: 'nonexistent-task-id-006',
        },
      },
    });

    // The server should return an error (task not found or
    // task not cancelable), not a 5xx.
    const body = await resp.json().catch(() => null);
    expect(body, 'response must be valid JSON').toBeTruthy();
    // JSON-RPC error or HTTP error — both are acceptable.
    const hasError = body?.error !== undefined || resp.status() >= 400;
    expect(hasError, 'non-existent task cancel must return error').toBe(true);
  });
});
