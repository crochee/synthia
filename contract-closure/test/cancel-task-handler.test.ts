import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';

import type { ContractFile } from '../lib/types.js';
import { unionEndpoints } from '../lib/unifier.js';

/**
 * Fix card #006 — `POST /a2a/tasks/{id}:cancel` backend handler.
 *
 * The cancel endpoint is already registered by `a2a-server-lf` under
 * the `/a2a` JSON-RPC router. `SynthiaExecutor::cancel()` calls
 * `SessionController::cancel()` and yields
 * `StreamResponse::Task(TaskState::Canceled)`.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');
const CONTRACT_PATH = join(ROOT, 'docs/interface-contract/contract.yaml');

function loadContract(): ContractFile {
  return parseYaml(readFileSync(CONTRACT_PATH, 'utf8')) as ContractFile;
}

describe('fix card #006 — cancel task handler', () => {
  it('contract.yaml has the cancel entry with status: closed', () => {
    const cf = loadContract();
    const ep = cf.endpoints.find(
      (e) => e.id === 'cancel /a2a/tasks/{key}:cancel',
    );
    expect(ep, 'cancel entry should exist in contract.yaml').toBeDefined();
    expect(ep!.method).toBe('cancel');
    expect(ep!.path).toBe('/a2a/tasks/{key}:cancel');
    expect(ep!.status).toBe('closed');
  });

  it('backend executor has cancel method', () => {
    const executorPath = join(
      ROOT,
      'crates/synthia-server/src/a2a/executor.rs',
    );
    const src = readFileSync(executorPath, 'utf8');
    expect(src).toContain('fn cancel(');
    expect(src).toContain('TaskState::Canceled');
    expect(src).toContain('controller.cancel()');
  });

  it('scanner preserves the cancel entry on regeneration', () => {
    const cf = loadContract();
    const preserved = cf.endpoints;
    const unioned = unionEndpoints([], [], preserved);
    const ep = unioned.endpoints.find(
      (e) => e.id === 'cancel /a2a/tasks/{key}:cancel',
    );
    expect(ep).toBeDefined();
    expect(ep!.status).toBe('closed');
  });
});
