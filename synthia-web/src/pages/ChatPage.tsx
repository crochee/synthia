import { useState, useRef, useEffect, type FormEvent, type KeyboardEvent } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Button } from '../components/ui/Button';
import { Card } from '../components/ui/Card';
import { taskStateToJSON } from '@a2a-js/sdk';
import {
  sendMessageStream,
  extractFromMessage,
  extractPartWithMetadata,
  type A2AStreamEvent,
  type SegmentType,
} from '../api/a2a-stream';
import './ChatPage.css';

/**
 * Map A2A TaskState enum names (TASK_STATE_*) to the CSS class
 * suffix used by nt-chat__message-status (.status-{suffix}).
 * Accepts both raw enum names (TASK_STATE_COMPLETED) and the
 * unprefixed lowercase form (completed) for resilience.
 */
function normalizeTaskState(state: string): string {
  const stripped = state.replace(/^TASK_STATE_/, '').toLowerCase();
  return stripped || 'unknown';
}

const THINK_OPEN = '<think>';
const THINK_CLOSE = '</think>';

/**
 * Incremental parser for streaming text with embedded
 * `<think>...</think>` markers. Tracks the *last* segment id
 * produced for each type so that callers can append rather
 * than create a new segment on every chunk.
 *
 * State machine:
 *   - `idle` outside any marker; plain text accumulates in `textTail`
 *   - `in_think` between markers; content accumulates in `thinkTail`
 *
 * The carry window is `THINK_OPEN.length-1` for plain text (so a
 * partial `<think>` opener is not flushed as text) and 0 inside a
 * thinking segment (a partial `</think>` is harmless — the next
 * delta will close it).
 */
class ThinkingParser {
  private textId: string | null = null;
  private thinkId: string | null = null;
  private textTail = '';
  private thinkTail = '';
  private pendingCarry = '';

  /**
   * Feed one SSE delta into the parser. Returns the segments to
   * upsert into the assistant message. Each returned segment
   * either (a) replaces an existing segment by id (when the
   * caller has appended to it) or (b) is a new segment to append.
   * Callers append them in order to the message's segment list.
   */
  /**
   * Force-close any in-flight thinking segment. Used by the
   * `response_complete` marker when the provider emitted
   * `<think>` but not `` — the dangling segment is
   * returned so the caller can append it to the message.
   * Returns null when no thinking segment is open.
   */
  forceCloseThinking(): MessageSegment | null {
    if (this.thinkId === null) return null;
    const segment: MessageSegment = {
      id: this.thinkId,
      type: 'thinking',
      content: this.thinkTail,
    };
    this.thinkId = null;
    this.thinkTail = '';
    return segment;
  }

  feed(delta: string): MessageSegment[] {
    const out: MessageSegment[] = [];
    let rest = delta;
    while (rest.length > 0) {
      if (this.thinkId !== null) {
        // Inside a thinking segment: scan for closer.
        const closeIdx = rest.indexOf(THINK_CLOSE);
        if (closeIdx === -1) {
          this.thinkTail += rest;
          out.push({
            id: this.thinkId,
            type: 'thinking',
            content: this.thinkTail,
          });
          return out;
        }
        this.thinkTail += rest.slice(0, closeIdx);
        out.push({
          id: this.thinkId,
          type: 'thinking',
          content: this.thinkTail,
        });
        this.thinkId = null;
        this.thinkTail = '';
        rest = rest.slice(closeIdx + THINK_CLOSE.length);
        continue;
      }

      // Idle: scan for <think>
      const openIdx = rest.indexOf(THINK_OPEN);
      if (openIdx === -1) {
        // No opener in this chunk — keep all of `rest` in
        // pendingCarry so a partial `<think>` straddling chunks
        // is preserved (will be re-prepended on the next delta).
        this.pendingCarry += rest;
        // If pendingCarry is now safely larger than a partial
        // opener would ever be, emit it as text.
        if (this.pendingCarry.length >= THINK_OPEN.length) {
          const safeLen = this.pendingCarry.length - (THINK_OPEN.length - 1);
          const safe = this.pendingCarry.slice(0, safeLen);
          const carry = this.pendingCarry.slice(safeLen);
          if (safe) {
            if (!this.textId) this.textId = crypto.randomUUID();
            this.textTail += safe;
            out.push({
              id: this.textId,
              type: 'text',
              content: this.textTail,
            });
          }
          this.pendingCarry = carry;
        }
        return out;
      }

      // Found <think> opener in this chunk.
      // First flush any carry that has accumulated since the last delta
      // (it sits *before* the opener).
      if (this.pendingCarry.length > 0) {
        this.textTail += this.pendingCarry;
        this.pendingCarry = '';
        if (!this.textId) this.textId = crypto.randomUUID();
        out.push({
          id: this.textId,
          type: 'text',
          content: this.textTail,
        });
      }
      // Flush any plain text *inside* this chunk before the opener
      if (openIdx > 0) {
        this.textTail += rest.slice(0, openIdx);
        if (!this.textId) this.textId = crypto.randomUUID();
        out.push({
          id: this.textId,
          type: 'text',
          content: this.textTail,
        });
      }
      // Reset text accumulator — the segment is closed by the opener.
      this.textTail = '';
      this.textId = null;

      // Enter thinking
      this.thinkId = crypto.randomUUID();
      this.thinkTail = '';
      rest = rest.slice(openIdx + THINK_OPEN.length);
    }
    return out;
  }

