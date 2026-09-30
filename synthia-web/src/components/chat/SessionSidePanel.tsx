/**
 * The chat session's side panel: what the run is *doing* rather than
 * what it is *saying*.
 *
 * Three sections, each derived from the transcript (see
 * `lib/session-panel-data`):
 *
 *  - progress — the latest `TodoWrite` list, as a checklist with a
 *    progress bar;
 *  - artifacts — files the agent wrote through the `write` tool;
 *  - sub-agents — `task` delegations, each linking to the child
 *    session that holds the delegated trace.
 *
 * The panel never replaces the transcript: the same information stays
 * inline in the conversation (a todo call renders as a checklist, a
 * `task` call as a sub-agent card), and the panel is the at-a-glance
 * aggregate the user would otherwise have to scroll to reconstruct.
 */
import { Link } from 'react-router-dom';
import {
  basename,
  type PanelArtifact,
  type PanelSubagent,
  type PanelTodoState,
} from '../../lib/session-panel-data';

export interface SessionSidePanelProps {
  todos: PanelTodoState | null;
  artifacts: PanelArtifact[];
  subagents: PanelSubagent[];
  onClose: () => void;
}

const STATUS_LABEL: Record<PanelTodoItemStatus, string> = {
  pending: '待办',
  in_progress: '进行中',
  completed: '已完成',
  cancelled: '已取消',
};

type PanelTodoItemStatus = 'pending' | 'in_progress' | 'completed' | 'cancelled' | string;

function statusLabel(status: string): string {
  return STATUS_LABEL[status as PanelTodoItemStatus] ?? status;
}

const SUBAGENT_LABEL: Record<PanelSubagent['status'], string> = {
  running: '执行中…',
  succeeded: '已完成',
  failed: '失败',
  aborted: '已中止',
};

export function SessionSidePanel({
  todos,
  artifacts,
  subagents,
  onClose,
}: SessionSidePanelProps): JSX.Element {
  const percent = todos && todos.total > 0 ? Math.round((todos.completed / todos.total) * 100) : 0;
  return (
    <aside className="nt-chat-panel" aria-label="会话进度与产物" data-testid="session-side-panel">
      <div className="nt-chat-panel__header">
        <span className="nt-chat-panel__title">进度与产物</span>
        <button
          type="button"
          className="nt-chat-panel__close"
          onClick={onClose}
          aria-label="收起面板"
          data-testid="session-panel-close"
        >
          ×
        </button>
      </div>

      <section className="nt-chat-panel__section" data-testid="panel-todos">
        <h3 className="nt-chat-panel__section-title">
          任务进度
          {todos && todos.total > 0 ? (
            <span className="nt-chat-panel__count">
              {todos.completed}/{todos.total}
            </span>
          ) : null}
        </h3>
        {todos && todos.total > 0 ? (
          <>
            <div
              className="nt-chat-panel__progress"
              role="progressbar"
              aria-valuenow={percent}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div className="nt-chat-panel__progress-fill" style={{ width: `${percent}%` }} />
            </div>
            <ol className="nt-chat-panel__todos">
              {todos.items.map((item) => (
                <li
                  key={item.id}
                  className={`nt-chat-panel__todo nt-chat-panel__todo--${item.status}`}
                >
                  <span className="nt-chat-panel__todo-mark" aria-hidden>
                    {item.status === 'completed'
                      ? '✓'
                      : item.status === 'in_progress'
                        ? '◐'
                        : item.status === 'cancelled'
                          ? '–'
                          : '○'}
                  </span>
                  <span className="nt-chat-panel__todo-text">{item.content}</span>
                  <span className="nt-chat-panel__todo-status">{statusLabel(item.status)}</span>
                </li>
              ))}
            </ol>
          </>
        ) : (
          <p className="nt-chat-panel__empty">本轮还没有任务清单。</p>
        )}
      </section>

      <section className="nt-chat-panel__section" data-testid="panel-subagents">
        <h3 className="nt-chat-panel__section-title">
          子代理
          {subagents.length > 0 ? (
            <span className="nt-chat-panel__count">{subagents.length}</span>
          ) : null}
        </h3>
        {subagents.length > 0 ? (
          <ul className="nt-chat-panel__subagents">
            {subagents.map((sub) => (
              <li key={sub.id} className="nt-chat-panel__subagent">
                <div className="nt-chat-panel__subagent-head">
                  <span className="nt-chat-panel__subagent-agent">{sub.agent}</span>
                  <span
                    className={`nt-chat-panel__subagent-status nt-chat-panel__subagent-status--${sub.status}`}
                  >
                    {SUBAGENT_LABEL[sub.status]}
                  </span>
                </div>
                {sub.prompt ? <p className="nt-chat-panel__subagent-prompt">{sub.prompt}</p> : null}
                {sub.childSessionId ? (
                  <Link
                    className="nt-chat-panel__subagent-link"
                    to={`/sessions/${sub.childSessionId}`}
                    data-testid={`subagent-link-${sub.childSessionId}`}
                  >
                    查看子会话 →
                  </Link>
                ) : null}
              </li>
            ))}
          </ul>
        ) : (
          <p className="nt-chat-panel__empty">还没有委派子代理。</p>
        )}
      </section>

      <section className="nt-chat-panel__section" data-testid="panel-artifacts">
        <h3 className="nt-chat-panel__section-title">
          产物
          {artifacts.length > 0 ? (
            <span className="nt-chat-panel__count">{artifacts.length}</span>
          ) : null}
        </h3>
        {artifacts.length > 0 ? (
          <ul className="nt-chat-panel__artifacts">
            {artifacts.map((artifact) => (
              <li
                key={artifact.id}
                className={`nt-chat-panel__artifact${artifact.error ? ' nt-chat-panel__artifact--error' : ''}`}
                title={artifact.path}
              >
                <span className="nt-chat-panel__artifact-icon" aria-hidden>
                  📄
                </span>
                <span className="nt-chat-panel__artifact-name">{basename(artifact.path)}</span>
                <span className="nt-chat-panel__artifact-state">
                  {artifact.pending ? '写入中…' : artifact.error ? '失败' : '已写入'}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="nt-chat-panel__empty">还没有写出的文件。</p>
        )}
      </section>
    </aside>
  );
}
