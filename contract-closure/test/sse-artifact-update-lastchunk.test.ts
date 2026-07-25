import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';

import type { ContractFile } from '../lib/types.js';
import { unionEndpoints } from '../lib/unifier.js';

/**
 * Fix card #004 — SSE `tasks/{id}:subscribe` event `artifact-update`
 * must include `lastChunk: boolean` per `@a2a-js/sdk@1.0.0`
 * `TaskArtifactUpdateEvent.lastChunk`.
 *
 * ARBITRATION.md priority 2 (SDK types > Synthia stable spec):
 * the proto field `last_chunk` serialises as `lastChunk` on the wire.
 * The backend MUST set it (the a2a-lf crate uses `Option<bool>`,
 * so `Some(true)` for final chunk, `Some(false)` for intermediate).
 * Frontend MUST read it (null-safety) to decide whether to flush.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');
const CONTRACT_PATH = join(ROOT, 'docs/interface-contract/contract.yaml');

function loadContract(): ContractFile {
  return parseYaml(readFileSync(CONTRACT_PATH, 'utf8')) as ContractFile;
}

describe('fix card #004 — SSE artifact-update lastChunk', () => {
  it('contract.yaml subscribe entry has artifact-update in sse_events', () => {
    const cf = loadContract();
    const ep = cf.endpoints.find(
      (e) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    expect(ep, 'subscribe entry should exist').toBeDefined();
    const eventNames = ep!.sse_events?.map((e) => e.name) ?? [];
    expect(eventNames).toContain('artifact-update');
  });

  it('contract.yaml artifact-update event documents lastChunk field', () => {
    const cf = loadContract();
    const ep = cf.endpoints.find(
      (e) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    const ev = ep!.sse_events?.find((e) => e.name === 'artifact-update');
    expect(ev, 'artifact-update event must exist').toBeDefined();
    expect(ev!.fields).toContain('lastChunk');
    // Notes must mention the SDK type for ARBITRATION traceability.
    const blob = `${ep!.notes ?? ''}\n${ev!.notes ?? ''}`;
    expect(blob).toContain('lastChunk');
  });

  it('backend mapping emits ArtifactUpdate with last_chunk for ToolResult', () => {
    // Verify by reading the Rust source. The function
    // `tool_result_to_artifact` must produce an `Artifact` and the
    // `AgentEvent::Model(ContentPart::ToolResult)` arm must push
    // `StreamResponse::ArtifactUpdate(TaskArtifactUpdateEvent {
    //   last_chunk: Some(true), ... })`.
    const mappingPath = join(
      ROOT,
      'crates/synthia-a2a/src/mapping.rs',
    );
    const src = readFileSync(mappingPath, 'utf8');
    expect(src).toContain('StreamResponse::ArtifactUpdate');
    expect(src).toContain('last_chunk: Some(true)');
    expect(src).toContain('tool_result_to_artifact');
  });

  it('frontend reads lastChunk from artifactUpdate event', () => {
    const streamPath = join(
      ROOT,
      'synthia-web/src/api/a2a-stream.ts',
    );
    const src = readFileSync(streamPath, 'utf8');
    // The frontend must read `.lastChunk` from the event.
    expect(src).toContain('lastChunk');
  });

  it('scanner preserves the subscribe entry on regeneration', () => {
    const cf = loadContract();
    const preserved = cf.endpoints;
    const unioned = unionEndpoints([], [], preserved);
    const ep = unioned.endpoints.find(
      (e) => e.id === 'GET /a2a/tasks/{key}:subscribe',
    );
    expect(ep).toBeDefined();
    const eventNames = ep!.sse_events?.map((e) => e.name) ?? [];
    expect(eventNames).toContain('artifact-update');
  });
});
