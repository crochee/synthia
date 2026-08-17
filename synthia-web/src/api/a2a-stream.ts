/**
 * A2A streaming client using @a2a-js/sdk v1.0.
 *
 * Wraps the official SDK Client for the browser. Discovers the agent via
 * `/.well-known/agent-card.json` (fetched through the Vite dev proxy),
 * then issues JSON-RPC requests against the `JSONRPC` interface.
 */

import {
  ClientFactory,
  ClientFactoryOptions,
  DefaultAgentCardResolver,
  JsonRpcTransportFactory,
} from '@a2a-js/sdk/client';
import { Message, StreamResponse, type SendMessageRequest, AGENT_CARD_PATH } from '@a2a-js/sdk';

/**
 * A2A stream event — unified interface for UI consumption.
 *
 * Derived from the JSON-RPC `StreamResponse` wire shape. The
 * SDK exposes `StreamResponse` as a protobuf-style
 * `payload: { $case, value }` oneof; we never read that
 * internal shape — the dispatcher calls
 * `StreamResponse.toJSON(event)` to flatten to the wire shape
 * (a flat object with one of `task` / `message` /
 * `statusUpdate` / `artifactUpdate` set) and then routes on
 * the presence of those keys.
 */
export interface A2AStreamEvent {
  type: 'task' | 'message' | 'statusUpdate' | 'artifactUpdate' | 'error';
  task?: WireTask;
  message?: WireMessage;
  statusUpdate?: {
    taskId: string;
    contextId: string;
    status: {
      state: string;
      message?: WireMessage;
    };
  };
  artifactUpdate?: {
    taskId: string;
    contextId: string;
    artifact: {
      artifactId: string;
      name?: string;
      parts: WirePart[];
      metadata?: Record<string, unknown>;
    };
    append: boolean;
    lastChunk: boolean;
  };
  error?: { code: number; message: string };
}

/**
 * Module-level test fetch override. Production code never sets
 * this; e2e tests do via `_setA2ATestFetch` (and the `_bootstrapTestFetch`
 * helper that reads `window.__synthiaMockFetch`). When `null`,
 * the SDK uses `window.fetch`.
 *
 * The override is stored on `globalThis` (not just a module-local
 * `let`) so that test bootstrap scripts that reach this module
 * via a different `import()` URL — which Vite may resolve to a
 * different module instance under HMR — still observe the
 * override. Module-local `let` is bypassed when the dynamic
 * import lands on a fresh instance; `globalThis` is shared by
 * every module instance of this file in the same page.
 *
 * @internal
 */
function getTestFetchImpl(): typeof fetch | null {
  const g = globalThis as unknown as { __synthiaTestFetchImpl?: typeof fetch | null };
  return typeof g.__synthiaTestFetchImpl === 'function' || g.__synthiaTestFetchImpl === null
    ? (g.__synthiaTestFetchImpl ?? null)
    : null;
}

/**
 * Resolve the fetch impl the SDK should use. Test override if
 * set, otherwise `window.fetch` bound to the page.
 *
 * @internal
 */
function resolveFetchImpl(): typeof fetch {
  const impl = getTestFetchImpl();
  return impl ?? window.fetch.bind(window);
}

/**
 * @internal
 * Test-only hook: replace the `fetch` implementation the A2A
 * SDK uses. Pass `null` to reset to the production default
 * (window.fetch). MUST be called BEFORE the first
 * `sendMessageStream()` invocation: `initA2AClient` caches the
 * SDK client instance at module level, so changing the fetch
 * impl after the client is built has no effect on the cached
 * client. E2E tests invoke this via `_bootstrapTestFetch`.
 */
// Internal helper for `_bootstrapTestFetch` below; `export`
// removed during the 2026-08-15 optimization pass (knip
// flagged as unused export).
function _setA2ATestFetch(fetchImpl: typeof fetch | null): void {
  (
    globalThis as unknown as { __synthiaTestFetchImpl: typeof fetch | null }
  ).__synthiaTestFetchImpl = fetchImpl;
}

