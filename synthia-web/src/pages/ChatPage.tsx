import {
  useState,
  useRef,
  useEffect,
  useLayoutEffect,
  useCallback,
  type FormEvent,
  type KeyboardEvent,
  type ChangeEvent,
} from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Button } from '../components/ui/Button';
import { Card } from '../components/ui/Card';
import { ChatMessageList, type ChatMessageViewItem } from '../components/chat/ChatMessageView';
import {
  sendMessageStream,
  extractFromMessage,
  regenerate as apiRegenerate,
  cancelSession as apiCancelSession,
  submitFeedback as apiSubmitFeedback,
  listModels as apiListModels,
  type SessionStreamEvent,
  type SegmentType,
  type SegmentMetadata,
  type AttachmentPart,
  type ModelEntry,
} from '../api/chat-stream';
import type { MessageSegment } from '../api/chat-message';
import type { AgentDetail, List, SessionPart } from '../api/types';
import { appendAttachmentSegment } from '../lib/append-attachment-segment';
import { stripAttachmentSegments } from '../lib/strip-attachment-segments';
import { useServerHealth, setServerHealth } from '../hooks/useServerHealth';
import { useToast } from '../hooks/useToast';
import { api } from '../api/client';
import './ChatPage.css';

/**
 * MIME prefixes the chat composer accepts as multimodal
 * attachments. Image and audio are the two categories the
 * MVP exposes; everything else falls through to the generic
 * `file` attachment type and is sent as `Part.raw`.
 *
 * Keep this list narrow — every entry is a UI affordance
 * (icon, accept-attribute, preview), so the user can tell
 * upfront what the model will receive.
 */
const ATTACHMENT_IMAGE_PREFIXES = ['image/'];
const ATTACHMENT_AUDIO_PREFIXES = ['audio/'];
const ATTACHMENT_FILE_PREFIXES = ['application/pdf', 'text/plain', 'text/markdown'];

/**
 * Classify a `File`'s MIME type into the matching
 * `AttachmentPart` kind. Returns `null` when the MIME type
 * isn't supported — the caller surfaces a toast and skips
 * the file.
 */
function classifyAttachment(mimeType: string): 'image' | 'audio' | 'file' | null {
  if (ATTACHMENT_IMAGE_PREFIXES.some((p) => mimeType.startsWith(p))) return 'image';
  if (ATTACHMENT_AUDIO_PREFIXES.some((p) => mimeType.startsWith(p))) return 'audio';
  if (ATTACHMENT_FILE_PREFIXES.some((p) => mimeType.startsWith(p))) return 'file';
  return null;
}

/** Read a `File` into a base64 `data:` URL. */
function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('FileReader failed'));
    reader.onload = () => {
      if (typeof reader.result === 'string') resolve(reader.result);
      else reject(new Error('FileReader did not return a string'));
    };
    reader.readAsDataURL(file);
  });
}

/**
 * Map session state enum names (SESSION_STATE_*) to the CSS
 * class suffix used by nt-chat__message-status
 * (.status-{suffix}). The canonical status set is closed and
 * surfaced by the `infer_status` helper on the backend; each
 * canonical value maps to a CSS class suffix that already
 * exists on `.nt-chat__message-status`. Inputs that don't
 * match any key fall back to `'unknown'`.
 */
const SESSION_STATE_MIGRATION: Record<string, string> = {
  SESSION_STATE_UNSPECIFIED: 'unspecified',
  SESSION_STATE_SUBMITTED: 'submitted',
  SESSION_STATE_WORKING: 'working',
  SESSION_STATE_COMPLETED: 'completed',
  SESSION_STATE_FAILED: 'failed',
  SESSION_STATE_CANCELED: 'canceled',
  SESSION_STATE_INPUT_REQUIRED: 'input-required',
  SESSION_STATE_REJECTED: 'rejected',
  SESSION_STATE_AUTH_REQUIRED: 'auth-required',
};

function normalizeSessionState(state: string): string {
  if (Object.prototype.hasOwnProperty.call(SESSION_STATE_MIGRATION, state)) {
    return SESSION_STATE_MIGRATION[state];
  }
  console.error('[chat] unknown session state on wire:', state);
  return 'unknown';
}

interface Message {
  id: string;
  role: 'user' | 'assistant';
  segments: MessageSegment[];
  sessionId?: string;
  status?: string;
  /**
   * User-supplied multimodal attachments for this turn.
   * Stored only on `user` turns; `assistant` turns never carry
   * attachments. Persisted to localStorage alongside
   * `segments` so a session restored from a cold load still
   * shows the previews. Re-rendered as inline previews in the
   * message bubble.
   */
  attachments?: AttachmentPart[];
}

const STORAGE_KEY = 'synthia.sessions.v1';

interface SessionMeta {
  id: string;
  title: string;
  createdAt: string;
}

/**
 * Find the index of the last segment in `segments` that can be
 * appended to (i.e. the same simple type — `text` or `thinking`
 * — as `type`). Tool segments always start a new block; only
 * plain text / thinking accumulate.
 */
function findAppendableIndex(segments: MessageSegment[], type: SegmentType): number {
  if (type !== 'text' && type !== 'thinking') return -1;
  // Only merge into the *immediately preceding* same-type segment.
  // Scanning further back lets chunks cross over tool / thinking
  // boundaries and corrupt the rendered order — e.g. a fresh text
  // delta arriving after a tool_block would otherwise merge into
  // a text segment that is now visually above that block.
  if (segments.length === 0) return -1;
  const last = segments[segments.length - 1];
  return last.type === type ? segments.length - 1 : -1;
}

/**
 * Find the closest still-pending `tool_block`, scanning from the
 * tail. Used to attach a `tool_result` to its matching call when
 * other segment types have interleaved between them. Returns -1
 * if no open block exists, in which case the caller falls back to
 * pushing a free-standing `tool_result` segment.
 *
 * When `toolUseId` is provided, only blocks carrying that exact
 * `toolUseId` match. This is the correct pairing key for
 * `Part::data({tool_use_id, content})` results — without it,
 * two in-flight tool calls whose events arrive interleaved
 * would each have their result attached to the most-recently
 * opened (wrong) block.
 *
 * When `toolUseId` is `undefined` (e.g. the wire omitted the id,
 * or a legacy replay doesn't carry one), we fall back to the
 * trailing-pending heuristic so single-call transcripts still
 * render. If `toolUseId` doesn't match any pending block, we
 * still scan for the trailing pending block — keeps the existing
 * behaviour when the wire emits a `tool_use_id` the frontend
 * never saw arrive as a `tool_call` (e.g. an out-of-band retry).
 */
