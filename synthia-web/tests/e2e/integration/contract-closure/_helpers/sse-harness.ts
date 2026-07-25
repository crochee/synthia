/**
 * Protocol-neutral SSE (Server-Sent Events) test harness.
 *
 * Why a custom parser (and not `eventsource` / `event-source-polyfill`)?
 * ---------------------------------------------------------------
 * - This harness is consumed by *Playwright* spec files that already own
 *   the network lifecycle (via `page.goto` / `page.request`).
 * - We need byte-level control over chunk boundaries to write the
 *   half-packet regression test required by `tasks.md §4.9`.
 * - ARBITRATION.md forbids third-party SSE protocol assumptions
 *   (we do not know the upstream event-name vocabulary ahead of time),
 *   so the parser is intentionally generic: only the on-the-wire framing
 *   (CRLF / blank-line / `event:` / `data:` / `id:` / `retry:`) is decoded.
 *
 * Spec we follow: <https://html.spec.whatwg.org/multipage/server-sent-events.html>
 *
 * Public surface (per `tasks.md §4.9`):
 *   - `subscribeAndCapture(url, options?)`  → returns a `CapturedStream`
 *   - `SSEEvent` / `CapturedStream` types
 *   - no defaults; callers pass `signal` themselves if they need abort
 */

export interface SSEEvent {
  /** Value of the `event:` field, or `"message"` if absent. */
  event: string;
  /** Concatenated `data:` lines, joined by "\n" per the spec. */
  data: string;
  /** Value of the `id:` field, if any. */
  id?: string;
}

export interface CapturedStream {
  /** Live array; appended to as the underlying reader yields more chunks. */
  events: SSEEvent[];
  /** Cancel the underlying reader. Idempotent and safe to call multiple times. */
  close: () => void;
  /**
   * Promise that resolves when the SSE stream terminates (server closes
   * the connection). Useful in tests that need to `await` the natural
   * end of a fixture before asserting.
   */
  done: Promise<void>;
}

export interface SubscribeOptions {
  /** Optional `AbortSignal` to cancel the fetch itself. */
  signal?: AbortSignal;
  /** Extra request headers (e.g. `Accept: text/event-stream`). */
  headers?: Record<string, string>;
}

interface MutableEvent extends SSEEvent {
  // Local scratch — we promote to a plain SSEEvent on dispatch.
}

/**
 * Subscribe to an SSE endpoint and accumulate every parsed event into
 * `result.events` until the connection closes or `close()` is called.
 *
 * Implementation note (per design.md D2):
 *   - We intentionally use the WHATWG Streams `ReadableStreamDefaultReader`
 *     obtained from `fetch().body.getReader()`. No polyfill, no third-party
 *     SSE library, no `EventSource` (which would buffer + auto-reconnect and
 *     hide chunk-boundary bugs from the test).
 */
export async function subscribeAndCapture(
  url: string,
  options: SubscribeOptions = {},
): Promise<CapturedStream> {
  const response = await fetch(url, {
    signal: options.signal,
    headers: {
      Accept: 'text/event-stream',
      ...(options.headers ?? {}),
    },
  });

  if (!response.ok) {
    throw new Error(`[sse-harness] ${url} returned ${response.status} ${response.statusText}`);
  }
  if (!response.body) {
    throw new Error(`[sse-harness] ${url} has no response body`);
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder('utf-8');

  const events: SSEEvent[] = [];
  let buffer = '';
  let scratch: MutableEvent | null = null;

  const dispatch = () => {
    if (scratch === null) return;
    if (scratch.data.length > 0 || scratch.event !== 'message') {
      const frozen: SSEEvent = {
        event: scratch.event,
        data: scratch.data,
        ...(scratch.id !== undefined ? { id: scratch.id } : {}),
      };
      events.push(frozen);
    }
    scratch = null;
  };

  const processChunk = (chunk: string) => {
    // The buffer may carry partial lines from a previous chunk. We always
    // re-append, then split on LF and process line-by-line; a CR at end of
    // line is tolerated per the spec.
    buffer += chunk;
    let lineStart = 0;
    for (let i = 0; i < buffer.length; i++) {
      const ch = buffer[i];
      if (ch !== '\n') continue;
      const rawLine = buffer.slice(lineStart, i);
      // Strip trailing \r (CRLF).
      const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine;
      handleLine(line);
      lineStart = i + 1;
    }
    buffer = buffer.slice(lineStart);
  };

  const handleLine = (line: string) => {
    // Blank line = event boundary. Per the spec, a blank line dispatches
    // the current event (even if no `data:` was present).
    if (line === '') {
      dispatch();
      return;
    }
    // Lines starting with `:` are comments — ignored.
    if (line.startsWith(':')) return;
    const colonIdx = line.indexOf(':');
    let field: string;
    let value: string;
    if (colonIdx === -1) {
      field = line;
      value = '';
    } else {
      field = line.slice(0, colonIdx);
      // Per spec: skip the first character of the value if it is a space.
      value =
        colonIdx + 1 < line.length && line[colonIdx + 1] === ' '
          ? line.slice(colonIdx + 2)
          : line.slice(colonIdx + 1);
    }
    if (field === 'event') {
      if (scratch === null) scratch = { event: 'message', data: '' };
      scratch.event = value;
    } else if (field === 'data') {
      if (scratch === null) scratch = { event: 'message', data: '' };
      // Per spec: append with a "\n" between consecutive `data:` lines.
      scratch.data = scratch.data === '' ? value : `${scratch.data}\n${value}`;
    } else if (field === 'id') {
      if (scratch === null) scratch = { event: 'message', data: '' };
      scratch.id = value;
    }
    // `retry:` is intentionally ignored — the harness does not auto-reconnect.
  };

  let cancelled = false;
  const done = (async () => {
    try {
      while (!cancelled) {
        const { value, done: readerDone } = await reader.read();
        if (readerDone) break;
        if (value === undefined) continue;
        const text = decoder.decode(value, { stream: true });
        processChunk(text);
      }
      // Flush any trailing event that wasn't terminated by a blank line.
      if (buffer.length > 0) {
        const tail = buffer.endsWith('\r') ? buffer.slice(0, -1) : buffer;
        handleLine(tail);
        buffer = '';
      }
      dispatch();
    } finally {
      cancelled = true;
    }
  })();

  return {
    events,
    close: () => {
      if (cancelled) return;
      cancelled = true;
      void reader.cancel().catch(() => {
        // Swallow: the reader may already be released after natural close.
      });
    },
    done,
  };
}
