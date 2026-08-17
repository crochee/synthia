/**
 * A2A HTTP client facade.
 *
 * Single entry point for every A2A request the synthia-web
 * frontend issues. Today it only re-exports the streaming
 * `sendMessageStream` from `a2a-stream.ts` (the SDK client
 * singleton lives there); future non-streaming or batch APIs
 * get added in this file so future SDK version bumps touch
 * one location.
 *
 * Wire field naming follows `@a2a-js/sdk@1.0.0` per
 * `docs/interface-contract/ARBITRATION.md` priority 2 (SDK
 * types > Synthia stable spec). The contract-closure test
 * `message-send-camelcase.test.ts` scans THIS file for any
 * snake_case residue in `Message` / `Part` field names
 * (`message_id`, `context_id`, `task_id`, `reference_task_ids`,
 * `media_type`) — DO NOT inline snake_case keys here, route
 * through `a2a-stream.ts` instead.
 *
 * Note: the v1 REST API in `client.ts` uses snake_case for
 * unrelated fields (`context_id`, `next_cursor`, …) because
 * that API has its own `#[serde(rename_all = "snake_case")]`
 * server convention. The two namespaces are independent.
 */

export {
  initA2AClient,
  sendMessageStream,
  classifyPartPayload,
  extractFromMessage,
  type A2AStreamEvent,
  type SegmentType,
  type SegmentMetadata,
  type PartWithMetadata,
} from './a2a-stream.js';