function findPendingToolBlockIndex(segments: MessageSegment[], toolUseId?: string): number {
  // Pass 1: exact id match — scan all pending blocks, prefer the
  // most recently opened one. Scanning the whole list (not just
  // the tail) matters because the matching call may not be the
  // last segment — a later tool_call's block can interleave
  // between the call and its result.
  if (toolUseId !== undefined) {
    let fallback = -1;
    for (let i = 0; i < segments.length; i++) {
      const s = segments[i];
      if (s.type !== 'tool_block' || s.toolPending !== true) continue;
      if (s.toolUseId === toolUseId) return i;
      // Keep the trailing pending block in reserve in case the
      // wire emits a tool_use_id without a matching call (rare,
      // but possible if the server resumes a prior task).
      fallback = i;
    }
    if (fallback !== -1) return fallback;
    return -1;
  }
  // Pass 2: legacy heuristic — attach to the closest still-
  // pending tool_block scanning from the tail. The first
  // tool_block we hit is the answer; if it's already resolved,
  // there's no open block and the caller falls back to a free-
  // standing tool_result segment.
  for (let i = segments.length - 1; i >= 0; i--) {
    const s = segments[i];
    if (s.type === 'tool_block') {
      return s.toolPending === true ? i : -1;
    }
  }
  return -1;
}

/**
 * Main chat page. Sends user messages to the chat backend via
 * `message/stream` and renders incremental assistant segments as
 * SSE events arrive.
 *
 * The wire is `Message` + `Part`s. Classification lives
 * in `api/chat-stream.ts::classifyPartPayload` and dispatches by natural
 * Part shape:
 *   - `Part.text`                         → text segment
 *   - `Part.data { id, name, input }`     → tool_call segment
 *   - `Part.data { tool_use_id, content }`→ tool_result segment
 *   - `Part.data { iteration, ... }`      → thinking segment
 *
 * The chat UI pairs consecutive `tool_call` + `tool_result`
 * segments into a `tool_block` so the user sees one card per
 * tool execution. The backend splits reasoning from text at
 * the source — providers that emit `<think>…</think>` markers
 * inline have those markers extracted by
 * `synthia-provider::streaming::ThinkExtractor` before chunks
 * reach the chat layer, so the frontend never parses marker
 * syntax.
 *
 * No wire-level `kind` discriminator is consulted. Session id
 * is read from the route param and persisted to localStorage.
 */