  /** Replay any carry from the previous feed before processing a new delta. */
  beginDelta(delta: string): MessageSegment[] {
    const out = this.feed(delta);
    return out;
  }

  /**
   * Called when the stream ends. Flushes any carry/text
   * accumulator that hasn't been emitted yet.
   */
  flush(): MessageSegment[] {
    const out: MessageSegment[] = [];
    if (this.pendingCarry) {
      // No more deltas coming — emit the residual text.
      if (!this.textId) this.textId = crypto.randomUUID();
      this.textTail += this.pendingCarry;
      this.pendingCarry = '';
      out.push({
        id: this.textId,
        type: 'text',
        content: this.textTail,
      });
      this.textTail = '';
      this.textId = null;
    }
    return out;
  }

  /**
   * Process the full content of a `LlmResponseComplete` message.
   * The text_delta stream has already emitted the same content
   * piecewise; we only want to pick up *new* `<think>…</think>`
   * regions that the parser hasn't seen yet (e.g. because the
   * second iteration's reasoning appears only in the final
   * message). Plain-text portions are skipped — they're already
   * represented by existing text segments.
   */
  feedForFinalize(content: string): MessageSegment[] {
    const out: MessageSegment[] = [];
    let rest = content;
    // Walk only `<think>…</think>` regions; skip everything else.
    while (rest.length > 0) {
      const openIdx = rest.indexOf(THINK_OPEN);
      if (openIdx === -1) return out;
      const afterOpen = openIdx + THINK_OPEN.length;
      const closeIdx = rest.indexOf(THINK_CLOSE, afterOpen);
      if (closeIdx === -1) return out;
      const thinkContent = rest.slice(afterOpen, closeIdx);
      out.push({
        id: crypto.randomUUID(),
        type: 'thinking',
        content: thinkContent,
      });
      rest = rest.slice(closeIdx + THINK_CLOSE.length);
    }
    return out;
  }
}

interface MessageSegment {
  id: string;
  type: SegmentType;
  content: string;
  toolName?: string;
  iteration?: number;
  expanded?: boolean;
}

interface Message {
  id: string;
  role: 'user' | 'assistant';
  segments: MessageSegment[];
  taskId?: string;
  status?: string;
}

const STORAGE_KEY = 'synthia.sessions.v1';

interface SessionMeta {
  id: string;
  title: string;
  createdAt: string;
}

