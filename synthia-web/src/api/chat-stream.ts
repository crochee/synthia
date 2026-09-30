/**
 * Chat transport for the synthia-web frontend.
 *
 * A turn is sent over the **Anthropic Messages protocol**
 * (`POST /v1/messages` with `stream: true`) — the same endpoint a
 * stock Anthropic client would drive against this server. Every other
 * action on the chat page (session listing / detail, cancel,
 * regenerate, feedback, usage) stays on the v1 REST surface under
 * `/api/v1/*`.
 *
 * The reducer contract does not move: [`sendMessageStream`] still
 * yields [`SessionStreamEvent`]s and `ChatPage.tsx` still consumes
 * them unchanged. Anthropic frames are lowered into that vocabulary
 * here, by rebuilding each content block as the `AgentEvent`-shaped
 * frame the chat surface has always consumed — the two protocols
 * agree on the shape of a model part (a `type` discriminator with the
 * payload at the root) — and handing it to [`adaptAgentEvent`], so a
 * streamed text / tool / reasoning part takes exactly the code path it
 * takes on `/api/v1/chat/*`.
 *
 * # Session binding
 *
 * The protocol is stateless and a stock client resends its whole
 * transcript; this client deliberately does not. It sends only the
 * final user turn and names its durable session in
 * `metadata.synthia_session_id`, so the run continues the very session
 * the page is showing — the same id the REST actions above, the
 * sessions list and the local message cache are keyed by. The binding
 * requires that session to exist server-side, so the transport
 * introduces it (`POST /api/v1/chat/sessions`) before the first turn.
 */

import { api, resolveApiKey } from './client';
import type { SessionDetail } from './types';
import { contentText, extractImages } from './chat-message';
import type { ChatImage } from './chat-message';

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
  | 'notice'
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
  /** Image parts the frame carried, already shaped for
   *  `<img src>`. A live tool result that returned a picture
   *  arrives here, so the reducer can render it without
   *  re-parsing the wire payload. */
  images?: ChatImage[];
}

