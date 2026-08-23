import { test } from '@playwright/test';

test('debug nav with getByText semantics', async ({ page }) => {
  page.on('console', (msg) => console.log(`[browser ${msg.type()}]`, msg.text()));
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) console.log(`[nav]`, frame.url());
  });

  await page.goto('/chat');
  await page.waitForTimeout(300);

  const nav = page.getByRole('navigation', { name: /primary navigation/i });
  await nav.waitFor({ state: 'visible' });

  // Test how many matches each pattern has
  const exact = await nav.getByText('Tools', { exact: true }).count();
  const upper = await nav.getByText('TOOLS').count();
  const regex = await nav.getByText(/^TOOLS$/).count();
  console.log('exact Tools=', exact, 'upper TOOLS=', upper, 'regex TOOLS=', regex);

  // Inspect the buttons themselves
  const buttons = await nav.getByRole('button').all();
  for (const b of buttons) {
    const text = await b.textContent();
    console.log('button text:', JSON.stringify(text));
  }
});
