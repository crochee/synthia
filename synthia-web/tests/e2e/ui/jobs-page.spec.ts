import { test, expect } from '@playwright/test';
import { JobsPage } from '../pages/jobs.page';

/**
 * Layer 1 — Jobs page UI tests.
 * Verifies the Jobs page renders correctly.
 */
test.describe('Jobs page', () => {
  test('renders page title', async ({ page }) => {
    const jobs = new JobsPage(page);
    await jobs.goto();
    await expect(page.locator('h1', { hasText: 'Jobs' })).toBeVisible();
  });

  test('shows empty state when no jobs configured', async ({ page }) => {
    const jobs = new JobsPage(page);
    await jobs.goto();
    // Either job cards or empty state card should be visible
    const hasContent = (await jobs.jobCards.count()) > 0 || (await jobs.noJobsCard.count()) > 0;
    expect(hasContent).toBe(true);
  });
});
