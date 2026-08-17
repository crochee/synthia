import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class ToolsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/tools');
    await this.waitForReady();
  }

  /** Card titles — Radix Themes `<Card>` renders its title as an
   *  `<h3>` via our `<Card>` wrapper, so the title list is the
   *  list of `h3` elements under `<main>`. */
  get toolCards(): Locator {
    return this.page.locator('main h3');
  }
}
