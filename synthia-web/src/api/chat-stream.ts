/**
 * REST + SSE chat client for the synthia-web frontend.
 *
 * Wraps the v1 `/api/v1/chat/*` REST surface and a
 * `text/event-stream` SSE pipeline. The stream-event vocabulary
 * (`SessionStreamEvent`, `AttachmentPart`, `sendMessageStream`)
 * is session-centric; the existing reducer / renderer code in
 * `ChatPage.tsx` consumes it directly.
 *
 * Each `AgentEvent`-shaped SSE frame from the server is adapted
 * into a `SessionStreamEvent` by the `adaptAgentEvent` mapper
 * below. The vocabulary is unified across the chat surface
 * type names so the reducer code reads as one coherent domain
 * language.
 */

import { api } from './client';

// ---------------------------------------------------------------------------
// Stream-event vocabulary
//
// `type` tags: `message` | `sessionStatus` | `turnStatus` |
// `attachment` | `error`.
// ---------------------------------------------------------------------------

export type SegmentType =
  | 'text'
  | 'thinking'
  | 'tool_call'
  | 'tool_result'
  | 'tool_block'
  | 'progress'
  | 'attachment';

export interface SegmentMetadata {
  tool_use_id?: string;
  tool_name?: string;
  text?: string;
  input?: unknown;
  is_error?: boolean;
}

export interface PartWithMetadata {
  type: SegmentType | null;
  text: string;
  metadata?: SegmentMetadata;
}

export interface SessionStreamEvent {
  type: 'sessionStatus' | 'message' | 'turnStatus' | 'attachment' | 'error';
  session?: WireSession;
  message?: WireMessage;
  turnStatus?: {
    sessionId: string;
    contextId: string;
    status: { state: string; message?: WireMessage };
  };
  attachment?: {
    sessionId: string;
    contextId: string;
    attachment: {
      attachmentId: string;
      name?: string;
      parts: WirePart[];
      metadata?: Record<string, unknown>;
    };
    append: boolean;
    lastChunk: boolean;
  };
  error?: { code: number; message: string };
}

interface WirePart {
  text?: string;
  data?: unknown;
  raw?: string;
  url?: string;
  filename?: string;
  mediaType?: string;
  metadata?: Record<string, unknown>;
  // Top-level fields for the synthia-provider `ContentPart`
  // shape — tool use / tool result parts serialise with all
  // payload fields at the root plus a `type` discriminator
  // (e.g. `{type: "tool_use", id, name, input}`).
  type?: string;
  id?: string;
  name?: string;
  input?: unknown;
  tool_use_id?: string;
  content?: unknown;
  is_error?: boolean;
}

interface WireMessage {
  messageId?: string;
  role?: string;
  parts?: WirePart[];
  contextId?: string;
  sessionId?: string;
  metadata?: Record<string, unknown>;
}

interface WireSession {
  id: string;
  contextId: string;
  status?: {
    state: string;
    message?: WireMessage;
    timestamp?: string;
  };
}

// ---------------------------------------------------------------------------
// Attachment shape — preserved from `chat-stream.ts` so the
// composer code keeps compiling without changes.
// ---------------------------------------------------------------------------

export type AttachmentPart =
  | { kind: 'text'; text: string }
  | { kind: 'image'; url?: string; dataUrl?: string; mimeType: string; filename?: string }
  | { kind: 'audio'; dataUrl: string; mimeType: string; filename?: string }
  | { kind: 'file'; dataUrl: string; mimeType: string; filename?: string };

// ---------------------------------------------------------------------------
// REST surface (non-streaming actions)
//
// Session listing and detail live in the management surface
// (`/api/v1/sessions` and `/api/v1/sessions/{id}`); see
// `../api/types.ts` for the canonical SessionSummary /
// SessionDetail wire shape and `../pages/SessionsPage.tsx` /
// `../pages/SessionDetailPage.tsx` for the consumers.
// ---------------------------------------------------------------------------

export interface UsageResponse {
  tokens_in: number;
  tokens_out: number;
  turns: number;
  sessions_total: number;
}

/**
 * Drop the in-flight run (if any) on a session. Used by the
 * "Stop" affordance and by navigation away from a half-finished
 * turn.
 */
export async function cancelSession(sessionId: string, signal?: AbortSignal): Promise<void> {
  await api.post<void>(
    `/api/v1/chat/sessions/${encodeURIComponent(sessionId)}/cancel`,
    undefined,
    signal,
  );
}

