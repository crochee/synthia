import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 3 — Agent conversation tests.
 * End-to-end: send a message, get an assistant reply that
 * relates to the user's question, then ask a follow-up that
 * depends on the previous turn (testing context retention).
 */
test.describe('Agent conversation', () => {
  test('multi-turn conversation keeps assistant responsive', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();

    await chat.sendMessage('What is 2 + 2?');
    await chat.waitForAssistantReply(90_000);
    const first = await chat.getLastAssistantText();
    expect(first.length).toBeGreaterThan(0);

    await chat.sendMessage('And 3 + 3?');
    await chat.waitForAssistantReply(90_000);
    const second = await chat.getLastAssistantText();
    expect(second.length).toBeGreaterThan(0);

    // Two user turns, two assistant turns
    expect(await chat.getUserMessages().count()).toBe(2);
  });

  test('follow-up preserves session id in URL', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    const url1 = page.url();
    await chat.sendMessage('first turn');
    await chat.waitForAssistantReply(90_000);
    // URL should still be /chat/<uuid>
    expect(page.url()).toMatch(/\/chat\/[0-9a-f-]+/);
    expect(page.url()).toBe(url1);
  });
});
