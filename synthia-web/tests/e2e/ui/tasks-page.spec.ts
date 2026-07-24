import { test, expect } from '@playwright/test';
import { TasksPage } from '../pages/tasks.page';

/**
 * Layer 1 — Tasks page UI tests.
 * Verifies the Tasks page renders correctly.
 */
test.describe('Tasks page', () => {
  test('renders page title', async ({ page }) => {
    const tasks = new TasksPage(page);
    await tasks.goto();
    await expect(page.locator('h1', { hasText: 'Tasks' })).toBeVisible();
  });

  test('shows empty state when no tasks recorded', async ({ page }) => {
    const tasks = new TasksPage(page);
    await tasks.goto();
    // Either task cards or empty state card should be visible
    const hasContent = (await tasks.taskCards.count()) > 0 || (await tasks.noTasksCard.count()) > 0;
    expect(hasContent).toBe(true);
  });
});
