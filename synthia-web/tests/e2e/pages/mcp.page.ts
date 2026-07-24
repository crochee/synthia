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

  get commandInput(): Locator {
    return this.page.getByTestId('mcp-command');
  }

  get argsInput(): Locator {
    return this.page.getByTestId('mcp-args');
  }

  get addButton(): Locator {
    return this.page.getByTestId('mcp-add');
  }

  async addServer(name: string, command: string, args?: string): Promise<void> {
    await this.nameInput.fill(name);
    await this.commandInput.fill(command);
    if (args) {
      await this.argsInput.fill(args);
    }
    await this.addButton.click();
  }
}
