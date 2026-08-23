/**
 * Layer 1 — Modal interaction coverage.
 *
 * Pins the user-facing contract of the lightweight Modal
 * primitive in `components/ui/Modal.tsx`:
 *
 *  - Escape key closes the dialog.
 *  - Clicking the backdrop closes the dialog.
 *  - Body scroll is locked while the dialog is open (so the
 *    list behind it cannot bleed through).
 *  - The Cancel button in the footer closes the dialog.
 *  - The dedicated close button (×) in the header closes the
 *    dialog.
 *  - Clicking inside the dialog does NOT close it (click
 *    events are stop-propagated so they don't bubble to the
 *    backdrop).
 *
 * Runs against the Agents page's "Create Agent" modal, which
 * is the only consumer in the current UI. (See AgentsPage.tsx.)
 */
import { test, expect } from '@playwright/test';

test.describe('Modal interactions', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/api/v1/agents**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ data: [], next_cursor: null, total: 0 }),
      }),
    );
    await page.goto('/agents');
  });

  async function openModal(page: import('@playwright/test').Page) {
    const modal = page.getByTestId('agent-create-modal');
    // Already open? skip.
    if (await modal.isVisible().catch(() => false)) return modal;
    // Some layouts expose the Add button inside the ListToolbar;
    // others use a primary button at the page level. Try both.
    const addBtn = page.getByTestId('agents-toolbar-add');
    if (await addBtn.count()) {
      await addBtn.click();
    } else {
      await page
        .getByRole('button', { name: /create|add|new agent/i })
        .first()
        .click();
    }
    await expect(modal).toBeVisible({ timeout: 5_000 });
    return modal;
  }

  test('Escape key closes the modal', async ({ page }) => {
    const modal = await openModal(page);
    await page.keyboard.press('Escape');
    await expect(modal).toBeHidden({ timeout: 5_000 });
  });

  test('× button closes the modal', async ({ page }) => {
    const modal = await openModal(page);
    await page.getByTestId('agent-create-modal-close').click();
    await expect(modal).toBeHidden({ timeout: 5_000 });
  });

  test('Cancel footer button closes the modal', async ({ page }) => {
    const modal = await openModal(page);
    await page.getByTestId('agent-cancel').click();
    await expect(modal).toBeHidden({ timeout: 5_000 });
  });

  test('clicking the backdrop closes the modal', async ({ page }) => {
    const modal = await openModal(page);
    // The backdrop sits behind the panel — clicking outside
    // the panel (top-left corner, away from the modal body)
    // bubbles up to the backdrop's onClick which closes the
    // dialog. AgentsPage's `<Modal>` carries a `testId` so the
    // backdrop testId is namespaced to `agent-create-modal-backdrop`.
    await page.getByTestId('agent-create-modal-backdrop').click({ position: { x: 5, y: 5 } });
    await expect(modal).toBeHidden({ timeout: 5_000 });
  });

  test('clicking inside the modal does NOT close it', async ({ page }) => {
    const modal = await openModal(page);
    // Click in the middle of the panel — far from the close
    // button. The stopPropagation on the panel prevents the
    // click from bubbling to the backdrop's onClose.
    await page.getByTestId('agent-create-modal').click({ position: { x: 100, y: 60 } });
    await expect(modal).toBeVisible({ timeout: 1_000 });
  });

  test('body scroll is locked while modal is open', async ({ page }) => {
    await openModal(page);
    const bodyOverflow = await page.evaluate(() => document.body.style.overflow);
    expect(bodyOverflow).toBe('hidden');
  });
});
