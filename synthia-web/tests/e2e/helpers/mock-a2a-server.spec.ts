/**
 * Pure-function tests for the mock-a2a-server helper. These
 * run under Playwright's node test loader (no browser) and
 * pin the helper's contract before e2e tests depend on it.
 */
import { expect, test } from '@playwright/test';
import { buildAgentCard, buildMockA2AFetch, streamToSSEResponse } from './mock-a2a-server';

test('buildMockA2AFetch returns agent-card JSON for the discovery path', async () => {
  const fetchImpl = buildMockA2AFetch({ streamEvents: [] });
  const res = await fetchImpl('http://localhost/.well-known/agent-card.json');
  expect(res.status).toBe(200);
  expect(res.headers.get('content-type')).toContain('application/json');
  const body = await res.json();
  expect(body.protocolVersion).toBe('1.0');
  expect(body.supportedInterfaces[0].url).toBe('http://localhost/a2a');
  expect(body.capabilities.streaming).toBe(true);
});

test('buildMockA2AFetch returns SSE for any other URL', async () => {
  const fetchImpl = buildMockA2AFetch({
    streamEvents: [{ data: 'hello' }, { data: 'world' }],
  });
  const res = await fetchImpl('http://localhost/a2a', {
    method: 'POST',
    body: '{}',
  });
  expect(res.status).toBe(200);
  expect(res.headers.get('content-type')).toBe('text/event-stream');
  const body = await res.text();
  expect(body).toContain('data: hello');
  expect(body).toContain('data: world');
});

test('buildMockA2AFetch recognizes agent-card path under subpaths', async () => {
  const fetchImpl = buildMockA2AFetch({ streamEvents: [] });
  const res = await fetchImpl('http://localhost/a2a/.well-known/agent-card.json');
  expect(res.headers.get('content-type')).toContain('application/json');
});

test('buildMockA2AFetch honors agentCard.url override', async () => {
  const fetchImpl = buildMockA2AFetch({
    agentCard: { url: 'http://override.example/a2a' },
    streamEvents: [],
  });
  const res = await fetchImpl('http://override.example/.well-known/agent-card.json');
  const body = await res.json();
  expect(body.url).toBe('http://override.example/a2a');
  expect(body.supportedInterfaces[0].url).toBe('http://override.example/a2a');
});

test('streamToSSEResponse with empty events yields an empty body', async () => {
  const res = streamToSSEResponse([]);
  expect(res.status).toBe(200);
  expect(await res.text()).toBe('');
});

test('streamToSSEResponse splits multi-line data into separate data: lines', async () => {
  const res = streamToSSEResponse([{ data: 'line1\nline2' }]);
  const body = await res.text();
  expect(body).toBe('data: line1\ndata: line2\n\n');
});

test('streamToSSEResponse emits an event: line when event is set', async () => {
  const res = streamToSSEResponse([{ event: 'artifactUpdate', data: '{"x":1}' }]);
  const body = await res.text();
  expect(body.startsWith('event: artifactUpdate\n')).toBe(true);
  expect(body).toContain('data: {"x":1}');
  expect(body.endsWith('\n\n')).toBe(true);
});

test('streamToSSEResponse separates frames with a blank line terminator', async () => {
  const res = streamToSSEResponse([{ data: 'a' }, { data: 'b' }]);
  const body = await res.text();
  // Each frame ends with a blank line (\n\n); two frames = two terminators.
  const terminatorCount = (body.match(/\n\n/g) ?? []).length;
  expect(terminatorCount).toBe(2);
});

test('buildAgentCard returns the expected v1.0 fixture shape', () => {
  const card = buildAgentCard() as Record<string, unknown>;
  expect(card.name).toBe('Mock Agent');
  expect(card.protocolVersion).toBe('1.0');
  expect(Array.isArray(card.supportedInterfaces)).toBe(true);
  expect((card.capabilities as { streaming: boolean }).streaming).toBe(true);
});
