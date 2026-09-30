import { Link } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { StatusPill } from '../ui/visual';
import { describeScheduleKind } from '../../lib/describeScheduleKind';
import type { Schedule } from '../../api/types';

const SCHEDULE_CARD_MODIFIER = 'nt-vis-card--tool';

export interface ScheduleCardProps {
  schedule: Schedule;
}

export function ScheduleCard({ schedule }: ScheduleCardProps) {
  const href = `/schedules/${encodeURIComponent(schedule.id)}`;
  const tone =
    schedule.status === 'active' ? 'green' : schedule.status === 'paused' ? 'cyan' : 'neutral';
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
          <span className="nt-vis-card__subtitle">{describeScheduleKind(schedule)}</span>
        </Box>
        <StatusPill tone={tone} withDot>
          {schedule.status}
        </StatusPill>
      </Flex>
      {schedule.description ? (
        <p className="nt-vis-card__desc">{schedule.description}</p>
      ) : (
        <p className="nt-vis-card__desc" style={{ color: 'var(--text-muted)' }}>
          No description provided.
        </p>
      )}
      <div className="nt-vis-card__footer">
        <div className="nt-vis-card__pills">
          <StatusPill tone="purple">schedule</StatusPill>
        </div>
        <div className="nt-vis-card__meta">next · {schedule.next_fire_at}</div>
      </div>
    </Link>
  );
}
