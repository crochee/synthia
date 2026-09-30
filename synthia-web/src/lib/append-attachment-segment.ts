/**
 * Reducer for `attachment` events in the chat stream.
 *
 * Lives in `lib/` (not in `pages/ChatPage.tsx`) because:
 *   - Pure module, no React / CSS imports
 *   - Unit-testable under Playwright's node test loader
 *   - Importable by other pages in the future without pulling
 *     in ChatPage's component graph
 *
 * Strict protocol:
 *
 *   - `append=true` only merges parts into an existing attachment
 *     segment with the same `attachmentId` *within the same
 *     message*. Phantom artifacts are never created from an
 *     orphan append.
 *   - `append=true` after `lastChunk=true` is dropped.
 *   - `append=false` with a duplicate `attachmentId` within the
 *     same message is dropped (append protocol is monotone).
 *   - `append` undefined (implicit) with a duplicate
 *     `attachmentId` AND `lastChunk=true` flips `isComplete`
 *     on the existing segment without adding parts.
 *   - `append` undefined (implicit) with a duplicate
 *     `attachmentId` AND no `lastChunk` falls through to
 *     create a NEW segment — this is the per-message scope
 *     rule (see below).
 *   - `lastChunk=true` flips `isComplete` without adding parts.
 *
 * The caller passes the *current message's* segments (not the
 * whole chat) — this is the per-message scope that makes "new
 * attachment in a later message" legal even when an earlier
 * message had an attachment with the same id.
 *
 * Every dropped event emits a `console.warn` so a future server
 * bug shows up in devtools instead of silently corrupting state.
 */

import type { MessageSegment } from '../api/chat-message';
import type { SessionPart } from '../api/types';

export function appendAttachmentSegment(
  prevSegments: ReadonlyArray<MessageSegment>,
  event: {
    sessionId: string;
    contextId: string;
    attachment: {
      attachmentId: string;
      name?: string;
      parts: ReadonlyArray<unknown>;
      metadata?: Record<string, unknown>;
    };
    append?: boolean;
    lastChunk?: boolean;
  },
): MessageSegment[] {
  const attachmentId = event.attachment?.attachmentId;
  if (typeof attachmentId !== 'string' || attachmentId.length === 0) {
    console.warn('[attachment] event missing attachmentId; dropping');
    return [...prevSegments];
  }
  const incomingParts = (
    Array.isArray(event.attachment.parts) ? event.attachment.parts : []
  ) as ReadonlyArray<SessionPart>;

  if (event.append === true) {
    const idx = prevSegments.findIndex(
      (s) => s.type === 'attachment' && s.attachmentId === attachmentId,
    );
    if (idx < 0) {
      console.warn(
        `[attachment] append=true with no prior attachment (id=${attachmentId}); dropping`,
      );
      return [...prevSegments];
    }
    const existing = prevSegments[idx];
    if (existing.isComplete === true) {
      console.warn(`[attachment] append=true after lastChunk (id=${attachmentId}); dropping`);
      return [...prevSegments];
    }
    const merged: MessageSegment = {
      ...existing,
      attachmentParts: [...(existing.attachmentParts ?? []), ...incomingParts],
    };
    const out = [...prevSegments];
    out[idx] = merged;
    if (event.lastChunk === true) {
      out[idx] = { ...out[idx], isComplete: true };
    }
    return out;
  }

  // append !== true. Three sub-cases:
  //
  //   1. `append === false` (explicit) with a duplicate
  //      `attachmentId` already in the same message — drop
  //      (append protocol is monotone within one message).
  //   2. `append === undefined` (implicit) with a duplicate
  //      `attachmentId` AND `lastChunk === true` — flip
  //      `isComplete` on the existing segment; do NOT add
  //      parts, do NOT create a new segment. This is the
  //      "lastChunk without an explicit append" path that
  //      closes out an in-progress attachment.
  //   3. `append === undefined` (implicit) with a duplicate
  //      `attachmentId` AND no `lastChunk` — fall through and
  //      create a NEW segment. This covers the per-message
  //      scope rule: the caller passes only the current
  //      message's segments, so a fresh attachment in this
  //      message with an id that was used in an earlier
  //      message must not collide with the prior segment.
  const dupeIdx = prevSegments.findIndex(
    (s) => s.type === 'attachment' && s.attachmentId === attachmentId,
  );
  if (dupeIdx >= 0) {
    if (event.append === false) {
      console.warn(
        `[attachment] append=false with duplicate attachmentId in message (id=${attachmentId}); dropping`,
      );
      return [...prevSegments];
    }
    // append === undefined + lastChunk=true → close the
    // existing segment.
    if (event.lastChunk === true) {
      const out = [...prevSegments];
      out[dupeIdx] = { ...out[dupeIdx], isComplete: true };
      return out;
    }
    // append === undefined + no lastChunk → fall through
    // to create a new segment.
  }
  const newSegment: MessageSegment = {
    id:
      typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
        ? crypto.randomUUID()
        : `seg-${Math.random().toString(36).slice(2)}-${Date.now()}`,
    type: 'attachment',
    content: '',
    attachmentId,
    attachmentName: event.attachment.name,
    attachmentParts: [...incomingParts],
    isComplete: event.lastChunk === true,
  };
  return [...prevSegments, newSegment];
}