/**
 * Replay the most recent user turn against the same attachments
 * — backs the "Regenerate" button.
 */
export async function regenerate(sessionId: string, signal?: AbortSignal): Promise<void> {
  await api.post<void>(
    `/api/v1/chat/sessions/${encodeURIComponent(sessionId)}/regenerate`,
    undefined,
    signal,
  );
}

/**
 * Record a thumbs-up / thumbs-down verdict against an
 * assistant message. The server persists the row in the
 * session JSONL so a future analytics endpoint can aggregate.
 */
export async function submitFeedback(
  messageId: string,
  thumbsUp: boolean,
  signal?: AbortSignal,
): Promise<void> {
  await api.post<void>(
    `/api/v1/chat/messages/${encodeURIComponent(messageId)}/feedback`,
    { thumbs_up: thumbsUp },
    signal,
  );
}

/**
 * Snapshot the process-wide usage counters — surfaced as a chip
 * in the header so the user sees the cumulative token spend.
 */
export async function getUsage(signal?: AbortSignal): Promise<UsageResponse> {
  return api.get<UsageResponse>('/api/v1/chat/usage', signal);
}

/**
 * Enumerate registered models + the workspace default — backs
 * the model-selector dropdown.
 */
export interface ModelEntry {
  provider: string;
  model: string;
  context_window: number;
  supports_tools: boolean;
  supports_streaming: boolean;
}

export interface ModelsResponse {
  models: ModelEntry[];
  default_provider: string;
  default_model: string;
}

export async function listModels(signal?: AbortSignal): Promise<ModelsResponse> {
  return api.get<ModelsResponse>('/api/v1/models', signal);
}

// ---------------------------------------------------------------------------
// Stream surface
// ---------------------------------------------------------------------------

export interface SendMessageStreamOptions {
  sessionId?: string;
  attachments?: AttachmentPart[];
  metadata?: Record<string, unknown>;
  signal?: AbortSignal;
  /** Optional override for the model used for this turn. The
   *  chat UI's model selector sets this; the server falls back
   *  to the workspace default when `undefined`. */
  model?: string;
}

interface SendMessageResponse {
  message_id: string;
  queued: boolean;
}

/**
 * Send a user turn and yield the server's events as
 * [`SessionStreamEvent`]s. Wire-compatible with the previous
 * `chat-stream.ts::sendMessageStream` so `ChatPage.tsx` can keep
 * its existing reducer untouched.
 *
 * Internally we:
 *   1. POST the message to `/api/v1/chat/sessions/{id}/messages`,
 *      receiving an opaque `message_id`.
 *   2. Open an SSE channel on
 *      `/api/v1/chat/sessions/{id}/messages/stream` and adapt
 *      each `AgentEvent`-shaped frame to the chat wire shape the
 *      reducer already understands.
 *
 * The stream stays open until the server emits a terminal
 * `System(SessionEnded)` frame, after which we resolve the
 * generator so the `for await ... of` loop in `ChatPage` exits
 * naturally.
 */
