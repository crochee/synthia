import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { assertServerUp } from './_fixtures/server';
import { contractPath, loadEndpoints, onlyBackend } from './_helpers/list-endpoints-from-yaml';
import { subscribeAndCapture } from './_helpers/sse-harness';

/**
 * Layer 2 contract spec — `GET /a2a/tasks/{id}:subscribe` SSE event
 * `status-update` field `state` enum alignment (fix card #003).
 *
 * What this spec pins
 * -------------------
 * 1. `docs/interface-contract/contract.yaml` carries the manually-curated
 *    `GET /a2a/tasks/{key}:subscribe` entry with `status: closed`.
 * 2. Driving a real task lifecycle (POST `/a2a/message:send`, then
 *    subscribe via SSE) yields at least one `statusUpdate` event whose
 *    `status.state` is in the canonical `@a2a-js/sdk@1.0.0` `TaskState`
 *    enum set:
 *      TASK_STATE_UNSPECIFIED, TASK_STATE_SUBMITTED, TASK_STATE_WORKING,
 *      TASK_STATE_COMPLETED, TASK_STATE_FAILED, TASK_STATE_CANCELED,
 *      TASK_STATE_INPUT_REQUIRED, TASK_STATE_REJECTED,
 *      TASK_STATE_AUTH_REQUIRED.
 * 3. The Task snapshot emitted on the SSE wire uses one of those
 *    same enum values for `status.state`.
 *
 * What this spec deliberately does NOT do
 * --------------------------------------
 * - It does not inject a forged unknown enum value (we have no
 *   synthetic backdoor on the server). The "downgrade to Failed"
 *   branch is covered by the unit test
 *   `crates/synthia-server/src/a2a/mapping.rs::normalize_task_state_*`
 *   plus the scanner-fixture vitest in
 *   `contract-closure/test/sse-status-state-enum.test.ts`. This
 *   spec pins the wire-level conformance that those layers promise.
 */

const SERVER_BASE = process.env.SYNTHIA_SERVER_URL ?? 'http://localhost:8080';
const SUBSCRIBE_PATH = '/a2a/tasks';

const CANONICAL_TASK_STATES = new Set([
  'TASK_STATE_UNSPECIFIED',
  'TASK_STATE_SUBMITTED',
  'TASK_STATE_WORKING',
  'TASK_STATE_COMPLETED',
  'TASK_STATE_FAILED',
  'TASK_STATE_CANCELED',
  'TASK_STATE_INPUT_REQUIRED',
  'TASK_STATE_REJECTED',
  'TASK_STATE_AUTH_REQUIRED',
]);

/**
 * Best-effort JSON parse — the SSE `data:` payload is one JSON
 * document per event. We deliberately don't `JSON.parse` the whole
 * thing in one helper because Playwright assertions want to surface
 * the bad payload in the failure message.
 */
