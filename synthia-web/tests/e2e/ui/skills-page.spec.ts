import { test, expect } from '@playwright/test';
import { SkillsPage } from '../pages/skills.page';

/**
 * Layer 1 — Skills page UI tests.
 * Verifies the Skills page renders correctly.
 */
test.describe('Skills page', () => {
  test('renders page title', async ({ page }) => {
    const skills = new SkillsPage(page);
    await skills.goto();
    await expect(page.locator('h1', { hasText: 'Skills' })).toBeVisible();
  });

  test('shows empty state when no skills registered', async ({ page }) => {
    // Stub /api/v1/skills to return an empty list so the page
    // reaches the empty-state branch regardless of which
    // fixture the workspace has pre-seeded.
    await page.route('**/api/v1/skills**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ data: [], total: 0, next_cursor: null }),
      }),
    );

    const skills = new SkillsPage(page);
    await skills.goto();
    await expect(skills.noSkillsCard).toBeVisible({ timeout: 5_000 });
  });
});
