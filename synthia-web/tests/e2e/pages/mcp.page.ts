import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class McpPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/mcp');
    await this.waitForReady();
  }

  get nameInput(): Locator {
    return this.page.getByTestId('mcp-name');
  }

  get urlInput(): Locator {
    return this.page.getByTestId('mcp-url');
  }

  get addButton(): Locator {
    return this.page.getByTestId('mcp-add');
  }

  async addServer(name: string, url: string): Promise<void> {
    await this.nameInput.fill(name);
    await this.urlInput.fill(url);
    await this.addButton.click();
  }
}
