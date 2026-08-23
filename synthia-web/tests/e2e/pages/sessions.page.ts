import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

export class SessionsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/sessions');
    await this.waitForReady();
  }

  /** Session cards — each card title is an `<h3>` via the Radix
   *  `<Card>` wrapper. The page also has an `<h2>` ("Recent Sessions")
   *  and an `<h1>` ("Sessions"); the card titles are the only `h3`s
   *  on this page. */
  get sessionCards(): Locator {
    return this.page.locator('main h3');
  }

  /** Empty-state card — matched by its visible title text. */
  get noSessionsCard(): Locator {
    return this.page.locator('main h3', { hasText: /no.*session/i });
  }

  /** Load-More button — present when the cursor page has more. */
  get loadMoreButton(): Locator {
    return this.page.getByTestId('sessions-load-more');
  }

  get sessionCount(): Promise<number> {
    return this.sessionCards.count();
  }

  /** Search input on the merged Sessions page (was the standalone
   *  Memory page before memory was folded into sessions). */
  get queryInput(): Locator {
    return this.page.getByTestId('memory-query');
  }

  /** Search button on the merged Sessions page. */
  get searchButton(): Locator {
    return this.page.getByTestId('memory-search');
  }

  /** Container that holds either the result list or the empty
   *  state — used by tests that just want to wait for the
   *  search round-trip to settle. */
  get searchResults(): Locator {
    return this.page.getByTestId('memory-results');
  }

  /** Run a memory search on the merged Sessions page. */
  async search(query: string): Promise<void> {
    await this.queryInput.fill(query);
    await this.searchButton.click();
  }
}
