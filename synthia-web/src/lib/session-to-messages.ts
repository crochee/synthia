/**
 * Conversion helpers between the session-detail wire format
 * (`SessionDetail` / `SessionTurn` / `SessionArtifact`) and
 * the `ChatPage`'s `Message[]` shape persisted under
 * `synthia.messages.<sessionId>`.
 *
 * The chat UI surfaces a conversation as a flat list of
 * `Message` objects, each with a `role` and a list of typed
 * `segments` (text, thinking, tool_block, ...). The session
 * detail endpoint persists the full conversation into
 * `session.history` as session events: user prompts
 * (role=user), agent text deltas, and tool_call / tool_result
 * events with `Part::data` payloads shaped like the provider's
 * `ToolUse` / `ToolResult` structs (`{id, name, input}` /
 * `{tool_use_id, content, is_error}`). The wire carries no
 * synthetic `kind` discriminator — we detect the segment kind
 * from the keys present in each `Part::data` payload.
 *
 * Source priority:
 *   1. `session.history` — primary source of truth. Each
 *      transcript entry maps to one chat message (or a
 *      segment inside an in-flight assistant turn).
 *   2. `session.artifacts` — legacy fallback for sessions
 *      that were completed before history persistence landed.
 *      The artifact `metadata.kind` discriminator here is the
 *      legacy `tool_call` / `tool_result` marker; the wire
 *      shape is identical to the history path so the chat UI
 *      can render both without special-casing.
 *
 * The exported `seedChatFromSession` is the entry point used
 * by `SessionDetailPage` and `SessionsPage` when the user
 * clicks "Continue chat": it merges the reconstructed messages
 * into whatever the local chat store already has, preferring
 * the existing entry (the user may have chatted in this
 * session and we'd rather not duplicate).
 */
import type { SessionArtifact, SessionDetail, SessionTurn } from '../api/types';

export interface ChatMessageLike {
  id: string;
  role: 'user' | 'assistant';
  segments: ChatSegmentLike[];
  sessionId?: string;
  status?: string;
}

// Internal type used only within this module;
// `export` removed during the 2026-08-15 optimization pass
// (knip flagged as unused export).
interface ChatSegmentLike {
  id: string;
  type: 'text' | 'thinking' | 'tool_call' | 'tool_result' | 'tool_block' | 'progress';
  content: string;
  toolName?: string;
  callContent?: string;
  resultContent?: string;
  toolPending?: boolean;
  /** True when the matching tool_result carried
   *  `is_error: true`. The chat-style renderer paints the
   *  result sub-block red so the user can tell a failing
   *  tool from a successful one. */
  toolError?: boolean;
  /** `Part::data.id` from the matching tool_call — the
   *  pairing key that lets a `tool_result` find its own
   *  `tool_block` when two blocks are open at once (parallel
   *  or near-parallel tool calls in the same assistant turn). */
  toolUseId?: string;
}

/** Event type discriminators carried on each durable
 *  `SessionTurn` envelope (see `SessionTurn` in
 *  `src/api/types.ts` and the `event-durability-classification`
 *  spec). New history messages use these tag values; legacy
 *  `Artifact`s used the natural-shape detection the
 *  `chat-stream` module still owns. */
const EVENT_USER_INPUT = 'UserInput';
const EVENT_MODEL = 'Model';

/** Discriminators on the inner `ContentPart` carried by
 *  `Model` events. */
const PART_TEXT = 'text';
const PART_TOOL_USE = 'tool_use';
const PART_TOOL_RESULT = 'tool_result';

/** Legacy discriminator carried on the `metadata` of pre-history
 *  `Artifact`s. Only sessions completed before history
 *  persistence landed still hit this code path; new history
 *  messages use the `Part` discriminators above. */
const LEGACY_ARTIFACT_KIND_TOOL_CALL = 'tool_call';
const LEGACY_ARTIFACT_KIND_TOOL_RESULT = 'tool_result';

/**
 * Flatten the `content` field of a durable `tool_result`
 * event. On the wire `ToolResult.content` is
 * `Vec<ContentPart>` — an array of internally-tagged parts —
 * while the renderer wants a single display string. Text
 * parts are joined; non-text parts contribute nothing. A
 * plain-string `content` (older wire versions) passes
 * through untouched.
 */
