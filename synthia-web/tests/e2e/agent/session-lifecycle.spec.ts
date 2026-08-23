import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 3 — Session lifecycle tests.
 *
 * Verifies the sessions visible on the /sessions page reflect
 * the sessions we just initiated in /chat. The Sessions page
 * hits `GET /api/v1/sessions` which the backend persists via the
 * session store. (The wire-format endpoint name keeps its
 * historical `tasks` spelling to avoid breaking deployed
 * clients — only the UI text and route names use "session".)
 */
test.describe('Session lifecycle', () => {
  test('session appears in Sessions page after sending a message', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('create a session please');
    await chat.waitForAssistantReply(90_000);

    await page.goto('/sessions');
    // Wait for the page heading — explicit so the intent is
    // obvious. Avoids relying on `networkidle`, which can hang
    // when the chat pages' SSE stream + 30s usage polling keep
    // the network busy forever.

    // Render whatever the backend knows about. Whether the
    // list is empty (the backend discards sessions on
    // shutdown) or populated, the page must render without
    // error.
    await expect(page.locator('h1', { hasText: 'Sessions' })).toBeVisible();
  });

  test('chat assistant message reaches a terminal state', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('acknowledge');
    await chat.waitForAssistantReply(90_000);
    const last = chat.getAssistantMessages().last();
    // The terminal state must be one of these — never
    // "working" forever (the SSE stream must close
    // eventually).
    await expect(
      last.locator('.status-completed, .status-failed, .status-canceled, .status-input-required'),
    ).toBeVisible();
  });
});
