/**
 * Drop any 'artifact' segments from a message list before
 * persisting to localStorage. Artifact payloads can be large
 * (file contents, structured data) and the MVP backend doesn't
 * emit them, so persisting them would just burn the 5-10 MB
 * localStorage quota for nothing. On read, we filter again as
 * defence-in-depth in case an older session's localStorage
 * somehow contains them. Spec §4.6.
 *
 * Lives in `lib/` (not in `pages/ChatPage.tsx`) because:
 *   - Pure module, no React / CSS imports
 *   - Unit-testable under Playwright's node test loader
 *   - Importable by any other persistence site in the future
 */

import type { MessageSegment } from '../api/chat-message';

/**
 * Structural shape of any object that carries an array of
 * `MessageSegment`s. Kept as a separate interface (not exported
 * as a constraint) because requiring the `[key: string]: unknown`
 * index signature on `Message` would force every caller to add
 * an index signature — and TypeScript does NOT propagate that
 * through `{ ...m }` correctly. The constraint was relaxed in
 * the 2026-08-15 optimization pass to fix the
 * `ChatPage.tsx:404` typecheck error.
 */
export interface WithSegments {
  segments: ReadonlyArray<MessageSegment>;
}

export function stripArtifactSegments<T extends WithSegments>(messages: ReadonlyArray<T>): T[] {
  return messages.map((m) => ({
    ...m,
    segments: m.segments.filter((s) => s.type !== 'artifact'),
  }));
}
