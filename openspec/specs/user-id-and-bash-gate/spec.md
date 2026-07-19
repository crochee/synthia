<!--
Cumulative spec for the user-id-and-bash-gate capability.

Per OpenSpec rules:
- This file lives in the cumulative path and MUST use the bare `## Requirements` header.
- Each Requirement's first sentence MUST contain SHALL or MUST.
- Each Scenario MUST use level-4 heading (`####`) with WHEN/THEN format.
- Every Requirement MUST have at least one Scenario.
-->

## Purpose

Define the user-isolation boundary for session persistence and the
defense-in-depth permission gate for bash command execution. Session
isolation is implemented by `user_id` namespacing in `synthia-session`,
with the user-to-session mapping enforced at the `synthia-server` layer.
Bash execution is gated by `PermissionChecker` plus a `CommandBlacklist`
for fail-closed defense in depth.

## Requirements

### Requirement: Session Persistence User-ID Namespace

The `synthia-session` store MUST namespace all session paths by `user_id`
in the form `{sessions_root}/{user_id}/{session_id}/` relative to the
session root. The system MUST create the `user_id` directory with mode
`0o700` on Unix and MUST refuse to list or load sessions whose
`owner_user_id` does not match the caller's `user_id`. The `Session`
struct MUST carry a `user_id: String` field, and `SessionMetadata` MUST
carry an `owner_user_id: String` field, both with `#[serde(default)]` to
remain backward-compatible with previously written JSONL.

#### Scenario: Cross-user session load is refused
- **WHEN** a caller with `user_id = "alice"` attempts to load a session
  whose `owner_user_id = "bob"`
- **THEN** the loader MUST return
  `Err(HashChainError::CrossUserAccess { caller: "alice", owner: "bob" })`
  and MUST NOT touch the on-disk files

#### Scenario: User directory is created with mode 0o700
- **WHEN** a new session is created for `user_id = "alice"`
- **THEN** the on-disk path is `sessions/alice/{session_id}/` and on Unix
  the `alice` directory MUST have Unix permission bits `0o700`

#### Scenario: Legacy sessions require explicit user assignment
- **WHEN** a session is created via the legacy `Session::new` path (no
  `user_id`) or exists at the legacy path `sessions/{session_id}/`
- **THEN** the session is parked in the legacy namespace and MUST be
  promoted via `SessionManager::assign_user` before it can be persisted
  under `sessions/{user_id}/{session_id}/`; the store MUST NOT lose any
  data during promotion

#### Scenario: Empty user_id is rejected
- **WHEN** `Session::new_with_user(id, "")` is called with an empty
  `user_id`
- **THEN** the constructor MUST return `Err(StoreError::EmptyUserId)` and
  MUST NOT create any on-disk files

### Requirement: BashTool Routes Through PermissionChecker

The `BashTool` MUST implement the `Tool` trait and MUST be registered in
`ToolRegistry::register_defaults`. The tool's `call` method MUST return a
`ToolOutput` value, and the bash command string MUST be passed to
`PermissionChecker::check` as a `PermissionRequest { tool_name: "Bash",
action: Action::RunBash(cmd) }` BEFORE the command is executed. The
`MergedPolicy::evaluate` function MUST return
`Err(PermissionError::UnregisteredTool)` when the `tool_name` is not in
the rule table (fail-closed). The `CommandBlacklist` MUST remain as a
defense-in-depth secondary check using AND-logic with the policy: both
must approve for execution to proceed. The bash tool MUST return
`is_concurrency_safe() -> false` so the agent scheduler serializes its
invocations.

#### Scenario: Bash command is gated by policy
- **WHEN** `BashTool::call` is invoked with `cmd = "rm -rf /"`
- **THEN** the policy MUST be consulted via `PermissionChecker::check`
  first; if the policy returns `Deny` or the `CommandBlacklist` denies,
  the command MUST be blocked and the tool MUST return
  `ToolOutput::error("denied by policy: {reason}")` without executing

#### Scenario: BashTool is discoverable by LLM
- **WHEN** the agent loop calls `ToolRegistry::list()`
- **THEN** the list MUST contain an entry with name `Bash` and
  `requires_permission() -> true`

#### Scenario: Unknown tool name is hard-denied
- **WHEN** `MergedPolicy::evaluate("BashX", ...)` is called with a tool
  name not in the rule table
- **THEN** the function MUST return `Err(PermissionError::UnregisteredTool)`
  and the caller MUST treat the action as denied

#### Scenario: CommandBlacklist is defense-in-depth with AND logic
- **WHEN** the policy approves a command but the `CommandBlacklist`
  denies it
- **THEN** the command MUST still be blocked and the tool MUST return
  `ToolOutput::error("denied by CommandBlacklist: {reason}")`

#### Scenario: Concurrent bash invocations are serialized
- **WHEN** two agent tasks invoke `BashTool` simultaneously
- **THEN** the scheduler MUST serialize them (second invocation waits for
  first to complete) because `is_concurrency_safe` returns `false`

### Requirement: UTF-8 Safe Truncation Public Helper

The `synthia-tool/builtin/utf8_safe.rs` module MUST export
`cap_to_char_boundary(s: &mut String, max_bytes: usize)` that scans
backward from `max_bytes` to the nearest valid UTF-8 character boundary.
The `web.rs:147-148` and `grep.rs:34-40` truncation paths MUST use this
helper. The helper MUST NOT panic on any input, including CJK 3-byte
characters, emoji 4-byte characters, mixed-multibyte strings, and empty
inputs.

#### Scenario: Multi-byte truncation does not panic
- **WHEN** a `String` ending in a 3-byte Chinese character (e.g.,
  `"中文"`) is truncated at a byte index inside that character
- **THEN** the function MUST scan backward to the character boundary,
  truncate, and MUST NOT panic

#### Scenario: Emoji 4-byte truncation does not panic
- **WHEN** a `String` ending in a 4-byte emoji character (e.g.,
  `"😀😀"`) is truncated at a byte index inside that character
- **THEN** the function MUST scan backward to the character boundary,
  truncate, and MUST NOT panic

#### Scenario: ASCII truncation is identical
- **WHEN** the input contains only ASCII characters and `max_bytes`
  lands on a character boundary
- **THEN** the truncated string MUST be byte-identical to
  `s.truncate(max_bytes)`

#### Scenario: Empty input and zero max_bytes
- **WHEN** the input is empty OR `max_bytes == 0`
- **THEN** the function MUST handle both cases without panic (empty
  input → no-op; zero max_bytes → empty result)

#### Scenario: web.rs and grep.rs use the public helper
- **WHEN** `WebFetchTool::call` or `GrepTool::call` is invoked on content
  with CJK or emoji characters
- **THEN** the truncation path MUST use `utf8_safe::cap_to_char_boundary`
  and MUST NOT panic

