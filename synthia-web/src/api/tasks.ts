/**
 * Tasks API client — sub-agent delegations.
 *
 * Backend wires a join endpoint (`GET /api/v1/tasks`) that
 * scans recent sessions' `Agent` envelopes server-side and
 * flattens them into one list. `listAll(signal)` is a thin
 * wrapper that returns the same shape — used by the Tasks
 * page and the `/tools` summary panel.
 */

import { api } from './client';
import type { List, TaskDelegation } from './types';

export const tasksApi = {
  /** GET /api/v1/tasks — server-side join across recent sessions. */
  listAll: (signal?: AbortSignal): Promise<List<TaskDelegation>> =>
    api.get<List<TaskDelegation>>('/api/v1/tasks', signal),

  /** GET /api/v1/tasks/{id} — fetch one delegation by its
   *  synthetic `<sessionId>:<eventSeq>` id. */
  get: (id: string, signal?: AbortSignal): Promise<TaskDelegation> =>
    api.get<TaskDelegation>(`/api/v1/tasks/${encodeURIComponent(id)}`, signal),
};
