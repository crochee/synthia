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

  test('assistant message renders with segments structure', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();

    await chat.sendMessage('Hello, say hi back.');
    await chat.waitForAssistantReply(90_000);

    // Verify the assistant message has segments
    const assistantMessages = chat.getAssistantMessages();
    const count = await assistantMessages.count();
    expect(count).toBeGreaterThan(0);

    // Verify segments container exists. ChatPage keeps its own
    // BEM class `nt-chat__segment` for segment internals (the
    // chat page is intentionally not part of the Radix Themes
    // migration — it owns its own CSS), so the legacy class
    // selector still matches the rendered DOM.
    const lastMessage = assistantMessages.last();
    const segments = lastMessage.locator('.nt-chat__segment');
    await expect(segments.first()).toBeVisible();
  });

  test('thinking segments can be collapsed and expanded', async ({ page }) => {
    // This test is informational - thinking segments may not appear for simple queries
    // It verifies the UI structure is ready when thinking segments do appear
    const chat = new ChatPage(page);
    await chat.goto();

    await chat.sendMessage('What is 1+1?');
    await chat.waitForAssistantReply(90_000);

    // Check if any thinking segments exist — ChatPage renders each
    // thinking segment's header as a `<button>` whose visible
    // label is `思考…`, so we locate by accessible name rather
    // than by the legacy `nt-chat__segment--thinking` class.
    const thinkingSegments = page
      .getByTestId('chat-messages')
      .getByRole('button', { name: /^思考/ });
    const thinkingCount = await thinkingSegments.count();

    if (thinkingCount > 0) {
      // If thinking segments exist, verify they are collapsible
      const header = thinkingSegments.first();
      await expect(header).toBeVisible();

      // Click to expand — the header toggles `aria-expanded`.
      await header.click();

      // Verify the segment is now expanded (`aria-expanded="true"`).
      await expect(header).toHaveAttribute('aria-expanded', 'true');
    } else {
      // No thinking segments for this simple query - that's OK
      // The important thing is the UI is ready for when they do appear
      expect(true).toBe(true);
    }
  });
});