function toolResultContentText(raw: unknown): string {
  if (typeof raw === 'string') return raw;
  if (!Array.isArray(raw)) return '';
  const parts: string[] = [];
  for (const p of raw) {
    if (
      p &&
      typeof p === 'object' &&
      typeof (p as Record<string, unknown>).text === 'string' &&
      ((p as Record<string, unknown>).type === undefined ||
        (p as Record<string, unknown>).type === 'text')
    ) {
      parts.push((p as Record<string, unknown>).text as string);
    }
  }
  return parts.join('');
}

/**
 * Detect `Part::text` that the LLM emitted as an echo of a
 * tool event in SSE wire format. Some models occasionally
 * produce a stream-style prefix (`data: `) followed by a
 * JSON payload that mirrors the same shape as a real
 * `Part::data` — but encoded as plain text. Two
 * natural-shape variants appear in practice:
 *
 *   - tool_use echo: `{ "id": "...", "input": {...} }` (often
 *     without `name`, since the LLM is parroting a
 *     less-formatted wire form)
 *   - tool_result echo: `{ "content": "...", "is_error": true }`
 *     (often without `tool_use_id`, for the same reason)
 *
 * The echo is redundant — the same event is already on the
 * wire as a real `Part::data` and will be rendered as a
 * proper tool block, so we suppress the text rendering. This
 * is a presentation rule only; the data is still in
 * `Session.history` for inspection.
 *
 * Heuristic: trim leading whitespace and any single
 * `data: ` SSE prefix, then attempt a `JSON.parse`. The
 * `JSON.parse` is what makes the check robust — it rejects
 * any non-JSON prose, even if it happens to contain the
 * literal strings `"id"` and `"input"`.
 *
 * Exported so both the chat-side reconstruction
 * (`historyToChatMessages`) and the session-detail renderer
 * (`SessionDetailPage`) can apply the same rule.
 *
 * Logging: every key branch emits a `console.debug` line so
 * future investigations can trace exactly why a given
 * `Part::text` was treated as an echo (or not). The text
 * payload is truncated to 200 chars in the log to keep the
 * console readable. Flip via the `echo` namespace — the
 * default is silent; future debug sessions can opt in
 * without code changes.
 */
export function isToolEchoText(text: string): boolean {
  // Early-exit on the common path: the average `Part::text`
  // is plain prose (the assistant's response), not a JSON
  // echo. Logging here would flood the console, so we
  // gate the per-call log on the *negative* branches — the
  // moments when the function actually inspects the text.
  // The two positive branches log "echo detected" with the
  // matched variant.
  const trimmed = text.trim();
  const stripped = trimmed.startsWith('data:')
    ? trimmed.slice('data:'.length).replace(/^\s+/, '')
    : trimmed;
  // Branch 1: the SSE `data:` prefix was stripped. Log the
  // before/after so we can see what the LLM actually
  // produced vs. what we ran JSON.parse against.
  if (trimmed.startsWith('data:')) {
    console.debug('[isToolEchoText] stripped SSE `data:` prefix', {
      before: trimmed.slice(0, 200),
      after: stripped.slice(0, 200),
    });
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(stripped);
  } catch (err) {
    // Branch 2: the input didn't parse as JSON. This is
    // the most common path — the assistant's text is
    // plain prose, not a wire-format echo. Log at debug
    // level so a future investigation can opt in
    // (`console.debug` is off by default in production).
    console.debug('[isToolEchoText] not a tool echo — JSON.parse failed', {
      text: trimmed.slice(0, 200),
      error: err instanceof Error ? err.message : String(err),
    });
    return false;
  }
  // Branch 3: parsed value is not a plain object (null,
  // primitive, array). Reject — neither tool_use nor
  // tool_result is shaped this way on the wire.
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    console.debug('[isToolEchoText] not a tool echo — parsed value is not an object', {
      text: trimmed.slice(0, 200),
      parsedType: Array.isArray(parsed) ? 'array' : typeof parsed,
    });
    return false;
  }
  const obj = parsed as Record<string, unknown>;
  // tool_use echo: { id, input } (with or without name)
  if (typeof obj.id === 'string' && 'input' in obj) {
    // Branch 4: matched a tool_use shape. This is the
    // "duplicate data" case the user observed on the
    // session detail page — log the id so we can correlate
    // with the real `Part::data({id, name, input})` event
    // already on the wire.
    console.debug('[isToolEchoText] ECHO DETECTED — tool_use shape matched', {
      toolUseId: obj.id,
      hasName: typeof obj.name === 'string',
      text: trimmed.slice(0, 200),
    });
    return true;
  }
  // tool_result echo: { content, ... } (with or without
  // tool_use_id and is_error).
  if (typeof obj.content !== 'undefined') {
    // Branch 5: matched a tool_result shape. Same
    // rationale as branch 4 — this is the LLM echoing
    // the tool outcome as text, redundant against the
    // real `Part::data({tool_use_id, content, is_error})`
    // event. Log the tool_use_id if present so we can
    // correlate.
    console.debug('[isToolEchoText] ECHO DETECTED — tool_result shape matched', {
      toolUseId: typeof obj.tool_use_id === 'string' ? obj.tool_use_id : null,
      isError: obj.is_error === true,
      text: trimmed.slice(0, 200),
    });
    return true;
  }
  // Branch 6: parsed as JSON object but neither tool_use
  // nor tool_result shape. Common for arbitrary structured
  // output that the LLM emits — not a duplicate tool turn,
  // render as text.
  console.debug(
    '[isToolEchoText] not a tool echo — JSON object missing both tool_use and tool_result keys',
    {
      keys: Object.keys(obj),
      text: trimmed.slice(0, 200),
    },
  );
  return false;
}

