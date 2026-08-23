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

  test('sidebar exposes all 5 routes', async ({ page }) => {
    await page.goto('/chat');
    const sidebar = page.getByRole('navigation', { name: /primary navigation/i });
    await sidebar.waitFor({ state: 'visible' });
    // The sidebar exposes five top-level entries — Chat,
    // Tools, Agents, Skills, Sessions. The historical "Tasks"
    // entry was merged into "Sessions" so we no longer expect
    // it in the sidebar.
    for (const label of ['CHAT', 'TOOLS', 'AGENTS', 'SKILLS', 'SESSIONS']) {
      await expect(sidebar.getByText(label)).toBeVisible();
    }
  });

  test('clicking sidebar links navigates to each page', async ({ page }) => {
    // Wait for ChatPage's session redirect (which replaces the
    // URL with `/chat/:sessionId/agent/:defaultAgent`) to settle
    // before we start clicking — otherwise our first click
    // races the `navigate(..., { replace: true })` and the
    // wrong page renders.
    await page.goto('/chat');
    await page.waitForURL(/\/chat\/.+\/agent\//);

    const visits = [
      { label: 'TOOLS', path: '/tools', url: /\/tools$/ },
      { label: 'SKILLS', path: '/skills', url: /\/skills$/ },
      { label: 'SESSIONS', path: '/sessions', url: /\/sessions$/ },
    ];
    for (const { label, path, url } of visits) {
      // The sidebar link's accessible name is `<label> <shortcut>`
      // (e.g. "Tools g T") because Radix' `Button` injects the
      // `aria-keyshortcuts` into its accessible name. Match by
      // href instead so we don't depend on the visible text.
      const link = page
        .getByRole('navigation', { name: /primary navigation/i })
        .locator(`a[href="${path}"]`);
      await link.click();
      await page.waitForURL(url);
      // The h1 text uses mixed-case labels (e.g. "Sessions");
      // match case-insensitively so the test doesn't break if
      // the label changes back to a single word.
      await expect(page.locator('h1', { hasText: new RegExp(label, 'i') })).toBeVisible();
    }
  });

  test('/tasks redirects to /sessions (legacy alias)', async ({ page }) => {
    // External links / bookmarks to the legacy URL must still
    // resolve to the canonical Sessions page so users do not
    // see a "page not found" error.
    await page.goto('/tasks');
    await page.waitForURL(/\/sessions$/);
    await expect(page.locator('h1', { hasText: /sessions/i })).toBeVisible();
  });
});
