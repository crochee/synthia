# doom-loop-early-exit Specification

## Purpose

Define the doom-loop early-exit signal: when the LLM produces 3 consecutive identical tool calls, `LoopDetectorSet` SHALL return `LoopAction::RequirePermission` so the caller can break the loop via a user permission check (mirrors opencode's `doom_loop` permission category).

## ADDED Requirements

### Requirement: LoopDetectorSet shall emit a RequirePermission action when doom loop is detected

When the `DoomLoopDetector` returns `LoopStatus::Detected` (3 consecutive identical `(tool, args)` calls), the `LoopDetectorSet::check()` SHALL return `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`. The `RequirePermission` action SHALL signal the caller to invoke `synthia_permission::Permission::ask` to break the loop.

#### Scenario: Doom loop triggers permission request
- **WHEN** the same `(tool_name, args_json)` is checked 3 consecutive times
- **THEN** `check()` SHALL return `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`
- **AND** the `Detected` status SHALL carry the highest severity (Critical)
- **AND** the caller (agent's `StepExecutor`) SHALL be responsible for invoking `permission.ask()`

#### Scenario: Caller must invoke permission.ask on RequirePermission
- **WHEN** the caller (e.g. `synthia-agent/src/stream_builder/builder.rs`) receives `(Detected, Some(RequirePermission))`
- **THEN** it SHALL call `synthia_permission::Permission::ask(...)` with the doom-loop permission category
- **AND** it SHALL pass the detected `(tool_name, args_json)` to the permission request
- **AND** the permission decision (allow / deny) SHALL determine whether the tool is executed

#### Scenario: Caller ignoring RequirePermission falls back to default behavior
- **WHEN** a caller does NOT handle `LoopAction::RequirePermission` explicitly
- **THEN** the caller SHALL still be able to block the loop using `LoopStatus::Detected` alone
- **AND** the worst-case behavior SHALL be identical to the pre-migration behavior (block without asking)

---

### Requirement: Doom loop detection shall follow opencode semantics

The doom loop detection logic in `synthia_guardian::DoomLoopDetector` SHALL mirror opencode's `DOOM_LOOP_THRESHOLD = 3` semantics: 3 consecutive tool calls with the **same tool name AND identical JSON-serialized input** SHALL trigger detection.

#### Scenario: Identical (tool, input_json) triple
- **WHEN** 3 consecutive calls have the same `tool_name` AND `JSON.stringify(args)` produces identical output
- **THEN** `DoomLoopDetector` SHALL return `Detected`

#### Scenario: Different args reset the window
- **WHEN** 2 calls are identical and the 3rd call has different `args_json`
- **THEN** `DoomLoopDetector` SHALL return `Ok` for the 3rd call
- **AND** the window SHALL contain only the 2nd and 3rd calls going forward

#### Scenario: Different tool name resets the window
- **WHEN** 2 calls use `tool_a` and the 3rd call uses `tool_b`
- **THEN** `DoomLoopDetector` SHALL return `Ok` for the 3rd call
- **AND** the window SHALL reset to only `tool_b`

---

### Requirement: Doom loop detection shall not require iteration counter

`DoomLoopDetector` SHALL detect loops based solely on the 3-call sliding window of `(tool_name, args_json)`. It SHALL NOT depend on the `iteration` argument passed to `check()`. This matches opencode's implementation, which uses `parts.slice(-DOOM_LOOP_THRESHOLD)` without an iteration counter.

#### Scenario: Iteration independence
- **WHEN** `DoomLoopDetector::check()` is called repeatedly with the same `(tool, args)` regardless of `iteration` value
- **THEN** it SHALL return `Detected` on the 3rd call exactly the same way
- **AND** the `iteration` parameter SHALL have no effect on DoomLoop behavior

---

### Requirement: End-to-end doom loop scenario test

A new e2e test in `synthia-e2e/src/scenarios/loop_detection.rs` SHALL verify that doom loop detection triggers a permission ask in the agent's main loop.

#### Scenario: E2E doom loop triggers permission
- **WHEN** the agent calls the same tool 3 times with identical input
- **THEN** `synthia_permission::Permission::ask` SHALL be invoked with the doom-loop category
- **AND** the agent SHALL NOT execute the 3rd tool call without an explicit user decision