export interface SessionStreamEvent {
  type:
    | 'sessionStatus'
    | 'message'
    | 'turnStatus'
    | 'warning'
    | 'attachment'
    | 'error'
    | 'snapshot'
    | 'cursor';
  session?: WireSession;
  message?: WireMessage;
  turnStatus?: {
    sessionId: string;
    contextId: string;
    status: { state: string; message?: WireMessage };
  };
  /** A non-terminal diagnostic notice (e.g. context
   *  compaction after the history was pruned). Rendered as a
   *  muted in-transcript note; never flips turn state. */
  warning?: { kind: string; message: string };
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
  /** Replay head marker: server tells the client which
   *  `last_event_seq` the upcoming replay was carved from.
   *  Compare with the client's own cursor to detect truncation.
   *  See `routes/chat.rs::resume_stream_to_sse`. */
  snapshot?: { lastEventSeq: number; streamIndex: number };
  /** Cursor progress marker emitted during replay and at the
   *  end of every `RESUME_CURSOR_BATCH` live frames. Clients
   *  persist it as `localStorage['synthia.streamIndex.<id>']`
   *  so a future reload can resume from this point. */
  cursor?: { lastEventSeq: number; streamIndex: number };
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
  sessionId: string,
  messageId: string,
  thumbsUp: boolean,
  signal?: AbortSignal,
): Promise<void> {
  await api.post<void>(
    `/api/v1/chat/messages/${encodeURIComponent(messageId)}/feedback`,
    // The session rides in the body: the message id is a client-side
    // label, so the server cannot resolve the session from it, and
    // without this the rating was recorded against a hardcoded
    // "default" session.
    { thumbs_up: thumbsUp, session_id: sessionId },
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

/**
 * Fetch one session's durable transcript.
 *
 * `regenerate` uses this to re-read the log after a rerun: the route
 * answers `202` and the controller replays the turn asynchronously,
 * so the only way the page learns the outcome is to read the result.
 * Rebuilding from the server's own history also keeps the two pages
 * (`/chat/:id` and `/sessions/:id`) showing the same transcript.
 */
export async function getSessionDetail(
  sessionId: string,
  signal?: AbortSignal,
): Promise<SessionDetail> {
  return api.get<SessionDetail>(`/api/v1/sessions/${encodeURIComponent(sessionId)}`, signal);
}

// ---------------------------------------------------------------------------
// Stream surface
// ---------------------------------------------------------------------------

/** The Messages endpoint. Mounted at the root, not under `/api/v1` —
 *  it is the protocol's path, which is what lets a stock Anthropic
 *  client point its `base_url` here. */
const MESSAGES_URL = '/v1/messages';

/** The protocol revision this client speaks. The server accepts and
 *  does not validate it, but every Anthropic client sends it. */
const ANTHROPIC_VERSION = '2023-06-01';

/**
 * `max_tokens` for a turn. Required by the protocol and validated
 * server-side; the run's real token budget is deployment configuration,
 * so this only has to be a sane ceiling. Mirrors the Anthropic
 * profile's default output budget.
 */
const DEFAULT_MAX_TOKENS = 8192;

/**
 * Session binding. Namespaced under `metadata` because the protocol
 * ignores unknown keys there — and because `metadata.user_id` is
 * already spoken for on this wire. Present ⇒ the server continues the
 * named session instead of minting an ephemeral one, which is what
 * keeps cancel / regenerate / feedback / sessions detail / the local
 * message cache pointing at one transcript.
 */
const META_SESSION_ID = 'synthia_session_id';

/** Agent routing for the turn (`metadata.synthia_agent_name`). */
const META_AGENT_NAME = 'synthia_agent_name';

export interface SendMessageStreamOptions {
  sessionId?: string;
  attachments?: AttachmentPart[];
  /**
   * Agent this turn routes to. Rides the frozen
   * `metadata.synthia_agent_name` field so the backend's chat layer
   * dispatches to the agent the page is showing rather than to the
   * configured default. Omitted → the server resolves its own default.
   */
  agentName?: string;
  signal?: AbortSignal;
  /** Model this turn runs on, as `"<provider>/<model>"` — the exact
   *  string the chat UI's model selector shows. Omitted → the workspace
   *  default is read from `/api/v1/models`. */
  model?: string;
}

/**
 * Send a user turn over the Anthropic Messages protocol and yield the
 * server's frames as [`SessionStreamEvent`]s. Wire-compatible with the
 * previous REST+SSE implementation so `ChatPage.tsx` keeps its
 * reducer untouched.
 *
 * One request carries the whole turn: the final user turn in
 * `messages`, `stream: true`, and the session binding in `metadata`.
 * The response is the protocol's SSE sequence —
 *
 * ```text
 * message_start
 * content_block_start → content_block_delta* → content_block_stop   (per block)
 * message_delta
 * message_stop
 * ```
 *
 * — which [`adaptStreamFrame`] lowers into the chat vocabulary.
 *
 * Earlier turns are deliberately NOT resent: the session named by
 * `metadata.synthia_session_id` already holds them durably, and the
 * server continues that session instead of starting a new one. See
 * the module docs.
 */
export async function* sendMessageStream(
  text: string,
  options: SendMessageStreamOptions = {},
): AsyncGenerator<SessionStreamEvent> {
  const { sessionId, attachments, agentName, model } = options;
  if (!sessionId) {
    yield {
      type: 'error',
      error: { code: 400, message: 'sessionId is required for chat streaming' },
    };
    return;
  }

  const { content, warnings } = contentBlocks(text, attachments ?? []);
  for (const warning of warnings) yield warning;

  // The protocol requires a non-empty `model`. The selector normally
  // supplies one; when it could not (the catalog request failed) the
  // workspace default is read from the same endpoint the selector is
  // built from. A failure here is reported rather than guessed at.
  const resolvedModel = model ?? (await workspaceDefaultModel());
  if (!resolvedModel) {
    yield {
      type: 'error',
      error: {
        code: -1,
        message:
          'No model selected and the model catalog could not be read, so this turn has no provider to run on.',
      },
    };
    return;
  }

  const body: Record<string, unknown> = {
    model: resolvedModel,
    max_tokens: DEFAULT_MAX_TOKENS,
    // Only the final user turn: the bound session already holds the
    // transcript, so resending it would duplicate the history.
    messages: [{ role: 'user', content }],
    stream: true,
    metadata: {
      [META_SESSION_ID]: sessionId,
      ...(agentName ? { [META_AGENT_NAME]: agentName } : {}),
    },
  };

  let response: Response;
  try {
    response = await fetch(MESSAGES_URL, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'text/event-stream',
        // Protocol version. The server accepts and does not validate
        // it, but every Anthropic client sends it and a strict proxy
        // in front of the API expects it.
        'anthropic-version': ANTHROPIC_VERSION,
        ...credentialHeader(),
      },
      body: JSON.stringify(body),
      signal: options.signal,
      credentials: 'same-origin',
    });
  } catch (err) {
    // An aborted request is the user's Stop, not a failure: exit
    // silently so the page can mark the turn `cancelled` (the abort
    // *is* the protocol-native cancel — the server drops the run when
    // the response stops being polled).
    if ((err as { name?: string } | null)?.name === 'AbortError') return;
    yield {
      type: 'error',
      error: { code: -1, message: err instanceof Error ? err.message : String(err) },
    };
    return;
  }

  if (!response.ok || !response.body) {
    yield {
      type: 'error',
      error: {
        code: response.status,
        message: await errorBody(response, 'stream open failed'),
      },
    };
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let sawTerminal = false;
  const blocks = new Map<number, BlockState>();
  try {
    while (!sawTerminal) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      // SSE frames are separated by `\n\n`; one chunk may contain
      // several frames so we split greedily.
      for (let end = buffer.indexOf('\n\n'); end !== -1; end = buffer.indexOf('\n\n')) {
        const frame = buffer.slice(0, end);
        buffer = buffer.slice(end + 2);
        for (const payload of parseSseFrame(frame)) {
          for (const event of adaptStreamFrame(payload, blocks)) {
            yield event;
          }
          // `message_stop` ends a stream that completed; `error` ends
          // one that failed. Either way there is nothing left to read.
          const type = (payload as { type?: string } | null)?.type;
          if (type === 'message_stop' || type === 'error') sawTerminal = true;
        }
      }
    }
    // Drain any leftover bytes (the server may close without the
    // trailing `\n\n`).
    if (buffer.trim().length > 0) {
      for (const payload of parseSseFrame(buffer)) {
        for (const event of adaptStreamFrame(payload, blocks)) {
          yield event;
        }
      }
    }
    if (!sawTerminal && options.signal?.aborted !== true) {
      // The socket closed without the protocol's terminal frame —
      // either the server closed after the last token or the
      // connection dropped. Synthesize the terminal status the wire
      // would have produced so the assistant turn leaves `working`;
      // otherwise regenerate / feedback stay disabled forever on a
      // truncated stream.
      //
      // An aborted turn is deliberately excluded: the user's Stop has
      // already marked the turn `canceled`, and a synthesized
      // completion here would race that local verdict.
      yield turnStatus(sessionId, 'SESSION_STATE_COMPLETED');
    }
  } catch (err) {
    if ((err as { name?: string } | null)?.name === 'AbortError') return;
    yield {
      type: 'error',
      error: { code: -1, message: err instanceof Error ? err.message : String(err) },
    };
  } finally {
    try {
      await reader.cancel();
    } catch {
      // ignore — best-effort cleanup
    }
  }
}

