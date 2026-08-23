import { useEffect, useMemo, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ChatMessageList, type ChatMessageViewItem } from '../components/chat/ChatMessageView';
import { api } from '../api/client';
import type { SessionArtifact, SessionDetail, SessionPart } from '../api/types';
import { reconstructMessagesFromSession, seedChatFromSession } from '../lib/session-to-messages';
import { shortId } from '../lib/short-id';
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

export function SessionDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [sessionDetail, setSessionDetail] = useState<SessionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

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

  // Group legacy tool_call / tool_result artifacts by
  // tool_use_id for the Artifacts fallback card (older
  // sessions only). Recomputed when the loaded session
  // changes; cheap (O(n)).
  const toolGroups = useMemo(
    () => (sessionDetail ? groupArtifactsByToolUse(sessionDetail.artifacts) : []),
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
    <div className="nt-session-detail">
      <Heading as="h1" size="6">
        Session
      </Heading>
      <p>
        <Link to="/sessions" data-testid="session-detail-back">
          ← Back to Sessions
        </Link>
        {sessionDetail?.context_id && (
          <>
            {' · '}
            <Link
              to={`/chat/${encodeURIComponent(sessionDetail.context_id)}`}
              data-testid="session-detail-continue-chat"
              onClick={() => {
                if (sessionDetail) seedChatFromSession(sessionDetail.context_id, sessionDetail);
              }}
            >
              Continue this session in chat →
            </Link>
          </>
        )}
      </p>
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
      {sessionDetail && (
        <>
          <Card title={`Session ${shortId(sessionDetail.id)}`}>
            <div className="nt-session-detail__summary">
              <code>id: {sessionDetail.id}</code>
            </div>
            <div className="nt-session-detail__summary">
              <code>status:</code>
              <span className={`nt-chat__message-status status-${sessionDetail.status}`}>
                {sessionDetail.status}
              </span>
            </div>
            <div className="nt-session-detail__summary">
              <code>context: {sessionDetail.context_id}</code>
            </div>
            {sessionDetail.created_at && (
              <div className="nt-session-detail__summary">
                <code>created: {sessionDetail.created_at}</code>
              </div>
            )}
            {sessionDetail.updated_at && (
              <div className="nt-session-detail__summary">
                <code>updated: {sessionDetail.updated_at}</code>
              </div>
            )}
          </Card>
          <Card title="History">
            {historyEmpty ? (
              <p data-testid="session-history-empty">
                No history recorded for this session. The session either completed before history
                persistence landed or produced no user / agent exchanges.
              </p>
            ) : (
              <ChatMessageList messages={viewMessages} now={now} />
            )}
          </Card>
          {toolGroups.length > 0 && (
            <Card title="Artifacts">
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
            </Card>
          )}
        </>
      )}
    </div>
  );
}
