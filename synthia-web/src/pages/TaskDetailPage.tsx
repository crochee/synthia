import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Box } from '@radix-ui/themes';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { BackLink, PageHero, PageShell, Section, StatusPill } from '../components/ui/visual';
import { tasksApi } from '../api/tasks';
import type { TaskDelegation } from '../api/types';

const TASK_GLYPH = '🧭';
const TASK_HERO_MODIFIER = 'nt-vis-hero--tool';

/**
 * Task detail page.
 *
 * Live data: `GET /api/v1/tasks/{id}` returns one
 * `TaskDelegation`, where `id` is the synthetic
 * `<sessionId>:<eventSeq>` the list page derives. The parent
 * session is linked so the user can jump to the transcript the
 * delegation came from.
 */
export function TaskDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [task, setTask] = useState<TaskDelegation | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    tasksApi
      .get(id, controller.signal)
      .then((t) => setTask(t))
      .catch((e: unknown) => {
        if ((e as { name?: string } | null)?.name === 'AbortError') return;
        setError((e as Error).message);
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, [id]);

  return (
    <PageShell testId="task-detail-page">
      <BackLink to="/tasks" testId="task-detail-back">
        Back to tasks
      </BackLink>
      <PageHero
        glyph={TASK_GLYPH}
        heroModifier={TASK_HERO_MODIFIER}
        title={task ? <code>{task.agent}</code> : id ? <code>{id}</code> : 'Task'}
        subtitle="Detail view for one sub-agent delegation."
        pills={
          task ? (
            <StatusPill
              tone={
                task.status === 'succeeded'
                  ? 'green'
                  : task.status === 'failed' || task.status === 'aborted'
                    ? 'red'
                    : 'yellow'
              }
              withDot
            >
              {task.status}
            </StatusPill>
          ) : undefined
        }
        testId="task-detail-hero"
      />

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Could not load delegation"
          description={error}
          testId="task-detail-error"
        />
      ) : loading ? (
        <SkeletonList count={2} testId="task-detail-skeleton" />
      ) : task ? (
        <>
          <Section title="Overview" glyph="📋" testId="task-detail-overview">
            <Box style={{ display: 'grid', gap: 'var(--spacing-sm)' }}>
              <div>
                <strong>Agent:</strong> <code>{task.agent}</code>
              </div>
              <div>
                <strong>Depth:</strong> {task.parent_depth}
              </div>
              <div>
                <strong>Parent session:</strong>{' '}
                <Link to={`/sessions/${encodeURIComponent(task.parent_session_id)}`}>
                  <code>{task.parent_session_id}</code>
                </Link>
              </div>
              {task.child_session_id && (
                <div>
                  <strong>Child trace:</strong> <code>{task.child_session_id}</code>
                </div>
              )}
              {task.started_at && (
                <div>
                  <strong>Started:</strong> {task.started_at}
                </div>
              )}
              {task.finished_at && (
                <div>
                  <strong>Finished:</strong> {task.finished_at}
                </div>
              )}
            </Box>
          </Section>
          {task.prompt && (
            <Section title="Prompt" glyph="📨" testId="task-detail-prompt">
              <pre className="nt-vis-code" data-testid="task-detail-prompt-code">
                <code>{task.prompt}</code>
              </pre>
            </Section>
          )}
        </>
      ) : null}
    </PageShell>
  );
}