// ---------------------------------------------------------------------------
// Anthropic frames → chat vocabulary
// ---------------------------------------------------------------------------

/**
 * The state of a content block that only exists once it closes.
 *
 * Text and thinking need none: each of their deltas is a complete value
 * the reducer appends (that is how the chat surface streams tokens), so
 * they are forwarded as they arrive. A tool call's input, by contrast,
 * arrives as JSON fragments that are meaningless until the last one,
 * and a tool result's payload only exists as one whole block — so those
 * two are held here until their `content_block_stop`.
 */
interface BlockState {
  type: string;
  /** Concatenated `input_json_delta` fragments of a `tool_use` block. */
  json: string;
  /** `tool_use.id` — the pairing key a later `tool_result` names. */
  id?: string;
  /** `tool_use.name` / `tool_result.name` (the latter is optional). */
  name?: string;
  /**
   * A `tool_result`'s payload, as the protocol spells it: `content` is
   * either a string or a block array, and it arrives whole on
   * `content_block_start` — no deltas follow it.
   */
  result?: unknown;
  isError?: boolean;
  toolUseId?: string;
}

/**
 * Lower one Anthropic SSE payload into chat events.
 *
 * ```text
 * message_start                              → (assistant bubble already open)
 * content_block_start  {text|thinking}       → (nothing until its deltas)
 * content_block_delta  text_delta            → a text part
 * content_block_delta  thinking_delta        → a reasoning part
 * content_block_start  {tool_use}            → (buffered)
 * content_block_delta  input_json_delta      → (buffered)
 * content_block_stop                         → the assembled tool call
 * content_block_start  {tool_result}         → (buffered)
 * content_block_stop                         → the tool result
 * message_delta / message_stop               → the terminal turn status
 * error                                      → an error event
 * ```
 *
 * Every part is emitted as the `AgentEvent`-shaped frame the chat
 * surface has always consumed and handed to [`adaptAgentEvent`], so it
 * takes exactly the same code path here as it does on
 * `/api/v1/chat/*` — including the `tool_use` / `tool_result` pairing
 * the reducer does on their shared `tool_use_id`.
 *
 * `blocks` is the accumulator, keyed by the protocol's block `index`
 * (a stream can interleave blocks, so it is not a single slot).
 */
