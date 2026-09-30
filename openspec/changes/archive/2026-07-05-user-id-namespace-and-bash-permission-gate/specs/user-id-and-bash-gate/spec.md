<!--
Delta spec for the user-id-and-bash-gate change.

Per OpenSpec rules:
- This is an ADDED-only delta (no existing user-id-and-bash-gate spec to modify).
- Each Requirement's first sentence MUST contain SHALL or MUST.
- Each Scenario MUST use level-4 heading (`####`) with WHEN/THEN format.
- Every Requirement MUST have at least one Scenario.
-->

## ADDED Requirements

### Requirement: Session Persistence User-ID Namespace

The `synthia-session` store MUST namespace all session paths by `user_id` in the form `{sessions_root}/{user_id}/{session_id}/` relative to the session root. The system MUST create the `user_id` directory with mode `0o700` on Unix and MUST refuse to list or load sessions whose `owner_user_id` does not match the caller's `user_id`. The `Session` struct MUST carry a `user_id: String` field, and `SessionMetadata` MUST carry an `owner_user_id: String` field, both with `#[serde(default)]` to remain backward-compatible with previously written JSONL.

#### Scenario: Cross-user session load is refused
- **WHEN** a caller with `user_id = "alice"` attempts to load a session whose `owner_user_id = "bob"`
- **THEN** the loader MUST return `Err(HashChainError::CrossUserAccess { caller: "alice", owner: "bob" })` and MUST NOT touch the on-disk files

#### Scenario: User directory is created with mode 0o700
- **WHEN** a new session is created for `user_id = "alice"`
- **THEN** the on-disk path is `sessions/alice/{session_id}/` and on Unix the `alice` directory MUST have Unix permission bits `0o700`

#### Scenario: Legacy layout migration is automatic
- **WHEN** a session exists at the legacy path `sessions/{session_id}/` (no `user_id` intermediate) and the loader is invoked
- **THEN** the loader MUST read the legacy layout, write the new layout `sessions/{user_id}/{session_id}/`, and MUST NOT lose any data

#### Scenario: Empty user_id is rejected
- **WHEN** `Session::new_with_user(id, "")` is called with an empty `user_id`
- **THEN** the constructor MUST return `Err(StoreError::EmptyUserId)` and MUST NOT create any on-disk files

### Requirement: promptCacheKey HMAC Includes User-ID Namespace

The system MUST compute the LLM provider's `prompt_cache_key` as `HMAC-SHA256(user_id || session_id)[:16]` (hex-encoded 32 characters), with a secret derived per process via `rand::thread_rng().gen()`. The system MUST NOT use `session_id` alone or any suffix of it as the cache key. The cache key MUST be injected into `providerOptions.prompt_cache_key` for OpenAI-compatible providers and the equivalent `cache_control` namespace for Anthropic.

#### Scenario: HMAC key is deterministic for fixed inputs
- **WHEN** `user_id = "alice"`, `session_id = "sess-1"`, and the same HMAC secret are provided to `compute_prompt_cache_key`
- **THEN** the resulting `prompt_cache_key` MUST be byte-identical across two calls

#### Scenario: Different users produce different keys for the same session id
- **WHEN** `user_id = "alice"` and `user_id = "bob"` request a cache key for the same `session_id`
- **THEN** the two keys MUST differ (no shared prefix of length 16) and MUST NOT collide

#### Scenario: Empty user_id is rejected
- **WHEN** `compute_prompt_cache_key("", "sess-1", secret)` is called
- **THEN** the function MUST return `Err(CacheKeyError::EmptyUserId)` and MUST NOT produce a cache key

#### Scenario: Process restart invalidates the cache key
- **WHEN** the process is restarted and a new HMAC secret is generated
- **THEN** the cache key for the same `(user_id, session_id)` MUST be different from the previous process's key (causing a cache miss, which is an accepted Stage 1 degradation per P4)

### Requirement: AgentEvent Version and Sequence Fields

Every `AgentEvent` variant MUST carry a `version: u32` field set to the constant `AGENT_EVENT_SCHEMA_VERSION = 1` and a `seq: u64` field assigned monotonically by `AgentEventEmitter::pair()`. The `seq` MUST be assigned from a process-local `AtomicU64` counter starting at `1` and MUST be unique within a session. Both fields MUST be marked `#[serde(default)]` so that events written by a previous schema version can still be read.

#### Scenario: Old reader loads new event
- **WHEN** an `AgentEvent` written by the new schema (with `version: 1` and `seq: 42`) is read by code compiled against the old schema (no `version` / `seq` fields)
- **THEN** the old reader MUST NOT fail to deserialize the event (default values `0` for `version`, `0` for `seq` are accepted)

#### Scenario: New reader loads old event
- **WHEN** an `AgentEvent` written by the old schema (no `version` / `seq` fields) is read by code compiled against the new schema
- **THEN** the new reader MUST populate `version: 0` and `seq: 0` as defaults and MUST NOT fail to deserialize

#### Scenario: seq is monotonically increasing
- **WHEN** `AgentEventEmitter::pair()` is invoked 100 times in a single process
- **THEN** the resulting `seq` values MUST be `1, 2, 3, ..., 100` with no gaps and no duplicates

### Requirement: EventLogger Debounced Flush With Critical Bypass

