# prune-idempotent-marker Specification

## Purpose

The compaction/pruning subsystem in `synthia-context` must provide an idempotent mechanism for marking old tool result messages as "cleared" while preserving the original content in storage. This enables KV cache prefix stability (the `time.compacted` timestamp is the only change) and supports replay/recovery (the original content is still available in storage).

## ADDED Requirements

### Requirement: Message SHALL have a tool_result_cleared_at field

The `synthia_context::Message` struct MUST include a `tool_result_cleared_at: Option<Instant>` field. The field MUST be serialized with `#[serde(default)]` so old messages without this field deserialize successfully (defaulting to `None`).

#### Scenario: New messages have field as None by default
- **WHEN** a new `Message` is constructed
- **THEN** `tool_result_cleared_at` SHALL be `None` by default

#### Scenario: Old messages deserialize without the field
- **WHEN** a JSON payload from pre-change storage is deserialized (lacking the `tool_result_cleared_at` field)
- **THEN** deserialization SHALL succeed
- **AND** the field SHALL be `None` for the deserialized message

#### Scenario: Setting the field persists across round-trip
- **WHEN** a message with `tool_result_cleared_at = Some(instant)` is serialized and then deserialized
- **THEN** the deserialized message MUST have the same instant value

### Requirement: prune() function MUST perform single-pass reverse scan with PRUNE_PROTECT budget

A `pub fn prune(messages: &mut Vec<Message>, protect_tokens: u32) -> PruneStats` function MUST be defined in `synthia_context::pruning`. The function MUST perform a single-pass reverse scan and mark tool result messages with `tool_result_cleared_at = Some(Instant::now())` when their cumulative token estimate would exceed `protect_tokens`.

#### Scenario: Single-pass reverse scan
- **WHEN** `prune(messages, 40_000)` is called with N messages
- **THEN** the function SHALL iterate messages in reverse order exactly once
- **AND** SHALL NOT make forward passes
- **AND** the time complexity SHALL be O(N) where N = messages.len()

#### Scenario: PRUNE_PROTECT budget is respected
- **WHEN** the cumulative token estimate of the most recent tool results (counting from the end) is ≤ `protect_tokens`
- **THEN** those messages SHALL NOT be marked
- **AND** older messages beyond the budget SHALL be marked

#### Scenario: Idempotent stop on previously cleared message
- **WHEN** the reverse scan encounters a message with `tool_result_cleared_at = Some(_)`
- **THEN** the function SHALL stop scanning (break out of the loop)
- **AND** all messages after that cleared message (in reverse order = older) SHALL remain unchanged

#### Scenario: PRUNE_PROTECT default value is 40,000 tokens
- **WHEN** a caller does not provide a custom protect_tokens value
- **THEN** the function SHOULD accept a default argument equal to 40,000 tokens
- **AND** a `pub const PRUNE_PROTECT_TOKENS: u32 = 40_000;` constant SHALL be exported from the module

#### Scenario: PruneStats reports counts
- **WHEN** `prune()` completes
- **THEN** the returned `PruneStats` SHALL include at minimum: `marked_count: usize` (number of newly marked messages) and `scanned_count: usize` (number of messages visited)

#### Scenario: Non-tool messages are skipped
- **WHEN** the scan encounters a non-tool message (e.g., user text, assistant text, system message)
- **THEN** the function SHALL NOT mark it
- **AND** SHALL continue scanning

### Requirement: Rendering layer MUST honor tool_result_cleared_at

The message rendering layer (used in `truncate_messages` and `step_sample` before sending to the LLM) MUST check the `tool_result_cleared_at` field. When the field is `Some(_)`, the renderer MUST replace the message content with a placeholder string and MUST NOT include the original content in the LLM-visible output.

#### Scenario: Cleared message is rendered as placeholder
- **WHEN** a message has `tool_result_cleared_at = Some(instant)` and the rendering layer processes it
- **THEN** the rendered output MUST be a placeholder containing the timestamp
- **AND** MUST NOT contain the original message content

#### Scenario: Placeholder format is consistent
- **WHEN** a cleared message is rendered
- **THEN** the placeholder SHALL be formatted as: `"[Old tool result content cleared at {ISO8601_timestamp}]"`

#### Scenario: Original content is preserved in storage
- **WHEN** a message is marked as cleared via `prune()`
- **THEN** the original `Message.content` field SHALL be unchanged in the in-memory `Vec<Message>`
- **AND** any persistence layer (e.g., `event_log` JSONL) SHALL still record the full original content
- **AND** only the rendering layer (LLM-visible) SHALL show the placeholder

#### Scenario: Non-cleared messages render normally
- **WHEN** a message has `tool_result_cleared_at = None`
- **THEN** the rendering layer SHALL include the original content unchanged
