# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`MessageKind` enum**: 5-variant classification (`System`, `User`, `Assistant`, `ToolCall`, `ToolResult`) extending the 4-variant `Role` with `ToolCall` for assistant messages containing tool-use requests. Added to `synthia-provider`.
- **`Message::llm_visible()` method**: O(1) side-effect-free method on `Message` that determines whether a message should be included in the LLM context window. Tool results with empty content are excluded.
- **`ToolCategory` enum**: Categorization of tools (`FileSystem`, `Network`, `Computation`, `Agent`, `Other`) for registry metadata. Re-exports from `synthia_core` when `unified-registry` feature is enabled.
- **`ToolMetadataSnapshot` struct**: Immutable snapshot of tool definition metadata (name, description, category, schema, version) for dual-index registry.
- **`ToolPermission` trait + `PermissionDecision` enum**: Thin abstraction for tool-level permission decisions. `PermissionDecision` has three variants: `Allow`, `Deny(String)`, `Ask`. Includes `PermissionAlwaysAllow` and `PermissionAlwaysDeny` default implementations plus `PermissionContext` struct.
- **ToolRegistry dual-index**: `ToolRegistry` now maintains `HashMap<String, ToolEntry>` + `Vec<ToolMetadataSnapshot>` atomically. New `snapshot()` method returns insertion-order-preserved metadata for LLM context building.

### Breaking Changes

- **Public import paths changed**:
  - `synthia_tool::sub_traits::{ToolDefinition, ToolExecution, ToolLifecycle}` removed (never implemented).
  - `synthia_tool::fragment::{ContextFragment, FragmentRegistry, FragmentContext, FragmentError}` → `synthia_context::fragment::{ContextFragment, FragmentRegistry, FragmentContext, FragmentError}`.
  - `synthia_tool::sub_traits::{ToolCategory, ToolMetadataSnapshot}` → `synthia_tool::descriptor::{ToolCategory, ToolMetadataSnapshot}`.
- **MergedPolicy::evaluate fail-closed**: `MergedPolicy::evaluate` now returns `Ask` (not `Allow`) for unknown patterns (ADR-2026-06-10). Migration: explicitly add `Allow` rules for all tools that should be silently allowed.
- **Sandbox renamed to CommandBlacklist**: `synthia_exec::sandbox::Sandbox` is now `synthia_exec::command_blacklist::CommandBlacklist`. The old name implied OS-level containment; the new name accurately describes a string-match blacklist with documented bypass techniques. A deprecated type alias `Sandbox = CommandBlacklist` is kept for one release cycle. The `is_command_allowed` method is preserved (returns `!is_command_blacklisted`) for compatibility; new code should use `is_command_blacklisted` directly.
- **`ApprovalRequest::NetworkAccess::turn_id` removed**: orphan field that had 0 production callers. `Guardian` decision logic no longer reads it; use `AgentContext::current_turn_id` for turn-level decision attribution.
- **`synthia-exec` split into `synthia-tool-bash` + `synthia-tool-exec-base`**: the monolith `synthia-exec` crate is split into a thin base (`synthia-tool-exec-base`, shared `CommandResult` / `ExecError` types) and the bash-specific implementation (`synthia-tool-bash`). Migration: depend on `synthia-tool-bash` directly; cross-crate consumers should use `synthia-tool-exec-base::CommandResult` for type sharing.
- **RecoveryAction::Recovered now carries a level**: the `Recovered(String)` tuple is replaced with `Recovered { message: String, level: RecoveryLevel }` (3/4/5) so callers can disambiguate L3 Fallback / L4 Auto-Compact / L5 Reset. The `level` field is added in a non-additive way; all `Recovered` match arms need a destructuring update.

### Security

- **Fail-closed default**: Closed CVE-level fail-open bug where unknown tools were silently allowed. `MergedPolicy::default()` is now consistent with `PermissionPolicy::default()` semantics (`RequireConfirm` / `Ask`).
- **Honest security naming**: The `Sandbox` struct was renamed to `CommandBlacklist` and its module-level docs now explicitly list 5 known bypass techniques (Unicode obfuscation, encoding indirection, shell metacharacter games, custom interpreters, language runtimes). The blacklist remains a *defensive* layer; it is not a containment boundary.
- **Bash tool UTF-8 panic fix**: `cap_to_char_boundary` for bash tool output now uses UTF-8 safe boundary detection, eliminating a panic when a tool result's truncation point fell inside a multi-byte character.

