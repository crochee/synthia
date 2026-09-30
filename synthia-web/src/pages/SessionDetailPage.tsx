import { useEffect, useMemo, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ChatMessageList, type ChatMessageViewItem } from '../components/chat/ChatMessageView';
import {
  BackLink,
  ENTITY_THEMES,
  formatTimestamp,
  PageHero,
  PageShell,
  Property,
  Section,
  StatusPill,
  statusHasDot,
  statusTone,
  shortId,
} from '../components/ui/visual';
import { api } from '../api/client';
import type { OperationSnapshot, SessionArtifact, SessionDetail, SessionPart } from '../api/types';
import { reconstructMessagesFromSession, seedChatFromSession } from '../lib/session-to-messages';
import './SessionDetailPage.css';
import './ChatPage.css';

/**
 * Extract plain text from a list of `SessionPart` objects as
 * serialized by the v1 REST API. Only consults `Part::text`;
 * `Part::data` and other content kinds fall through to empty
 * strings so callers can render the raw JSON for inspection.
 */
function extractPartText(parts: ReadonlyArray<SessionPart> | undefined): string {
  if (!parts) return '';
  return parts.map((p) => (typeof p.text === 'string' ? p.text : '')).join('');
}

/**
 * Try to parse a tool input JSON string and pretty-print it.
 * Tool call arguments on the wire are stored as a `text` part
 * whose body is itself a JSON object (see `tool_call_to_artifact`
 * in `crates/synthia-server/src/session/controller.rs`), so rendering
 * the raw string leaves users looking at `"{\"command\":\"ls\"}"`
 * instead of an inspectable tree. We attempt to parse and
 * pretty-print; on failure we fall back to the raw string so the
 * original payload is never lost.
 */
function prettyJsonOrRaw(raw: string): { formatted: string; parsed: boolean } {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return { formatted: '', parsed: false };
  if (trimmed[0] !== '{' && trimmed[0] !== '[' && trimmed[0] !== '"') {
    return { formatted: raw, parsed: false };
  }
  try {
    const parsed = JSON.parse(trimmed);
    return { formatted: JSON.stringify(parsed, null, 2), parsed: true };
  } catch {
    return { formatted: raw, parsed: false };
  }
}

/**
 * Legacy attachment pairing — keeps the pre-history MVP behavior
 * where `Task.artifacts` carries tool calls / results with a
 * `metadata.kind` discriminator. New tasks route tool turns
 * through `Task.history` (handled by `reconstructMessagesFromSession`
 * + the shared chat renderer below); this fallback is kept for
 * reading sessions completed before history persistence landed.
 *
 * Terminology note: `Task`/`SessionArtifact` are the historical
 * wire-format names retained for the chat protocol. The UI
 * calls the same resource a "session"; the canonical REST
 * endpoint is `/api/v1/sessions/:id`.
 */
interface ToolGroup {
  toolUseId: string;
  toolName?: string;
  call?: SessionArtifact;
  result?: SessionArtifact;
  isError?: boolean;
}

function groupArtifactsByToolUse(artifacts: ReadonlyArray<SessionArtifact>): ToolGroup[] {
  const groups = new Map<string, ToolGroup>();
  const ungrouped: ToolGroup[] = [];

  for (const art of artifacts) {
    const kind = art.metadata?.kind;
    const toolUseId = art.metadata?.tool_use_id;

    if ((kind === 'tool_call' || kind === 'tool_result') && typeof toolUseId === 'string') {
      let group = groups.get(toolUseId);
      if (!group) {
        group = { toolUseId, toolName: art.metadata?.tool_name };
        groups.set(toolUseId, group);
      }
      if (kind === 'tool_call') {
        group.call = art;
        if (art.metadata?.tool_name) group.toolName = art.metadata.tool_name;
      } else {
        group.result = art;
        if (art.metadata?.is_error) group.isError = true;
      }
    } else {
      ungrouped.push({
        toolUseId: art.attachmentId ?? `attachment-${Math.random()}`,
        call: kind === undefined ? art : undefined,
        result: kind === undefined ? undefined : art,
      });
    }
  }

  return [...groups.values(), ...ungrouped];
}

/**
 * Compute a session lifecycle summary from the persisted
 * events. Counts how many user prompts, assistant replies,
 * tool calls, tool results, and sub-agent turns the
 * transcript contains. Cheap O(n) pass used by the property
 * grid at the top of the page.
 */
interface SessionMetrics {
  userTurns: number;
  assistantTurns: number;
  toolCalls: number;
  toolResults: number;
  subAgentTurns: number;
  total: number;
}

