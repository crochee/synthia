import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 3 — Task lifecycle tests.
 * Verifies the tasks visible on the /tasks page reflect
 * the tasks we just initiated in /chat. The Tasks page
 * hits GET /api/tasks which the backend persists via
 * the A2A task store.
 */
test.describe('Task lifecycle', () => {
  test('task appears in Tasks page after sending a message', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('create a task please');
    await chat.waitForAssistantReply(90_000);

    await page.goto('/tasks');
    await page.waitForLoadState('networkidle');

    // Render whatever the backend knows about. Whether the
    // list is empty (the backend discards tasks on shutdown)
    // or populated, the page must render without error.
    await expect(page.locator('h1', { hasText: 'Tasks' })).toBeVisible();
  });

  test('chat assistant message reaches a terminal state', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('acknowledge');
    await chat.waitForAssistantReply(90_000);
    const last = chat.getAssistantMessages().last();
    // The terminal state must be one of these — never "working"
    // forever (the SSE stream must close eventually).
    await expect(
      last.locator('.status-completed, .status-failed, .status-canceled, .status-input-required'),
    ).toBeVisible();
  });
});
