# tool-input-validation Specification

## Purpose
TBD - created by archiving change subagent-tool-debt-closure. Update Purpose after archive.
## Requirements
### Requirement: ToolAdapter SHALL validate tool input via serde deserialization before execution

The `ToolAdapter<T>` impl SHALL require `T::Input: serde::de::DeserializeOwned` as a trait bound. In `ToolAdapter::execute`, the adapter SHALL call `serde_json::from_value::<T::Input>(request.arguments.clone())` before invoking `tool.call(input)`. If deserialization fails, the adapter SHALL return `ToolOutput::error(format!("Invalid input: {err}"))` without executing the tool.

#### Scenario: Valid input passes validation
- **WHEN** the LLM provides arguments that match the tool's input schema
- **THEN** `serde_json::from_value` SHALL succeed
- **AND** the tool SHALL be called with the deserialized input

#### Scenario: Invalid input type is rejected gracefully
- **WHEN** the LLM provides `{"path": 123}` but the tool expects `path: String`
- **THEN** `serde_json::from_value` SHALL fail
- **AND** the adapter SHALL return `ToolOutput::error("Invalid input: ...")` with the serde error message
- **AND** the tool SHALL NOT be called

#### Scenario: Missing required field is rejected
- **WHEN** the LLM omits a required field from the input schema
- **THEN** `serde_json::from_value` SHALL fail
- **AND** the adapter SHALL return `ToolOutput::error("Invalid input: missing field `xxx`")`
- **AND** the tool SHALL NOT be called

#### Scenario: Extra unknown fields behavior follows serde default
- **WHEN** the LLM provides extra fields not in the input struct
- **THEN** the behavior SHALL follow the tool's `Input` struct serde attributes (default: ignore unknown fields unless `#[serde(deny_unknown_fields)]`)

---

### Requirement: Tool input validation error SHALL be visible to LLM as tool error result

The `ToolOutput::error` returned by failed validation SHALL be propagated through the tool execution pipeline as a normal tool result with `is_error: true`. The LLM SHALL see the error message in the next turn to allow self-correction.

#### Scenario: LLM sees validation error and corrects
- **WHEN** the LLM provides invalid input and receives the error result
- **THEN** the error message SHALL include the specific field and type mismatch
- **AND** the LLM SHALL be able to retry with corrected input in the next turn

