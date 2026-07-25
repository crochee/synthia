import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';

import type { ContractFile } from '../lib/types.js';
import { unionEndpoints } from '../lib/unifier.js';

/**
 * Fix card #003 — SSE `tasks/{id}:subscribe` event `status-update`
 * field `state` enum values aligned with the frontend reducer state
 * machine and the canonical `@a2a-js/sdk@1.0.0` `TaskState` enum.
 *
 * ARBITRATION.md priority chain (cycle #2 brief §4.3):
 *   1. A2A official protocol spec
 *   2. `@a2a-js/sdk` TypeScript types  ← source of truth for the enum set
 *   3. Synthia stable spec
 *
 * Per the @a2a-js/sdk@1.0.0 type definitions the canonical enum
 * string set (after `taskStateToJSON`) is:
 *
 *   TASK_STATE_UNSPECIFIED, TASK_STATE_SUBMITTED, TASK_STATE_WORKING,
 *   TASK_STATE_COMPLETED, TASK_STATE_FAILED, TASK_STATE_CANCELED,
 *   TASK_STATE_INPUT_REQUIRED, TASK_STATE_REJECTED,
 *   TASK_STATE_AUTH_REQUIRED
 *
 * The wire layer must NOT silently truncate this set. When a
 * backend code path emits an unknown value (e.g. a future SDK
 * version adding a new variant that the Synthia fork has not
 * adopted yet), the server must downgrade the event to
 * `TASK_STATE_FAILED` and emit `tracing::warn!`; the client must
 * `console.error` but NOT throw — the SSE stream must stay alive.
 *
 * The two halves of that contract are pinned in two dedicated test
 * groups below. The "wire-shape" conformance is independently
 * covered by the Playwright spec
 * `synthia-web/tests/e2e/integration/contract-closure/contract-closure.sse-status-update.spec.ts`
 * which drives a real task lifecycle through `sse-harness`.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');
const CONTRACT_PATH = join(ROOT, 'docs/interface-contract/contract.yaml');

function loadContract(): ContractFile {
  return parseYaml(readFileSync(CONTRACT_PATH, 'utf8')) as ContractFile;
}

/** Canonical enum set per `@a2a-js/sdk@1.0.0` `TaskState`. */
const CANONICAL_TASK_STATES = [
  'TASK_STATE_UNSPECIFIED',
  'TASK_STATE_SUBMITTED',
  'TASK_STATE_WORKING',
  'TASK_STATE_COMPLETED',
  'TASK_STATE_FAILED',
  'TASK_STATE_CANCELED',
  'TASK_STATE_INPUT_REQUIRED',
  'TASK_STATE_REJECTED',
  'TASK_STATE_AUTH_REQUIRED',
] as const;

/** Subset the reducer migration table must explicitly cover
 *  (Input-required / Auth-required are the new SDK additions). */
const REQUIRED_MIGRATION_KEYS = [
  'TASK_STATE_UNSPECIFIED',
  'TASK_STATE_SUBMITTED',
  'TASK_STATE_WORKING',
  'TASK_STATE_COMPLETED',
  'TASK_STATE_FAILED',
  'TASK_STATE_CANCELED',
  'TASK_STATE_INPUT_REQUIRED',
  'TASK_STATE_REJECTED',
  'TASK_STATE_AUTH_REQUIRED',
];

describe('fix card #003 — SSE status-update state enum alignment', () => {
  it('contract.yaml has the subscribe entry with status: closed', () => {
    const cf = loadContract();
    const ep = cf.endpoints.find(
      (e) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    expect(ep, 'entry should exist in contract.yaml').toBeDefined();
    expect(ep!.method).toBe('GET');
    expect(ep!.path).toBe('/a2a/tasks/{key}:subscribe');
    expect(ep!.source).toBe('both');
    expect(ep!.status).toBe('closed');
    expect(ep!.sse_events?.map((e) => e.name)).toContain('status-update');
  });

  it('contract.yaml status-update event documents the canonical TaskState enum set', () => {
    const cf = loadContract();
    const ep = cf.endpoints.find(
      (e) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    const ev = ep!.sse_events?.find((e) => e.name === 'status-update');
    expect(ev, 'status-update SSE event must exist').toBeDefined();
    expect(ev!.fields).toContain('status.state');
    // Every canonical enum value must appear somewhere in the
    // endpoint's combined notes (entry-level + sse-event-level)
    // so the contract file is self-documenting and a future
    // bump to the SDK that drops a variant is caught here.
    const blob = `${ep!.notes ?? ''}\n${ev!.notes ?? ''}`;
    for (const value of CANONICAL_TASK_STATES) {
      expect(blob, `enum value ${value} must be documented`).toContain(value);
    }
  });

  it('frontend reducer migration table covers the full SDK TaskState enum set', () => {
    // The reducer in synthia-web/src/pages/ChatPage.tsx
    // (`normalizeTaskState`) lower-cases the SDK's
    // `taskStateToJSON` output for CSS class suffixes. We pin
    // the explicit migration keys (and "unspecified" as the
    // unknown-fallback) by reading the source and asserting the
    // canonical names appear.
    const reducerPath = join(
      ROOT,
      'synthia-web/src/pages/ChatPage.tsx',
    );
    const src = readFileSync(reducerPath, 'utf8');
    // Strip comments so a docstring that mentions the values
    // historically does not pollute the result.
    const code = src
      .split('\n')
      .filter(
        (line) =>
          !line.trim().startsWith('//') && !line.trim().startsWith('*'),
      )
      .join('\n');
    for (const key of REQUIRED_MIGRATION_KEYS) {
      expect(code, `reducer must reference ${key}`).toContain(key);
    }
    // The reducer must call `console.error` (not throw) for
    // unknown enum values, so the SSE stream stays alive.
    expect(code, 'reducer must call console.error on unknown state').toMatch(
      /console\.error\([^)]*(?:unknown|TaskState|state)/i,
    );
  });

  it('scanner preserves the manually-curated subscribe entry on regeneration', () => {
    // Mirrors the #002 scanner regression: if the backend
    // scanner ever sees `/a2a/tasks/{id}:subscribe` (currently
    // mounted via `nest_service` so it can't), or the frontend
    // scanner ever sees a direct call, the unifier must still
    // surface our manually-curated entry with `status: closed`
    // so a fresh `make contract-scan` does not silently drop
    // the fix card.
    const cf = loadContract();
    const preserved = cf.endpoints;
    const unioned = unionEndpoints([], [], preserved);
    const ep = unioned.endpoints.find(
      (e) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    expect(ep).toBeDefined();
    expect(ep!.status).toBe('closed');
    expect(ep!.source).toBe('both');
  });
});