import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class MemoryPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/memory');
    await this.waitForReady();
  }

  get queryInput(): Locator {
    return this.page.getByTestId('memory-query');
  }

  get searchButton(): Locator {
    return this.page.getByTestId('memory-search');
  }

  async search(query: string): Promise<void> {
    await this.queryInput.fill(query);
    await this.searchButton.click();
  }
}
