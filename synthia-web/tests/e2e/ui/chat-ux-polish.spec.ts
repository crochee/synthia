import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — Chat UX polish.
 *
 * Pins the visual affordances that mirror the ChatGPT /
 * Claude.ai / Gemini convention:
 *   - typing dots while the model is generating
 *   - inline "Regenerate" button on the trailing assistant turn
 *   - 👍 / 👎 feedback buttons
 *   - model selector dropdown
 *   - usage chip in the header
 *
 * All tests use mocked REST responses (the e2e harness
 * already stubs `/api/v1/chat/*` and `/api/models`) so the
 * UI behaviours are decoupled from a live backend.
 */
test.describe('Chat — UX polish', () => {
  let chat: ChatPage;

  test.beforeEach(async ({ page }) => {
    chat = new ChatPage(page);
  });

  test('typing dots appear while the assistant is generating', async ({ page }) => {
    await chat.goto();
    await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
      // Hold the SSE open so we can observe the indicator.
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body: ': heartbeat\n\n',
      });
    });
    await chat.sendMessage('hello');
    await expect(chat.typingDots).toBeVisible();
  });

  test('regenerate button shows once the assistant finishes', async ({ page }) => {
    await chat.goto();
    await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body: 'data: {"type":"Model","data":{"text":"hi"}}\n\n',
      });
    });
    await chat.sendMessage('hello');
    await chat.waitForAssistantReply();
    await expect(chat.regenerateButton).toBeVisible();
  });

  test('feedback buttons accept a thumbs-up click', async ({ page }) => {
    let feedbackPayload: { message_id: string; thumbs_up: boolean } | null = null;
    await chat.goto();
    await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body: 'data: {"type":"Model","data":{"text":"hi"}}\n\n',
      });
    });
    await page.route('**/api/v1/chat/messages/*/feedback', async (route, request) => {
      feedbackPayload = JSON.parse(request.postData() ?? '{}');
      await route.fulfill({ status: 204, body: '' });
    });
    await chat.sendMessage('hello');
    await chat.waitForAssistantReply();
    // The message id is the trailing assistant id. Click the
    // first feedback button found under message-actions.
    // `dispatchEvent` fires the click directly on the up-vote
    // button rather than routing by screen coordinates — the
    // action-bar sits inside an opacity-on-hover overlay, so
    // coordinate-based clicks occasionally land on the
    // overlapping down-vote instead.
    await expect(chat.messageActions).toBeVisible();
    await page
      .getByTestId(/^feedback-up-/)
      .first()
      .dispatchEvent('click');
    expect(feedbackPayload).not.toBeNull();
    expect(feedbackPayload?.thumbs_up).toBe(true);
  });

  test('model selector renders when /api/models returns entries', async ({ page }) => {
    await page.route('**/api/models', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          models: [
            {
              provider: 'openai',
              model: 'gpt-5',
              context_window: 128000,
              supports_tools: true,
              supports_streaming: true,
            },
            {
              provider: 'anthropic',
              model: 'claude-sonnet-4-6',
              context_window: 200000,
              supports_tools: true,
              supports_streaming: true,
            },
          ],
          default_provider: 'openai',
          default_model: 'gpt-5',
        }),
      });
    });
    await chat.goto();
    await expect(chat.modelSelector).toBeVisible();
    await expect(chat.modelSelect.locator('option')).toHaveCount(2);
  });

  test('usage chip stays present regardless of server health', async () => {
    await chat.goto();
    await expect(chat.usageChip).toBeVisible();
  });
});
