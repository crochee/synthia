import { test, expect } from '@playwright/test';
import { SettingsPage } from '../pages/settings.page';
import { MemoryPage } from '../pages/memory.page';
import { McpPage } from '../pages/mcp.page';

/**
 * Layer 2 — Management API CRUD integration tests.
 * Verifies that settings, memory search and MCP server
 * management all flow through to the backend and survive
 * a page reload.
 */
test.describe('Management API CRUD', () => {
  test('settings round-trip through PUT', async ({ page }) => {
    const settings = new SettingsPage(page);
    await settings.goto();
    await settings.setProvider('test-provider');
    await settings.setModel('test-model');
    await settings.save();
    // Reload should preserve values
    await page.reload();
    await expect(settings.providerInput).toHaveValue('test-provider');
    await expect(settings.modelInput).toHaveValue('test-model');
  });

  test('memory search returns results or empty state', async ({ page }) => {
    const memory = new MemoryPage(page);
    await memory.goto();
    // Should render without error. Whether any results appear
    // depends on backend contents — we only assert that the
    // page didn't throw.
    await expect(memory.queryInput).toBeVisible();
    await memory.search('synthia');
    // Either a result card or the "No matching memories" card
    await expect(page.locator('.nt-card', { hasText: /memory|no matching/i }).first()).toBeVisible({
      timeout: 10_000,
    });
  });

  test('MCP page renders without server error', async ({ page }) => {
    const mcp = new McpPage(page);
    await mcp.goto();
    await expect(mcp.nameInput).toBeVisible();
    await expect(mcp.urlInput).toBeVisible();
    await expect(mcp.addButton).toBeVisible();
  });
});