/**
 * @internal
 * Test-only bootstrap: read `window.__synthiaMockFetch` (set by
 * the test via `page.evaluate`) and register it as the SDK
 * fetch impl. Tests call this AFTER `page.goto` so that
 * `a2a-stream.ts`'s module has evaluated and `initA2AClient`
 * hasn't run yet for the first time.
 */
export function _bootstrapTestFetch(): void {
  const g = globalThis as unknown as {
    __synthiaMockFetch?: typeof fetch;
  };
  if (typeof g.__synthiaMockFetch === 'function') {
    _setA2ATestFetch(g.__synthiaMockFetch);
  }
}

// Singleton client instance. Stored on `globalThis` so the
// `initA2AClient()` / `getClient()` cache survives across
// module instances that Vite may produce when the same file
// is imported via different URLs (e.g. the production
// `import` and the e2e test's `import('/src/api/a2a-stream.ts')`).
// Production code reads/writes via the same getter/setter
// helpers below; tests call `_resetClientForTesting` to drop
// the cached client.
type Client = Awaited<ReturnType<ClientFactory['createFromUrl']>>;
function getClientInstance(): Client | null {
  const g = globalThis as unknown as { __synthiaClientInstance?: Client | null };
  return g.__synthiaClientInstance ?? null;
}
function setClientInstance(c: Client | null): void {
  (globalThis as unknown as { __synthiaClientInstance: Client | null }).__synthiaClientInstance = c;
}

/**
 * @internal
 * Test-only: drop the cached SDK client so the next
 * `sendMessageStream()` invocation re-runs `initA2AClient`,
 * picking up whatever `testFetchImpl` was set via
 * `_setA2ATestFetch`. Use in tests when `main.tsx`'s eager
 * pre-warm has already cached the client before the test
 * could register its mock fetch. Production code MUST NOT
 * call this.
 */
export function _resetClientForTesting(): void {
  setClientInstance(null);
}

/**
 * Initialize the A2A client.
 *
 * Important: We use `window.location.origin` so the SDK resolves the
 * `AgentCard.supportedInterfaces[].url` relative to the page origin.
 * In dev this routes through Vite's `/a2a` proxy; in prod it goes
 * straight to the same origin as the frontend.
 *
 * The backend's `/.well-known/agent-card.json` handler builds the
 * absolute URL on each request using the Host header, so the SDK
 * always gets a usable `http(s)://host:port/a2a` to fetch against.
 */
async function initA2AClient(): Promise<void> {
  if (getClientInstance()) return;

  const baseUrl = window.location.origin;

  const factory = new ClientFactory(
    ClientFactoryOptions.createFrom(ClientFactoryOptions.default, {
      cardResolver: new DefaultAgentCardResolver({ fetchImpl: resolveFetchImpl() }),
      transports: [new JsonRpcTransportFactory({ fetchImpl: resolveFetchImpl() })],
      preferredTransports: ['JSONRPC'],
    }),
  );

  setClientInstance(await factory.createFromUrl(baseUrl, AGENT_CARD_PATH));
}

/**
 * Get the initialized client, initializing if necessary.
 */
async function getClient(): Promise<Client> {
  if (!getClientInstance()) {
    await initA2AClient();
  }
  const inst = getClientInstance();
  if (!inst) {
    throw new Error('A2A client initialization failed');
  }
  return inst;
}

/**
 * Pre-warm the A2A client. Called from `main.tsx` so the first
 * `sendMessageStream` doesn't have to block on the
 * `/.well-known/agent-card.json` fetch. Safe to call multiple
 * times — the singleton short-circuits on subsequent calls.
 */
export { initA2AClient };

/**
 * Build a v1.0 `SendMessageRequest` from a user text + optional
 * context. We describe the Message in its A2A v1.0 wire shape
 * (`{ role, parts: [{ text }] }`); the SDK's `Message.fromJSON`
 * accepts that shape and returns the in-memory representation
 * the JSON-RPC transport needs.
 */
