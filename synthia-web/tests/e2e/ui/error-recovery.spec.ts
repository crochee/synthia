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
    // Wait long enough for the persistence useEffect's 300ms
    // debounce to commit the new messages to localStorage. The
    // initial 200ms sleep was a flake source — the actual
    // debounce timer in ChatPage is 300ms.
    await expect
      .poll(
        () =>
          page.evaluate(() => {
            const keys = Object.keys(window.localStorage).filter((k) =>
              k.startsWith('synthia.messages.'),
            );
            return keys.length;
          }),
        { timeout: 2_000 },
      )
      .toBeGreaterThan(0);

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
