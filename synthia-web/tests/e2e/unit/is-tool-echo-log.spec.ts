/**
 * Live exercise of the `isToolEchoText` debug logging in
 * `src/lib/session-to-messages.ts`. The unit tests under
 * `session-to-messages.spec.ts` cover the function's return
 * value; this file covers the *log output* — every key
 * branch of the detector should emit a single `console.debug`
 * line with a recognisable payload.
 *
 * Strategy: load the real `SessionDetailPage` against a
 * mocked `/api/v1/sessions/:id` response, then capture every
 * `console.debug` event with prefix `[isToolEchoText]` via
 * `page.on('console', ...)`. The mock carries one assistant
 * `Message(agent)` per branch so the renderer's loop walks
 * all six paths in a single render. We then assert each
 * branch's distinctive log marker appeared exactly once.
 *
 * This runs in the Playwright chromium worker (real React
 * render, real DOM, real `console.debug` channel) — closer
 * to production behaviour than the existing function-level
 * unit tests, which run the function in isolation.
 */
import { expect, test, type ConsoleMessage } from '@playwright/test';
import type { SessionDetail, SessionTurn, SessionPart } from '../../../src/api/types';

/** Build a `Part::text` carrying a plain string. */
function textPart(text: string): SessionPart {
  return { text };
}

/** Build a `Part::data` carrying a structured JSON object. */
function dataPart(data: Record<string, unknown>): SessionPart {
  return { data: data as SessionPart['data'] };
}

/**
 * Build a `ROLE_AGENT` history message with the given parts.
 * The shape mirrors what the server emits on the wire.
 */
function agentMessage(parts: SessionPart[]): SessionTurn {
  return {
    messageId: `m-${Math.random().toString(36).slice(2, 10)}`,
    role: 'ROLE_AGENT',
    parts,
  };
}

/**
 * Build the mock `SessionDetail` the page reads on mount.
 * The wire-format type name is `SessionDetail` (kept stable
 * for the wire contract); the page renders the same
 * data on the `/sessions/:id` route today.
 * The `history` is intentionally crafted so the renderer
 * walks every branch of `isToolEchoText` in a single pass:
 *
 *   - m0 (real tool_use as Part::data — not an echo, but
 *     proves the renderer reaches the parts loop)
 *   - m1 (tool_use echo with `data:` prefix  → branch 1 + 4)
 *   - m2 (tool_result echo with `data:` prefix → branch 1 + 5)
 *   - m3 (plain prose → branch 2 — JSON.parse fails)
 *   - m4 (JSON array → branch 3 — not an object)
 *   - m5 (JSON object with neither tool keys → branch 6)
 *
 * Each message goes through `renderHistoryMessage` in order.
 */
const TASK_ID = '019fef0a-bc3a-7d72-b47a-b1a937960883';

const mockTaskDetail: SessionDetail = {
  id: TASK_ID,
  contextId: '019ffadd-4621-44d0-8c15-3d72ba0b0829',
  status: 'SESSION_STATE_COMPLETED',
  history: [
    agentMessage([
      dataPart({
        id: 'call_real_1',
        name: 'shell',
        input: { command: 'echo real' },
      }),
    ]),
    // Branch 1 + 4: SSE `data:` prefix + tool_use shape
    agentMessage([textPart('data: {"id":"call_echo_1","input":{"command":"echo hello"}}')]),
    // Branch 1 + 5: SSE `data:` prefix + tool_result shape
    agentMessage([
      textPart('data: {"tool_use_id":"call_echo_1","content":"hello\\n","is_error":false}'),
    ]),
    // Branch 2: plain prose — JSON.parse fails
    agentMessage([textPart('抱歉，我作为代码助手没有直接获取实时天气数据的工具/API。')]),
    // Branch 3: JSON array — not a plain object
    agentMessage([textPart('[1, 2, 3]')]),
    // Branch 6: JSON object missing both tool_use and tool_result keys
    agentMessage([textPart('{"foo":"bar"}')]),
  ],
  artifacts: [],
};