function buildSendRequest(text: string, sessionId?: string): SendMessageRequest {
  const message = Message.fromJSON({
    messageId: crypto.randomUUID(),
    role: 'ROLE_USER',
    parts: [{ text }],
    contextId: sessionId || '',
  });

  return {
    tenant: '',
    message,
    configuration: undefined,
    metadata: undefined,
  };
}

/**
 * Segment taxonomy for the chat UI's rendering + state machine.
 *
 * This is a frontend-only union — it has no wire-level meaning. The
 * wire itself is A2A v1.0 messages (`Message` with `Part`s); we
 * classify parts by their natural shape (`Part.text` vs `Part.data`
 * with `{id, name, input}` vs `Part.data` with
 * `{tool_use_id, content, is_error}`) and translate into one of these
 * segment types at the receive boundary.
 *
 *   - `text`           — `Part.text` or any text-only payload
 *   - `thinking`       — `Part.data` carrying model reasoning (not
 *     emitted today; reserved for forward-compat with reasoning
 *     events)
 *   - `tool_call`      — `Part.data` shaped like
 *     `{ id, name, input }`
 *   - `tool_result`    — `Part.data` shaped like
 *     `{ tool_use_id, content, is_error }`
 *   - `tool_block`     — the *paired* view of a tool_call +
 *     tool_result cycle used by the chat UI's merge logic; never
 *     arrives directly on the wire
 *   - `progress`       — internal observability signals (e.g.
 *     long-running tool updates); preserved for forward-compat
 *   - `artifact`       — A2A v1.0 `ArtifactUpdate` carrier;
 *     accumulates `TaskPart`s across `append=true` events for
 *     a single `artifactId` within one assistant message
 */
export type SegmentType =
  'text' | 'thinking' | 'tool_call' | 'tool_result' | 'tool_block' | 'progress' | 'artifact';

/**
 * Local shadow of the subset of `Part::data` fields the chat UI
 * needs to render a segment. Populated by
 * [`extractFromMessage`] from the natural-shape `Part.data`
 * payload — there is no synthetic `kind` discriminator.
 */
export interface SegmentMetadata {
  /** `tool_use.id` for tool_call, `tool_use_id` for tool_result. */
  tool_use_id?: string;
  /** `tool_use.name` for tool_call. */
  tool_name?: string;
  /** `tool_result.content` for tool_result (string preview). */
  text?: string;
  /** `tool_use.input` for tool_call (raw JSON tree). */
  input?: unknown;
  /** `tool_result.is_error` — true when the tool runner marked
   *  the result as a failure. Forwarded to the chat reducer so
   *  the renderer can paint the result sub-block red. Only
   *  populated for `tool_result` segments; `undefined` for
   *  everything else. */
  is_error?: boolean;
}

export interface PartWithMetadata {
  type: SegmentType | null;
  text: string;
  metadata?: SegmentMetadata;
}

// Internal helpers ----------------------------------------------------

/**
 * A2A v1.0 wire shape of a `Part`, as defined by the spec and
 * what `Part.fromJSON` accepts / `Part.toJSON` produces.
 *
 *   - `text`  — plain text content (string)
 *   - `data`  — arbitrary JSON value — the only shape we use
 *     for tool calls and results
 *   - `raw`   — base64-encoded binary (file body)
 *   - `url`   — file reference by URL (string)
 *   - `filename`, `mediaType` — optional string fields
 *     attached to the part
 *   - `metadata` — optional arbitrary Part-level metadata
 *
 * The SDK keeps Parts in memory as a protobuf-style
 * `content: { $case, value }` oneof. We never read that
 * internal representation directly — the boundary layer calls
 * `Part.toJSON(p)` so the rest of the frontend works with the
 * flat wire shape and never depends on SDK implementation
 * details.
 */
interface WirePart {
  text?: string;
  data?: unknown;
  raw?: string;
  url?: string;
  filename?: string;
  mediaType?: string;
  metadata?: Record<string, unknown>;
}

