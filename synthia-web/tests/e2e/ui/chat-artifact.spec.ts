/**
 * End-to-end coverage for live A2A ArtifactUpdate rendering in
 * the chat stream. The MVP backend doesn't emit artifact events,
 * so we drive the SDK via a mocked fetch (see
 * `_setA2ATestFetch` / `_bootstrapTestFetch` /
 * `_resetClientForTesting` in `src/api/a2a-stream.ts`).
 *
 * Pinned user-visible contract:
 *   - the `chat-artifact-a-file` testid appears after submit
 *   - the badge text matches the artifact name
 *   - all 3 part bodies are rendered
 *   - the streaming chip is gone after lastChunk
 */
import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';
import { fileURLToPath } from 'node:url';

/** Three events that drive the reducer through its full lifecycle. */
const artifactEvents: ReadonlyArray<{ data: string }> = [
  // 1) Start new artifact
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't1',
        contextId: 'c1',
        artifact: {
          artifactId: 'a-file',
          name: 'hello.py',
          parts: [{ text: "print('a')\n" }],
        },
        append: false,
      },
    }),
  },
  // 2) Append second part
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't1',
        contextId: 'c1',
        artifact: { artifactId: 'a-file', parts: [{ text: "print('b')\n" }] },
        append: true,
      },
    }),
  },
  // 3) Append third part + lastChunk
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't1',
        contextId: 'c1',
        artifact: { artifactId: 'a-file', parts: [{ text: "print('c')\n" }] },
        append: true,
        lastChunk: true,
      },
    }),
  },
];

test('badge appears, parts accumulate, streaming chip clears on lastChunk', async ({ page }) => {
  const chat = new ChatPage(page);
  await chat.goto();

  // 1. Define the mock fetch in-page so ReadableStream / TextEncoder are available.
  const factorySource = readFileSync(
    fileURLToPath(new URL('../helpers/mock-a2a-server.runtime.js', import.meta.url)),
    'utf8',
  );
  const eventsJson = JSON.stringify(artifactEvents);
  await page.evaluate(
    ({ eventsJson, factorySource }: { eventsJson: string; factorySource: string }) => {
      const factory = new Function(
        'events',
        // The .js shadow already wraps each event's data in a JSON-RPC
        // envelope and auto-appends a statusUpdate terminal. We only
        // pass the user-supplied events.
        factorySource + '\nreturn buildMockA2AFetch({ streamEvents: events });',
      );
      const events = JSON.parse(eventsJson) as Array<{ data: string }>;
      (window as unknown as { __synthiaMockFetch: typeof fetch }).__synthiaMockFetch =
        factory(events);
    },
    { eventsJson, factorySource },
  );

  // 2. Wire the mock into the SDK and reset the cached client.
  await page.evaluate(async () => {
    const mod = await import('/src/api/a2a-stream.ts');
    (mod as unknown as { _bootstrapTestFetch: () => void })._bootstrapTestFetch();
    (mod as unknown as { _resetClientForTesting: () => void })._resetClientForTesting();
  });

  // 3. Submit.
  await chat.sendMessage('emit an artifact');

  // 4. Wait for the assistant terminal status (driven by the .js
  //    shadow's auto-appended TASK_STATE_COMPLETED statusUpdate).
  await chat.waitForAssistantReply(30_000);

  // 5. Assertions: badge, parts, streaming chip.
  const card = page.getByTestId('chat-artifact-a-file');
  await expect(card).toBeVisible({ timeout: 15_000 });

  await expect(card.getByText('hello.py')).toBeVisible();

  const bodyText = await card.innerText();
  expect(bodyText).toContain("print('a')");
  expect(bodyText).toContain("print('b')");
  expect(bodyText).toContain("print('c')");

  await expect(card.locator('.nt-chat__artifact-streaming')).toHaveCount(0);
});
