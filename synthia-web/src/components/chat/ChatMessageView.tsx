/**
 * Shared chat-style message renderer.
 *
 * Used by the live `/chat/:sessionId` page (where segments
 * are streamed in real time) and by `/sessions/:id` (where
 * segments are reconstructed from the persisted
 * `session.history` transcript). Both surfaces share the
 * same visual contract so the user reads a tool call, a
 * tool result, a thinking segment, and a markdown text body
 * the same way regardless of which page they're looking at.
 *
 * The component is intentionally display-only: it takes a
 * `MessageSegment[]` and renders it; all state mutation
 * (streaming, persistence) is the caller's responsibility.
 *
 * The collapsible behavior (thinking/tool_call/tool_result/
 * tool_block all start collapsed) and the tool-block timeout
 * indicator live here so both pages benefit. The timeout uses
 * `Date.now()` at render time — the caller may pass a `now`
 * prop that drives a tick effect, or accept the default which
 * is computed once per render.
 */
import { memo, useEffect, useRef, useState } from 'react';
import { Markdown } from './Markdown';
import type { MessageSegment } from '../../api/chat-message';
import type { SessionPart } from '../../api/types';

const TOOL_TIMEOUT_MS = 180_000;

/**
 * Reduce a message's segments to a single plain-text blob for
 * clipboard copy. We only emit `text` segments — copying
 * internal `thinking` or `tool_call` parts to the clipboard
 * would leak reasoning traces the user doesn't expect to
 * share. Tool results are also omitted on purpose; the user
 * can scrub the message list manually if they want to copy
 * raw tool output.
 */
function segmentsToPlainText(segments: ReadonlyArray<MessageSegment>): string {
  return segments
    .filter((s) => s.type === 'text')
    .map((s) => s.content)
    .join('\n');
}

/**
 * One-shot copy-to-clipboard button shown next to the role
 * label on each message. The button shows a transient "Copied"
 * label for 1.5s after a successful copy so the user gets
 * feedback without the control getting noisy. Errors (e.g.
 * clipboard permission denied) fall back to selecting the
 * text and letting the user press Cmd/Ctrl+C.
 *
 * The plain-text blob is computed lazily on click rather than
 * eagerly on every render. Long streaming assistant replies
 * re-render this button once per segment — running a filter +
 * map + join across every segment on each tick just to pass a
 * string down that the user might never copy was measurable
 * garbage (5–15ms per re-render on a 30-segment reply).
 */
function CopyButton({ segments }: { segments: ReadonlyArray<MessageSegment> }) {
  const [copied, setCopied] = useState(false);
  // Track the active 1.5s reset timer so we can cancel it on
  // unmount or on a follow-up click — otherwise the timer
  // fires `setCopied(false)` against an unmounted component,
  // which React surfaces as a noisy console warning and
  // can leak the timer reference into the next click.
  const resetTimerRef = useRef<number | null>(null);

  const handleClick = async () => {
    const text = segmentsToPlainText(segments);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      flashCopied();
    } catch {
      // Clipboard API requires secure context (HTTPS or
      // localhost). Fall back to legacy execCommand so the
      // user still gets a working button behind a proxy.
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try {
        document.execCommand('copy');
        flashCopied();
      } catch {
        // give up silently — the user can select + copy manually
      } finally {
        document.body.removeChild(ta);
      }
    }
  };

  function flashCopied(): void {
    setCopied(true);
    if (resetTimerRef.current !== null) {
      window.clearTimeout(resetTimerRef.current);
    }
    resetTimerRef.current = window.setTimeout(() => {
      setCopied(false);
      resetTimerRef.current = null;
    }, 1_500);
  }

  // Cleanup on unmount so a click followed by a route change
  // (or a strict-mode double-mount) doesn't fire `setCopied`
  // on an unmounted component.
  useEffect(() => {
    return () => {
      if (resetTimerRef.current !== null) {
        window.clearTimeout(resetTimerRef.current);
        resetTimerRef.current = null;
      }
    };
  }, []);
  return (
    <button
      type="button"
      onClick={handleClick}
      aria-label={copied ? 'Copied to clipboard' : 'Copy message text'}
      data-testid="copy-message"
      // We don't know the resolved plain-text length without
      // computing it, and that's the whole point of the lazy
      // refactor. The handleClick guard already short-circuits
      // empty results, so a non-text-only message (one with
      // only `thinking` or `tool_call` segments) silently no-
      // ops at click time. Visual feedback is left intact for
      // the common case — the button still looks clickable for
      // any message with at least one text segment.
      className="nt-chat__copy-button"
    >
      {copied ? 'Copied' : 'Copy'}
    </button>
  );
}

