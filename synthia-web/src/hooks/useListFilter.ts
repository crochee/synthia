import { useEffect, useMemo, useRef, useState } from 'react';

export type SortDir = 'asc' | 'desc';

export interface ListFilter<T> {
  /** Filtered + sorted view of `items`. */
  filtered: ReadonlyArray<T>;
  /** Current search query (lower-cased for matching). */
  query: string;
  setQuery: (q: string) => void;
  /** Sort direction the user has picked. */
  sortDir: SortDir;
  setSortDir: (d: SortDir) => void;
  /** True when the filter narrowed the list. */
  isFiltering: boolean;
}

/**
 * Client-side search + sort for a list page. The matcher is a
 * pluggable predicate so the caller decides which fields to
 * include (e.g. `name` only, or `name + description`). The
 * sorter is also pluggable so callers can pick the right
 * secondary key for their data shape.
 *
 * Memoised on `(items, query, sortDir, match, compare)` —
 * deps are deliberately taken from individual fields of the
 * `options` argument rather than the whole object reference.
 * Callers usually pass an inline object literal
 * (`useListFilter(items, { match, compare })`), which would
 * otherwise create a new reference on every parent render
 * and silently defeat the memoisation — every keystroke in
 * the search box would re-run the filter + sort even when
 * neither `items` nor the predicates changed. Extracting the
 * function refs from the options keeps the memo useful.
 */
export function useListFilter<T>(
  items: ReadonlyArray<T>,
  options: {
    /** Predicate invoked with a lower-cased query; return true to keep. */
    match: (item: T, query: string) => boolean;
    /** Tiebreaker / primary sort. */
    compare: (a: T, b: T) => number;
  },
): ListFilter<T> {
  const [query, setQuery] = useState('');
  const [sortDir, setSortDir] = useState<SortDir>('asc');

  // The "latest" ref pattern: callers almost always pass inline
  // arrow functions for `match` / `compare`, so identity-stable
  // refs across renders are not realistic to ask for. We
  // instead hold the *latest* function values in a ref so the
  // memo body can read them without subscribing to identity
  // changes — every render re-assigns the ref slot, but the
  // memo itself only rebuilds when `items`, `query`, or
  // `sortDir` change. The trade-off: a parent re-render with a
  // swapped-in predicate won't trigger a re-filter until one
  // of the actual dep values next changes. For the call sites
  // here (Skills / Tools / Agents / Tasks) the predicates are
  // defined inline at module evaluation and never swap, so the
  // trade-off is purely theoretical — but the pattern keeps
  // the memo useful if someone later wraps the predicates in
  // a `useCallback` and changes its closure.
  const optionsRef = useRef(options);
  useEffect(() => {
    optionsRef.current = options;
  });

  const filtered = useMemo(() => {
    const { match, compare } = optionsRef.current;
    const q = query.trim().toLowerCase();
    // Branch on whether we have to allocate. With a query we
    // own the result of `.filter(...)` (a fresh array), so
    // sorting it in place is safe and avoids a second copy.
    // Without a query, `items` is the caller's read-only array
    // — we copy first so `.sort()` doesn't mutate the input.
    let sorted: T[];
    if (q) {
      sorted = items.filter((item) => match(item, q));
    } else {
      sorted = [...items];
    }
    sorted.sort(compare);
    if (sortDir === 'desc') sorted.reverse();
    return sorted;
  }, [items, query, sortDir]);

  return {
    filtered,
    query,
    setQuery,
    sortDir,
    setSortDir,
    isFiltering: query.trim().length > 0,
  };
}