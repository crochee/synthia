/**
 * Chat-message types used by the frontend's chat UI.
 *
 * Lives in `api/` (not inside `pages/ChatPage.tsx`) because the
 * pure reducers in `lib/` need to import these types without
 * pulling in ChatPage's React component graph and CSS imports.
 * Playwright's node test runner cannot load modules that import
 * `.css` files, so any reducer exported from ChatPage.tsx is
 * un-importable from unit tests.
 */

import type { AttachmentPart, SegmentType } from './chat-stream';
import type { SessionPart } from './types';

/**
 * One renderable image carried by a message segment, already
 * shaped for `<img src>`.
 *
 * The chat surface deals in two directions: a user prompt that
 * attached a picture, and a tool result that returned one (a
 * screenshot, a chart, OCR output). Both arrive from the wire as
 * `synthia_provider::ContentPart::Image { data, mime_type }` and
 * are normalised here once, so the renderer never touches base64.
 */
export interface ChatImage {
  /** Stable key for React's list rendering. */
  id: string;
  /** A `data:` URL (base64 payload) or a remote URL. */
  dataUrl: string;
  /** Provider-reported MIME type, used for the `alt` text. */
  mimeType: string;
}

/** True when `data` is already something `<img src>` can use
 *  directly: an inline `data:` URL, a remote `http(s)://` URL, or
 *  a `blob:` handle.
 *
 *  `synthia_provider::ImageContent::data` is polymorphic — the
 *  server writes a base64 payload for a `base64` attachment and the
 *  **remote URL itself** for a `url` attachment
 *  (`synthia-server/src/routes/chat.rs`, `build_parts`). Prefixing
 *  the latter with `data:…;base64,` produces a broken image, so the
 *  two cases must be told apart before rendering.
 */
function isRenderableUrl(data: string): boolean {
  return data.startsWith('data:') || data.startsWith('blob:') || /^https?:\/\//i.test(data);
}

/** Pull the renderable `image` parts out of a
 *  `Vec<ContentPart>`-shaped array.
 *
 *  The content array of a `tool_result` (a screenshot / chart / OCR
 *  tool's reply) is the shape this serves. A part is an image when
 *  its `type` discriminator is `image`; `data` holds either base64
 *  or a remote URL and `mime_type` names the format. Every other
 *  part is ignored — the caller renders its text separately.
 */
export function extractImages(raw: unknown): ChatImage[] {
  if (!Array.isArray(raw)) return [];
  const out: ChatImage[] = [];
  for (const p of raw) {
    if (!p || typeof p !== 'object') continue;
    const part = p as Record<string, unknown>;
    if (part.type !== 'image') continue;
    const data = typeof part.data === 'string' ? part.data : '';
    if (!data) continue;
    const mimeType =
      typeof part.mime_type === 'string' && part.mime_type ? part.mime_type : 'image/png';
    out.push({
      id: crypto.randomUUID(),
      dataUrl: isRenderableUrl(data) ? data : `data:${mimeType};base64,${data}`,
      mimeType,
    });
  }
  return out;
}

/** Convert the composer's pending attachments into renderable images.
 *
 *  The user's own picture is the other half of the multimodal
 *  surface: it reaches the model through `POST /messages`
 *  (`build_parts` in `synthia-server/src/routes/chat.rs`), but the
 *  chat view only ever rendered `segments` — the message's
 *  `attachments` field was dropped at the `ChatMessageViewItem`
 *  boundary, so an image-only send produced an empty `> USER`
 *  bubble. Normalising here lets the user's turn show what they
 *  sent.
 *
 *  Only `image` attachments contribute; a file part is rendered by
 *  `filesFromAttachments` instead.
 *
 *  `AttachmentPart.dataUrl` carries the base64 payload and `url`
 *  the remote one, so `dataUrl` wins when both are present.
 */
export function imagesFromAttachments(attachments: ReadonlyArray<AttachmentPart>): ChatImage[] {
  const out: ChatImage[] = [];
  for (const a of attachments) {
    if (a.kind !== 'image') continue;
    const src = a.dataUrl ?? a.url;
    if (!src) continue;
    out.push({
      id: crypto.randomUUID(),
      dataUrl: isRenderableUrl(src) ? src : `data:${a.mimeType};base64,${src}`,
      mimeType: a.mimeType,
    });
  }
  return out;
}

/**
 * One file the user attached to a turn: everything the bubble needs to
 * show that the turn carried it. The bytes deliberately stay behind —
 * they are already on the wire, and re-hosting them in the transcript
 * would only cost the localStorage quota that
 * `stripAttachmentSegments` exists to protect.
 *
 * Two sources fill this: the composer's own `AttachmentPart` (the send
 * path, via `filesFromAttachments`) and the attachment references a
 * persisted `UserInput` row carries (the rebuild path, via
 * `filesFromPersistedAttachments`). The second has no bytes and no
 * blob URL, so `uri` and `byteLen` are optional and the chip degrades
 * to name plus MIME type.
 */
