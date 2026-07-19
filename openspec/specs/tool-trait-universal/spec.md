# tool-trait-universal Specification

## Purpose
TBD - created by archiving change tool-abstraction-and-extensibility. Update Purpose after archive.
## Requirements
### Requirement: Tool trait SHALL expose execution_mode for orchestrator routing

The `synthia_tool::Tool` trait SHALL expose an `execution_mode()` method returning `ExecutionMode` enum, enabling the orchestrator to route tools to sequential or parallel execution paths.

#### Scenario: Default execution_mode is Parallel
- **WHEN** a tool implementation does not override `execution_mode()`
- **THEN** the default value SHALL be `ExecutionMode::Parallel`
- **AND** the orchestrator SHALL be able to execute the tool concurrently with other Parallel tools in the same batch

#### Scenario: Sequential tool forces batch to sequential execution
- **WHEN** any tool in a tool call batch declares `ExecutionMode::Sequential` via `execution_mode()`
- **THEN** the orchestrator SHALL execute the entire batch sequentially
- **AND** the reason SHALL be traceable via OTel span attribute `orchestrator.batch.reason = "sequential_tool"`

#### Scenario: BashTool declares Sequential
- **WHEN** `BashTool::execution_mode()` is called
- **THEN** the value SHALL be `ExecutionMode::Sequential`
- **AND** no concurrent bash executions SHALL be allowed in the same batch

#### Scenario: WriteTool and EditTool declare Sequential
- **WHEN** `WriteTool::execution_mode()` / `EditTool::execution_mode()` / `MultiEditTool::execution_mode()` / `ApplyPatchTool::execution_mode()` are called
- **THEN** each SHALL return `ExecutionMode::Sequential`

### Requirement: Tool trait SHALL distinguish user_invocable from hidden

The `synthia_tool::Tool` trait SHALL expose `is_user_invocable()` as a method separate from `is_hidden()`. A tool can be both `is_user_invocable=true` AND `is_hidden=true` (visible to LLM, not shown in help).

#### Scenario: Default is_user_invocable is true
- **WHEN** a tool implementation does not override `is_user_invocable()`
- **THEN** the default SHALL be `true`

#### Scenario: load_skill is user-invocable but hidden
- **WHEN** `LoadSkillTool::is_user_invocable()` is called
- **THEN** the value SHALL be `true`
- **AND** `LoadSkillTool::is_hidden()` SHALL be `true`
- **AND** the tool SHALL appear in the LLM's `tool_choice` enumeration
- **AND** the tool SHALL NOT appear in user-facing help text

#### Scenario: Built-in tools remain visible
- **WHEN** a built-in tool like `ReadTool` is registered
- **THEN** `is_user_invocable()` SHALL be `true` (default)
- **AND** `is_hidden()` SHALL be `false` (default)

### Requirement: Tool outputs SHALL be structured with truncation metadata

The `synthia_tool::Tool` trait SHALL expose an `output()` method that converts raw JSON output to a structured `ToolOutput` containing content, metadata, and truncation information.

#### Scenario: ToolOutput structure
- **WHEN** `output()` is called with raw output
- **THEN** it SHALL return `ToolOutput { content: String, metadata: serde_json::Map, truncated_by: Option<TruncatedBy> }`
- **AND** `TruncatedBy` SHALL be `enum { Lines { shown, total }, Bytes { shown, total } }`

#### Scenario: Default output preserves raw value
- **WHEN** a tool implementation does not override `output()`
- **THEN** the default SHALL serialize the raw value to JSON string for `content`
- **AND** `metadata` SHALL be empty
- **AND** `truncated_by` SHALL be `None`

#### Scenario: Truncated output preserves metadata
- **WHEN** a tool's output exceeds 2000 lines or 50KB
- **THEN** `output()` SHALL set `truncated_by` to `Some(Lines { shown: 2000, total: <actual> })` or `Some(Bytes { shown: 50_000, total: <actual> })`
- **AND** the original `total` value SHALL be preserved in metadata for LLM to decide whether to re-read

#### Scenario: UTF-8 safe truncation
- **WHEN** a tool truncates output at byte boundary
- **THEN** the truncation SHALL find the next UTF-8 character boundary
- **AND** SHALL NOT panic on multi-byte characters (per project hard constraint: "Bash tool output truncation must handle multi-byte UTF-8 characters to prevent panic")

---

### Requirement: ToolContext SHALL provide runtime metadata

The `synthia_tool::Tool::call_with_sandbox()` and `call_with_progress()` methods SHALL receive a `ToolContext` containing `cancel_token`, `extension_ctx`, `directory`, `worktree`, and `abort` fields.

#### Scenario: ToolContext structure
- **WHEN** a tool is invoked via `call_with_sandbox()` or `call_with_progress()`
- **THEN** the second argument SHALL be `&ToolContext`
- **AND** `ToolContext` SHALL contain:
  - `cancel_token: &CancellationToken`
  - `extension_ctx: Option<&ExtensionContext>`
  - `directory: &Path` (current project directory, not `process::current_dir()`)
  - `worktree: &Path` (worktree root, used to generate stable relative paths)
  - `abort: AbortSignal`-equivalent (or `CancellationToken` reference)

#### Scenario: Backward compatibility
- **WHEN** a tool implementation uses the old `call_with_sandbox(input, sandbox, &CancellationToken)` signature
- **THEN** the orchestrator SHALL provide a compatibility wrapper that constructs a default `ToolContext` with the cancel token
- **AND** old implementations SHALL continue to compile and run without modification

