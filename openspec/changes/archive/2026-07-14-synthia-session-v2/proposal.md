# Proposal: synthia-session-v2 (Change 3 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval (no auto-commit, no writer launched)
**Parent design**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.3
**Depends on**:
- Change 1 (`synthia-tool-refactor`) — needs `AgentTool` + `ExtensionTool` + `ToolRouter` types
- Change 2 (`synthia-event-first`) — needs 27 `ExtensionEvent` + `ExtensionRegistry` + `ExtensionCtx` types

**Absorbs**:
- `9-abstractions-toolification` spec's full implementation (R8 closure): `compact_context`, `subagent`, `guardian`, `monitor`, MCP, `external_hook_tool`, `QuerySkillUsageTool`, `plugin_cli` entries
- `add-dynamic-tool-provider-system` R1.1-R1.3 (3 remaining ToolProvider implementations)
- `adopt-explore-agent-recommendations` R1.1-R4.3 archive + spec hygiene sweep + architecture-audit verify

## Why

Synthia has **two parallel session backends** — neither adequate:

1. **`synthia-session/src/types/session.rs`**: flat `Session` struct (13 fields), no `Message`/`Part` abstraction. No way to:
   - Stream a tool call as `pending → running → completed`
   - Mark a part as `compacted` in a type-safe way
   - Embed subagent `SubtaskPart` inside a parent message
   - Do **interruptible revert** (the input was never persisted)
2. **`synthia-session/src/store/`** (5059 LOC across 21 files): synchronous file writes per append (no background task, no `Flush`, no `Shutdown`), checkpoint + event_log + metadata.json **3 ledgers**. No `branch()`/`fork()`/non-destructive rollback. No `W3cTraceContext` propagation.

The hardcoded `ToolOutput` (`crates/synthia-tool/src/types.rs:50-146`) has `is_error: Option<bool>` only — no state machine; the entire tool-call story is replay-as-string.

Three production agents have independently converged on **part-based + JSONL + wire protocol**:
- **opencode** (`packages/opencode/src/session/message-v2.ts:206-413`, `core/v1/session.ts:253-385`): `WithParts { info, parts: Part[] }` 11-variant discriminated union; `ToolPart.state: ToolState` 4-machine with type-safe `time.compacted: Option<NonNegativeInt>`; `filterCompacted()` reorders at part granularity.
- **codex** (`codex-rs/protocol/src/protocol.rs:155-1239+`): `Submission { id, op, trace }` + `EventMsg` wire envelope + `W3cTraceContext` carrier + `AskForApproval` enum + `GranularApprovalConfig` + `RolloutRecorder` background-task JSONL writer with `mpsc` + oneshot ack.
- **pi-mono** (`packages/coding-agent/src/core/session-manager.ts:44-49, 669-1163`): append-only JSONL tree with `id`/`parentId` 8-char ULID; `branch(fromId)` non-destructive leaf-pointer move; `buildSessionContext` walks leaf→root replacing compaction entries with summary.

## What Changes

**C3.1** New crate `synthia-protocol/`:
- `Submission { id, op, trace: Option<W3cTraceContext> }`
- `Op` enum (Interrupt, Compact, UserInput, ThreadRollback, ApprovalResponse, RefreshTools, Resubmit, UpdateModel, UpdateThinkingLevel, SwitchSession, ForkSession)
- `EventMsg` enum (SessionCreated, TurnStarted, TurnComplete, ToolCall, ToolCallOutput, ApprovalRequest, ApprovalResponded, CompactStarted, CompactCompleted, ThreadRolledBack, TokenCount, ModelRerouted, ToolSearched, Error, Warning)
- `W3cTraceContext { traceparent, tracestate }` with `from_current_otel()` + `attach_to_current_otel()` 
- `AskForApproval` enum (per codex `protocol/src/protocol.rs:807-855`)
- `ExecApprovalRequirement { Skip | Forbidden | NeedsApproval }`

**C3.2** New crate `synthia-session-v2/`:
- `Message { info: MessageInfo, parts: Vec<Part> }` (mirroring opencode `WithParts`)
- `Part` enum (Text/Reasoning/Tool/File/StepStart/StepFinish/Patch/Snapshot/Compaction/Subtask/Agent/Custom) — 11 variants
- `ToolPart { call_id, tool_name, args, state: ToolState, metadata, attachments, time }`
- `ToolState` 4-state machine (Pending/Running/Completed/Error) with `time.compacted: Option<u64>` type-safe
- `SessionEntry` enum (Header/Message/Compaction/BranchSummary/ModelChange/ThinkingLevelChange/Label/SessionInfo/CustomMessageEntry/CustomEntry/Fork/Rollback/ErrorEvent) — 14 variants
- `SessionTree { entries, children, root, leaf, paths_from_root }`
- `SessionManager { tree, path, write_tx: mpsc::Sender, flush_handle }`
- `branch(target)` + `branch_with_summary(target, summary)` + `fork(at_id)`
- `build_context()` walks root→leaf replacing compaction entries with summary
- `session_writer_task` (background writer with `tokio::spawn`, mpsc + 50ms batch flush, oneshot ack)
- Idempotent `migrate_v1_to_v2` / `migrate_v2_to_v3` chains

