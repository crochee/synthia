/**
 * Unit tests for the `stripAttachmentSegments` helper used by
 * ChatPage's localStorage persistence layer (spec §4.6).
 *
 * The contract is simple but pinned: attachment segments must
 * NEVER round-trip through localStorage. Other segment types
 * (text, thinking, tool_block, etc.) must round-trip
 * untouched. The helper lives in `lib/strip-attachment-segments.ts`
 * so it can be unit-tested under Playwright's node loader.
 */
import { expect, test } from '@playwright/test';
import { stripAttachmentSegments } from '../../../src/lib/strip-attachment-segments';

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

test('drops attachment segments but keeps everything else', () => {
  const input: unknown[] = [
    makeMsg('m1', [
      seg('text', 'hello'),
      seg('attachment', ''), // would carry attachmentParts in real use
      seg('tool_block', ''),
      seg('attachment', ''),
      seg('thinking', '...'),
    ]),
  ];
  const out = stripAttachmentSegments(input as never[]);
  expect(out).toHaveLength(1);
  expect(out[0].segments.map((s: { type: string }) => s.type)).toEqual([
    'text',
    'tool_block',
    'thinking',
  ]);
});

test('returns a fresh array (no in-place mutation)', () => {
  const input: unknown[] = [makeMsg('m1', [seg('attachment', '')])];
  const out = stripAttachmentSegments(input as never[]);
  expect(out).not.toBe(input);
  expect(out[0]).not.toBe(input[0]);
  expect(out[0].segments).not.toBe(input[0].segments);
});

test('empty input returns empty output', () => {
  expect(stripAttachmentSegments([])).toEqual([]);
});

test('input with no attachment segments is unchanged in shape', () => {
  const input: unknown[] = [
    makeMsg('m1', [seg('text', 'a')]),
    makeMsg('m2', [seg('tool_block', '')]),
  ];
  const out = stripAttachmentSegments(input as never[]);
  expect(out).toHaveLength(2);
  expect(out[0].segments).toHaveLength(1);
  expect(out[1].segments).toHaveLength(1);
});

test('survives JSON.stringify / JSON.parse round-trip', () => {
  const input: unknown[] = [
    makeMsg('m1', [
      seg('text', 'hello'),
      {
        id: 'a',
        type: 'attachment',
        content: '',
        attachmentId: 'x',
        attachmentParts: [{ text: 'big' }],
      },
    ]),
  ];
  const serialised = JSON.stringify(stripAttachmentSegments(input as never[]));
  const parsed = JSON.parse(serialised);
  // Re-strip — should be idempotent (already no attachment segments, but the
  // caller in ChatPage does this on the read path so verify the property).
  const out = stripAttachmentSegments(parsed);
  expect(out[0].segments.map((s: { type: string }) => s.type)).toEqual(['text']);
});