/**
 * Append a text segment to an in-progress assistant turn,
 * coalescing with the most recent text segment when one
 * already exists. Mirrors the append rules used by the live
 * chat stream so the reconstructed transcript renders the same
 * way.
 */
function appendAgentText(messages: ChatMessageLike[], text: string, sessionId: string): void {
  if (!text) return;
  const last = messages[messages.length - 1];
  if (last && last.role === 'assistant') {
    const segments = last.segments;
    const tail = segments[segments.length - 1];
    if (tail && tail.type === 'text') {
      segments[segments.length - 1] = {
        ...tail,
        content: tail.content + text,
      };
      return;
    }
    segments.push({
      id: crypto.randomUUID(),
      type: 'text',
      content: text,
    });
    return;
  }
  messages.push({
    id: crypto.randomUUID(),
    role: 'assistant',
    segments: [{ id: crypto.randomUUID(), type: 'text', content: text }],
    sessionId,
    status: 'completed',
  });
}

/**
 * Append a `tool_call` event as a fresh `tool_block` segment on
 * the trailing assistant turn (or open a new one). The block
 * starts pending; a following `tool_result` event will fill in
 * the result side. If the trailing turn already has a pending
 * tool_block — e.g. a tool_result that arrived before its call
 * — we still open a fresh block rather than corrupt the prior
 * pair's ordering.
 *
 * Carries `Part::data.id` forward as `toolUseId` so the
 * matching `tool_result` can attach to this exact block when
 * the assistant turn has more than one open call at once
 * (the live chat streamer's pairing key — see
 * `ChatPage::findPendingToolBlockIndex`).
 */
function appendToolCall(
  messages: ChatMessageLike[],
  payload: Record<string, unknown>,
  sessionId: string,
): void {
  const toolName = typeof payload.name === 'string' ? payload.name : undefined;
  const input = payload.input;
  const callBody =
    input !== undefined
      ? (() => {
          try {
            return JSON.stringify(input, null, 2);
          } catch {
            return String(input);
          }
        })()
      : '';
  const toolUseId = typeof payload.id === 'string' ? payload.id : undefined;
  const segment: ChatSegmentLike = {
    id: crypto.randomUUID(),
    type: 'tool_block',
    content: '',
    toolName,
    callContent: callBody,
    toolPending: true,
    toolUseId,
  };
  const last = messages[messages.length - 1];
  if (last && last.role === 'assistant') {
    last.segments.push(segment);
    return;
  }
  messages.push({
    id: crypto.randomUUID(),
    role: 'assistant',
    segments: [segment],
    sessionId,
    status: 'completed',
  });
}

