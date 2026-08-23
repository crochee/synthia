/**
 * Pure-function unit tests for `appendAttachmentSegment`.
 *
 * `appendAttachmentSegment(prev, event)` is the reducer ChatPage
 * uses to fold an `attachment` event into the assistant
 * message's segment list. The contract is strict (spec §4.4):
 *   - `append=true` only merges parts into an existing attachment
 *     segment with the same `attachmentId` *within the same
 *     message*. Phantom artifacts are never created.
 *   - `append=true` after `lastChunk=true` is dropped.
 *   - `append=false` with a duplicate `attachmentId` within the
 *     same message is dropped (append protocol is monotone).
 *   - `lastChunk=true` flips `isComplete` and is otherwise a
 *     no-op on the segment list.
 *
 * The reducer is exported from `lib/append-attachment-segment.ts`
 * (a pure module — no React, no CSS imports) so it can be unit
 * tested in isolation under Playwright's node test loader.
 * The 10 cases below are the pinned contract — a future
 * refactor that breaks any one of them should fail loudly here.
 */
import { expect, test } from '@playwright/test';
import type { SessionPart } from '../../../src/api/types';
import { appendAttachmentSegment } from '../../../src/lib/append-attachment-segment';

const part = (text: string): SessionPart => ({ text });

test.describe('appendAttachmentSegment — strict attachment protocol', () => {
  test('new attachment with append=undefined pushes a segment', () => {
    const prev: never[] = [];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: {
        attachmentId: 'a-1',
        parts: [part('hello')],
      },
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { type: string }).type).toBe('attachment');
    expect((next[0] as { attachmentId: string }).attachmentId).toBe('a-1');
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'hello' },
    ]);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(false);
  });

  test('append=true merges parts into existing attachment, leaves name/id alone', () => {
    const initial = {
      id: 'seg-1',
      type: 'attachment',
      content: '',
      attachmentId: 'a-1',
      attachmentName: 'my-name',
      attachmentParts: [part('hello')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part(' world')], name: 'should-be-ignored' },
      append: true,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { attachmentName: string }).attachmentName).toBe('my-name');
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'hello' },
      { text: ' world' },
    ]);
  });

  test('append=true with no prior attachment in this message is dropped', () => {
    const prev: never[] = [];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('hello')] },
      append: true,
    });
    expect(next).toHaveLength(0);
  });

  test('append=false with duplicate attachmentId within same message is dropped', () => {
    const initial = {
      id: 'seg-1',
      type: 'attachment',
      content: '',
      attachmentId: 'a-1',
      attachmentParts: [part('hello')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('replace-me')] },
      append: false,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'hello' },
    ]);
  });

  test('append=true after lastChunk=true is dropped', () => {
    const initial = {
      id: 'seg-1',
      type: 'attachment',
      content: '',
      attachmentId: 'a-1',
      attachmentParts: [part('hello')],
      isComplete: true,
    };
    const prev: never[] = [initial as never];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('extra')] },
      append: true,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'hello' },
    ]);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(true);
  });

  test('lastChunk=true flips isComplete without adding parts', () => {
    const initial = {
      id: 'seg-1',
      type: 'attachment',
      content: '',
      attachmentId: 'a-1',
      attachmentParts: [part('hello')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [] },
      lastChunk: true,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(true);
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'hello' },
    ]);
  });

  test('multiple append=true events accumulate correctly', () => {
    let prev: never[] = [];
    prev = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('a')] },
    });
    prev = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('b')] },
      append: true,
    });
    prev = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('c')] },
      append: true,
    });
    expect(prev).toHaveLength(1);
    expect(
      (prev[0] as { attachmentParts: SessionPart[] }).attachmentParts.map((p) => p.text),
    ).toEqual(['a', 'b', 'c']);
  });

  test('append protocol is per-message scope: duplicate id across messages is allowed', () => {
    const initial = {
      id: 'seg-1',
      type: 'attachment',
      content: '',
      attachmentId: 'a-1',
      attachmentParts: [part('first')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-1', parts: [part('fresh')] },
    });
    expect(next).toHaveLength(2);
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'first' },
    ]);
    expect((next[1] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([
      { text: 'fresh' },
    ]);
    expect((next[1] as { isComplete: boolean }).isComplete).toBe(false);
  });
});

test.describe('appendAttachmentSegment — empty / degenerate inputs', () => {
  test('empty parts array still creates the segment', () => {
    const prev: never[] = [];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: 'a-empty', parts: [] },
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { attachmentParts: SessionPart[] }).attachmentParts).toEqual([]);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(false);
  });

  test('event without attachmentId is dropped', () => {
    const prev: never[] = [];
    const next = appendAttachmentSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      attachment: { attachmentId: '', parts: [] },
    });
    expect(next).toHaveLength(0);
  });
});
