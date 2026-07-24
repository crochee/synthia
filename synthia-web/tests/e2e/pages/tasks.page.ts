import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class TasksPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/tasks');
    await this.waitForReady();
  }

  get noTasksCard(): Locator {
    return this.page.locator('.nt-card', { hasText: /no.*task/i });
  }

  get taskCards(): Locator {
    return this.page.locator('.nt-card');
  }

  get taskCount(): Promise<number> {
    return this.taskCards.count();
  }
}
