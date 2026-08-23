import type { Page, Locator } from '@playwright/test';

/**
 * Base Page Object Model for synthia-web pages.
 *
 * All page objects extend this so that they share:
 *   - a stable navigation entry point
 *   - a consistent "wait for page ready" helper
 *   - assertion helpers that wait for sidebar/header presence
 */
export abstract class BasePage {
  constructor(protected readonly page: Page) {}

  protected get header(): Locator {
    return this.page.getByRole('banner');
  }

  protected get sidebar(): Locator {
    return this.page.getByRole('navigation', { name: /primary navigation/i });
  }

  abstract goto(): Promise<void>;

  /**
   * Wait until the page header and sidebar are both rendered.
   *
   * Intentionally does NOT call `page.waitForLoadState('networkidle')`:
   * the Chat page polls `/api/v1/chat/usage` every 30s and may
   * open an SSE stream to `/chat/sessions/.../messages/stream`,
   * neither of which ever settle into a true idle state. Waiting
   * for header + sidebar is a sufficient "app shell is up" signal
   * for the page objects that extend this base.
   */
  async waitForReady(): Promise<void> {
    await this.header.waitFor({ state: 'visible' });
    await this.sidebar.waitFor({ state: 'visible' });
  }

  /** Wait for the connection indicator to become ONLINE. */
  async waitForOnline(timeoutMs = 10_000): Promise<void> {
    await this.page.getByText('ONLINE').waitFor({ state: 'visible', timeout: timeoutMs });
  }
}
