import { test, expect } from '@playwright/test';
import { SessionsPage } from '../pages/sessions.page';

/**
 * Layer 1 — Session detail page UI tests.
 *
 * Regression coverage for the `/sessions/:id` route. The page
 * the data-testid namespace
 * here is `session-detail-*` / `session-*`. The shared CSS
 * classes (`nt-session__artifact-*`) are also renamed to
 * match — the underlying wire types use `Session*`
 * contract) but the UI surfaces everything as "session".
 */
test.describe('Session detail page', () => {
  test('renders not-found state for an unknown session id', async ({ page }) => {
    await page.goto('/sessions/this-id-does-not-exist');
    // Either the API returned 404 (rendered as "Error" card) or
    // the page shows "Not found" — both are acceptable terminal
    // states. Asserting on the back link keeps the test stable.
    await expect(page.getByTestId('session-detail-back')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('h1', { hasText: 'Session' })).toBeVisible();
  });

  test('back link returns to the sessions list', async ({ page }) => {
    await page.goto('/sessions/any-id');
    const back = page.getByTestId('session-detail-back');
    await expect(back).toBeVisible({ timeout: 10000 });
    await back.click();
    await expect(page).toHaveURL(/\/sessions(?:\?.*)?$/);
    await expect(page.locator('h1', { hasText: 'Sessions' })).toBeVisible();
  });

  test('renders detail fields for a session returned by the list endpoint', async ({ page }) => {
    // Pick the first session id from the listing page so the
    // detail route has data to render. Skip if the workspace
    // has none (the test is workspace-state dependent).
    const sessions = new SessionsPage(page);
    await sessions.goto();

    // The SessionsPage renders each card with a "View" link or
    // similar affordance. We probe for the common pattern: a
    // link to /sessions/<id>. If none exist, skip.
    const firstSessionLink = page
      .locator('a[href^="/sessions/"]')
      .filter({ hasNot: page.getByTestId('session-detail-back') })
      .first();
    const href = await firstSessionLink.getAttribute('href').catch(() => null);
    test.skip(!href, 'no sessions on this workspace to drill into');

    await firstSessionLink.click();
    await expect(page).toHaveURL(/\/sessions\/[^/]+$/);

    // Detail page chrome.
    await expect(page.locator('h1', { hasText: 'Session' })).toBeVisible({
      timeout: 10000,
    });
    await expect(page.getByTestId('session-detail-back')).toBeVisible();

    // At least one of the rendered cards (summary, history,
    // artifacts) must be present. We assert on the most stable
    // marker — the "Session " card title — rather than on
    // optional fields.
    // The summary card title is rendered as an `<h3>` (Radix Card
    // wrapper) and reads `Session <prefix>`.
    await expect(page.locator('main h3', { hasText: /^Session / })).toBeVisible({
      timeout: 10000,
    });
  });

  test('pairs tool_call and tool_result artifacts by tool_use_id', async ({ page }) => {
    // Legacy fallback path: `task.artifacts` carries tool calls
    // and results with a `metadata.kind` discriminator. This
    // path exists for sessions completed before the
    // `Task.history`-based wire was wired up; new sessions
    // route tool turns through `Message(agent) + Part::data`
    // (no `kind` discriminator). Pin the
    // legacy pairing-by-`tool_use_id` behaviour so old sessions
    // remain readable.
    //
    // Mock the session detail endpoint so the test does not
    // depend on workspace state. Two artifacts sharing a
    // tool_use_id (one tool_call, one tool_result) must be
    // rendered as a single grouped block with both the call
    // sub-block and the result sub-block visible.
    //
    // The CSS class names `nt-session__artifact-*` are kept
    // (shared with the live chat page) so the chat-style
    // rendering remains byte-identical.
    await page.route('**/api/v1/sessions/test-tool-pair', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-tool-pair',
          status: 'completed',
          context_id: 'ctx-1',
          history: [],
          artifacts: [
            {
              attachmentId: 'attachment-call-1',
              parts: [{ text: '{"command":"ls -la"}' }],
              metadata: {
                kind: 'tool_call',
                tool_use_id: 'call-1',
                tool_name: 'shell',
              },
            },
            {
              attachmentId: 'attachment-result-1',
              parts: [{ text: 'exit: 0\n\nstdout:\nhello world' }],
              metadata: {
                kind: 'tool_result',
                tool_use_id: 'call-1',
                is_error: false,
              },
            },
          ],
        }),
      });
    });

    await page.goto('/sessions/test-tool-pair');

    // Header carries the tool name so the user can identify the
    // paired block at a glance.
    const block = page.getByTestId('session-attachment-call-1');
    await expect(block).toBeVisible({ timeout: 10000 });
    await expect(block).toContainText('工具 · shell');

    // Both sub-blocks must render — proving call + result are
    // grouped into the same tool_use_id rather than split
    // across two separate cards.
    await expect(block.locator('.nt-session__artifact-call')).toBeVisible();
    await expect(block.locator('.nt-session__artifact-result')).toBeVisible();

    // JSON in the call block must be pretty-printed rather
    // than rendered as a raw escaped string.
    const callPre = block.locator('.nt-session__artifact-call pre').first();
    await expect(callPre).toContainText('"command"');
    await expect(callPre).toContainText('"ls -la"');
  });

  test('renders error badge when tool_result has is_error', async ({ page }) => {
    // Legacy fallback path (see previous test's comment for
    // the background). The session detail endpoint returns two
    // legacy artifacts; the frontend must surface `is_error:
    // true` as a red badge on the tool block so callers can
    // tell a failing tool from a successful one.
    await page.route('**/api/v1/sessions/test-tool-error', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-tool-error',
          status: 'completed',
          context_id: 'ctx-1',
          history: [],
          artifacts: [
            {
              attachmentId: 'attachment-call-2',
              parts: [{ text: '{"command":"false"}' }],
              metadata: {
                kind: 'tool_call',
                tool_use_id: 'call-2',
                tool_name: 'shell',
              },
            },
            {
              attachmentId: 'attachment-result-2',
              parts: [{ text: 'exit: 1\n\nstderr:\nboom' }],
              metadata: {
                kind: 'tool_result',
                tool_use_id: 'call-2',
                is_error: true,
              },
            },
          ],
        }),
      });
    });

    await page.goto('/sessions/test-tool-error');
    const block = page.getByTestId('session-attachment-call-2');
    await expect(block).toBeVisible({ timeout: 10000 });
    await expect(block.locator('.nt-session__artifact-error')).toBeVisible();
  });

  test('shows history-empty hint when history array is empty', async ({ page }) => {
    await page.route('**/api/v1/sessions/test-no-history', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-no-history',
          status: 'completed',
          context_id: 'ctx-1',
          history: [],
          artifacts: [],
        }),
      });
    });

    await page.goto('/sessions/test-no-history');
    await expect(page.getByTestId('session-history-empty')).toBeVisible({
      timeout: 10000,
    });
  });

  test('Continue-in-chat link routes to /chat/:contextId', async ({ page }) => {
    // Mock the session detail endpoint so the test owns the
    // context_id it asserts on.
    await page.route('**/api/v1/sessions/test-continue', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-continue',
          status: 'completed',
          context_id: 'ctx-existing-123',
          history: [],
          artifacts: [],
        }),
      });
    });

    await page.goto('/sessions/test-continue');

    const link = page.getByTestId('session-detail-continue-chat');
    await expect(link).toBeVisible({ timeout: 10000 });
    await expect(link).toHaveAttribute('href', '/chat/ctx-existing-123');

    await link.click();
    await expect(page).toHaveURL(/\/chat\/ctx-existing-123$/);
  });

  test('renders user prompt and agent text from session history', async ({ page }) => {
    // The backend persists the full conversation into
    // `task.history` — user prompts, agent text, and
    // tool_call / tool_result events. The detail page now
    // reuses the chat-style renderer so the user sees the
    // same `> USER` / `> ASSISTANT` cards, status pill,
    // and tool_block layout as the live `/chat/:sessionId`
    // page. This test pins that the History card maps a
    // user prompt to a `.nt-chat__message--user` row with
    // the user's text inside, an agent text entry to a
    // `.nt-chat__message--assistant` row, and a paired
    // tool_call / tool_result pair to a single
    // `.nt-chat__segment--tool_block` with both the call
    // sub-block and the result sub-block.
    await page.route('**/api/v1/sessions/test-with-history', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-with-history',
          status: 'completed',
          context_id: 'ctx-history-1',
          history: [
            {
              messageId: 'm-user',
              role: 'ROLE_USER',
              parts: [{ text: 'hello world' }],
            },
            {
              messageId: 'm-agent-text',
              role: 'ROLE_AGENT',
              parts: [{ text: 'hi from the assistant' }],
            },
            {
              messageId: 'm-agent-tool-call',
              role: 'ROLE_AGENT',
              parts: [
                {
                  data: {
                    id: 'call-history-1',
                    name: 'shell',
                    input: { command: 'echo hi' },
                  },
                },
              ],
            },
            {
              messageId: 'm-agent-tool-result',
              role: 'ROLE_AGENT',
              parts: [
                {
                  data: {
                    tool_use_id: 'call-history-1',
                    content: 'hi\n',
                    is_error: false,
                  },
                },
              ],
            },
          ],
          artifacts: [],
        }),
      });
    });

    await page.goto('/sessions/test-with-history');

    // The empty-hint must NOT appear.
    await expect(page.getByTestId('session-history-empty')).toHaveCount(0);

    // The user message card must render with the chat-style
    // role border and the user's text inside.
    const userMessage = page.locator('.nt-chat__message--user').first();
    await expect(userMessage).toBeVisible({ timeout: 10000 });
    await expect(userMessage.locator('.nt-chat__message-role')).toHaveText('> USER');
    await expect(userMessage).toContainText('hello world');

    // The assistant message must render with the chat-style
    // role border and a status pill on the chat-style
    // metadata strip.
    const assistantMessage = page.locator('.nt-chat__message--assistant').first();
    await expect(assistantMessage).toBeVisible();
    await expect(assistantMessage.locator('.nt-chat__message-role')).toHaveText('> ASSISTANT');
    await expect(assistantMessage.locator('.nt-chat__message-status')).toContainText('completed');

    // The text deltas must render with markdown fidelity
    // — the chat-style renderer passes every text segment
    // through `ReactMarkdown`, so the plain "hi from the
    // assistant" body is visible inside the assistant
    // message.
    await expect(assistantMessage).toContainText('hi from the assistant');

    // The paired tool_call / tool_result must render as a
    // single `.nt-chat__segment--tool_block` with both the
    // call sub-block and the result sub-block visible —
    // mirroring the live chat page. Tool blocks start
    // collapsed; click the toggle to expose the call /
    // result sub-blocks for inspection.
    const toolBlock = page.locator('.nt-chat__segment--tool_block').first();
    await expect(toolBlock).toBeVisible();
    await expect(toolBlock).toContainText('工具 · shell');
    await toolBlock.locator('.chat-toggle').click();
    await expect(toolBlock.locator('.nt-chat__tool-block-call')).toBeVisible();
    await expect(toolBlock.locator('.nt-chat__tool-block-result')).toBeVisible();
    const callPre = toolBlock.locator('.nt-chat__tool-block-call pre').first();
    await expect(callPre).toContainText('"command"');
    await expect(callPre).toContainText('"echo hi"');
    await expect(toolBlock.locator('.nt-chat__tool-block-result pre').first()).toContainText('hi');

    // Regression: the chat-style renderer must NOT also
    // re-paint each Part::data payload as `data: {...}`
    // text (the previous fallback used to dump the raw
    // JSON for every unrecognised part). With the chat
    // renderer the only `<pre>` blocks in the document are
    // those inside tool sub-blocks, and none of them
    // contain a `data: {...}` prefix.
    const dataPrefails = await page.evaluate(
      () =>
        Array.from(document.querySelectorAll('pre')).filter((p) =>
          (p.textContent ?? '').startsWith('data: {'),
        ).length,
    );
    expect(dataPrefails).toBe(0);
  });

  test('flags tool_result errors when history carries is_error', async ({ page }) => {
    // A failing tool must surface the red error state on the
    // paired tool_block the same way the live chat page does.
    // The chat-style renderer paints the result sub-block red
    // when the reconstructed `tool_block` carries
    // `is_error: true` — this test pins that behaviour for
    // the session-detail History card.
    await page.route('**/api/v1/sessions/test-history-error', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-history-error',
          status: 'completed',
          context_id: 'ctx-history-err',
          history: [
            {
              messageId: 'm-user',
              role: 'ROLE_USER',
              parts: [{ text: 'try this' }],
            },
            {
              messageId: 'm-call',
              role: 'ROLE_AGENT',
              parts: [
                {
                  data: {
                    id: 'call-err',
                    name: 'shell',
                    input: { command: 'false' },
                  },
                },
              ],
            },
            {
              messageId: 'm-result',
              role: 'ROLE_AGENT',
              parts: [
                {
                  data: {
                    tool_use_id: 'call-err',
                    content: 'exit: 1\nstderr:\nboom',
                    is_error: true,
                  },
                },
              ],
            },
          ],
          artifacts: [],
        }),
      });
    });

    await page.goto('/sessions/test-history-error');

    // Find the erroring tool block by its tool name label.
    const toolBlock = page.locator('.nt-chat__segment--tool_block', {
      hasText: '工具 · shell',
    });
    await expect(toolBlock).toBeVisible({ timeout: 10000 });

    // Expand the tool block so the errored result sub-block
    // is exposed (tool blocks start collapsed).
    await toolBlock.locator('.chat-toggle').click();

    // The errored result sub-block must be marked with the
    // chat-style error variant so the user can tell a failing
    // tool from a successful one.
    await expect(toolBlock.locator('.nt-chat__tool-block-result--error')).toBeVisible();
    await expect(toolBlock.locator('.nt-chat__tool-block-result--error pre').first()).toContainText(
      'boom',
    );
  });
});
