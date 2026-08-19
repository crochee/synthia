import { test, expect } from '@playwright/test';

/**
 * Layer 2 — W3C TraceContext integration tests.
 *
 * Validates that the server implements the W3C TraceContext — Level 1
 * recommendation (`traceparent` / `tracestate`) and exposes the
 * short-form `x-trace-id` header for log aggregators.
 *
 * The test targets the Rust server directly (port 8080) to keep
 * the assertions independent of the Vite dev proxy, which would
 * otherwise normalize or strip these headers.
 *
 * Spec: https://www.w3.org/TR/trace-context/
 */
const SERVER_URL = 'http://localhost:8080';

// W3C `traceparent` regex: `vv-trace_id-parent_id-flags`,
// vv=2, trace_id=32, parent_id=16, flags=2 (all hex).
const TRACEPARENT_PATTERN = /^([0-9a-f]{2})-([0-9a-f]{32})-([0-9a-f]{16})-([0-9a-f]{2})$/;

// trace_id must be 32 hex chars (non-zero per W3C spec).

// span_id is 16 hex chars.
const NON_ZERO_HEX_16 = /^[0-9a-f]{16}$/;

test.describe('W3C TraceContext propagation', () => {
  // Tests in this block hit an authenticated management endpoint
  // (`/api/v1/skills`) so they exercise the full middleware chain.
  // Public endpoints (`/livez`, `/readyz`, `/.well-known/agent-card.json`)
  // intentionally bypass the trace-context middleware; their
  // behaviour is asserted in the nested describe block at the
  // bottom of this file.
  const SAMPLE_PATH = '/api/v1/skills';

  test('response carries a fresh traceparent when the request has none', async ({ request }) => {
    const response = await request.get(`${SERVER_URL}${SAMPLE_PATH}`);
    expect(response.ok()).toBe(true);

    const traceparent = response.headers()['traceparent'];
    expect(traceparent, 'traceparent header must be present').toBeTruthy();

    const match = TRACEPARENT_PATTERN.exec(traceparent);
    expect(match, `traceparent=${traceparent} should match W3C format`).not.toBeNull();

    const [, , traceId, spanId, flags] = match!;
    // The trace id and span id must not be all zero.
    expect(traceId).not.toBe('00000000000000000000000000000000');
    expect(spanId).not.toBe('0000000000000000');
    // Flags must be valid (00 or 01 — sampled vs not-sampled).
    expect(['00', '01']).toContain(flags);

    // The short-form x-trace-id must equal the trace-id segment.
    expect(response.headers()['x-trace-id']).toBe(traceId);
  });

  test('upstream traceparent is preserved end-to-end', async ({ request }) => {
    const upstreamTraceparent = '00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01';
    const response = await request.get(`${SERVER_URL}${SAMPLE_PATH}`, {
      headers: { traceparent: upstreamTraceparent },
    });
    expect(response.ok()).toBe(true);

    const echoed = response.headers()['traceparent'];
    expect(echoed, 'echoed traceparent must be present').toBeTruthy();
    const match = TRACEPARENT_PATTERN.exec(echoed);
    expect(match, `traceparent=${echoed}`).not.toBeNull();

    const [, , traceId, parentSpanId, flags] = match!;
    // Upstream trace id must be preserved.
    expect(traceId).toBe('0af7651916cd43dd8448eb211c80319c');
    // Upstream flags must be preserved.
    expect(flags).toBe('01');
    // The parent span id in the echoed traceparent must NOT equal
    // the upstream one — it should be the server's locally
    // generated span id, so downstream services see us as the
    // parent.
    expect(parentSpanId).not.toBe('b7ad6b7169203331');
    expect(parentSpanId).toMatch(NON_ZERO_HEX_16);
    // The short-form x-trace-id must match the preserved trace id.
    expect(response.headers()['x-trace-id']).toBe(traceId);
  });

  test('tracestate is passed through verbatim', async ({ request }) => {
    const tracestate = 'vendor1=value1,vendor2=value2';
    const response = await request.get(`${SERVER_URL}${SAMPLE_PATH}`, {
      headers: { tracestate },
    });
    expect(response.ok()).toBe(true);
    expect(response.headers()['tracestate']).toBe(tracestate);
  });

  test('malformed traceparent is ignored and a fresh trace is minted', async ({ request }) => {
    const response = await request.get(`${SERVER_URL}${SAMPLE_PATH}`, {
      headers: { traceparent: 'this is not a traceparent' },
    });
    expect(response.ok()).toBe(true);
    const tp = response.headers()['traceparent'];
    expect(tp).toBeTruthy();
    // The malformed value must not be echoed verbatim.
    expect(tp).not.toContain('this is not a traceparent');
    // The minted value must be a valid W3C traceparent.
    expect(TRACEPARENT_PATTERN.exec(tp)).not.toBeNull();
  });

  test('every authenticated management API endpoint emits a traceparent', async ({ request }) => {
    // Public endpoints (`/livez`, `/readyz`, `/.well-known/agent-card.json`)
    // intentionally bypass tracing — see the dedicated tests below.
    // Everything else must carry W3C TraceContext.
    const endpoints = ['/api/v1/skills', '/api/v1/tools', '/api/v1/tasks'];

    for (const path of endpoints) {
      const response = await request.get(`${SERVER_URL}${path}`);
      expect(response.ok(), `${path} should respond 2xx`).toBe(true);
      const tp = response.headers()['traceparent'];
      expect(tp, `${path} should emit traceparent`).toBeTruthy();
      expect(TRACEPARENT_PATTERN.exec(tp), `${path} traceparent=${tp}`).not.toBeNull();
      // The full set of correlation headers must be present on
      // every authenticated endpoint.
      expect(response.headers()['x-trace-id']).toBeTruthy();
      expect(response.headers()['x-request-time-ms']).toBeTruthy();
    }
  });

  test('authenticated response carries timing header and never the legacy X-Request-ID', async ({
    request,
  }) => {
    // Correlation is unified on W3C TraceContext: the legacy
    // X-Request-ID header has been retired to avoid two parallel
    // schemes competing for the same role. This test guards the
    // retirement so the header does not silently come back.
    const response = await request.get(`${SERVER_URL}/api/v1/skills`);
    expect(response.ok()).toBe(true);

    expect(
      response.headers()['x-request-id'],
      'x-request-id must not be emitted; use x-trace-id + traceparent instead',
    ).toBeUndefined();
    expect(
      response.headers()['X-Request-ID'],
      'X-Request-ID must not be emitted (case-insensitive check)',
    ).toBeUndefined();

    // x-request-time-ms is the access-log timing surface; its
    // presence guards the request-tracing layer.
    const elapsedHeader = response.headers()['x-request-time-ms'];
    expect(elapsedHeader, 'x-request-time-ms must be present').toBeTruthy();
    const elapsedMs = Number.parseFloat(elapsedHeader);
    expect(elapsedMs).toBeGreaterThanOrEqual(0);
    expect(elapsedMs).toBeLessThan(2000);
  });

  test.describe('public endpoints bypass tracing', () => {
    // `/livez`, `/readyz` and `/.well-known/agent-card.json` are
    // deliberately mounted outside the trace-context / access-log
    // middleware chain. They are hit by orchestrators and external
    // scanners many times per second; emitting a trace id per call
    // floods the log pipeline without producing useful correlation.

    test('/livez does not emit trace headers', async ({ request }) => {
      const response = await request.get(`${SERVER_URL}/livez`);
      expect(response.ok()).toBe(true);
      expect(response.headers()['traceparent']).toBeUndefined();
      expect(response.headers()['x-trace-id']).toBeUndefined();
      expect(response.headers()['x-request-time-ms']).toBeUndefined();
    });

    test('/.well-known/agent-card.json does not emit trace headers', async ({ request }) => {
      const response = await request.get(`${SERVER_URL}/.well-known/agent-card.json`);
      expect(response.ok()).toBe(true);
      expect(response.headers()['traceparent']).toBeUndefined();
      expect(response.headers()['x-trace-id']).toBeUndefined();
      expect(response.headers()['x-request-time-ms']).toBeUndefined();
      // The card itself must still be valid A2A.
      const body = await response.json();
      expect(body.name).toBeTruthy();
      expect(Array.isArray(body.supportedInterfaces)).toBe(true);
    });

    test('/readyz still serves CORS for cross-origin probes', async ({ request }) => {
      // Public endpoints still need CORS — only the trace layer
      // is bypassed. Verify the CORS layer remains active.
      const response = await request.get(`${SERVER_URL}/readyz`, {
        headers: { origin: 'https://example.com' },
      });
      expect(response.ok()).toBe(true);
      expect(response.headers()['access-control-allow-origin']).toBe('*');
    });
  });
});
