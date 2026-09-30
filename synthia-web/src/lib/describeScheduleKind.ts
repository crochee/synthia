import type { Schedule } from '../api/types';

/**
 * Format a schedule kind for the card subtitle. Maps the
 * internally-tagged `JobKind` to a single-line description
 * such as `interval · 60s`, `cron · /5`, or `once`.
 *
 * Kept in `lib/` (not co-located with the card component)
 * so `ScheduleCard.tsx` only exports components and HMR
 * (`react-refresh/only-export-components`) keeps working.
 */
export function describeScheduleKind(schedule: Schedule): string {
  switch (schedule.kind.kind) {
    case 'interval':
      return `interval · ${schedule.kind.interval_secs}s`;
    case 'cron':
      return `cron · ${schedule.kind.expr}`;
    case 'once':
      return 'once';
  }
}
