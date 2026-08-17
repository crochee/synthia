/**
 * Chat-message types used by the frontend's chat UI.
 *
 * Lives in `api/` (not inside `pages/ChatPage.tsx`) because the
 * pure reducers in `lib/` need to import these types without
 * pulling in ChatPage's React component graph and CSS imports.
 * Playwright's node test runner cannot load modules that import
 * `.css` files, so any reducer exported from ChatPage.tsx is
 * un-importable from unit tests.
 */

import type { SegmentType } from './a2a-stream';
import type { TaskPart } from './types';

/**
 * A single typed unit inside a chat `Message`. The chat UI
 * renders one row per segment.
 */
export interface MessageSegment {
  id: string;
  type: SegmentType;
  content: string;
  toolName?: string;
  /** When type === 'tool_block': the request body of the call
   *  (rendered as a yellow sub-block). */
  callContent?: string;
  /** When type === 'tool_block': the output of the call
   *  (rendered as a green sub-block). */
  resultContent?: string;
  /** When type === 'tool_block': the provider-native `tool_use.id`
   *  from the matching `Part::data({id, name, input})`. The chat
   *  reducer uses this to attach a `Part::data({tool_use_id, ...})`
   *  result to the correct block when two blocks are open at once
   *  (parallel or near-parallel tool calls). Falls back to the
   *  trailing-pending heuristic when the wire omits an id. */
  toolUseId?: string;
  /** True while the tool is still executing — the call block
   *  is rendered but the result block is hidden/placeholder. */
  toolPending?: boolean;
  /** When type === 'tool_block': the result was tagged
   *  `is_error: true` by the tool runner (or by the
   *  reconstructed history when seeding the chat from a
   *  task). The renderer paints the result sub-block red so
   *  the user can tell a failing tool from a successful one
   *  at a glance. */
  toolError?: boolean;
  /** When type === 'tool_block' and toolPending === true:
   *  `Date.now()` of the moment the `tool_call` event arrived.
   *  Used to render a timeout indicator once
   *  `Date.now() - pendingSince > TOOL_TIMEOUT_MS` and no
   *  `tool_result` event has been received yet, so the user
   *  can distinguish a still-running tool from a stuck one. */
  pendingSince?: number;
  /** When type === 'artifact': the A2A `artifactId` so
   *  subsequent `append=true` events can find this segment
   *  within the same assistant message and merge into it. */
  artifactId?: string;
  /** When type === 'artifact': the artifact's optional name
   *  (mirrors A2A `Artifact.name`). */
  artifactName?: string;
  /** When type === 'artifact': the accumulating list of
   *  `TaskPart`s for this artifact. Each `append=true` event
   *  pushes new parts onto this array. */
  artifactParts?: TaskPart[];
  /** When type === 'artifact': mirrors the A2A wire's
   *  `lastChunk` flag. False while streaming; flipped to true
   *  once `lastChunk=true` arrives, after which any further
   *  updates for this `artifactId` are dropped. */
  isComplete?: boolean;
}
