/**
 * Unit tests for `sse-harness.ts`.
 *
 * Coverage (per `tasks.md §4.9`):
 *   (a) normal event sequence — server emits a few well-formed events,
 *       harness collects them in order
 *   (b) half-packet parsing — server splits a single SSE message across
 *       two `write()` calls at an arbitrary byte boundary, harness must
 *       reassemble the message correctly
 *   (c) `event: error` is captured as a regular SSE event (not thrown) —
 *       the harness must NOT interpret an SSE `error` field as a transport
 *       failure, per ARBITRATION.md (servers signal error state via the
 *       `event: error` channel)
 *   (d) `close()` cancels the reader — verified by racing `close()` against
 *       a server that never closes on its own
 */

import { describe, expect, it } from 'vitest';
import { createServer, type Server, type ServerResponse } from 'node:http';
import { type AddressInfo } from 'node:net';
import { subscribeAndCapture, type CapturedStream } from './sse-harness';

interface SseHandle {
  res: ServerResponse;
  /** End the response stream (triggers harness `done`). */
  finish: () => void;
}

interface SseFixture {
  baseUrl: string;
  cap: CapturedStream;
  handle: SseHandle;
  /** Run `body` after the SSE connection is open, then auto-cleanup. */
  withConnection: <T>(body: (h: SseHandle) => Promise<T> | T) => Promise<T>;
  /** Manually tear down (called automatically at end of `withConnection`). */
  teardown: () => Promise<void>;
}

async function startFixture(): Promise<SseFixture> {
  let resolveHandle!: (h: SseHandle) => void;
  const handleReady = new Promise<SseHandle>((r) => (resolveHandle = r));
  const server: Server = createServer();
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  server.on('request', (_req, res) => {
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
    });
    // Write a no-op SSE comment line IMMEDIATELY to flush the response
    // headers — otherwise `await fetch()` in the client blocks until the
    // first `write()` (or `end()`), which is deadlocked against the
    // test handler that runs *after* `subscribeAndCapture` resolves.
    res.write(': open\n\n');
    resolveHandle({
      res,
      finish: () => res.end(),
    });
  });
  const addr = server.address() as AddressInfo;
  const baseUrl = `http://127.0.0.1:${addr.port}`;
  const cap = await subscribeAndCapture(`${baseUrl}/`);
  const handle = await handleReady;

  const teardown = async () => {
    cap.close();
    await cap.done.catch(() => {
      // done may reject if the transport died; ignore.
    });
    await new Promise<void>((r) => server.close(() => r()));
  };

  const withConnection = async <T>(body: (h: SseHandle) => Promise<T> | T): Promise<T> => {
    const out = await body(handle);
    await teardown();
    return out;
  };

  return { baseUrl, cap, handle, withConnection, teardown };
}

