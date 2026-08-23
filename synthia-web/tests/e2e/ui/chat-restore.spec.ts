import { test, expect } from '@playwright/test';

/**
 * Layer 1 — Chat session restoration regression.
 *
 * Regression coverage for a previously-unreported bug where the
 * ChatPage's "persist" effect clobbered localStorage on the first
 * commit with the initial empty messages array, wiping out any
 * prior conversation for the same sessionId. A user who returned
 * to a `/chat/<existingId>` route (e.g. via the session detail page's
 * "继续 chat" link) always saw the "Welcome" card and an empty
 * history.
 *
 * The fix moves initial hydration into the useState lazy
 * initializer and gates the persist effect's first write with a
 * skip-ref reset on every `sessionId` change.
 */
test.describe('Chat session restoration', () => {
  test('preserves prior messages when reopening a session', async ({ page }) => {
    // Pre-seed localStorage as if a previous session already
    // exists for this sessionId.
    const sessionId = 'restore-test-existing';
    await page.addInitScript(
      ({ sessionId, messages }) => {
        localStorage.setItem(`synthia.messages.${sessionId}`, JSON.stringify(messages));
      },
      {
        sessionId,
        messages: [
          {
            id: 'm1',
            role: 'user',
            segments: [{ id: 's1', type: 'text', content: 'previous question' }],
          },
          {
            id: 'm2',
            role: 'assistant',
            segments: [{ id: 's2', type: 'text', content: 'previous answer' }],
            status: 'completed',
          },
        ],
      },
    );

    await page.goto(`/chat/${sessionId}`);

    // Both messages from the prior session must render — this is
    // the core regression check.
    await expect(page.getByTestId('message-user').first()).toBeVisible({
      timeout: 10000,
    });
    await expect(
      page.getByTestId('message-user').first().getByText('previous question'),
    ).toBeVisible();
    await expect(
      page.getByTestId('message-assistant').first().getByText('previous answer'),
    ).toBeVisible();

    // The localStorage payload must still match the pre-seeded
    // shape. A regression of the persist-before-hydrate bug would
    // have left an empty array here.
    const stored = await page.evaluate(
      (s) => localStorage.getItem(`synthia.messages.${s}`),
      sessionId,
    );
    expect(stored).toBeTruthy();
    expect(stored).not.toBe('[]');
    expect(JSON.parse(stored as string)).toHaveLength(2);
  });
});
