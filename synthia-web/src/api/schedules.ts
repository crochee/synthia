/**
 * Schedules API client — `synthia-scheduler` domain.
 *
 * Wire shape follows `crates/synthia-scheduler/src/job.rs::Job`.
 * The list endpoint returns a tight `ScheduleSummary` row; the
 * detail endpoint flattens the full `Job` so every field is at
 * the top level of the response.
 */

import { api } from './client';
import type {
  CreateScheduleRequest,
  List,
  Schedule,
  ScheduleSummary,
  ScheduleTick,
  UpdateScheduleRequest,
} from './types';

export const schedulesApi = {
  /** `GET /api/v1/schedules` — list all scheduled jobs. */
  list: (signal?: AbortSignal): Promise<List<ScheduleSummary>> =>
    api.get<List<ScheduleSummary>>('/api/v1/schedules', signal),

  /** `GET /api/v1/schedules/{id}` — single job detail. */
  get: (id: string, signal?: AbortSignal): Promise<Schedule> =>
    api.get<Schedule>(`/api/v1/schedules/${encodeURIComponent(id)}`, signal),

  /** `POST /api/v1/schedules` — register a new schedule. */
  create: (req: CreateScheduleRequest, signal?: AbortSignal): Promise<Schedule> =>
    api.post<Schedule>('/api/v1/schedules', req, signal),

  /** `PATCH /api/v1/schedules/{id}` — pause / resume / update payload. */
  update: (id: string, req: UpdateScheduleRequest, signal?: AbortSignal): Promise<Schedule> =>
    api.patch<Schedule>(`/api/v1/schedules/${encodeURIComponent(id)}`, req, signal),

  /** `DELETE /api/v1/schedules/{id}` — remove the job and its on-disk file. */
  remove: (id: string, signal?: AbortSignal): Promise<void> =>
    api.del(`/api/v1/schedules/${encodeURIComponent(id)}`, signal),

  /** `POST /api/v1/schedules/{id}/tick` — drive a single tick and
   *  return the fired jobs (debug / "fire now" affordance). */
  tick: (id: string, signal?: AbortSignal): Promise<ScheduleTick> =>
    api.post<ScheduleTick>(`/api/v1/schedules/${encodeURIComponent(id)}/tick`, {}, signal),
};
