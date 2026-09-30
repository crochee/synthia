/**
 * Visual primitives for list / detail pages.
 *
 * Pages had drifted toward the same flat Card + heading +
 * MetadataTable treatment, which was functional but had no
 * visual hierarchy. These primitives give every entity type
 * (skill / tool / agent / session) the same colourful surface
 * treatment while leaving pages free to compose them as they
 * see fit.
 *
 * This module re-exports everything pages need from one
 * place: components from here, and the constants / pure
 * helpers (theme map, status tone, timestamp formatter) from
 * `./visual-utils`. The split keeps the
 * `react-refresh/only-export-components` ESLint rule happy
 * and means pages only need a single import statement.
 *
 * `react-refresh/only-export-components` complains when a
 * file mixes components with plain functions / constants,
 * so visual.tsx imports the helpers it needs at runtime
 * (the `PillTone` type and `formatTimestamp` for the
 * `Property` component) and the public re-exports live
 * alongside the components — the ESLint rule matches the
 * `react-refresh` HMR contract, not the consumer import
 * surface, and the warning disappears once the warnings
 * are refactored out. If the warning resurfaces after
 * changes, push the offending export to `./visual-utils`.
 */

import type { CSSProperties, ReactNode } from 'react';
import type { PillTone } from './visual-utils';

// Re-export the helpers / constants / types from visual-utils
// so pages can `import { ENTITY_THEMES, StatusPill, PageHero }`
// from a single module. The
// `react-refresh/only-export-components` rule wants every file
// to export either only components or only non-components;
// here we want both, so the explicit disable is the documented
// escape hatch (vs. splitting the surface across two modules
// that pages have to import from). The HMR cost is a full
// remount of the page list when this file changes — acceptable
// for a styling/visual vocabulary module that changes rarely.
/* eslint-disable react-refresh/only-export-components */
export {
  ENTITY_THEMES,
  agentTheme,
  formatTimestamp,
  shortId,
  statusHasDot,
  statusTone,
} from './visual-utils';
export type { EntityKind, EntityTheme, PillTone } from './visual-utils';
/* eslint-enable react-refresh/only-export-components */

/* ----------------------------------------------------------------
 * Status pill
 * ----------------------------------------------------------------
 *
 * Small coloured pill used both in list cards (compact) and
 * in hero headers (slightly larger via the hero context). The
 * colour is driven by `tone`; an optional leading dot gives
 * the live-status indicator look on session cards.
 */

interface StatusPillProps {
  tone: PillTone;
  children: ReactNode;
  withDot?: boolean;
  title?: string;
  testId?: string;
}

export function StatusPill({ tone, children, withDot = false, title, testId }: StatusPillProps) {
  return (
    <span className={`nt-vis-pill nt-vis-pill--${tone}`} title={title} data-testid={testId}>
      {withDot && <span className="nt-vis-pill__dot" aria-hidden />}
      {children}
    </span>
  );
}

/* ----------------------------------------------------------------
 * Page chrome
 * ----------------------------------------------------------------
 *
 * The list / detail pages used to wrap themselves in a
 * bare `<div>` and let headings float at the top. This
 * wrapper centres a content column at a comfortable width
 * (1080px) and applies the gap rhythm so cards sit evenly.
 */

interface PageShellProps {
  children: ReactNode;
  testId?: string;
  style?: CSSProperties;
}

export function PageShell({ children, testId, style }: PageShellProps) {
  return (
    <div className="nt-vis-page" data-testid={testId} style={style}>
      {children}
    </div>
  );
}

/* ----------------------------------------------------------------
 * Page hero
 * ----------------------------------------------------------------
 *
 * Coloured header at the top of every list / detail page.
 * Houses the icon tile, the title, optional subtitle, an
 * optional cluster of pills, and an optional right-side
 * metadata cluster (timestamps, totals). The accent colour
 * comes from the `heroModifier` class — pages set it from
 * the entity-theme map.
 */

