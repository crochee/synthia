import { useState, useCallback, useEffect, useRef, useMemo, type FormEvent } from 'react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { Link, useNavigate } from 'react-router-dom';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import { ListToolbar } from '../components/ui/ListToolbar';
import { Modal } from '../components/ui/Modal';
import { useToast } from '../hooks/useToast';
import { api } from '../api/client';
import type { ForkSessionResponse, List, SearchHit, SessionSummary } from '../api/types';
import {
  ENTITY_THEMES,
  PageHero,
  PageShell,
  StatusPill,
  CardGrid,
  statusHasDot,
  statusTone,
  formatTimestamp,
  shortId,
} from '../components/ui/visual';

const SESSION_THEME = ENTITY_THEMES.session;

/** Hit cap for the unified search — the backend clamps to the same
 *  number; stating it here keeps the " Showing the top N hits "
 *  footnote honest. */
const SEARCH_LIMIT = 20;

/**
 * Map a session status to the right `nt-vis-card` accent
 * class. Working sessions get a pending accent, failed
 * sessions get the error variant, others fall back to the
 * default green. The accent shows up as the top ribbon and
 * the icon tile gradient.
 */
function sessionCardModifier(status: string | undefined): string {
  if (!status) return SESSION_THEME.cardModifier;
  const s = status.toLowerCase();
  if (s === 'failed' || s === 'error' || s === 'errored' || s === 'cancelled') {
    return 'nt-vis-card--session-error';
  }
  if (
    s === 'working' ||
    s === 'running' ||
    s === 'in_progress' ||
    s === 'pending' ||
    s === 'streaming'
  ) {
    return 'nt-vis-card--session-pending';
  }
  return SESSION_THEME.cardModifier;
}

/** Where a hit navigates: session hits deep-link into the
 *  transcript; everything else has a detail page keyed by the
 *  id's suffix (skill:pdf → /skills/pdf, agent:x → /agents/x). */
function hitHref(hit: SearchHit): string | null {
  switch (hit.domain) {
    case 'session':
      return `/sessions/${encodeURIComponent(hit.id)}`;
    case 'skill':
    case 'agent': {
      const name = hit.id.split(':')[1];
      if (!name) return null;
      return `/${hit.domain === 'skill' ? 'skills' : 'agents'}/${encodeURIComponent(name)}`;
    }
    case 'tool':
    case 'mcp':
      return `/tools/${encodeURIComponent(hit.id)}`;
    default:
      return null;
  }
}

/**
 * Sessions page. Lists chat sessions (cursor-paginated) and
 * carries the unified cross-domain search card: one query fans
 * out across the server's tool / mcp / skill / memory / agent /
 * session catalogs (`GET /api/v1/search`) — the same engine the
 * agent's `search` tool queries, so the UI and the model agree on
 * what exists.
 */
