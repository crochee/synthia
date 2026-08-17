/**
 * Unit tests for the `stripArtifactSegments` helper used by
 * ChatPage's localStorage persistence layer (spec §4.6).
 *
 * The contract is simple but pinned: artifact segments must
 * NEVER round-trip through localStorage. Other segment types
 * (text, thinking, tool_block, etc.) must round-trip
 * untouched. The helper lives in `lib/strip-artifact-segments.ts`
 * so it can be unit-tested under Playwright's node loader.
 */
import { expect, test } from '@playwright/test';
import { stripArtifactSegments } from '../../../src/lib/strip-artifact-segments';

const makeMsg = (id: string, segments: ReadonlyArray<unknown>): unknown => ({
  id,
  role: 'assistant',
  segments,
  status: 'completed',
});

const seg = (type: string, content: string): unknown => ({
  id: `s-${Math.random()}`,
  type,
  content,
});

test('drops artifact segments but keeps everything else', () => {
  const input: unknown[] = [
    makeMsg('m1', [
      seg('text', 'hello'),
      seg('artifact', ''), // would carry artifactParts in real use
      seg('tool_block', ''),
      seg('artifact', ''),
      seg('thinking', '...'),
    ]),
  ];
  const out = stripArtifactSegments(input as never[]);
  expect(out).toHaveLength(1);
  expect(out[0].segments.map((s: { type: string }) => s.type)).toEqual([
    'text',
    'tool_block',
    'thinking',
  ]);
});

test('returns a fresh array (no in-place mutation)', () => {
  const input: unknown[] = [makeMsg('m1', [seg('artifact', '')])];
  const out = stripArtifactSegments(input as never[]);
  expect(out).not.toBe(input);
  expect(out[0]).not.toBe(input[0]);
  expect(out[0].segments).not.toBe(input[0].segments);
});

test('empty input returns empty output', () => {
  expect(stripArtifactSegments([])).toEqual([]);
});

test('input with no artifact segments is unchanged in shape', () => {
  const input: unknown[] = [
    makeMsg('m1', [seg('text', 'a')]),
    makeMsg('m2', [seg('tool_block', '')]),
  ];
  const out = stripArtifactSegments(input as never[]);
  expect(out).toHaveLength(2);
  expect(out[0].segments).toHaveLength(1);
  expect(out[1].segments).toHaveLength(1);
});

test('survives JSON.stringify / JSON.parse round-trip', () => {
  const input: unknown[] = [
    makeMsg('m1', [
      seg('text', 'hello'),
      { id: 'a', type: 'artifact', content: '', artifactId: 'x', artifactParts: [{ text: 'big' }] },
    ]),
  ];
  const serialised = JSON.stringify(stripArtifactSegments(input as never[]));
  const parsed = JSON.parse(serialised);
  // Re-strip — should be idempotent (already no artifact segments, but the
  // caller in ChatPage does this on the read path so verify the property).
  const out = stripArtifactSegments(parsed);
  expect(out[0].segments.map((s: { type: string }) => s.type)).toEqual(['text']);
});