/**
 * A2A v1.0 wire shape of a `Message`. Matches the JSON shape
 * `Message.fromJSON` accepts and `Message.toJSON` produces —
 * never the SDK's in-memory `content: { $case, value }`
 * representation.
 */
// Internal wire-format type used only by `parseA2AStream` below;
// `export` removed during the 2026-08-15 optimization pass
// (knip flagged as unused export).
interface WireMessage {
  messageId?: string;
  role?: string;
  parts?: WirePart[];
  contextId?: string;
  taskId?: string;
  metadata?: Record<string, unknown>;
}

/**
 * A2A v1.0 wire shape of a `Task`.
 *
 * `status.state` is a string holding the A2A v1.0 enum name
 * (e.g. `TASK_STATE_WORKING`) — the SDK's `StreamResponse.toJSON`
 * serialises the in-memory `TaskState` enum to its proto name
 * string. The frontend's `TASK_STATE_MIGRATION` table maps
 * these names to CSS-class-friendly slugs.
 */
// Internal wire-format type used only by `parseA2AStream` below;
// `export` removed during the 2026-08-15 optimization pass
// (knip flagged as unused export).
interface WireTask {
  id: string;
  contextId: string;
  status?: {
    state: string;
    message?: WireMessage;
    timestamp?: string;
  };
  artifacts?: Array<{
    artifactId: string;
    name?: string;
    description?: string;
    parts?: WirePart[];
    metadata?: Record<string, unknown>;
  }>;
  history?: WireMessage[];
  metadata?: Record<string, unknown>;
}

/**
 * Flatten a Part (in whatever shape the SDK hands us) to the
 * A2A v1.0 wire shape. The SDK exposes `Part.toJSON` for this
 * purpose; if the SDK's in-memory shape ever changes, the
 * fallback paths in this function still recognise the wire
 * shape directly so dispatch keeps working.
 */
function flattenPart(part: unknown): WirePart {
  if (!part || typeof part !== 'object') return {};
  const p = part as Record<string, unknown>;
  // Wire shape already (e.g. a raw JSON object from a
  // JSON-RPC response that wasn't run through fromJSON).
  // A Part is uniquely identified by one of the four
  // A2A v1.0 content fields (`text` / `data` / `raw` /
  // `url`) — the optional `filename` / `mediaType` /
  // `metadata` fields can also appear without any content
  // field, but those parts are inert and won't classify
  // into a segment type, so detecting content-key
  // presence is enough to recognise the wire shape.
  if ('text' in p || 'data' in p || 'raw' in p || 'url' in p) {
    return p as WirePart;
  }
  // SDK's in-memory shape: { content: { $case, value } }. Use
  // the SDK's toJSON so we don't have to mirror its flattening
  // rules. If toJSON is absent (defensive), fall through.
  const toJSON = (p as { toJSON?: () => unknown }).toJSON;
  if (typeof toJSON === 'function') {
    const out = toJSON.call(p) as WirePart | null | undefined;
    if (out && typeof out === 'object') return out;
  }
  return {};
}

/**
 * Extract the text content from a `WirePart`. The MVP only
 * uses the `{text: "..."}` shape; a `Part::data` whose JSON
 * happens to carry a string `text` field is treated as
 * structured data, not as Part text.
 */
function readPartText(part: WirePart): string {
  return typeof part.text === 'string' ? part.text : '';
}

/**
 * Read the JSON payload of a `Part::data` part. Returns `null`
 * if the part is not a `Part::data` or its value isn't an
 * object — the only shape we care about for tool calls and
 * results.
 */
function readPartData(part: WirePart): Record<string, unknown> | null {
  if (!part.data || typeof part.data !== 'object') return null;
  return part.data as Record<string, unknown>;
}

