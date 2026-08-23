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

export interface SessionSummary {
  id: string;
  status: string;
  context_id?: string;
  created_at?: string;
}

/**
 * Full session detail returned by `GET /api/v1/sessions/:id`.
 *
 * `history` and `artifacts` use minimal local interfaces rather
 * than a third-party SDK — the REST endpoint returns plain JSON,
 * and we only need a few fields for display. `artifacts` is kept
 * as an empty array in practice; tool calls / results now flow
 * through `history` as agent frames.
 */
export interface SessionDetail {
  id: string;
  status: string;
  context_id: string;
  created_at?: string | null;
  updated_at?: string | null;
  history: SessionTurn[];
  artifacts: SessionArtifact[];
}

/**
 * One durable event in the session transcript (one JSONL line).
 *
 * The session sink persists exactly two event families (see the
 * `event-durability-classification` spec — ephemeral system
 * events like `SessionStarted` / `SessionEnded` are broadcast
 * only and never persisted):
 *
 * - `{ "type": "UserInput", "data": { "text": string } }` —
 *   the synthetic envelope the controller writes for each
 *   user prompt.
 * - `{ "type": "Model", "data": ContentPart }` — one durable
 *   `AgentEvent::Model` frame; `ContentPart` is internally
 *   tagged on `type`: `text` | `tool_use` | `tool_result` |
 *   `resource`.
 */
export interface SessionTurn {
  type?: string;
  data?: Record<string, unknown>;
  /** RFC 3339 timestamp of the persisted event, when present. */
  ts?: string;
}

/** Legacy attachment slot — kept as an empty array in modern
 *  sessions so the detail page can iterate without
 *  special-casing undefined. */
export interface SessionArtifact {
  attachmentId?: string;
  name?: string;
  description?: string;
  parts?: ReadonlyArray<SessionPart>;
  /**
   * Server-side metadata on a legacy attachment. The modern
   * equivalent carries tool calls / results as agent turns
   * in `SessionDetail.history`; this metadata block is only
   * populated for sessions completed before history
   * persistence landed. We keep reading it for the legacy
   * fallback path.
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
 * Minimal view of one transcript segment.
 *
 * The v1 server uses field-presence serialization: exactly one of
 * `text` / `raw` / `url` / `data` is present per part. We type
 * only the `text` field because that's what the UI renders; other
 * content kinds fall through to a JSON dump.
 */
export interface SessionPart {
  text?: string;
  data?: unknown;
  raw?: string;
  url?: string;
  filename?: string;
  mediaType?: string;
}