/**
 * Append a `tool_result` event. When the trailing assistant
 * turn has a still-pending `tool_block` (i.e. we have an
 * in-flight call), we attach the result content to that block
 * and clear `toolPending`. Otherwise we emit a free-standing
 * `tool_result` segment on the trailing assistant turn (or open
 * a new one) — matches the chat UI's behaviour for tool results
 * whose call was never persisted (e.g. recovered mid-stream).
 *
 * Pairing rule: when the result carries a `tool_use_id`, only
 * blocks whose `toolUseId` matches exactly are considered —
 * the trailing-pending heuristic is reserved for the fallback
 * case (no id on the wire, or id mismatch) so two in-flight
 * tool calls each get their own result.
 */
function appendToolResult(
  messages: ChatMessageLike[],
  payload: Record<string, unknown>,
  sessionId: string,
): void {
  const content = toolResultContentText(payload.content);
  const toolName = typeof payload.tool_name === 'string' ? payload.tool_name : undefined;
  const isError = payload.is_error === true;
  const toolUseId = typeof payload.tool_use_id === 'string' ? payload.tool_use_id : undefined;

  const last = messages[messages.length - 1];
  if (last && last.role === 'assistant') {
    const segments = last.segments;
    let matchIdx = -1;
    let fallback = -1;
    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i];
      if (seg.type !== 'tool_block' || seg.toolPending !== true) continue;
      if (toolUseId !== undefined && seg.toolUseId === toolUseId) {
        matchIdx = i;
        break;
      }
      // Last-resort fallback: the trailing still-pending
      // block. Only used when no exact id match exists —
      // keeps the legacy behaviour for transcripts that
      // don't carry `tool_use_id` (older wire versions).
      fallback = i;
    }
    if (matchIdx === -1) matchIdx = fallback;
    if (matchIdx !== -1) {
      segments[matchIdx] = {
        ...segments[matchIdx],
        resultContent: content,
        toolPending: false,
        toolError: isError,
      };
      return;
    }
    segments.push({
      id: crypto.randomUUID(),
      type: 'tool_result',
      content,
      toolName,
      toolError: isError,
    });
    return;
  }
  messages.push({
    id: crypto.randomUUID(),
    role: 'assistant',
    segments: [
      {
        id: crypto.randomUUID(),
        type: 'tool_result',
        content,
        toolName,
        toolError: isError,
      },
    ],
    sessionId,
    status: 'completed',
  });
}

/**
 * Convert one history entry into chat messages. A `UserInput`
 * envelope produces one user message; `Model` envelopes dispatch
 * on the inner `ContentPart.type` — text appends to the trailing
 * assistant turn (coalesced), `tool_use` / `tool_result` drive
 * the tool-block pairing logic above.
 *
 * Each `Model` envelope carries exactly one `ContentPart` on the
 * wire (the controller writes one event per agent frame), so a
 * single dispatch is enough — no inner part loop is needed.
 */
function applyHistoryEntry(entry: SessionTurn, sessionId: string, out: ChatMessageLike[]): void {
  const envelope = entry.type;
  const data = entry.data ?? {};

  if (envelope === EVENT_USER_INPUT) {
    const text = typeof data.text === 'string' ? data.text : '';
    if (!text) return;
    out.push({
      id: crypto.randomUUID(),
      role: 'user',
      segments: [{ id: crypto.randomUUID(), type: 'text', content: text }],
    });
    return;
  }

  if (envelope !== EVENT_MODEL) return;

  const kind = typeof data.type === 'string' ? data.type : '';
  if (kind === PART_TOOL_USE) {
    appendToolCall(out, data, sessionId);
    return;
  }
  if (kind === PART_TOOL_RESULT) {
    appendToolResult(out, data, sessionId);
    return;
  }
  if (kind === PART_TEXT) {
    const text = typeof data.text === 'string' ? data.text : '';
    if (!text) return;
    // Skip the LLM's text echo of a tool event. The real
    // event is already on the wire as a typed `ContentPart`
    // and will be rendered as a tool block; the text echo
    // would otherwise append the same data to the
    // assistant's text segment, creating the appearance
    // of a duplicate tool turn.
    if (isToolEchoText(text)) return;
    appendAgentText(out, text, sessionId);
  }
}

