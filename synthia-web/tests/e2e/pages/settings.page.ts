import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class SettingsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/settings');
    await this.waitForReady();
  }

  get providerInput(): Locator {
    return this.page.getByTestId('settings-provider');
  }

  get modelInput(): Locator {
    return this.page.getByTestId('settings-model');
  }

  get saveButton(): Locator {
    return this.page.getByTestId('settings-save');
  }

  async setProvider(value: string): Promise<void> {
    await this.providerInput.fill(value);
  }

  async setModel(value: string): Promise<void> {
    await this.modelInput.fill(value);
  }

  async save(): Promise<void> {
    await this.saveButton.click();
  }
}