function computeSessionMetrics(detail: SessionDetail): SessionMetrics {
  const m: SessionMetrics = {
    userTurns: 0,
    assistantTurns: 0,
    toolCalls: 0,
    toolResults: 0,
    subAgentTurns: 0,
    total: detail.history.length,
  };
  for (const turn of detail.history) {
    const t = (turn.type ?? '').toLowerCase();
    const data = turn.data;
    if (t === 'userinput') m.userTurns += 1;
    else if (t === 'model') m.assistantTurns += 1;
    else if (t === 'agent') {
      // Recursive envelope — the inner event dictates what we
      // count. A delegated sub-agent always wraps a Model /
      // Tool / Agent frame, so peel one layer.
      m.subAgentTurns += 1;
      if (Array.isArray(data) && data.length >= 2) {
        const inner = data[1] as { type?: string };
        const it = (inner?.type ?? '').toLowerCase();
        if (it === 'model') m.assistantTurns += 1;
        else if (it === 'tool_use' || it === 'toolcall') m.toolCalls += 1;
        else if (it === 'tool_result' || it === 'toolresult') m.toolResults += 1;
      }
    } else if (t === 'tool_use' || t === 'toolcall') m.toolCalls += 1;
    else if (t === 'tool_result' || t === 'toolresult') m.toolResults += 1;
  }
  return m;
}

