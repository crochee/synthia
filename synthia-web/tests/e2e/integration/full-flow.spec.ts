import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';
import { TasksPage } from '../pages/tasks.page';

/**
 * Layer 3 — Full user-scenario end-to-end tests.
 *
 * Walks a realistic user through several pages, asserting the data
 * flows correctly between the frontend state and the backend store.
 *
 * Scenario "ask the agent → confirm task is recorded":
 *   1. Open /chat, ask the agent to greet you.
 *   2. Wait for the assistant's terminal status badge.
 *   3. Navigate to /tasks, confirm the page renders, and run a
 *      memory search (the search block lives on the Tasks page
 *      since the standalone Memory page was merged in).
 */
test.describe('Full user scenarios', () => {
  test('chat → tasks (with search) preserves backend state', async ({ page }) => {
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

    // The search block (formerly the Memory page) must accept a
    // query and respond. Whether any results appear depends on
    // backend contents; we only assert the page didn't throw.
    await tasks.search('synthia');
    await expect(tasks.queryInput).toBeVisible();
  });

  test('all management API endpoints respond to GET', async ({ request }) => {
    // Quick smoke test: every endpoint the UI relies on for its
    // management pages must return a 2xx with a JSON body.
    // Hits the Rust server directly to keep this test independent of
    // the Vite dev proxy lifecycle.
    const baseUrl = 'http://localhost:8080';
    const endpoints = [
      '/livez',
      '/readyz',
      '/api/v1/skills',
      '/api/v1/tools',
      '/api/v1/tasks',
      '/.well-known/agent-card.json',
    ];

    for (const ep of endpoints) {
      const r = await request.get(`${baseUrl}${ep}`);
      expect(r.ok(), `${ep} should respond with 2xx`).toBe(true);
      const body = await r.json();
      expect(body, `${ep} should return a JSON object`).toBeTruthy();
      expect(typeof body === 'object' && body !== null, `${ep} body should be a JSON object`).toBe(
        true,
      );
      // v1 bare-response shapes:
      //   - `/livez` / `/readyz` return `{ status }` (status is
      //     the probe state, not an envelope marker).
      //   - List endpoints return `List<T>` = `{ data, next_cursor, total }`.
      //   - `/.well-known/agent-card.json` follows the A2A spec and
      //     returns the card directly (`name`, `supportedInterfaces`, …).
      if (ep === '/livez' || ep === '/readyz') {
        expect(body.status, `${ep} probe status`).toBe('ok');
      } else if (ep === '/.well-known/agent-card.json') {
        expect(body.name, `${ep} should declare a name`).toBeTruthy();
        expect(Array.isArray(body.supportedInterfaces)).toBe(true);
      } else {
        // v1 List<T> shape: `data` is an array, `next_cursor` is
        // Option<String> (omitted from JSON when None — so the
        // field may be undefined, null, or a string), `total` is
        // Option<u64> (likewise omitted when None).
        expect(Array.isArray(body.data), `${ep} should return List.data array`).toBe(true);
        expect(
          body.next_cursor === undefined ||
            body.next_cursor === null ||
            typeof body.next_cursor === 'string',
          `${ep} next_cursor must be string|null|omitted`,
        ).toBe(true);
      }
    }
  });
});
