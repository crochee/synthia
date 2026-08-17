/**
 * Live exercise of the artifact reducer + renderer's debug
 * logging. Drives the chat page through 5 artifact events
 * (start, append, lastChunk, orphan append, duplicate
 * append=false) and pins that every branch — both the
 * [applyStreamEvent artifact] apply branch and the [artifact]
 * strict-protocol warnings — fires its console.debug/warn.
 *
 * The SDK's JSON-RPC fetch is mocked via the
 * `_setA2ATestFetch` / `_bootstrapTestFetch` hooks added in
 * `src/api/a2a-stream.ts`. The mock factory
 * `buildMockA2AFetch` from `tests/e2e/helpers/mock-a2a-server.ts`
 * is read from disk and re-evaluated in-page via `new Function`
 * so that `ReadableStream` / `TextEncoder` (browser globals)
 * are available where the factory runs.
 *
 * Note: the plain-JS shadow `mock-a2a-server.runtime.js` is
 * what we actually read into the page — `new Function()` can't
 * parse the `.ts` source's `export` / `interface` / `: T`
 * annotations. The shadow is kept in lockstep with the `.ts`
 * source; the `.ts` source remains the canonical typed
 * implementation.
 */
import { readFileSync } from 'node:fs';
import { expect, test, type ConsoleMessage } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';
import { fileURLToPath } from 'node:url';

/** Five artifact events driving the reducer through every branch. */
const artifactEvents: ReadonlyArray<{ data: string }> = [
  // 1) Start new artifact (append=undefined)
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't',
        contextId: 'c',
        artifact: { artifactId: 'a1', name: 'hi.txt', parts: [{ text: 'a' }] },
        append: false,
      },
    }),
  },
  // 2) Append second part (append=true)
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't',
        contextId: 'c',
        artifact: { artifactId: 'a1', parts: [{ text: 'b' }] },
        append: true,
      },
    }),
  },
  // 3) Append third part + lastChunk (append=true, lastChunk=true)
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't',
        contextId: 'c',
        artifact: { artifactId: 'a1', parts: [{ text: 'c' }] },
        append: true,
        lastChunk: true,
      },
    }),
  },
  // 4) Orphan append=true with no prior segment for a-orphan → reducer warns
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't',
        contextId: 'c',
        artifact: { artifactId: 'a-orphan', parts: [{ text: 'x' }] },
        append: true,
      },
    }),
  },
  // 5) Duplicate append=false with same id as #1 → reducer warns
  {
    data: JSON.stringify({
      artifactUpdate: {
        taskId: 't',
        contextId: 'c',
        artifact: { artifactId: 'a1', parts: [{ text: 'y' }] },
        append: false,
      },
    }),
  },
];

test('every artifact branch emits its distinctive console.debug/warn', async ({ page }) => {
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
        // Wrap the factory source so it returns the mock fetch with the given events.
        factorySource + '\nreturn buildMockA2AFetch({ streamEvents: events });',
      );
      const events = JSON.parse(eventsJson) as Array<{ data: string }>;
      (window as unknown as { __synthiaMockFetch: typeof fetch }).__synthiaMockFetch =
        factory(events);
    },
    { eventsJson, factorySource },
  );

  // 2. Bootstrap a2a-stream.ts to wire the mock into the SDK. Reset the
  // cached client too: main.tsx pre-warms the SDK at boot with the
  // production window.fetch, so without a reset the cached client
  // keeps using the real transport even after we install the mock.
  await page.evaluate(async () => {
    const mod = await import('/src/api/a2a-stream.ts');
    (mod as unknown as { _bootstrapTestFetch: () => void })._bootstrapTestFetch();
    (mod as unknown as { _resetClientForTesting: () => void })._resetClientForTesting();
  });

  // 3. Capture console messages for the assertions below.
  const captured: string[] = [];
  page.on('console', (msg: ConsoleMessage) => {
    const t = msg.type();
    const text = msg.text();
    if (
      (t === 'debug' || t === 'warning') &&
      (text.startsWith('[applyStreamEvent artifact]') || text.startsWith('[artifact]'))
    ) {
      captured.push(`${t}: ${text}`);
    }
  });

  // 4. Submit. SDK init triggers fetchImpl discovery, then streams events.
  await chat.sendMessage('emit');

  // 5. Wait for the assistant terminal status (stream end). The mock
  // auto-appends a TASK_STATE_COMPLETED statusUpdate after the user's
  // events so the chat UI's terminal state fires.
  await chat.waitForAssistantReply(30_000);

  // Apply branch: every event emits an [applyStreamEvent artifact] debug line.
  const applyBranches = captured.filter((l) => l.startsWith('debug: [applyStreamEvent artifact]'));
  expect(
    applyBranches.length,
    `expected at least 5 apply branches, got ${applyBranches.length}. Captured:\n  ${captured.join('\n  ')}`,
  ).toBeGreaterThanOrEqual(5);

  // Strict-protocol warnings: orphan append + duplicate append=false.
  const orphanWarn = captured.find((l) => l.includes('append=true with no prior artifact'));
  expect(
    orphanWarn,
    'expected orphan-append warn; captured: ' + captured.join('\n  '),
  ).toBeDefined();

  const dupeWarn = captured.find((l) =>
    l.includes('append=false with duplicate artifactId in message'),
  );
  expect(
    dupeWarn,
    'expected duplicate-append=false warn; captured: ' + captured.join('\n  '),
  ).toBeDefined();
});
