import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class SkillsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/skills');
    await this.waitForReady();
  }

  get noSkillsCard(): Locator {
    return this.page.locator('.nt-card', { hasText: /no skills/i });
  }

  get skillCards(): Locator {
    return this.page.locator('.nt-card');
  }
}