/**
 * Classify a `Part::data` JSON payload as a tool call, tool
 * result, or neither. Detects the segment kind from the
 * natural JSON keys — no synthetic `kind` discriminator:
 *
 *   - `{ id, name, input }` is a tool_use (`'tool_call'`).
 *   - `{ tool_use_id, content }` is a tool_result (`'tool_result'`).
 *   - Anything else returns `null`; the caller falls through
 *     to text rendering.
 *
 * The same detection is used by both the live A2A stream
 * ([`classifyPart`] below) and the REST history endpoint
 * (`task-to-messages.ts`), so the rules are pinned in one
 * place — if a future LLM provider changes the field names,
 * only this function needs to change.
 */
export function classifyPartPayload(
  data: Record<string, unknown>,
): 'tool_call' | 'tool_result' | null {
  if (typeof data.id === 'string' && typeof data.name === 'string') {
    return 'tool_call';
  }
  if (typeof data.tool_use_id === 'string' && 'content' in data) {
    return 'tool_result';
  }
  return null;
}

/**
 * Classify an A2A Part into a chat-UI segment type by inspecting
 * its natural wire shape. This is the *only* place that decides
 * "this Part is a tool call" or "this Part is a tool result".
 *
 * Rules:
 *   - `Part.text` → `'text'` (the chat UI coalesces text
 *     deltas into the trailing assistant message).
 *   - `Part.data` whose keys match the provider-native ToolUse
 *     shape (`id` + `name` + `input`) → `'tool_call'`. The
 *     presence of these three keys — *not* a synthetic `kind`
 *     discriminator — is the A2A-faithful signal.
 *   - `Part.data` whose keys match the provider-native
 *     ToolResult shape (`tool_use_id` + `content` + `is_error`) →
 *     `'tool_result'`.
 *   - Anything else (`Part.raw`, `Part.url`, malformed data,
 *     heartbeat ping, etc.) → falls through as a no-op; the
 *     caller skips the segment.
 *
 * The result is the SegmentType *and* a structured payload the
 * dispatcher can render directly (without a second pass through
 * the JSON). This collapses what used to be a wire `kind`
 * discriminator + per-shape metadata copy into one read.
 */
// Internal helper for `convertPart` below; `export` removed
// during the 2026-08-15 optimization pass (knip flagged as
// unused export).
function classifyPart(part: unknown): {
  type: SegmentType | null;
  text: string;
  data: Record<string, unknown> | null;
} {
  const wire = flattenPart(part);
  const text = readPartText(wire);
  const data = readPartData(wire);

  if (data) {
    const kind = classifyPartPayload(data);
    if (kind) {
      return { type: kind, text, data };
    }
    // Unknown data shape — drop silently.
    return { type: null, text, data };
  }

  if (text) {
    return { type: 'text', text, data: null };
  }

  return { type: null, text: '', data: null };
}

/**
 * Extract the relevant chat-UI fields from the first significant
 * Part of an A2A Message. The wire is `Message(parts=[Part])`;
 * we walk the parts, classify each via `classifyPart`, and
 * return the first part that yields a non-null segment type.
 *
 * Why "first significant" rather than "first": A2A Messages can
 * carry multiple Parts (e.g. a tool call + the trailing
 * annotation). Today the backend emits one Part per Message,
 * but defensive iteration keeps us safe against future
 * multi-part messages.
 */
export function extractFromMessage(message: {
  parts?: ReadonlyArray<unknown>;
  metadata?: Record<string, unknown>;
}): PartWithMetadata {
  const parts = message.parts;
  if (!parts || parts.length === 0) {
    return { type: null, text: '', metadata: undefined };
  }
  for (const p of parts) {
    const { type, text, data } = classifyPart(p);
    if (type !== null) {
      // Translate the data payload into `SegmentMetadata` so
      // existing callers (ChatPage's dispatchPartPayload) can
      // keep reading `metadata.tool_use_id` / `metadata.input`
      // / etc. without taking a second dependency on
      // `classifyPart`. The translation is a pure rename —
      // the wire shape is preserved.
      const metadata: SegmentMetadata | undefined =
        type === 'tool_call' && data
          ? {
              tool_use_id: typeof data.id === 'string' ? data.id : undefined,
              tool_name: typeof data.name === 'string' ? data.name : undefined,
              input: data.input,
            }
          : type === 'tool_result' && data
            ? {
                tool_use_id: typeof data.tool_use_id === 'string' ? data.tool_use_id : undefined,
                text: typeof data.content === 'string' ? data.content : undefined,
                is_error: data.is_error === true,
              }
            : undefined;
      return { type, text, metadata };
    }
  }
  return { type: null, text: '', metadata: undefined };
}

