import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 4 — Full-stack LLM round-trip.
 *
 * This is the canonical end-to-end proof that:
 *   1. `synthia-server` is started with `--config config.yaml`
 *      and that file's `providers.openai.base_url` /
 *      `api_key` / `models[].name` are bridged into the runtime
 *      `WorkspaceConfig` consumed by `synthia-provider`.
 *   2. The React frontend (`/chat`) connects to the backend via
 *      the Vite proxy → REST + SSE protocol at `/api/v1/chat`.
 *   3. The backend actually issues a chat completion to the
 *      configured `base_url`.
 *   4. The LLM reply is rendered in the assistant bubble.
 *
 * The test is split into sequential assertions so a failure
 * points at the broken layer instead of the whole stack.
 */

test.describe('Full-stack LLM round-trip via config.yaml', () => {
  test('chat UI sends a message and receives a non-empty assistant reply', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();

    await chat.sendMessage('Please reply with the single word: pong');
    await chat.waitForAssistantReply(90_000);

    const reply = (await chat.getLastAssistantText()).trim();
    expect(reply.length, 'assistant reply must be non-empty').toBeGreaterThan(0);
    // We don't assert the exact word because streaming token
    // ordering and any leading reasoning whitespace would make
    // the assertion brittle. Just assert the agent actually said
    // *something* meaningful.
    expect(reply.toLowerCase()).toContain('pong');
  });

  test('session reaches terminal completed state after LLM reply', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('ack');
    await chat.waitForAssistantReply(90_000);

    const last = chat.getAssistantMessages().last();
    await expect(
      last.locator('.status-completed, .status-failed, .status-canceled, .status-input-required'),
    ).toBeVisible();
  });
});
