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

  /** Wait until the page header and sidebar are both rendered. */
  async waitForReady(): Promise<void> {
    await this.header.waitFor({ state: 'visible' });
    await this.sidebar.waitFor({ state: 'visible' });
    await this.page.waitForLoadState('networkidle');
  }

  /** Click a sidebar link by its visible shortcut letter. */
  async navigateByShortcut(shortcut: 'C' | 'T' | 'K' | 'A' | 'M' | 'J' | 'X' | 'S') {
    const map: Record<typeof shortcut, string> = {
      C: 'CHAT',
      T: 'TOOLS',
      K: 'SKILLS',
      A: 'TASKS',
      M: 'MEMORY',
      J: 'JOBS',
      X: 'MCP',
      S: 'SETTINGS',
    };
    await this.sidebar.getByText(map[shortcut]).click();
  }

  /** Wait for the connection indicator to become ONLINE. */
  async waitForOnline(timeoutMs = 10_000): Promise<void> {
    await this.page.getByText('ONLINE').waitFor({ state: 'visible', timeout: timeoutMs });
  }
}