### Fixed

- **A2A SSE keepalive**: `SynthiaExecutor` now emits a `Working` `StatusUpdate` heartbeat every 15 s (`HEARTBEAT_INTERVAL` in `synthia-server/src/a2a/executor.rs`) while waiting on the agent event stream. Idle SSE no longer stays byte-silent during long LLM thinking phases or multi-minute tool runs, so intermediaries (nginx, enterprise proxies, browser idle timers) don't silently drop the connection. Mapping for `SystemEvent::ToolProgress { tool_name: "heartbeat", .. }` lives in `synthia-server/src/a2a/mapping.rs` and carries a `kind = "heartbeat"` metadata marker for the frontend to no-op. The dead `synthia-server/src/sse.rs` module (defined `HEARTBEAT_INTERVAL` but never wired) is removed.

### Changed

- **Frontend palette softened**: Neon Terminal color tokens (`synthia-web/src/styles/tokens.css`) and ChatPage segment colors are swapped for low-saturation equivalents (muted teal `#5fb89a`, dusty cyan `#5fb3c4`, desaturated amber `#c9b265`, dusty rose `#c66a78`, charcoal `#14141f`) to reduce eye strain on long sessions. The visual identity (terminal aesthetic, monospace, glow) is preserved.
- **Frontend tool-block UI timeout raised**: `TOOL_TIMEOUT_MS` in `ChatPage.tsx` is now 180 000 ms (3 min) so the "tool timed out" placeholder only fires on genuinely stalled connections, not during legitimate long quiet phases (with the SSE heartbeat above, those no longer disconnect).

### Added

- **V4A `apply_patch` builtin tool** (portable `codex apply_patch` V4A grammar): `apply_patch` lets the LLM apply a structured multi-file patch (V4A grammar: `*** Begin Patch` … `*** End Patch`) in a single tool call. Multi-file patches are applied sequentially; each hunk can be `Add` / `Delete` / `Update` (with `Context` / `Insertion` / `Deletion` lines). 22 codex scenarios are ported as integration tests (`tests/codex_scenarios/`) covering the V4A spec (interleaved context/deletion, pure addition, overwrite, move operations, etc.). Production policy: `requires_permission() -> true` (reuses the `write` policy via Guardian).
- **TurnId (Uuid) MVP turn label**: introduces `synthia_agent::turn::TurnId(Uuid)` as the canonical turn identifier. `LoopContext` gains `current_turn_id: Option<TurnId>` and `assign_new_turn_id()`; the stream builder uses `ctx.current_turn_id.map(|t| t.0.to_string()).unwrap_or_else(|| format!("turn-{}", ctx.iteration))` to format the turn label. 5 unit tests in `loop_context.rs` + 3 integration tests in `tests/turn_id_test.rs` cover construction, default, assignment, and formatting. The `format_turn_id` helper that previously lived in `synthia_agent::turn_id` is deleted; `format_turn_id` calls are replaced with `ctx.current_turn_id` reads.
- **AgentsMdSection with hierarchical discovery**: new `synthia_context::agents_md::AgentsMdSection` walks `workspace_dir.ancestors().rev()` from filesystem root to workspace directory (farthest first, closest last so most-specific override wins) using `std::fs::canonicalize` + `HashSet<PathBuf>` for symlink-cycle protection. Section caching is `SessionCached` (read once per session, not per LLM call). `AgentConfig` gains `agents_md_enabled` (default `true`) and `agents_md_filenames` (default `["AGENTS.md"]`) with `#[serde(default = "default_agents_md_*")]` for backward compatibility. `IdentitySection::WORKSPACE_FILES` is reduced from `["IDENTITY.md", "USER.md", "MEMORY.md", "AGENTS.md"]` to drop AGENTS.md (now handled by the new section). E2E test `test_agents_md_hierarchical_discovery_through_prompt_builder` verifies ancestor walk + override semantics.
- **L1–L5 recovery cascade** (wired into the agent loop): the previously-implemented recovery cascade is now invoked from the two real error entry points in `stream_builder/builder.rs` (LLM sampling errors and tool execution errors). Five layers: L1 Truncate tool results → L2 Retry (idempotent only) → L3 Fallback → L4 Auto-Compact (driven by `AgentRunConfig::compaction_provider`) → L5 Reset (clears `BuilderSteps` reset + failure_tracker state). `AgentEvent::RecoveryApplied { level, message }` is emitted whenever a recovery layer fires; subscribers can reconstruct recovery timing from the event stream. 3 E2E tests cover tool error cascade, L5 reset, and L1 truncation paths.
- **`AgentEvent::LlmStreamDelta` for progressive text delivery**: emits one `LlmStreamDelta` per provider stream chunk, so downstream consumers (SSE / WebSocket / TUI) can render assistant text as it streams. `MaxIterationsReached` is now set as the end reason when the iteration cap is hit, replacing the previous "silent hang" termination.
- **`Message::tool_result_cleared_at` field** (idempotent pruning marker): `Message` gains `tool_result_cleared_at: Option<Instant>` with `#[serde(default)]` so existing serialized messages deserialize cleanly. `Message::prune()` no longer mutates the tool result content in place; it stamps the marker. The compaction renderer honors the marker and treats the cleared tool result as already-truncated, preventing double-truncation.
- **`scripts/check_synced_spec_format.sh`** (CI gate for OpenSpec drift): 20-line bash script that greps `openspec/specs/*/spec.md` for `^## (ADDED|MODIFIED) Requirements$` (delta format leaked into the cumulative path). Exits 0 on clean / 1 on drift with file path printed. Self-validating (synthetic drift file → FAIL; clean state → PASS). Joins the existing `scripts/check_*.sh` family (next to `check_reexports.sh`).