export interface ChatFile {
  /** Stable key for React's list rendering. */
  id: string;
  filename: string;
  mimeType: string;
  /** Short-form URI, when the wire named one that is not a `data:` URI.
   *  The renderer surfaces it as the chip's tooltip; a `data:` URI is
   *  dropped at the projection (see `filesFromPersistedAttachments`), so
   *  the base64 payload can never reach the DOM or the chat store. */
  uri?: string;
  /** Size the persisted row reported, in bytes. Rendered as a hint in the
   *  tooltip when present; the composer path has no size to report. */
  byteLen?: number;
}

/**
 * Convert the composer's pending attachments into renderable
 * files. The counterpart of `imagesFromAttachments` for the
 * protocol's `document` block: without it a file-only send
 * produced an empty `> USER` bubble, the same defect the image
 * path was fixed for.
 *
 * This is the *send* side — it reads the composer's
 * `AttachmentPart`s. A rebuilt transcript has no `AttachmentPart`s,
 * so `filesFromPersistedAttachments` serves the persisted side.
 */
export function filesFromAttachments(attachments: ReadonlyArray<AttachmentPart>): ChatFile[] {
  const out: ChatFile[] = [];
  for (const a of attachments) {
    if (a.kind !== 'file') continue;
    out.push({
      id: crypto.randomUUID(),
      filename: a.filename ?? 'attachment',
      mimeType: a.mimeType,
    });
  }
  return out;
}

/** First non-empty string value among `keys`, or `''`. */
function firstString(raw: Record<string, unknown>, keys: ReadonlyArray<string>): string {
  for (const key of keys) {
    const value = raw[key];
    if (typeof value === 'string' && value.length > 0) return value;
  }
  return '';
}

/**
 * Matches a `data:` URI. Unlike `isRenderableUrl`'s question (can
 * `<img src>` use this?), this asks the projection's question: is the
 * value the payload itself? Such a URI is never a label and never a
 * tooltip — rendering it would paint the whole base64 body into the
 * transcript and, once persisted, into the chat store.
 */
const DATA_URI_SCHEME = /^data:/i;

/**
 * Longest string still accepted as a chip label. A real filename is a
 * few dozen characters; anything past this is a payload that reached a
 * field named `name` (a writer echoing the URI, say), which is exactly
 * the lie the fallback rule exists to avoid.
 */
const MAX_LABEL_CHARS = 200;

/** Accept `candidate` as a label, or `''` when it cannot be one. */
function usableLabel(candidate: string): string {
  if (candidate.length === 0 || candidate.length > MAX_LABEL_CHARS) return '';
  return DATA_URI_SCHEME.test(candidate) ? '' : candidate;
}

/**
 * Short readable form of a URI: its last path segment, without the query
 * or fragment. Used when the row carried a URI but no name — the
 * tooltip gets the whole URI, the chip gets `report.pdf` rather than
 * `https://host/a/b/report.pdf?token=…`.
 */
