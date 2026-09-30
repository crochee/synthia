## Why

The previous `compact-truncate-prune-convergence` change introduced `prune()` to mark old tool-result messages with a `tool_result_cleared_at` timestamp, expecting the `truncate_messages` renderer to surface a placeholder for marked messages. The new integration test exposed a shape mismatch: `prune()`'s `is_tool_result` predicate only recognizes `ContentPart::ToolResult` content, while `truncate_messages`' cleared-placeholder branch keys off `ContentPart::Text` content via `extract_text()`. Net effect: a `prune()`-marked `Role::User + ContentPart::ToolResult` message keeps its original payload in the LLM-visible stream, defeating the purpose of the marker.

No production path triggers this gap today (the agent loop stores tool results in a sidecar, not in `ctx.messages`), but any future change that promotes tool results into `ctx.messages` will hit it immediately. The 5-day-old integration test in the integration suite already constructs a `prune()`-shaped message by hand and shows the gap is real. Closing it now is ~20 LoC of changes, plus 1 spec delta, and zero behavior change for the existing Shape-B test paths.

## What Changes

**Renderer cleared-placeholder branch shape dispatch**
- From: `truncate_messages` checks `msg.content.extract_text().is_some()` to decide whether to surface the placeholder. This matches only the legacy `Role::Tool + ContentPart::Text + tool_call_id` shape.
- To: `truncate_messages` calls a new private helper `replace_first_text_anywhere(&mut msg.content, &marker)` that drills into `ContentPart::ToolResult.content[0].text` for the Anthropic convention (`Role::User + ContentPart::ToolResult`) **and** continues to replace the top-level `ContentPart::Text.text` for the legacy shape. The helper returns `bool`; the renderer treats a `false` as a no-op (no panic, no fallthrough to size-based truncation).
- Reason: closes the gap that the prior change left between the pruner and the renderer, without changing the meaning of `is_tool_result` (which remains a content-shape predicate for `prune()`).
- Impact: non-breaking. The existing 6 cleared-placeholder tests (all Shape B) keep passing unchanged. The new 4 unit tests + 1 production-path integration test cover Shape A.

**Production-path integration test coverage**
- From: no test exercises the chain `prune() → truncate_messages → placeholder` on a Shape A message.
- To: a new integration test in `crates/synthia-context/tests/compact_truncate_pipeline.rs` constructs a Shape A tool-result, calls `prune(&mut msgs, 0)`, then calls `truncate_messages`, and asserts the placeholder is rendered in the LLM-visible text.
- Reason: the new test would have failed before this change; after this change, it passes.
- Impact: non-breaking. Adds 1 test (≈40 LoC), 0 LoC removed.

**Spec deltas (no implementation behavior changes elsewhere)**
- `prune-idempotent-marker`: 1 new ADDED Requirement — "Renderer SHALL honor `tool_result_cleared_at` for messages with `ContentPart::ToolResult` content".
- `tool-output-truncate`: 1 new ADDED Requirement — "Cleared-placeholder rendering SHALL support both legacy `ContentPart::Text` and Anthropic `ContentPart::ToolResult` shapes".

**`is_tool_result` predicate (not changed)**
- The free function in `pruning.rs:43-47` keeps its current `Content::iter().any(ContentPart::ToolResult(_))` definition. Its meaning — "this message has a `ToolResult` part" — is correct for the pruner. Adding `Role::Tool || tool_call_id.is_some()` would let `prune()` start marking Shape B messages, which is **wrong** (Shape B's `ContentPart::Text` is real user-visible text, not a tool payload).

## Capabilities

### New Capabilities
*(none — this change adds requirement-level behavior to two existing capabilities, not new capabilities)*

### Modified Capabilities
- `prune-idempotent-marker`: ADD a requirement that the renderer honors the marker for `ContentPart::ToolResult` content (the shape that `prune()` actually marks).
- `tool-output-truncate`: ADD a requirement that cleared-placeholder rendering supports both on-the-wire shapes (Anthropic `ContentPart::ToolResult` and legacy `ContentPart::Text` + `tool_call_id`).

## Impact

| Layer | Files | Lines |
|-------|-------|-------|
| `synthia-context` (renderer) | `crates/synthia-context/src/truncate.rs` | +52 / -8 |
| `synthia-context` (tests) | `crates/synthia-context/tests/compact_truncate_pipeline.rs` | +40 / 0 |
| `synthia-context` (unit) | `crates/synthia-context/src/truncate.rs` (tests mod) | +75 / 0 |
| `synthia-context` (lib) | `crates/synthia-context/src/pruning.rs` | 0 / 0 (untouched) |
| `synthia-provider` | `crates/synthia-provider/src/types.rs` | 0 / 0 (untouched) |
| `synthia-agent` | stream builder / loop_context / sample / conversation_fix | 0 / 0 (all untouched) |
| OpenSpec | `openspec/specs/prune-idempotent-marker/spec.md` (delta), `openspec/specs/tool-output-truncate/spec.md` (delta) | +35 / 0 |

**API impact**: zero breaking changes. The new helper `replace_first_text_anywhere` is module-private (`fn` not `pub fn`). The existing `truncate_messages` signature is unchanged.

**Wire-format impact**: zero. The renderer still only modifies `content` for cleared messages; on-the-wire serialization (`role` / `tool_use_id` / `tool_call_id`) is unchanged.

**Follow-up deferred (out of scope)**:
- **FU.6** (auto-invoke `prune()` in `StepCompact`): deferred. With tool results not yet in `ctx.messages` in the production loop, wiring it now would scan an empty list. Lands with the change that promotes tool results into `ctx.messages`.
- **FU.2** (pre-computed `original_tokens` in `compact_with_fallback`): unchanged.
- **FU.3** (rustfmt nightly/stable baseline): unchanged.
- **FU.4** (`lifecycle_tools.rs` 308 → < 300 LoC): unchanged.
