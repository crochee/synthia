/**
 * End-to-end coverage for the SessionEnded error message
 * propagating from the SSE stream to the chat UI.
 *
 * Background: when an upstream LLM call fails (e.g.
 * `stream error (http_failure)`) the agent loop emits a
 * `SessionEnded { reason: Error(msg) }` event. The server
 * serializes it as a `statusUpdate` whose `status.message`
 * carries a `Part::text(msg)` body — and the chat UI must
 * surface that text inside the assistant message so the user
 * sees *why* the session ended. Without this, the user sees
 * a bare `failed` status pill with no actionable detail.
 *
 * Pinned user-visible contract:
 *   - the assistant message's status pill is `failed`
 *   - the assistant message body contains the upstream error
 *     string verbatim (e.g. `rate limited`)
 *
 * Drives the SDK via a mocked fetch (see
 * `mock-a2a-server.runtime.js`) since the live backend is
 * offline in the e2e harness.
 */
import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';
import { fileURLToPath } from 'node:url';

const ERROR_TEXT = 'stream error (http_failure): rate limited';

const failureEvents: ReadonlyArray<{ data: string }> = [
  // Working → session in flight.
  {
    data: JSON.stringify({
      statusUpdate: {
        taskId: 't1',
        contextId: 'c1',
        status: { state: 'TASK_STATE_WORKING' },
      },
    }),
  },
  // SessionEnded with Error(msg) — the wire shape the backend
  // emits for an upstream LLM/provider failure.
  {
    data: JSON.stringify({
      statusUpdate: {
        taskId: 't1',
        contextId: 'c1',
        status: {
          state: 'TASK_STATE_FAILED',
          message: {
            messageId: 'm-err',
            contextId: 'c1',
            taskId: 't1',
            role: 'ROLE_AGENT',
            parts: [{ text: ERROR_TEXT }],
          },
        },
        final: true,
      },
    }),
  },
];

test('SessionEnded error message renders in the assistant reply', async ({ page }) => {
  const chat = new ChatPage(page);
  await chat.goto();

  // 1. Define the mock fetch in-page.
  const factorySource = readFileSync(
    fileURLToPath(new URL('../helpers/mock-a2a-server.runtime.js', import.meta.url)),
    'utf8',
  );
  const eventsJson = JSON.stringify(failureEvents);
  await page.evaluate(
    ({ eventsJson, factorySource }: { eventsJson: string; factorySource: string }) => {
      const factory = new Function(
        'events',
        factorySource + '\nreturn buildMockA2AFetch({ streamEvents: events });',
      );
      const events = JSON.parse(eventsJson) as Array<{ data: string }>;
      (window as unknown as { __synthiaMockFetch: typeof fetch }).__synthiaMockFetch =
        factory(events);
    },
    { eventsJson, factorySource },
  );

  // 2. Wire the mock into the SDK.
  await page.evaluate(async () => {
    const mod = await import('/src/api/a2a-stream.ts');
    (mod as unknown as { _bootstrapTestFetch: () => void })._bootstrapTestFetch();
    (mod as unknown as { _resetClientForTesting: () => void })._resetClientForTesting();
  });

  // 3. Submit. The mock's auto-append logic detects the user's
  //    TASK_STATE_FAILED statusUpdate and skips the synthetic
  //    TASK_STATE_COMPLETED terminal.
  await chat.sendMessage('请帮我跑命令');

  // 4. Wait for the terminal failed status to land.
  await chat.waitForAssistantReply(30_000);

  // 5. The assistant message MUST surface the upstream error
  //    string — that's the whole point of the wire-shape change.
  const assistant = chat.getAssistantMessages().last();
  await expect(assistant).toContainText(ERROR_TEXT);

  // 6. The status pill MUST read `failed` so the user knows the
  //    session did not complete normally.
  await expect(assistant.locator('.nt-chat__message-status')).toContainText('failed');
});
