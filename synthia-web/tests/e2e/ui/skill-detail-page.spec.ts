import { test, expect } from '@playwright/test';
import { SkillsPage, SkillDetailPage } from '../pages/skills.page';

/**
 * Layer 1 — Skill detail page UI tests.
 * Verifies the SKILL.md renders as a metadata table + markdown body.
 */
test.describe('Skill detail page', () => {
  test('renders frontmatter table and markdown body for an installed skill', async ({ page }) => {
    const list = new SkillsPage(page);
    await list.goto();
    // Use the first "View" link as a proxy for "a skill card exists".
    const firstView = page.locator('[data-testid^="skill-view-"]').first();
    const viewCount = await firstView.count();
    test.skip(viewCount === 0, 'no skills registered on this workspace');

    await expect(firstView).toBeVisible({ timeout: 5000 });
    const testId = await firstView.getAttribute('data-testid');
    const skillName = testId!.replace('skill-view-', '');

    // Navigate via the "View" link to assert the in-app routing
    // path also works (not just a direct URL hit).
    await list.viewLink(skillName).click();
    await page.waitForURL(new RegExp(`/skills/${skillName}$`));

    const detail = new SkillDetailPage(page, skillName);
    await expect(detail.backButton).toBeVisible({ timeout: 10_000 });

    // Metadata table must show at least the name and the path
    // (always-present keys) — verifies the API + frontmatter
    // parsing are wired up.
    await expect(detail.metaCell('Name')).toContainText(skillName);
    await expect(detail.metaCell('Path')).toBeVisible();

    // The body is either rendered as markdown or shows the
    // "No markdown body" placeholder — both are valid outcomes
    // depending on what the workspace ships.
    const body = detail.markdownBody;
    const placeholder = page.locator('text=No markdown body');
    const hasBody = (await body.count()) > 0;
    const hasPlaceholder = (await placeholder.count()) > 0;
    expect(hasBody || hasPlaceholder).toBe(true);
  });

  test('back link returns to the skills list', async ({ page }) => {
    const list = new SkillsPage(page);
    await list.goto();
    const firstView = page.locator('[data-testid^="skill-view-"]').first();
    const viewCount = await firstView.count();
    test.skip(viewCount === 0, 'no skills registered on this workspace');

    await expect(firstView).toBeVisible({ timeout: 5000 });
    const testId = await firstView.getAttribute('data-testid');
    const skillName = testId!.replace('skill-view-', '');

    await list.viewLink(skillName).click();
    await page.waitForURL(new RegExp(`/skills/${skillName}$`));

    const detail = new SkillDetailPage(page, skillName);
    await detail.backButton.click();
    await page.waitForURL(/\/skills$/);
    await expect(page.locator('h1', { hasText: 'Skills' })).toBeVisible();
  });
});
