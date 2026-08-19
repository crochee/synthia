import { test, expect, type Page } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — Responsive / cross-viewport UI tests.
 *
 * Verifies that the Neon Terminal layout reflows correctly across
 * desktop / tablet / mobile viewports without breaking the primary
 * navigation or the chat input experience.
 *
 * - Desktop: 1440×900 — sidebar visible, header full-width
 * - Tablet:   1024×768 — sidebar still visible, layout shrinks
 * - Mobile:    390×844 — sidebar collapses / wraps, input still usable
 */

interface ViewportSpec {
  label: string;
  width: number;
  height: number;
}

const VIEWPORTS: ViewportSpec[] = [
  { label: 'desktop', width: 1440, height: 900 },
  { label: 'tablet', width: 1024, height: 768 },
  { label: 'mobile', width: 390, height: 844 },
];

async function expectCoreUiRenders(page: Page): Promise<void> {
  // Header banner ("SYNTHIA") must always be present.
  await expect(page.getByRole('banner')).toBeVisible();
  // The chat input must be visible on /chat regardless of viewport.
  const chat = new ChatPage(page);
  await expect(chat.input).toBeVisible();
  await expect(chat.sendButton).toBeVisible();
}

for (const vp of VIEWPORTS) {
  test.describe(`responsive layout — ${vp.label} (${vp.width}×${vp.height})`, () => {
    test.use({ viewport: { width: vp.width, height: vp.height } });

    test('homepage chat renders without overflow', async ({ page }) => {
      await page.goto('/chat');
      await expectCoreUiRenders(page);
      // Sidebar must remain accessible; on small viewports the
      // primary nav text is hidden behind the icon column.
      const sidebar = page.getByRole('navigation', { name: /primary navigation/i });
      await sidebar.waitFor({ state: 'visible' });
    });

    test('chat input stays inside viewport', async ({ page }) => {
      await page.goto('/chat');
      const chat = new ChatPage(page);
      const box = await chat.input.boundingBox();
      expect(box, 'chat input should have a bounding box').not.toBeNull();
      // The input must fit horizontally within the viewport.
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(vp.width + 1);
      // And it must be tall enough to type at least one line.
      expect(box!.height).toBeGreaterThan(20);
    });

    test('no horizontal page overflow', async ({ page }) => {
      // Regression: at narrow viewports the sidebar's hard width
      // plus the lack of `min-width: 0` on the row Flex made the
      // <main> column overflow the viewport, forcing horizontal
      // scroll. The bug is invisible to a per-element bounding
      // box check (each element fits), so we measure
      // `documentElement.scrollWidth` directly — that is what the
      // browser actually scrolls.
      await page.goto('/chat');
      const { scrollWidth, clientWidth } = await page.evaluate(() => ({
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth,
      }));
      expect(
        scrollWidth,
        `documentElement.scrollWidth (${scrollWidth}) must not exceed clientWidth (${clientWidth}) on ${vp.label}`,
      ).toBeLessThanOrEqual(vp.width + 1);
    });

    test('all 5 sidebar entries remain reachable', async ({ page }) => {
      await page.goto('/chat');
      const sidebar = page.getByRole('navigation', { name: /primary navigation/i });
      await sidebar.waitFor({ state: 'visible' });
      // Sidebar entries are `<NavLink>` elements with stable `href`
      // values. On mobile (≤ 767px) the label text inside collapses
      // via `display: none`, which removes it from the accessible
      // name; matching by `href` is therefore the viewport-agnostic
      // way to prove every entry is still present in the DOM.
      const hrefs = ['/chat', '/tools', '/agents', '/skills', '/tasks'];
      for (const href of hrefs) {
        await expect(sidebar.locator(`a[href="${href}"]`)).toBeAttached();
      }
    });

    test('typing in the input updates the send button state', async ({ page }) => {
      await page.goto('/chat');
      const chat = new ChatPage(page);
      await expect(chat.sendButton).toBeDisabled();
      await chat.input.fill('hi');
      await expect(chat.sendButton).toBeEnabled();
      await chat.input.fill('');
      await expect(chat.sendButton).toBeDisabled();
    });
  });
}
