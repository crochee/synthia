import type { ReactNode } from 'react';

/**
 * Reusable cell helpers for `<MetadataTable>`. Kept in a sibling
 * file (rather than co-located with the component) so the
 * component module only exports components — required by the
 * `react-refresh/only-export-components` ESLint rule.
 */

/**
 * Renders an array of strings as small teal pills separated by
 * whitespace. Returns `undefined` (and the table will skip the
 * row) when the list is empty or all whitespace.
 */
export function stringListCell(items: ReadonlyArray<string>): ReactNode {
  const clean = items.map((s) => s.trim()).filter((s) => s.length > 0);
  if (clean.length === 0) return undefined;
  return (
    <>
      {clean.map((s) => (
        <span key={s} className="nt-pill">
          {s}
        </span>
      ))}
    </>
  );
}

/** Render a single value as a pill (used for short badges). */
export function pillCell(value: string): ReactNode {
  return <span className="nt-pill">{value}</span>;
}

/** Muted placeholder for intentionally-empty cells. */
export function emptyCell(): ReactNode {
  return <span className="nt-pill nt-pill--muted">—</span>;
}
