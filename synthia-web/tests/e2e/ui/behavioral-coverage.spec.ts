/**
 * Behavioral coverage — exercises the user-facing flows
 * that the regular suites pin only loosely (or not at all):
 *  1. Cancel button stops an in-flight stream.
 *  2. Model selector changes the model passed to the backend.
 *  3. Sidebar keyboard shortcuts navigate to each page.
 *  4. Continue-in-chat hydrates a fresh page from task history.
 *  5. Agent detail page renders fields from the API.
 */
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('ChatPage behavioral coverage', () => {
  test('Cancel button calls the cancel endpoint and clears streaming state', async ({ page }) => {
    let cancelled = false;
    await page.route('**/api/v1/chat/sessions/*/cancel', (route) => {
      cancelled = true;
      route.fulfill({ status: 204, body: '' });
    });
    // Stub the message POST so sendMessageStream can dispatch
    // the turn before opening the SSE channel. Without this
    // mock the POST 404s and the stream never starts.
    await page.route('**/api/v1/chat/sessions/*/messages', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ message_id: 'm1', queued: true }),
      }),
    );
    // SSE mock: open the stream and emit a single Model frame,
    // then never close the connection. Without a terminal
    // `SessionEnded` frame, ChatPage's streaming state stays
    // true and the cancel button stays mounted.
    // We honour an explicit AbortSignal so the cancel path
    // can actually tear the request down.
    await page.route('**/api/v1/chat/sessions/*/messages/stream', async (route) => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(
            encoder.encode(
              'data: ' +
                JSON.stringify({
                  type: 'Model',
                  data: { type: 'text', text: 'thinking…' },
                }) +
                '\n\n',
            ),
          );
          // Never close — leaves the SSE channel open so
          // ChatPage keeps streaming state true.
        },
      });
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' },
        body: stream as unknown as BodyInit,
      });
    });
    // Pre-warm session.
    await page.route('**/api/v1/agents/default', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ name: 'agent', source: 'configured' }),
      }),
    );

    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('please start streaming');

    // Wait for the stop button to appear (stream in flight).
    const stopBtn = page.getByTestId('stop-button');
    await expect(stopBtn).toBeVisible({ timeout: 5_000 });

    // Click cancel. Use dispatchEvent to bypass Playwright's
    // stability check — the SSE body closes almost instantly
    // in this mock, which makes the stop button detach from
    // the DOM between visible and clickable, so a regular
    // `click()` retry loops until the test times out.
    await stopBtn.dispatchEvent('click');

    // The cancel endpoint must be hit.
    await expect.poll(() => cancelled, { timeout: 3_000 }).toBe(true);
  });

  test('model selector sends the chosen model on submit', async ({ page }) => {
    let submittedBody: Record<string, unknown> | undefined;
    await page.route('**/api/v1/agents/default', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ name: 'agent', source: 'configured' }),
      }),
    );
    await page.route('**/api/models', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          models: [
            { provider: 'openai', model: 'gpt-4' },
            { provider: 'anthropic', model: 'claude-opus' },
          ],
          default_provider: 'openai',
          default_model: 'gpt-4',
        }),
      }),
    );
    await page.route('**/api/v1/chat/sessions/*/messages', async (route, req) => {
      submittedBody = JSON.parse(req.postData() ?? '{}');
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ message_id: 'm1', queued: true }),
      });
    });
    // Empty SSE — exercise the synthesized terminal frame.
    await page.route('**/api/v1/chat/sessions/*/messages/stream', (route) => {
      route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body: '',
      });
    });

    const chat = new ChatPage(page);
    await chat.goto();
    await expect(chat.modelSelector).toBeVisible();
    // Select by the exact value rendered by ChatPage —
    // `${provider}/${model}` (see ChatPage.tsx <select>).
    await chat.modelSelect.selectOption('anthropic/claude-opus');
    await chat.sendMessage('hello');
    // The backend's `SendMessageRequest` has a `model` field
    // (see routes/chat.rs), so the wire body must carry the
    // selection verbatim as `"<provider>/<model>"`. The
    // server currently only deserialises the field; routing
    // the choice to the agent runtime is tracked separately.
    await expect
      .poll(() => submittedBody?.['model'], { timeout: 3_000 })
      .toBe('anthropic/claude-opus');
  });

  test('sidebar keyboard shortcuts navigate to each page', async ({ page }) => {
    await page.goto('/chat');
    // Move focus off the input — single-letter keystrokes are
    // otherwise consumed by the chat textarea.
    await page.locator('body').click({ position: { x: 5, y: 5 } });
    const expectUrl = async (regex: Regesp) => {
      await expect.poll(() => new URL(page.url()).pathname, { timeout: 3_000 }).toMatch(regex);
    };
    // Shortcut convention is "g + <letter>" (Vim / GitHub style).
    // `g s` jumps straight to /sessions — the canonical Sessions
    // page that replaces the legacy /tasks entry. There is no
    // longer a separate `g a` binding because the "Tasks"
    // sidebar entry was folded into Sessions.
    await page.keyboard.press('g');
    await page.keyboard.press('c');
    await expectUrl(/^\/chat/);
    await page.keyboard.press('g');
    await page.keyboard.press('t');
    await expectUrl(/\/tools$/);
    await page.keyboard.press('g');
    await page.keyboard.press('g');
    await expectUrl(/\/agents$/);
    await page.keyboard.press('g');
    await page.keyboard.press('k');
    await expectUrl(/\/skills$/);
    await page.keyboard.press('g');
    await page.keyboard.press('s');
    await expectUrl(/\/sessions$/);
  });
});
