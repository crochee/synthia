import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class ToolsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/tools');
    await this.waitForReady();
  }

  get toolCards(): Locator {
    return this.page.locator('.nt-card__title');
  }
}