interface PageHeroProps {
  glyph: string;
  heroModifier: string;
  title: ReactNode;
  subtitle?: ReactNode;
  pills?: ReactNode;
  meta?: ReactNode;
  testId?: string;
}

export function PageHero({
  glyph,
  heroModifier,
  title,
  subtitle,
  pills,
  meta,
  testId,
}: PageHeroProps) {
  return (
    <header className={`nt-vis-hero ${heroModifier}`} data-testid={testId}>
      <div className="nt-vis-hero__icon" aria-hidden>
        {glyph}
      </div>
      <div className="nt-vis-hero__body">
        <h1 className="nt-vis-hero__title">{title}</h1>
        {subtitle && <p className="nt-vis-hero__subtitle">{subtitle}</p>}
        {pills && <div className="nt-vis-hero__pills">{pills}</div>}
      </div>
      {meta && <div className="nt-vis-hero__meta">{meta}</div>}
    </header>
  );
}

/* ----------------------------------------------------------------
 * Section card
 * ----------------------------------------------------------------
 *
 * Replacement for `<Card title="...">` on detail pages. The
 * Radix Card surface is fine for forms, but for content
 * sections (Metadata / Body / Instructions / History) we
 * want a tighter header with an icon glyph and an optional
 * meta line — the same vocabulary used by every other
 * surface in the visual system.
 */

interface SectionProps {
  title: ReactNode;
  glyph?: string;
  meta?: ReactNode;
  children: ReactNode;
  testId?: string;
}

export function Section({ title, glyph, meta, children, testId }: SectionProps) {
  return (
    <section className="nt-vis-section" data-testid={testId}>
      <header className="nt-vis-section__header">
        <h2 className="nt-vis-section__title">
          {glyph && (
            <span className="nt-vis-section__title-glyph" aria-hidden>
              {glyph}
            </span>
          )}
          <span>{title}</span>
        </h2>
        {meta && <div className="nt-vis-section__meta">{meta}</div>}
      </header>
      <div className="nt-vis-section__body">{children}</div>
    </section>
  );
}

/* ----------------------------------------------------------------
 * Property card
 * ----------------------------------------------------------------
 *
 * Single key/value card for the property grid used at the
 * top of detail pages. Replaces one row of the legacy
 * MetadataTable with a denser, more visual surface.
 */

interface PropertyProps {
  label: string;
  value: ReactNode;
  /** Accent rail colour for this card — defaults to none. */
  accent?: PillTone;
  testId?: string;
}

export function Property({ label, value, accent, testId }: PropertyProps) {
  const accentClass = accent ? `nt-vis-prop--accent-${accent}` : '';
  return (
    <div className={`nt-vis-prop ${accentClass}`} data-testid={testId}>
      <span className="nt-vis-prop__label">{label}</span>
      <span className="nt-vis-prop__value">{value}</span>
    </div>
  );
}

/* ----------------------------------------------------------------
 * Back link
 * ----------------------------------------------------------------
 *
 * Inline anchor styled like a "← Back" link. Lives above
 * the hero on detail pages and above the search-card on
 * SessionsPage.
 */

interface BackLinkProps {
  to: string;
  children: ReactNode;
  testId?: string;
}

export function BackLink({ to, children, testId }: BackLinkProps) {
  return (
    <a className="nt-vis-back" href={to} data-testid={testId}>
      <span aria-hidden>←</span>
      <span>{children}</span>
    </a>
  );
}

/* ----------------------------------------------------------------
 * Card grid
 * ----------------------------------------------------------------
 *
 * Light wrapper around the responsive grid so pages don't
 * repeat the class names. `wide` bumps the min column
 * width for a 2-column-heavy layout (AgentsPage).
 */

interface CardGridProps {
  children: ReactNode;
  wide?: boolean;
  testId?: string;
}

export function CardGrid({ children, wide = false, testId }: CardGridProps) {
  return (
    <div className={wide ? 'nt-vis-grid nt-vis-grid--wide' : 'nt-vis-grid'} data-testid={testId}>
      {children}
    </div>
  );
}
