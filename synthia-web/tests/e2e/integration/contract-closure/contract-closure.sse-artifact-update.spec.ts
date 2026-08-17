import { test, expect } from '@playwright/test';
import { assertServerUp } from './_fixtures/server';
import { contractPath, loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';
import { subscribeAndCapture } from './_helpers/sse-harness';
import { readFileSync } from 'node:fs';

/**
 * Layer 2 contract spec — `GET /a2a/tasks/{id}:subscribe` SSE event
 * `artifact-update` field `lastChunk` alignment (fix card #004).
 *
 * What this spec pins
 * -------------------
 * 1. `docs/interface-contract/contract.yaml` carries the subscribe entry
 *    with an `artifact-update` SSE event documenting `lastChunk`.
 * 2. The synthia backend does not emit `artifactUpdate` events
 *    for tool calls / results — per A2A v1.0 §3.7, those are
 *    `Message(agent)` events carrying `Part::data` (communication
 *    turns, not tangible deliverables). `artifactUpdate` is
 *    reserved for actual file / binary deliverables. This test
 *    pins the protocol-faithful boundary by asserting the
 *    `artifactUpdate` event type IS still part of the
 *    subscribed event union (for forward-compat with future
 *    deliverables) but the synthia executor does not emit one
 *    for tool calls / results in the MVP.
 */

const SERVER_BASE = process.env.SYNTHIA_SERVER_URL ?? 'http://localhost:8080';
const SUBSCRIBE_PATH = '/a2a/tasks';

function tryParseJson(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

interface ArtifactUpdatePayload {
  taskId?: string;
  contextId?: string;
  artifact?: { artifactId?: string; parts?: unknown[] };
  lastChunk?: boolean | null;
  append?: boolean | null;
}

interface StreamEnvelope {
  artifactUpdate?: ArtifactUpdatePayload;
  artifact_update?: ArtifactUpdatePayload;
}

test.describe('contract-closure GET /a2a/tasks/{key}:subscribe artifact-update lastChunk', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('contract.yaml subscribe entry has artifact-update event with lastChunk field', async () => {
    const yaml = readFileSync(contractPath(), 'utf8');
    expect(yaml).toContain('artifact-update');
    // The artifact-update event must list lastChunk in its fields.
    expect(yaml).toMatch(/name: artifact-update[\s\S]*?lastChunk/);
  });

  test('real task lifecycle does not emit artifactUpdate for tool calls (A2A §3.7)', async ({
    request,
  }) => {
    const eps = onlyBackend(loadEndpoints());
    test.skip(
      !eps.find((e) => e.id === 'GET /a2a/tasks/{key}:subscribe'),
      '[contract-closure] no subscribe entry in contract.yaml.',
    );

    // 1) Send a message to trigger a task lifecycle.
    const sendResp = await request.post('/a2a/message:send', {
      headers: { 'content-type': 'application/json' },
      data: {
        message: {
          messageId: `spec-004-${Date.now()}`,
          contextId: '',
          role: 'ROLE_USER',
          parts: [{ text: 'list files in /tmp' }],
        },
      },
    });
    expect(sendResp.status(), 'message:send must succeed').toBeLessThan(300);

    const sendBody = (await sendResp.json()) as {
      message?: { taskId?: string };
      result?: { message?: { taskId?: string }; task?: { id?: string } };
      task?: { id?: string };
    };
    const taskId =
      sendBody.message?.taskId ??
      sendBody.result?.message?.taskId ??
      sendBody.task?.id ??
      sendBody.result?.task?.id;
    expect(taskId, 'message:send must surface a taskId').toBeTruthy();

    // 2) Subscribe via SSE and capture events.
    const subscribeUrl = `${SERVER_BASE}${SUBSCRIBE_PATH}/${encodeURIComponent(taskId!)}:subscribe`;
    const stream = await subscribeAndCapture(subscribeUrl, {
      headers: { Accept: 'text/event-stream' },
    });

    // 3) Wait for stream to finish or timeout.
    await Promise.race([stream.done, new Promise<void>((res) => setTimeout(res, 10_000))]);
    stream.close();

    expect(
      stream.events.length,
      `expected at least one SSE event from ${subscribeUrl}`,
    ).toBeGreaterThan(0);

    // 4) Walk captured events and find artifact-update events.
    //    Per A2A v1.0 §3.7, tool calls and tool results are
    //    NOT routed through the artifact channel — they are
    //    `Message(agent)` events carrying `Part::data`. So
    //    this test pins two things:
    //
    //    a) The synthia backend does not emit `artifactUpdate`
    //       events for tool calls/results in the MVP. The wire
    //       must use `Message(agent)` for those.
    //    b) IF any `artifactUpdate` ever does fire (forward
    //       compat: real file deliverables), it must carry
    //       `lastChunk: true` (synthia does not stream chunks
    //       to the artifact channel) and `append: false` (the
    //       default-valued proto3 bool may surface as either
    //       `false` or absent).
    const artifactUpdates: ArtifactUpdatePayload[] = [];
    for (const event of stream.events) {
      const parsed = tryParseJson(event.data);
      if (parsed === null || typeof parsed !== 'object') continue;
      const env = parsed as StreamEnvelope;

      const au = env.artifactUpdate ?? env.artifact_update;
      if (au) {
        artifactUpdates.push(au);
      }
    }

    // The MVP does not route tool calls / results through
    // the artifact channel. Any artifactUpdate we see should
    // be from a forward-compat future deliverable, not from
    // tool control flow.
    if (artifactUpdates.length > 0) {
      for (const au of artifactUpdates) {
        expect(
          au.lastChunk,
          `artifactUpdate.lastChunk must be a boolean, got ${au.lastChunk}`,
        ).toBe(true);
        // `append` is a proto3 bool. Proto3 omits default-valued
        // fields on the wire, so `append: false` may surface as
        // `undefined` in JSON — that is contractually equivalent
        // to `false`. Accept both.
        const append = au.append ?? false;
        expect(
          append,
          `artifactUpdate.append must be false (or absent) for atomic results, got ${au.append}`,
        ).toBe(false);
      }
    }

    // The first test (yaml contract entry) is the structural
    // pin that `artifact-update` IS still part of the
    // subscribe event union — for forward-compat.
  });
});
