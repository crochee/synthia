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
import { Link } from 'react-router-dom';
import { Markdown } from './Markdown';
import type { ChatFile, ChatImage, MessageSegment } from '../../api/chat-message';
import type { SessionPart } from '../../api/types';
import { basename, childSessionIdOf, parseTodoItems } from '../../lib/session-panel-data';

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
/**
 * Render the `image` parts a segment carries.
 *
 * The multimodal path lands here: a tool that returned a picture
 * (screenshot, chart, OCR) instead of text. Parts are already
 * normalised to a renderable URL by `extractImages`, so this is a
 * straight `<img>` — no fetching, no blob lifecycle.
 *
 * The picture is capped by CSS so a full-resolution screenshot
 * cannot blow out the transcript; clicking toggles a full-size
 * view *in place*. An `<a target="_blank">` is deliberately not
 * used: Chromium and Firefox both refuse top-frame navigation to
 * a `data:` URL, so the link would open an empty tab for the
 * common base64 case.
 */
const SegmentImages = memo(function SegmentImages({
  images,
}: {
  images: ChatImage[];
}): JSX.Element {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  return (
    <div className="nt-chat__segment-images">
      {images.map((image) => {
        const expanded = expandedId === image.id;
        return (
          <button
            key={image.id}
            type="button"
            className={`nt-chat__segment-image-button${expanded ? ' is-expanded' : ''}`}
            onClick={() => setExpandedId(expanded ? null : image.id)}
            aria-expanded={expanded}
            title={expanded ? '点击还原' : `${image.mimeType} — 点击查看原图`}
          >
            <img
              className="nt-chat__segment-image"
              src={image.dataUrl}
              alt={image.mimeType}
              loading="lazy"
            />
          </button>
        );
      })}
    </div>
  );
});

/**
 * Human-readable size for a chip's tooltip. Rounded at one decimal so a
 * `byte_len` the wire reported cannot paint a wall of digits; the exact
 * byte count was never the point of the hint.
 */
