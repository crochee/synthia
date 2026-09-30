/**
 * Workflows API client — `synthia-workflow` domain.
 *
 * Document shape follows `crates/synthia-workflow/src/spec.rs::WorkflowSpec`,
 * with `Step` internally tagged on `kind` (`agent` / `fan_out` /
 * `best_of` / `pipeline` / `mcts`).
 *
 * Run-result shape follows `crates/synthia-workflow/src/result.rs::WorkflowRun`.
 * The runtime is synchronous — one `POST /api/v1/workflows/{id}/run`
 * returns the whole `WorkflowRun` JSON. There is no SSE stream.
 */

import { api } from './client';
import type { CreateWorkflowRequest, List, WorkflowRunResult, WorkflowSpec } from './types';

/** Minimal row returned by `GET /api/v1/workflows`. */
export interface WorkflowSummary {
  id: string;
  step_count: number;
  phase_count: number;
}

export const workflowsApi = {
  /** `GET /api/v1/workflows` — list stored workflow documents. */
  list: (signal?: AbortSignal): Promise<List<WorkflowSummary>> =>
    api.get<List<WorkflowSummary>>('/api/v1/workflows', signal),

  /** `GET /api/v1/workflows/{id}` — single document detail. */
  get: (id: string, signal?: AbortSignal): Promise<WorkflowSpec> =>
    api.get<WorkflowSpec>(`/api/v1/workflows/${encodeURIComponent(id)}`, signal),

  /** `POST /api/v1/workflows` — register a workflow document. */
  create: (req: CreateWorkflowRequest, signal?: AbortSignal): Promise<WorkflowSpec> =>
    api.post<WorkflowSpec>('/api/v1/workflows', req, signal),

  /** `PUT /api/v1/workflows/{id}` — replace a stored document. */
  replace: (id: string, spec: WorkflowSpec, signal?: AbortSignal): Promise<WorkflowSpec> =>
    api.put<WorkflowSpec>(`/api/v1/workflows/${encodeURIComponent(id)}`, spec, signal),

  /** `DELETE /api/v1/workflows/{id}` — drop a stored document. */
  remove: (id: string, signal?: AbortSignal): Promise<void> =>
    api.del(`/api/v1/workflows/${encodeURIComponent(id)}`, signal),

  /**
   * `POST /api/v1/workflows/{id}/run` — execute the document and
   * return the final `WorkflowRun`. The runtime is synchronous;
   * long-running runs are bounded by the host's call caps. Today
   * the backend returns a stubbed result (every call settles as
   * `superseded`) until a real `WorkflowHost` lands.
   */
  run: (id: string, signal?: AbortSignal): Promise<WorkflowRunResult> =>
    api.post<WorkflowRunResult>(`/api/v1/workflows/${encodeURIComponent(id)}/run`, {}, signal),
};