### Changed

- **LoopDetectorSet unified into synthia-guardian**: All loop detection logic is now centralized in `synthia_guardian::LoopDetectorSet` with five independent detectors (`DoomLoop`, `GenericRepeat`, `PingPong`, `PollNoProgress`, `GlobalCircuit`). The local `crates/synthia-agent/src/stream_builder/loop_detection.rs` (467 lines) has been deleted; `AgentDependencies` and `StreamBuilder` now import from `synthia_guardian`. The check API returns `(LoopStatus, Option<LoopAction>)` to disambiguate caller responses; `LoopAction::RequirePermission` is the doom-loop signal (mirrors opencode's `doom_loop` permission category) and is currently treated as a blocking signal until `synthia-permission` is wired into the stream loop.
- **TokenUsage unified across 4 crates**: `synthia-agent`, `synthia-guardian`, `synthia-provider`, `synthia-telemetry` now all consume the same `TokenUsage` type via 1-line re-export shims. Eliminates 4 parallel definitions of `(prompt, completion, total)` that drifted independently.
- **synthia-session 3-layer re-export guard** (`synthia-session-reexport-policy` spec): the `pub use session::{Session, SessionError, SessionManager}` re-export was deleted to prevent `SessionManager` / `SessionError` from shadowing their `types::` counterparts at the crate root. Defense in depth: (1) `compile_fail` doc tests in `src/lib.rs`, (2) integration test `tests/reexport_policy.rs`, (3) CI script `scripts/check_reexports.sh`. Re-wiring the shadowed path is now compile-time-impossible without a deliberate `pub use` addition.
- **Single-pass compaction + summary anchoring**: `crates/synthia-context` compaction now uses a single-pass scan instead of O(n²) repeated walks. `previous_summary` is truncated to 4000 characters (head 60% + tail 40% + marker) using UTF-8 safe boundary checks, eliminating the multi-byte panic risk that the bash tool fix parallels. `crates/synthia-agent/src/agent_tools.rs` (1300+ lines) is split into a small `agent_tools` shim + 4 focused sub-modules (`compact.rs`, `truncate.rs`, `prune.rs`, `tools_render.rs`). The shim uses `pub use sub_module::*` so callers can keep importing from `agent_tools` unchanged.
- **`agent.run.compaction_provider` config knob**: `AgentRunConfig` gains `compaction_provider: Option<Arc<dyn CompactionProvider>>` so L4 Auto-Compact can call into a user-supplied compaction backend (defaults to the built-in synthia-context provider).
- **`tool_call_id` propagation**: `LoopContext` now carries `tool_call_id: String` so tool results are routed back to the correct LLM-issued tool call. Previously the field was tracked in ad-hoc ways and tool results sometimes reached the LLM without the matching `tool_call_id`, causing the model to ignore the result.
- **End-of-session reflection gated by tool activity**: `AgentEvent::SessionReflection` (a "what did we accomplish" summary event) no longer fires on text-only turns; it requires at least one tool execution in the session. This eliminates noise on chat-only sessions.
- **Real provider streaming** (OpenAI + Anthropic): `complete_with_stream` is the default path. `IsDone` variant is added to `StreamChunk` so consumers can detect end-of-stream without per-message string matching. `SamplingResult` is moved from `synthia-agent` to `synthia-provider` (the type is provider-shaped; the old home was a layering leak). `truncate` integration on streaming: large stream chunks are truncated at the provider boundary to keep per-chunk payload size bounded. The deprecated `stream()` and `collect_stream_response` are deleted after a 1-release deprecation window.

### Fixed

- **Cache control hash independence**: `CacheControlMark` is now hashed independently of `system_content`, preventing false cache hits when system content is unchanged but cache directives differ. Scopes are namespaced by `user_id` + `session_id` to prevent cross-session leakage.
- **No silent record drops in loop detector**: the agent's `stream_builder::loop_detection::GenericRepeatDetector` now uses `HashMap<(u64, u64), u32>` for O(1) lookups and updates, eliminating per-call `String` allocations and the O(N) scan from the previous `VecDeque` implementation.
- **Removed dead code**: Deleted the unused `crates/synthia-agent/src/agent/` module tree (never declared as a module, never compiled) along with `crates/synthia-cli/src/agent.rs` and `crates/synthia-server/src/agent.rs` (also never declared). Deleted the dead `crates/synthia-tool/src/exec/` module that referenced non-existent `PermissionLevel`. Removed the legacy `PermissionPolicy` struct from `synthia-permission`, unifying on `MergedPolicy`. Deleted 11 additional dead files in `synthia-cli/src/` (`color.rs`, `config.rs`, `handler.rs`, `output.rs`, `runner.rs`, `modes/`, `input/`, and others) — ~2572 lines of code that was never compiled because `synthia-cli/src/lib.rs` only declared 5 of the 11 `.rs` files present.
- **`synthia-agent` doctest import paths corrected**: several `AgentError` doctests used stale `use crate::...` paths after the module split. The corrected paths resolve at `cargo test --doc` time.
- **`synthia-session` test compile fixes**: `tests/session_persistence.rs` now uses `synthia_session::types::Session` and `synthia_session::manager::SessionManager` (qualified paths) to avoid the `SessionManager` shadowing that previously blocked the test from compiling. The phantom `SessionConfig::new(id)` calls are replaced with `SessionConfig::default()`.
- **`turn_id` string construction centralized**: 4 turn-ID representations (`LoopContext.iteration: usize`, `AgentContext.turn_id: String`, `PrefixStabilityEvent.turn_id: u64`, `ApprovalRequest::NetworkAccess.turn_id: String`) are converged via a centralized `format_turn_id` helper. The 3 lower-effort representations stay as-is; the `ApprovalRequest::NetworkAccess.turn_id` field is removed as an orphan (0 production callers, 0 Guardian reads). Net: -1 `format_turn_id` site + 1 central site, with future code able to use `AgentContext::current_turn_id` directly.
- **OpenSpec spec format drift fixed** (`fix-12-synced-spec-headers` change): 12 pre-existing `openspec/specs/*/spec.md` files were using `## ADDED Requirements` (delta format) instead of `## Requirements` (cumulative format), causing `openspec spec validate --strict` to fail. 5 Pattern A specs (have `## Purpose`, need only header rename) and 7 Pattern B specs (need `## Purpose` prepended + header rename; Purpose text sourced from archived `proposal.md` "Why" section) are repaired. New CI gate `scripts/check_synced_spec_format.sh` prevents future drift.

### Removed

- **`sub_traits` module removed from synthia-tool**: The placeholder sub-trait design (`ToolDefinition` / `ToolExecution` / `ToolLifecycle`) was never implemented. `ToolCategory` enum and `ToolMetadataSnapshot` struct are preserved as `pub` symbols in `crates/synthia-tool/src/descriptor.rs` next to `ToolDescriptor`.
- **`fragment` module relocated from synthia-tool to synthia-context**: `ContextFragment` trait, `FragmentRegistry`, and 6 built-in fragments (`SystemPromptFragment` / `TokenBudgetFragment` / `PermissionsFragment` / `EnvironmentFragment` / `RolloutBudgetFragment` / `CustomFragment`) now live in `crates/synthia-context/src/fragment/`. System prompt injection logic now sits next to `ContextAssembler`. Update `use synthia_tool::fragment::*` → `use synthia_context::fragment::*`.
- **Deprecated `provider::stream()` and `provider::collect_stream_response`**: removed after a 1-release deprecation window. Use `complete_with_stream` + `StreamChunk` (with `IsDone` variant) instead.
- **`stream_builder/loop_detection.rs`** (467 lines, local to `synthia-agent`): now in `synthia-guardian` (`LoopDetectorSet`).
- **`synthia-exec` monolith**: replaced by `synthia-tool-bash` + `synthia-tool-exec-base` (see Breaking Changes).
- **Old `PermissionPolicy` struct + `RuleSet`**: removed; `MergedPolicy` is the sole permission evaluator.
- **Orphaned `streaming-2part-truncate` files**: scaffolding from a refactor that was completed differently. ~120 lines.
- **`synthia-cli` dead files (11 files, ~2572 lines)**: see "Removed dead code" under Fixed.
- **`synthia-agent/executor/` and `synthia-agent/builder/` modules**: pre-refactor scaffolding replaced by the current `stream_builder/` module.
- **`synthia-agent` simplification pass (13 waves, ~5700 LOC removed)**: removed unused subsystems from `crates/synthia-agent/src/` — `registry/` (936), `checkpoint/` (566), `hook_view.rs` (140), `error_recovery/` (~770, including `run_recovery_cascade` replaced by a `WarningKind`-level signal), `control/{fork_policy,mailbox,reservation}.rs` (552), `agent_permission.rs` + `steps/spawn.rs` + `iteration/loop_detect.rs` + `LoopDetectInterceptor` (~456), `AgentRunStateConfig` (no consumers) plus dead `synthia_provider::{ContentPart, ToolResult, ToolUse}` re-exports, the entire `control/` multi-agent plane (`agent_path.rs` + `registry.rs` + `core_ctrl.rs` + `format_background_task_notification` + `agent_control` poll path in `main_loop.rs`, ~617 LOC) plus the now-unused `regex` crate dependency, the dead `ExtensionRegistry::skill_registry()` accessor (zero callers), the dead `tool_args`/`token_usage` `InterceptorContext.data` map insertions in `main_loop.rs` that were vestigial after `LoopDetectInterceptor` deletion, 7 dead `AgentConfig` fields (`executor_kind`, `agents_md_enabled`, `agents_md_filenames`, `agents_md_config()`, `doom_loop_threshold`, `compaction_provider`, `token_budget`, `checkpoint_dir`) + the entire `ExecutorKindConfig` enum (~268 LOC removed), and the entire vestigial `error/` module (`AgentError` enum with 14 variants, 9 constructors, 6 `From` impls, 16 unit tests = 478 LOC) that was declared but never re-exported and had zero external callers — production code uses `synthia_core::Error` throughout. The `test_no_underscore_prefixed_fields_in_run_config` audit table was updated to reflect the actual current `AgentRunConfig` field list (added `extension_registry` + `interceptor_chain`; removed stale references to `context_assembler`, `agent_control`, `compaction_provider`). Net: `synthia-agent/src/` shrunk from 15,204 → 9,508 LOC across 60 files (~37% reduction). External `synthia-cli` + `synthia-server` callsites dropped for `agent_control`, `approval_service`, `guardian_coordinator`, `fork_policy`, `token_budget`, `checkpoint_dir`, `compaction_provider`. The LLM sampling path still classifies transient errors as `LlmSampleOutcome::Continue` and validation errors as `LlmSampleOutcome::Terminate`; doom-loop detection, fork-policy, and the background sub-agent registry are removed.

### Deferred

- **Phase 3 trait abstractions**: `LoopDetector`, `PermissionPolicy` sub-traits, `OsSandbox`, and `Message::cache_control` field abstractions are deferred 6 months (re-evaluate 2026-12-10). The 6-expert adversarial review concluded that trait abstraction is premature before critical bug fixes and code deduplication are stable. The freeze for `turn-id-mvp` (separate meta-change) was originally set to 2026-09-13 and then user-overridden; the 3-month observation window is kept as a *default* but is no longer binding when the user explicitly requests implementation.

## [0.2.0] - 2026-05-21

### Breaking Changes

- **Plan API Removed**: The `Plan`, `PlanStep`, and `Planner` types have been removed entirely. The planning subsystem has been replaced with a Task-centric execution model. Users who previously used the Plan API should migrate to using the `Task` tool with structured `TaskContext` for task decomposition and execution. See `docs/migration-guides/plan-to-task.md` for a detailed migration guide.

### Added

- **Task-Centric Execution Model**
  - Enhanced `Task` tool with structured `TaskContext` (description, file_references, code_snippets, constraints)
  - YAML-based context serialization for rich task descriptions
  - Configurable timeout with 30s default via `tokio::time::timeout`
  - Priority enum (High, Medium, Low) with Medium default for task scheduling
  - Structured `TaskResult` with output, status, exit_code, and artifacts fields
  - File reference resolution for automatic file content inclusion in sub-agent prompts

### Changed

- Migrated from Plan-based execution to Task-centric execution model
- Agent now uses Task tool as the primary dispatch mechanism instead of Plan generation
- Simplified multi-agent orchestration to use Task-based dependency management

## [0.1.0] - 2026-05-09

### Added

- **Core Agent System**
  - ReAct loop implementation with full event streaming
  - Agent configuration with validation and builder pattern
  - Session lifecycle management
  - Context assembly and management
  - Checkpoint system for state persistence

- **Memory Systems**
  - Hot memory for frequently accessed information
  - Cold memory for long-term storage
  - Episodic memory for conversation history
  - Context memory for current session state
  - Memory persistence with automatic compaction
  - Token budget management with multi-tier thresholds

- **Hook System**
  - Extensible hook system for agent lifecycle events
  - Pre/post hooks for various agent stages
  - Hook registry with failure handling

- **Tool System**
  - Tool registry and execution framework
  - Permission-based tool execution
  - MCP (Model Context Protocol) integration
  - MCP server discovery and tool adapter creation

- **Provider System**
  - LLM provider abstraction layer
  - OpenAI provider implementation
  - Anthropic provider implementation
  - Streaming support
  - Prefix caching support

- **Observability**
  - OpenTelemetry integration
  - Prometheus metrics export
  - Structured logging
  - Audit logging with buffer flushing

- **Testing Infrastructure**
  - Comprehensive unit tests
  - Integration tests
  - End-to-end tests
  - Test support utilities with FakeProvider

### Fixed

- Fixed AgentConfig missing fields in examples
- Fixed test_support.rs FakeProvider implementation
- Fixed type mismatches in e2e tests
- Fixed deprecated function warnings

### Changed

- Migrated from `run_loop` to `Agent::run()` for improved API
- Updated test support to use streaming chunks
- Improved error handling in memory persistence

### Security

- Guardian circuit breaker for security loop detection
- Permission-based tool execution sandbox

### Performance

- Memory compaction for context token budget optimization
- Background memory persistence
- Efficient message handling with hot memory caching