The `EventLogger` MUST accept a `flush_interval: Duration` constructor argument and MUST start a background flush task that calls `sync_all` at most every `flush_interval`. The events of types `Decision`, `Error`, and `ToolResult { is_error: true }` MUST bypass the debounce and MUST be flushed synchronously via `critical_flush`. The agent loop MUST wire up the `EventLogger` to the run entry point so that events are actually persisted.

#### Scenario: Critical event is flushed immediately
- **WHEN** a `ToolResult { is_error: true }` event is emitted and the `EventLogger` is configured with `flush_interval = 50ms`
- **THEN** the event MUST appear on disk within the same `write_all + sync_all` call (no debounce delay) and MUST survive a process kill at the next instruction

#### Scenario: Non-critical event uses debounce
- **WHEN** an `LlmStreamDelta` event is emitted under the same configuration
- **THEN** the event MAY be batched for up to 50ms before `sync_all` and MUST be visible to a subsequent `read_all` call after the next flush tick

#### Scenario: Wire-up to synthia-agent run entry
- **WHEN** `synthia-agent::run` is invoked
- **THEN** an `EventLogger` MUST be constructed with `flush_interval = 50ms` and MUST be passed to the agent loop, persisting all emitted events

### Requirement: BashTool Routes Through PermissionChecker

The `BashTool` MUST implement the `Tool` trait and MUST be registered in `ToolRegistry::register_defaults`. The tool's `call` method MUST return a `ToolOutput` value, and the bash command string MUST be passed to `PermissionChecker::check` as a `PermissionRequest { tool_name: "Bash", action: Action::RunBash(cmd) }` BEFORE the command is executed. The `MergedPolicy::evaluate` function MUST return `Err(PermissionError::UnregisteredTool)` when the `tool_name` is not in the rule table (fail-closed). The `CommandBlacklist` MUST remain as a defense-in-depth secondary check using AND-logic with the policy: both must approve for execution to proceed. The bash tool MUST return `is_concurrency_safe() -> false` so the agent scheduler serializes its invocations.

#### Scenario: Bash command is gated by policy
- **WHEN** `BashTool::call` is invoked with `cmd = "rm -rf /"`
- **THEN** the policy MUST be consulted via `PermissionChecker::check` first; if the policy returns `Deny` or the `CommandBlacklist` denies, the command MUST be blocked and the tool MUST return `ToolOutput::error("denied by policy: {reason}")` without executing

#### Scenario: BashTool is discoverable by LLM
- **WHEN** the agent loop calls `ToolRegistry::list()`
- **THEN** the list MUST contain an entry with name `Bash` and `requires_permission() -> true`

#### Scenario: Unknown tool name is hard-denied
- **WHEN** `MergedPolicy::evaluate("BashX", ...)` is called with a tool name not in the rule table
- **THEN** the function MUST return `Err(PermissionError::UnregisteredTool)` and the caller MUST treat the action as denied

#### Scenario: CommandBlacklist is defense-in-depth with AND logic
- **WHEN** the policy approves a command but the `CommandBlacklist` denies it
- **THEN** the command MUST still be blocked and the tool MUST return `ToolOutput::error("denied by CommandBlacklist: {reason}")`

#### Scenario: Concurrent bash invocations are serialized
- **WHEN** two agent tasks invoke `BashTool` simultaneously
- **THEN** the scheduler MUST serialize them (second invocation waits for first to complete) because `is_concurrency_safe` returns `false`

### Requirement: UTF-8 Safe Truncation Public Helper

The `synthia-tool/builtin/utf8_safe.rs` module MUST export `cap_to_char_boundary(s: &mut String, max_bytes: usize)` that scans backward from `max_bytes` to the nearest valid UTF-8 character boundary. The `web.rs:147-148` and `grep.rs:34-40` truncation paths MUST use this helper. The helper MUST NOT panic on any input, including CJK 3-byte characters, emoji 4-byte characters, mixed-multibyte strings, and empty inputs.

#### Scenario: Multi-byte truncation does not panic
- **WHEN** a `String` ending in a 3-byte Chinese character (e.g., `"中文"`) is truncated at a byte index inside that character
- **THEN** the function MUST scan backward to the character boundary, truncate, and MUST NOT panic

#### Scenario: Emoji 4-byte truncation does not panic
- **WHEN** a `String` ending in a 4-byte emoji character (e.g., `"😀😀"`) is truncated at a byte index inside that character
- **THEN** the function MUST scan backward to the character boundary, truncate, and MUST NOT panic

#### Scenario: ASCII truncation is identical
- **WHEN** the input contains only ASCII characters and `max_bytes` lands on a character boundary
- **THEN** the truncated string MUST be byte-identical to `s.truncate(max_bytes)`

#### Scenario: Empty input and zero max_bytes
- **WHEN** the input is empty OR `max_bytes == 0`
- **THEN** the function MUST handle both cases without panic (empty input → no-op; zero max_bytes → empty result)

#### Scenario: web.rs and grep.rs use the public helper
- **WHEN** `WebFetchTool::call` or `GrepTool::call` is invoked on content with CJK or emoji characters
- **THEN** the truncation path MUST use `utf8_safe::cap_to_char_boundary` and MUST NOT panic

## MODIFIED Requirements

- (none — 不修改现有 spec 的 requirement；本 change 是 `user-id-and-bash-gate` 新 capability，archive 流程后由 cumulative spec 统一管理)
