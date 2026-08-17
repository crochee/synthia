import { useEffect, useMemo, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ChatMessageList, type ChatMessageViewItem } from '../components/chat/ChatMessageView';
import { api } from '../api/client';
import type { TaskArtifact, TaskDetail, TaskPart } from '../api/types';
import { reconstructMessagesFromTask, seedChatFromTask } from '../lib/task-to-messages';
import { shortId } from '../lib/short-id';
import './TaskDetailPage.css';
import './ChatPage.css';

/**
 * Extract plain text from a list of A2A `Part` objects as
 * serialized by the v1 REST API. Only consults `Part::text`;
 * `Part::data` and other content kinds fall through to empty
 * strings so callers can render the raw JSON for inspection.
 */
function extractPartText(parts: ReadonlyArray<TaskPart> | undefined): string {
  if (!parts) return '';
  return parts.map((p) => (typeof p.text === 'string' ? p.text : '')).join('');
}

/**
 * Try to parse a tool input JSON string and pretty-print it.
 * Tool call arguments on the wire are stored as a `text` part
 * whose body is itself a JSON object (see `tool_call_to_artifact`
 * in `crates/synthia-server/src/a2a/mapping.rs`), so rendering
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
 * Legacy artifact pairing — keeps the pre-history MVP behavior
 * where `Task.artifacts` carries tool calls / results with a
 * `metadata.kind` discriminator. New tasks route tool turns
 * through `Task.history` (handled by `reconstructMessagesFromTask`
 * + the shared chat renderer below); this fallback is kept for
 * reading tasks completed before history persistence landed.
 */
interface ToolGroup {
  toolUseId: string;
  toolName?: string;
  call?: TaskArtifact;
  result?: TaskArtifact;
  isError?: boolean;
}

function groupArtifactsByToolUse(artifacts: ReadonlyArray<TaskArtifact>): ToolGroup[] {
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
        toolUseId: art.artifactId ?? `artifact-${Math.random()}`,
        call: kind === undefined ? art : undefined,
        result: kind === undefined ? undefined : art,
      });
    }
  }

  return [...groups.values(), ...ungrouped];
}

export function TaskDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [task, setTask] = useState<TaskDetail | null>(null);
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
      .get<TaskDetail>(
        `/api/v1/tasks/${encodeURIComponent(id)}`,
        controller.signal,
      )
      .then((data) => {
        if (cancelled) return;
        setTask(data);
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
  // tool_use_id for the Artifacts fallback card (older tasks
  // only). Recomputed on task change; cheap (O(n)).
  const toolGroups = useMemo(() => (task ? groupArtifactsByToolUse(task.artifacts) : []), [task]);

  // Reconstruct chat-shaped messages from the persisted
  // `task.history` so the History card renders with the same
  // chat-style cards, role borders, markdown, and tool-block
  // sub-blocks as the live `/chat/:sessionId` page.
  //
  // Status pill propagation matches the live chat page:
  // only assistant messages carry a status, never user
  // messages. We deliberately do NOT paint a status pill on
  // user rows — doing so would diverge from the live chat
  // page. The pill's value is the current task status
  // (e.g. `working`, `completed`) so the user can tell at a
  // glance whether the task they are looking at is still
  // running or has finished — independent of what each
  // individual agent turn was doing.
  const reconstructed = useMemo(() => (task ? reconstructMessagesFromTask(task) : []), [task]);
  const viewMessages: ChatMessageViewItem[] = useMemo(
    () =>
      reconstructed.map((m) => ({
        id: m.id,
        role: m.role,
        segments: m.segments,
        status: m.role === 'assistant' ? task?.status : undefined,
      })),
    [reconstructed, task?.status],
  );

  // Wall-clock value for the chat-style renderer. Task-detail
  // viewing is static (no streaming) so the tick is irrelevant;
  // pass `Date.now()` once per render.
  const now = Date.now();

  const historyEmpty = !!task && task.history.length === 0 && reconstructed.length === 0;

  return (
    <div className="nt-task-detail">
      <Heading as="h1" size="6">
        Task
      </Heading>
      <p>
        <Link to="/tasks" data-testid="task-detail-back">
          ← Back to Tasks
        </Link>
        {task?.context_id && (
          <>
            {' · '}
            <Link
              to={`/chat/${encodeURIComponent(task.context_id)}`}
              data-testid="task-detail-continue-chat"
              onClick={() => {
                if (task) seedChatFromTask(task.context_id, task);
              }}
            >
              在 chat 中继续此 session →
            </Link>
          </>
        )}
      </p>
      {loading && <SkeletonList count={3} testId="task-detail-skeleton" />}
      {error && (
        <EmptyState
          icon="⚠️"
          title="Failed to load task"
          description={
            <code style={{ color: 'var(--text-muted)' }}>{error}</code>
          }
          testId="task-detail-error"
        />
      )}
      {!loading && !error && !task && (
        <EmptyState
          icon="🔍"
          title="Task not found"
          description="The task may have been deleted, or the URL is malformed."
          testId="task-detail-not-found"
        />
      )}
      {task && (
        <>
          <Card title={`Task ${shortId(task.id)}`}>
            <div className="nt-task-detail__summary">
              <code>id: {task.id}</code>
            </div>
            <div className="nt-task-detail__summary">
              <code>status:</code>
              <span className={`nt-chat__message-status status-${task.status}`}>{task.status}</span>
            </div>
            <div className="nt-task-detail__summary">
              <code>context: {task.context_id}</code>
            </div>
            {task.created_at && (
              <div className="nt-task-detail__summary">
                <code>created: {task.created_at}</code>
              </div>
            )}
            {task.updated_at && (
              <div className="nt-task-detail__summary">
                <code>updated: {task.updated_at}</code>
              </div>
            )}
          </Card>
          <Card title="History">
            {historyEmpty ? (
              <p data-testid="task-history-empty">
                No history recorded for this task. The task either completed before history
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
                    className="nt-task__artifact nt-task__artifact--tool"
                    data-testid={`task-artifact-${group.toolUseId}`}
                  >
                    <div className="nt-task__artifact-header">
                      <code>{label}</code>
                      {group.isError && <span className="nt-task__artifact-error">error</span>}
                    </div>
                    {group.call && (
                      <div className="nt-task__artifact-call">
                        <div className="nt-task__artifact-section-label">请求</div>
                        <pre className="nt-task__artifact-pre">
                          {prettyJsonOrRaw(callText).formatted}
                        </pre>
                      </div>
                    )}
                    {group.result && (
                      <div
                        className={`nt-task__artifact-result${
                          group.isError ? ' nt-task__artifact-result--error' : ''
                        }`}
                      >
                        <div className="nt-task__artifact-section-label">结果</div>
                        <pre className="nt-task__artifact-pre">{resultText}</pre>
                      </div>
                    )}
                    {!group.call && !group.result && (
                      <pre className="nt-task__artifact-pre">
                        {group.toolUseId || '(empty artifact)'}
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
