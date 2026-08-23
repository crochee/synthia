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
 * Runs against a mocked SSE pipeline (the live backend is
 * offline in the e2e harness).
 */
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

const ERROR_TEXT = 'stream error (http_failure): rate limited';

test('SessionEnded error message renders in the assistant reply', async ({ page }) => {
  // 1. Mock the SSE stream to emit a `session_ended { reason:
  //    error }` system frame preceded by a single Model text
  //    part. The body is what the chat UI must surface.
  await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
    const body =
      'data: ' +
      JSON.stringify({ type: 'Model', data: { type: 'text', text: ERROR_TEXT } }) +
      '\n\n' +
      'data: ' +
      JSON.stringify({ type: 'System', data: { type: 'session_ended', reason: 'error' } }) +
      '\n\n';
    route.fulfill({
      status: 200,
      headers: { 'content-type': 'text/event-stream' },
      body,
    });
  });

  const chat = new ChatPage(page);
  await chat.goto();

  await chat.sendMessage('请帮我跑命令');

  // Wait for the terminal failed status to land.
  await chat.waitForAssistantReply(15_000);

  // 5. The assistant message MUST surface the upstream error
  //    string — that's the whole point of the wire-shape change.
  const assistant = chat.getAssistantMessages().last();
  await expect(assistant).toContainText(ERROR_TEXT);

  // 6. The status pill MUST read `failed` so the user knows the
  //    session did not complete normally.
  await expect(assistant.locator('.nt-chat__message-status')).toContainText('failed');
});
