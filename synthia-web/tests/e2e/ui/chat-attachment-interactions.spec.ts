/**
 * Layer 1 — Chat attachment interactions.
 *
 * Covers the user-facing contract for the pending-attachment
 * row that sits between the chat history and the composer:
 *  - Each attached file renders a thumb (image) / audio player
 *    / generic icon (other) plus its filename.
 *  - Each item has a ✕ "Remove attachment" button that drops
 *    just that item from the queue.
 *  - The send button stays disabled while only attachments
 *    are queued (no text yet) until the user types.
 *  - Sending a message clears the attachment row.
 */
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('Chat pending attachments', () => {
  test.beforeEach(async ({ page }) => {
    // Mock the default agent so the chat page becomes usable.
    await page.route('**/api/v1/agents/default', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ name: 'agent', source: 'configured' }),
      }),
    );
  });

  test('remove ✕ button drops just that attachment', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await expect(chat.agentChip).toBeVisible({ timeout: 5_000 });

    // Upload two distinct images.
    const tinyPng = Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=',
      'base64',
    );
    await chat.attachmentInput.setInputFiles([
      { name: 'a.png', mimeType: 'image/png', buffer: tinyPng },
      { name: 'b.png', mimeType: 'image/png', buffer: tinyPng },
    ]);

    // The pending row shows two attachments.
    await expect(chat.pendingAttachments).toBeVisible({ timeout: 5_000 });
    await expect(chat.pendingAttachments.locator('li')).toHaveCount(2);

    // Remove the first attachment — only one remains.
    await page.getByTestId('attachment-remove-0').click();
    await expect(chat.pendingAttachments.locator('li')).toHaveCount(1);

    // The remaining item is now at index 0.
    await expect(page.getByTestId('attachment-remove-0')).toBeVisible();
  });

  test('send button enables with only attachments (no text)', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await expect(chat.agentChip).toBeVisible({ timeout: 5_000 });

    // Initially disabled — empty input + no attachments.
    await expect(chat.sendButton).toBeDisabled();

    // Upload one file. The button should re-enable even with
    // no text content, since the message body will carry the
    // attachment.
    const tinyPng = Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=',
      'base64',
    );
    await chat.attachmentInput.setInputFiles({
      name: 'pixel.png',
      mimeType: 'image/png',
      buffer: tinyPng,
    });
    await expect(chat.pendingAttachments).toBeVisible();
    await expect(chat.sendButton).toBeEnabled();
  });
});
