/**
 * Regression coverage for the chat UI's tool-result rendering.
 *
 * Background: the chat UI used to render the assistant turn's
 * tool_use as a `tool_block` (with a `pending` placeholder for
 * the result). When the tool_result event arrived on the SSE
 * stream, the dispatcher dropped it on the floor — `dispatchPartPayload`
 * had a guard `if (!text && segmentType !== 'tool_call') return false`
 * that treated the empty `text` field as a no-op signal. The
 * actual result body lives on `metadata.text` (translated from
 * `Part::data({tool_use_id, content})`), so the payload was
 * real and just got dropped.
 *
 * User-visible symptom: the tool_block stayed "执行中…" forever,
 * even after the backend had executed the shell command and
 * streamed a proper tool_result event. Replaying the same task
 * on the task detail page showed the result correctly because
 * the history path bypassed the broken guard.
 *
 * This test pins the contract end-to-end: send a prompt that
 * triggers a shell tool call, wait for the stream to finish,
 * then assert the tool_block carries the result body (not the
 * pending placeholder).
 *
 * It runs against the real synthia-server + Vite dev stack —
 * the test triggers the actual SSE stream through
 * `sendMessageStream`, not the localStorage hydration path.
 */
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('Chat — tool result rendering', () => {
  let chat: ChatPage;

  test.beforeEach(async ({ page }) => {
    chat = new ChatPage(page);
    await chat.goto();
  });

  test('tool_block stops pending and shows result after tool_result event lands', async () => {
    // Submit a prompt the LLM will answer via the shell
    // tool. Backend will execute `echo hi`, returning
    // `tool_result` with is_error=false and stdout="hi\n".
    await chat.sendMessage('run shell: echo hi');

    // Wait for the assistant turn + a tool_block to appear.
    // We use a generous timeout because the LLM round-trip
    // can take a few seconds on cold start.
    const assistant = chat.getAssistantMessages().last();
    await expect(assistant).toBeVisible({ timeout: 30_000 });

    const toolBlock = assistant.locator('.nt-chat__segment--tool_block');
    await expect(toolBlock).toBeVisible({ timeout: 30_000 });

    // The regression: the tool_block must NOT carry the
    // "执行中…" placeholder once the result has arrived.
    // We poll the body text until the result label appears
    // (no longer "执行中…") — this is a positive assertion
    // rather than a negative one, so the test fails loudly
    // if the result is silently absent instead of just
    // "no longer pending".
    await expect
      .poll(
        async () => {
          const text = (await toolBlock.textContent()) ?? '';
          return text.trim();
        },
        {
          timeout: 30_000,
          intervals: [500, 1000, 2000],
        },
      )
      .not.toMatch(/执行中/);

    // Expand the tool_block — the body section (with the
    // 请求 / 结果 sub-blocks) is only rendered when the
    // block is expanded. The toggle is the chat-toggle
    // button inside the tool_block; clicking it switches
    // the local `expanded` state.
    await toolBlock.locator('button.chat-toggle').click();

    // Stronger assertion: the tool_block should now show
    // a result section with a "结果" label and the
    // tool's stdout. The body text after the call header
    // must contain the result section's content marker
    // (`nt-chat__tool-block-result` class), AND the
    // `<pre>` rendered result must contain something —
    // anything — to prove the result body is wired through.
    const resultSection = toolBlock.locator('.nt-chat__tool-block-result');
    await expect(resultSection).toBeVisible({ timeout: 5_000 });
    const resultText = (await resultSection.textContent()) ?? '';
    expect(resultText.trim().length).toBeGreaterThan(0);

    // Pin the result content. The shell tool's stdout
    // format is "exit: 0\n\nstdout:\n<output>", so the
    // rendered <pre> must at least contain "exit: 0"
    // and "stdout:" markers — this proves the result body
    // came from the tool_result event, not a placeholder
    // string.
    expect(resultText).toContain('exit: 0');
    expect(resultText).toContain('stdout:');

    // Wait for the assistant to reach a terminal status
    // (completed/failed/etc.) so the test doesn't race the
    // stream finishing.
    await chat.waitForAssistantReply(30_000);

    // Final check: the assistant's status is now `completed`
    // (not `working`), which is the user-visible signal
    // that the turn is done.
    await expect(assistant.locator('.status-completed')).toBeVisible();
  });
});
