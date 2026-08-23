/**
 * Cursor-based pagination hook for v1 list endpoints.
 *
 * Wraps the typical "fetch first page on mount, append more on
 * demand" pattern used by every list page (`/skills`, `/tools`,
 * `/sessions`). Pages that need extra query parameters beyond
 * `cursor`/`limit`/`sort` build the path themselves and fetch
 * directly — the sessions page does this for `/memory/search`.
 *
 * The hook resets the accumulated items whenever `path` (or any
 * of the static `opts`) changes, then fetches the first page.
 * Calling `loadMore()` while a request is in flight or when
 * there is no `next_cursor` is a no-op.
 */
import { useCallback, useEffect, useRef, useState } from 'react';

import { api } from '../api/client';
import type { List } from '../api/types';

export interface UseCursorListOptions {
  /** Page size hint. The server clamps to [1, 100]. */
  limit?: number;
  /** Sort field; prefix with `-` for descending. */
  sort?: string;
}

export interface UseCursorListResult<T> {
  items: T[];
  loading: boolean;
  error: string | null;
  hasMore: boolean;
  total: number | null;
  loadMore: () => Promise<void>;
  refresh: () => Promise<void>;
  /// Imperative local update so callers can apply a server-confirmed
  /// response (e.g. a `PUT` reply) without an extra `GET` round-trip.
  /// Use sparingly; prefer `refresh()` when the server might have
  /// mutated more than the one item you patched.
  setItems: React.Dispatch<React.SetStateAction<T[]>>;
}

function buildUrl(path: string, cursor: string | null, opts: UseCursorListOptions): string {
  const params = new URLSearchParams();
  if (cursor) params.set('cursor', cursor);
  if (opts.limit !== undefined) params.set('limit', String(opts.limit));
  if (opts.sort) params.set('sort', opts.sort);
  const qs = params.toString();
  return qs ? `${path}?${qs}` : path;
}

export function useCursorList<T>(
  path: string,
  opts: UseCursorListOptions = {},
): UseCursorListResult<T> {
  const [items, setItems] = useState<T[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Mirror mutable state into refs so the stable `loadMore` callback
  // can read the latest values without re-creating on every render.
  const nextCursorRef = useRef<string | null>(null);
  nextCursorRef.current = nextCursor;
  const loadingRef = useRef(false);
  loadingRef.current = loading;

  // Tracks the most-recent in-flight `AbortController`. Both the
  // mount-reset effect and `loadMore()` route through `fetchPage`
  // so centralizing the controller here ensures only one request
  // is ever pending per hook instance. When the list page
  // unmounts (user navigates away, or React strict-mode tears
  // down the component to re-mount it) the controller is aborted,
  // which fails the in-flight fetch with `AbortError` rather than
  // letting the network socket dangle until the server answers.
  const inFlightRef = useRef<AbortController | null>(null);

  // Stringify the opts to detect changes (limit/sort). Inline
  // because `opts` is a fresh object on every render otherwise.
  const optsKey = `${opts.limit ?? ''}|${opts.sort ?? ''}`;

  const fetchPage = useCallback(
    async (cursor: string | null, replace: boolean) => {
      // Abort the previous in-flight request, if any, so a fast
      // path/opts change doesn't pile up two concurrent requests
      // racing for `setItems` ownership.
      if (inFlightRef.current) inFlightRef.current.abort();
      const controller = new AbortController();
      inFlightRef.current = controller;
      setLoading(true);
      setError(null);
      try {
        const result = await api.get<List<T>>(buildUrl(path, cursor, opts), controller.signal);
        // Guard against a stale response landing after the caller
        // (or the effect) has already moved on to a new request.
        // Without this, an aborted-then-resumed request could
        // clobber the fresh data set in the meanwhile.
        if (inFlightRef.current === controller) {
          setItems((prev) => (replace ? result.data : [...prev, ...result.data]));
          setNextCursor(result.next_cursor ?? null);
          setTotal(result.total ?? null);
        }
      } catch (e) {
        // AbortError fires when the effect is torn down before
        // the request resolves — that's expected, and the
        // post-await `setItems` is already guarded by the
        // controller-identity check above. Swallow it.
        if ((e as Error).name !== 'AbortError' && inFlightRef.current === controller) {
          setError((e as Error).message);
        }
      } finally {
        if (inFlightRef.current === controller) {
          inFlightRef.current = null;
          setLoading(false);
        }
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [path, optsKey],
  );

  // Reset + initial load whenever the path or static opts change.
  useEffect(() => {
    setItems([]);
    setNextCursor(null);
    setTotal(null);
    void fetchPage(null, true);
    // Cleanup: abort whatever the initial fetch is doing so a
    // quick path / opts change doesn't leak. The next render
    // (or unmount) replaces this controller.
    return () => {
      if (inFlightRef.current) inFlightRef.current.abort();
    };
  }, [fetchPage]);

  const loadMore = useCallback(async () => {
    if (loadingRef.current) return;
    const cursor = nextCursorRef.current;
    if (!cursor) return;
    await fetchPage(cursor, false);
  }, [fetchPage]);

  const refresh = useCallback(async () => {
    await fetchPage(null, true);
  }, [fetchPage]);

  return {
    items,
    loading,
    error,
    hasMore: nextCursor !== null,
    total,
    loadMore,
    refresh,
    setItems,
  };
}