function SegmentView({
  segment,
  streaming,
}: {
  segment: MessageSegment;
  /** True when the parent message is still receiving chunks
   *  (status === 'working' and isStreaming). When false the
   *  typewriter snaps to the full content. */
  streaming: boolean;
}) {
  // Both thinking and tool_call default to collapsed so they
  // don't dominate the viewport — user clicks to reveal.
  const [expanded, setExpanded] = useState(segment.expanded ?? false);

  const isCollapsible = segment.type === 'thinking' || segment.type === 'tool_call';

  // For plain text, reveal characters progressively so a
  // large chunk doesn't pop in instantly. useTypewriter returns
  // the prefix to render; the rest is still being drawn.
  const revealed = useTypewriter(segment.id, segment.content, streaming);

  if (!isCollapsible) {
    return (
      <div className={`nt-chat__segment nt-chat__segment--${segment.type}`}>
        {revealed}
        {revealed.length < segment.content.length && (
          <span className="nt-chat__caret" aria-hidden="true" />
        )}
      </div>
    );
  }

  const label =
    segment.type === 'thinking'
      ? `思考${segment.iteration ? ` · 迭代 ${segment.iteration}` : ''}`
      : `工具${segment.toolName ? ` · ${segment.toolName}` : ''}`;

  return (
    <div className={`nt-chat__segment nt-chat__segment--${segment.type}`}>
      <button
        className="nt-chat__segment-header"
        onClick={() => setExpanded(!expanded)}
        type="button"
        aria-expanded={expanded}
      >
        <span className={`nt-chat__segment-icon ${expanded ? 'expanded' : ''}`}>▸</span>
        <span className="nt-chat__segment-label">{label}</span>
      </button>
      {expanded && <div className="nt-chat__segment-content">{segment.content}</div>}
    </div>
  );
}

/**
 * Reveals a growing string one character at a time at a constant
 * visual rate, driven by `requestAnimationFrame`. Designed to make
 * one big SSE chunk feel like a smooth stream instead of a flash.
 *
 * Returns the prefix that should be rendered right now. When the
 * stream is no longer active (caller passes `streaming=false`) and
 * the full content fits, we immediately return everything; otherwise
 * the loop continues until the revealed prefix catches up.
 */
function useTypewriter(segmentId: string, content: string, streaming: boolean): string {
  const [revealed, setRevealed] = useState('');
  const revealedRef = useRef('');
  const contentRef = useRef('');
  // Chars per second target. ~120 chars/sec reads naturally.
  const CHARS_PER_SEC = 120;

  // Reset when the underlying segment identity changes (new
  // segment id from the parser).
  useEffect(() => {
    revealedRef.current = '';
    contentRef.current = '';
    setRevealed('');
  }, [segmentId]);

  // Keep the latest content accessible from the RAF callback
  // without restarting the loop on every chunk.
  useEffect(() => {
    contentRef.current = content;
    // If new content arrived while the loop was idle (e.g. during
    // a status-update that flushed everything synchronously), make
    // sure we render at least up to whatever has been revealed.
    if (revealedRef.current.length < content.length) {
      // No-op: the RAF loop will catch up. But if the loop is
      // not running we kick it.
      setRevealed(revealedRef.current);
    }
  }, [content]);

  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const delta = (now - last) / 1000;
      last = now;
      const target = contentRef.current.length;
      const have = revealedRef.current.length;
      if (have < target) {
        // Reveal `CHARS_PER_SEC * delta` characters per frame.
        // Cap step so very long pauses don't snap.
        const step = Math.min(target - have, Math.max(1, Math.round(CHARS_PER_SEC * delta)));
        const next = revealedRef.current + contentRef.current.slice(have, have + step);
        revealedRef.current = next;
        setRevealed(next);
        raf = requestAnimationFrame(tick);
      } else if (!streaming) {
        // Caught up and stream is done: stop the loop.
        return;
      } else {
        // Caught up but stream still active: idle and wait for
        // more content to arrive.
        raf = requestAnimationFrame(tick);
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [segmentId, streaming]);

  // Auto-scroll the chat container while the typewriter is
  // animating, so newly-revealed text doesn't get clipped at
  // the bottom of the viewport.
  useEffect(() => {
    if (!streaming) return;
    const container = document.querySelector('.nt-chat__messages');
    if (container) container.scrollTop = container.scrollHeight;
  }, [revealed, streaming]);

  return revealed;
}

/**
 * Main chat page. Sends user messages to the A2A backend via
 * `message/stream` and renders incremental assistant text as
 * SSE events arrive. Session id is read from the route param
 * and persisted to localStorage.
 */
