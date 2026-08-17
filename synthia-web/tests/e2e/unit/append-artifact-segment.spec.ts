/**
 * Pure-function unit tests for `appendArtifactSegment`.
 *
 * `appendArtifactSegment(prev, event)` is the reducer ChatPage
 * uses to fold an A2A `ArtifactUpdate` event into the assistant
 * message's segment list. The contract is strict (spec §4.4):
 *   - `append=true` only merges parts into an existing artifact
 *     segment with the same `artifactId` *within the same
 *     message*. Phantom artifacts are never created.
 *   - `append=true` after `lastChunk=true` is dropped.
 *   - `append=false` with a duplicate `artifactId` within the
 *     same message is dropped (append protocol is monotone).
 *   - `lastChunk=true` flips `isComplete` and is otherwise a
 *     no-op on the segment list.
 *
 * The reducer is exported from `lib/append-artifact-segment.ts`
 * (a pure module — no React, no CSS imports) so it can be unit
 * tested in isolation under Playwright's node test loader.
 * The 10 cases below are the pinned contract — a future
 * refactor that breaks any one of them should fail loudly here.
 */
import { expect, test } from '@playwright/test';
import type { TaskPart } from '../../../src/api/types';
import { appendArtifactSegment } from '../../../src/lib/append-artifact-segment';

const part = (text: string): TaskPart => ({ text });

test.describe('appendArtifactSegment — strict A2A §3.7 protocol', () => {
  test('new artifact with append=undefined pushes a segment', () => {
    const prev: never[] = [];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: {
        artifactId: 'a-1',
        parts: [part('hello')],
      },
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { type: string }).type).toBe('artifact');
    expect((next[0] as { artifactId: string }).artifactId).toBe('a-1');
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([{ text: 'hello' }]);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(false);
  });

  test('append=true merges parts into existing artifact, leaves name/id alone', () => {
    const initial = {
      id: 'seg-1',
      type: 'artifact',
      content: '',
      artifactId: 'a-1',
      artifactName: 'my-name',
      artifactParts: [part('hello')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part(' world')], name: 'should-be-ignored' },
      append: true,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { artifactName: string }).artifactName).toBe('my-name');
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([
      { text: 'hello' },
      { text: ' world' },
    ]);
  });

  test('append=true with no prior artifact in this message is dropped', () => {
    const prev: never[] = [];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('hello')] },
      append: true,
    });
    expect(next).toHaveLength(0);
  });

  test('append=false with duplicate artifactId within same message is dropped', () => {
    const initial = {
      id: 'seg-1',
      type: 'artifact',
      content: '',
      artifactId: 'a-1',
      artifactParts: [part('hello')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('replace-me')] },
      append: false,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([{ text: 'hello' }]);
  });

  test('append=true after lastChunk=true is dropped', () => {
    const initial = {
      id: 'seg-1',
      type: 'artifact',
      content: '',
      artifactId: 'a-1',
      artifactParts: [part('hello')],
      isComplete: true,
    };
    const prev: never[] = [initial as never];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('extra')] },
      append: true,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([{ text: 'hello' }]);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(true);
  });

  test('lastChunk=true flips isComplete without adding parts', () => {
    const initial = {
      id: 'seg-1',
      type: 'artifact',
      content: '',
      artifactId: 'a-1',
      artifactParts: [part('hello')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [] },
      lastChunk: true,
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(true);
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([{ text: 'hello' }]);
  });

  test('multiple append=true events accumulate correctly', () => {
    let prev: never[] = [];
    prev = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('a')] },
    });
    prev = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('b')] },
      append: true,
    });
    prev = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('c')] },
      append: true,
    });
    expect(prev).toHaveLength(1);
    expect((prev[0] as { artifactParts: TaskPart[] }).artifactParts.map((p) => p.text)).toEqual([
      'a',
      'b',
      'c',
    ]);
  });

  test('append protocol is per-message scope: duplicate id across messages is allowed', () => {
    const initial = {
      id: 'seg-1',
      type: 'artifact',
      content: '',
      artifactId: 'a-1',
      artifactParts: [part('first')],
      isComplete: false,
    };
    const prev: never[] = [initial as never];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-1', parts: [part('fresh')] },
    });
    expect(next).toHaveLength(2);
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([{ text: 'first' }]);
    expect((next[1] as { artifactParts: TaskPart[] }).artifactParts).toEqual([{ text: 'fresh' }]);
    expect((next[1] as { isComplete: boolean }).isComplete).toBe(false);
  });
});

test.describe('appendArtifactSegment — empty / degenerate inputs', () => {
  test('empty parts array still creates the segment', () => {
    const prev: never[] = [];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: 'a-empty', parts: [] },
    });
    expect(next).toHaveLength(1);
    expect((next[0] as { artifactParts: TaskPart[] }).artifactParts).toEqual([]);
    expect((next[0] as { isComplete: boolean }).isComplete).toBe(false);
  });

  test('event without artifactId is dropped', () => {
    const prev: never[] = [];
    const next = appendArtifactSegment(prev, {
      taskId: 't1',
      contextId: 'c1',
      artifact: { artifactId: '', parts: [] },
    });
    expect(next).toHaveLength(0);
  });
});
