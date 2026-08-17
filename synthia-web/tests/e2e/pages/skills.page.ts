import type { Locator, Page } from '@playwright/test';
import { BasePage } from './base.page';

export class SkillsPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/skills');
    await this.waitForReady();
  }

  /** Empty-state card — Radix `<Card>` renders its title in an
   *  `<h3>`. Match the card by the visible title text. */
  get noSkillsCard(): Locator {
    return this.page.locator('main h3', { hasText: /no skills/i });
  }

  /** Skill cards — each card's title is rendered as an `<h3>` via
   *  the Radix `<Card>` wrapper. Excludes the page heading (`h1`)
   *  and the empty-state heading. */
  get skillCards(): Locator {
    return this.page.locator('main h3');
  }

  /** "View" link that navigates to a skill's detail page. */
  viewLink(name: string): Locator {
    return this.page.locator(`[data-testid="skill-view-${name}"]`);
  }
}

export class SkillDetailPage extends BasePage {
  constructor(page: Page, name: string) {
    super(page);
    this._name = name;
  }

  private readonly _name: string;

  override async goto(): Promise<void> {
    await this.page.goto(`/skills/${encodeURIComponent(this._name)}`);
    await this.waitForReady();
  }

  /** Metadata table cell value for a given row label. */
  metaCell(label: string): Locator {
    return this.page
      .locator('.nt-meta-table')
      .locator('tr', { has: this.page.locator('th', { hasText: label }) })
      .locator('td');
  }

  get markdownBody(): Locator {
    return this.page.locator('[data-testid="skill-markdown-body"]');
  }

  get backButton(): Locator {
    return this.page.locator('[data-testid="skill-detail-back"]');
  }
}
