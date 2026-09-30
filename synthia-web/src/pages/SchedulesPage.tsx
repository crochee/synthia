import { useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { PageHero, PageShell, StatusPill, CardGrid } from '../components/ui/visual';
import { schedulesApi } from '../api/schedules';
import type { CreateScheduleRequest, ScheduleSummary } from '../api/types';
import { useToast } from '../hooks/useToast';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';

const SCHEDULE_GLYPH = '⏰';
const SCHEDULE_HERO_MODIFIER = 'nt-vis-hero--tool';
const SCHEDULE_CARD_MODIFIER = 'nt-vis-card--tool';

/**
 * Schedules list page.
 *
 * Live data: `GET /api/v1/schedules` returns `ScheduleSummary[]`
 * (id / name / kind / status / next_fire_at). Clicking a card
 * navigates to `/schedules/:id` where the detail page fetches
 * the full `Job` shape (description / payload / last_fired_at).
 *
 * The "+ New Schedule" modal posts to `/api/v1/schedules` and
 * refreshes the list on success. Pause/resume wires to
 * `PATCH /api/v1/schedules/{id}`.
 */
export function SchedulesPage() {
  const {
    items: schedules,
    loading,
    error,
    hasMore,
    loadMore,
    refresh,
  } = useCursorList<ScheduleSummary>('/api/v1/schedules');

  const { filtered, query, setQuery, sortDir, setSortDir, isFiltering } = useListFilter(schedules, {
    match: (s, q) => s.name.toLowerCase().includes(q),
    compare: (a, b) => a.name.localeCompare(b.name),
  });

  const [createOpen, setCreateOpen] = useState(false);
  const [createForm, setCreateForm] = useState<{
    name: string;
    description: string;
    kind: 'once' | 'interval' | 'cron';
    interval_secs: string;
    cron_expr: string;
    payload: string;
  }>({
    name: '',
    description: '',
    kind: 'once',
    interval_secs: '60',
    cron_expr: '*/5 * * * *',
    payload: '{}',
  });
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const toast = useToast();

  const submitCreate = async (): Promise<void> => {
    let payload: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(createForm.payload || '{}');
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('payload must be a JSON object');
      }
      payload = parsed as Record<string, unknown>;
    } catch (e) {
      setCreateError(`Invalid payload JSON: ${(e as Error).message}`);
      return;
    }
    let body: CreateScheduleRequest;
    if (createForm.kind === 'once') {
      body = {
        name: createForm.name,
        description: createForm.description,
        kind: { kind: 'once' },
        payload,
      };
    } else if (createForm.kind === 'interval') {
      const secs = Number.parseInt(createForm.interval_secs, 10);
      if (Number.isNaN(secs) || secs <= 0) {
        setCreateError('interval_secs must be a positive integer');
        return;
      }
      body = {
        name: createForm.name,
        description: createForm.description,
        kind: { kind: 'interval', interval_secs: secs },
        payload,
      };
    } else {
      body = {
        name: createForm.name,
        description: createForm.description,
        kind: { kind: 'cron', expr: createForm.cron_expr },
        payload,
      };
    }
    setCreating(true);
    setCreateError(null);
    try {
      await schedulesApi.create(body);
      toast.push({ variant: 'success', message: `Registered schedule "${body.name}".` });
      setCreateOpen(false);
      setCreateForm({
        name: '',
        description: '',
        kind: 'once',
        interval_secs: '60',
        cron_expr: '*/5 * * * *',
        payload: '{}',
      });
      await refresh();
    } catch (e) {
      setCreateError((e as Error).message);
    } finally {
      setCreating(false);
    }
  };

  const heroPills = (
    <>
      <StatusPill tone="cyan">{`${schedules.length} jobs`}</StatusPill>
      {isFiltering && (
        <StatusPill tone="yellow" withDot>
          filtering · {filtered.length}/{schedules.length}
        </StatusPill>
      )}
    </>
  );

  return (
    <PageShell testId="schedules-page">
      <PageHero
        glyph={SCHEDULE_GLYPH}
        heroModifier={SCHEDULE_HERO_MODIFIER}
        title="Schedules"
        subtitle="Recurring jobs the synthia-scheduler owns. Each row persists to <workspace>/schedules/<id>.json under PID-locked atomic writes; the scheduler ticks when its host asks."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Source</span>
              <span>synthia-scheduler · ScheduleStore</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>API</span>
              <span>/api/v1/schedules</span>
            </div>
          </div>
        }
        testId="schedules-hero"
      />

      <ListToolbar
        query={query}
        onQueryChange={setQuery}
        sortDir={sortDir}
        onSortDirChange={setSortDir}
        searchLabel="Schedules"
        testId="schedules-toolbar"
      >
        <Button
          variant="soft"
          onClick={() => void refresh()}
          disabled={loading}
          data-testid="schedules-refresh"
        >
          {loading ? 'Refreshing...' : 'Refresh'}
        </Button>
        <Button variant="solid" onClick={() => setCreateOpen(true)} data-testid="schedules-new">
          + New Schedule
        </Button>
      </ListToolbar>

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load schedules"
          description={error}
          testId="schedules-error"
        />
      ) : loading && schedules.length === 0 ? (
        <SkeletonList count={4} testId="schedules-skeleton" />
      ) : filtered.length === 0 && !error ? (
        <EmptyState
          icon={isFiltering ? '🔍' : '⏰'}
          title={isFiltering ? 'No schedules match your search' : 'No schedules yet'}
          description={
            isFiltering
              ? `No schedules matched "${query}". Try clearing the search.`
              : 'Register a recurring job to surface it here. The cron and interval kinds are accepted; one-shot fires on next tick.'
          }
          testId="schedules-empty"
          action={
            !isFiltering ? (
              <Button
                variant="solid"
                onClick={() => setCreateOpen(true)}
                data-testid="schedules-register-empty"
              >
                + New Schedule
              </Button>
            ) : undefined
          }
        />
      ) : (
        <CardGrid testId="schedules-grid">
          {filtered.map((s) => (
            <ScheduleSummaryCard key={s.id} schedule={s} />
          ))}
        </CardGrid>
      )}

      {hasMore && (
        <div style={{ display: 'flex', justifyContent: 'center' }}>
          <Button
            variant="soft"
            onClick={() => void loadMore()}
            disabled={loading}
            data-testid="schedules-load-more"
          >
            {loading ? 'Loading...' : 'Load More Schedules'}
          </Button>
        </div>
      )}

      {createOpen && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Create schedule"
          data-testid="schedules-create-modal"
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <form
            className="nt-form"
            onSubmit={(e: FormEvent) => {
              e.preventDefault();
              void submitCreate();
            }}
            data-testid="schedules-create-form"
            style={{
              background: 'var(--bg-primary)',
              padding: 'var(--spacing-lg)',
              borderRadius: 'var(--radius-md)',
              minWidth: 480,
              maxWidth: 720,
            }}
          >
            <h2 style={{ marginTop: 0 }}>New schedule</h2>
            <label className="nt-form__label">
              <span>Name</span>
              <input
                type="text"
                value={createForm.name}
                onChange={(e) => setCreateForm((f) => ({ ...f, name: e.target.value }))}
                required
                data-testid="schedules-create-name"
              />
            </label>
            <label className="nt-form__label">
              <span>Description</span>
              <input
                type="text"
                value={createForm.description}
                onChange={(e) =>
                  setCreateForm((f) => ({
                    ...f,
                    description: e.target.value,
                  }))
                }
                data-testid="schedules-create-description"
              />
            </label>
            <label className="nt-form__label">
              <span>Kind</span>
              <select
                value={createForm.kind}
                onChange={(e) =>
                  setCreateForm((f) => ({
                    ...f,
                    kind: e.target.value as 'once' | 'interval' | 'cron',
                  }))
                }
                data-testid="schedules-create-kind"
              >
                <option value="once">once</option>
                <option value="interval">interval</option>
                <option value="cron">cron</option>
              </select>
            </label>
            {createForm.kind === 'interval' && (
              <label className="nt-form__label">
                <span>Interval (seconds)</span>
                <input
                  type="number"
                  min={1}
                  value={createForm.interval_secs}
                  onChange={(e) =>
                    setCreateForm((f) => ({
                      ...f,
                      interval_secs: e.target.value,
                    }))
                  }
                  data-testid="schedules-create-interval-secs"
                />
              </label>
            )}
            {createForm.kind === 'cron' && (
              <label className="nt-form__label">
                <span>Cron expression</span>
                <input
                  type="text"
                  value={createForm.cron_expr}
                  onChange={(e) =>
                    setCreateForm((f) => ({
                      ...f,
                      cron_expr: e.target.value,
                    }))
                  }
                  placeholder="*/5 * * * *"
                  data-testid="schedules-create-cron-expr"
                />
              </label>
            )}
            <label className="nt-form__label">
              <span>Payload (JSON object)</span>
              <textarea
                rows={6}
                value={createForm.payload}
                onChange={(e) => setCreateForm((f) => ({ ...f, payload: e.target.value }))}
                spellCheck={false}
                data-testid="schedules-create-payload"
              />
            </label>
            {createError && (
              <p className="nt-form__error" role="alert" data-testid="schedules-create-error">
                <code>{createError}</code>
              </p>
            )}
            <Flex gap="2" justify="end" mt="3">
              <Button
                variant="soft"
                type="button"
                onClick={() => setCreateOpen(false)}
                data-testid="schedules-create-cancel"
              >
                Cancel
              </Button>
              <Button
                variant="solid"
                type="submit"
                disabled={creating || !createForm.name.trim()}
                loading={creating}
                data-testid="schedules-create-submit"
              >
                {creating ? 'Creating...' : 'Create'}
              </Button>
            </Flex>
          </form>
        </div>
      )}
    </PageShell>
  );
}

