import { test, expect } from '@playwright/test';

/**
 * Layer 1 — Detail page UI tests.
 *
 * Covers the two inspector pages that no other spec exercises:
 *   - `/agents/:name` (AgentDetailPage)
 *   - `/tools/:name`  (ToolDetailPage)
 *
 * Each page renders a read-only metadata table backed by a
 * single `GET /api/v1/<resource>/:name` call. The error path
 * (failed fetch) is covered too so we know the EmptyState +
 * "Back" button are wired up.
 */
test.describe('Agent detail page', () => {
  test('renders metadata table and instructions for a known agent', async ({ page }) => {
    // Stub the list so /agents has something to click on.
    await page.route('**/api/v1/agents**', (route) => {
      // /api/v1/agents list vs /api/v1/agents/:name detail —
      // distinguish by URL: detail requests carry a path
      // segment after `/agents/`.
      const url = route.request().url();
      if (/\/api\/v1\/agents\/[^/]+$/.test(url)) {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            name: 'demo-agent',
            kind: 'react',
            version: '0.1.0',
            description: 'A demo agent used by the e2e test.',
            capabilities: ['code-review', 'summarise'],
            tools: ['skill', 'memory_search'],
            handoffs: [],
            owner: 'e2e-suite',
            domain: 'general',
            persona: 'helpful reviewer',
            modelHint: 'claude-sonnet-4-6',
            protected: true,
            instructions: '# demo-agent\n\nYou are a demo agent.\n',
          }),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          data: [{ name: 'demo-agent', protected: true }],
          next_cursor: null,
          total: 1,
        }),
      });
    });

    await page.goto('/agents');
    await page.getByTestId('agent-row-demo-agent').click();
    await page.waitForURL(/\/agents\/demo-agent$/);

    // The page header includes the agent name; the back
    // button lives under it.
    await expect(page.locator('h1', { hasText: 'demo-agent' })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('agent-detail-back')).toBeVisible();
    // Built-in pill surfaces for protected descriptors.
    await expect(page.getByTestId('agent-protected-pill')).toBeVisible();

    // Metadata table rows — labels render in `<th scope="row">`
    // which the accessibility tree exposes as `rowheader`
    // (not `columnheader`). Filter the row by its rowheader.
    await expect(
      page.getByRole('row').filter({ has: page.getByRole('rowheader', { name: 'Name' }) }),
    ).toBeVisible();
    const sourceRow = page
      .getByRole('row')
      .filter({ has: page.getByRole('rowheader', { name: 'Source' }) });
    await expect(sourceRow).toContainText('built-in');
    const kindRow = page
      .getByRole('row')
      .filter({ has: page.getByRole('rowheader', { name: 'Kind' }) });
    await expect(kindRow).toContainText('react');
    // Instructions markdown block.
    await expect(page.getByTestId('agent-instructions-markdown')).toBeVisible();
  });

  test('shows EmptyState when the agent detail fetch fails', async ({ page }) => {
    await page.route('**/api/v1/agents/ghost', (route) =>
      route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'not_found', message: 'Agent ghost not found' } }),
      }),
    );

    await page.goto('/agents/ghost');
    await expect(page.getByTestId('agent-detail-error')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('agent-detail-error')).toContainText('Agent ghost not found');
  });
});

test.describe('Tool detail page', () => {
  test('renders metadata and input schema for a known tool', async ({ page }) => {
    await page.route('**/api/v1/tools**', (route) => {
      const url = route.request().url();
      if (/\/api\/v1\/tools\/[^/]+$/.test(url)) {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            name: 'skill',
            description: 'Load a named skill into the prompt context.',
            provenance: 'core',
            input_schema: {
              type: 'object',
              properties: {
                name: { type: 'string', description: 'Skill identifier' },
              },
              required: ['name'],
            },
          }),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          data: [{ name: 'skill', description: 'Load a named skill.' }],
          next_cursor: null,
          total: 1,
        }),
      });
    });

    await page.goto('/tools');
    await page.getByTestId('tool-link-skill').click();
    await page.waitForURL(/\/tools\/skill$/);

    await expect(page.locator('h1', { hasText: 'Tool: skill' })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('tool-detail-back')).toBeVisible();
    // Provenance surfaces inline as a `<code>` tag.
    await expect(page.locator('main code', { hasText: 'provenance: core' })).toBeVisible();
    // The serialised JSON schema lands in the pre/code block.
    await expect(page.locator('main pre code', { hasText: '"name"' })).toBeVisible();
  });

  test('shows EmptyState when the tool detail fetch fails', async ({ page }) => {
    await page.route('**/api/v1/tools/ghost', (route) =>
      route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'not_found', message: 'Tool ghost not found' } }),
      }),
    );

    await page.goto('/tools/ghost');
    await expect(page.getByTestId('tool-detail-error')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('tool-detail-error')).toContainText('Tool ghost not found');
  });
});
