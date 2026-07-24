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
import {
  Message,
  type Artifact,
  type Part,
  type SendMessageRequest,
  type Task,
  type TaskArtifactUpdateEvent,
  type TaskState,
  type TaskStatusUpdateEvent,
  type StreamResponse,
  AGENT_CARD_PATH,
} from '@a2a-js/sdk';

/**
 * A2A stream event - unified interface for UI consumption.
 *
 * Mirrors the SDK's `StreamResponse.payload` `$case` discriminator and
 * adds an `error` variant for transport-level failures so callers can
 * surface them uniformly.
 */
export interface A2AStreamEvent {
  type: 'task' | 'message' | 'statusUpdate' | 'artifactUpdate' | 'error';
  task?: Task;
  message?: Message;
  statusUpdate?: {
    taskId: string;
    contextId: string;
    status: {
      state: TaskState;
      message?: Message;
    };
  };
  artifactUpdate?: {
    taskId: string;
    contextId: string;
    artifact: {
      artifactId: string;
      name?: string;
      parts: Part[];
      metadata?: SegmentMetadata;
    };
    append: boolean;
    lastChunk: boolean;
  };
  error?: { code: number; message: string };
}

// Singleton client instance
type Client = Awaited<ReturnType<ClientFactory['createFromUrl']>>;
let clientInstance: Client | null = null;

/**
 * Initialize the A2A streaming client.
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
export async function initA2AStreamClient(): Promise<void> {
  if (clientInstance) return;

  const baseUrl = window.location.origin;

  const factory = new ClientFactory(
    ClientFactoryOptions.createFrom(ClientFactoryOptions.default, {
      cardResolver: new DefaultAgentCardResolver(),
      transports: [new JsonRpcTransportFactory()],
      preferredTransports: ['JSONRPC'],
    }),
  );

  clientInstance = await factory.createFromUrl(baseUrl, AGENT_CARD_PATH);
}

/**
 * Get the initialized client, initializing if necessary.
 */
export async function getClient(): Promise<Client> {
  if (!clientInstance) {
    await initA2AStreamClient();
  }
  if (!clientInstance) {
    throw new Error('A2A client initialization failed');
  }
  return clientInstance;
}

/**
 * Build a v1.0 `SendMessageRequest` from a user text + optional context.
 *
 * Note: The SDK takes a `Message` object with TS oneof `content` parts
 * and the JSON shape `{text: "..."}` for wire-format; the toJSON
 * conversion inside the transport handles the SDK's internal `$case`
 * representation transparently, so we just describe the JSON shape.
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
 * Extract plain text from a list of A2A v1.0 `Part` objects.
 *
 * SDK v1.0 stores the actual content under `part.content.$case`
 * (one of `text` / `raw` / `url` / `data`). Older v0.3.x drafts
 * used `part.kind === 'text'` + `part.text`; that path no longer
 * matches anything produced by the current server, so we look
 * at the v1.0 shape first and fall back to the v0.3 shape for
 * resilience against drift.
 */
export function extractPartText(parts: ReadonlyArray<unknown> | undefined): string {
  if (!parts) return '';
  return parts
    .map((raw) => {
      const p = raw as {
        content?: { $case?: string; value?: unknown };
        kind?: string;
        text?: string;
      };
      if (p.content?.$case === 'text' && typeof p.content.value === 'string') {
        return p.content.value;
      }
      if (p.content?.$case === 'data') {
        const v = p.content.value as { text?: string } | undefined;
        return typeof v?.text === 'string' ? v.text : '';
      }
      if (p.kind === 'text' && typeof p.text === 'string') {
        return p.text;
      }
      return '';
    })
    .join('');
}

// 新增：消息片段类型
//
// The wire-side `kind` values emitted by Phase-5's `Part::data({ kind,
// ...payload })` cover more event kinds than the legacy frontend
// dispatch needs.  We keep the historical `SegmentType` union
// (used by ChatPage.tsx CSS classes and merging logic) and map
// the new wire `kind` values onto it via `wireKindToSegmentType`
// in ChatPage.tsx.  New wire kinds that don't map cleanly
// (model_image / model_audio / model_resource / warning /
// recovery / usage / steering_message / guardian_confirm_* /
// custom / agent_meta) fall through to the `'text'`
// rendering branch until specific UIs are designed for them.
export type SegmentType =
  | 'text'
  | 'text_delta'
  | 'thinking'
  | 'tool_call'
  | 'tool_result'
  | 'tool_block'
  | 'progress'
  | 'response_complete';

