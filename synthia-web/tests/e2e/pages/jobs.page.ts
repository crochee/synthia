import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class JobsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/jobs');
    await this.waitForReady();
  }

  get noJobsCard(): Locator {
    return this.page.locator('.nt-card', { hasText: /no scheduled jobs/i });
  }

  get jobCards(): Locator {
    return this.page.locator('.nt-card');
  }

  getJobCard(key: string): Locator {
    return this.page.locator(`.nt-card`, { hasText: key });
  }

  getJobToggleButton(key: string): Locator {
    return this.page.getByTestId(`job-toggle-${key}`);
  }
}