function adaptStreamFrame(payload: unknown, blocks: Map<number, BlockState>): SessionStreamEvent[] {
  if (!payload || typeof payload !== 'object') return [];
  const frame = payload as {
    type?: string;
    index?: number;
    content_block?: Record<string, unknown>;
    delta?: Record<string, unknown>;
    error?: { type?: string; message?: string };
  };
  switch (frame.type) {
    case 'message_start':
      // Nothing to render: the message's identity is the assistant
      // bubble the page opened before sending, and its `content` is
      // empty until the blocks below arrive.
      return [];
    case 'content_block_start': {
      const block = frame.content_block ?? {};
      const type = typeof block.type === 'string' ? block.type : 'text';
      // Text / thinking carry their content in deltas, so nothing
      // needs to be held for them.
      if (type === 'text' || type === 'thinking') return [];
      blocks.set(frame.index ?? 0, {
        type,
        json: '',
        id: typeof block.id === 'string' ? block.id : undefined,
        name: typeof block.name === 'string' ? block.name : undefined,
        result: block.content,
        isError: block.is_error === true,
        toolUseId: typeof block.tool_use_id === 'string' ? block.tool_use_id : undefined,
      });
      return [];
    }
    case 'content_block_delta': {
      const delta = frame.delta ?? {};
      const chunk = (value: unknown) => (typeof value === 'string' ? value : '');
      if (delta.type === 'text_delta') {
        const text = chunk(delta.text);
        return text ? modelFrame({ type: 'text', text }) : [];
      }
      if (delta.type === 'thinking_delta') {
        const text = chunk(delta.thinking);
        return text ? modelFrame({ type: 'reasoning', text }) : [];
      }
      // Tool input streams as partial JSON fragments; the value only
      // exists once the last fragment landed, so it is buffered until
      // the block closes.
      const state = blocks.get(frame.index ?? 0);
      if (state && delta.type === 'input_json_delta') {
        state.json += chunk(delta.partial_json);
      }
      // `signature_delta` (a thinking block's provider signature) has
      // no counterpart in the chat vocabulary — nothing to carry.
      return [];
    }
    case 'content_block_stop': {
      const index = frame.index ?? 0;
      const state = blocks.get(index);
      if (!state) return [];
      blocks.delete(index);
      if (state.type === 'tool_use') {
        return modelFrame({
          type: 'tool_use',
          id: state.id,
          name: state.name,
          // Partial JSON that never parsed still needs a body, so the
          // renderer shows what the model sent instead of an empty
          // call.
          input: parsePartialJson(state.json),
        });
      }
      if (state.type === 'tool_result') {
        return modelFrame({
          type: 'tool_result',
          tool_use_id: state.toolUseId,
          name: state.name,
          content: normaliseToolResultContent(state.result),
          is_error: state.isError === true,
        });
      }
      // `image` / `audio` / `document` blocks have no chat-vocabulary
      // counterpart; a tool's media rides its `tool_result` instead.
      return [];
    }
    // The run's terminal state. `message_delta` carries the stop
    // reason and the token totals, neither of which the chat
    // vocabulary has a field for, and `message_stop` is the frame that
    // ends the stream — so the terminal status rides that one, exactly
    // once. A mid-run `error` is the protocol's failure signal and
    // takes the dedicated error path in the reducer.
    case 'message_delta':
      return [];
    case 'message_stop':
      return [turnStatus('', 'SESSION_STATE_COMPLETED')];
    case 'error': {
      const detail = frame.error;
      return [
        {
          type: 'error',
          error: {
            // Keep the HTTP-status vocabulary the reducer knows:
            // `-1` is its transport/run-failure signal (it also flips
            // the header's server-health flag). A mid-stream error
            // frame means the run failed after the request was
            // accepted, not that the server is down.
            code: -1,
            message: detail?.message ?? detail?.type ?? 'the agent run failed',
          },
        },
      ];
    }
    default:
      // `ping` and any frame a future protocol revision adds — an
      // unknown frame must never break the stream.
      return [];
  }
}

/**
 * A `Model(ContentPart)` frame — one part, lowered into the chat
 * vocabulary by [`adaptAgentEvent`].
 *
 * The two protocols agree on a part's shape: a `type` discriminator
 * with the payload at the root. That is why no second reducer is
 * needed here — the part is wrapped in the envelope the chat surface
 * has always received and adapted by the same mapper.
 */
function modelFrame(part: Record<string, unknown>): SessionStreamEvent[] {
  const event = adaptAgentEvent({ type: 'Model', data: part });
  return event ? [event] : [];
}

/**
 * Project a `tool_result`'s payload into the shape the chat segment
 * extractor renders.
 *
 * Anthropic sends `string | [block]`; the chat surface reads
 * `[{type:"text", text}]` (text is joined) plus `[{type:"image", …}]`
 * (rendered inline), so a string becomes one text part and a block
 * array is translated into that same spelling. A `text` block's own
 * field name is kept as-is — both spellings are read (`text`).
 */
function normaliseToolResultContent(content: unknown): unknown {
  if (typeof content === 'string') return [{ type: 'text', text: content }];
  if (!Array.isArray(content)) return [];
  const parts: unknown[] = [];
  for (const raw of content) {
    if (!raw || typeof raw !== 'object') continue;
    const block = raw as Record<string, unknown>;
    if (block.type === 'text') {
      parts.push({
        type: 'text',
        text: typeof block.text === 'string' ? block.text : '',
      });
      continue;
    }
    if (block.type === 'image') {
      const source = block.source as
        { type?: string; media_type?: string; data?: string; url?: string } | undefined;
      // `ChatImage` (see `api/chat-message.ts`) renders `data` as a
      // `data:` URL when it is bare base64 and passes a URL through.
      parts.push({
        type: 'image',
        data: source?.type === 'url' ? source.url : source?.data,
        mime_type: source?.media_type,
      });
    }
  }
  return parts;
}

/**
 * Parse a streamed `partial_json` payload.
 *
 * A complete payload parses into the tool's input. A truncated one
 * (the run ended mid-call) is kept verbatim so the call block shows
 * what arrived rather than an empty object.
 */
