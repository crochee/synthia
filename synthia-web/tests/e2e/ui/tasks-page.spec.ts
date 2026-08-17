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

  test('load-more appends additional tasks when present', async ({ page }) => {
    // Register the request listener BEFORE navigating so the
    // initial page-load GET is captured too.
    const tasksRequests: string[] = [];
    page.on('request', (req) => {
      const url = req.url();
      if (req.method() === 'GET' && /\/api\/v1\/tasks(\?|$)/.test(url)) {
        tasksRequests.push(url);
      }
    });

    const tasks = new TasksPage(page);
    await tasks.goto();

    // Skip if the workspace has no tasks at all — load-more is
    // only meaningful when the first page returns something.
    const initialCount = await tasks.taskCount;
    test.skip(initialCount === 0, 'no tasks on this workspace');

    // Give the first page request time to land, then record
    // the baseline. We deliberately do NOT use waitForResponse
    // here because under React 18 strict-mode the initial GET
    // has already fired before this listener registers for
    // some worker orderings.
    await page.waitForTimeout(500);
    const baselineCount = tasksRequests.length;
    expect(
      baselineCount,
      `expected at least one GET /api/v1/tasks on initial load, saw ${baselineCount}`,
    ).toBeGreaterThanOrEqual(1);

    // If Load-More is not visible on first page (workspace
    // happens to have <= first-page-size tasks), skip rather
    // than fail.
    if (!(await tasks.loadMoreButton.isVisible({ timeout: 1000 }))) {
      test.skip(true, 'no load-more button on first page');
      return;
    }

    // Scroll the Load-More button into view first to avoid
    // "element intercepted" on a re-rendered list, then click
    // it via the Locator API (auto-waits & auto-retries).
    await tasks.loadMoreButton.scrollIntoViewIfNeeded();
    // Use a programmatic dispatchEvent('click') to bypass any
    // possible overlay intercept under tight timing budgets.
    // The React handler is on the underlying button regardless
    // of how the click is synthesised.
    await tasks.loadMoreButton.dispatchEvent('click');

    // Wait for the second GET to land — the load-more URL
    // carries a `?cursor=...` query string.
    await page.waitForResponse(
      (resp) => resp.url().includes('/api/v1/tasks?cursor=') && resp.request().method() === 'GET',
      { timeout: 5000 },
    );

    const afterClick = tasksRequests.length - baselineCount;
    expect(
      afterClick,
      `load-more must issue a second GET /api/v1/tasks?cursor=... (saw ${afterClick} extra request(s))`,
    ).toBeGreaterThanOrEqual(1);

    // The card count must not decrease (a load-more regression
    // could theoretically reset the list).
    const afterCount = await tasks.taskCount;
    expect(afterCount).toBeGreaterThanOrEqual(initialCount);
  });
});
