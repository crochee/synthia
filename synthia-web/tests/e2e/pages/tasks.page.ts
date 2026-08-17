import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class TasksPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/tasks');
    await this.waitForReady();
  }

  /** Task cards — each card title is an `<h3>` via the Radix
   *  `<Card>` wrapper. The page also has an `<h2>` ("Recent Tasks")
   *  and an `<h1>` ("Tasks"); the card titles are the only `h3`s
   *  on this page. */
  get taskCards(): Locator {
    return this.page.locator('main h3');
  }

  /** Empty-state card — matched by its visible title text. */
  get noTasksCard(): Locator {
    return this.page.locator('main h3', { hasText: /no.*task/i });
  }

  /** Load-More button — present when the cursor page has more. */
  get loadMoreButton(): Locator {
    return this.page.getByTestId('tasks-load-more');
  }

  get taskCount(): Promise<number> {
    return this.taskCards.count();
  }

  /** Search input on the merged Tasks page (was the standalone
   *  Memory page before memory was folded into tasks). */
  get queryInput(): Locator {
    return this.page.getByTestId('memory-query');
  }

  /** Search button on the merged Tasks page. */
  get searchButton(): Locator {
    return this.page.getByTestId('memory-search');
  }

  /** Container that holds either the result list or the empty
   *  state — used by tests that just want to wait for the
   *  search round-trip to settle. */
  get searchResults(): Locator {
    return this.page.getByTestId('memory-results');
  }

  /** Run a memory search on the merged Tasks page. */
  async search(query: string): Promise<void> {
    await this.queryInput.fill(query);
    await this.searchButton.click();
  }
}
