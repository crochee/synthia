import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Fix card #008 — SSE reconnect + backpressure.
 *
 * The A2A SSE endpoint (`tasks/{id}:subscribe`) is served by
 * `a2a-server-lf`, which does not currently emit `Retry-After`
 * headers or heartbeat comments. The Synthia internal SSE
 * endpoint (`/api/sessions/{id}/stream`) has heartbeat at
 * 15-second intervals.
 *
 * For the A2A endpoint, reconnect logic lives in the
 * `@a2a-js/sdk` client (which exposes `resubscribeTask`).
 * The frontend should use exponential backoff when reconnecting.
 *
 * This card documents the cadence contract rather than
 * implementing full reconnect logic (which requires upstream
 * changes to `a2a-server-lf`).
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');

describe('fix card #008 — SSE reconnect + backpressure', () => {
  it('Synthia internal SSE has heartbeat', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/sse.rs'),
      'utf8',
    );
    expect(src).toContain('HEARTBEAT_INTERVAL');
    expect(src).toContain(': ping');
    expect(src).toContain('KeepAlive');
  });

  it('contract.yaml subscribe entry documents cadence', () => {
    const { parse: parseYaml } = require('yaml');
    const contract = parseYaml(
      readFileSync(join(ROOT, 'docs/interface-contract/contract.yaml'), 'utf8'),
    );
    const ep = contract.endpoints.find(
      (e: { id: string }) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    expect(ep).toBeDefined();
    // Notes must mention cadence/reconnect.
    const blob = ep.notes ?? '';
    expect(blob).toMatch(/cadence|reconnect|heartbeat|Retry-After/i);
  });

  it('SDK client supports resubscribeTask', () => {
    const src = readFileSync(
      join(ROOT, 'synthia-web/node_modules/@a2a-js/sdk/dist/multitransport-client-D8tp_la5.d.cts'),
      'utf8',
    );
    expect(src).toContain('resubscribeTask');
  });
});
