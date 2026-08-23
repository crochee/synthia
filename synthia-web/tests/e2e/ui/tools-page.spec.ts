import { test, expect } from '@playwright/test';
import { ToolsPage } from '../pages/tools.page';

/**
 * Layer 1 — Tools page UI tests.
 * Verifies the Tools page renders correctly and shows tool cards.
 */
test.describe('Tools page', () => {
  test('renders page title and content', async ({ page }) => {
    const tools = new ToolsPage(page);
    await tools.goto();
    await expect(page.locator('h1', { hasText: 'Tools' })).toBeVisible();
    await expect(tools.toolCards.first()).toBeVisible({ timeout: 10_000 });
  });

  test('shows empty state when no tools registered', async ({ page }) => {
    // Stub /api/v1/tools to return an empty list so the page
    // reaches the empty-state branch regardless of which
    // fixture the workspace has pre-seeded.
    await page.route('**/api/v1/tools**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ data: [], total: 0, next_cursor: null }),
      }),
    );

    const tools = new ToolsPage(page);
    await tools.goto();
    await expect(page.locator('main h3', { hasText: /no tools/i })).toBeVisible({
      timeout: 5_000,
    });
  });
});
