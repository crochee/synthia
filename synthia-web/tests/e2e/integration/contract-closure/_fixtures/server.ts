/**
 * Server-lifecycle fixture for the contract-closure sub-suite.
 *
 * The sub-suite points its Playwright baseURL directly at the running
 * synthia-server. We assume the server is already reachable when tests
 * start (either `make dev` locally or `cargo run -p synthia-server` in CI).
 *
 * This file documents the assumption and exposes a single `assertServerUp`
 * helper that the spec files call in `test.beforeAll`. We do NOT spawn the
 * server inside Playwright: the e2e.yml workflow already launches it as a
 * job step, and spawning it again would race for the same port.
 */

import { request } from '@playwright/test';

const SERVER_BASE = process.env.SYNTHIA_SERVER_URL ?? 'http://localhost:8080';
const HEALTH_PATH = '/readyz';
const READY_TIMEOUT_MS = 30_000;

export async function assertServerUp(): Promise<void> {
  const deadline = Date.now() + READY_TIMEOUT_MS;
  let lastErr: unknown = null;
  while (Date.now() < deadline) {
    try {
      const ctx = await request.newContext({
        baseURL: SERVER_BASE,
        extraHTTPHeaders: { Origin: SERVER_BASE },
      });
      const r = await ctx.get(HEALTH_PATH, { timeout: 2_000 });
      await ctx.dispose();
      if (r.ok()) return;
      lastErr = new Error(`server returned ${r.status()} on ${HEALTH_PATH}`);
    } catch (e) {
      lastErr = e;
    }
    await new Promise((res) => setTimeout(res, 500));
  }
  throw new Error(
    `[contract-closure] synthia-server not reachable at ${SERVER_BASE} ` +
      `(readiness probe ${HEALTH_PATH} did not respond 2xx within ${READY_TIMEOUT_MS}ms). ` +
      `Last error: ${(lastErr as Error)?.message ?? 'unknown'}. ` +
      `Did you forget to run \`make dev\` or \`cargo run -p synthia-server\`?`,
  );
}
