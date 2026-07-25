import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';

import type { ContractFile, Endpoint } from '../lib/types.js';
import { unionEndpoints } from '../lib/unifier.js';

/**
 * Fix card #002 — `POST /a2a/message:send` payload field naming.
 *
 * Asserts the contract is sound after the fix:
 *   1. `docs/interface-contract/contract.yaml` carries the entry with
 *      `status: closed`.
 *   2. The frontend payload-building sites use **camelCase** field
 *      names that match `@a2a-js/sdk@1.0.0`'s `SendMessageRequest`
 *      type (per `docs/interface-contract/ARBITRATION.md` priority 2:
 *      SDK types win over Synthia stable spec).
 *   3. The scanner preserves the manually-curated entry when
 *      re-running (it would otherwise drop anything not seen by the
 *      backend router.rs or frontend fetch caller scanner).
 *
 * The test deliberately avoids spawning a server. Wire-shape
 * conformance is independently covered by the Playwright spec
 * `synthia-web/tests/e2e/integration/contract-closure/contract-closure.message-send.spec.ts`.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');
const CONTRACT_PATH = join(ROOT, 'docs/interface-contract/contract.yaml');
const A2A_CLIENT_PATH = join(ROOT, 'synthia-web/src/api/a2a-client.ts');
const A2A_STREAM_PATH = join(ROOT, 'synthia-web/src/api/a2a-stream.ts');

function loadContract(): ContractFile {
  return parseYaml(readFileSync(CONTRACT_PATH, 'utf8')) as ContractFile;
}

const SNAKE_CASE_MESSAGE_FIELDS = [
  'message_id',
  'context_id',
  'task_id',
  'reference_task_ids',
  'media_type',
];

function snakeCaseInFrontend(filePath: string): string[] {
  const src = readFileSync(filePath, 'utf8');
  // Strip the type-narrowing comments + leading/trailing whitespace
  // before scanning so a comment that mentions e.g. `message_id`
  // historically does not pollute the result.
  const code = src
    .split('\n')
    .filter((line) => !line.trim().startsWith('//') && !line.trim().startsWith('*'))
    .join('\n');
  return SNAKE_CASE_MESSAGE_FIELDS.filter((field) => new RegExp(`\\b${field}\\b`).test(code));
}

describe('fix card #002 — POST /a2a/message:send payload naming', () => {
  it('contract.yaml has the entry with status: closed', () => {
    const cf = loadContract();
    const ep = cf.endpoints.find((e) => e.id === 'message:send /a2a/message:send');
    expect(ep, 'entry should exist in contract.yaml').toBeDefined();
    expect(ep!.method).toBe('message:send');
    expect(ep!.path).toBe('/a2a/message:send');
    expect(ep!.source).toBe('both');
    expect(ep!.status).toBe('closed');
  });

  it('frontend payload uses camelCase field names per @a2a-js/sdk v1.0', () => {
    // Per ARBITRATION.md priority 2 (SDK types > Synthia stable spec),
    // the wire shape is the SDK's `Message` type, which uses camelCase.
    expect(
      snakeCaseInFrontend(A2A_CLIENT_PATH),
      `a2a-client.ts should not contain any of ${SNAKE_CASE_MESSAGE_FIELDS.join(', ')}`,
    ).toEqual([]);
    expect(
      snakeCaseInFrontend(A2A_STREAM_PATH),
      `a2a-stream.ts should not contain any of ${SNAKE_CASE_MESSAGE_FIELDS.join(', ')}`,
    ).toEqual([]);
  });

  it('scanner preserves the manually-curated entry on regeneration', () => {
    // Simulate a fresh scan where neither the backend scanner nor
    // the frontend scanner can see `/a2a/message:send` (it is
    // mounted via `nest_service` in router.rs, not picked up by
    // either scanner). The unifier must still surface the entry
    // from the previously-saved contract.yaml so a fresh
    // `make contract-scan` does not silently drop the fix card.
    const cf = loadContract();
    const preserved = cf.endpoints;
    const be: Endpoint[] = [];
    const fe: Endpoint[] = [];
    const unioned = unionEndpoints(be, fe, preserved);
    const ep = unioned.endpoints.find((e) => e.id === 'message:send /a2a/message:send');
    expect(ep).toBeDefined();
    expect(ep!.status).toBe('closed');
    expect(ep!.source).toBe('both');
  });
});