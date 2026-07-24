import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class SkillsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/skills');
    await this.waitForReady();
  }

  get skillCards(): Locator {
    return this.page.locator('.nt-card__title');
  }

  /** Click the toggle button for a skill, identified by its card title. */
  async toggleSkill(name: string): Promise<void> {
    const card = this.page.locator('.nt-card', {
      has: this.page.locator('.nt-card__title', { hasText: name }),
    });
    await card.getByRole('button').click();
  }
}
