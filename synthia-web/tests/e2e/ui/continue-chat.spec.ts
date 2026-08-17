import { test, expect } from '@playwright/test';

/**
 * Layer 1 — Task → Chat continuation flow (regression).
 *
 * Regression coverage for the user-visible "still does not work"
 * symptom: clicking "在 chat 中继续此 session" from the task
 * detail page dropped the user into an empty chat instead of
 * restoring the prior conversation tied to the task's
 * `context_id`. The root cause was a ChatPage effect-ordering bug
 * — its persist effect clobbered the stored messages with `[]`
 * on the first commit. The TaskDetailPage routing is now
 * covered end-to-end here.
 */
test.describe('Task → chat continuation', () => {
  test('clicking continue-in-chat restores the prior conversation', async ({ page }) => {
    page.on('console', (msg) => {
      if (msg.type() === 'log') console.log('[browser]', msg.text());
    });

    // 1. Pre-seed localStorage as if the user previously chatted
    //    in this exact context_id. This simulates a session
    //    recovered after a reload.
    const sessionId = 'ctx-existing-789';
    await page.addInitScript(
      ({ sessionId, messages }) => {
        localStorage.setItem(`synthia.messages.${sessionId}`, JSON.stringify(messages));
      },
      {
        sessionId,
        messages: [
          {
            id: 'm1',
            role: 'user',
            segments: [{ id: 's1', type: 'text', content: 'earlier user turn' }],
          },
          {
            id: 'm2',
            role: 'assistant',
            segments: [{ id: 's2', type: 'text', content: 'earlier assistant turn' }],
            status: 'completed',
          },
        ],
      },
    );

    // 2. Mock the task detail endpoint so we can assert on a
    //    deterministic context_id.
    await page.route('**/api/v1/tasks/test-continue-flow', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'test-continue-flow',
          status: 'completed',
          context_id: sessionId,
          history: [],
          artifacts: [],
        }),
      });
    });

    // 3. Land on the task detail page and click the link.
    await page.goto('/tasks/test-continue-flow');
    const link = page.getByTestId('task-detail-continue-chat');
    await expect(link).toBeVisible({ timeout: 10000 });
    await link.click();
    await expect(page).toHaveURL(/\/chat\/ctx-existing-789$/);

    // 4. The prior conversation must be visible — proving the
    //    task.context_id is used as the chat sessionId and that
    //    the localStorage restore survived the SPA navigation.
    await expect(page.getByTestId('message-user').first()).toBeVisible({
      timeout: 10000,
    });
    await expect(
      page.getByTestId('message-user').first().getByText('earlier user turn'),
    ).toBeVisible();
    await expect(
      page.getByTestId('message-assistant').first().getByText('earlier assistant turn'),
    ).toBeVisible();
  });

  test('reconstructs user + agent text from task history when localStorage is empty', async ({
    page,
  }) => {
    // The server now persists the full conversation into
    // `task.history`. When the user clicks "继续 chat" with
    // an empty localStorage, the reconstructor must seed the
    // chat store with one user message and one assistant
    // message (the latter containing a tool_block whose call
    // and result sides are paired by tool_use_id).
    const sessionId = 'ctx-history-reconstruct';
    await page.route('**/api/v1/tasks/task-with-history', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'task-with-history',
          status: 'completed',
          context_id: sessionId,
          history: [
            {
              messageId: 'm-user',
              role: 'ROLE_USER',
              parts: [{ text: 'first user turn' }],
            },
            {
              messageId: 'm-agent-text',
              role: 'ROLE_AGENT',
              parts: [{ text: 'first assistant turn' }],
            },
            {
              messageId: 'm-call',
              role: 'ROLE_AGENT',
              parts: [
                {
                  data: {
                    id: 'call-rc-1',
                    name: 'shell',
                    input: { command: 'echo hi' },
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
                    tool_use_id: 'call-rc-1',
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

    await page.goto('/tasks/task-with-history');
    await page.getByTestId('task-detail-continue-chat').click();
    await expect(page).toHaveURL(new RegExp(`/chat/${sessionId}$`));

    // Both the user message and the assistant turn render in
    // the chat view, proving history is the source of truth.
    const user = page.getByTestId('message-user').first();
    const assistant = page.getByTestId('message-assistant').first();
    await expect(user).toBeVisible({ timeout: 10000 });
    await expect(user.getByText('first user turn')).toBeVisible();
    await expect(assistant).toBeVisible();
    await expect(assistant.getByText('first assistant turn')).toBeVisible();
    await expect(assistant.getByText('shell')).toBeVisible();
  });

  test('reconstructs assistant tool blocks from task artifacts when localStorage is empty', async ({
    page,
  }) => {
    // Legacy fallback path: tool calls/results carried via
    // `task.artifacts` with a `metadata.kind` discriminator. This
    // path exists for tasks completed before the
    // `Task.history`-based wire was wired up; new tasks route
    // tool turns through `Message(agent) + Part::data` per
    // A2A v1.0 §3.7 (no `kind` discriminator). Pin the legacy
    // path so old tasks remain readable.
    //
    // The task is loaded AFTER the click in the real flow, so the
    // page already has `task` populated by the time the user can
    // interact. We mock the detail endpoint and click the link.
    const sessionId = 'ctx-reconstruct-1';
    await page.route('**/api/v1/tasks/task-with-tools', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'task-with-tools',
          status: 'completed',
          context_id: sessionId,
          history: [],
          artifacts: [
            {
              artifactId: 'artifact-call-1',
              parts: [{ text: '{"command":"ls -la"}' }],
              metadata: {
                kind: 'tool_call',
                tool_use_id: 'call-1',
                tool_name: 'shell',
              },
            },
            {
              artifactId: 'artifact-result-1',
              parts: [{ text: 'exit: 0\n\nstdout:\nhi' }],
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

    await page.goto('/tasks/task-with-tools');
    await page.getByTestId('task-detail-continue-chat').click();
    await expect(page).toHaveURL(new RegExp(`/chat/${sessionId}$`));

    // No prior localStorage was seeded. After the click, the
    // click handler seeds the chat store with one assistant
    // message containing a tool_block reconstructed from the
    // task's tool_call / tool_result pair. The chat view must
    // render that tool block so the user can see what the
    // previous run actually did.
    const assistant = page.getByTestId('message-assistant').first();
    await expect(assistant).toBeVisible({ timeout: 10000 });
    await expect(assistant.getByText('shell')).toBeVisible();
  });
});
