/**
 * Mock A2A fetch — returns a minimal v1.0 `agent-card.json`
 * discovery response and a `text/event-stream` Response
 * carrying the supplied SSE events. Designed for
 * `@a2a-js/sdk@1.0`'s JSON-RPC transport.
 *
 * Why SSE rather than yielding `StreamResponse` objects
 * directly: the SDK's SSE parser is non-trivial (handles
 * keep-alive newlines, partial UTF-8, multi-line `data:`
 * concatenation). Re-using the real parser via a mock
 * `Response` keeps us on the same code path as production.
 *
 * This module is consumed by e2e tests via `readFileSync` +
 * `new Function` re-evaluation inside `page.evaluate`, so it
 * must not import anything that breaks in that context (no
 * `@playwright/test`, no Node-only modules). Use only
 * browser globals (`ReadableStream`, `TextEncoder`, `Response`).
 */

export interface SSEEvent {
  /** Value of the `event:` field; omit for default (`message`). */
  event?: string;
  /** Single concatenated `data:` payload. Newlines inside become individual data lines per SSE spec. */
  data: string;
}

export interface AgentCardFixture {
  /** Override the `supportedInterfaces[].url` field. */
  url?: string;
}

export interface MockA2AConfig {
  agentCard?: AgentCardFixture;
  streamEvents: ReadonlyArray<SSEEvent>;
}

const AGENT_CARD_PATH = '/.well-known/agent-card.json';

/**
 * Build a `typeof fetch` mock that dispatches on URL path:
 * `/.well-known/agent-card.json` returns a minimal v1.0 card;
 * any other URL returns the SSE stream. The SDK only hits these
 * two paths in practice (verified against `@a2a-js/sdk@1.0`).
 */
export function buildMockA2AFetch(config: MockA2AConfig): typeof fetch {
  const agentCardJson = JSON.stringify(buildAgentCard(config.agentCard));
  const streamEvents = config.streamEvents;
  return async (input: RequestInfo | URL, _init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
    if (url.endsWith(AGENT_CARD_PATH) || url.includes(AGENT_CARD_PATH)) {
      return new Response(agentCardJson, {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }
    return streamToSSEResponse(streamEvents);
  };
}

/**
 * Encode an array of events as an SSE `text/event-stream`
 * Response. Each event becomes a frame ending with a blank
 * line; multi-line `data` strings are split into multiple
 * `data:` lines per the WHATWG SSE spec.
 */
export function streamToSSEResponse(events: ReadonlyArray<SSEEvent>): Response {
  const encoder = new TextEncoder();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const ev of events) {
        if (ev.event) {
          controller.enqueue(encoder.encode(`event: ${ev.event}\n`));
        }
        for (const line of ev.data.split('\n')) {
          controller.enqueue(encoder.encode(`data: ${line}\n`));
        }
        controller.enqueue(encoder.encode('\n'));
      }
      controller.close();
    },
  });
  return new Response(body, {
    status: 200,
    headers: {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
    },
  });
}

/**
 * Build a minimal v1.0 `AgentCard` JSON the SDK's
 * `DefaultAgentCardResolver` will accept. The fields below
 * are the SDK's minimum required for `createFromUrl` to
 * succeed (verified by reading `node_modules/.pnpm/@a2a-js+sdk@1.0.0/...`).
 */
export function buildAgentCard(fixture?: AgentCardFixture): unknown {
  const url = fixture?.url ?? 'http://localhost/a2a';
  return {
    name: 'Mock Agent',
    description: 'Test fixture for e2e tests',
    version: '1.0',
    url,
    protocolVersion: '1.0',
    supportedInterfaces: [{ protocolVersion: '1.0', url }],
    capabilities: { streaming: true },
    defaultInputModes: ['text/plain'],
    defaultOutputModes: ['text/plain'],
    skills: [],
  };
}
