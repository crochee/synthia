<!--
Delta spec for tool-output-truncate capability.
Modifies openspec/specs/tool-output-truncate/spec.md
-->

## ADDED Requirements

### Requirement: Cleared-placeholder rendering SHALL support both legacy and Anthropic tool-result shapes

The `truncate_messages` cleared-placeholder branch MUST surface the placeholder for tool-result messages regardless of which on-the-wire shape they use. The two supported shapes are:
1. **Anthropic / OpenAI shape**: `Role::User` (or `Role::Tool` with `tool_use_id` set) + `content = Content::Single(ContentPart::ToolResult(_))` — text lives inside `ToolResult.content[]` as `ContentPart::Text`.
2. **Legacy shape**: `Role::Tool` + `content = Content::Multi(vec![ContentPart::Text(_)])` + `tool_call_id = Some(_)` — text lives at the top level.

For shape (1), the renderer MUST replace the first `ContentPart::Text.text` inside the `ToolResult.content[]` array. For shape (2), the renderer MUST replace the top-level `ContentPart::Text.text`. For any other content variant (e.g., `ContentPart::Image(_)`), the renderer MUST treat the message as a no-op (no panic, no fallthrough).

#### Scenario: Shape 1 (Anthropic) cleared message renders placeholder
- **WHEN** a message has `content = Content::Single(ContentPart::ToolResult(_))` and `tool_result_cleared_at = Some(_)`
- **THEN** the first text inside `ToolResult.content[]` MUST be replaced with the cleared placeholder
- **AND** the `role`, `tool_use_id`, `is_error`, and `structured_content` fields MUST be preserved in storage

#### Scenario: Shape 2 (legacy) cleared message renders placeholder
- **WHEN** a message has `content = Content::Multi(vec![ContentPart::Text(_)])` and `tool_call_id = Some(_)` and `tool_result_cleared_at = Some(_)`
- **THEN** the top-level `ContentPart::Text.text` MUST be replaced with the cleared placeholder
- **AND** the `role` and `tool_call_id` fields MUST be preserved in storage

#### Scenario: Image content cleared message is a no-op
- **WHEN** a message has `content = Content::Single(ContentPart::Image(_))` and `tool_result_cleared_at = Some(_)`
- **THEN** the renderer MUST NOT panic
- **AND** the renderer MUST NOT modify the message
- **AND** the renderer MUST NOT fall through to size-based truncation

#### Scenario: Mixed content cleared message replaces only the first text-like field
- **WHEN** a message has `content = Content::Multi(vec![ContentPart::Text(_), ContentPart::ToolResult(_), ContentPart::Image(_)])` and `tool_result_cleared_at = Some(_)`
- **THEN** the first `ContentPart::Text` in the array MUST be replaced with the cleared placeholder
- **AND** the subsequent `ContentPart::ToolResult` and `ContentPart::Image` MUST NOT be modified

#### Scenario: Idempotent re-render produces identical output
- **WHEN** `truncate_messages` is called twice on the same set of cleared messages with the same `cfg`
- **THEN** the LLM-visible text after the second call MUST be byte-identical to the text after the first call
- **AND** the placeholder MUST NOT be re-stamped with a new timestamp