interface ScheduleCardProps {
  schedule: ScheduleSummary;
}

function ScheduleSummaryCard({ schedule }: ScheduleCardProps) {
  const href = `/schedules/${encodeURIComponent(schedule.id)}`;
  const tone =
    schedule.status === 'active' ? 'green' : schedule.status === 'paused' ? 'cyan' : 'neutral';
  const kindLabel =
    schedule.kind.kind === 'cron'
      ? `cron · ${schedule.kind.expr}`
      : schedule.kind.kind === 'interval'
        ? `interval · ${schedule.kind.interval_secs}s`
        : 'once';
  return (
    <Link
      to={href}
      className={`nt-vis-card ${SCHEDULE_CARD_MODIFIER}`}
      data-testid={`schedule-card-${schedule.id}`}
    >
      <Flex justify="between" align="start" mb="2">
        <Box>
          <h3 className="nt-vis-card__name" title={schedule.name}>
            <code>{schedule.name}</code>
          </h3>
          <span className="nt-vis-card__subtitle">{kindLabel}</span>
        </Box>
        <StatusPill tone={tone} withDot>
          {schedule.status}
        </StatusPill>
      </Flex>
      <p className="nt-vis-card__desc" style={{ color: 'var(--text-muted)', margin: '4px 0 0' }}>
        next · {schedule.next_fire_at}
      </p>
      <div className="nt-vis-card__footer">
        <div className="nt-vis-card__pills">
          <StatusPill tone="purple">schedule</StatusPill>
        </div>
      </div>
    </Link>
  );
}
