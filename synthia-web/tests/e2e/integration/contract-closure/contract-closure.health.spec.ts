import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — probe endpoints `GET /livez` + `GET /readyz`.
 *
 * Validates that synthia-server exposes both k8s-style probes and that
 * each responds 2xx with a body containing a `status` field. Loose on
 * shape because the contract sub-suite enforces wiring, not feature
 * behavior (the latter lives in tests/e2e/{agent,ui}/).
 *
 * If `contract.yaml` has no probe entries, the corresponding check is
 * skipped — actionable via `make contract-scan` rather than silently
 * passing.
 */
test.describe('contract-closure probes', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  for (const path of ['/livez', '/readyz'] as const) {
    test(`GET ${path} reachable with status field`, async ({ request }) => {
      const eps = onlyBackend(loadEndpoints());
      const target = eps.find((e) => e.id === `GET ${path}`);
      test.skip(
        !target,
        `[contract-closure] no GET ${path} in contract.yaml. Run \`make contract-scan\`.`,
      );

      const r = await request.get(path);
      expect(r.status(), `${path} must respond 2xx`).toBeLessThan(300);
      const body = await r.json();
      expect(body).toHaveProperty('status');
    });
  }
});