export interface SegmentMetadata {
  /** Raw wire `kind` discriminator emitted by the new
   *  `Part::data({ kind, ...payload })` shape.  Existing
   *  rendering branches in ChatPage.tsx dispatch through
   *  `wireKindToSegmentType` before consulting CSS class
   *  suffixes, so unknown kinds degrade to `'text'`. */
  kind?: SegmentType | string;
  /** Common data-part payload fields. */
  tool_use_id?: string;
  tool_name?: string;
  text?: string;
  signature?: string | null;
  input?: unknown;
  /** Warning payload. */
  source?: string;
  message?: string;
  /** Recovery payload. */
  level?: number;
  /** Recovery / warning payload — coerced from `number | null`
   *  at the wire boundary since the frontend renderer treats
   *  null and undefined identically. */
  iteration?: number;
  /** Progress payload. */
  step?: number;
  total?: number;
  /** Hook(SteeringMessage) payload. */
  priority?: number;
  /** Hook(ConfirmResponse) payload. */
  approved?: boolean;
  /** Hook(Custom) payload. */
  custom_kind?: string;
  data?: unknown;
  /** Agent(meta) envelope payload (when the inner event
   *  is carried in a nested Part::data). */
  parent_session_id?: string;
  child_session_id?: string;
  parent_depth?: number;
  // Legacy shape kept for resilience against any residual
  // event that still emits `metadata.segment_type`.
  segment_type?: SegmentType;
}

export interface PartWithMetadata {
  text: string;
  metadata?: SegmentMetadata;
}

// A2A Message interface (matches @a2a-js/sdk structure)
interface A2AMessage {
  parts?: ReadonlyArray<unknown>;
  metadata?: SegmentMetadata;
}

export function extractPartWithMetadata(
  parts: ReadonlyArray<unknown> | undefined,
  messageMetadata?: SegmentMetadata,
): PartWithMetadata {
  if (!parts || parts.length === 0) return { text: '', metadata: messageMetadata };

  const firstPart = parts[0] as {
    content?: { $case?: string; value?: unknown };
    kind?: string;
    text?: string;
    metadata?: SegmentMetadata;
  };

  let text = '';
  let metadata: SegmentMetadata | undefined = messageMetadata;

  if (firstPart.content?.$case === 'data' && firstPart.content.value) {
    // Phase-5 wire shape: `Part::data({ kind, ...payload })`.
    // The payload object is the new metadata: it carries the
    // `kind` discriminator plus the per-event fields.  We treat
    // it as a SegmentMetadata so ChatPage.tsx can stay
    // dispatch-on-`metadata.kind`-only.
    const value = firstPart.content.value as SegmentMetadata & { text?: string };
    if (typeof value.text === 'string') {
      text = value.text;
    }
    // Coerce wire `null` to undefined for fields the frontend
    // renderer treats uniformly (specifically `iteration`).
    if (value.iteration === null) {
      value.iteration = undefined;
    }
    metadata = value;
  } else if (firstPart.content?.$case === 'text' && typeof firstPart.content.value === 'string') {
    // Legacy text-only Part (StatusUpdate and any residual
    // non-data events still use this shape).
    text = firstPart.content.value;
  } else if (firstPart.kind === 'text' && typeof firstPart.text === 'string') {
    // v0.3.x compatibility — kept for resilience against drift.
    text = firstPart.text;
  }

  // Legacy: some older v0.3 drafts placed metadata in
  // `part.metadata`.  Kept as a tertiary fallback so we don't
  // silently drop segments during migration.
  if (firstPart.metadata) {
    metadata = firstPart.metadata;
  }

  // Legacy: a few early events wrapped text + metadata across
  // two parts (parts[0].text + parts[1].metadata).  Kept as a
  // tertiary fallback.
  if (parts.length > 1) {
    const secondPart = parts[1] as {
      metadata?: SegmentMetadata;
    };
    if (secondPart.metadata) {
      metadata = secondPart.metadata;
    }
  }

  return { text, metadata };
}

// 从 Message 对象提取文本和 metadata
export function extractFromMessage(message: A2AMessage): PartWithMetadata {
  return extractPartWithMetadata(message.parts, message.metadata as SegmentMetadata | undefined);
}

/**
 * Send a message and yield the resulting stream as `A2AStreamEvent`s.
 *
 * Uses the SDK's `sendMessageStream` (SendStreamingMessage JSON-RPC)
 * which transparently handles SSE parsing.
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
      const payload = event.payload;
      if (!payload) continue;

      switch (payload.$case) {
        case 'task': {
          const task = payload.value;
          yield { type: 'task', task };
          break;
        }
        case 'message': {
          const message = payload.value;
          yield { type: 'message', message };
          break;
        }
        case 'statusUpdate': {
          const update: TaskStatusUpdateEvent = payload.value;
          if (!update.status) continue;
          yield {
            type: 'statusUpdate',
            statusUpdate: {
              taskId: update.taskId,
              contextId: update.contextId,
              status: {
                state: update.status.state,
                message: update.status.message,
              },
            },
          };
          break;
        }
        case 'artifactUpdate': {
          const update: TaskArtifactUpdateEvent = payload.value;
          const artifact: Artifact | undefined = update.artifact;
          if (!artifact) continue;
          yield {
            type: 'artifactUpdate',
            artifactUpdate: {
              taskId: update.taskId,
              contextId: update.contextId,
              artifact: {
                artifactId: artifact.artifactId,
                name: artifact.name,
                parts: artifact.parts,
                metadata: artifact.metadata as SegmentMetadata | undefined,
              },
              append: update.append,
              lastChunk: update.lastChunk,
            },
          };
          break;
        }
      }
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

export { initA2AStreamClient as initA2AClient };
export type { StreamResponse };