/**
 * Send a message and yield the resulting stream as `A2AStreamEvent`s.
 *
 * The SDK hands us `StreamResponse` events whose `payload` is a
 * protobuf-style `{ $case, value }` oneof. We never read that
 * internal shape directly — instead we call
 * `StreamResponse.toJSON(event)` to flatten to the wire shape
 * (a flat object with one of `task` / `message` /
 * `statusUpdate` / `artifactUpdate` set) and dispatch on the
 * presence of those keys. This keeps the rest of the frontend
 * entirely in the A2A v1.0 wire shape and free of SDK
 * implementation details.
 */
export async function* sendMessageStream(
  text: string,
  sessionId?: string,
): AsyncGenerator<A2AStreamEvent> {
  const client = await getClient();
  const request = buildSendRequest(text, sessionId);

  try {
    const stream = client.sendMessageStream(request);
    for await (const event of stream) {
      yield* dispatchStreamResponse(event);
    }
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    console.error('[A2A] Stream error:', err);
    yield {
      type: 'error',
      error: { code: -1, message },
    };
  }
}

/**
 * Convert one SDK `StreamResponse` into zero-or-more
 * `A2AStreamEvent`s. Routes on the wire shape (which key is
 * present on the flattened object), not on the SDK's
 * `payload.$case` discriminator.
 */
function* dispatchStreamResponse(event: StreamResponse): Generator<A2AStreamEvent> {
  const wire = StreamResponse.toJSON(event) as Record<string, unknown>;

  if (wire.task) {
    yield { type: 'task', task: wire.task as WireTask };
    return;
  }
  if (wire.message) {
    yield { type: 'message', message: wire.message as WireMessage };
    return;
  }
  if (wire.statusUpdate || wire.status_update) {
    const update = (wire.statusUpdate ?? wire.status_update) as {
      taskId?: string;
      contextId?: string;
      status?: { state: string; message?: WireMessage };
    };
    if (!update.status) return;
    yield {
      type: 'statusUpdate',
      statusUpdate: {
        taskId: update.taskId ?? '',
        contextId: update.contextId ?? '',
        status: {
          state: update.status.state,
          message: update.status.message,
        },
      },
    };
    return;
  }
  if (wire.artifactUpdate || wire.artifact_update) {
    // The MVP backend does not emit ArtifactUpdate for tool
    // calls / results — they're surfaced as `Message(agent)`
    // carrying `Part::data` per A2A v1.0 §3.7. ArtifactUpdate
    // remains reserved for tangible deliverables (e.g. a
    // generated file). We still parse it for forward-compat
    // so a future deliverable that arrives with a Part.text
    // body becomes a `text` segment; anything else is
    // dropped by the classifier.
    const update = (wire.artifactUpdate ?? wire.artifact_update) as {
      taskId?: string;
      contextId?: string;
      artifact?: {
        artifactId: string;
        name?: string;
        parts?: WirePart[];
        metadata?: Record<string, unknown>;
      };
      append?: boolean;
      lastChunk?: boolean;
    };
    if (!update.artifact) return;
    yield {
      type: 'artifactUpdate',
      artifactUpdate: {
        taskId: update.taskId ?? '',
        contextId: update.contextId ?? '',
        artifact: {
          artifactId: update.artifact.artifactId,
          name: update.artifact.name,
          parts: update.artifact.parts ?? [],
          metadata: update.artifact.metadata,
        },
        append: update.append ?? false,
        lastChunk: update.lastChunk ?? false,
      },
    };
    return;
  }
}
