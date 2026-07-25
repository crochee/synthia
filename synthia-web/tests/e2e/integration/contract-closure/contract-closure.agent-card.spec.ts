import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `GET /.well-known/agent-card.json`.
 *
 * Validates that synthia-server exposes a well-known A2A AgentCard document
 * whose shape matches the A2A v1.0 protocol expectations referenced in
 * `docs/interface-contract/ARBITRATION.md`. Field-level pinning is intentionally
 * loose (we only assert the documented top-level keys exist), leaving exact
 * semantic validation to the A2A SDK type tests in
 * `synthia-web/node_modules/@a2a-js/sdk/...`.
 *
 * Per the ARBITRATION priority (A2A official > @a2a-js/sdk > local spec),
 * the source of truth for these keys is the A2A v1.0 protocol; if upstream
 * changes the shape, this spec must be updated to match.
 */
test.describe('contract-closure /.well-known/agent-card.json', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('GET /.well-known/agent-card.json — shape conformance', async ({ request }) => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /.well-known/agent-card.json');
    test.skip(
      !target,
      '[contract-closure] no GET /.well-known/agent-card.json in contract.yaml. Run `make contract-scan`.',
    );

    const r = await request.get('/.well-known/agent-card.json');
    expect(r.status()).toBeLessThan(300);
    expect(r.headers()['content-type']).toMatch(/application\/json/);

    const body = await r.json();
    // A2A v1.0 mandatory top-level fields.
    for (const key of ['name', 'description', 'version', 'supportedInterfaces', 'capabilities']) {
      expect(body, `agent-card must contain "${key}"`).toHaveProperty(key);
    }
    expect(typeof body.name).toBe('string');
    expect(typeof body.description).toBe('string');
    expect(typeof body.version).toBe('string');
    expect(Array.isArray(body.supportedInterfaces)).toBe(true);
    expect(typeof body.capabilities).toBe('object');
    // streaming capability is required for Synthia's chat flow.
    expect(body.capabilities).toHaveProperty('streaming');
  });
});