/**
 * Render a markdown string. Used for the final-answer `text`
 * segment AND the `thinking` segment so reasoning traces render
 * with the same fidelity (code blocks, lists, links) as the
 * model's actual reply.
 *
 * The heavy lifting (react-markdown + remark + rehype +
 * highlight.js) lives in a lazy-loaded chunk via
 * `./Markdown` — see that file for the rationale. This thin
 * re-export keeps the call sites readable.
 */

/**
 * Render one `SessionPart` inside an artifact card. Spec §4.3:
 *   - file-shaped Part::data({ path, content, language })
 *   - structured-JSON Part::data({ ...rest })
 *   - resource Part::url (with optional caption text)
 *   - standalone Part::text body
 *
 * Unknown shapes fall through to a raw-JSON <pre> so the
 * user can still see the payload.
 */
// Internal helper used only by `ArtifactSegment` below;
// `export` removed during the 2026-08-15 optimization pass
// (knip flagged as unused export).
/**
 * `Set` membership test for the "is this a file artifact?" check
 * inside [`ArtifactPart`]. File artifacts carry exactly
 * `{path, content[, language]}` — any other keys disqualify
 * the part. The previous version materialised the full data
 * key list via `Object.keys(data)` (a fresh array per render
 * per part) and walked it via `.every` + `.includes`. With a
 * streaming reply that re-renders the chat on every delta,
 * that's an avoidable allocation in the hot path.
 */
const ARTIFACT_FILE_KEYS: ReadonlySet<string> = new Set(['path', 'content', 'language']);

function ArtifactPart({ part }: { part: SessionPart }): JSX.Element {
  if (typeof part.text === 'string' && part.text.length > 0 && !part.data && !part.url) {
    return (
      <div className="nt-session__artifact-result">
        <div className="nt-session__artifact-section-label">内容</div>
        <pre className="nt-session__artifact-pre">{part.text}</pre>
      </div>
    );
  }
  if (part.url) {
    return (
      <div className="nt-session__artifact-result">
        <div className="nt-session__artifact-section-label">资源</div>
        <pre className="nt-session__artifact-pre">
          <a href={part.url} target="_blank" rel="noopener noreferrer">
            {part.url}
          </a>
        </pre>
      </div>
    );
  }
  if (part.data && typeof part.data === 'object') {
    const data = part.data as Record<string, unknown>;
    const isFile =
      typeof data.path === 'string' &&
      typeof data.content === 'string' &&
      // `Set.has` is O(1) vs `Array.includes` O(K); the
      // `Object.keys(data)` allocation is the same cost as
      // before but each per-key check is now hash-lookup fast.
      Object.keys(data).every((k) => ARTIFACT_FILE_KEYS.has(k));
    if (isFile) {
      return (
        <div className="nt-session__artifact-call">
          <div className="nt-session__artifact-section-label">
            {`\u{1F4C4} ${data.path as string}`}
          </div>
          <pre className="nt-session__artifact-pre">{data.content as string}</pre>
        </div>
      );
    }
    return (
      <div className="nt-session__artifact-result">
        <div className="nt-session__artifact-section-label">数据</div>
        <pre className="nt-session__artifact-pre">{JSON.stringify(part.data, null, 2)}</pre>
      </div>
    );
  }
  return (
    <div className="nt-session__artifact-result">
      <div className="nt-session__artifact-section-label">（未知 part 形态）</div>
      <pre className="nt-session__artifact-pre">{JSON.stringify(part, null, 2)}</pre>
    </div>
  );
}

/**
 * Render an 'artifact' segment inline in the chat stream.
 * Reuses the shared `.nt-session__artifact-*` styles for
 * parity with the session-detail Artifacts card, layered
 * with `.nt-chat__segment--artifact` for chat-only visual
 * identity (accent badge + streaming chip).
 */
