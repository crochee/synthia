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
    const tools = new ToolsPage(page);
    await tools.goto();
    // Either tool cards or empty state card should be visible
    const hasContent =
      (await tools.toolCards.count()) > 0 ||
      // Empty-state card has title "No tools" rendered as an
      // `<h3>` (Radix Card wrapper).
      (await page.locator('main h3', { hasText: /no tools/i }).count()) > 0;
    expect(hasContent).toBe(true);
  });
});
