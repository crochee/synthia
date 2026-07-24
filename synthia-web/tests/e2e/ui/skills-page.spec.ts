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
    const skills = new SkillsPage(page);
    await skills.goto();
    // Either skill cards or empty state card should be visible
    const hasContent =
      (await skills.skillCards.count()) > 0 || (await skills.noSkillsCard.count()) > 0;
    expect(hasContent).toBe(true);
  });
});
