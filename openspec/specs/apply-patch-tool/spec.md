# apply-patch-tool Specification

## Purpose
Define Synthia's built-in `apply_patch` tool that consumes Anthropic V4A patch
format and applies multi-file edits sequentially with structured
`AppliedFailure` reporting. The tool reuses existing `check_path_safety`,
inherits the `write` permission policy, and is registered alongside
`MultiEditTool` in `ToolRegistry::register_defaults()`. The Move operation is
parsed in V4A grammar (for protocol compatibility) but disabled at runtime
by default (`enable_move = false`), aligning with `opencode`'s "moves are
not supported yet" stance and `codex` scenario 015's
`failure_after_partial_success_leaves_changes` semantics.

## Requirements

### Requirement: V4A Patch Parsing

The `apply_patch` tool SHALL parse Anthropic V4A patch format delimited by `*** Begin Patch` and `*** End Patch` markers. The parser MUST recognize four operation headers: `*** Update File: <path>`, `*** Add File: <path>`, `*** Delete File: <path>`, and `*** Move to: <new_path>` (the latter appearing on its own line within an Update or Add block).

#### Scenario: Well-formed patch parses without error
- **WHEN** the LLM provides a patch string with `*** Begin Patch` / `*** End Patch` markers containing valid Update/Add/Delete/Move headers
- **THEN** the parser returns a `Vec<PatchOp>` with one entry per operation in source order, and no error

#### Scenario: Malformed patch is rejected at parse time
- **WHEN** the patch string is missing the `*** Begin Patch` marker, contains an unknown operation header, or has a `*** Move to:` line outside an Update/Add block
- **THEN** the parser returns a structured `ParseError` and the tool surfaces it as `ToolOutput::error` without touching the filesystem

### Requirement: Hunk-Level Diff Application

For each `*** Update File:` operation the tool MUST apply diff-style hunks consisting of context lines (prefix ` `), insertion lines (prefix `+`), and deletion lines (prefix `-`). Hunk matching SHALL be whitespace-sensitive by default. A `*** End of File` marker on a hunk line MUST mean "the following lines must reach the literal end of the file".

#### Scenario: Single hunk updates one block
- **WHEN** the LLM provides a single hunk with two context lines, one deletion, and one insertion matching a contiguous block in the target file
- **THEN** the file is rewritten with the deletion replaced by the insertion, leaving the rest of the file unchanged

#### Scenario: Hunk context mismatch fails fast
- **WHEN** the LLM provides a hunk whose context lines do not match any substring in the target file
- **THEN** the tool returns `ToolOutput::error` indicating the hunk index and the unmatched context (first 50 chars), and the original file is left untouched

### Requirement: Sequential Multi-File Apply with Failure Reporting

The tool MUST execute operations sequentially in source order, applying each hunk and each file operation to the real filesystem one at a time. If a parse or path-safety error occurs before any filesystem write, the tool MUST NOT touch the filesystem. If a write fails mid-patch (e.g., hunk context mismatch, disk full, permission denied), the tool MUST stop at the failed operation, leave the failed operation's file untouched, and return a structured `AppliedFailure` response that enumerates BOTH the operations that were successfully applied and the operation that failed. Earlier-applied operations are intentionally retained to match the V4A semantics of `codex` scenario 015 (`failure_after_partial_success_leaves_changes`) and `opencode`'s explicit "atomic rollback are not supported yet" stance.

#### Scenario: All operations succeed
- **WHEN** a patch contains 3 operations (Update file A, Add file B, Delete file C) and all apply steps succeed
- **THEN** all 3 filesystem mutations are committed in source order, and the tool returns an `Applied` summary listing every operation in order

#### Scenario: Mid-patch failure reports applied + failed
- **WHEN** a patch commits Update file A and Add file B successfully, then fails on a subsequent Update file C (e.g., hunk context mismatch, disk full, permission denied)
- **THEN** the tool stops at file C, returns an `AppliedFailure` response that lists the 2 applied operations (A, B) and the 1 failed operation (C with reason), and file C is left untouched. The earlier-applied A and B are intentionally retained

#### Scenario: Pre-apply parse failure leaves filesystem untouched
- **WHEN** a patch fails at the parse or path-safety stage (no apply started)
- **THEN** the tool returns `ToolOutput::error` with the parse/path-safety failure reason, and no file is modified

### Requirement: Path Safety and Permission Reuse

The tool MUST apply the existing `check_path_safety` check to every file path referenced in the patch, including the source path of an Update/Add/Delete and the target path of a `*** Move to:`. The tool MUST return `requires_permission() -> true` so the Guardian flow reuses the `write` permission policy.

#### Scenario: Path outside workspace is rejected
- **WHEN** any path in the patch resolves outside the workspace root (e.g., `../../../etc/passwd`)
- **THEN** the tool returns `ToolOutput::error` listing the offending path, and the filesystem is not touched

#### Scenario: Move target outside workspace is rejected
- **WHEN** a `*** Move to: <path>` line resolves outside the workspace root
- **THEN** the tool returns `ToolOutput::error` listing the move target, and the source file is not modified

### Requirement: Tool Registration and Concurrency

The tool MUST be registered in `ToolRegistry::register_defaults()` alongside `MultiEditTool`. The tool MUST return `is_concurrency_safe() -> false` so the agent scheduler serializes its invocations. The tool's name MUST be `apply_patch` (single underscore, no hyphen) to match Anthropic convention.

#### Scenario: Tool is discoverable by LLM
- **WHEN** the agent loop requests the tool list from `ToolRegistry`
- **THEN** the list contains an entry with name `apply_patch` and the V4A description in its `description` field

#### Scenario: Concurrent invocations are serialized
- **WHEN** two agent tasks invoke `apply_patch` simultaneously
- **THEN** the scheduler serializes them (second invocation waits for first to complete) because `is_concurrency_safe` returns false