export function ChatPage() {
  const { sessionId: routeSessionId, agentName: routeAgentName } = useParams<{
    sessionId?: string;
    agentName?: string;
  }>();
  const navigate = useNavigate();

  // Ensure a session exists: if none in URL, create one and
  // redirect to /chat/:sessionId so the URL is shareable.
  //
  // `navigatedRef` is a ref-based one-shot guard so React 18
  // StrictMode's dev-only double-invocation of effects
  // (mount → cleanup → re-mount) doesn't generate *two*
  // `crypto.randomUUID()` calls and end up landing on the
  // second UUID — the URL would still resolve to a single
  // session, but the discarded first UUID would still get
  // registered on the server if any concurrent dispatch had
  // already read it. With the guard, only the first invocation
  // issues a navigate; subsequent invocations (StrictMode's
  // second pass, or a no-op re-run from a same-route re-render)
  // see `navigatedRef.current === true` and bail.
  const navigatedRef = useRef(false);
  useEffect(() => {
    if (routeSessionId) {
      navigatedRef.current = true;
      return;
    }
    if (navigatedRef.current) return;
    navigatedRef.current = true;
    const id = crypto.randomUUID();
    navigate(`/chat/${id}`, { replace: true });
  }, [routeSessionId, navigate]);

  const sessionId = routeSessionId;

  /**
   * URL-based agent selection:
   *
   *   /chat/:sessionId                  → no agent, redirect to the
   *                                       default agent (configured or
   *                                       first registered)
   *   /chat/:sessionId/agent/:agentName → use the named agent
   *                                       verbatim; if the name doesn't
   *                                       match a registered agent, the
   *                                       user gets a clear inline error
   *                                       (no silent fallback — explicit
   *                                       intent is the contract).
   *
   * `navigate` is the only correct side-effect here: it
   * updates the URL bar, makes the choice shareable /
   * bookmarkable, and keeps React Router's history in sync
   * with the user's selection. The agent name also rides
   * along on every `sendMessageStream` call so the backend's
   * chat layer can route the dispatch to the same agent
   * even if the URL and the metadata ever diverge.
   */
  const [agentName, setAgentName] = useState<string | null>(routeAgentName ?? null);
  const [agentError, setAgentError] = useState<string | null>(null);
  useEffect(() => {
    if (!sessionId) return; // wait for the session redirect above
    if (routeAgentName !== undefined) {
      setAgentName(routeAgentName);
      setAgentError(null);
      return;
    }
    // No agent in the URL — resolve the default via the
    // backend and replace the path so the URL stays in sync
    // with reality. A `404` (no agents registered) is treated
    // as a permanent error so the user isn't stuck in a
    // redirect loop on a fresh install.
    let cancelled = false;
    (async () => {
      try {
        const resp = await api.get<{ name: string; source: string }>('/api/v1/agents/default');
        if (cancelled) return;
        setAgentError(null);
        setAgentName(resp.name);
        navigate(`/chat/${sessionId}/agent/${encodeURIComponent(resp.name)}`, {
          replace: true,
        });
      } catch (err) {
        if (cancelled) return;
        setAgentError(err instanceof Error ? err.message : 'No agents registered.');
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [routeAgentName, sessionId, navigate]);

  // Read the initial message list from localStorage synchronously
  // (lazy initializer). The previous design used a useEffect to
  // hydrate `messages`, but a sibling effect that *persists*
  // `messages` to localStorage runs in the same commit before the
  // hydrate effect's `setMessages` can land. The persist effect
  // then clobbered the stored data with the initial empty array,
  // so a fresh load of `/chat/<existing-sessionId>` always showed
  // the "Welcome" card instead of the user's prior conversation.
  // Reading here makes the first render's `messages` reflect the
  // stored value, so the persist effect's first run writes back
  // what we just read instead of `[]`.
  const [messages, setMessages] = useState<Message[]>(() => {
    if (!sessionId) return [];
    try {
      const raw = localStorage.getItem(`synthia.messages.${sessionId}`);
      return raw ? (JSON.parse(raw) as Message[]) : [];
    } catch {
      return [];
    }
  });
  // Lazy initializer so the first paint shows the persisted
  // draft (if any) and avoids the empty → "…" flicker. Per
  // session, scoped via the route param so two open tabs
  // pointing at different sessions don't clobber each other.
  const [input, setInput] = useState<string>(() => {
    if (!sessionId) return '';
    try {
      return localStorage.getItem(`synthia.draft.${sessionId}`) ?? '';
    } catch {
      return '';
    }
  });
  const [isStreaming, setIsStreaming] = useState(false);
  /**
   * Multimodal attachments queued for the next submission.
   * Each entry is one `AttachmentPart`. The list is cleared
   * on successful submit and survives across input edits so
   * the user can attach, type, edit, and send without losing
   * the attachments.
   */
  const [pendingAttachments, setPendingAttachments] = useState<AttachmentPart[]>([]);
  /**
   * Available models + currently-selected model id. The chat
   * UI's model selector surfaces this so the user can swap
   * models mid-session — a per-turn override that mirrors what
   * ChatGPT / Claude / Gemini all offer today. Falls back to
   * an empty list when `/api/v1/models` is unreachable.
   */
  const [models, setModels] = useState<ModelEntry[]>([]);
  const [selectedModel, setSelectedModel] = useState<string | null>(null);
  /**
   * Registered agents (from `GET /api/v1/agents`). When more
   * than one agent exists the composer renders an agent
   * dropdown; a single agent keeps the chip. Empty (fetch
   * failed) falls back to the chip so routing still works.
   */
  const [agents, setAgents] = useState<AgentDetail[]>([]);
  /**
   * Process-wide usage counters, polled every 30s. Surfaced
   * as a chip in the header so the user sees the cumulative
   * token spend without having to dig into a settings page.
   */
  const [usage, setUsage] = useState<{ tokens_in: number; tokens_out: number; turns: number }>({
    tokens_in: 0,
    tokens_out: 0,
    turns: 0,
  });
  /**
   * Stream-level error state. Lives at the page level so the
   * banner survives across remounts of the message list and so
   * we can drive a single Retry button from one place. When the
   * retry succeeds, the banner is cleared and the message
   * continues from the last assistant turn.
   */
  const [streamError, setStreamError] = useState<string | null>(null);
  /** Text of the last user-submitted message, kept so the
   *  Retry button can re-send without forcing the user to
   *  re-type. Cleared on successful submission. */
  const [lastSubmittedText, setLastSubmittedText] = useState<string | null>(null);
  /**
   * Counter incremented each time the user clicks "Retry".
   * Used as a useEffect dependency so the retry submission
   * fires when the value changes (rather than racing the
   * click handler's state updates).
   */
  const [retryNonce, setRetryNonce] = useState(0);
  const isServerAvailable = useServerHealth();
  const { push: pushToast } = useToast();
  const messagesEndRef = useRef<HTMLDivElement>(null);
  // Ref for the message textarea so we can refocus it after
  // the user submits — chat UIs feel sluggish if focus stays
  // on the (now disabled) textarea or wanders to the body.
  const inputRef = useRef<HTMLTextAreaElement>(null);
  // Flag guarding the first persist write after a sessionId
  // change so it doesn't race with the hydrate effect. Set to
  // `true` initially (we don't want the very first commit to
  // write before the lazy initializer has been used for the
  // mount, although that path is safe anyway) and reset to
  // `true` on every sessionId change by the effect below.
  const persistSkipRef = useRef(true);

  /**
   * Tick counter that advances every 5 seconds. Used purely to
   * drive re-renders so the tool-block timeout indicator
   * (`pendingSince + TOOL_TIMEOUT_MS`) flips to red without
   * needing a separate `setTimeout` per pending call. The interval
   * runs continuously so a tool that was abandoned (stream ended
   * with no result) still shows the timeout state.
   */
  const [tick, setTick] = useState(0);
  useEffect(() => {
    // Always run the tick — the timeout indicator needs to flip
    // even after the SSE stream ends with a still-pending tool
    // (e.g. the result was lost). 5s granularity is fine for
    // human-perceivable timeouts.
    const id = setInterval(() => setTick((n) => n + 1), 5_000);
    return () => clearInterval(id);
  }, []);

  // Kick off the markdown chunk on mount so the *first* assistant
  // reply (which almost always contains at least one fenced code
  // block, link, or bullet list) doesn't have to wait for the
  // `react-markdown` + `remark-gfm` + `rehype-highlight` + the
  // highlight.js CSS chunk to download. Without this prefetch, the
  // user sees the `MarkdownSkeleton` placeholder for the first
  // 200-400ms of the assistant's response — long enough to be
  // noticeable on a fast machine, and even longer on a cold cache
  // or a slow network. The promise is fire-and-forget; React's
  // Suspense boundary in `<Markdown>` will pick up the chunk when
  // it's ready and replace the skeleton without a re-render.
  useEffect(() => {
    void Promise.all([
      import('react-markdown'),
      import('remark-gfm'),
      import('rehype-highlight'),
      import('highlight.js/styles/atom-one-light.css'),
    ]);
  }, []);

  // Auto-grow the message textarea based on its scrollHeight.
  // Runs synchronously after DOM mutations so the height
  // adjustment is invisible to the user (no flicker). The CSS
  // clamp (min-height 60px, max-height 240px, overflow-y auto)
  // handles the bounds; this effect just feeds in the natural
  // content height between those bounds.
  useLayoutEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 240)}px`;
  }, [input]);

  // Auto-scroll: on every messages change (new segment
  // landed during streaming, new message submitted, or
  // session hydration finished) snap the messages container
  // to the latest position. Synthia treats the latest
  // assistant message as the live "stage" and the input box
  // below as the fixed footer — we deliberately do not
  // preserve a stale reading position from a user who
  // happened to be scrolled up.
  //
  // The scroll target is the previous sibling of the anchor,
  // i.e. `.nt-chat__messages`. Direct `scrollTop` assignment
  // is required rather than `scrollIntoView` on the anchor:
  // the anchor is a *sibling* of the messages container, not
  // a child of it, so the browser would walk past it to the
  // nearest scrollable ancestor (`.nt-app-main`) and scroll
  // the wrong element. When the messages container is shorter
  // than its content the assignment is a no-op — matching the
  // user's request: "if the scrollbar appears, it should
  // always be at the latest position".
  //
  // The effect re-runs on `messages`, `isStreaming`, and
  // `tick` so the timeout indicator refreshes without a
  // custom event bus.
  useLayoutEffect(() => {
    const list = messagesEndRef.current?.previousElementSibling as HTMLElement | null;
    if (!list || list.scrollHeight <= list.clientHeight) return;
    list.scrollTop = list.scrollHeight;
  }, [messages, isStreaming, tick]);

  // Persist the in-flight draft to localStorage. Debounced so
  // fast typing doesn't thrash disk I/O. The lazy initializer
  // above reads from the same key on mount.
  useEffect(() => {
    if (!sessionId) return;
    const key = `synthia.draft.${sessionId}`;
    if (input === '') {
      // Avoid leaving stale empty drafts around — clear on
      // empty so a future session that lands on the same id
      // doesn't see leftover whitespace.
      try {
        localStorage.removeItem(key);
      } catch {
        // ignore quota / private-mode failures
      }
      return;
    }
    const timer = setTimeout(() => {
      try {
        localStorage.setItem(key, input);
      } catch {
        // localStorage may be unavailable (private mode /
        // quota); drafts are best-effort.
      }
    }, 200);
    return () => clearTimeout(timer);
  }, [input, sessionId]);

  // Clear the draft after a successful submit. The submit
  // handler empties the input, so we just need to drop the
  // persisted copy here. The effect above will then run for
  // the empty string and `removeItem` it.
  useEffect(() => {
    if (input === '' && sessionId) {
      try {
        localStorage.removeItem(`synthia.draft.${sessionId}`);
      } catch {
        // ignore
      }
    }
  }, [input, sessionId]);

  // Toast on server health transitions so the user isn't left
  // guessing why the chat stopped responding. Only fires on
  // the change, not the initial value, so a healthy boot
  // doesn't spam a "back online" toast.
  const lastHealthRef = useRef(isServerAvailable);
  useEffect(() => {
    if (lastHealthRef.current === isServerAvailable) return;
    lastHealthRef.current = isServerAvailable;
    if (isServerAvailable) {
      pushToast({
        variant: 'success',
        message: 'Synthia backend is back online.',
        durationMs: 3000,
      });
    } else {
      pushToast({
        variant: 'warning',
        message: 'Synthia backend unreachable — sending is disabled.',
        durationMs: 5000,
      });
    }
  }, [isServerAvailable, pushToast]);

  /**
   * Append a thinking segment from a `Part.data` payload
   * that we classify as reasoning.
   * Reasoning providers emit a dedicated event with no embedded
   * markers, so each chunk accumulates into the most recent open
   * thinking segment (or opens a new one).
   */
  const appendThinkingFromReasoning = useCallback((assistantId: string, content: string) => {
    if (!content) return;
    setMessages((prev) =>
      prev.map((m) => {
        if (m.id !== assistantId) return m;
        const segments = [...m.segments];
        const idx = findAppendableIndex(segments, 'thinking');
        if (idx >= 0) {
          const existing = segments[idx];
          segments[idx] = {
            ...existing,
            content: existing.content + content,
          };
        } else {
          segments.push({
            id: crypto.randomUUID(),
            type: 'thinking',
            content,
          });
        }
        return { ...m, segments };
      }),
    );
  }, []);

  /**
   * Append a plain-text segment from a `Part.text` payload.
   * Same rationale as `appendThinkingFromReasoning`: the
   * backend has already split reasoning from text, so each
   * chunk just appends to the most recent open text segment
   * (or opens a new one).
   */
  const appendText = useCallback((assistantId: string, content: string) => {
    if (!content) return;
    setMessages((prev) =>
      prev.map((m) => {
        if (m.id !== assistantId) return m;
        const segments = [...m.segments];
        const idx = findAppendableIndex(segments, 'text');
        if (idx >= 0) {
          const existing = segments[idx];
          segments[idx] = {
            ...existing,
            content: existing.content + content,
          };
        } else {
          segments.push({
            id: crypto.randomUUID(),
            type: 'text',
            content,
          });
        }
        return { ...m, segments };
      }),
    );
  }, []);

  /**
   * Append a `tool_call` (opens a new `tool_block`) or a
   * `tool_result` (attaches to the most recent open
   * `tool_block`). tool_blocks render as a single collapsible
   * header containing two sub-blocks (yellow call body + green
   * result body).
   */
  const appendToolSegment = useCallback(
    (
      assistantId: string,
      seg: Pick<MessageSegment, 'type' | 'content' | 'toolName'> & {
        toolUseId?: string;
        isError?: boolean;
      },
    ) => {
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id !== assistantId) return m;
          const segments = [...m.segments];

          if (seg.type === 'tool_call') {
            const block: MessageSegment = {
              id: crypto.randomUUID(),
              type: 'tool_block',
              content: '',
              toolName: seg.toolName,
              callContent: seg.content,
              toolPending: true,
              // Stamp the moment the call arrived so the renderer
              // can flip to a timeout indicator after
              // TOOL_TIMEOUT_MS of silence. Cleared implicitly
              // when `toolPending` flips to false below.
              pendingSince: Date.now(),
              // Carry the provider-native `tool_use.id` so a
              // later `tool_result` carrying `tool_use_id` can
              // be matched to *this* block instead of whichever
              // tool_block happens to be trailing. Without this
              // tag, two in-flight tool calls would each get
              // their result attached to the wrong block.
              toolUseId: seg.toolUseId,
            };
            segments.push(block);
            return { ...m, segments };
          }

          if (seg.type === 'tool_result') {
            // Find the closest still-pending tool_block, scanning
            // backwards. The block we want may not be the last
            // segment — other segment types (e.g. a follow-up
            // `thinking` chunk) can interleave between the call
            // and its result. Only fall back to a free-standing
            // `tool_result` segment when there is no open block.
            //
            // When the result carries a `tool_use_id`, use it as
            // the pairing key so a `tool_result` for tool A
            // can't accidentally attach to tool B's still-open
            // block.
            const pendingIdx = findPendingToolBlockIndex(segments, seg.toolUseId);
            if (pendingIdx >= 0) {
              segments[pendingIdx] = {
                ...segments[pendingIdx],
                resultContent: seg.content,
                toolPending: false,
                toolError: seg.isError === true,
              };
            } else {
              segments.push({
                id: crypto.randomUUID(),
                type: 'tool_result',
                content: seg.content,
                toolName: seg.toolName,
                toolError: seg.isError === true,
              });
            }
            return { ...m, segments };
          }

          return m;
        }),
      );
    },
    [],
  );

  // Persist session metadata
  useEffect(() => {
    if (!sessionId) return;
    const raw = localStorage.getItem(STORAGE_KEY);
    const sessions: SessionMeta[] = raw ? JSON.parse(raw) : [];
    if (!sessions.find((s) => s.id === sessionId)) {
      const meta: SessionMeta = {
        id: sessionId,
        title: `Session ${sessions.length + 1}`,
        createdAt: new Date().toISOString(),
      };
      sessions.push(meta);
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sessions));
    }
  }, [sessionId]);

  // Re-hydrate messages when the route session changes. The
  // initial value comes from the useState lazy initializer above
  // (covers the mount case); this effect handles the in-app
  // navigation case where the user goes from /chat/A to /chat/B
  // without a full reload and React keeps the same ChatPage
  // instance. It runs *after* the persist effect on the same
  // commit — but unlike the previous design, the persist effect
  // now writes the freshly-read messages back rather than an
  // empty array, so there is no clobber.
  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    try {
      const raw = localStorage.getItem(`synthia.messages.${sessionId}`);
      if (!cancelled) {
        const parsed = raw ? (JSON.parse(raw) as Message[]) : [];
        setMessages(stripAttachmentSegments(parsed));
      }
    } catch {
      if (!cancelled) setMessages([]);
    }
    // Reset the persist-skip guard so the very next persist run
    // (which will see the freshly-hydrated messages in state) is
    // allowed to write. This must run *before* the persist effect
    // on the same commit, so we do it synchronously here.
    persistSkipRef.current = true;
    // Initial focus on the input — chat UIs land here first,
    // and a manual click is friction. Focus even on session
    // restore: the user is choosing to open this view to type.
    inputRef.current?.focus();
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  // Persist messages whenever they change. The first commit after
  // a `sessionId` change must be skipped so the hydrate effect
  // (which reads from localStorage and calls `setMessages`) has a
  // chance to land. Otherwise we would write the *previous*
  // session's messages into the new session's storage key on the
  // same commit the hydrate effect is about to overwrite, racing
  // and corrupting state. The skip ref is reset on every
  // `sessionId` change by a sibling effect (above).
  //
  // Debounced 300ms — a long tool call can produce dozens of
  // segments in a few seconds, and serialising the whole array
  // per event is wasted I/O. 300ms is short enough to feel
  // instant on refresh, long enough to coalesce bursts.
  //
  // `attachments` are NOT persisted — the base64 payloads
  // would bloat localStorage past the ~5 MB quota on a single
  // large image, and a cold reload loses the original `File`
  // anyway. The next message starts fresh; past turns are
  // reconstructed from the assistant's replies alone.
  useEffect(() => {
    if (!sessionId) return;
    if (persistSkipRef.current) {
      persistSkipRef.current = false;
      return;
    }
    const timer = setTimeout(() => {
      try {
        const stripped = messages.map((m) => ({ ...m, attachments: undefined }));
        localStorage.setItem(
          `synthia.messages.${sessionId}`,
          JSON.stringify(stripAttachmentSegments(stripped)),
        );
      } catch {
        // Quota errors on long conversations are non-fatal —
        // the in-memory state still serves the active session.
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [sessionId, messages]);

  const handleSubmit = async (e?: FormEvent) => {
    e?.preventDefault();
    const text = input.trim();
    // Multimodal turns can be sent with zero typed text (e.g.
    // "describe this image") — the attachments carry the
    // content. A purely-attachment submission is still valid;
    // a purely-empty submission is rejected (no signal at
    // all would just trip the model's no-op handler).
    const hasAttachments = pendingAttachments.length > 0;
    if ((!text && !hasAttachments) || isStreaming || !sessionId || !isServerAvailable) return;
    if (agentName === null && agentError === null) return; // still resolving default

    const userMessage: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      segments: [{ id: crypto.randomUUID(), type: 'text', content: text }],
      attachments: hasAttachments ? pendingAttachments : undefined,
    };
    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    // Snapshot the attachments before clearing so we can
    // hand them to the stream below — `pendingAttachments`
    // is cleared immediately so the next message starts
    // clean.
    const attachmentsSnapshot = hasAttachments ? [...pendingAttachments] : undefined;
    setPendingAttachments([]);
    setLastSubmittedText(text);
    setIsStreaming(true);
    setStreamError(null);

    // Restore focus to the input right after submit so the
    // user can immediately type the next message without
    // clicking back into the textarea.
    inputRef.current?.focus();

    const assistantId = crypto.randomUUID();
    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: 'assistant', segments: [], status: 'working' },
    ]);

    // Mirror the URL's `:agentName` segment onto the wire so
    // the chat layer can route the dispatch to the same
    // agent even if the URL and the metadata ever diverge
    // (e.g. a custom client invoking the same backend
    // without a URL). `agentName` is `null` while the redirect
    // is in flight — early-out above guarantees we don't
    // reach this line in that state.
    const metadata = agentName ? { 'synthia.agent_name': agentName } : undefined;

    try {
      for await (const event of sendMessageStream(text, {
        sessionId,
        attachments: attachmentsSnapshot,
        metadata,
        model: selectedModel ?? undefined,
      })) {
        applyStreamEvent(assistantId, event);
      }
      // Successful end of stream — drop the cached submission
      // text so a future error doesn't silently re-send the
      // last good message via Retry.
      setLastSubmittedText(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setStreamError(message);
      // Network-level failures (TypeError "Failed to fetch",
      // DNS errors, etc.) on a chat round trip prove the server
      // is unreachable right now — flip the global health flag
      // so the rest of the UI doesn't wait for the next 30s
      // tick before reacting.
      setServerHealth(false);
      const errorSegment: MessageSegment = {
        id: crypto.randomUUID(),
        type: 'text',
        content: `\n\n[error: ${message}]`,
      };
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId
            ? { ...m, segments: [...m.segments, errorSegment], status: 'failed' }
            : m,
        ),
      );
    } finally {
      setIsStreaming(false);
      // Re-focus the textarea once streaming completes. The
      // textarea is disabled during streaming so the user
      // can't type while the model is generating, but as soon
      // as the response lands they should be ready to send
      // the next message without an extra click.
      inputRef.current?.focus();
    }
  };

  /**
   * Retry the last failed submission. Triggered by the
   * "Retry" button on the stream error banner — uses the same
   * code path as a fresh submit (it appends a new assistant
   * turn rather than rewinding the failed one), so the user
   * sees an explicit "retry" marker in the transcript.
   *
   * The retry is driven by `retryNonce` so the click handler
   * can just bump the counter; the actual send logic lives in
   * the effect below.
   */
  const handleRetry = useCallback(() => {
    if (!lastSubmittedText || isStreaming) return;
    setRetryNonce((n) => n + 1);
  }, [lastSubmittedText, isStreaming]);

  /**
   * Effect that turns a retry click into a submission. We
   * use a nonce so the click handler doesn't need to talk to
   * `applyStreamEvent` directly (which is a closure over the
   * current render's state and would race otherwise).
   */
  useEffect(() => {
    if (retryNonce === 0) return; // skip the initial mount
    if (!lastSubmittedText || !sessionId || !isServerAvailable) return;
    // Mirror the submit path: append a new user message,
    // append a new assistant placeholder, then re-issue the
    // stream. The previous failed assistant turn is left in
    // place so the user can see what didn't work.
    const userMessage: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      segments: [{ id: crypto.randomUUID(), type: 'text', content: lastSubmittedText }],
    };
    setMessages((prev) => [...prev, userMessage]);
    setStreamError(null);
    setIsStreaming(true);
    inputRef.current?.focus();

    const assistantId = crypto.randomUUID();
    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: 'assistant', segments: [], status: 'working' },
    ]);

    // Track an in-flight flag so that if the user navigates
    // away (or React unmounts the page during the retry), the
    // pending `setState` calls land on a dead tree and React
    // surfaces noisy "Can't perform state update on unmounted
    // component" warnings. The stream itself can't be aborted
    // (SDK limitation), but the consumer-side mutation does not
    // need to run after unmount.
    let cancelled = false;
    (async () => {
      try {
        for await (const event of sendMessageStream(lastSubmittedText, {
          sessionId,
          metadata: agentName ? { 'synthia.agent_name': agentName } : undefined,
        })) {
          if (cancelled) return;
          applyStreamEvent(assistantId, event);
        }
        if (!cancelled) setLastSubmittedText(null);
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        setStreamError(message);
        // Network-level failures (TypeError "Failed to fetch",
        // DNS errors, etc.) on a chat round trip prove the server
        // is unreachable right now — flip the global health flag
        // so the rest of the UI doesn't wait for the next 30s
        // tick before reacting.
        setServerHealth(false);
        const errorSegment: MessageSegment = {
          id: crypto.randomUUID(),
          type: 'text',
          content: `\n\n[retry error: ${message}]`,
        };
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? { ...m, segments: [...m.segments, errorSegment], status: 'failed' }
              : m,
          ),
        );
      } finally {
        if (!cancelled) {
          setIsStreaming(false);
          inputRef.current?.focus();
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [retryNonce]); // eslint-disable-line react-hooks/exhaustive-deps
  // Intentionally narrow deps: only fire when the nonce bumps.
  // `applyStreamEvent` and `lastSubmittedText` are read at
  // effect time and intentionally NOT re-subscribed, to avoid
  // a stale-effect loop.

  /**
   * Fetch the model catalog from `/api/v1/models` once on mount.
   * If the request fails (server down, version too old) we
   * silently fall back to an empty list and the composer
   * hides its model selector. The user can keep chatting
   * through the default model — a feature, not a regression.
   */
  useEffect(() => {
    if (!isServerAvailable) return;
    let cancelled = false;
    apiListModels()
      .then((resp) => {
        if (cancelled) return;
        setModels(resp.models);
        // Seed the selector with the workspace default so the
        // first send picks up the same model the server would
        // otherwise resolve internally.
        const seed = `${resp.default_provider}/${resp.default_model}`;
        setSelectedModel(seed);
      })
      .catch(() => {
        // Network failure — leave the selector hidden.
      });
    return () => {
      cancelled = true;
    };
  }, [isServerAvailable]);

  // Fetch the registered agent list once on mount so the
  // composer can render a dropdown when the server has more
  // than one agent. On failure we keep the chip fallback.
  useEffect(() => {
    if (!isServerAvailable) return;
    let cancelled = false;
    api
      .get<List<AgentDetail>>('/api/v1/agents')
      .then((resp) => {
        if (cancelled) return;
        setAgents(resp.data);
      })
      .catch(() => {
        if (cancelled) return;
        setAgents([]);
      });
    return () => {
      cancelled = true;
    };
  }, [isServerAvailable]);

  /**
   * Poll `/api/v1/chat/usage` every 30s. Cheap endpoint
   * (returns three integers) so we don't bother with an
   * explicit connection. Interval is cleared on unmount.
   */
  useEffect(() => {
    if (!isServerAvailable) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const resp = await import('../api/chat-stream').then((m) => m.getUsage());
        if (!cancelled) {
          setUsage({ tokens_in: resp.tokens_in, tokens_out: resp.tokens_out, turns: resp.turns });
        }
      } catch {
        // Ignore — the chip will just keep showing the prior
        // value until the next tick succeeds.
      }
    };
    tick();
    const id = window.setInterval(tick, 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [isServerAvailable]);

  /**
   * Dispatch a single (segment type, text, metadata) payload to
   * the right segment-append helper. The segment type is the
   * natural-shape classification already done by
   * `extractFromMessage` — no wire-level `kind` discriminator
   * is consulted (`Part.text` is text,
   * `Part.data` carries tool calls / results via the
   * provider-native shape).
   *
   * Returns `true` if the payload was consumed and the message
   * state was updated, `false` if the payload was a no-op
   * (empty text / unknown shape) so the caller can decide
   * whether to also update the session status.
   */
  const dispatchPartPayload = useCallback(
    (
      assistantId: string,
      segmentType: SegmentType | null,
      text: string,
      metadata: SegmentMetadata | undefined,
    ): boolean => {
      if (segmentType === null) return false;
      // tool_call and tool_result carry their content on
      // the metadata side (`metadata.input` for tool_call,
      // `metadata.text` for tool_result), so the `text`
      // parameter is the empty string for both — that's
      // the wire-faithful shape and not a no-op signal.
      // Don't drop these payloads on the empty-text guard.
      if (!text && segmentType !== 'tool_call' && segmentType !== 'tool_result') {
        return false;
      }

      if (segmentType === 'tool_call' || segmentType === 'tool_result') {
        // Wire payload carries arguments in `input` (not `text`)
        // for `tool_call`, so stringify the JSON so the call
        // block renders the same way as the result block.
        // For `tool_result`, the result body lives on
        // `metadata.text` (also empty in the `text` param).
        // Fall back to `text` when metadata is missing so
        // the segment is never silently empty.
        const callBody =
          segmentType === 'tool_call' && metadata?.input !== undefined
            ? JSON.stringify(metadata.input, null, 2)
            : (metadata?.text ?? text);
        appendToolSegment(assistantId, {
          type: segmentType,
          content: callBody,
          toolName: metadata?.tool_name,
          // Forward the pairing id + error flag from the wire
          // so the reducer can attach tool_results to the
          // correct tool_block when two are in-flight at once.
          // Both fields live on `Part::data`:
          //   tool_call:   { id, name, input }
          //   tool_result: { tool_use_id, content, is_error }
          toolUseId: metadata?.tool_use_id,
          isError: metadata?.is_error,
        });
        return true;
      }

      if (segmentType === 'thinking') {
        appendThinkingFromReasoning(assistantId, text);
        return true;
      }

      if (segmentType === 'text') {
        appendText(assistantId, text);
        return true;
      }

      // `progress` and any future kind that survives the
      // classifier — render each event as its own
      // non-accumulating segment.
      const newSegment: MessageSegment = {
        id: crypto.randomUUID(),
        type: segmentType,
        content: text,
        toolName: metadata?.tool_name,
      };
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId ? { ...m, segments: [...m.segments, newSegment] } : m,
        ),
      );
      return true;
    },
    [appendText, appendThinkingFromReasoning, appendToolSegment],
  );

  const applyStreamEvent = (assistantId: string, event: SessionStreamEvent) => {
    if (event.type === 'error') {
      const errorSegment: MessageSegment = {
        id: crypto.randomUUID(),
        type: 'text',
        content: `\n[error ${event.error!.code}: ${event.error!.message}]`,
      };
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId
            ? { ...m, segments: [...m.segments, errorSegment], status: 'failed' }
            : m,
        ),
      );
      return;
    }

    switch (event.type) {
      case 'turnStatus': {
        if (!event.turnStatus) return;
        // `event.turnStatus.status.state` is the
        // wire enum name (e.g. `SESSION_STATE_WORKING`) — it
        // comes through `StreamResponse.toJSON` already in
        // wire form, so we feed it straight to
        // `normalizeSessionState` (no serialization round
        // trip — that helper expects the SDK's in-memory
        // numeric enum and would return `UNRECOGNIZED`).
        const state = normalizeSessionState(event.turnStatus.status.state);
        const statusMsg = event.turnStatus.status.message;

        // Update the message's task state regardless of whether
        // the status carries a part payload — the user wants to
        // see the working → completed transition even when no
        // new text arrives.
        const setState = () =>
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantId ? { ...m, status: state } : m)),
          );

        if (statusMsg) {
          const extracted = extractFromMessage(statusMsg);
          if (extracted) {
            dispatchPartPayload(assistantId, extracted.type, extracted.text, extracted.metadata);
          }
          setState();
        } else {
          setState();
        }
        break;
      }

      case 'message': {
        if (!event.message) return;
        const extracted = extractFromMessage(event.message);
        if (extracted) {
          dispatchPartPayload(assistantId, extracted.type, extracted.text, extracted.metadata);
        }
        break;
      }

      case 'attachment': {
        // The `attachment` event is reserved for tangible
        // session deliverables (e.g. a generated file). The MVP
        // backend doesn't emit any, but we still parse it for
        // forward-compat — see `chat-stream.ts::sendMessageStream`.
        //
        // The reducer is the only place that decides what to do
        // with the append/lastChunk protocol. Strict mode drops
        // malformed events (see `appendAttachmentSegment`'s doc).
        if (!event.attachment) return;
        const att = event.attachment;
        const attachmentId = att.attachment?.attachmentId;
        console.debug('[applyStreamEvent attachment]', {
          attachmentId,
          append: att.append,
          lastChunk: att.lastChunk,
          partCount: Array.isArray(att.attachment?.parts) ? att.attachment.parts.length : 0,
        });
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? {
                  ...m,
                  segments: appendAttachmentSegment(m.segments, {
                    sessionId: att.sessionId ?? '',
                    contextId: att.contextId ?? '',
                    attachment: {
                      attachmentId: attachmentId ?? '',
                      name: att.attachment?.name,
                      parts: (att.attachment?.parts ?? []) as ReadonlyArray<SessionPart>,
                      metadata: att.attachment?.metadata,
                    },
                    append: att.append,
                    lastChunk: att.lastChunk,
                  }),
                }
              : m,
          ),
        );
        break;
      }

      case 'sessionStatus': {
        if (!event.session) return;
        // `event.session.status?.state` is the session
        // status enum name string; pass it directly to
        // `normalizeSessionState`.
        const sessionState = event.session.status?.state;
        const state = sessionState ? normalizeSessionState(sessionState) : 'unknown';
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, status: state } : m)),
        );
        break;
      }
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  // Wall-clock value refreshed by the tick effect. Read at render
  // time and passed to each SegmentView so a tool-block's
  // `pendingSince` can be compared without per-segment setTimeout
  // chains. `tick` is unused but referenced to keep the closure
  // honest (otherwise the value never advances after the first
  // render).
  void tick;
  const now = Date.now();

  // Project the chat-side `Message` shape onto the shared
  // ChatMessageView shape so the same renderer powers both
  // the live `/chat/:sessionId` page and the
  // `/sessions/:id` reconstructed history view. `isStreaming`
  // propagates only for the active assistant turn so the
  // blinking-cursor affordance lights up while the model
  // is generating.
  const viewMessages: ChatMessageViewItem[] = messages.map((msg) => ({
    id: msg.id,
    role: msg.role,
    segments: msg.segments,
    status: msg.status,
    isStreaming: isStreaming && msg.role === 'assistant' && msg.status === 'working',
  }));

  // Identify the most recent user/assistant turn pair so we
  // can wire up the "Regenerate" affordance against the
  // trailing assistant message. Per ChatGPT/Claude/Gemini
  // convention, regenerate is only available when the last
  // turn has finished (completed / failed / canceled) and is
  // not currently streaming.
  const lastAssistant = (() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      if (messages[i].role === 'assistant') return messages[i];
    }
    return null;
  })();
  const canRegenerate =
    !isStreaming &&
    lastAssistant !== null &&
    lastAssistant !== undefined &&
    lastAssistant.status !== 'working';

  const handleRegenerate = useCallback(async () => {
    if (!sessionId || isStreaming) return;
    try {
      await apiRegenerate(sessionId);
    } catch (err) {
      pushToast({
        variant: 'error',
        message: `Regenerate failed: ${err instanceof Error ? err.message : String(err)}`,
        durationMs: 5000,
      });
    }
  }, [sessionId, isStreaming, pushToast]);

  const handleFeedback = useCallback(
    async (messageId: string, thumbsUp: boolean) => {
      try {
        await apiSubmitFeedback(messageId, thumbsUp);
        pushToast({
          variant: 'success',
          message: thumbsUp ? 'Thanks for the feedback 👍' : 'Thanks for the feedback 👎',
          durationMs: 2000,
        });
      } catch (err) {
        pushToast({
          variant: 'error',
          message: `Feedback failed: ${err instanceof Error ? err.message : String(err)}`,
          durationMs: 5000,
        });
      }
    },
    [pushToast],
  );

  return (
    <div className="nt-chat">
      <div className="nt-chat__usage-chip" data-testid="usage-chip">
        <span aria-hidden>🧮</span>
        <span>
          {usage.tokens_in.toLocaleString()} in · {usage.tokens_out.toLocaleString()} out ·{' '}
          {usage.turns} turn{usage.turns === 1 ? '' : 's'}
        </span>
      </div>
      {messages.length === 0 ? (
        <div className="nt-chat__messages">
          <Card title="System">
            <p>
              Welcome to <strong>Synthia</strong>. Type a message below to start a chat. Session:{' '}
              <code>{sessionId?.slice(0, 8)}</code>
            </p>
          </Card>
        </div>
      ) : (
        <ChatMessageList messages={viewMessages} now={now} />
      )}
      <div ref={messagesEndRef} aria-hidden />
      {lastAssistant && canRegenerate && (
        <div className="nt-chat__message-actions" data-testid="message-actions">
          <Button
            size="1"
            variant="soft"
            color="gray"
            onClick={handleRegenerate}
            data-testid="regenerate-button"
            aria-label="Regenerate response"
          >
            <span aria-hidden>↻</span>
            Regenerate
          </Button>
          <Button
            size="1"
            variant="ghost"
            color="gray"
            onClick={() => handleFeedback(lastAssistant.id, true)}
            data-testid={`feedback-up-${lastAssistant.id}`}
            aria-label="Mark response as helpful"
          >
            <span aria-hidden>👍</span>
          </Button>
          <Button
            size="1"
            variant="ghost"
            color="gray"
            onClick={() => handleFeedback(lastAssistant.id, false)}
            data-testid={`feedback-down-${lastAssistant.id}`}
            aria-label="Mark response as unhelpful"
          >
            <span aria-hidden>👎</span>
          </Button>
        </div>
      )}
      {isStreaming && (
        <div className="nt-chat__streaming-indicator" data-testid="typing-dots">
          <span className="nt-chat__typing-dot" />
          <span className="nt-chat__typing-dot" />
          <span className="nt-chat__typing-dot" />
          <span className="nt-chat__typing-label">Thinking…</span>
        </div>
      )}
      {isStreaming && (
        <button
          type="button"
          aria-label="Stop generating"
          data-testid="stop-button"
          className="nt-chat__stop-button"
          onClick={async () => {
            // Local cancellation + server cancel. The REST
            // surface exposes `POST /chat/sessions/{id}/cancel`
            // which the controller translates into a
            // `SessionOp::Cancel` so the in-flight ReAct run
            // exits at its next yield point. Without this the
            // user clicks Stop but the model keeps generating
            // and the next SSE frame still arrives.
            if (sessionId) {
              try {
                await apiCancelSession(sessionId);
              } catch {
                // Best-effort — the local state update still
                // closes the user's session.
              }
            }
            setIsStreaming(false);
            setMessages((prev) =>
              prev.map((m) =>
                m.role === 'assistant' && m.status === 'working' ? { ...m, status: 'canceled' } : m,
              ),
            );
          }}
        >
          <span aria-hidden style={{ fontSize: 10 }}>
            ■
          </span>
          Stop
        </button>
      )}

      {streamError && lastSubmittedText && (
        <div className="nt-chat__error-banner" role="alert" aria-live="assertive">
          <span className="nt-chat__error-banner-text">
            <strong>Stream interrupted:</strong> {streamError}
          </span>
          <Button
            size="1"
            variant="soft"
            color="blue"
            onClick={handleRetry}
            disabled={!isServerAvailable || isStreaming}
            data-testid="retry-button"
          >
            Retry
          </Button>
          <Button
            size="1"
            variant="ghost"
            color="gray"
            onClick={() => {
              setStreamError(null);
              setLastSubmittedText(null);
            }}
          >
            Dismiss
          </Button>
        </div>
      )}

      <form onSubmit={handleSubmit} className="nt-chat__form">
        {agentError && (
          <div
            className="nt-chat__error-banner"
            role="alert"
            aria-live="assertive"
            data-testid="agent-error"
          >
            <span className="nt-chat__error-banner-text">
              <strong>Agent routing unavailable:</strong> {agentError}
            </span>
          </div>
        )}
        {pendingAttachments.length > 0 && (
          <ul className="nt-chat__attachments" data-testid="pending-attachments">
            {pendingAttachments.map((a, idx) => (
              <li
                key={`${a.kind}-${idx}-${'filename' in a ? (a.filename ?? '') : ''}`}
                className="nt-chat__attachment"
              >
                {a.kind === 'image' && a.dataUrl ? (
                  <img
                    src={a.dataUrl}
                    alt={a.filename ?? 'attached image'}
                    className="nt-chat__attachment-thumb"
                  />
                ) : a.kind === 'audio' && a.dataUrl ? (
                  <audio src={a.dataUrl} controls className="nt-chat__attachment-audio" />
                ) : (
                  <span aria-hidden className="nt-chat__attachment-icon">
                    📎
                  </span>
                )}
                <span className="nt-chat__attachment-name">
                  {'filename' in a && a.filename ? a.filename : a.kind}
                </span>
                <Button
                  size="1"
                  variant="ghost"
                  color="gray"
                  onClick={() => setPendingAttachments((prev) => prev.filter((_, i) => i !== idx))}
                  data-testid={`attachment-remove-${idx}`}
                  aria-label="Remove attachment"
                >
                  ✕
                </Button>
              </li>
            ))}
          </ul>
        )}
        <textarea
          ref={inputRef}
          className="nt-chat__input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
          rows={3}
          disabled={!isServerAvailable}
          data-testid="chat-input"
          aria-label="Message input"
        />
        <div className="nt-chat__composer-bar">
          {models.length > 0 && (
            <div className="nt-chat__model-selector" data-testid="model-selector">
              <label htmlFor="chat-model-select" className="nt-chat__model-label">
                Model:
              </label>
              <select
                id="chat-model-select"
                className="nt-chat__model-select"
                value={selectedModel ?? ''}
                onChange={(e) => setSelectedModel(e.target.value || null)}
                data-testid="model-select"
              >
                {models.map((m) => (
                  <option key={`${m.provider}/${m.model}`} value={`${m.provider}/${m.model}`}>
                    {m.provider}/{m.model}
                  </option>
                ))}
              </select>
            </div>
          )}
          {agentName &&
            (agents.length > 1 ? (
              <div className="nt-chat__agent-selector" data-testid="agent-selector">
                <label htmlFor="chat-agent-select" className="nt-chat__agent-label">
                  Agent:
                </label>
                <select
                  id="chat-agent-select"
                  className="nt-chat__agent-select"
                  value={agentName}
                  onChange={(e) => {
                    if (e.target.value === '') {
                      // Back to the default-resolution flow.
                      navigate(`/chat/${sessionId}`);
                    } else {
                      navigate(`/chat/${sessionId}/agent/${encodeURIComponent(e.target.value)}`, {
                        replace: true,
                      });
                    }
                  }}
                  data-testid="agent-select"
                >
                  <option value="">Default</option>
                  {agents.map((a) => (
                    <option key={a.name} value={a.name}>
                      {a.name}
                    </option>
                  ))}
                </select>
              </div>
            ) : (
              <div className="nt-chat__agent-chip" data-testid="agent-chip">
                <span aria-hidden>🤖</span>
                <span>
                  Agent: <code data-testid="agent-chip-name">{agentName}</code>
                </span>
                <Button
                  size="1"
                  variant="ghost"
                  onClick={() => navigate(`/chat/${sessionId}`)}
                  data-testid="agent-clear"
                  aria-label="Clear agent selection"
                >
                  Clear
                </Button>
              </div>
            ))}
          <label className="nt-chat__attach-button">
            <input
              type="file"
              accept="image/*,audio/*,application/pdf,text/plain,text/markdown"
              multiple
              onChange={async (e: ChangeEvent<HTMLInputElement>) => {
                const files = Array.from(e.target.files ?? []);
                if (files.length === 0) return;
                const added: AttachmentPart[] = [];
                for (const file of files) {
                  const kind = classifyAttachment(file.type);
                  if (kind === null) {
                    pushToast({
                      variant: 'warning',
                      message: `Skipping ${file.name}: unsupported type ${file.type || 'unknown'}.`,
                      durationMs: 4000,
                    });
                    continue;
                  }
                  try {
                    const dataUrl = await readFileAsDataUrl(file);
                    added.push({
                      kind,
                      dataUrl,
                      mimeType: file.type,
                      filename: file.name,
                    } as AttachmentPart);
                  } catch (err) {
                    pushToast({
                      variant: 'error',
                      message: `Failed to read ${file.name}: ${
                        err instanceof Error ? err.message : String(err)
                      }`,
                      durationMs: 5000,
                    });
                  }
                }
                if (added.length > 0) {
                  setPendingAttachments((prev) => [...prev, ...added]);
                }
                // Reset the file input so the same file can be
                // re-selected after removal.
                e.target.value = '';
              }}
              data-testid="attachment-input"
              aria-label="Attach files"
            />
            <span aria-hidden>📎</span>
            <span>Attach</span>
          </label>
          <Button
            type="submit"
            className="nt-chat__send-button"
            disabled={
              (!input.trim() && pendingAttachments.length === 0) ||
              isStreaming ||
              !isServerAvailable ||
              agentName === null
            }
            data-testid="send-button"
          >
            {isStreaming ? 'Streaming...' : 'Send'}
          </Button>
        </div>
      </form>
    </div>
  );
}
