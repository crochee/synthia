import { test, expect } from '@playwright/test';

/**
 * Layer 1 — Agents page UI tests.
 *
 * Verifies the agents page renders list-first: a list row for
 * each registered agent is visible on load, and the registration
 * form is hidden behind a "Create Agent" modal rather than
 * pinned above the list.
 */
test.describe('Agents page', () => {
  test('renders list and Create Agent button before opening modal', async ({ page }) => {
    await page.goto('/agents');
    await expect(page.locator('h1', { hasText: 'Agents' })).toBeVisible();
    // The "Create Agent" button lives in the toolbar — it's
    // visible without any user interaction.
    await expect(page.getByTestId('agents-create')).toBeVisible();
    // The form must not be visible until the user opens the
    // modal — its inputs are gated behind the dialog.
    await expect(page.getByTestId('agent-create-form')).toHaveCount(0);
    // At least one row should be visible — the backend ships
    // with the built-in "react" agent.
    await expect(page.getByTestId('agents-list')).toBeVisible({ timeout: 10_000 });
  });

  test('Create Agent modal opens, validates, and closes', async ({ page }) => {
    await page.goto('/agents');
    await expect(page.getByTestId('agents-create')).toBeVisible();
    await page.getByTestId('agents-create').click();
    // Modal panel + form become visible.
    await expect(page.getByTestId('agent-create-modal')).toBeVisible();
    await expect(page.getByTestId('agent-create-form')).toBeVisible();
    // Register button must be disabled until Name + Description
    // are non-empty.
    await expect(page.getByTestId('agent-submit')).toBeDisabled();
    await page.getByTestId('agent-name').fill('demo-agent');
    await page.getByTestId('agent-description').fill('A demo agent');
    await expect(page.getByTestId('agent-submit')).toBeEnabled();
    // Close via Cancel button — modal unmounts.
    await page.getByTestId('agent-cancel').click();
    await expect(page.getByTestId('agent-create-modal')).toHaveCount(0);
  });

  test('Escape closes the Create Agent modal', async ({ page }) => {
    await page.goto('/agents');
    await page.getByTestId('agents-create').click();
    await expect(page.getByTestId('agent-create-modal')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('agent-create-modal')).toHaveCount(0);
  });

  test('clicking a list row navigates to the agent detail page', async ({ page }) => {
    await page.goto('/agents');
    const firstRow = page.locator('[data-testid^="agent-row-"]').first();
    await expect(firstRow).toBeVisible({ timeout: 10_000 });
    await firstRow.click();
    await expect(page).toHaveURL(/\/agents\/.+/);
  });
});