export function SessionDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [sessionDetail, setSessionDetail] = useState<SessionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  /** Live-run snapshot from `GET /api/v1/sessions/:id/status`
   *  (`OperationSnapshot`). `null` = no snapshot yet / not
   *  running / endpoint 404 — all normal, never an error. */
  const [liveSnapshot, setLiveSnapshot] = useState<OperationSnapshot | null>(null);

  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    // Forward an AbortController so the fetch is actually
    // cancelled on unmount or `id` change — without it, a
    // fast route change leaves a dangling socket and can
    // surface an unhandled rejection.
    const controller = new AbortController();
    setLoading(true);
    api
      .get<SessionDetail>(`/api/v1/sessions/${encodeURIComponent(id)}`, controller.signal)
      .then((data) => {
        if (cancelled) return;
        setSessionDetail(data);
        setError(null);
      })
      .catch((e: Error) => {
        if (cancelled || e.name === 'AbortError') return;
        setError(e.message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [id]);

  // Poll the live-run status while the session is working. The
  // endpoint 404s both for an unknown session and for a session
  // with no run in flight — both are "nothing live", so the
  // poll just leaves `liveSnapshot` null. While the snapshot's
  // state is Running / Completing we keep polling (2s); once it
  // reaches a terminal state we stop and keep the last value so
  // the strip shows how the run ended.
  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    let timer: number | undefined;

    const poll = async (): Promise<void> => {
      try {
        const snap = await api.get<OperationSnapshot>(
          `/api/v1/sessions/${encodeURIComponent(id)}/status`,
        );
        if (cancelled) return;
        setLiveSnapshot(snap);
        const state = snap.state.toLowerCase();
        if (state === 'running' || state === 'completing') {
          timer = setTimeout(() => void poll(), 2_000);
        }
      } catch {
        // 404 or network failure — no live run to show.
        if (!cancelled) setLiveSnapshot(null);
      }
    };

    void poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [id]);

  // Group legacy tool_call / tool_result artifacts by
  // tool_use_id for the Artifacts fallback card (older
  // sessions only). Recomputed when the loaded session
  // changes; cheap (O(n)).
  const toolGroups = useMemo(
    () => (sessionDetail ? groupArtifactsByToolUse(sessionDetail.artifacts) : []),
    [sessionDetail],
  );

  const metrics = useMemo(
    () => (sessionDetail ? computeSessionMetrics(sessionDetail) : null),
    [sessionDetail],
  );

  // Reconstruct chat-shaped messages from the persisted
  // session history so the History card renders with the same
  // chat-style cards, role borders, markdown, and tool-block
  // sub-blocks as the live `/chat/:sessionId` page.
  //
  // Status pill propagation matches the live chat page:
  // only assistant messages carry a status, never user
  // messages. We deliberately do NOT paint a status pill on
  // user rows — doing so would diverge from the live chat
  // page. The pill's value is the current session status
  // (e.g. `working`, `completed`) so the user can tell at a
  // glance whether the session they are looking at is still
  // running or has finished — independent of what each
  // individual agent turn was doing.
  const reconstructed = useMemo(
    () => (sessionDetail ? reconstructMessagesFromSession(sessionDetail) : []),
    [sessionDetail],
  );
  const viewMessages: ChatMessageViewItem[] = useMemo(
    () =>
      reconstructed.map((m) => ({
        id: m.id,
        role: m.role,
        subagentDepth: m.role === 'subagent' ? m.subagentDepth : undefined,
        segments: m.segments,
        status: m.role === 'assistant' ? sessionDetail?.status : undefined,
      })),
    [reconstructed, sessionDetail?.status],
  );

  // Wall-clock value for the chat-style renderer. Session-detail
  // viewing is static (no streaming) so the tick is irrelevant;
  // pass `Date.now()` once per render.
  const now = Date.now();

  const historyEmpty =
    !!sessionDetail && sessionDetail.history.length === 0 && reconstructed.length === 0;

  return (
    <PageShell testId="session-detail-page">
      <BackLink to="/sessions" testId="session-detail-back">
        Back to Sessions
      </BackLink>

      <PageHero
        glyph={ENTITY_THEMES.session.glyph}
        heroModifier={ENTITY_THEMES.session.heroModifier}
        title={<code>{`…${shortId(sessionDetail?.id ?? id ?? '')}`}</code>}
        subtitle={
          sessionDetail?.context_id ? (
            <>
              Linked to context <code>{`…${shortId(sessionDetail.context_id)}`}</code>
              {sessionDetail.context_id && (
                <>
                  {' · '}
                  <Link
                    to={`/chat/${encodeURIComponent(sessionDetail.context_id)}`}
                    data-testid="session-detail-continue-chat"
                    onClick={() => {
                      if (sessionDetail)
                        seedChatFromSession(sessionDetail.context_id, sessionDetail);
                    }}
                    style={{ color: 'var(--accent-primary)', textDecoration: 'underline' }}
                  >
                    Continue this session in chat →
                  </Link>
                </>
              )}
            </>
          ) : (
            'Loading session…'
          )
        }
        pills={
          sessionDetail && (
            <>
              <StatusPill
                tone={statusTone(sessionDetail.status)}
                withDot={statusHasDot(sessionDetail.status)}
                testId="session-detail-status-pill"
              >
                {sessionDetail.status}
              </StatusPill>
              {metrics && metrics.total > 0 && (
                <StatusPill tone="cyan">{`${metrics.total} events`}</StatusPill>
              )}
              {liveSnapshot && (
                <>
                  <StatusPill
                    tone={statusTone(liveSnapshot.state)}
                    withDot={statusHasDot(liveSnapshot.state)}
                    testId="session-detail-live-state"
                  >
                    {`live: ${liveSnapshot.state}`}
                  </StatusPill>
                  <StatusPill tone="cyan" testId="session-detail-live-iteration">
                    {`step ${liveSnapshot.iteration + 1}/${liveSnapshot.max_iterations}`}
                  </StatusPill>
                  {(liveSnapshot.usage.input_tokens !== undefined ||
                    liveSnapshot.usage.output_tokens !== undefined) && (
                    <StatusPill tone="neutral" testId="session-detail-live-tokens">
                      {`${liveSnapshot.usage.input_tokens ?? 0}→${liveSnapshot.usage.output_tokens ?? 0} tok`}
                    </StatusPill>
                  )}
                </>
              )}
            </>
          )
        }
        meta={
          sessionDetail && (
            <div className="nt-vis-meta-cluster">
              {sessionDetail.created_at && (
                <div className="nt-vis-meta-cluster__row">
                  <span>Created</span>
                  <span>{formatTimestamp(sessionDetail.created_at)}</span>
                </div>
              )}
              {sessionDetail.updated_at && (
                <div className="nt-vis-meta-cluster__row">
                  <span>Updated</span>
                  <span>{formatTimestamp(sessionDetail.updated_at)}</span>
                </div>
              )}
            </div>
          )
        }
        testId="session-hero"
      />

      {liveSnapshot?.last_error && (
        <div
          className="nt-vis-search-card"
          style={{ borderColor: 'var(--accent-red)' }}
          role="alert"
          data-testid="session-detail-live-error"
        >
          <h2 className="nt-vis-search-card__title">
            <span aria-hidden>⚠️</span>
            <span>Last run failed</span>
          </h2>
          <p className="nt-vis-search-card__hit-content">
            <code>{liveSnapshot.last_error}</code>
          </p>
        </div>
      )}

      {loading && <SkeletonList count={3} testId="session-detail-skeleton" />}
      {error && (
        <EmptyState
          icon="⚠️"
          title="Failed to load session"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="session-detail-error"
        />
      )}
      {!loading && !error && !sessionDetail && (
        <EmptyState
          icon="🔍"
          title="Session not found"
          description="The session may have been deleted, or the URL is malformed."
          testId="session-detail-not-found"
        />
      )}

      {sessionDetail && metrics && (
        <>
          <Section title="Overview" glyph="📋" testId="session-overview">
            <div className="nt-vis-prop-grid">
              <Property
                label="ID"
                value={
                  <code style={{ fontSize: '0.85em' }}>{`…${shortId(sessionDetail.id)}`}</code>
                }
                accent="green"
                testId="session-prop-id"
              />
              <Property
                label="Status"
                value={
                  <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                    <span
                      aria-hidden
                      style={{
                        width: 8,
                        height: 8,
                        borderRadius: '50%',
                        background: `var(--accent-${statusTone(sessionDetail.status) === 'green' ? 'success' : statusTone(sessionDetail.status)})`,
                        display: 'inline-block',
                      }}
                    />
                    {sessionDetail.status}
                  </span>
                }
                accent={statusTone(sessionDetail.status)}
                testId="session-prop-status"
              />
              <Property
                label="Context"
                value={
                  <code
                    style={{ fontSize: '0.85em' }}
                  >{`…${shortId(sessionDetail.context_id)}`}</code>
                }
                accent="cyan"
                testId="session-prop-context"
              />
              <Property
                label="User turns"
                value={metrics.userTurns}
                accent="blue"
                testId="session-prop-user"
              />
              <Property
                label="Assistant turns"
                value={metrics.assistantTurns}
                accent="purple"
                testId="session-prop-assistant"
              />
              <Property
                label="Tool calls"
                value={metrics.toolCalls}
                accent="yellow"
                testId="session-prop-tool-calls"
              />
              <Property
                label="Tool results"
                value={metrics.toolResults}
                accent="green"
                testId="session-prop-tool-results"
              />
              <Property
                label="Sub-agents"
                value={metrics.subAgentTurns}
                accent={metrics.subAgentTurns > 0 ? 'magenta' : undefined}
                testId="session-prop-subagents"
              />
              {sessionDetail.parent_session_id && (
                <Property
                  label="Parent session"
                  value={
                    <Link
                      to={`/sessions/${encodeURIComponent(sessionDetail.parent_session_id)}`}
                      data-testid="session-prop-parent-link"
                    >
                      <code style={{ fontSize: '0.85em' }}>
                        {`…${shortId(sessionDetail.parent_session_id)}`}
                      </code>
                    </Link>
                  }
                  accent="cyan"
                  testId="session-prop-parent"
                />
              )}
              {sessionDetail.agent_name && (
                <Property
                  label="Sub-agent"
                  value={sessionDetail.agent_name}
                  accent="magenta"
                  testId="session-prop-agent-name"
                />
              )}
            </div>
          </Section>

          <Section
            title="History"
            glyph="📜"
            meta={
              metrics.total > 0 ? (
                <StatusPill tone="ghost">{`${metrics.total} events`}</StatusPill>
              ) : null
            }
            testId="session-history"
          >
            {historyEmpty ? (
              <EmptyState
                icon="📭"
                title="No history recorded"
                description="Either the session completed before history persistence landed or no exchanges occurred."
                testId="session-history-empty"
              />
            ) : (
              <ChatMessageList messages={viewMessages} now={now} />
            )}
          </Section>

          {toolGroups.length > 0 && (
            <Section
              title="Artifacts"
              glyph="🗂️"
              meta={<StatusPill tone="ghost">{`${toolGroups.length} attachments`}</StatusPill>}
              testId="session-artifacts"
            >
              {toolGroups.map((group) => {
                const label = group.toolName
                  ? `工具 · ${group.toolName}`
                  : group.call?.name || group.result?.name || group.toolUseId;
                const callText = group.call ? extractPartText(group.call.parts) : '';
                const resultText = group.result ? extractPartText(group.result.parts) : '';
                return (
                  <div
                    key={group.toolUseId}
                    className="nt-session__artifact nt-session__artifact--tool"
                    data-testid={`session-attachment-${group.toolUseId}`}
                  >
                    <div className="nt-session__artifact-header">
                      <code>{label}</code>
                      {group.isError && <span className="nt-session__artifact-error">error</span>}
                    </div>
                    {group.call && (
                      <div className="nt-session__artifact-call">
                        <div className="nt-session__artifact-section-label">请求</div>
                        <pre className="nt-session__artifact-pre">
                          {prettyJsonOrRaw(callText).formatted}
                        </pre>
                      </div>
                    )}
                    {group.result && (
                      <div
                        className={`nt-session__artifact-result${
                          group.isError ? ' nt-session__artifact-result--error' : ''
                        }`}
                      >
                        <div className="nt-session__artifact-section-label">结果</div>
                        <pre className="nt-session__artifact-pre">{resultText}</pre>
                      </div>
                    )}
                    {!group.call && !group.result && (
                      <pre className="nt-session__artifact-pre">
                        {group.toolUseId || '(empty attachment)'}
                      </pre>
                    )}
                  </div>
                );
              })}
            </Section>
          )}

          <div>
            <Link to="/sessions">
              <Button variant="soft">← Back to Sessions</Button>
            </Link>
          </div>
        </>
      )}
    </PageShell>
  );
}