/**
 * Build the chat messages that best represent a session's
 * `history` array. Each transcript entry becomes either a
 * standalone chat message (user / tool events) or a segment
 * appended to the trailing assistant turn (text deltas
 * coalesce; tool_call opens a `tool_block`; tool_result fills
 * the most recent open block or falls back to a free-standing
 * segment).
 *
 * The output preserves the on-wire ordering of events. The chat
 * UI renders the resulting `segments[]` verbatim.
 */
function historyToChatMessages(
  history: ReadonlyArray<SessionTurn>,
  sessionId: string,
): ChatMessageLike[] {
  const out: ChatMessageLike[] = [];
  for (const entry of history) {
    applyHistoryEntry(entry, sessionId, out);
  }
  return out;
}

/**
 * Group `tool_call` and `tool_result` artifacts by
 * `tool_use_id` and emit one assistant `tool_block` message per
 * pair. This is the legacy fallback path used when a session
 * has no persisted history — pre-`Session.history` runs only
 * had artifacts to reconstruct from. The legacy artifact
 * `metadata.kind` carries the explicit discriminator; new
 * history messages don't need it.
 */
function artifactsToAssistantMessage(
  artifacts: ReadonlyArray<SessionArtifact>,
  sessionId: string,
): ChatMessageLike | null {
  const groups = new Map<
    string,
    {
      toolUseId: string;
      toolName?: string;
      call?: SessionArtifact;
      result?: SessionArtifact;
      isError?: boolean;
    }
  >();
  const loose: SessionArtifact[] = [];

  for (const art of artifacts) {
    const kind = art.metadata?.kind;
    const toolUseId = art.metadata?.tool_use_id;
    if (
      (kind === LEGACY_ARTIFACT_KIND_TOOL_CALL || kind === LEGACY_ARTIFACT_KIND_TOOL_RESULT) &&
      typeof toolUseId === 'string'
    ) {
      let group = groups.get(toolUseId);
      if (!group) {
        group = { toolUseId, toolName: art.metadata?.tool_name };
        groups.set(toolUseId, group);
      }
      if (kind === LEGACY_ARTIFACT_KIND_TOOL_CALL) {
        group.call = art;
        if (art.metadata?.tool_name) group.toolName = art.metadata.tool_name;
      } else {
        group.result = art;
        if (art.metadata?.is_error) group.isError = true;
      }
    } else {
      loose.push(art);
    }
  }

  const segments: ChatSegmentLike[] = [];
  for (const group of groups.values()) {
    const callText = group.call ? extractArtifactText(group.call) : '';
    const resultText = group.result ? extractArtifactText(group.result) : '';
    segments.push({
      id: crypto.randomUUID(),
      type: 'tool_block',
      content: '',
      toolName: group.toolName,
      callContent: callText,
      resultContent: resultText,
      toolPending: false,
      toolError: group.isError,
    });
  }
  for (const art of loose) {
    segments.push({
      id: crypto.randomUUID(),
      type: 'text',
      content: extractArtifactText(art),
    });
  }

  if (segments.length === 0) return null;
  return {
    id: crypto.randomUUID(),
    role: 'assistant',
    segments,
    sessionId,
    status: 'completed',
  };
}

function extractArtifactText(art: SessionArtifact): string {
  if (!art.parts) return '';
  return art.parts.map((p) => (typeof p.text === 'string' ? p.text : '')).join('');
}

/**
 * Build the messages that best represent the session's history
 * for the chat UI. Prefers `session.history`; falls back to
 * `session.artifacts` when the server has not yet persisted
 * history (legacy sessions).
 */
export function reconstructMessagesFromSession(session: SessionDetail): ChatMessageLike[] {
  const out: ChatMessageLike[] = [];
  if (session.history && session.history.length > 0) {
    out.push(...historyToChatMessages(session.history, session.id));
    return out;
  }
  const assistant = artifactsToAssistantMessage(session.artifacts, session.id);
  if (assistant) out.push(assistant);
  return out;
}