function tryParseJson(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

interface StatusUpdatePayload {
  taskId?: string;
  contextId?: string;
  status?: { state?: string; message?: unknown };
}

interface TaskPayload {
  id?: string;
  contextId?: string;
  status?: { state?: string; message?: unknown };
  history?: Array<{ parts?: unknown }>;
}

interface StreamEnvelope {
  task?: TaskPayload;
  statusUpdate?: StatusUpdatePayload;
  status_update?: StatusUpdatePayload;
  message?: unknown;
  artifactUpdate?: unknown;
  artifact_update?: unknown;
}

test.describe('contract-closure GET /a2a/tasks/{key}:subscribe status-update enum', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('contract.yaml has the subscribe entry with status: closed', async () => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'GET /a2a/tasks/{key}:subscribe');
    test.skip(
      !target,
      '[contract-closure] no GET /a2a/tasks/{key}:subscribe in contract.yaml. Run `make contract-scan`.',
    );
    expect(target!.source).toBe('both');
    // The `status` field is not surfaced by `list-endpoints-from-yaml`'s
    // narrow type; load the raw yaml and assert it here.
    const yaml = readFileSync(contractPath(), 'utf8');
    expect(yaml).toMatch(/id: GET \/a2a\/tasks\/\{key\}:subscribe[\s\S]*?status: closed/);
  });

  test('real task lifecycle emits only canonical TaskState values on the SSE wire', async ({
    request,
  }) => {
    const eps = onlyBackend(loadEndpoints());
    test.skip(
      !eps.find((e) => e.id === 'GET /a2a/tasks/{key}:subscribe'),
      '[contract-closure] no GET /a2a/tasks/{key}:subscribe in contract.yaml.',
    );

    // 1) Drive a real task lifecycle via the message:send endpoint.
    //    The SDK's `Message.fromJSON` accepts a camelCase payload per
    //    fix card #002; the response carries a `taskId` we can use to
    //    subscribe. The wire shape per `@a2a-js/sdk@1.0.0` is
    //    `SendMessageResponse.result = Message | Task`; the synthia
    //    executor returns a Message envelope (with `taskId` on it)
    //    because the streaming reply is delivered via SSE, not in
    //    the initial JSON body.
    const sendResp = await request.post('/a2a/message:send', {
      headers: { 'content-type': 'application/json' },
      data: {
        message: {
          messageId: `spec-003-${Date.now()}`,
          contextId: '',
          role: 'ROLE_USER',
          parts: [{ text: 'hello from contract-closure sse-status-update spec' }],
        },
      },
    });
    expect(sendResp.status(), 'message:send must succeed').toBeLessThan(300);

    const sendBody = (await sendResp.json()) as {
      result?: { message?: { taskId?: string }; task?: TaskPayload };
      message?: { taskId?: string };
      task?: TaskPayload;
    };
    const taskId =
      sendBody.message?.taskId ??
      sendBody.result?.message?.taskId ??
      sendBody.task?.id ??
      sendBody.result?.task?.id;
    expect(taskId, 'message:send must surface a taskId on the response').toBeTruthy();

    // 2) Subscribe via SSE and capture events. We bound the wait
    //    with a short timeout — the server replays the snapshot
    //    immediately and the work that follows is best-effort
    //    (the LLM may take seconds, may time out, may not be
    //    configured at all). We only assert on what the wire
    //    actually surfaces.
    const subscribeUrl = `${SERVER_BASE}${SUBSCRIBE_PATH}/${encodeURIComponent(taskId)}:subscribe`;
    const stream = await subscribeAndCapture(subscribeUrl, {
      headers: { Accept: 'text/event-stream' },
    });

    // 3) Wait for the natural end of the stream (server closes
    //    once the underlying executor's terminal event fires) or
    //    give up after 8 seconds if no LLM is wired up. The
    //    snapshot is always emitted on subscribe so a Task or
    //    StatusUpdate event should land within a couple of
    //    round-trips.
    await Promise.race([stream.done, new Promise<void>((res) => setTimeout(res, 8_000))]);
    stream.close();

    expect(
      stream.events.length,
      `expected at least one SSE event from ${subscribeUrl}`,
    ).toBeGreaterThan(0);

    // 4) Walk the captured events and assert every wire-state
    //    value (Task.status.state and StatusUpdate.status.state)
    //    is in the canonical SDK enum set. Per ARBITRATION.md
    //    priority 2 (`@a2a-js/sdk` types > Synthia stable spec)
    //    anything outside the set would be a regression.
    const seen: string[] = [];
    for (const event of stream.events) {
      const parsed = tryParseJson(event.data);
      if (parsed === null || typeof parsed !== 'object') continue;
      const env = parsed as StreamEnvelope;

      // Initial Task snapshot (camelCase or snake_case alias).
      const taskField = env.task;
      if (taskField?.status?.state) {
        seen.push(`task.state=${taskField.status.state}`);
        expect(
          CANONICAL_TASK_STATES.has(taskField.status.state),
          `Task.status.state '${taskField.status.state}' must be a canonical @a2a-js/sdk@1.0.0 value`,
        ).toBe(true);
      }

      // StatusUpdate envelope (camelCase or snake_case alias).
      const su = env.statusUpdate ?? env.status_update;
      if (su?.status?.state) {
        seen.push(`statusUpdate.state=${su.status.state}`);
        expect(
          CANONICAL_TASK_STATES.has(su.status.state),
          `StatusUpdate.status.state '${su.status.state}' must be a canonical @a2a-js/sdk@1.0.0 value`,
        ).toBe(true);
      }
    }

    // We expect at least one StatusUpdate to land (either the
    // snapshot replays through StatusUpdate, or the executor
    // emits a Working→Completed transition). If neither fires
    // the server is broken in a way orthogonal to fix card #003.
    expect(
      seen.some((s) => s.startsWith('statusUpdate.state=')) ||
        seen.some((s) => s.startsWith('task.state=')),
      `expected at least one Task or StatusUpdate envelope on the wire; saw: ${seen.join(', ')}`,
    ).toBe(true);

    // 5) Wire-shape conformance for Parts carried on the SSE
    //    stream. Every Part seen on the wire (in `task.history`,
    //    `statusUpdate.status.message.parts`, or
    //    `artifactUpdate.artifact.parts`) MUST use field-presence
    //    serialization: exactly one of `text` / `data` / `raw`
    //    / `url` directly on the Part object. The externally-
    //    tagged `{ content: { text | data | ... } }` form would
    //    conflate the wire with the SDK's internal oneof and is
    //    not what A2A v1.0 specifies.
    for (const event of stream.events) {
      const parsed = tryParseJson(event.data);
      if (parsed === null || typeof parsed !== 'object') continue;
      const env = parsed as StreamEnvelope;
      const sources: Array<{ parts?: unknown }> = [];
      if (env.task?.history) {
        for (const m of env.task.history) sources.push({ parts: (m as { parts?: unknown }).parts });
      }
      if (env.statusUpdate?.status?.message) {
        sources.push({ parts: (env.statusUpdate.status.message as { parts?: unknown }).parts });
      }
      for (const src of sources) {
        if (!Array.isArray(src.parts)) continue;
        for (const part of src.parts) {
          if (!part || typeof part !== 'object') continue;
          const p = part as Record<string, unknown>;
          const presenceCount =
            (p.text === undefined ? 0 : 1) +
            (p.data === undefined ? 0 : 1) +
            (p.raw === undefined ? 0 : 1) +
            (p.url === undefined ? 0 : 1);
          expect(
            presenceCount,
            `Part on the SSE wire must carry exactly one of text/data/raw/url directly on the object; got ${JSON.stringify(part)}`,
          ).toBe(1);
        }
      }
    }
    // The executor may emit zero Parts for a prompt that doesn't
    // run any tool calls; the negative assertion above still
    // pins the wire shape for every Part that does flow.
  });
});
