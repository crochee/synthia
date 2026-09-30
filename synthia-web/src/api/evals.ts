/**
 * Evals API client — `synthia-eval` domain.
 *
 * Mirrors the future server surface (see `SERVER-ROUTES-TODO.md`).
 * Wire shapes follow `crates/synthia-eval/src/lib.rs` /
 * `metrics.rs`: `EvalSuite` carries `TestCase`s; `EvalReport` is
 * the runner's structured output with per-case metric scores.
 */

import { api } from './client';
import type { CreateEvalSuiteRequest, EvalReport, EvalSuite, List } from './types';

export const evalsApi = {
  /** `GET /api/v1/evals` — list stored eval suites. */
  list: (signal?: AbortSignal): Promise<List<EvalSuite>> =>
    api.get<List<EvalSuite>>('/api/v1/evals', signal),

  /** `GET /api/v1/evals/{name}` — single suite detail (cases included). */
  get: (name: string, signal?: AbortSignal): Promise<EvalSuite> =>
    api.get<EvalSuite>(`/api/v1/evals/${encodeURIComponent(name)}`, signal),

  /** `POST /api/v1/evals` — register a suite from JSON. */
  create: (req: CreateEvalSuiteRequest, signal?: AbortSignal): Promise<EvalSuite> =>
    api.post<EvalSuite>('/api/v1/evals', req, signal),

  /** `DELETE /api/v1/evals/{name}` — drop a stored suite. */
  remove: (name: string, signal?: AbortSignal): Promise<void> =>
    api.del(`/api/v1/evals/${encodeURIComponent(name)}`, signal),

  /** `POST /api/v1/evals/{name}/run` — run the suite and return the report. */
  run: (name: string, signal?: AbortSignal): Promise<EvalReport> =>
    api.post<EvalReport>(`/api/v1/evals/${encodeURIComponent(name)}/run`, {}, signal),
};