**C3.3** Collapse `synthia-session/src/store/` (21 files, 5059 LOC) → thin migration shim re-exporting synthia-session-v2 types

**C3.4** Split `AgentRunConfig` (17 fields) into:
- `RunContext { cancel_token, user_id, session_id, input }`
- `ToolContext { orchestrator, router, approval }`
- `SessionContext { store, manager }`

**C3.5** Server/CLI wire protocol:
- `synthia-server/` axum routes:
  - `POST /submission` (accepts `Submission`)
  - `GET /ws` (streams `EventMsg` over WebSocket)
- `synthia-cli/` consumes the same protocol (no behavior change for end user)

**C3.6** `ProviderRegistry` v2:
- Hot-swap with `source_id` isolation (per codex protocol + pi-mono api-registry.ts)
- `register(name, provider, source_id)` + `unregister(name, source_id)` + atomic `replace_source(source_id, new_set)`
- 3 extension events: `ProviderRegister` / `ProviderUnregister` / `ExtensionHotSwap`

**C3.7** Migrate 9-abstractions spec to full closure:
- `compact_context_tool` already moved in Change 1 R6 (verify completeness)
- `subagent` / `guardian` / `monitor` / MCP — already ExtensionTool in Change 2 R7 (verify binding)
- `ExternalHookTool` full implementation (`bind_extension` actually subscribes to events)
- `QuerySkillUsageTool` actual implementation
- Plugin CLI entries (`kind: Tool` in manifest → `ExtensionTool` registration)

**C3.8** Spec hygiene sweep:
- 71 specs in `openspec/specs/*/spec.md` carry `TBD Purpose` boilerplate
- 5 highest-impact specs get proper `## Purpose` (`architecture-audit`, `agent-bus`, `context-compaction`, `agent-react-loop`, `convergent-prompt-assembly`)
- Verify `architecture-audit` spec's 3 requirements (mechanical completion of `### Requirement: ...` `#### Scenario: VERIFIED`)

**C3.9** Archive `add-dynamic-tool-provider-system` and `adopt-explore-agent-recommendations` changes (both fully consumed by Change 1 + this)

## Capabilities

### New Capabilities

- `Message { info, parts[] }` + 11-variant `Part` enum
- `ToolPart` + `ToolState` 4-state machine with type-safe `time.compacted`
- `SessionEntry` 14-variant tagged union
- `SessionTree` + `branch()` / `branch_with_summary()` / `fork()` non-destructive operations
- `SessionManager` + background JSONL writer (mpsc + 50ms batch + oneshot ack)
- Idempotent migration chains (v1→v2→v3)
- `Submission` / `Op` / `EventMsg` / `W3cTraceContext` wire protocol
- `W3cTraceContext` propagation through `tokio::task_local!` + OpenTelemetry
- `AskForApproval` enum with `Granular` config
- `ProviderRegistry` v2 with `source_id` hot-swap
- 9-abstractions-toolification full closure (ExternalHookTool, QuerySkillUsageTool, Plugin CLI)
- 5 spec hygiene + 1 verify (architecture-audit scenario VERIFIED)

### Modified Capabilities

- `AgentRunConfig` splits into 3 sub-contexts
- `synthia-session/store/` shrinks from 5059 LOC to thin re-export shim
- `synthia-server` exposes Submission/EventMsg over HTTP+WS
- `synthia-cli` consumes same protocol

## Risks

| Risk | Mitigation |
|------|-----------|
| JSONL build_context scan on >100MB files | root→leaf paths_from_root index + compaction splits |
| Migration shim double-runs old + new | Migration gated by `version: u32` field, idempotent |
| W3cTraceContext field set but OTel not attached | Force assertion `OpenTelemetryContext::current() != empty` before publish |
| Fork produces N× disk usage | Default `compaction_on_fork: true` in lifecycle config |
| Provider hot-swap mid-request data inconsistency | `replace_source` bumps version; in-flight requests held by Rcu |
| Server/CLI protocol change breaks existing users | Old `run_stream` entrypoint retained as compatibility shim |
| 8-Round change touches every CLI/server endpoint | 8 Rounds × 1 commit each, additive non-breaking throughout |

## Out of Scope (Deferred)

- SQLite-derived metadata mirror (separate Change)
- Codex-style `code-mode` JS/WASM runtime (explicitly rejected)
- Bazel build system
- Multi-agent namespaces (`codex-rs/tools/handlers/multi_agents/`)
- jiti-style compile-time extension loading

## Reference

- Parent design: [design.md](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- Codex pattern: `codex-rs/protocol/src/protocol.rs:155-1239+`, `codex-rs/rollout/src/recorder.rs:74-153`, `codex-rs/execpolicy/src/policy.rs:28-251`
- opencode pattern: `packages/opencode/src/session/message-v2.ts:206-413`, `packages/core/src/v1/session.ts:253-385`
- pi-mono pattern: `packages/coding-agent/src/core/session-manager.ts:669-1163`
- 9-abstractions spec: [`openspec/specs/9-abstractions-toolification/spec.md`](../../specs/9-abstractions-toolification/spec.md)
- Absorbed: `add-dynamic-tool-provider-system`, `adopt-explore-agent-recommendations`