function shortUriLabel(uri: string): string {
  const cut = uri.search(/[?#]/);
  const path = cut === -1 ? uri : uri.slice(0, cut);
  const segment = path.slice(path.lastIndexOf('/') + 1);
  return usableLabel(segment || path);
}

/**
 * The chip label for one persisted attachment: its `title` (the
 * protocol's document name, which the send path sets from the composer's
 * filename) or `name`, else the last path segment of a non-`data:` URI,
 * else the MIME type, else `attachment N` — a numbered stand-in that
 * names the position instead of inventing a filename the wire never had.
 * A `data:` URI satisfies none of these branches (it is a payload, not a
 * name), so it falls through to the same `attachment N` label.
 */
function persistedFileName(
  row: Record<string, unknown>,
  uri: string,
  mimeType: string,
  position: number,
): string {
  const named = usableLabel(firstString(row, ['title', 'name', 'filename']));
  if (named) return named;
  if (uri && !DATA_URI_SCHEME.test(uri)) {
    const fromUri = shortUriLabel(uri);
    if (fromUri) return fromUri;
  }
  if (mimeType) return mimeType;
  return `attachment ${position}`;
}

/**
 * Convert the attachment REFERENCES a persisted `UserInput` row carries
 * into renderable file chips — the same `ChatFile` shape (and the same
 * `SegmentFiles` renderer) the live chat bubble uses, so a reloaded turn
 * reads the same way the sent one did.
 *
 * A persisted row holds references only, never bytes: the sink
 * deliberately keeps a turn's base64 out of the JSONL (a multi-megabyte
 * image per turn would bloat the log), so a rebuilt transcript can show
 * *that* a file rode the turn, with its name and type, but cannot offer
 * the bytes or a link to them. The row is written by the controller as
 *
 * ```json
 * { "text": "…", "attachments": [{ "kind": "file", "name": "a.pdf",
 *   "mime_type": "application/pdf", "byte_len": 1024 }] }
 * ```
 *
 * Tolerant by design: a missing, malformed, or non-array `attachments`
 * value yields no chips (never a throw), a missing name falls back
 * through `persistedFileName`, and an entry whose `uri` is a `data:` URI
 * contributes its name and type but no URI.
 */
export function filesFromPersistedAttachments(raw: unknown): ChatFile[] {
  if (!Array.isArray(raw)) return [];
  const out: ChatFile[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) continue;
    const row = entry as Record<string, unknown>;
    const mimeType = firstString(row, ['mime_type', 'mimeType', 'media_type']);
    const uri = firstString(row, ['uri', 'url']);
    const byteLen =
      typeof row.byte_len === 'number' && row.byte_len >= 0 ? row.byte_len : undefined;
    out.push({
      id: crypto.randomUUID(),
      filename: persistedFileName(row, uri, mimeType, out.length + 1),
      mimeType,
      uri: uri && !DATA_URI_SCHEME.test(uri) ? uri : undefined,
      byteLen,
    });
  }
  return out;
}

/** Flatten a `tool_result` `content` payload into display text.
 *
 *  On the wire `ToolResult.content` is `Vec<ContentPart>` — an
 *  array of parts tagged by `type` — while the renderer wants one
 *  string. Text parts are joined; every other kind (image, audio,
 *  resource) contributes nothing here because `extractImages`
 *  renders it separately. A plain-string `content` (older wire
 *  versions, and the shape the UI specs mock) passes through
 *  untouched.
 */
export function contentText(raw: unknown): string {
  if (typeof raw === 'string') return raw;
  if (!Array.isArray(raw)) return '';
  const parts: string[] = [];
  for (const p of raw) {
    if (!p || typeof p !== 'object') continue;
    const part = p as Record<string, unknown>;
    if (typeof part.text !== 'string') continue;
    if (part.type !== undefined && part.type !== 'text') continue;
    parts.push(part.text);
  }
  return parts.join('');
}

/**
 * A single typed unit inside a chat `Message`. The chat UI
 * renders one row per segment.
 */
export interface MessageSegment {
  id: string;
  type: SegmentType;
  content: string;
  toolName?: string;
  /** When type === 'tool_block': the request body of the call
   *  (rendered as a yellow sub-block). */
  callContent?: string;
  /** When type === 'tool_block': the output of the call
   *  (rendered as a green sub-block). */
  resultContent?: string;
  /** Image parts this segment carries, already shaped for
   *  `<img src>`. A tool that returned `[text, image]` keeps its
   *  text in `content` / `resultContent` and its pictures here.
   *  Populated on both the live stream and history replay, so a
   *  resumed session shows the same pictures the model saw. */
  images?: ChatImage[];
  /** Files the user attached to this turn, rendered as chips under
   *  the text. Populated on the sent turn (from the composer's pending
   *  attachments) and on a rebuilt transcript (from the references the
   *  persisted `UserInput` row carries), so the same turn reads the
   *  same way on `/chat/:id` and `/sessions/:id`. */
  files?: ChatFile[];
  /** When type === 'tool_block': the provider-native `tool_use.id`
   *  from the matching `Part::data({id, name, input})`. The chat
   *  reducer uses this to attach a `Part::data({tool_use_id, ...})`
   *  result to the correct block when two blocks are open at once
   *  (parallel or near-parallel tool calls). Falls back to the
   *  trailing-pending heuristic when the wire omits an id. */
  toolUseId?: string;
  /** True while the tool is still executing — the call block
   *  is rendered but the result block is hidden/placeholder. */
  toolPending?: boolean;
  /** When type === 'tool_block': the result was tagged
   *  `is_error: true` by the tool runner (or by the
   *  reconstructed history when seeding the chat from a
   *  session). The renderer paints the result sub-block red so
   *  the user can tell a failing tool from a successful one
   *  at a glance. */
  toolError?: boolean;
  /** When type === 'tool_block' and toolPending === true:
   *  `Date.now()` of the moment the `tool_call` event arrived.
   *  Used to render a timeout indicator once
   *  `Date.now() - pendingSince > TOOL_TIMEOUT_MS` and no
   *  `tool_result` event has been received yet, so the user
   *  can distinguish a still-running tool from a stuck one. */
  pendingSince?: number;
  /** When type === 'tool_block' and toolPending === true: the
   *  turn reached a terminal state (cancel / session end /
   *  error) without ever delivering a result for this call.
   *  The tool did not hang — the run that would have produced
   *  the result stopped. The renderer shows an explicit
   *  "aborted" state and suppresses the timeout warning, which
   *  would otherwise claim the backend may have died. */
  toolAborted?: boolean;
  /** When type === 'attachment': the chat-surface `attachmentId` so
   *  subsequent `append=true` events can find this segment
   *  within the same assistant message and merge into it. */
  attachmentId?: string;
  /** When type === 'attachment': the attachment's optional name
   *  (mirrors the chat-surface artifact name). */
  attachmentName?: string;
  /** When type === 'attachment': the accumulating list of
   *  `SessionPart`s for this attachment. Each `append=true` event
   *  pushes new parts onto this array. */
  attachmentParts?: SessionPart[];
  /** When type === 'attachment': mirrors the wire's
   *  `lastChunk` flag. False while streaming; flipped to true
   *  once `lastChunk=true` arrives, after which any further
   *  updates for this `attachmentId` are dropped. */
  isComplete?: boolean;
}
