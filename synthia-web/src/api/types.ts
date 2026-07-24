/**
 * Shared types for the synthia-web frontend.
 *
 * Only types actually consumed by the React app live here. Backend
 * field names mirror the JSON wire format (camelCase where the
 * Rust server serializes camelCase, snake_case elsewhere).
 */

export interface Skill {
  name: string;
  description?: string;
  enabled: boolean;
}

export interface Tool {
  name: string;
  description?: string;
  status?: string;
}

/**
 * Shape returned by `GET /api/memory/search`. The backend uses
 * `score` (not `relevance`); the type is named `ScoreHit` to make
 * the unit explicit at the call site.
 */
export interface ScoreHit {
  id: string;
  content: string;
  score?: number;
}

export interface Settings {
  provider?: string;
  model?: string;
  apiKey?: string;
}

export interface TaskSummary {
  id: string;
  status: string;
  createdAt?: string;
  contextId?: string;
}

export interface Job {
  key: string;
  description: string;
  trigger_desc: string;
  paused: boolean;
}

export interface JobsListResponse {
  jobs: Job[];
  paused: string[];
  count: number;
}

export interface McpServer {
  id: string;
  name: string;
  command: string;
  args: string[];
  status?: string;
}
