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

  it('backend mapping routes tool_call / tool_result to Message channel only (A2A v1.0 §3.7)', () => {
    // Per A2A v1.0 §3.7 ("Messages and Artifacts"), tool
    // calls and tool results are communication turns on the
    // `Message` channel, not tangible deliverables. The MVP
    // therefore routes them through
    // `Message(agent) + Part::data` and never through
    // `ArtifactUpdate`. The same `mapping.rs` legitimately
    // DOES emit `StreamResponse::ArtifactUpdate` for
    // tangible deliverables (ResourceLink / Image / Audio) —
    // we check the tool-control-flow arm in isolation.
    const mappingPath = join(
      ROOT,
      'crates/synthia-server/src/a2a/mapping.rs',
    );
    const src = readFileSync(mappingPath, 'utf8');
    // Extract each `ContentPart::Xxx` variant arm by
    // stopping at the first `\n            },?` — the
    // closing brace of the arrow body. We deliberately
    // ignore any `// comment` text in front of the next
    // sibling because a comment is NOT code emission.
    function extractArm(variant: string): string {
      const startMarker = `ContentPart::${variant}(`;
      const start = src.indexOf(startMarker);
      if (start === -1) {
        throw new Error(`variant ${variant} not found in mapping.rs`);
      }
      const fromVariant = src.slice(start);
      // Stop at the FIRST sibling `ContentPart::Xxx(` opening
      // after the start marker — that marks the boundary of
      // the current arm regardless of whether the original
      // source uses a blank line between siblings.
      const siblingRe = /\n\s{12}ContentPart::/g;
      siblingRe.lastIndex = 0;
      let end = fromVariant.length;
      let m = siblingRe.exec(fromVariant);
      if (m && m.index !== undefined) {
        end = m.index;
      }
      const armText = fromVariant.slice(0, end);
      // Strip `// ...` line comments so a doc-comment that
      // legitimately mentions `ArtifactUpdate` (explaining
      // what the NEXT sibling does) does not pollute the
      // assertion.
      return armText
        .split('\n')
        .filter((line) => !line.trim().startsWith('//'))
        .join('\n');
    }
    const toolUseArm = extractArm('ToolUse');
    const toolResultArm = extractArm('ToolResult');
    // ToolUse arm MUST go through Message(Part::data), never ArtifactUpdate.
    expect(
      toolUseArm,
      'ToolUse arm must NOT emit StreamResponse::ArtifactUpdate',
    ).not.toContain('ArtifactUpdate');
    // ToolResult arm MUST go through Message(Part::data), never ArtifactUpdate.
    expect(
      toolResultArm,
      'ToolResult arm must NOT emit StreamResponse::ArtifactUpdate',
    ).not.toContain('ArtifactUpdate');
    // The legacy helper name must be gone.
    expect(
      src,
      'mapping.rs must not reference tool_result_to_responses — the legacy artifact-path helper is gone',
    ).not.toContain('tool_result_to_responses');
    // SessionEnded must still flow through System path → StatusUpdate.
    expect(src).toMatch(/AgentEvent::System\([\s\S]*?SessionEnded/);
  });

  it('backend mapping routes tangible deliverables (ResourceLink / Image / Audio) to ArtifactUpdate', () => {
    // The complementary contract: per A2A v1.0 §3.7, tangible
    // deliverables belong on the `Artifact` channel. The MVP
    // routes `ContentPart::Resource` (the canonical
    // pointer-to-external-resource carrier) and the
    // image/audio content variants to
    // `StreamResponse::ArtifactUpdate`.
    const mappingPath = join(
      ROOT,
      'crates/synthia-server/src/a2a/mapping.rs',
    );
    const src = readFileSync(mappingPath, 'utf8');
    expect(
      src,
      'ContentPart::Resource arm must route to ArtifactUpdate',
    ).toMatch(/ContentPart::Resource[\s\S]{0,200}?StreamResponse::ArtifactUpdate/);
    expect(
      src,
      'ContentPart::Image arm must route to ArtifactUpdate',
    ).toMatch(/ContentPart::Image[\s\S]{0,200}?StreamResponse::ArtifactUpdate/);
    expect(
      src,
      'ContentPart::Audio arm must route to ArtifactUpdate',
    ).toMatch(/ContentPart::Audio[\s\S]{0,200}?StreamResponse::ArtifactUpdate/);
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