function byteSize(len: number): string {
  if (len < 1024) return `${len} B`;
  if (len < 1024 * 1024) return `${Math.round(len / 1024)} kB`;
  return `${(len / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Tooltip for one file chip: the short-form URI when the wire named one,
 * the MIME type, and the size when the row reported it. A `data:` URI
 * never reaches here — `filesFromPersistedAttachments` drops it at the
 * projection — so the base64 payload cannot leak into the DOM through
 * the title attribute.
 */
function fileChipTitle(file: ChatFile): string {
  return [file.uri, file.mimeType, file.byteLen === undefined ? '' : byteSize(file.byteLen)]
    .filter((part): part is string => typeof part === 'string' && part.length > 0)
    .join(' · ');
}

const SegmentFiles = memo(function SegmentFiles({ files }: { files: ChatFile[] }): JSX.Element {
  return (
    <div className="nt-chat__segment-files">
      {files.map((file) => (
        <span key={file.id} className="nt-chat__segment-file" title={fileChipTitle(file)}>
          <span aria-hidden className="nt-chat__segment-file-icon">
            📎
          </span>
          <span className="nt-chat__segment-file-name">{file.filename}</span>
        </span>
      ))}
    </div>
  );
});

/**
 * Always-visible summary for a `TodoWrite` call: the checklist and
 * a progress bar, instead of a collapsed JSON dump the reader has
 * to expand to learn what the agent is doing.
 */
function TodoSummary({ callContent }: { callContent: string | undefined }): JSX.Element | null {
  const items = parseTodoItems(callContent);
  if (!items || items.length === 0) return null;
  const completed = items.filter((item) => item.status === 'completed').length;
  const percent = Math.round((completed / items.length) * 100);
  return (
    <>
      <ol className="nt-chat__todo-list">
        {items.map((item) => (
          <li key={item.id} className={`nt-chat__todo-item nt-chat__todo-item--${item.status}`}>
            <span aria-hidden>
              {item.status === 'completed'
                ? '✓'
                : item.status === 'in_progress'
                  ? '◐'
                  : item.status === 'cancelled'
                    ? '–'
                    : '○'}
            </span>
            <span className="nt-chat__todo-text">{item.content}</span>
          </li>
        ))}
      </ol>
      <div className="nt-chat__progress">
        <span className="nt-chat__progress-track">
          <span className="nt-chat__progress-fill" style={{ width: `${percent}%` }} />
        </span>
        <span>{`${completed}/${items.length}`}</span>
      </div>
    </>
  );
}

/**
 * Always-visible summary for a `task` call: the delegated peer, its
 * status, and a link to the child session that holds the trace.
 * The child session id is minted server-side and echoed in the tool
 * result, so the link appears once the result lands.
 */
function SubagentSummary({ segment }: { segment: MessageSegment }): JSX.Element | null {
  const body = segment.callContent ? safeParseObject(segment.callContent) : null;
  const agent = typeof body?.agent === 'string' ? body.agent : null;
  const prompt = typeof body?.prompt === 'string' ? body.prompt : null;
  if (!agent && !prompt) return null;
  const childSessionId = childSessionIdOf(segment.resultContent);
  const status = segment.toolAborted
    ? '已中止'
    : segment.toolPending
      ? '执行中…'
      : segment.toolError
        ? '失败'
        : '已完成';
  return (
    <div className="nt-chat__subagent-card">
      <div className="nt-chat__subagent-card-head">
        <span className="nt-chat__subagent-card-agent">{agent ?? 'unknown'}</span>
        <span>{status}</span>
      </div>
      {prompt ? <p className="nt-chat__subagent-card-prompt">{prompt}</p> : null}
      {childSessionId ? (
        <Link className="nt-chat__subagent-card-link" to={`/sessions/${childSessionId}`}>
          查看子会话 →
        </Link>
      ) : null}
    </div>
  );
}

/**
 * Always-visible summary for a `write` call: the file it wrote,
 * as a chip. The path is the thing the reader recognises; the
 * content stays in the expandable body.
 */
function WriteSummary({ segment }: { segment: MessageSegment }): JSX.Element | null {
  const body = segment.callContent ? safeParseObject(segment.callContent) : null;
  const path = typeof body?.file_path === 'string' ? body.file_path : null;
  if (!path) return null;
  const state = segment.toolPending ? '写入中…' : segment.toolError ? '失败' : '已写入';
  return (
    <div className="nt-chat__file-chips">
      <span
        className={`nt-chat__file-chip${segment.toolError ? ' nt-chat__file-chip--error' : ''}`}
        title={path}
      >
        <span aria-hidden>📄</span>
        <span>{basename(path)}</span>
        <span className="nt-chat__file-chip-state">{state}</span>
      </span>
    </div>
  );
}

/** Parse a JSON object, tolerating junk (a truncated stream chunk). */
function safeParseObject(raw: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
    return null;
  } catch {
    return null;
  }
}

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
  // becomes false and the timeout indicator disappears. An aborted
  // block never times out — the run that owed the result is gone, so
  // claiming the tool may have hung would be wrong.
  const isAborted = segment.type === 'tool_block' && segment.toolAborted === true;
  const isTimedOut =
    segment.type === 'tool_block' &&
    segment.toolPending === true &&
    !isAborted &&
    segment.pendingSince !== undefined &&
    now - segment.pendingSince > TOOL_TIMEOUT_MS;

  if (!isCollapsible) {
    return (
      <>
        <div className={`nt-chat__segment nt-chat__segment--${segment.type}`}>
          {segment.content ? <Markdown source={segment.content} /> : null}
          {segment.images && segment.images.length > 0 && <SegmentImages images={segment.images} />}
          {segment.files && segment.files.length > 0 && <SegmentFiles files={segment.files} />}
        </div>
      </>
    );
  }

  // tool_block renders as a single header `工具 · <name>` that
  // expands into two sub-blocks — the yellow call body (the JSON
  // we sent) and the green result body (the JSON we got back).
  //
  // Three tools lead with a human-readable summary *without*
  // expanding (a todo checklist, a delegation card, a file chip):
  // their raw JSON is a poor primary rendering of information the
  // reader came to the transcript for. Everything else keeps the
  // collapsed-JSON default.
  if (segment.type === 'tool_block') {
    const elapsed = segment.pendingSince ? Math.floor((now - segment.pendingSince) / 1000) : 0;
    const summary =
      segment.toolName === 'TodoWrite' ? (
        <TodoSummary callContent={segment.callContent} />
      ) : segment.toolName === 'task' ? (
        <SubagentSummary segment={segment} />
      ) : segment.toolName === 'write' ? (
        <WriteSummary segment={segment} />
      ) : null;
    return (
      <>
        <div
          className={`nt-chat__segment nt-chat__segment--tool_block${
            summary ? ' nt-chat__segment--tool-summary' : ''
          }${isTimedOut ? ' nt-chat__segment--tool_block-timeout' : ''}`}
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
                : isAborted
                  ? ' · 已中止'
                  : segment.toolPending
                    ? ' · 执行中…'
                    : segment.toolError
                      ? ' · 失败'
                      : ''}
            </span>
          </button>
          {summary}
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
              ) : isAborted ? (
                <div className="nt-chat__tool-block-result nt-chat__tool-block-result--aborted">
                  <div className="nt-chat__tool-block-label">结果</div>
                  <pre className="nt-chat__tool-block-pre">
                    本次运行已结束（取消或中断），该工具调用没有返回结果。
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
                  {segment.resultContent ? (
                    <pre className="nt-chat__tool-block-pre">{segment.resultContent}</pre>
                  ) : null}
                  {segment.images && segment.images.length > 0 && (
                    <SegmentImages images={segment.images} />
                  )}
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
      ? '思考过程'
      : segment.type === 'tool_result'
        ? `工具${segment.toolName ? ` · ${segment.toolName}` : ''} · 结果`
        : `工具${segment.toolName ? ` · ${segment.toolName}` : ''}`;

  // A collapsed thinking block shows its opening line as a muted
  // preview: enough to recognise what the model was working on
  // without expanding, and — crucially — enough to tell the
  // scratchpad apart from the answer at a glance.
  const thinkingPreview =
    segment.type === 'thinking' && !expanded && segment.content
      ? segment.content.trim().split('\n')[0].slice(0, 160)
      : null;

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
        {thinkingPreview ? (
          <span className="nt-chat__thinking-preview">{thinkingPreview}</span>
        ) : null}
        {expanded && (
          <div className="nt-chat__segment-content">
            {segment.content ? <Markdown source={segment.content} /> : null}
            {segment.images && segment.images.length > 0 && (
              <SegmentImages images={segment.images} />
            )}
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
  role: 'user' | 'assistant' | 'subagent';
  /** Nesting depth of a `role: 'subagent'` message (1 = direct
   *  sub-agent of the session). Undefined for user/assistant. */
  subagentDepth?: number;
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
              {msg.role === 'user'
                ? '> USER'
                : msg.role === 'subagent'
                  ? `> SUB-AGENT${msg.subagentDepth ? ` · depth ${msg.subagentDepth}` : ''}`
                  : '> ASSISTANT'}
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
