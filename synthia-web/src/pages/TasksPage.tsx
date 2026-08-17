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
import type { List, ScoreHit, TaskSummary } from '../api/types';
import { shortId } from '../lib/short-id';

/**
 * Tasks page. Lists A2A tasks (cursor-paginated) and exposes a
 * memory search input. The original `/memory` page was merged
 * here because, at the prototype level, memory is just a way to
 * look things up by relevance score and tasks already list
 * session history — the two lived in the same sidebar slot and
 * split navigation only added friction.
 */
export function TasksPage() {
  const {
    items: tasks,
    loading: tasksLoading,
    error: tasksError,
    hasMore: tasksHasMore,
    loadMore: tasksLoadMore,
  } = useCursorList<TaskSummary>('/api/v1/tasks');

  const {
    filtered: visibleTasks,
    query: tasksQuery,
    setQuery: setTasksQuery,
    sortDir: tasksSortDir,
    setSortDir: setTasksSortDir,
    isFiltering: isTasksFiltering,
  } = useListFilter(tasks, {
    match: (t, q) =>
      t.id.toLowerCase().includes(q) ||
      (t.context_id && t.context_id.toLowerCase().includes(q)) ||
      t.status.toLowerCase().includes(q),
    // Tasks default to most-recent first; the user-facing
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
        Tasks
      </Heading>

      <Card title="Search">
        <form
          onSubmit={handleSearchSubmit}
          className="nt-tasks__search"
          role="search"
          aria-label="Search memory"
        >
          <input
            type="search"
            className="nt-tasks__search-input"
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
          <p className="nt-tasks__search-error" role="alert">
            <code>{searchError}</code>
          </p>
        )}
        {hasSearched && !searching && !searchError && (
          <div className="nt-tasks__search-results" data-testid="memory-results">
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
        Recent Tasks
      </Heading>
      <ListToolbar
        query={tasksQuery}
        onQueryChange={setTasksQuery}
        sortDir={tasksSortDir}
        onSortDirChange={setTasksSortDir}
        searchLabel="Tasks"
        testId="tasks-toolbar"
      />
      {tasksError ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load tasks"
          description={tasksError}
          testId="tasks-error"
        />
      ) : tasksLoading && tasks.length === 0 ? (
        <SkeletonList count={3} testId="tasks-skeleton" />
      ) : visibleTasks.length === 0 && !tasksError ? (
        <EmptyState
          icon={isTasksFiltering ? '🔍' : '📭'}
          title={isTasksFiltering ? 'No tasks match your search' : 'No tasks recorded yet'}
          description={
            isTasksFiltering
              ? `No tasks matched "${tasksQuery}". Try clearing the search.`
              : 'Tasks appear here once you send a message through the Chat page.'
          }
          testId="tasks-empty"
        />
      ) : (
        <>
          {visibleTasks.map((task) => (
            <Card key={task.id} title={`Task ${shortId(task.id)}`}>
              <div>
                <code>status: {task.status}</code>
              </div>
              {task.context_id && (
                <div>
                  <code>context: {task.context_id}</code>
                </div>
              )}
              {task.created_at && (
                <div>
                  <code>created: {task.created_at}</code>
                </div>
              )}
              <div className="nt-tasks__actions">
                <Link
                  to={`/tasks/${encodeURIComponent(task.id)}`}
                  data-testid={`task-detail-${task.id}`}
                >
                  <Button variant="soft">View Detail</Button>
                </Link>
                {task.context_id && (
                  <Link
                    to={`/chat/${encodeURIComponent(task.context_id)}`}
                    data-testid={`task-continue-chat-${task.id}`}
                  >
                    <Button variant="soft" color="blue">
                      继续 chat
                    </Button>
                  </Link>
                )}
              </div>
            </Card>
          ))}
        </>
      )}
      {tasksHasMore && (
        <div>
          <Button
            variant="soft"
            onClick={tasksLoadMore}
            disabled={tasksLoading}
            data-testid="tasks-load-more"
          >
            {tasksLoading ? 'Loading...' : 'Load More'}
          </Button>
        </div>
      )}
    </div>
  );
}
