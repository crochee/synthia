import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — Chat URL-based agent routing.
 *
 * Covers the two URL shapes the chat page supports:
 *
 *   /chat/:sessionId                  → resolves the default
 *                                        agent via
 *                                        `GET /api/v1/agents/default`
 *                                        and replaces the path
 *                                        with
 *                                        `/chat/:sessionId/agent/:name`.
 *   /chat/:sessionId/agent/:agentName → uses the named agent
 *                                        verbatim, surfaces the
 *                                        choice as an inline
 *                                        chip the user can clear.
 *
 * These are pure DOM/routing tests; they assert the URL
 * transitions and the visible affordances without requiring a
 * running backend (the default endpoint is mocked at the
 * Playwright level where necessary).
 */
test.describe('Chat — agent routing', () => {
  let chat: ChatPage;

  test.beforeEach(async ({ page }) => {
    chat = new ChatPage(page);
  });

  test('redirects to /agent/<default> when no agent in URL', async ({ page }) => {
    // Mock the default-agent endpoint so the redirect is
    // deterministic regardless of which agents the real backend
    // happens to register. Without this, the page lands on the
    // first registered agent (server-side policy), which isn't
    // stable across environments.
    await page.route('**/api/v1/agents/default', (route) => {
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ name: 'mock-default-agent', source: 'configured' }),
      });
    });

    await chat.goto();
    await expect(chat.agentChip).toBeVisible({ timeout: 5_000 });
    await expect(chat.agentChipName).toHaveText('mock-default-agent');
    // URL must be rewritten to the explicit-agent path.
    await expect.poll(() => chat.getCurrentAgentName()).toBe('mock-default-agent');
  });

  test('keeps the explicit agent from the URL', async ({ page }) => {
    // No mock needed — an explicit `:agentName` skips the
    // /agents/default lookup. The chip renders immediately.
    await page.route('**/api/v1/agents/default', (route) => {
      route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'NOT_FOUND', message: 'no agents' } }),
      });
    });
    await chat.gotoWithAgent('custom-agent');
    await expect(chat.agentChipName).toHaveText('custom-agent');
    expect(chat.getCurrentAgentName()).toBe('custom-agent');
    // The default endpoint must NOT have been called when the
    // URL already pins an agent.
    const called = await page.evaluate(() => {
      return window.performance
        .getEntriesByType('resource')
        .some((r) => r.name.includes('/api/v1/agents/default'));
    });
    expect(called).toBe(false);
  });

  test('clear button drops the agent and re-runs the default lookup', async ({ page }) => {
    let calls = 0;
    await page.route('**/api/v1/agents/default', (route) => {
      calls += 1;
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ name: `agent-${calls}`, source: 'configured' }),
      });
    });

    await chat.gotoWithAgent('first-choice');
    await expect(chat.agentChipName).toHaveText('first-choice');

    // Click Clear — navigates back to /chat/:sessionId which
    // re-runs the default-resolution effect. The explicit
    // `:agentName` URL did NOT call /agents/default, so the
    // counter is still at 0; after Clear the lookup fires and
    // we land on `agent-1`.
    await page.getByTestId('agent-clear').click();
    await expect(chat.agentChip).toBeVisible({ timeout: 5_000 });
    // URL must lose the agent segment.
    await expect.poll(() => chat.getCurrentAgentName()).toBeNull();
    // The chip should reflect the freshly-resolved default.
    await expect(chat.agentChipName).toHaveText('agent-1');
    expect(calls).toBe(1);
  });

  test('surfaces an inline error when no agents are registered', async ({ page }) => {
    await page.route('**/api/v1/agents/default', (route) => {
      route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'NOT_FOUND', message: 'no agents registered' } }),
      });
    });

    await chat.goto();
    await expect(chat.agentError).toBeVisible({ timeout: 5_000 });
    await expect(chat.agentError).toContainText('Agent routing unavailable');
    // The send button must stay disabled while routing is broken
    // — there's no agent to dispatch to.
    await chat.input.fill('hello');
    await expect(chat.sendButton).toBeDisabled();
  });

  test('attach button is present and accepts image and audio files', async ({ page }) => {
    await page.route('**/api/v1/agents/default', (route) => {
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ name: 'agent', source: 'configured' }),
      });
    });

    await chat.goto();
    await expect(chat.agentChip).toBeVisible({ timeout: 5_000 });

    const input = chat.attachmentInput;
    await expect(input).toBeAttached();

    // The `accept` attribute must include both image/* and
    // audio/* so the OS file picker filters to multimodal
    // attachments. A missing attribute is a silent regression.
    const accept = await input.getAttribute('accept');
    expect(accept).toContain('image/*');
    expect(accept).toContain('audio/*');

    // Uploading a small PNG should produce a pending-attachments
    // row, proving the FileReader pipeline reaches the UI.
    const tinyPng = Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=',
      'base64',
    );
    await input.setInputFiles({ name: 'pixel.png', mimeType: 'image/png', buffer: tinyPng });
    await expect(chat.pendingAttachments).toBeVisible({ timeout: 5_000 });
    await expect(chat.pendingAttachments.locator('img')).toBeVisible();
  });
});