export async function* sendMessageStream(
  text: string,
  options: SendMessageStreamOptions = {},
): AsyncGenerator<SessionStreamEvent> {
  const { sessionId, attachments, metadata, model } = options;
  if (!sessionId) {
    yield {
      type: 'error',
      error: { code: 400, message: 'sessionId is required for chat streaming' },
    };
    return;
  }

  // Step 1 — dispatch the turn. The server returns a
  // `{message_id, queued}` envelope; we don't surface the id to
  // the page today but it's reserved for a future "edit this
  // message" affordance.
  try {
    await api.post<SendMessageResponse>(
      `/api/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
      {
        text,
        attachments: attachmentsToWire(attachments ?? []),
        agent_name: (metadata?.['synthia.agent_name'] as string | undefined) ?? undefined,
        model: model ?? undefined,
      },
      options.signal,
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    yield { type: 'error', error: { code: -1, message } };
    return;
  }

  // Step 2 — open the SSE channel. We use a hand-rolled
  // `ReadableStream` reader (the native browser SSE API uses
  // `EventSource` but `EventSource` doesn't support POST /
  // custom headers / abort signals in a clean way, so we
  // stream from `fetch` directly).
  const url = `/api/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages/stream`;
  let response: Response;
  try {
    response = await fetch(url, {
      headers: {
        Accept: 'text/event-stream',
        // Forward the API key so the auth layer sees the same
        // bearer as a regular REST call. The `api` client keeps
        // its own copy; we read the same storage key here so
        // the two paths agree.
        ...bearerHeader(),
      },
      signal: options.signal,
      credentials: 'same-origin',
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    yield { type: 'error', error: { code: -1, message } };
    return;
  }
  if (!response.ok || !response.body) {
    const text = await response.text().catch(() => '');
    yield {
      type: 'error',
      error: {
        code: response.status,
        message: text || `stream open failed: HTTP ${response.status}`,
      },
    };
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let sawTerminal = false;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      // SSE frames are separated by `\n\n`; one chunk may
      // contain several frames so we split greedily.
      let sepIdx: number;
      while ((sepIdx = buffer.indexOf('\n\n')) !== -1) {
        const frame = buffer.slice(0, sepIdx);
        buffer = buffer.slice(sepIdx + 2);
        const events = parseSseFrame(frame);
        for (const ev of events) {
          const adapted = adaptAgentEvent(ev);
          if (adapted) {
            if (adapted.type === 'error' && isTerminalError(adapted)) {
              sawTerminal = true;
            }
            yield adapted;
            if (isTerminalFrame(ev)) {
              sawTerminal = true;
            }
          }
        }
      }
      if (sawTerminal) break;
    }
    // Drain any leftover bytes (server may close without the
    // trailing `\n\n`).
    if (buffer.trim().length > 0) {
      const events = parseSseFrame(buffer);
      for (const ev of events) {
        const adapted = adaptAgentEvent(ev);
        if (adapted) yield adapted;
      }
    }
    // If the server closed the stream without emitting a
    // terminal `SessionEnded` frame (some providers just drop
    // the connection after the last token), synthesize one so
    // the chat page can flip the assistant turn out of
    // `working`. Without this, regenerate/feedback stay
    // disabled forever on truncated streams.
    if (!sawTerminal) {
      yield {
        type: 'turnStatus',
        turnStatus: {
          sessionId: '',
          contextId: '',
          status: { state: 'SESSION_STATE_COMPLETED' },
        },
      };
    }
  } catch (err) {
    if ((err as { name?: string }).name === 'AbortError') {
      // Cancellation is the user's intent — exit silently so
      // the page can mark the turn `cancelled`.
      return;
    }
    const message = err instanceof Error ? err.message : String(err);
    yield { type: 'error', error: { code: -1, message } };
  } finally {
    try {
      await reader.cancel();
    } catch {
      // ignore — best-effort cleanup
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers — SSE parsing + agent-event → chat-wire adaptation
// ---------------------------------------------------------------------------

function bearerHeader(): Record<string, string> {
  try {
    const key = localStorage.getItem('synthia.apiKey');
    if (!key) return {};
    return { Authorization: `Bearer ${key}` };
  } catch {
    return {};
  }
}

function parseSseFrame(frame: string): unknown[] {
  const out: unknown[] = [];
  for (const line of frame.split('\n')) {
    if (!line.startsWith('data:')) continue;
    const payload = line.slice(5).trim();
    if (!payload || payload === '[DONE]') continue;
    try {
      out.push(JSON.parse(payload));
    } catch {
      // Bad frame — skip it rather than crashing the whole
      // stream. A malformed line is unrecoverable for this
      // frame but the next one is still meaningful.
    }
  }
  return out;
}

function adaptAgentEvent(frame: unknown): SessionStreamEvent | null {
  if (!frame || typeof frame !== 'object') return null;
  const f = frame as { type?: string; data?: unknown };
  // The server emits `AgentEvent::Model(ContentPart)` and
  // `AgentEvent::System(SystemEvent)` frames. Map them onto
  // the chat reducer vocabulary:
  //   - ContentPart::Text → message (assistant role)
  //   - ContentPart::ToolUse → message (tool_call) with input
  //   - ContentPart::ToolResult → message (tool_result) with body
  //   - SystemEvent::SessionEnded → turnStatus (terminal)
  if (f.type === 'Model') {
    const part = f.data as Record<string, unknown> | undefined;
    if (!part) return null;
    const message: WireMessage = {
      messageId: crypto.randomUUID(),
      role: 'agent',
      parts: [part as WirePart],
    };
    return {
      type: 'message',
      message,
    };
  }
  if (f.type === 'System') {
    // System frames are observed for terminal detection but
    // the reducer only needs the streaming `Model` frames;
    // synthesize a turnStatus so the loop can observe the
    // close. When the wire frame is a session-end frame we
    // carry its reason through to the synthesized status so
    // the assistant turn leaves the `working` state on the
    // final tick (and lands on `completed` / `canceled` /
    // `failed` as appropriate), instead of flipping back to
    // `working` after the loop exits.
    const terminalState = terminalFrameState(frame);
    return {
      type: 'turnStatus',
      turnStatus: {
        sessionId: '',
        contextId: '',
        status: {
          state: terminalState ?? 'SESSION_STATE_WORKING',
        },
      },
    };
  }
  if (f.type === 'Agent') {
    // Recursive subagent trace — flatten the inner event.
    const inner = (f as { data?: [unknown, unknown] }).data;
    if (Array.isArray(inner) && inner.length === 2) {
      return adaptAgentEvent(inner[1]);
    }
  }
  return null;
}

/**
 * Read the wire-level session end reason and translate it into
 * the `SESSION_STATE_*` enum string the chat reducer expects.
 *
 * Wire: `System { data: { type: "session_ended", reason: "Completed" | "Cancelled" | "Failed" } }`
 * (see `synthia-server::event_stream`). Returns `null` when the
 * frame isn't a session-end frame so the caller can fall through.
 */
function terminalFrameState(frame: unknown): string | null {
  if (!frame || typeof frame !== 'object') return null;
  const f = frame as {
    type?: string;
    data?: { type?: string; kind?: string; reason?: string };
  };
  if (f.type !== 'System') return null;
  const dataType = f.data?.type;
  const kind = f.data?.kind;
  const isEnd = dataType === 'session_ended' || kind === 'SessionEnded' || kind === 'End';
  if (!isEnd) return null;
  const reason = (f.data?.reason ?? '').toString();
  if (/cancel/i.test(reason)) return 'SESSION_STATE_CANCELED';
  if (/fail|error/i.test(reason)) return 'SESSION_STATE_FAILED';
  return 'SESSION_STATE_COMPLETED';
}

/** @deprecated prefer `terminalFrameState` — kept for tests. */
function isTerminalFrame(frame: unknown): boolean {
  return terminalFrameState(frame) !== null;
}

function isTerminalError(_event: SessionStreamEvent): boolean {
  return false;
}

/**
 * Convert frontend `AttachmentPart`s into the REST wire shape the
 * server expects. Binary attachments carry a base64 data URL;
 * URL attachments carry a remote URI; the server resolves the
 * dispatch based on `kind`.
 */
function attachmentsToWire(attachments: AttachmentPart[]): WireAttachment[] {
  const out: WireAttachment[] = [];
  for (const a of attachments) {
    if (a.kind === 'text') continue;
    if (a.kind === 'image') {
      out.push({
        kind: 'image',
        url: a.url,
        data_base64: a.dataUrl ? dataUrlPayload(a.dataUrl) : undefined,
        mime_type: a.mimeType,
        filename: a.filename,
      });
    } else if (a.kind === 'audio') {
      out.push({
        kind: 'audio',
        data_base64: dataUrlPayload(a.dataUrl),
        mime_type: a.mimeType,
        filename: a.filename,
      });
    } else if (a.kind === 'file') {
      out.push({
        kind: 'file',
        data_base64: dataUrlPayload(a.dataUrl),
        mime_type: a.mimeType,
        filename: a.filename,
      });
    }
  }
  return out;
}

interface WireAttachment {
  kind: string;
  data_base64?: string;
  url?: string;
  mime_type?: string;
  filename?: string;
}

function dataUrlPayload(dataUrl: string): string | undefined {
  const m = /^data:[^;]+(;base64)?,(.*)$/s.exec(dataUrl);
  if (!m) return dataUrl;
  return m[2] ?? '';
}

/**
 * Pre-warm hook — kept as a no-op so `main.tsx` keeps calling
 * it without a code change. The REST+SSE stack does not need
 * any client warm-up because every request opens a fresh
 * fetch.
 */
export function initChatClient(): Promise<void> {
  return Promise.resolve();
}

/**
 * Detect an agent card / session failure and surface it as an
 * error event the chat reducer can render. Useful when the
 * caller awaits a non-streaming action and wants to forward a
 * connection failure into the transcript the same way the
 * streaming path does.
 */
export function errorEvent(message: string, code = -1): SessionStreamEvent {
  return { type: 'error', error: { code, message } };
}

// ---------------------------------------------------------------------------
// Segment extraction — same `Part` → typed-segment contract the
// reducer in `ChatPage.tsx` consumes. Detection is by natural
// JSON keys (no synthetic `kind` discriminator) so future
// providers that change the field names only need to update
// this one function.
// ---------------------------------------------------------------------------

/**
 * Classify a `Part` JSON payload as a tool call or tool
 * result. Detection is by natural JSON keys (no synthetic
 * `kind` discriminator) so future providers that change the
 * field names only need to update this one function.
 *
 * Accepts either shape the wire might carry:
 *   - The new `synthia_provider::ContentPart` representation
 *     (serialised by serde with `tag = "type"`, all fields at
 *     the top level), e.g. `{type: "tool_use", id, name, input}`.
 *   - The legacy `Part.data` wrapper that nests the
 *     payload under a `data` key.
 */
export function classifyPartPayload(
  payload: Record<string, unknown>,
): 'tool_call' | 'tool_result' | null {
  // Pass 1 — look at the payload as-is (new synthia-provider
  // wire shape: top-level fields with a `type` tag).
  if (typeof payload.id === 'string' && typeof payload.name === 'string') {
    return 'tool_call';
  }
  if (typeof payload.tool_use_id === 'string' && 'content' in payload) {
    return 'tool_result';
  }
  // Pass 2 — fall back to the legacy `data` wrapper.
  const inner = payload.data;
  if (inner && typeof inner === 'object') {
    const nested = inner as Record<string, unknown>;
    if (typeof nested.id === 'string' && typeof nested.name === 'string') {
      return 'tool_call';
    }
    if (typeof nested.tool_use_id === 'string' && 'content' in nested) {
      return 'tool_result';
    }
  }
  return null;
}

function readPartText(part: WirePart): string {
  return typeof part.text === 'string' ? part.text : '';
}

/**
 * Lower a streamed `WireMessage` to a `PartWithMetadata` that
 * the reducer can dispatch on. Picks the most appropriate
 * segment kind based on the part's natural shape:
 *   - `text` parts → `text`
 *   - `reasoning` parts → `thinking`
 *   - `tool_use` parts → `tool_call`
 *   - `tool_result` parts → `tool_result`
 *   - everything else → `null` (caller decides the fallback).
 */
export function extractFromMessage(message: WireMessage): PartWithMetadata | null {
  const parts = message.parts ?? [];
  for (const part of parts) {
    const merged = mergePart(part);
    const kind = classifyPartPayload(merged);
    if (kind === 'tool_call') {
      return {
        type: 'tool_call',
        text: readPartText(part),
        metadata: partMetadata(merged),
      };
    }
    if (kind === 'tool_result') {
      return {
        type: 'tool_result',
        text: '',
        metadata: partMetadata(merged),
      };
    }
    if (merged.type === 'reasoning' || part.type === 'reasoning') {
      return {
        type: 'thinking',
        text: readPartText(part) || (typeof merged.text === 'string' ? merged.text : ''),
        metadata: partMetadata(merged),
      };
    }
    if (merged.type === 'text' || part.type === 'text') {
      return {
        type: 'text',
        text: readPartText(part),
        metadata: partMetadata(merged),
      };
    }
  }
  return null;
}

function mergePart(part: WirePart): Record<string, unknown> {
  const out: Record<string, unknown> = { ...part };
  if (part.data && typeof part.data === 'object') {
    Object.assign(out, part.data as Record<string, unknown>);
  }
  return out;
}

function partMetadata(part: Record<string, unknown>): SegmentMetadata | undefined {
  const meta: SegmentMetadata = {};
  if (typeof part.id === 'string') meta.tool_use_id = part.id;
  if (typeof part.name === 'string') meta.tool_name = part.name;
  if (typeof part.tool_use_id === 'string') meta.tool_use_id = part.tool_use_id;
  if (typeof part.text === 'string') meta.text = part.text;
  if (part.input !== undefined) meta.input = part.input;
  if (typeof part.is_error === 'boolean') meta.is_error = part.is_error;
  return Object.keys(meta).length > 0 ? meta : undefined;
}