/**
 * Merge a list of reconstructed messages into an existing chat
 * store.
 *
 * Dedup rules (in order):
 *   1. If the existing store already contains a user message
 *      with the same text as the first user message in the
 *      reconstructed list, the chat is already synced with
 *      this session — return the existing store unchanged.
 *      This catches the common case where the user has
 *      chatted in this session, then clicks "Continue chat" on
 *      the session detail page; seeding would otherwise add
 *      a duplicate user prompt. The check is "any user
 *      message in the existing list" (not "first one")
 *      because a user may have multiple sessions in the same
 *      thread — only the matching prompt indicates the
 *      session is already in the chat.
 *   2. Otherwise, drop reconstructed messages whose
 *      `sessionId` is already represented in the existing
 *      store. `sessionId` is set on the assistant turn that
 *      produced a streamed agent event; matches mean
 *      "this session's transcript is already in the chat".
 *   3. Reconstructed messages WITHOUT a `sessionId` (i.e.
 *      the user prompt reconstructed from
 *      `Session.history`) are appended — they have no
 *      `sessionId`-keyed overlap risk, and rule 1 already
 *      ruled out the same prompt being present.
 *
 * Returns the merged array. Pure function so callers can apply
 * it and then persist the result.
 */
export function mergeReconstructedMessages(
  existing: ReadonlyArray<ChatMessageLike>,
  reconstructed: ReadonlyArray<ChatMessageLike>,
): ChatMessageLike[] {
  if (reconstructed.length === 0) return existing.slice();

  // (1) Chat already in sync with this session?
  const reconstructedFirstUserText = firstUserPromptText(reconstructed);
  if (reconstructedFirstUserText !== null) {
    if (existing.some((m) => userPromptText(m) === reconstructedFirstUserText)) {
      return existing.slice();
    }
  }

  // (2) Drop reconstructed messages whose sessionId is
  // already in the existing store.
  const existingSessionIds = new Set(
    existing.map((m) => m.sessionId).filter((id): id is string => Boolean(id)),
  );
  const fresh = reconstructed.filter((m) => !m.sessionId || !existingSessionIds.has(m.sessionId));
  if (fresh.length === 0) return existing.slice();
  return [...existing, ...fresh];
}

/**
 * Extract the user-prompt text of a single message. Returns
 * `''` for non-user messages or user messages with no text
 * segments (e.g. an empty `progress` placeholder).
 */
function userPromptText(m: ChatMessageLike): string {
  if (m.role !== 'user') return '';
  return m.segments
    .filter((s) => s.type === 'text')
    .map((s) => s.content)
    .join('');
}

/**
 * Extract the text of the first user message in a list. The
 * chat UI renders user prompts as a single `text` segment on a
 * `user`-role message; everything else (thinking segments,
 * empty segments) is irrelevant for "have we already seen
 * this prompt?" matching.
 *
 * Returns `null` if the list has no user message — caller
 * treats that as "no overlap check possible" and falls through
 * to the sessionId-based dedup.
 */
export function firstUserPromptText(messages: ReadonlyArray<ChatMessageLike>): string | null {
  for (const m of messages) {
    const text = userPromptText(m);
    if (text) return text;
  }
  return null;
}

/**
 * Convenience wrapper for the "Continue chat" click path.
 * Reads the current chat store under
 * `synthia.messages.<sessionId>`, merges in the reconstructed
 * messages, and writes the result back. Safe to call when
 * nothing is stored yet — the store starts as an empty array
 * and is updated in place.
 */
export function seedChatFromSession(
  sessionId: string,
  session: SessionDetail,
  storageKeyPrefix = 'synthia.messages.',
): boolean {
  if (typeof localStorage === 'undefined') return false;
  const key = `${storageKeyPrefix}${sessionId}`;
  const reconstructed = reconstructMessagesFromSession(session);
  if (reconstructed.length === 0) return false;
  let existing: ChatMessageLike[] = [];
  try {
    const raw = localStorage.getItem(key);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) existing = parsed as ChatMessageLike[];
    }
  } catch {
    existing = [];
  }
  const merged = mergeReconstructedMessages(existing, reconstructed);
  if (merged.length === existing.length) return false;
  try {
    localStorage.setItem(key, JSON.stringify(merged));
    return true;
  } catch {
    return false;
  }
}
