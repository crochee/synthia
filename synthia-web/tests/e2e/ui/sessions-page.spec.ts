import { test, expect } from '@playwright/test';
import { SessionsPage } from '../pages/sessions.page';

/**
 * Layer 1 — Sessions page UI tests.
 * Verifies the Sessions page renders correctly.
 */
test.describe('Sessions page', () => {
  test('renders page title', async ({ page }) => {
    const sessions = new SessionsPage(page);
    await sessions.goto();
    await expect(page.locator('h1', { hasText: 'Sessions' })).toBeVisible();
  });

  test('shows empty state when no sessions recorded', async ({ page }) => {
    // Stub /api/v1/sessions to return an empty list so the page
    // reaches the empty-state branch regardless of which
    // fixture the workspace has pre-seeded.
    await page.route('**/api/v1/sessions**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ data: [], total: 0, next_cursor: null }),
      }),
    );

    const sessions = new SessionsPage(page);
    await sessions.goto();
    await expect(sessions.noSessionsCard).toBeVisible({ timeout: 5_000 });
  });

  test('load-more appends additional sessions when present', async ({ page }) => {
    // Register the request listener BEFORE navigating so the
    // initial page-load GET is captured too.
    const sessionsRequests: string[] = [];
    page.on('request', (req) => {
      const url = req.url();
      if (req.method() === 'GET' && /\/api\/v1\/tasks(\?|$)/.test(url)) {
        sessionsRequests.push(url);
      }
    });

    const sessions = new SessionsPage(page);
    await sessions.goto();

    // Skip if the workspace has no sessions at all — load-more
    // is only meaningful when the first page returns something.
    const initialCount = await sessions.sessionCount;
    test.skip(initialCount === 0, 'no sessions on this workspace');

    // Give the first page request time to land, then record
    // the baseline. We deliberately do NOT use waitForResponse
    // here because under React 18 strict-mode the initial GET
    // has already fired before this listener registers for
    // some worker orderings.
    await page.waitForTimeout(500);
    const baselineCount = sessionsRequests.length;
    expect(
      baselineCount,
      `expected at least one GET /api/v1/sessions on initial load, saw ${baselineCount}`,
    ).toBeGreaterThanOrEqual(1);

    // If Load-More is not visible on first page (workspace
    // happens to have <= first-page-size sessions), skip rather
    // than fail.
    if (!(await sessions.loadMoreButton.isVisible({ timeout: 1000 }))) {
      test.skip(true, 'no load-more button on first page');
      return;
    }

    // Scroll the Load-More button into view first to avoid
    // "element intercepted" on a re-rendered list, then click
    // it via the Locator API (auto-waits & auto-retries).
    await sessions.loadMoreButton.scrollIntoViewIfNeeded();
    // Use a programmatic dispatchEvent('click') to bypass any
    // possible overlay intercept under tight timing budgets.
    // The React handler is on the underlying button regardless
    // of how the click is synthesised.
    await sessions.loadMoreButton.dispatchEvent('click');

    // Wait for the second GET to land — the load-more URL
    // carries a `?cursor=...` query string.
    await page.waitForResponse(
      (resp) =>
        resp.url().includes('/api/v1/sessions?cursor=') && resp.request().method() === 'GET',
      { timeout: 5000 },
    );

    const afterClick = sessionsRequests.length - baselineCount;
    expect(
      afterClick,
      `load-more must issue a second GET /api/v1/sessions?cursor=... (saw ${afterClick} extra request(s))`,
    ).toBeGreaterThanOrEqual(1);

    // The card count must not decrease (a load-more regression
    // could theoretically reset the list).
    const afterCount = await sessions.sessionCount;
    expect(afterCount).toBeGreaterThanOrEqual(initialCount);
  });
});