test.describe('isToolEchoText logging — live renderer exercise', () => {
  test('every key branch emits its distinctive console.debug', async ({ page }) => {
    // Intercept the task fetch and return the mock. The
    // Vite proxy normally routes `/api/v1/sessions/...` to
    // 8080; we short-circuit it before it leaves the page.
    await page.route(
      (url) =>
        url.pathname === `/api/v1/sessions/${TASK_ID}` &&
        url.hostname === 'localhost' &&
        url.port === '5173',
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(mockTaskDetail),
        });
      },
    );

    // Capture every browser console message before we
    // trigger the render. Filter for the [isToolEchoText]
    // namespace so unrelated logs (React DevTools, vite
    // HMR, etc.) don't pollute the assertion.
    const captured: string[] = [];
    page.on('console', (msg: ConsoleMessage) => {
      if (msg.type() !== 'debug') return;
      const text = msg.text();
      if (text.startsWith('[isToolEchoText]')) {
        captured.push(text);
      }
    });

    // Load the session detail page. The page's useEffect
    // fires `api.get('/api/v1/sessions/:id')` which our route
    // intercepts, then `renderHistoryMessage` walks each
    // history entry and calls `isToolEchoText` per text
    // part. By the time the heading appears, every branch
    // has been walked.
    await page.goto(`/sessions/${TASK_ID}`);

    // Wait for the History card to render so we know the
    // render loop has finished walking history. The card
    // title is rendered as the first child of the History
    // Card; matching on a stable string avoids relying on
    // CSS class names that may shift between revisions.
    await expect(page.getByRole('heading', { name: /History/i }).first()).toBeVisible({
      timeout: 10_000,
    });

    // Dump the captured log lines so a debugging session can
    // see exactly which branch fired when the test fails.
    if (captured.length === 0) {
      throw new Error(
        'expected at least one [isToolEchoText] console.debug, got none. ' +
          'Check that the mock task detail was actually rendered.',
      );
    }

    // Console-output the captured lines (test passed so
    // it's the easiest way for a future maintainer to
    // see, at a glance, what each branch produced when
    // they tweak the detector).
    for (const line of captured) {
      console.log(`captured: ${line}`);
    }

    // Each branch's distinctive log prefix. The numbers
    // come straight from the comment markers in
    // `isToolEchoText`.
    const expectedPrefixes = [
      // Branch 1 fires whenever the SSE `data:` prefix is
      // stripped. Two messages have the prefix (m1, m2) →
      // two log lines.
      '[isToolEchoText] stripped SSE `data:` prefix',
      // Branch 4 — tool_use echo matched (m1).
      '[isToolEchoText] ECHO DETECTED — tool_use shape matched',
      // Branch 5 — tool_result echo matched (m2).
      '[isToolEchoText] ECHO DETECTED — tool_result shape matched',
      // Branch 2 — JSON.parse failed (m3, plain prose).
      '[isToolEchoText] not a tool echo — JSON.parse failed',
      // Branch 3 — parsed value is not an object (m4, array).
      '[isToolEchoText] not a tool echo — parsed value is not an object',
      // Branch 6 — JSON object with neither tool_use nor
      // tool_result keys (m5).
      '[isToolEchoText] not a tool echo — JSON object missing both tool_use and tool_result keys',
    ];

    for (const prefix of expectedPrefixes) {
      const matches = captured.filter((line) => line.startsWith(prefix));
      expect(
        matches.length,
        `expected at least one log line starting with:\n  ${prefix}\n` +
          `captured lines:\n  ${captured.join('\n  ')}`,
      ).toBeGreaterThanOrEqual(1);
    }

    // Spot-check that branch 4 carries the toolUseId we
    // put in the mock, so future refactors that drop the
    // correlation field break this assertion explicitly.
    const branch4 = captured.find((l) =>
      l.startsWith('[isToolEchoText] ECHO DETECTED — tool_use shape matched'),
    );
    expect(branch4).toBeDefined();
    expect(branch4!).toContain('call_echo_1');

    // Same for branch 5 — tool_use_id correlation.
    const branch5 = captured.find((l) =>
      l.startsWith('[isToolEchoText] ECHO DETECTED — tool_result shape matched'),
    );
    expect(branch5).toBeDefined();
    expect(branch5!).toContain('call_echo_1');
  });
});
