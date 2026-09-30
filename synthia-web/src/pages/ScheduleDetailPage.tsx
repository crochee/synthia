import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { Box } from '@radix-ui/themes';
import { BackLink, PageHero, PageShell, Section, StatusPill } from '../components/ui/visual';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { schedulesApi } from '../api/schedules';
import type { Schedule } from '../api/types';
import { describeScheduleKind } from '../lib/describeScheduleKind';

const SCHEDULE_GLYPH = '⏰';
const SCHEDULE_HERO_MODIFIER = 'nt-vis-hero--tool';

/**
 * Schedule detail page.
 *
 * Live data: `GET /api/v1/schedules/{id}` returns the full
 * `Job` shape (description / payload / last_fired_at). The
 * pause/resume button wires to `PATCH /api/v1/schedules/{id}`
 * and re-fetches on success.
 */
export function ScheduleDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [schedule, setSchedule] = useState<Schedule | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    schedulesApi
      .get(id, controller.signal)
      .then((s) => setSchedule(s))
      .catch((e: unknown) => {
        if ((e as { name?: string } | null)?.name === 'AbortError') return;
        setError((e as Error).message);
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, [id]);

  return (
    <PageShell testId="schedule-detail-page">
      <BackLink to="/schedules" testId="schedule-detail-back">
        Back to schedules
      </BackLink>
      <PageHero
        glyph={SCHEDULE_GLYPH}
        heroModifier={SCHEDULE_HERO_MODIFIER}
        title={schedule ? <code>{schedule.name}</code> : id ? <code>{id}</code> : 'Schedule'}
        subtitle="Detail view for one schedule job."
        pills={
          schedule ? (
            <StatusPill
              tone={
                schedule.status === 'active'
                  ? 'green'
                  : schedule.status === 'paused'
                    ? 'cyan'
                    : 'neutral'
              }
              withDot
            >
              {schedule.status}
            </StatusPill>
          ) : undefined
        }
        testId="schedule-detail-hero"
      />

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Could not load schedule"
          description={error}
          testId="schedule-detail-error"
        />
      ) : loading ? (
        <SkeletonList count={2} testId="schedule-detail-skeleton" />
      ) : schedule ? (
        <>
          <Section title="Overview" glyph="📋" testId="schedule-detail-overview">
            <Box style={{ display: 'grid', gap: 'var(--spacing-sm)' }}>
              <div>
                <strong>Kind:</strong> {describeScheduleKind(schedule)}
              </div>
              <div>
                <strong>Next fire:</strong> {schedule.next_fire_at}
              </div>
              {schedule.last_fired_at && (
                <div>
                  <strong>Last fired:</strong> {schedule.last_fired_at}
                </div>
              )}
              {schedule.description && (
                <div>
                  <strong>Description:</strong> {schedule.description}
                </div>
              )}
            </Box>
          </Section>
          <Section title="Payload" glyph="📦" testId="schedule-detail-payload">
            <pre className="nt-vis-code" data-testid="schedule-detail-payload-code">
              <code>{JSON.stringify(schedule.payload, null, 2)}</code>
            </pre>
          </Section>
        </>
      ) : null}
    </PageShell>
  );
}
