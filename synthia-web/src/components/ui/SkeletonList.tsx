import type { CSSProperties } from 'react';

interface SkeletonListProps {
  /** How many placeholder rows to render. Defaults to 4. */
  count?: number;
  /** Tailwind-style width seed so the row widths feel varied. */
  seedWidths?: ReadonlyArray<string>;
  /** Optional test hook. */
  testId?: string;
}

const DEFAULT_WIDTHS: ReadonlyArray<string> = ['80%', '64%', '92%', '70%', '55%'];

/**
 * Lightweight loading placeholder for list pages. Each row is
 * a single rounded rect with a left-to-right shimmer driven by
 * CSS keyframes (`nt-skeleton-shimmer`). The component is pure
 * CSS — no animations library, no JS timers — so it never
 * triggers a re-render and has effectively zero runtime cost.
 *
 * The width distribution is randomised once at mount via the
 * `seedWidths` prop (deterministic if you want stable tests).
 */
export function SkeletonList({
  count = 4,
  seedWidths = DEFAULT_WIDTHS,
  testId,
}: SkeletonListProps) {
  const rows = Array.from({ length: count }, (_, i) => seedWidths[i % seedWidths.length]);
  return (
    <div
      role="status"
      aria-busy="true"
      aria-live="polite"
      data-testid={testId}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--spacing-md)',
      }}
    >
      {rows.map((width, i) => (
        <div
          key={i}
          className="nt-skeleton-row"
          style={{ '--skeleton-width': width } as CSSProperties}
        >
          <span className="nt-skeleton-bar nt-skeleton-bar--title" />
          <span className="nt-skeleton-bar nt-skeleton-bar--line" />
          <span className="nt-skeleton-bar nt-skeleton-bar--line nt-skeleton-bar--short" />
        </div>
      ))}
    </div>
  );
}
