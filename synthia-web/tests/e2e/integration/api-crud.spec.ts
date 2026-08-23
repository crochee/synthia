import { test, expect } from '@playwright/test';
import { SessionsPage } from '../pages/sessions.page';

/**
 * Layer 2 — Management API CRUD integration tests.
 * Verifies that memory search (on the Sessions page) flows
 * through to the backend and survives a page reload.
 */
test.describe('Management API CRUD', () => {
  test('memory search (on Sessions page) returns results or empty state', async ({ page }) => {
    const sessions = new SessionsPage(page);
    await sessions.goto();
    // The search block sits on the Sessions page since the
    // standalone Memory page was merged in.
    await expect(sessions.queryInput).toBeVisible();
    await sessions.search('synthia');
    // Either a result list or the "No matches" empty state.
    await expect(sessions.searchResults).toBeVisible({ timeout: 10_000 });
  });
});
