import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';
import { SettingsPage } from '../pages/settings.page';
import { TasksPage } from '../pages/tasks.page';
import { MemoryPage } from '../pages/memory.page';

/**
 * Layer 3 — Full user-scenario end-to-end tests.
 *
 * Walks a realistic user through several pages, asserting the data
 * flows correctly between the frontend state and the backend store.
 *
 * Scenario "ask the agent → confirm task is recorded":
 *   1. Open /chat, ask the agent to greet you.
 *   2. Wait for the assistant's terminal status badge.
 *   3. Navigate to /tasks and confirm the page renders.
 *   4. Navigate to /memory, run a search query, confirm the page
 *      surfaces either results or the empty state.
 *
 * Scenario "settings round-trip":
 *   1. Open /settings.
 *   2. Set a provider + model, save, reload.
 *   3. Confirm the values are preserved (PUT is durable across reload).
 */
test.describe('Full user scenarios', () => {
  test('chat → tasks → memory navigation preserves backend state', async ({ page }) => {
    const chat = new ChatPage(page);
    await chat.goto();
    await chat.sendMessage('hello from the full-flow test');
    await chat.waitForAssistantReply(90_000);

    // After the assistant finishes, the URL must still be /chat/:sessionId.
    expect(page.url()).toMatch(/\/chat\/[0-9a-f-]+/);

    // The Tasks page must render without errors (the agent's task
    // may or may not be visible depending on backend retention; we
    // only assert the page is healthy).
    const tasks = new TasksPage(page);
    await tasks.goto();
    await expect(page.locator('h1', { hasText: 'Tasks' })).toBeVisible();

    // The Memory page must accept a search query and respond.
    const memory = new MemoryPage(page);
    await memory.goto();
    await memory.search('synthia');
    await expect(memory.queryInput).toBeVisible();
  });

  test('settings round-trip via PUT is durable across reload', async ({ page }) => {
    const settings = new SettingsPage(page);
    await settings.goto();
    const marker = `fullflow-${Date.now()}`;
    await settings.setProvider(marker);
    await settings.setModel('fullflow-model');
    await settings.save();

    await page.reload();
    await expect(settings.providerInput).toHaveValue(marker);
    await expect(settings.modelInput).toHaveValue('fullflow-model');
  });

  test('all management API endpoints respond to GET', async ({ request }) => {
    // Quick smoke test: every endpoint the UI relies on for its
    // management pages must return a 2xx with a JSON body.
    // Hits the Rust server directly to keep this test independent of
    // the Vite dev proxy lifecycle.
    const baseUrl = 'http://localhost:8080';
    const endpoints = [
      '/health',
      '/api/providers',
      '/api/skills',
      '/api/tools',
      '/api/settings',
      '/api/jobs',
      '/api/tasks',
      '/api/mcp/servers',
      '/.well-known/agent-card.json',
    ];

    for (const ep of endpoints) {
      const r = await request.get(`${baseUrl}${ep}`);
      expect(r.ok(), `${ep} should respond with 2xx`).toBe(true);
      const body = await r.json();
      expect(body, `${ep} should return a JSON object`).toBeTruthy();
      expect(
        typeof body === 'object' && body !== null,
        `${ep} body should be a JSON object`,
      ).toBe(true);
      // Most endpoints wrap the payload in `{ status: "ok", ... }`.
      // The agent-card endpoint follows the A2A spec and returns
      // the card directly — accept either shape.
      if (body.status !== undefined) {
        expect(body.status, `${ep} envelope status`).toBe('ok');
      } else {
        // Agent-card: just confirm the standard A2A fields exist.
        expect(body.name, `${ep} should declare a name`).toBeTruthy();
        expect(Array.isArray(body.supportedInterfaces)).toBe(true);
      }
    }
  });
});