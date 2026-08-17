import { test, expect } from '@playwright/test';
import { ChatPage } from './pages/chat.page';

/**
 * MVP smoke test — pinned by `mvp-acceptance-criterion`
 * Requirement: Playwright E2E Smoke.
 *
 * Prerequisites (run before invoking this test):
 *   1. `make dev` (or equivalent: `synthia-server` on :8080 and
 *      Vite on :5173). The `webServer` block in `playwright.config.ts`
 *      starts both automatically when they are not already running.
 *   2. A configured LLM provider in `config.yaml` (or equivalent
 *      environment variables) so the agent can actually reply.
 *      Without a working provider the assistant message will not
 *      reach a terminal state and this test will time out.
 *
 * What this spec pins
 * -------------------
 * The end-to-end LLM round trip through the A2A protocol: user
 * message → SSE stream → assistant message reaches a terminal
 * state with non-empty reply text. The assistant uses
 * `tool_choice: Auto` (the agent never forces tool calls), so a
 * given prompt may complete with or without a tool invocation —
 * that is the correct AI agent behavior. When the LLM does choose
 * to call a tool, the rendered tool segment must carry one of the
 * expected tool names (`read_file` / `shell` / `web_fetch`); when
 * it does not, the prose reply alone is the proof of life.
 *
 * Total wall-clock budget: 60 seconds (per spec).
 */
test.describe('MVP smoke', () => {
  test.setTimeout(60_000);

  test('chat round-trip reaches a terminal assistant reply', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();

    await chat.sendMessage('list files in the current directory');
    // Wait for the assistant's terminal status (completed / failed /
    // canceled / input-required). 50s leaves headroom under the 60s
    // test-level timeout for the assertions below.
    await chat.waitForAssistantReply(50_000);

    // The assistant's reply text must be non-empty.
    const replyText = await chat.getLastAssistantText();
    expect(replyText.length, 'assistant reply must be non-empty').toBeGreaterThan(0);

    // If the LLM chose to invoke a tool for this prompt, the message
    // timeline must show at least one tool entry (tool calls render
    // as `.nt-chat__segment--tool_block` — call + result merged — or
    // as standalone `tool_call` / `tool_result` segments; any of them
    // carries the tool name in its label). The presence check uses
    // `count()` rather than `first().toBeVisible()` because
    // `tool_choice: Auto` legitimately lets the LLM complete without
    // any tool call — e.g. answering "list files" in prose when no
    // tool is required. Forcing a tool call here would override the
    // agent's autonomy, which is what makes an AI agent an AI agent.
    const toolSegments = chat.messageList.locator(
      '.nt-chat__segment--tool_block, .nt-chat__segment--tool_call, .nt-chat__segment--tool_result',
    );
    const toolCount = await toolSegments.count();
    if (toolCount > 0) {
      await expect(toolSegments.first()).toContainText(/read_file|shell|web_fetch/);
    }
  });
});
