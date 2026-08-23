import { test, expect } from '@playwright/test';

/**
 * Live runtime audit — verify the rendered DOM at the key
 * user-facing routes contains zero "Task" substrings and at
 * least one "Session" substring. The probe also confirms
 * the page h1 matches "Sessions" / "Session" / etc.
 *
 * Filename starts with `_` so Playwright's default `testMatch`
 * skips it; this is an audit probe, not a regression test.
 */
test.describe('runtime audit — task/session UI surface', () => {
  test('sidebar shows Session entry, not Task', async ({ page }) => {
    await page.goto('/chat');
    await page
      .getByRole('navigation', { name: /primary navigation/i })
      .waitFor({ state: 'visible' });

    const sidebar = page.getByRole('navigation', { name: /primary navigation/i });
    const sidebarText = await sidebar.innerText();
    expect(sidebarText.toLowerCase()).toContain('sessions');
    expect(sidebarText.toLowerCase()).not.toContain('tasks');
  });

  test('header breadcrumb on /sessions shows Sessions', async ({ page }) => {
    await page.goto('/sessions');
    await expect(page.locator('main h1').first()).toHaveText('Sessions', {
      timeout: 15_000,
    });

    const header = page.getByRole('banner');
    const headerText = await header.innerText();
    expect(headerText.toLowerCase()).not.toContain('task');
  });

  test('main body on /sessions contains Sessions and no Task', async ({ page }) => {
    await page.goto('/sessions');
    await expect(page.locator('main h1').first()).toHaveText('Sessions', {
      timeout: 15_000,
    });

    const main = page.getByRole('main');
    const mainText = await main.innerText();
    expect(mainText.toLowerCase()).not.toContain('task');
    expect(mainText).toContain('Sessions');
  });

  test('legacy /tasks route navigates to /sessions', async ({ page }) => {
    await page.goto('/tasks');
    await expect(page).toHaveURL(/\/sessions(?:\?.*)?$/, { timeout: 15_000 });
    await expect(page.locator('main h1').first()).toHaveText('Sessions', {
      timeout: 15_000,
    });
  });

  test('legacy /tasks/:id route navigates to /sessions', async ({ page }) => {
    await page.goto('/tasks/any-uuid');
    await expect(page).toHaveURL(/\/sessions(?:\?.*)?$/, { timeout: 15_000 });
    await expect(page.locator('main h1').first()).toHaveText('Sessions', {
      timeout: 15_000,
    });
  });
});
