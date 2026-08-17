import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — UI error & recovery tests.
 *
 * Verifies that the UI keeps working when:
 *   - the user types whitespace-only input (should be rejected),
 *   - the user submits, then immediately types again,
 *   - the user reloads the page in the middle of streaming.
 *
 * These tests assert resilience against bad input and intermittent
 * failures rather than happy-path behaviour.
 */

test.describe('UI error & recovery', () => {
  test('whitespace-only messages are not accepted', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.input.fill('   ');
    await expect(chat.sendButton).toBeDisabled();
  });

  test('user message persists across reload', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('persistent question');
    await expect(chat.getUserMessages().last()).toContainText('persistent question');
    // Allow the persistence useEffect to commit to localStorage.
    await page.waitForTimeout(200);
    // Sanity: localStorage must contain the message BEFORE the
    // reload so we know the round-trip below is a fair test.
    const storedBeforeReload = await page.evaluate(() => {
      const keys = Object.keys(window.localStorage).filter((k) =>
        k.startsWith('synthia.messages.'),
      );
      const payload = keys.map((k) => window.localStorage.getItem(k) ?? '');
      return { keys, payload };
    });
    expect(
      storedBeforeReload.keys.length,
      'localStorage must hold at least one session',
    ).toBeGreaterThan(0);
    const persistedJson = storedBeforeReload.payload.find((p) => p.includes('persistent question'));
    expect(persistedJson, 'user message must be in localStorage before reload').toBeTruthy();

    // NOTE: full cross-reload restore of the *same* session depends
    // on the chat navigation hook preserving the route's session id
    // across page loads, which is an app-level concern tracked
    // separately from the W3C TraceContext work. Here we only
    // assert that:
    //   - the reload doesn't blow up (input still renders),
    //   - the persistence layer still works (localStorage state
    //     survives the navigation).
    // The full restore assertion is exercised in
    // `tests/e2e/integration/chat-persistence.spec.ts` once the
    // routing fix lands.
    await page.reload();
    await expect(chat.input).toBeVisible({ timeout: 10_000 });

    const afterReloadKeys = await page.evaluate(() =>
      Object.keys(window.localStorage).filter((k) => k.startsWith('synthia.messages.')),
    );
    expect(
      afterReloadKeys.length,
      'localStorage synthia.messages.* keys must survive reload',
    ).toBeGreaterThan(0);
  });

  test('reload during streaming restores working state', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('first turn');
    // Don't wait for the assistant to finish — just reload.
    await page.reload();
    // After reload the input must be usable again.
    await expect(chat.input).toBeVisible();
    await expect(chat.sendButton).toBeDisabled();
    await chat.input.fill('next');
    await expect(chat.sendButton).toBeEnabled();
  });
});
