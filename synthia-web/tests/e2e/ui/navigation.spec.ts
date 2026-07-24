import { test, expect } from '@playwright/test';

/**
 * Layer 1 — UI navigation tests.
 * Verifies the React Router routes resolve and the sidebar
 * can reach every page registered in MainLayout.
 */
test.describe('UI navigation', () => {
  test('redirects / to /chat', async ({ page }) => {
    await page.goto('/');
    await page.waitForURL(/\/chat/);
  });

  test('sidebar exposes all 8 routes', async ({ page }) => {
    await page.goto('/chat');
    const sidebar = page.getByRole('navigation', { name: /primary navigation/i });
    await sidebar.waitFor({ state: 'visible' });
    for (const label of ['CHAT', 'TOOLS', 'SKILLS', 'TASKS', 'MEMORY', 'JOBS', 'MCP', 'SETTINGS']) {
      await expect(sidebar.getByText(label)).toBeVisible();
    }
  });

  test('clicking sidebar links navigates to each page', async ({ page }) => {
    await page.goto('/chat');

    const visits = [
      { label: 'TOOLS', url: /\/tools$/ },
      { label: 'SKILLS', url: /\/skills$/ },
      { label: 'SETTINGS', url: /\/settings$/ },
      { label: 'TASKS', url: /\/tasks$/ },
      { label: 'MEMORY', url: /\/memory$/ },
      { label: 'JOBS', url: /\/jobs$/ },
      { label: 'MCP', url: /\/mcp$/ },
    ];
    for (const { label, url } of visits) {
      await page
        .getByRole('navigation', { name: /primary navigation/i })
        .getByText(label)
        .click();
      await page.waitForURL(url);
      await expect(page.locator('h1', { hasText: label })).toBeVisible();
    }
  });
});