function parsePartialJson(raw: string): unknown {
  if (raw.length === 0) return {};
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/**
 * The turn-status event the chat page renders.
 *
 * `contextId` mirrors `sessionId`: the chat surface is session-centric
 * while the Anthropic wire carries no context id, and the reducer
 * treats the pair as the turn's identity.
 */
function turnStatus(sessionId: string, state: string): SessionStreamEvent {
  return {
    type: 'turnStatus',
    turnStatus: { sessionId, contextId: sessionId, status: { state } },
  };
}

/** Read the server's error envelope for a rejected request. */
async function errorBody(response: Response, fallback: string): Promise<string> {
  const text = await response.text().catch(() => '');
  if (!text) return `${fallback}: HTTP ${response.status}`;
  // The protocol's envelope is `{"type":"error","error":{…}}`; the
  // application's is `{"code", "message"}`. Accept either so a proxy
  // or a non-Anthropic error path still yields a readable sentence.
  try {
    const parsed = JSON.parse(text) as {
      message?: string;
      error?: { message?: string };
    };
    const message = parsed.error?.message ?? parsed.message;
    if (typeof message === 'string' && message.length > 0) {
      return `${message} (HTTP ${response.status})`;
    }
  } catch {
    // Not JSON — fall through to the raw body.
  }
  return `${text} (HTTP ${response.status})`;
}

/**
 * The model the workspace runs on, as `"<provider>/<model>"`.
 *
 * The protocol requires a `model` on every request. The composer's
 * selector normally supplies the user's choice; this is the fallback
 * for the window where the catalog request failed (or has not landed
 * yet) — the same `/api/v1/models` answer the selector is seeded from.
 * Cached for the page's lifetime, and a failure is *not* cached so a
 * later turn can still resolve one.
 */
let defaultModel: Promise<string | null> | null = null;

function workspaceDefaultModel(): Promise<string | null> {
  defaultModel ??= listModels().then(
    (catalog) =>
      catalog.default_provider && catalog.default_model
        ? `${catalog.default_provider}/${catalog.default_model}`
        : null,
    () => {
      defaultModel = null;
      return null;
    },
  );
  return defaultModel;
}

// ---------------------------------------------------------------------------
// Helpers — SSE parsing + agent-event → chat-wire adaptation
// ---------------------------------------------------------------------------

/**
 * The credential the rest of the app authenticates with, in both
 * spellings this server accepts: `x-api-key` is what an Anthropic
 * client sends, `Authorization` is what every other surface here
 * sends. The server resolves `Authorization` first, so the two can
 * never disagree — they are read from one source.
 */
function credentialHeader(): Record<string, string> {
  const key = resolveApiKey();
  if (!key) return {};
  return { 'x-api-key': key, Authorization: `Bearer ${key}` };
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

/**
 * Sub-agent event adaptation context. When a frame is nested
 * inside `AgentEvent::Agent(meta, inner)`, `adaptAgentEvent`
 * recurses with this context so inner content surfaces as a
 * dedicated `role: 'subagent'` message bubble (keyed by the
 * child trace id) instead of being flattened into the parent
 * turn.
 */
export interface SubagentTraceContext {
  /** `AgentMeta.parent_depth` of the child run. */
  depth: number;
  /** `AgentMeta.child_session_id` — a per-run trace id the
   *  backend mints for every delegated child (ULID), so
   *  parallel children stay distinguishable. Absent on old
   *  transcripts that predate the id. */
  childSessionId?: string;
}

/** Metadata keys carried on `WireMessage.metadata` for
 *  `role: 'subagent'` messages. Dot-namespaced so they cannot
 *  collide with future provider metadata. */
export const META_SUBAGENT_DEPTH = 'synthia.subagentDepth';
export const META_SUBAGENT_CHILD = 'synthia.childSessionId';

/**
 * Read the sub-agent depth off an adapted `message` event's
 * metadata.
 */
export function subagentDepthOf(message: WireMessage): number | undefined {
  const d = message.metadata?.[META_SUBAGENT_DEPTH];
  return typeof d === 'number' ? d : undefined;
}

/**
 * Read the sub-agent child trace id off an adapted `message`
 * event's metadata.
 */
export function subagentChildOf(message: WireMessage): string | undefined {
  const id = message.metadata?.[META_SUBAGENT_CHILD];
  return typeof id === 'string' && id.length > 0 ? id : undefined;
}

export function adaptAgentEvent(
  frame: unknown,
  nested?: SubagentTraceContext | null,
): SessionStreamEvent | null {
  if (!frame || typeof frame !== 'object') return null;
  const f = frame as { type?: string; data?: unknown };
  // The server emits `AgentEvent::Model(ContentPart)` and
  // `AgentEvent::System(SystemEvent)` frames. Map them onto
  // the chat reducer vocabulary:
  //   - ContentPart::Text → message (assistant role)
  //   - ContentPart::ToolUse → message (tool_call) with input
  //   - ContentPart::ToolResult → message (tool_result) with body
  //   - SystemEvent::SessionEnded → turnStatus (terminal)
  //   - SystemEvent::Warning → warning (non-terminal notice)
  if (f.type === 'Model') {
    const part = f.data as Record<string, unknown> | undefined;
    if (!part) return null;
    const message: WireMessage = {
      messageId: crypto.randomUUID(),
      role: nested ? 'subagent' : 'agent',
      parts: [part as WirePart],
    };
    if (nested) {
      message.metadata = {
        [META_SUBAGENT_DEPTH]: nested.depth,
        ...(nested.childSessionId ? { [META_SUBAGENT_CHILD]: nested.childSessionId } : {}),
      };
    }
    return {
      type: 'message',
      message,
    };
  }
  if (f.type === 'System') {
    // System frames inside a sub-agent trace are lifecycle noise
    // for the PARENT turn: especially the child's own
    // `SessionEnded`, which must never terminate the parent's
    // assistant message. Suppress them entirely at this level.
    if (nested) return null;
    // Warnings (context compaction, token budget, loop guard,
    // hook denials, edit conflicts) are diagnostics worth
    // surfacing as an in-transcript notice — not as a status
    // flip. SystemEvent::Warning serialises as
    // `{type: "warning", kind: "<snake_case>", message, iteration}`.
    const data = f.data as
      { type?: string; kind?: string; message?: unknown; reason?: unknown } | undefined;
    if (
      data &&
      (data.type === 'warning' || data.kind === 'Warning') &&
      typeof data.message === 'string'
    ) {
      const kind = typeof data.kind === 'string' && data.kind !== 'Warning' ? data.kind : 'warning';
      return {
        type: 'warning',
        warning: { kind, message: data.message },
      };
    }
    // Non-warning System frames are observed for terminal
    // detection but the reducer only needs the streaming
    // `Model` frames; synthesize a turnStatus so the loop can
    // observe the close. When the wire frame is a session-end
    // frame we carry its reason through to the synthesized
    // status so the assistant turn leaves the `working` state
    // on the final tick (and lands on `completed` / `canceled`
    // / `failed` as appropriate), instead of flipping back to
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
    // Recursive sub-agent trace. The wire payload is
    // `[AgentMeta, inner AgentEvent]`; unwrap it, remember the
    // child's identity, and recurse so the inner event adapts
    // with sub-agent context. Inner `System` frames are
    // suppressed by the `nested` guard above, so a child's
    // lifecycle can never end (or restart) the parent turn.
    const data = f.data;
    if (Array.isArray(data) && data.length >= 2) {
      const meta = (data[0] ?? {}) as Record<string, unknown>;
      const rawDepth = meta.parent_depth;
      const rawChildId = meta.child_session_id;
      const context: SubagentTraceContext = {
        depth: typeof rawDepth === 'number' ? rawDepth : (nested?.depth ?? 0) + 1,
        ...(typeof rawChildId === 'string' && rawChildId.length > 0
          ? { childSessionId: rawChildId }
          : {}),
      };
      return adaptAgentEvent(data[1], context);
    }
    return null;
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

/**
 * Build the turn's `content` array: the typed text the user sent plus
 * any attachment they queued.
 *
 * Anthropic's content blocks are the protocol's own multimodal shape,
 * so this is a direct projection — a `{kind:'image'}` attachment with
 * inline bytes (or a remote URL) becomes an `image` block, and a
 * `{kind:'file'}` attachment (PDF, plain text, markdown) becomes a
 * `document` block carrying its base64 bytes, both next to the `text`
 * block.
 *
 * The protocol has no block for audio, so the composer no longer
 * offers it. An attachment that does carry a payload is therefore never
 * dropped: the one remaining warning case is a part whose bytes never
 * made it into a `data:` URL, which this wire cannot represent. That
 * warning is rendered as a muted in-transcript notice (the same
 * vocabulary the transcript already uses for diagnostics), leaving the
 * text and the other attachments of the turn intact.
 */
function contentBlocks(
  text: string,
  attachments: ReadonlyArray<AttachmentPart>,
): { content: unknown[]; warnings: SessionStreamEvent[] } {
  const content: unknown[] = [];
  const warnings: SessionStreamEvent[] = [];
  if (text) content.push({ type: 'text', text });
  for (const attachment of attachments) {
    if (attachment.kind === 'text') continue;
    if (attachment.kind === 'image') {
      const payload = attachment.dataUrl ? dataUrlPayload(attachment.dataUrl) : undefined;
      if (payload !== undefined) {
        content.push({
          type: 'image',
          source: {
            type: 'base64',
            // The protocol requires a media type here; the composer
            // always records the file's own type.
            media_type: attachment.mimeType,
            data: payload,
          },
        });
      } else if (attachment.url) {
        content.push({ type: 'image', source: { type: 'url', url: attachment.url } });
      } else {
        warnings.push(unsupportedAttachment(attachment, 'it carries no bytes or URL'));
      }
      continue;
    }
    const payload = dataUrlPayload(attachment.dataUrl);
    if (payload !== undefined) {
      content.push({
        type: 'document',
        source: {
          type: 'base64',
          media_type: attachment.mimeType,
          data: payload,
        },
        // The protocol's own field for the document's display name. It
        // is the only route by which the filename reaches the server's
        // canonical `ResourceLink` (and therefore the durable
        // attachment reference a reloaded transcript renders), so a
        // turn without it can only ever show its MIME type.
        ...(attachment.filename ? { title: attachment.filename } : {}),
      });
    } else {
      warnings.push(unsupportedAttachment(attachment, 'it carries no bytes'));
    }
  }
  return { content, warnings };
}

function unsupportedAttachment(
  attachment: Exclude<AttachmentPart, { kind: 'text' }>,
  reason: string,
): SessionStreamEvent {
  return {
    type: 'warning',
    warning: {
      kind: 'attachment_not_sent',
      message: `Attachment "${attachment.filename ?? attachment.kind}" was not sent: ${reason}.`,
    },
  };
}

/**
 * The base64 payload of a `data:` URL, with the `data:<mime>[;params],`
 * prefix stripped — the Messages protocol wants the raw base64 in
 * `source.data`, never the URL. The parameter list is consumed in
 * full, so a parameterised type (`text/plain;charset=utf-8` is a legal,
 * if rare, `File.type`) cannot be mistaken for a payload prefix. A
 * string that is not a `data:` URL yields `undefined`: there are no
 * bytes to send.
 */
function dataUrlPayload(dataUrl: string): string | undefined {
  const m = /^data:[^,]*,([\s\S]*)$/s.exec(dataUrl);
  return m ? (m[1] ?? '') : undefined;
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
      // `ToolResult` has no `text` field — its body lives in the
      // `content` array (or a plain string on older wire
      // versions), so flatten it here. Without this the live
      // result block rendered empty for *every* tool result,
      // text-only ones included; `partMetadata` only looks at
      // `part.text`, which a tool result never carries.
      const images = extractImages(merged.content);
      return {
        type: 'tool_result',
        text: contentText(merged.content),
        metadata: partMetadata(merged),
        images: images.length > 0 ? images : undefined,
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

// ---------------------------------------------------------------------------
// Session stream reconnect (resume protocol)
//
// The server exposes a GET endpoint that replays every durable
// event after a cursor and then continues with the live tail:
//
//   GET /api/v1/chat/sessions/{id}/stream?from=<seq>
//
// Wire shape (SSE with named events):
//
//   event: snapshot
//   data: {"last_event_seq": <u64>}
//
//   event: message      // one frame per replayed / live event
//   data: <AgentEvent JSON>
//
//   event: cursor
//   data: {"last_event_seq": <u64>}
//
// `subscribeSession` opens that endpoint and yields the same
// `SessionStreamEvent` vocabulary as `sendMessageStream` —
// replayed events go through `adaptAgentEvent` so the chat
// reducer doesn't need new code paths. `snapshot` and `cursor`
// arrive as dedicated event types that the caller can persist
// or surface.
// ---------------------------------------------------------------------------

export interface SubscribeSessionOptions {
  /** Replay cursor (per-session stream_index). When
   *  `undefined`, the server treats it as `0` -- the
   *  "replay from the start" cursor -- and the wire always
   *  carries snapshot + cursor frames
   *  (`2026-09-24-stream-index.md` §4.4). */
  fromStreamIndex?: number;
  /** External cancellation -- mirrors `sendMessageStream`. */
  signal?: AbortSignal;
  /** Called every time a `cursor` frame arrives. The caller
   *  persists the value to localStorage so a future reload
   *  can resume from this point. Receives the
   *  `stream_index` (the per-session wire cursor). */
  onCursor?: (streamIndex: number) => void;
}

const STREAM_RESUME_URL = (sessionId: string): string =>
  `/api/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages/stream`;

/**
 * Iterate one SSE stream into named events. The existing
 * `parseSseFrame` reads only `data:` lines; SSE events with a
 * named event type (`event: snapshot` / `event: cursor` /
 * `event: message`) need the `event:` line too. Frames are
 * delimited by a blank line (`\n\n`).
 */
async function* parseSseStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
): AsyncGenerator<{ event: string; data: unknown }> {
  const decoder = new TextDecoder('utf-8');
  let buffer = '';
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let idx = buffer.indexOf('\n\n');
    while (idx !== -1) {
      const raw = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 2);
      let eventName = 'message';
      let dataPayload: unknown = undefined;
      let hasData = false;
      for (const line of raw.split('\n')) {
        if (line.startsWith(':')) continue; // comment
        if (line.startsWith('event:')) {
          const trimmed = line.slice(6).trim();
          if (trimmed) eventName = trimmed;
        } else if (line.startsWith('data:')) {
          const payload = line.slice(5).trim();
          if (payload === '[DONE]') {
            hasData = false;
            break;
          }
          try {
            dataPayload = JSON.parse(payload);
            hasData = true;
          } catch {
            // Skip malformed frames rather than crashing the
            // stream — a single bad frame is unrecoverable
            // for that frame, but the next one still flows.
          }
        }
        // Other directives (id:, retry:, …) are ignored — the
        // server does not emit them on this endpoint.
      }
      if (hasData) yield { event: eventName, data: dataPayload };
      idx = buffer.indexOf('\n\n');
    }
  }
}

/**
 * Read `last_event_seq` off a `cursor` or `snapshot` payload,
 * tolerating both snake_case (server wire) and camelCase
 * (the existing chat event vocabulary).
 */
function lastEventSeqOf(payload: unknown): number | undefined {
  if (!payload || typeof payload !== 'object') return undefined;
  const obj = payload as Record<string, unknown>;
  const v = obj.last_event_seq ?? obj.lastEventSeq;
  return typeof v === 'number' ? v : undefined;
}

/**
 * Read `stream_index` off a `cursor` or `snapshot` payload.
 * The server now emits the per-session stream cursor as
 * its own field (`stream_index`); the snake_case key
 * (`stream_index`) is the wire contract. `streamIndex`
 * camelCase is kept as a fallback for any older payload
 * shape that hasn't been retired yet
 * (`2026-09-24-stream-index.md` §4.3).
 */
function streamIndexOf(payload: unknown): number | undefined {
  if (!payload || typeof payload !== 'object') return undefined;
  const obj = payload as Record<string, unknown>;
  const v = obj.stream_index ?? obj.streamIndex;
  return typeof v === 'number' ? v : undefined;
}

/**
 * Subscribe to a session's stream and yield `SessionStreamEvent`s.
 *
 * Mirrors `sendMessageStream`'s error contract: transport
 * failures surface as yielded `error` events rather than
 * thrown exceptions, so the caller's `for await` loop can
 * decide what to do without a `try / catch` wrapping the
 * whole body. The loop exits cleanly when the caller aborts
 * or the stream emits a terminal `System::SessionEnded`
 * frame.
 */
export async function* subscribeSession(
  sessionId: string,
  options: SubscribeSessionOptions = {},
): AsyncGenerator<SessionStreamEvent> {
  const { fromStreamIndex, signal, onCursor } = options;
  const params = new URLSearchParams();
  if (typeof fromStreamIndex === 'number' && fromStreamIndex >= 0) {
    params.set('from', String(fromStreamIndex));
  }
  const url = params.toString()
    ? `${STREAM_RESUME_URL(sessionId)}?${params.toString()}`
    : STREAM_RESUME_URL(sessionId);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      credentials: 'include',
      headers: credentialHeader(),
      signal,
    });
  } catch (err) {
    if ((err as { name?: string } | null)?.name === 'AbortError') {
      return;
    }
    yield { type: 'error', error: { code: -1, message: String(err) } };
    return;
  }
  if (!response.ok) {
    let detail = `${response.status} ${response.statusText}`;
    try {
      const body = await response.json();
      if (body && typeof body.message === 'string') detail = body.message;
    } catch {
      // ignore — the status line is enough
    }
    yield {
      type: 'error',
      error: { code: response.status, message: detail },
    };
    return;
  }
  if (!response.body) {
    yield {
      type: 'error',
      error: { code: -1, message: 'stream response had no body' },
    };
    return;
  }
  const reader = response.body.getReader();
  try {
    for await (const { event, data } of parseSseStream(reader)) {
      if (event === 'snapshot') {
        const seq = lastEventSeqOf(data);
        const idx = streamIndexOf(data);
        if (seq !== undefined || idx !== undefined) {
          // `seq` is the sink-internal ordinal, `idx` is the
          // wire stream_index. They are equal today (sink
          // ord == stream pos) but the field is split so
          // future compaction can diverge.
          yield {
            type: 'snapshot',
            snapshot: {
              lastEventSeq: seq ?? idx ?? 0,
              streamIndex: idx ?? seq ?? 0,
            },
          };
        }
        continue;
      }
      if (event === 'cursor') {
        const seq = lastEventSeqOf(data);
        const idx = streamIndexOf(data);
        if (seq !== undefined || idx !== undefined) {
          const effectiveIdx = idx ?? seq ?? 0;
          if (onCursor) onCursor(effectiveIdx);
          yield {
            type: 'cursor',
            cursor: {
              lastEventSeq: seq ?? idx ?? 0,
              streamIndex: effectiveIdx,
            },
          };
        }
        continue;
      }
      // `message` (default) and any other event name — feed
      // through the same `adaptAgentEvent` the live path uses,
      // so silent skip / subagent routing / warning projection
      // all behave identically.
      const adapted = adaptAgentEvent(data);
      if (adapted) yield adapted;
    }
  } catch (err) {
    if ((err as { name?: string } | null)?.name === 'AbortError') return;
    yield { type: 'error', error: { code: -1, message: String(err) } };
  } finally {
    try {
      reader.releaseLock();
    } catch {
      // already released
    }
  }
}
