import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { PageHero, PageShell, StatusPill, CardGrid } from '../components/ui/visual';
import { tasksApi } from '../api/tasks';
import type { TaskDelegation } from '../api/types';

const TASK_GLYPH = '🧭';
const TASK_HERO_MODIFIER = 'nt-vis-hero--tool';
const TASK_CARD_MODIFIER = 'nt-vis-card--tool';

/**
 * Tasks list page — sub-agent delegation history.
 *
 * Unlike the other three new pages, this one DOES fetch on
 * mount: the `task` tool's history is reconstructed from
 * session events (which the backend already exposes at
 * `/api/v1/sessions/:id`), so the page works today even
 * without a dedicated `/api/v1/tasks` endpoint. See
 * `src/api/tasks.ts::listAll` for the N+1 fan-out cost
 * (bounded by `?limit=50` on the outer session query).
 *
 * Once a dedicated `/api/v1/tasks` endpoint ships, this page
 * switches to a single list call without UI changes.
 */
export function TasksPage() {
  const [delegations, setDelegations] = useState<TaskDelegation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    tasksApi
      .listAll(controller.signal)
      .then((result) => setDelegations(result.data))
      .catch((e: unknown) => {
        if ((e as { name?: string } | null)?.name === 'AbortError') return;
        setError((e as Error).message);
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, []);

  const heroPills = (
    <>
      <StatusPill tone="cyan">{`${delegations.length} delegations`}</StatusPill>
    </>
  );

  return (
    <PageShell testId="tasks-page">
      <PageHero
        glyph={TASK_GLYPH}
        heroModifier={TASK_HERO_MODIFIER}
        title="Tasks"
        subtitle="Sub-agent delegation history. The task tool (`synthia-tool-task`) spawns child agents during a chat run; the resulting { type: 'Agent' } envelopes persist as session events and are reconstructed here. Once /api/v1/tasks ships, the N+1 fan-out in tasks.listAll is replaced by a single list call."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Source</span>
              <span>synthia-tool-task · session Agent envelopes</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>Depth</span>
              <span>root → grandchild (recursion)</span>
            </div>
          </div>
        }
        testId="tasks-hero"
      />

      <ListToolbar
        query=""
        onQueryChange={() => {
          /* client-side filter placeholder; listAll is fetch-once */
        }}
        sortDir="asc"
        onSortDirChange={() => {
          /* sort placeholder */
        }}
        searchLabel="Tasks"
        testId="tasks-toolbar"
      />

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load delegations"
          description={error}
          testId="tasks-error"
        />
      ) : loading && delegations.length === 0 ? (
        <SkeletonList count={4} testId="tasks-skeleton" />
      ) : delegations.length === 0 ? (
        <EmptyState
          icon="🧭"
          title="No sub-agent delegations yet"
          description={
            <>
              Run a chat session that invokes the <code>task</code> tool to see its sub-agent
              envelopes here. The reconstruction walks the 50 most recent sessions and extracts
              every <code>{`{ type: 'Agent' }`}</code> event.
            </>
          }
          testId="tasks-empty"
        />
      ) : (
        <CardGrid testId="tasks-grid">
          {delegations.map((task) => (
            <TaskCard key={task.id} task={task} />
          ))}
        </CardGrid>
      )}
    </PageShell>
  );
}

interface TaskCardProps {
  task: TaskDelegation;
}

function TaskCard({ task }: TaskCardProps) {
  const href = `/tasks/${encodeURIComponent(task.id)}`;
  const tone =
    task.status === 'succeeded'
      ? 'green'
      : task.status === 'failed' || task.status === 'aborted'
        ? 'red'
        : 'yellow';
  return (
    <Link
      to={href}
      className={`nt-vis-card ${TASK_CARD_MODIFIER}`}
      data-testid={`task-card-${task.id}`}
    >
      <Flex justify="between" align="start" mb="2">
        <Box>
          <h3 className="nt-vis-card__name" title={task.agent}>
            <code>{task.agent}</code>
          </h3>
          <span className="nt-vis-card__subtitle">depth · {task.parent_depth}</span>
        </Box>
        <StatusPill tone={tone} withDot>
          {task.status}
        </StatusPill>
      </Flex>
      {task.prompt ? (
        <p className="nt-vis-card__desc">
          {task.prompt.length > 140 ? `${task.prompt.slice(0, 140)}…` : task.prompt}
        </p>
      ) : (
        <p className="nt-vis-card__desc" style={{ color: 'var(--text-muted)' }}>
          No prompt recorded.
        </p>
      )}
      <div className="nt-vis-card__footer">
        <div className="nt-vis-card__pills">
          <StatusPill tone="cyan">delegation</StatusPill>
        </div>
        <div className="nt-vis-card__meta">session · {task.parent_session_id.slice(0, 8)}</div>
      </div>
    </Link>
  );
}
