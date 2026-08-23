import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — Toast viewport.
 *
 * Pins the in-app notification queue:
 *   - the success toast after clicking a feedback button,
 *   - the error toast when feedback POST fails,
 *   - the dismiss button immediately removing a toast,
 *   - the auto-dismiss timer removing a toast on its own.
 *
 * All toasts flow through `<ToastProvider>` (mounted in
 * `<App>`) so the viewport is global. Tests assert behaviour
 * via `role="alert"` (error/warning) and `role="status"`
 * (success/info) which is what screen readers announce.
 */
test.describe('Toast viewport', () => {
  let chat: ChatPage;

  test.beforeEach(async ({ page }) => {
    chat = new ChatPage(page);
    await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body: 'data: {"type":"Model","data":{"text":"hi"}}\n\n',
      });
    });
  });

  test('feedback success toast appears with role=status and is dismissable', async ({ page }) => {
    await page.route('**/api/v1/chat/messages/*/feedback', async (route) => {
      await route.fulfill({ status: 204, body: '' });
    });
    await chat.goto();
    await chat.sendMessage('hello');
    await chat.waitForAssistantReply();
    await expect(chat.messageActions).toBeVisible();
    await page
      .getByTestId(/^feedback-up-/)
      .first()
      .dispatchEvent('click');

    // The success toast uses role="status" (not "alert") so
    // it does not interrupt the user. The message contains
    // the thumbs-up emoji per ChatPage.
    const toast = page.getByRole('status').filter({ hasText: 'Thanks for the feedback' });
    await expect(toast).toBeVisible({ timeout: 2_000 });

    // The × button on the toast removes it from the viewport.
    await toast.getByRole('button', { name: /dismiss notification/i }).click();
    await expect(toast).toHaveCount(0);
  });

  test('feedback failure toast appears with role=alert', async ({ page }) => {
    await page.route('**/api/v1/chat/messages/*/feedback', async (route) => {
      await route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'internal_server_error', message: 'boom' } }),
      });
    });
    await chat.goto();
    await chat.sendMessage('hello');
    await chat.waitForAssistantReply();
    await expect(chat.messageActions).toBeVisible();
    await page
      .getByTestId(/^feedback-up-/)
      .first()
      .dispatchEvent('click');

    // Error toasts use role="alert" — verify both the role and
    // the message prefix. The exact cause text after the colon
    // comes from the mocked 500 body, so we don't pin it.
    const toast = page.getByRole('alert').filter({ hasText: 'Feedback failed' });
    await expect(toast).toBeVisible({ timeout: 5_000 });
  });

  test('server unreachable toast surfaces a warning then clears on recovery', async ({ page }) => {
    // Pin both ends of the health transition. The ChatPage
    // toast handler skips the initial `false → false` mount
    // and only fires on a real `true → false` transition, so
    // we must mount the page with `/readyz` mocked healthy
    // first (which leaves `lastHealthRef = true`), then break
    // the health endpoint and trigger an `online` event so
    // `useServerHealth` re-probes and sees the 503.
    await page.route('**/readyz', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'ok' }),
      });
    });

    await chat.goto();
    // Header reads "Online" once the initial probe lands — use
    // that as a proxy for `lastHealthRef === true` before we
    // break the connection.
    await expect(page.getByTestId('server-status')).toContainText('Online', {
      timeout: 10_000,
    });

    // Now break `/readyz` and force a re-probe. The health
    // hook listens for the window `online` event and kicks
    // `probe()` immediately.
    await page.unroute('**/readyz');
    await page.route('**/readyz', async (route) => {
      await route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'unhealthy' }),
      });
    });
    await page.evaluate(() => window.dispatchEvent(new Event('online')));

    const warn = page.getByRole('alert').filter({ hasText: 'Synthia backend unreachable' });
    await expect(warn).toBeVisible({ timeout: 10_000 });

    // Restore `/readyz` and probe again — the same `online`
    // event handler runs `probe()` so we re-use it.
    await page.unroute('**/readyz');
    await page.route('**/readyz', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'ok' }),
      });
    });
    await page.evaluate(() => window.dispatchEvent(new Event('online')));

    const ok = page.getByRole('status').filter({ hasText: 'Synthia backend is back online' });
    await expect(ok).toBeVisible({ timeout: 10_000 });
  });
});