export function SessionsPage() {
  const {
    items: sessions,
    loading: sessionsLoading,
    error: sessionsError,
    hasMore: sessionsHasMore,
    loadMore: sessionsLoadMore,
    setItems: setSessions,
  } = useCursorList<SessionSummary>('/api/v1/sessions');

  const toast = useToast();
  const navigate = useNavigate();
  /** Session awaiting delete confirmation, and the per-card
   *  in-flight flag that keeps the confirm button from double-firing. */
  const [confirmTarget, setConfirmTarget] = useState<SessionSummary | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Record<string, boolean>>({});
  const [pendingFork, setPendingFork] = useState<Record<string, boolean>>({});

  /**
   * `DELETE /api/v1/sessions/{id}` — the durable transcript goes with
   * it, so the card is dropped from the list rather than re-fetched.
   * A turn that is still running answers `409`; the message the toast
   * shows says to cancel it first.
   */
  const deleteSession = async (session: SessionSummary): Promise<void> => {
    setConfirmTarget(null);
    setPendingDelete((prev) => ({ ...prev, [session.id]: true }));
    try {
      await api.del(`/api/v1/sessions/${encodeURIComponent(session.id)}`);
      setSessions((prev) => prev.filter((s) => s.id !== session.id));
      toast.push({
        variant: 'success',
        message: `Deleted session "${shortId(session.id)}".`,
      });
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setPendingDelete((prev) => ({ ...prev, [session.id]: false }));
    }
  };

  /**
   * Download the durable transcript (`GET /api/v1/sessions/{id}/export`).
   * Through the API client, not a bare `<a href>`: the route is
   * auth-gated and a link cannot carry the key.
   */
  const exportSession = async (session: SessionSummary): Promise<void> => {
    try {
      const response = await api.getRaw(
        `/api/v1/sessions/${encodeURIComponent(session.id)}/export`,
      );
      const href = URL.createObjectURL(await response.blob());
      const link = document.createElement('a');
      link.href = href;
      link.download = `${session.id}.jsonl`;
      link.click();
      URL.revokeObjectURL(href);
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    }
  };

  /**
   * `POST /api/v1/sessions/{id}/fork` — branch this conversation into
   * a new one that inherits its transcript (the whole log; the route
   * also takes a `from_stream_index` cutoff) and then open the branch.
   * Opening it is the point: the branch is where you keep going, and
   * the original is left exactly as it was.
   */
  const forkSession = async (session: SessionSummary): Promise<void> => {
    setPendingFork((prev) => ({ ...prev, [session.id]: true }));
    try {
      const forked = await api.post<ForkSessionResponse>(
        `/api/v1/sessions/${encodeURIComponent(session.id)}/fork`,
        {},
      );
      toast.push({
        variant: 'success',
        message: `Forked ${shortId(session.id)} → ${shortId(forked.session_id)} (${forked.rows} rows).`,
      });
      navigate(`/chat/${encodeURIComponent(forked.session_id)}`);
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setPendingFork((prev) => ({ ...prev, [session.id]: false }));
    }
  };

  const {
    filtered: visibleSessions,
    query: sessionsQuery,
    setQuery: setSessionsQuery,
    sortDir: sessionsSortDir,
    setSortDir: setSessionsSortDir,
    isFiltering: isSessionsFiltering,
  } = useListFilter(sessions, {
    match: (s, q) => {
      if (s.id.toLowerCase().includes(q)) return true;
      if (s.status.toLowerCase().includes(q)) return true;
      if (s.context_id && s.context_id.toLowerCase().includes(q)) return true;
      return false;
    },
    compare: (a, b) => a.id.localeCompare(b.id),
  });

  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchHit[]>([]);
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
      const response = await api.get<List<SearchHit>>(
        `/api/v1/search?q=${encodeURIComponent(trimmed)}&limit=${SEARCH_LIMIT}`,
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
  // empty query as an explicit "no active search" and wipe the
  // results panel.
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

  // Aggregate stats for the hero pills. Counting statuses
  // here is cheap (one pass) and lets the user tell at a
  // glance "we have 30 sessions, 2 are still running, 1
  // failed" without scrolling the list.
  const sessionStats = useMemo(() => {
    let active = 0;
    let failed = 0;
    let completed = 0;
    for (const s of sessions) {
      const k = s.status.toLowerCase();
      if (k === 'working' || k === 'running' || k === 'in_progress' || k === 'pending') {
        active += 1;
      } else if (k === 'failed' || k === 'error' || k === 'cancelled') {
        failed += 1;
      } else if (k === 'completed' || k === 'done') {
        completed += 1;
      }
    }
    return { active, failed, completed, total: sessions.length };
  }, [sessions]);

  const heroPills = (
    <>
      <StatusPill tone="green" testId="sessions-total-pill">
        {`${sessionStats.total} sessions`}
      </StatusPill>
      {sessionStats.active > 0 && (
        <StatusPill tone="yellow" withDot>
          {`${sessionStats.active} active`}
        </StatusPill>
      )}
      {sessionStats.completed > 0 && (
        <StatusPill tone="green">{`${sessionStats.completed} completed`}</StatusPill>
      )}
      {sessionStats.failed > 0 && (
        <StatusPill tone="red">{`${sessionStats.failed} failed`}</StatusPill>
      )}
      {isSessionsFiltering && (
        <StatusPill tone="cyan" withDot>
          filtering · {visibleSessions.length}/{sessionStats.total}
        </StatusPill>
      )}
    </>
  );

  return (
    <PageShell testId="sessions-page">
      <PageHero
        glyph={SESSION_THEME.glyph}
        heroModifier={SESSION_THEME.heroModifier}
        title="Sessions"
        subtitle="Every chat session, then and now. Open a session to read its transcript, or search every catalog — tools, skills, memory, agents and past conversations — from one box."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>List</span>
              <span>/api/v1/sessions</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>Search</span>
              <span>/api/v1/search</span>
            </div>
          </div>
        }
        testId="sessions-hero"
      />

      {/* Unified cross-domain search — its own visually distinct
          card so it doesn't get lost in the list rhythm. One query,
          every catalog: the same registry the agent's `search` tool
          fans out across. */}
      <div className="nt-vis-search-card">
        <h2 className="nt-vis-search-card__title">
          <span aria-hidden>🔍</span>
          <span>Search everything</span>
        </h2>
        <form
          onSubmit={handleSearchSubmit}
          className="nt-vis-search-card__form"
          role="search"
          aria-label="Search all resources"
        >
          <input
            type="search"
            placeholder="Search tools, skills, memory, agents, sessions…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            data-testid="unified-query"
            aria-label="Search all resources"
          />
          <Button
            type="submit"
            variant="solid"
            disabled={searching || !query.trim()}
            data-testid="unified-search"
          >
            {searching ? 'Searching...' : 'Search'}
          </Button>
        </form>
        {searchError && (
          <p className="nt-vis-search-card__error" role="alert">
            <code>{searchError}</code>
          </p>
        )}
        {hasSearched && !searching && !searchError && (
          <div className="nt-vis-search-card__results" data-testid="unified-results">
            {results.length === 0 ? (
              <p className="nt-vis-search-card__empty">No matches for &ldquo;{query}&rdquo;.</p>
            ) : (
              results.map((hit) => {
                const href = hitHref(hit);
                const meta = (
                  <>
                    <span className="nt-vis-search-card__hit-domain">{hit.domain}</span>
                    <code>{hit.id}</code>
                    <span>· score {hit.score.toFixed(2)}</span>
                  </>
                );
                const body = (
                  <p className="nt-vis-search-card__hit-content">{hit.preview ?? hit.title}</p>
                );
                return (
                  <div
                    key={`${hit.domain}:${hit.id}`}
                    className="nt-vis-search-card__hit"
                    data-testid={`unified-result-${hit.domain}-${hit.id}`}
                  >
                    <div className="nt-vis-search-card__hit-meta">
                      {href ? <Link to={href}>{meta}</Link> : meta}
                    </div>
                    {body}
                  </div>
                );
              })
            )}
            {results.length >= SEARCH_LIMIT && (
              <p className="nt-vis-search-card__empty">
                Showing the top {results.length} hits — refine the query to narrow further.
              </p>
            )}
          </div>
        )}
      </div>

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
        <SkeletonList count={6} testId="sessions-skeleton" />
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
        <CardGrid testId="sessions-grid">
          {visibleSessions.map((session) => {
            const detailHref = `/sessions/${encodeURIComponent(session.id)}`;
            const chatHref = session.context_id
              ? `/chat/${encodeURIComponent(session.context_id)}`
              : null;
            const createdAt = formatTimestamp(session.created_at);
            const tone = statusTone(session.status);
            return (
              <div
                key={session.id}
                className={`nt-vis-card ${sessionCardModifier(session.status)}`}
                data-testid={`session-card-${session.id}`}
              >
                <div className="nt-vis-card__header">
                  <div className="nt-vis-card__icon" aria-hidden>
                    {SESSION_THEME.glyph}
                  </div>
                  <div className="nt-vis-card__main">
                    <h3 className="nt-vis-card__name">
                      <code title={session.id}>{`…${shortId(session.id)}`}</code>
                    </h3>
                    <span className="nt-vis-card__subtitle">
                      {createdAt ? `started ${createdAt}` : 'session'}
                    </span>
                  </div>
                </div>
                {session.context_id && (
                  <p className="nt-vis-card__desc">
                    <span style={{ color: 'var(--text-muted)' }}>context </span>
                    <code
                      style={{ fontSize: 'var(--fs-xs)' }}
                    >{`…${shortId(session.context_id)}`}</code>
                  </p>
                )}
                <div className="nt-vis-card__footer">
                  <div className="nt-vis-card__pills">
                    <StatusPill tone={tone} withDot={statusHasDot(session.status)}>
                      {session.status}
                    </StatusPill>
                    {session.parent_session_id && (
                      <StatusPill tone="cyan" testId={`session-subagent-${session.id}`}>
                        {`子代理${session.agent_name ? ` · ${session.agent_name}` : ''}`}
                      </StatusPill>
                    )}
                  </div>
                  <div className="nt-vis-card__actions">
                    {chatHref && (
                      <Link to={chatHref} data-testid={`session-continue-chat-${session.id}`}>
                        <Button variant="soft" size="1" color="blue">
                          Continue
                        </Button>
                      </Link>
                    )}
                    <Link to={detailHref} data-testid={`session-detail-${session.id}`}>
                      <Button variant="solid" size="1">
                        Open →
                      </Button>
                    </Link>
                    <Button
                      variant="soft"
                      size="1"
                      color="gray"
                      disabled={pendingFork[session.id] === true}
                      onClick={() => void forkSession(session)}
                      data-testid={`session-fork-${session.id}`}
                    >
                      {pendingFork[session.id] === true ? 'Forking…' : 'Fork'}
                    </Button>
                    <Button
                      variant="soft"
                      size="1"
                      color="gray"
                      onClick={() => void exportSession(session)}
                      data-testid={`session-export-${session.id}`}
                    >
                      Export
                    </Button>
                    <Button
                      variant="soft"
                      size="1"
                      color="red"
                      disabled={pendingDelete[session.id] === true}
                      onClick={() => setConfirmTarget(session)}
                      data-testid={`session-delete-${session.id}`}
                    >
                      Delete
                    </Button>
                  </div>
                </div>
              </div>
            );
          })}
        </CardGrid>
      )}

      {sessionsHasMore && (
        <div style={{ display: 'flex', justifyContent: 'center' }}>
          <Button
            variant="soft"
            onClick={sessionsLoadMore}
            disabled={sessionsLoading}
            data-testid="sessions-load-more"
          >
            {sessionsLoading ? 'Loading...' : 'Load More Sessions'}
          </Button>
        </div>
      )}
      <Modal
        open={confirmTarget !== null}
        onClose={() => setConfirmTarget(null)}
        title="Delete session"
        testId="session-delete-modal"
        footer={
          <>
            <Button
              variant="soft"
              onClick={() => setConfirmTarget(null)}
              data-testid="session-delete-cancel"
            >
              Cancel
            </Button>
            <Button
              variant="solid"
              color="red"
              disabled={confirmTarget ? pendingDelete[confirmTarget.id] === true : false}
              onClick={() => confirmTarget && void deleteSession(confirmTarget)}
              data-testid="session-delete-confirm"
            >
              Delete
            </Button>
          </>
        }
      >
        <p>
          Delete session <code>{confirmTarget ? shortId(confirmTarget.id) : ''}</code>? Its durable
          transcript is removed with it. This cannot be undone.
        </p>
      </Modal>
    </PageShell>
  );
}
