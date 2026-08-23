import { useState, useCallback, useEffect, useRef, type FormEvent } from 'react';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { Link } from 'react-router-dom';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import { ListToolbar } from '../components/ui/ListToolbar';
import { api } from '../api/client';
import type { List, ScoreHit, SessionSummary } from '../api/types';
import { shortId } from '../lib/short-id';

/**
 * Sessions page. Lists chat sessions (cursor-paginated) and
 * exposes a memory search input. The original `/memory` page
 * was merged here because, at the prototype level, memory is
 * just a way to look things up by relevance score and sessions
 * already list chat history — the two lived in the same
 * sidebar slot and split navigation only added friction.
 *
 * Backed by `GET /api/v1/sessions` (the management listing).
 * UI text, routes (`/sessions/:id`), and the data-testid
 * namespace all use `session*`.
 */
export function SessionsPage() {
  const {
    items: sessions,
    loading: sessionsLoading,
    error: sessionsError,
    hasMore: sessionsHasMore,
    loadMore: sessionsLoadMore,
  } = useCursorList<SessionSummary>('/api/v1/sessions');

  const {
    filtered: visibleSessions,
    query: sessionsQuery,
    setQuery: setSessionsQuery,
    sortDir: sessionsSortDir,
    setSortDir: setSessionsSortDir,
    isFiltering: isSessionsFiltering,
  } = useListFilter(sessions, {
    match: (s, q) =>
      s.id.toLowerCase().includes(q) ||
      (s.context_id && s.context_id.toLowerCase().includes(q)) ||
      s.status.toLowerCase().includes(q),
    // Sessions default to most-recent first; the user-facing
    // sort toggle re-flips them.
    compare: (a, b) => {
      const aTs = a.created_at ?? '';
      const bTs = b.created_at ?? '';
      return aTs.localeCompare(bTs);
    },
  });

  const [query, setQuery] = useState('');
  const [results, setResults] = useState<ScoreHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [hasSearched, setHasSearched] = useState(false);

  // Tracks the in-flight search so a fast typing / re-submit
  // doesn't race two responses into `setResults`. Cleanup
  // cancels the previous fetch when the user navigates away
  // mid-search — without this, leaving the page during a
  // search would leave a dangling socket and surface an
  // unhandled rejection in the browser console.
  const searchAbortRef = useRef<AbortController | null>(null);

  const runSearch = useCallback(async (q: string) => {
    const trimmed = q.trim();
    if (!trimmed) {
      if (searchAbortRef.current) searchAbortRef.current.abort();
      setResults([]);
      setHasSearched(false);
      setSearchError(null);
      return;
    }
    if (searchAbortRef.current) searchAbortRef.current.abort();
    const controller = new AbortController();
    searchAbortRef.current = controller;
    setSearching(true);
    setSearchError(null);
    setHasSearched(true);
    try {
      const response = await api.get<List<ScoreHit>>(
        `/api/v1/memory/search?q=${encodeURIComponent(trimmed)}`,
        controller.signal,
      );
      if (searchAbortRef.current !== controller) return;
      setResults(response.data);
    } catch (e) {
      if ((e as Error).name === 'AbortError') return;
      if (searchAbortRef.current !== controller) return;
      setSearchError((e as Error).message);
      setResults([]);
    } finally {
      if (searchAbortRef.current === controller) {
        searchAbortRef.current = null;
        setSearching(false);
      }
    }
  }, []);

  // Cleanup on unmount so navigating away mid-search doesn't
  // leave a dangling fetch.
  useEffect(() => {
    return () => {
      if (searchAbortRef.current) searchAbortRef.current.abort();
    };
  }, []);

  // When the user clears the search box (clicks the native
  // ✕ on `type="search"`, or selects all + deletes), the input
  // fires `onChange` but `runSearch` is only called from
  // submit. Without this effect, the stale `results` block
  // would keep showing the previous query's hits — confusing
  // because the user just visibly cleared the box. Treat an
  // empty query as an explicit "no active search" and wipe
  // the results panel.
  useEffect(() => {
    if (query.trim() === '' && (results.length > 0 || hasSearched)) {
      if (searchAbortRef.current) searchAbortRef.current.abort();
      setResults([]);
      setHasSearched(false);
      setSearchError(null);
    }
  }, [query, results.length, hasSearched]);

  const handleSearchSubmit = (e: FormEvent) => {
    e.preventDefault();
    void runSearch(query);
  };

  return (
    <div>
      <Heading as="h1" size="6">
        Sessions
      </Heading>

      <Card title="Search">
        <form
          onSubmit={handleSearchSubmit}
          className="nt-sessions__search"
          role="search"
          aria-label="Search memory"
        >
          <input
            type="search"
            className="nt-sessions__search-input"
            placeholder="Search memory (skills)..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            data-testid="memory-query"
            aria-label="Search memory"
          />
          <Button type="submit" disabled={searching || !query.trim()} data-testid="memory-search">
            {searching ? 'Searching...' : 'Search'}
          </Button>
        </form>
        {searchError && (
          <p className="nt-sessions__search-error" role="alert">
            <code>{searchError}</code>
          </p>
        )}
        {hasSearched && !searching && !searchError && (
          <div className="nt-sessions__search-results" data-testid="memory-results">
            {results.length === 0 ? (
              <p>No matches for &ldquo;{query}&rdquo;.</p>
            ) : (
              <ul>
                {results.map((hit) => (
                  <li key={hit.id} data-testid={`memory-result-${hit.id}`}>
                    <code>{hit.id}</code>
                    {typeof hit.score === 'number' && (
                      <span> &middot; score {hit.score.toFixed(2)}</span>
                    )}
                    <p>{hit.content}</p>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </Card>

      <Heading as="h2" size="4">
        Recent Sessions
      </Heading>
      <ListToolbar
        query={sessionsQuery}
        onQueryChange={setSessionsQuery}
        sortDir={sessionsSortDir}
        onSortDirChange={setSessionsSortDir}
        searchLabel="Sessions"
        testId="sessions-toolbar"
      />
      {sessionsError ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load sessions"
          description={sessionsError}
          testId="sessions-error"
        />
      ) : sessionsLoading && sessions.length === 0 ? (
        <SkeletonList count={3} testId="sessions-skeleton" />
      ) : visibleSessions.length === 0 && !sessionsError ? (
        <EmptyState
          icon={isSessionsFiltering ? '🔍' : '📭'}
          title={isSessionsFiltering ? 'No sessions match your search' : 'No sessions recorded yet'}
          description={
            isSessionsFiltering
              ? `No sessions matched "${sessionsQuery}". Try clearing the search.`
              : 'Sessions appear here once you send a message through the Chat page.'
          }
          testId="sessions-empty"
        />
      ) : (
        <>
          {visibleSessions.map((session) => (
            <Card key={session.id} title={`Session ${shortId(session.id)}`}>
              <div>
                <code>status: {session.status}</code>
              </div>
              {session.context_id && (
                <div>
                  <code>context: {session.context_id}</code>
                </div>
              )}
              {session.created_at && (
                <div>
                  <code>created: {session.created_at}</code>
                </div>
              )}
              <div className="nt-sessions__actions">
                <Link
                  to={`/sessions/${encodeURIComponent(session.id)}`}
                  data-testid={`session-detail-${session.id}`}
                >
                  <Button variant="soft">View Detail</Button>
                </Link>
                {session.context_id && (
                  <Link
                    to={`/chat/${encodeURIComponent(session.context_id)}`}
                    data-testid={`session-continue-chat-${session.id}`}
                  >
                    <Button variant="soft" color="blue">
                      Continue chat
                    </Button>
                  </Link>
                )}
              </div>
            </Card>
          ))}
        </>
      )}
      {sessionsHasMore && (
        <div>
          <Button
            variant="soft"
            onClick={sessionsLoadMore}
            disabled={sessionsLoading}
            data-testid="sessions-load-more"
          >
            {sessionsLoading ? 'Loading...' : 'Load More'}
          </Button>
        </div>
      )}
    </div>
  );
}
