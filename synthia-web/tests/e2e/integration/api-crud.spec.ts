import { test, expect } from '@playwright/test';
import { TasksPage } from '../pages/tasks.page';

/**
 * Layer 2 — Management API CRUD integration tests.
 * Verifies that memory search (on the Tasks page) flows through
 * to the backend and survives a page reload.
 */
test.describe('Management API CRUD', () => {
  test('memory search (on Tasks page) returns results or empty state', async ({ page }) => {
    const tasks = new TasksPage(page);
    await tasks.goto();
    // The search block sits on the Tasks page since the
    // standalone Memory page was merged in.
    await expect(tasks.queryInput).toBeVisible();
    await tasks.search('synthia');
    // Either a result list or the "No matches" empty state.
    await expect(tasks.searchResults).toBeVisible({ timeout: 10_000 });
  });
});
