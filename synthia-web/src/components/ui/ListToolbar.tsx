import { useEffect, useRef, useState, type ReactNode } from 'react';
import type { SortDir } from '../../hooks/useListFilter';

interface ListToolbarProps {
  query: string;
  onQueryChange: (q: string) => void;
  sortDir: SortDir;
  onSortDirChange: (d: SortDir) => void;
  /** Visible label for the search field (a11y + placeholder). */
  searchLabel: string;
  /** Right-side extra controls (e.g. "Add" button). */
  children?: ReactNode;
  testId?: string;
}

/** Debounce window for the search input. 150ms is short enough
 *  that the filter still feels responsive to a fast typist (the
 *  filter runs on the same beat as the last keystroke), but long
 *  enough that we collapse the keystrokes "ab" "abc" "abcd" into
 *  a single filter run for the full word. Below ~100ms a fast
 *  typist (60+ wpm) still triggers a re-filter mid-word; above
 *  ~250ms the lag between input and result starts to feel like a
 *  stall. 150ms is the spot the Material UI / Ant Design defaults
 *  settled on, so we're matching the muscle memory users already
 *  have from those libraries. */
const SEARCH_DEBOUNCE_MS = 150;

/**
 * Shared toolbar at the top of every list page. Combines:
 *
 *   - Search input with built-in 150ms debounce so a fast typist
 *     doesn't trigger a re-filter on every keystroke. The input
 *     itself stays snappy (each keystroke updates the local
 *     `draftQuery` synchronously so the caret never lags); only
 *     the *commit* to `onQueryChange` is debounced.
 *   - Sort direction toggle (A→Z / Z→A).
 *   - Optional slot for an "Add" / "Create" button on the right.
 *
 * The component is pure CSS — no Radix dependency — so it
 * composes inside a Card / page wrapper without forcing a
 * specific layout.
 */
export function ListToolbar({
  query,
  onQueryChange,
  sortDir,
  onSortDirChange,
  searchLabel,
  children,
  testId,
}: ListToolbarProps) {
  // Mirror the committed query into a local draft that drives
  // the input. This lets the input feel instant while the
  // downstream filter only sees the debounced snapshot.
  const [draftQuery, setDraftQuery] = useState(query);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep the draft in sync when the parent resets the query
  // (e.g. external clear, route change). Without this effect,
  // a `setQuery('')` from the parent would leave the input
  // showing stale text because the draft only ever updates
  // on local keystrokes.
  useEffect(() => {
    setDraftQuery(query);
  }, [query]);

  // Cleanup pending timer on unmount so a fast route change
  // doesn't fire `onQueryChange` after the page is gone (which
  // would otherwise trigger a stale-state update on the next
  // mount of a list page using the same hook).
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const handleSearchChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const next = e.target.value;
    setDraftQuery(next);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      onQueryChange(next);
      timerRef.current = null;
    }, SEARCH_DEBOUNCE_MS);
  };

  return (
    <div
      role="search"
      aria-label={`${searchLabel} controls`}
      data-testid={testId}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 'var(--spacing-sm)',
        marginBottom: 'var(--spacing-md)',
        flexWrap: 'wrap',
      }}
    >
      <input
        type="search"
        value={draftQuery}
        onChange={handleSearchChange}
        placeholder={`Search ${searchLabel.toLowerCase()}...`}
        aria-label={`Search ${searchLabel}`}
        data-testid={`${testId}-search`}
        style={{
          flex: '1 1 240px',
          minWidth: 200,
          padding: '6px 10px',
          background: 'var(--bg-secondary)',
          border: '1px solid var(--border-strong)',
          borderRadius: 'var(--radius-sm)',
          color: 'var(--text-primary)',
          fontFamily: 'var(--font-sans)',
          fontSize: 'var(--fs-sm)',
        }}
      />
      <button
        type="button"
        onClick={() => onSortDirChange(sortDir === 'asc' ? 'desc' : 'asc')}
        aria-label={`Sort ${sortDir === 'asc' ? 'Z→A' : 'A→Z'}`}
        data-testid={`${testId}-sort`}
        style={{
          padding: '6px 10px',
          background: 'var(--bg-secondary)',
          border: '1px solid var(--border-strong)',
          borderRadius: 'var(--radius-sm)',
          color: 'var(--text-secondary)',
          cursor: 'pointer',
          fontFamily: 'inherit',
          fontSize: 'var(--fs-sm)',
        }}
      >
        Sort {sortDir === 'asc' ? 'A→Z' : 'Z→A'}
      </button>
      {children && <div style={{ marginLeft: 'auto' }}>{children}</div>}
    </div>
  );
}
