<!--
Delta spec for prune-idempotent-marker capability.
Modifies openspec/specs/prune-idempotent-marker/spec.md
-->

## ADDED Requirements

### Requirement: Renderer SHALL honor tool_result_cleared_at for ContentPart::ToolResult shape

The message rendering layer (used in `truncate_messages` and `step_sample` before sending to the LLM) MUST honor the `tool_result_cleared_at` field for messages whose `content` is `Content::Single(ContentPart::ToolResult(_))` (the Anthropic / OpenAI on-the-wire shape that `prune()` actually marks). When the field is `Some(_)` and the message is a tool-result message, the renderer MUST replace the text inside the first `ContentPart::Text` of `ToolResult.content[]` with a placeholder string and MUST NOT include the original tool payload in the LLM-visible output. The on-the-wire `content` structure (role, tool_use_id, is_error, structured_content) MUST be preserved; only the in-memory text is swapped.

#### Scenario: Cleared tool-result with ContentPart::ToolResult renders placeholder
- **WHEN** a message has `content = Content::Single(ContentPart::ToolResult(_))` and `tool_result_cleared_at = Some(_)`
- **THEN** the renderer MUST replace the first text inside `ToolResult.content[]` with the cleared placeholder
- **AND** the original `content` structure (role, `tool_use_id`, `is_error`) MUST be preserved in storage

#### Scenario: Cleared tool-result with ContentPart::Text renders placeholder
- **WHEN** a message has `content = Content::Single(ContentPart::Text(_))` or `content = Content::Multi(vec![ContentPart::Text(_)])` and `tool_result_cleared_at = Some(_)`
- **THEN** the renderer MUST replace the top-level `ContentPart::Text.text` with the cleared placeholder
- **AND** the original `content` structure (role, `tool_call_id`) MUST be preserved in storage

#### Scenario: Cleared message with no text-like field is a no-op
- **WHEN** a message has `tool_result_cleared_at = Some(_)` but its `content` contains no `ContentPart::Text` (e.g., `Content::Single(ContentPart::Image(_))` or an empty `ToolResult.content[]`)
- **THEN** the renderer MUST NOT panic
- **AND** the renderer MUST treat this as a no-op (skip the placeholder branch, do not fall through to size-based truncation)

#### Scenario: Rendered placeholder format matches cleared_placeholder()
- **WHEN** any cleared message is rendered
- **THEN** the LLM-visible text MUST be exactly `cleared_placeholder(at)` where `at` is the value of `tool_result_cleared_at`
- **AND** the placeholder format MUST be `"[Old tool result content cleared at {ISO8601_timestamp}]"` (as defined in `synthia_context::truncate::cleared_placeholder`)