export function ChatPage() {
  const { sessionId: routeSessionId } = useParams<{ sessionId?: string }>();
  const navigate = useNavigate();

  // Ensure a session exists: if none in URL, create one and
  // redirect to /chat/:sessionId so the URL is shareable.
  useEffect(() => {
    if (!routeSessionId) {
      const id = crypto.randomUUID();
      navigate(`/chat/${id}`, { replace: true });
    }
  }, [routeSessionId, navigate]);

  const sessionId = routeSessionId;

  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  // Per-stream parser that incrementally splits text deltas
  // into text / thinking segments based on `<think>` markers.
  const parserRef = useRef<ThinkingParser | null>(null);
  // True once the first text_delta arrives — used to suppress
  // duplicate LlmResponseComplete content (whose `text` is the
  // accumulated stream output we've already rendered).
  const sawTextDeltaRef = useRef(false);

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

  // Restore messages for the current session
  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    const raw = localStorage.getItem(`synthia.messages.${sessionId}`);
    if (!cancelled) {
      setMessages(raw ? JSON.parse(raw) : []);
    }
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  // Persist messages whenever they change
  useEffect(() => {
    if (!sessionId) return;
    localStorage.setItem(`synthia.messages.${sessionId}`, JSON.stringify(messages));
  }, [sessionId, messages]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSubmit = async (e?: FormEvent) => {
    e?.preventDefault();
    const text = input.trim();
    if (!text || isStreaming || !sessionId) return;

    const userMessage: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      segments: [{ id: crypto.randomUUID(), type: 'text', content: text }],
    };
    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setIsStreaming(true);

    const assistantId = crypto.randomUUID();
    parserRef.current = new ThinkingParser();
    sawTextDeltaRef.current = false;
    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: 'assistant', segments: [], status: 'working' },
    ]);

    try {
      for await (const event of sendMessageStream(text, sessionId)) {
        applyStreamEvent(assistantId, event);
      }
      // Flush any remaining text the parser was holding
      const flushUpdates = parserRef.current?.flush() ?? [];
      if (flushUpdates.length > 0) {
        setMessages((prev) =>
          prev.map((m) => {
            if (m.id !== assistantId) return m;
            const next = [...m.segments];
            for (const seg of flushUpdates) {
              const idx = next.findIndex((s) => s.id === seg.id);
              if (idx >= 0) {
                next[idx] = seg;
              } else {
                next.push(seg);
              }
            }
            return { ...m, segments: next };
          }),
        );
      }
    } catch (err) {
      const errorSegment: MessageSegment = {
        id: crypto.randomUUID(),
        type: 'text',
        content: `\n\n[error: ${(err as Error).message}]`,
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
    }
  };

  const applyStreamEvent = (assistantId: string, event: A2AStreamEvent) => {
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
      case 'statusUpdate': {
        if (!event.statusUpdate) return;
        const raw = event.statusUpdate.status.state;
        const state = normalizeTaskState(taskStateToJSON(raw));
        const statusMsg = event.statusUpdate.status.message;

        if (statusMsg) {
          const { text, metadata } = extractFromMessage(statusMsg);
          if (text) {
            const segmentType: SegmentType = metadata?.segment_type || 'text';
            const newSegment: MessageSegment = {
              id: crypto.randomUUID(),
              type: segmentType,
              content: text,
              toolName: metadata?.tool_name,
              iteration: metadata?.iteration,
            };
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId
                  ? { ...m, status: state, segments: [...m.segments, newSegment] }
                  : m,
              ),
            );
          }
        } else {
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantId ? { ...m, status: state } : m)),
          );
        }
        break;
      }

      case 'message': {
        if (!event.message) return;
        const { text, metadata } = extractFromMessage(event.message);
        if (text || metadata?.segment_type === 'response_complete') {
          const segmentType: SegmentType = metadata?.segment_type || 'text';

          if (segmentType === 'response_complete') {
            // Backend marker signalling the LLM has finished its
            // current response iteration. Force-close any open
            // thinking segment the streaming missed (provider
            // emitted `<think>` but not ``). No content to emit.
            const parser = parserRef.current;
            const orphan = parser?.forceCloseThinking();
            if (orphan) {
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId ? { ...m, segments: [...m.segments, orphan] } : m,
                ),
              );
            }
            return;
          }

          if (segmentType === 'text_delta') {
            sawTextDeltaRef.current = true;
            const parser = parserRef.current;
            if (!parser) return;
            const updates = parser.beginDelta(text);
            if (updates.length === 0) return;

            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                // Upsert each segment by id — replaces existing
                // segment with the same id, appends otherwise.
                const next = [...m.segments];
                for (const seg of updates) {
                  const idx = next.findIndex((s) => s.id === seg.id);
                  if (idx >= 0) {
                    next[idx] = seg;
                  } else {
                    next.push(seg);
                  }
                }
                return { ...m, segments: next };
              }),
            );
          } else if (segmentType === 'text' && sawTextDeltaRef.current) {
            // LlmResponseComplete: text_delta already streamed the
            // body, so the raw text would be a duplicate. Only pick
            // up *new* `<think>…</think>` regions that didn't
            // appear during streaming (e.g. a second-iteration
            // reasoning that arrived only in the final message).
            const parser = parserRef.current;
            if (!parser) return;
            const additions = parser.feedForFinalize(text);
            if (additions.length === 0) return;
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId ? { ...m, segments: [...m.segments, ...additions] } : m,
              ),
            );
          } else {
            const newSegment: MessageSegment = {
              id: crypto.randomUUID(),
              type: segmentType,
              content: text,
              toolName: metadata?.tool_name,
              iteration: metadata?.iteration,
            };
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId ? { ...m, segments: [...m.segments, newSegment] } : m,
              ),
            );
          }
        }
        break;
      }

      case 'artifactUpdate': {
        if (!event.artifactUpdate) return;
        const { text, metadata } = extractPartWithMetadata(
          event.artifactUpdate.artifact.parts as unknown as ReadonlyArray<unknown> | undefined,
        );
        if (text) {
          const segmentType: SegmentType = metadata?.segment_type || 'text';
          const newSegment: MessageSegment = {
            id: crypto.randomUUID(),
            type: segmentType,
            content: text,
            toolName: metadata?.tool_name,
            iteration: metadata?.iteration,
          };
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId ? { ...m, segments: [...m.segments, newSegment] } : m,
            ),
          );
        }
        break;
      }

      case 'task': {
        if (!event.task) return;
        const taskState = event.task.status?.state;
        const state = taskState ? normalizeTaskState(taskStateToJSON(taskState)) : 'unknown';
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

  return (
    <div className="nt-chat">
      <div
        className="nt-chat__messages"
        data-testid="chat-messages"
        aria-live="polite"
        aria-relevant="additions text"
      >
        {messages.length === 0 && (
          <Card title="System" glow="green">
            <p>
              Welcome to <strong>Synthia</strong>. Type a message below to start an A2A task.
              Session: <code>{sessionId?.slice(0, 8)}</code>
            </p>
          </Card>
        )}
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`nt-chat__message nt-chat__message--${msg.role}`}
            data-role={msg.role}
            data-testid={`message-${msg.role}`}
            data-streaming={isStreaming && msg.role === 'assistant' && msg.status === 'working'}
          >
            <div className="nt-chat__message-meta">
              <span className="nt-chat__message-role">
                {msg.role === 'user' ? '> USER' : '> ASSISTANT'}
              </span>
              {msg.status && (
                <span className={`nt-chat__message-status status-${msg.status}`}>{msg.status}</span>
              )}
            </div>
            <div className="nt-chat__message-content">
              {msg.segments.map((segment) => (
                <SegmentView
                  key={segment.id}
                  segment={segment}
                  streaming={isStreaming && msg.role === 'assistant' && msg.status === 'working'}
                />
              ))}
            </div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      <form onSubmit={handleSubmit} className="nt-chat__form">
        <textarea
          className="nt-chat__input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
          rows={3}
          disabled={isStreaming}
          data-testid="chat-input"
          aria-label="Message input"
        />
        <Button type="submit" disabled={!input.trim() || isStreaming} data-testid="send-button">
          {isStreaming ? 'Streaming...' : 'Send'}
        </Button>
      </form>
    </div>
  );
}
