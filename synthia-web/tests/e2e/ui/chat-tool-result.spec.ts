/**
 * Regression coverage for the chat UI's tool-result rendering.
 *
 * Pins the contract that `dispatchPartPayload` correctly
 * handles a `tool_result` event after a matching `tool_use`
 * — the body must transition out of the pending placeholder
 * once the result lands, and the rendered result section must
 * surface the tool's stdout / stderr.
 *
 * Runs against a mocked SSE pipeline (the live LLM round-trip
 * is too slow + non-deterministic for a UI-layer test). The
 * wire shape mirrors what the real backend emits: a Model
 * event carrying a `ToolUse` ContentPart, then a Model event
 * carrying a `ToolResult` ContentPart, then a terminal
 * `session_ended` system event.
 */
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('Chat — tool result rendering', () => {
  let chat: ChatPage;

  test.beforeEach(async ({ page }) => {
    chat = new ChatPage(page);
  });

  test('tool_block stops pending and shows result after tool_result event lands', async ({
    page,
  }) => {
    // Mock the SSE stream to emit a tool_use + tool_result pair
    // followed by a terminal frame, so we can verify the UI
    // rendering deterministically.
    await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
      const body =
        // 1. ToolUse: shell command "echo hi" with id=tu-1
        'data: ' +
        JSON.stringify({
          type: 'Model',
          data: { type: 'tool_use', id: 'tu-1', name: 'shell', input: { command: 'echo hi' } },
        }) +
        '\n\n' +
        // 2. ToolResult: exit 0, stdout "hi\n"
        'data: ' +
        JSON.stringify({
          type: 'Model',
          data: {
            type: 'tool_result',
            tool_use_id: 'tu-1',
            content: 'exit: 0\n\nstdout:\nhi\n',
            is_error: false,
          },
        }) +
        '\n\n' +
        // 3. Terminal
        'data: ' +
        JSON.stringify({ type: 'System', data: { type: 'session_ended', reason: 'completed' } }) +
        '\n\n';
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body,
      });
    });

    await chat.goto();
    await chat.sendMessage('run shell: echo hi');

    const assistant = chat.getAssistantMessages().last();
    await expect(assistant).toBeVisible({ timeout: 15_000 });

    const toolBlock = assistant.locator('.nt-chat__segment--tool_block');
    await expect(toolBlock).toBeVisible({ timeout: 15_000 });

    // The tool_block must carry the result body (not the
    // pending placeholder) once the result lands.
    await expect
      .poll(
        async () => {
          const text = (await toolBlock.textContent()) ?? '';
          return text.trim();
        },
        { timeout: 15_000, intervals: [500, 1000, 2000] },
      )
      .not.toMatch(/执行中/);

    // Expand the block to render the 请求 / 结果 sub-sections.
    await toolBlock.locator('button.chat-toggle').click();

    const resultSection = toolBlock.locator('.nt-chat__tool-block-result');
    await expect(resultSection).toBeVisible({ timeout: 5_000 });
    const resultText = (await resultSection.textContent()) ?? '';
    expect(resultText.trim().length).toBeGreaterThan(0);
    expect(resultText).toContain('exit: 0');
    expect(resultText).toContain('stdout:');

    // Terminal status must land — driven by the explicit
    // session_ended system frame the mock emits.
    await chat.waitForAssistantReply(15_000);
    await expect(assistant.locator('.status-completed')).toBeVisible();
  });
});
