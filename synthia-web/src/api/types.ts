/**
 * Shared types for the synthia-web frontend.
 *
 * Mirrors the v1 Management API wire format served by
 * `synthia-server` at `/api/v1/*`. Field names match the JSON
 * serialization (camelCase where the Rust server serializes
 * camelCase via `#[serde(rename_all = "camelCase")]`).
 *
 * The v1 API returns resources directly as top-level JSON (no
 * `{ status, data }` envelope). List endpoints return the
 * generic `List<T>` shape; detail endpoints return the resource
 * type itself.
 */

/**
 * Generic list response envelope used by every v1 list endpoint.
 *
 * `next_cursor` is `null`/absent when there are no more pages.
 * `total` is `null`/absent when the server cannot cheaply compute
 * a count (e.g. large datasets); clients must not assume it is
 * always present.
 */
export interface List<T> {
  data: T[];
  next_cursor?: string | null;
  total?: number | null;
}

export interface Skill {
  name: string;
  description?: string;
}

/**
 * Full skill detail returned by `GET /api/v1/skills/:name`.
 *
 * `frontmatter` is the parsed YAML/JSON frontmatter of SKILL.md
 * exposed as a flat `key → value` map (including the nested
 * `metadata` block flattened one level deep). `body` is the
 * raw markdown body (everything after the frontmatter closer)
 * that the detail page renders as markdown.
 */
export interface SkillDetail {
  name: string;
  description: string;
  path: string;
  frontmatter: Record<string, unknown>;
  body: string;
}

export interface Tool {
  name: string;
  description?: string;
}

/**
 * Detail payload returned by `GET /api/v1/tools/{name}`.
 *
 * `provenance` is `"core"` for tools compiled into the binary
 * and `"dynamic"` for tools registered at runtime. `input_schema`
 * is the JSON Schema the LLM uses for tool_choice.
 */
export interface ToolDetail {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  provenance: 'core' | 'dynamic';
}

/**
 * Payload returned by `GET /api/v1/agents` and
 * `GET /api/v1/agents/{name}`. The same shape is used for
 * list rows and detail rows; `protected: true` means the
 * name is in the server-side built-in whitelist and cannot
 * be deleted or re-registered.
 */
export interface AgentDetail {
  name: string;
  description: string;
  kind: string;
  version: string;
  instructions: string;
  capabilities: string[];
  tools: string[];
  handoffs: string[];
  modelHint?: string;
  owner?: string;
  domain?: string;
  persona?: string;
  protected: boolean;
}

/** Body sent to `POST /api/v1/agents`. Mirrors the backend
 *  `AgentDescriptorRequest` struct. */
export interface AgentDescriptorRequest {
  name: string;
  description: string;
  kind: string;
  version?: string;
  instructions?: string;
  capabilities?: string[];
  tools?: string[];
  handoffs?: string[];
  modelHint?: string;
  owner?: string;
  domain?: string;
  persona?: string;
}

/**
 * Shape returned by `GET /api/v1/memory/search`. The backend uses
 * `score` (not `relevance`); the type is named `ScoreHit` to make
 * the unit explicit at the call site.
 */
export interface ScoreHit {
  id: string;
  content: string;
  score?: number;
}

export interface TaskSummary {
  id: string;
  status: string;
  context_id?: string;
  created_at?: string;
}

/**
 * Full task detail returned by `GET /api/v1/tasks/:id`.
 *
 * `history` and `artifacts` use minimal local interfaces rather
 * than the @a2a-js/sdk `Message`/`Artifact` classes — the REST
 * endpoint returns plain JSON, not SDK class instances, and we
 * only need a few fields for display.
 */
export interface TaskDetail {
  id: string;
  status: string;
  context_id: string;
  created_at?: string | null;
  updated_at?: string | null;
  history: TaskMessage[];
  artifacts: TaskArtifact[];
}

/** Minimal view of an A2A `Message` as serialized by the v1 API. */
export interface TaskMessage {
  messageId?: string;
  role?: string;
  parts?: ReadonlyArray<TaskPart>;
  contextId?: string;
  taskId?: string;
  /**
   * Wire-level `Message.metadata` (synthia extension). When
   * present, `metadata["a2a_conversion"]` carries the lossless
   * `AgentEvent → A2A` conversion entry that the backend
   * attached at stream time. The task-detail page threads this
   * through to the segment renderer so the reconstructed
   * transcript shows the same conversion panel as the live
   * chat stream.
   */
  metadata?: Record<string, unknown>;
}

/** Minimal view of an A2A `Artifact` as serialized by the v1 API. */
export interface TaskArtifact {
  artifactId?: string;
  name?: string;
  description?: string;
  parts?: ReadonlyArray<TaskPart>;
  /**
   * Server-side metadata on a legacy artifact. New tasks
   * carry tool calls / results in `Task.history` as
   * `Message(agent) + Part::data` (A2A v1.0 §3.7 —
   * communication turns, not artifacts), so this metadata
   * block is only populated for tasks completed before
   * `Task.history` was wired up. The MVP used a
   * `kind: "tool_call" | "tool_result"` discriminator here;
   * we keep reading it for the legacy fallback path.
   */
  metadata?: {
    kind?: string;
    tool_name?: string;
    tool_use_id?: string;
    is_error?: boolean;
    [key: string]: unknown;
  };
}

/**
 * Minimal view of an A2A `Part` as serialized by the v1 API.
 *
 * The v1 server uses field-presence serialization: exactly one of
 * `text` / `raw` / `url` / `data` is present per part. We type
 * only the `text` field because that's what the UI renders; other
 * content kinds fall through to a JSON dump.
 */
export interface TaskPart {
  text?: string;
  data?: unknown;
  raw?: string;
  url?: string;
  filename?: string;
  mediaType?: string;
}