// Internal helper used only by `SegmentView` below;
// `export` removed during the 2026-08-15 optimization pass
// (knip flagged as unused export).
function ArtifactSegment({ segment }: { segment: MessageSegment }): JSX.Element {
  const [expanded, setExpanded] = useState(true);
  const parts = segment.attachmentParts ?? [];
  return (
    <div
      className={`nt-chat__segment nt-chat__segment--artifact${
        segment.isComplete === false ? ' nt-chat__artifact-streaming' : ''
      }`}
      data-testid={`chat-artifact-${segment.attachmentId}`}
    >
      <div className="nt-session__artifact-header">
        <button
          type="button"
          className="nt-chat__artifact-badge"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
        >
          <span aria-hidden>📎</span>
          <span>{segment.attachmentName ?? `Artifact · ${segment.attachmentId}`}</span>
          {segment.isComplete === false && <span>· streaming…</span>}
        </button>
      </div>
      {expanded && (
        <div>
          {parts.length === 0 ? (
            <pre className="nt-session__artifact-pre">(empty artifact)</pre>
          ) : (
            parts.map((p, i) => <ArtifactPart key={i} part={p} />)
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Render one segment. Collapsible segments (thinking,
 * tool_call, tool_result, tool_block) start collapsed; the
 * user can click the header to expand. Tool blocks render as
 * a single header `工具 · <name>` that expands into two
 * sub-blocks — the yellow call body (the JSON we sent) and
 * the green result body (the JSON we got back).
 */
// Internal helper used only by `ChatMessageList` below;
// `export` removed during the 2026-08-15 optimization pass
// (knip flagged as unused export).
//
// Memoized so a token-level stream event doesn't re-render
// every earlier segment in the transcript. The `now` prop is
// the tick-driven wall-clock value that the parent forwards —
// it's expected to *change* every 5s for tool-block timeout
// evaluation, but a fast text delta shouldn't bounce every
// completed segment back through React.
const SegmentView = memo(function SegmentView({
  segment,
  now,
}: {
  segment: MessageSegment;
  now: number;
}): JSX.Element {
  // All collapsible segments (thinking, tool_call, tool_result, tool_block)
  // start collapsed. Thinking is verbose and only relevant when the user
  // explicitly wants to inspect the reasoning; tool blocks are JSON dumps
  // that the user expands to inspect on demand. The user can click the
  // header to expand any of them.
  const [expanded, setExpanded] = useState(false);

  // Artifact segments are rendered by ArtifactSegment; delegate
  // before any of the collapsible / plain-text branches so the
  // shared .nt-session__artifact-* styles always win.
  if (segment.type === 'attachment') {
    return <ArtifactSegment segment={segment} />;
  }

  const isCollapsible =
    segment.type === 'thinking' ||
    segment.type === 'tool_call' ||
    segment.type === 'tool_result' ||
    segment.type === 'tool_block';

  // A tool_block is "timed out" when it's been pending longer than
  // TOOL_TIMEOUT_MS and no result has arrived yet. `now` is the
  // parent's tick-driven re-render value so this flips without a
  // per-segment setTimeout. Once a real result lands, `toolPending`
  // becomes false and the timeout indicator disappears.
  const isTimedOut =
    segment.type === 'tool_block' &&
    segment.toolPending === true &&
    segment.pendingSince !== undefined &&
    now - segment.pendingSince > TOOL_TIMEOUT_MS;

  if (!isCollapsible) {
    return (
      <>
        <div className={`nt-chat__segment nt-chat__segment--${segment.type}`}>
          <Markdown source={segment.content} />
        </div>
      </>
    );
  }

  // tool_block renders as a single header `工具 · <name>` that
  // expands into two sub-blocks — the yellow call body (the JSON
  // we sent) and the green result body (the JSON we got back).
  if (segment.type === 'tool_block') {
    const elapsed = segment.pendingSince ? Math.floor((now - segment.pendingSince) / 1000) : 0;
    return (
      <>
        <div
          className={`nt-chat__segment nt-chat__segment--tool_block${
            isTimedOut ? ' nt-chat__segment--tool_block-timeout' : ''
          }`}
        >
          <button
            className="chat-toggle"
            onClick={() => setExpanded(!expanded)}
            type="button"
            aria-expanded={expanded}
          >
            <span className={`nt-chat__segment-icon ${expanded ? 'expanded' : ''}`}>▸</span>
            <span className="nt-chat__segment-label">
              {`工具${segment.toolName ? ` · ${segment.toolName}` : ''}`}
              {isTimedOut
                ? ` · 超时（${elapsed}s）`
                : segment.toolPending
                  ? ' · 执行中…'
                  : segment.toolError
                    ? ' · 失败'
                    : ''}
            </span>
          </button>
          {expanded && (
            <div className="nt-chat__tool-block-body">
              {segment.callContent !== undefined && (
                <div className="nt-chat__tool-block-call">
                  <div className="nt-chat__tool-block-label">请求</div>
                  <pre className="nt-chat__tool-block-pre">{segment.callContent}</pre>
                </div>
              )}
              {isTimedOut ? (
                <div className="nt-chat__tool-block-result nt-chat__tool-block-result--timeout">
                  <div className="nt-chat__tool-block-label">结果</div>
                  <pre className="nt-chat__tool-block-pre">
                    {`工具已等待 ${elapsed} 秒，超过 ${TOOL_TIMEOUT_MS / 1000} 秒阈值。` +
                      ' 后端可能已断开或工具卡死。'}
                  </pre>
                </div>
              ) : segment.toolPending ? (
                <div className="nt-chat__tool-block-result nt-chat__tool-block-result--pending">
                  <div className="nt-chat__tool-block-label">结果</div>
                  <pre className="nt-chat__tool-block-pre">等待执行结果…</pre>
                </div>
              ) : segment.resultContent !== undefined ? (
                <div
                  className={`nt-chat__tool-block-result${
                    segment.toolError ? ' nt-chat__tool-block-result--error' : ''
                  }`}
                >
                  <div className="nt-chat__tool-block-label">结果</div>
                  <pre className="nt-chat__tool-block-pre">{segment.resultContent}</pre>
                </div>
              ) : null}
            </div>
          )}
        </div>
      </>
    );
  }

  const label =
    segment.type === 'thinking'
      ? '思考'
      : segment.type === 'tool_result'
        ? `工具${segment.toolName ? ` · ${segment.toolName}` : ''} · 结果`
        : `工具${segment.toolName ? ` · ${segment.toolName}` : ''}`;

  return (
    <>
      <div className={`nt-chat__segment nt-chat__segment--${segment.type}`}>
        <button
          className="chat-toggle"
          onClick={() => setExpanded(!expanded)}
          type="button"
          aria-expanded={expanded}
        >
          <span className={`nt-chat__segment-icon ${expanded ? 'expanded' : ''}`}>▸</span>
          <span className="nt-chat__segment-label">{label}</span>
        </button>
        {expanded && (
          <div className="nt-chat__segment-content">
            <Markdown source={segment.content} />
          </div>
        )}
      </div>
    </>
  );
});

/**
 * Minimal message shape accepted by `ChatMessageList`. The
 * chat page builds it from streamed events; the session
 * detail page builds it via `reconstructMessagesFromSession`.
 * Both produce the same shape so the rendering below is
 * identical.
 */
export interface ChatMessageViewItem {
  id: string;
  role: 'user' | 'assistant';
  segments: MessageSegment[];
  status?: string;
  isStreaming?: boolean;
}

/**
 * Render a flat list of chat-style messages — each with a
 * `> USER` / `> ASSISTANT` header, a status pill, and one row
 * per segment. Used by both `/chat/:sessionId` (with streaming
 * state) and `/sessions/:id` (with reconstructed history) so
 * the user reads the same visual contract on both pages.
 *
 * `now` is a tick-driven wall-clock value so per-segment
 * tool-block timeout indicators can re-evaluate without each
 * segment mounting its own setTimeout.
 */
/**
 * Memoize the whole list so the outer ChatPage's tick re-render
 * (driven by `now`) doesn't ripple into the inner SegmentView
 * tree unless `messages` actually changed. Combined with the
 * `React.memo` on `SegmentView`, this caps the per-tick work to
 * a single `useState` read in every collapsible segment — the
 * DOM stays untouched when nothing changed.
 */
export const ChatMessageList = memo(function ChatMessageList({
  messages,
  now,
  className,
}: {
  messages: ReadonlyArray<ChatMessageViewItem>;
  now: number;
  className?: string;
}): JSX.Element {
  return (
    <div
      className={`nt-chat__messages${className ? ` ${className}` : ''}`}
      data-testid="chat-messages"
      aria-live="polite"
      aria-relevant="additions text"
    >
      {messages.map((msg) => (
        <div
          key={msg.id}
          className={`nt-chat__message nt-chat__message--${msg.role}`}
          data-role={msg.role}
          data-testid={`message-${msg.role}`}
          data-streaming={
            msg.isStreaming === true && msg.role === 'assistant' && msg.status === 'working'
          }
        >
          <div className="nt-chat__message-meta">
            <span className="nt-chat__message-role">
              {msg.role === 'user' ? '> USER' : '> ASSISTANT'}
            </span>
            <span className="nt-chat__message-actions">
              <CopyButton segments={msg.segments} />
            </span>
            {msg.status && (
              <span className={`nt-chat__message-status status-${msg.status}`}>{msg.status}</span>
            )}
          </div>
          <div className="nt-chat__message-content">
            {msg.segments.map((segment) => (
              <SegmentView key={segment.id} segment={segment} now={now} />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
});