describe('sse-harness', () => {
  describe('(a) normal event sequence', () => {
    it('parses a sequence of well-formed events', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        h.res.write('event: ping\ndata: hello\n\n');
        h.res.write('event: result\ndata: {"ok":true}\nid: 42\n\n');
        h.res.write('data: bare message\n\n');
        // Poll for all 3 events then close.
        await waitForEvents(fx.cap, 3, 500);
      });
      expect(fx.cap.events).toEqual([
        { event: 'ping', data: 'hello' },
        { event: 'result', data: '{"ok":true}', id: '42' },
        { event: 'message', data: 'bare message' },
      ]);
    });

    it('joins multi-line `data:` fields with LF per spec', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        h.res.write('data: line one\ndata: line two\ndata: line three\n\n');
        await waitForEvents(fx.cap, 1, 500);
      });
      expect(fx.cap.events).toEqual([{ event: 'message', data: 'line one\nline two\nline three' }]);
    });

    it('ignores comment lines starting with ":"', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        h.res.write(': this is a comment\n');
        h.res.write('data: real payload\n\n');
        await waitForEvents(fx.cap, 1, 500);
      });
      expect(fx.cap.events).toEqual([{ event: 'message', data: 'real payload' }]);
    });
  });

  describe('(b) half-packet parsing', () => {
    it('reassembles a message split across chunks mid-event', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        // Single SSE event: `event: split\ndata: payload\n\n`
        // Split at byte 11 (inside the data: line) and again at
        // byte 18 (inside the blank line) to exercise the buffer
        // reassembly paths.
        const full = 'event: split\ndata: payload\n\n';
        h.res.write(full.slice(0, 11));
        await new Promise((r) => setImmediate(r));
        h.res.write(full.slice(11, 18));
        await new Promise((r) => setImmediate(r));
        h.res.write(full.slice(18));
        await waitForEvents(fx.cap, 1, 500);
      });
      expect(fx.cap.events).toEqual([{ event: 'split', data: 'payload' }]);
    });

    it('reassembles when chunk splits in the middle of the event-name field', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        const full = 'event: status\ndata: ok\n\n';
        h.res.write(full.slice(0, 5));
        await new Promise((r) => setImmediate(r));
        h.res.write(full.slice(5));
        await waitForEvents(fx.cap, 1, 500);
      });
      expect(fx.cap.events).toEqual([{ event: 'status', data: 'ok' }]);
    });

    it('reassembles a UTF-8 codepoint split across two chunks', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        // 4-byte sequence F0 9F 98 80 = 😀. We split right after the
        // first byte of the emoji to prove the TextDecoder's
        // `stream: true` correctly holds the partial multi-byte
        // sequence and does not produce U+FFFD.
        const full = 'data: hi \u{1F600}\n\n';
        const bytes = Buffer.from(full, 'utf8');
        const emojiStart = bytes.indexOf(0xf0, 8);
        const split = emojiStart + 1;
        h.res.write(bytes.subarray(0, split));
        await new Promise((r) => setImmediate(r));
        h.res.write(bytes.subarray(split));
        await waitForEvents(fx.cap, 1, 500);
      });
      expect(fx.cap.events).toEqual([{ event: 'message', data: 'hi \u{1F600}' }]);
    });
  });

  describe('(c) `event: error` is captured, not thrown', () => {
    it('records an `event: error` SSE message as a regular event', async () => {
      const fx = await startFixture();
      await fx.withConnection(async (h) => {
        h.res.write('event: error\ndata: {"code":"E_BAD","msg":"nope"}\n\n');
        h.res.write('event: complete\ndata: done\n\n');
        await waitForEvents(fx.cap, 2, 500);
      });
      expect(fx.cap.events).toEqual([
        { event: 'error', data: '{"code":"E_BAD","msg":"nope"}' },
        { event: 'complete', data: 'done' },
      ]);
    });
  });

  describe('(d) close() cancels the reader', () => {
    it('causes done to resolve promptly when the server never closes', async () => {
      const fx = await startFixture();
      // Intentionally never call h.finish() — keep the stream open.
      fx.handle.res.write('event: first\ndata: one\n\n');
      await waitForEvents(fx.cap, 1, 500);
      expect(fx.cap.events).toEqual([{ event: 'first', data: 'one' }]);

      const t0 = Date.now();
      const close = fx.cap.close;
      const done = fx.cap.done;
      close();
      await done;
      expect(Date.now() - t0).toBeLessThan(1000);
      // Cleanup.
      await fx.teardown();
    });
  });
});

/**
 * Poll the harness `events` array until it contains at least `n` items
 * or the timeout elapses. Resolves on success, rejects on timeout.
 */
async function waitForEvents(cap: CapturedStream, n: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (cap.events.length >= n) return;
    await new Promise((r) => setTimeout(r, 5));
  }
  throw new Error(
    `[sse-harness test] timeout: expected >= ${n} events, got ${cap.events.length}: ` +
      JSON.stringify(cap.events),
  );
}
