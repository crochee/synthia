# Plan: synthia-session-v2 (Change 3 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval (no auto-commit, no writer launched)
**Parent**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.3
**Proposal**: [proposal.md](../proposal.md)
**Design**: [design.md](../design.md)

## Principles (must hold through every Round)

1. **No auto-commit** — each Round ends with "等用户明确指示"
2. **Cargo +nightly fmt --all + clippy --all-targets --all-features --tests --all -- -D warnings** after every Round
3. **`main_loop.rs` ≤ 400 LOC** (must stay green)
4. **No `unsafe`**, no `as any` / `#[allow(async_fn_in_trait)]`
5. **Type safety** — every public surface `Send + Sync + 'static`
6. **Backward compat** — old `synthia-session/store/` retained as shim until 0.3.0
7. **P1 P8 P9 P10** — KV-cache prefix consistency / no info loss / OTel observability / file-as-memory
8. **9-abstractions**: full closure (ExternalHookTool + QuerySkillUsageTool + plugin CLI as Tool) verified green by Round 8

## 8 Implementation Rounds

Each Round produces 1 commit, additive non-breaking, verified, then **WAITS** for user approval before next Round.

| # | Round | LOC Δ | Files Δ | Depends on |
|---|-------|------:|--------:|------------|
| 1 | `synthia-protocol` skeleton | +1500 | +1 crate | Changes 1+2 merged |
| 2 | `synthia-session-v2` skeleton (Message/Part/ToolState) | +1500 | +1 crate | R1 |
| 3 | Collapse `synthia-session/store/` → shim | -3500 | -20 | R1+R2 |
| 4 | Background writer (`session_writer_task`) | -50 net | rewrite 1 | R2+R3 |
| 5 | Split `AgentRunConfig` (17 → 3 sub-contexts) | -400 | 1 modified | R1+R2+R3 |
| 6 | Server/CLI wire protocol (axum POST /submission, GET /ws) | +600 | +3 | R1+R2 |
| 7 | `ProviderRegistry` v2 (source_id isolation hot-swap) | +200 | +1 + 3 events | R1+R2 |
| 8 | 9-abstractions-toolification closure | +800 | 6 modified | all above |

**Total LOC impact**: net **-350** (+3100 added, -3450 deleted); net **-22 files**

## Round 1 — `synthia-protocol` skeleton

**Goal**: Adopt codex wire protocol (`Submission`/`Op`/`EventMsg`/`W3cTraceContext`/`AskForApproval`/`GranularApprovalConfig`).

**Files**:
- `crates/synthia-protocol/Cargo.toml` — new crate
- `crates/synthia-protocol/src/lib.rs`
- `crates/synthia-protocol/src/submission.rs`
- `crates/synthia-protocol/src/event.rs`
- `crates/synthia-protocol/src/trace.rs` — `W3cTraceContext::from_current_otel()` + `attach_to_current_otel()`
- `crates/synthia-protocol/src/approval.rs` — `AskForApproval` enum + `GranularApprovalConfig`
- `crates/synthia-protocol/src/error.rs`
- `crates/synthia-protocol/src/version.rs` — `PROTOCOL_VERSION = 2`
- `crates/synthia-protocol/tests/wire_roundtrip.rs` — JSON serialization tests
- `crates/synthia-protocol/tests/trace_context.rs` — non-empty current context assertion
- workspace `Cargo.toml` — add workspace member

**Must**:
- Use `serde_json` only (no `bincode` fallback — Simplicity First)
- `W3cTraceContext::invalid()` constant for placeholder
- `SubmissionId`, `MessageId`, `SessionId`, `CallId`, `ApprovalId`, `InputItem` typed wrappers via newtype
- Every wire enum has `#[non_exhaustive]`
- 100% doc comments on public types

**Must not**:
- Touch any other crate
- Auto-commit
- Skip OTel context assertion in debug builds

**Verification**:
```bash
cargo +nightly fmt --all
cargo check -p synthia-protocol --all-features
cargo clippy -p synthia-protocol --all-targets --all-features --tests -- -D warnings
cargo test -p synthia-protocol
```

**End**: WAIT for user approval → R2.

## Round 2 — `synthia-session-v2` skeleton

**Goal**: Adopt opencode V2 message model + pi-mono append-only tree, with type-safe Compaction marker.

**Files**:
- `crates/synthia-session-v2/Cargo.toml`
- `crates/synthia-session-v2/src/lib.rs`
- `crates/synthia-session-v2/src/message.rs` — `Message { info, parts[] }`
- `crates/synthia-session-v2/src/part.rs` — 11-variant `Part` enum
- `crates/synthia-session-v2/src/tool_part.rs` — `ToolPart` + `ToolState` 4-state + `ToolTime.compacted: Option<DateTime<Utc>>`
- `crates/synthia-session-v2/src/entry.rs` — 14-variant `SessionEntry`
- `crates/synthia-session-v2/src/tree.rs` — `SessionTree`
- `crates/synthia-session-v2/src/session_versions.rs` — `CURRENT_SESSION_VERSION = 2`
- `crates/synthia-session-v2/tests/serializer_roundtrip.rs`
- workspace `Cargo.toml` — add workspace member

**Must**:
- `Part::Compaction(CompactionPart)` + `ToolTime.compacted: Option<...>` type-safe (NOT plain field)
- `SessionEntry::Header` first → `Message`s → `Compaction` → `Fork` → `Rollback`
- BTreeMap<MessageId, SessionEntry> ordering
- All types `Send + Sync + 'static`
- `From<SubtaskPart>` / `From<AgentPart>` convenience impls
- Every `Part` variant has doc + example

**Must not**:
- Add background writer yet (R4)
- Add wire protocol integration yet (R6)
- Touch `synthia-session/store/`

**Verification**:
```bash
cargo +nightly fmt --all
cargo check -p synthia-session-v2 --all-features
cargo clippy -p synthia-session-v2 --all-targets --all-features --tests -- -D warnings
cargo test -p synthia-session-v2
```

**End**: WAIT for user approval → R3.

## Round 3 — Collapse `synthia-session/store/` → shim

**Goal**: Replace 21 files / 5059 LOC with thin migration shim.

**Files**:
- DELETE: `crates/synthia-session/src/store/*.rs` (20 files)
- MODIFY: `crates/synthia-session/src/lib.rs` — `pub use synthia_session_v2::*;` + legacy aliases
- MODIFY: `crates/synthia-session/src/store/mod.rs` — keeps old module path re-exporting new types
- NEW: `crates/synthia-session/src/migration.rs` — v1→v2 chain
- NEW: `crates/synthia-session/src/deprecation.rs` — `#[deprecated]` aliases
- workspace `Cargo.toml` — `synthia-session` depends on `synthia-session-v2`

**Must**:
- Keep **all** public types reachable through `synthia-session::*` (no test code breaks)
- Add `#[deprecated(since = "0.2.0", note = "use synthia_session_v2")]` on moved types
- Migration chain **idempotent** (v1→v2 reads `version: u32`, no-op on v2)
- Update only 1 file per commit in user-facing re-export

**Must not**:
- Change any re-export semantics
- Break `synthia-cli` or `synthia-server` callers
- Auto-commit

**Verification**:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test --workspace
```

**End**: WAIT for user approval → R4.

## Round 4 — Background writer (`session_writer_task`)

**Goal**: Adopt codex `RolloutRecorder` pattern: mpsc + 50ms batch + oneshot ack + `Shutdown` join handle.

**Files**:
- NEW: `crates/synthia-session-v2/src/writer_task.rs` — `session_writer_task`
- NEW: `crates/synthia-session-v2/src/manager.rs` — `SessionManager { tree, write_tx, flush_handle }`
- NEW: `crates/synthia-session-v2/src/branch.rs` — `branch`/`branch_with_summary`/`fork`
- MODIFY: `crates/synthia-session-v2/src/tree.rs` — `SessionTree.paths_from_root`
- NEW: `crates/synthia-session-v2/tests/stress.rs` — append 1000 ops, assert no blocking

**Must**:
- `mpsc::Sender<TreeCmd>` with bounded `10_000` capacity + backpressure via `try_send` → caller awaits `oneshot`
- `Flush` ack via oneshot — non-blocking from caller side
- `Shutdown` drains remaining + flushes + joins + returns
- fsync every 50ms batch
- `branch(target)` updates `leaf` pointer
- `branch_with_summary(target, summary)` appends `SessionEntry::BranchSummary`
- `fork(at)` clones subtree to new `SessionId`
- `paths_from_root` rebuilt on every leaf change (cached, invalidated on append)

**Must not**:
- Skip oneshot ack (every cmd must have ack unless explicitly fire-and-forget)
- Block caller on disk (only writer task may block on fsync)
- Auto-commit

**Verification**:
```bash
cargo +nightly fmt --all
cargo check -p synthia-session-v2 --all-features
cargo clippy -p synthia-session-v2 --all-targets --all-features --tests -- -D warnings
cargo test -p synthia-session-v2
```

**End**: WAIT for user approval → R5.

## Round 5 — Split `AgentRunConfig` (17 → 3 sub-contexts)

**Goal**: Adopt codex `TurnContext`/`RunContext` shape — split 17-field struct into `RunContext` + `ToolContext` + `SessionContext`.

**Files**:
- MODIFY: `crates/synthia-agent/src/agent.rs` — `AgentRunConfig` split
- NEW: `crates/synthia-agent/src/context.rs` — `RunContext`, `ToolContext`, `SessionContext`
- MODIFY: `crates/synthia-agent/src/main_loop.rs` — accept 3 sub-contexts
- MODIFY: all consumers of `AgentRunConfig` — pass 3 sub-contexts instead
- NEW: `crates/synthia-agent/tests/context_split.rs`

**Must**:
- `RunContext` owns `cancel_token`, `user_id`, `session_id`, `input`
- `ToolContext` owns `tool_orchestrator`, `tool_router`, `approval_handler`
- `SessionContext` owns `session_manager` (from R2-R4)
- `main_loop.rs` ≤ 400 LOC (already green from Change 2 R4)
- `agent.rs` ≤ 700 LOC (after split)
- All 5 historical e2e tests pass

**Must not**:
- Change any public API of `Agent` struct
- Auto-commit

**Verification**:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test -p synthia-agent
cargo test -p synthia-agent --test react_loop_test
cargo test -p synthia-agent --test e2e_llm_test
cargo test -p synthia-agent --test e2e_event_sequence_test
cargo test -p synthia-agent --test e2e_memory_correctness_test
```

**End**: WAIT for user approval → R6.

## Round 6 — Server/CLI wire protocol

**Goal**: Adopt codex `codex app-server` JSON-RPC style: `POST /submission` + `GET /ws` (WebSocket EventMsg stream).

**Files**:
- MODIFY: `crates/synthia-server/Cargo.toml` — add `axum` (already there) + `tokio-tungstenite` + `tower-http`
- NEW: `crates/synthia-server/src/routes/submission.rs` — `POST /submission` handler
- NEW: `crates/synthia-server/src/routes/ws_event.rs` — `GET /ws` WebSocket
- MODIFY: `crates/synthia-server/src/main.rs` — register routes
- MODIFY: `crates/synthia-cli/src/main.rs` — `--wire` opt-in flag (default stdin/stdout unchanged)
- NEW: `crates/synthia-server/tests/wiremock_submission.rs`
- NEW: `crates/synthia-server/tests/wiremock_ws.rs`

**Must**:
- `POST /submission` accepts `Submission` JSON, dispatches via `SessionManager`
- `GET /ws` upgrades to WebSocket, streams `EventMsg` events
- Both routes emit `extension.hook` OTel span per event (P9)
- CLI opt-in via `synthia-cli --wire` (default stays stdin)
- Legacy `/run_stream` HTTP kept as `#[deprecated]` for 1 minor

**Must not**:
- Change default CLI behavior
- Add bincode fallback
- Auto-commit

**Verification**:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test -p synthia-server
cargo test -p synthia-cli
```

**End**: WAIT for user approval → R7.

## Round 7 — `ProviderRegistry` v2

**Goal**: Adopt codex `provider` registry + pi-mono `api-registry.ts` with `source_id` isolation hot-swap.

**Files**:
- NEW: `crates/synthia-provider/src/registry/v2.rs` — `ProviderRegistry`
- MODIFY: `crates/synthia-provider/src/registry/mod.rs` — re-export both v1 + v2
- NEW: `crates/synthia-provider/src/registry/v2_events.rs` — `ProviderRegister` / `ProviderUnregister` / `ExtensionHotSwap`
- MODIFY: `crates/synthia-provider/src/extension.rs` — emit 3 events
- NEW: `crates/synthia-provider/tests/source_id_isolation.rs`

**Must**:
- `register(name, provider, source_id)` — re-register with same `source_id` REJECTS (different sources are independent)
- `unregister(name, source_id)` — silently ignores missing
- `replace_source(source_id, new_set)` — atomic `swap` (single writer lock)
- in-flight requests held by `Arc` (RCU-like)
- Default to v2 via `[experimental] provider_v2: true` in `config.toml` (1 minor cycle)
- v1 path retains deprecation shim

**Must not**:
- Auto-commit

**Verification**:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test -p synthia-provider
```

**End**: WAIT for user approval → R8.

## Round 8 — 9-abstractions-toolification full closure

**Goal**: Close out the 9-abstractions spec — every entry goes through ExtensionTool + ExtensionEvent + ExtensionContext.

**Files**:
- MODIFY: `crates/synthia-tool/src/extension_tool.rs` — full `ExternalHookTool` (already moved in Change 1 R2, verify actual subscription)
- MODIFY: `crates/synthia-tool/src/query_skill_usage_tool.rs` — full implementation
- MODIFY: `crates/synthia-tool/src/mcp_provider.rs` — verify MCP integration
- NEW: `crates/synthia-cli/src/plugin_loader.rs` — `kind: Tool` → `ExtensionTool` registration
- MODIFY: `crates/synthia-agent/src/main_loop.rs` — verify all 9 abstractions wired
- NEW: `crates/synthia-agent/tests/9_abstractions.rs` — every abstraction goes through new path

**9-abstractions checklist** (verify each):
1. `compact_context` — `CompactContextTool` (Change 1 R6)
2. `subagent` — `SubagentTool` (Change 1 R6)
3. `guardian` — `GuardianTool` (Change 2 R7)
4. `monitor` — `MonitorTool` (Change 2 R7)
5. MCP — `McpProviderTool` (already exists, verify ExtensionTool binding)
6. `external_hook_tool` — FULL subscription to ExtensionEvent
7. `query_skill_usage_tool` — actual query
8. `plugin_cli` — `kind: Tool` → `ExtensionTool` registration
9. `tool_search` — `ToolSearchTool` (already exists, verify)

**Must**:
- Each abstraction has a regression test
- `cargo test --workspace --features otel` green
- 5 historical e2e tests green
- `main_loop.rs` ≤ 400 LOC
- `session/store/` shim still works (test green)

**Must not**:
- Skip any of the 9 abstractions
- Auto-commit

**Verification**:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features --features otel
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test --workspace --features otel
cargo test -p synthia-agent --test react_loop_test --test e2e_llm_test --test e2e_event_sequence_test --test e2e_memory_correctness_test
```

**End**: WAIT for user approval → Archive.

## Spec hygiene sweep (within R8 or parallel)

**5 highest-impact specs to give proper `## Purpose`**:
1. `architecture-audit`
2. `agent-bus`
3. `context-compaction`
4. `agent-react-loop`
5. `convergent-prompt-assembly`

**1 verification** (`architecture-audit`):
- Mechanical completion of `### Requirement: ...` `#### Scenario: VERIFIED`

## Archive (after R8)

- `add-dynamic-tool-provider-system` — fully consumed
- `adopt-explore-agent-recommendations` — fully consumed
- `synthia-tool-refactor` — Change 1 of v3
- `synthia-event-first` — Change 2 of v3
- `synthia-session-v2` — Change 3 of v3 (this change)

## Open Questions (carry into execution)

1. **`Session` header `version` field** — start at `2` or `3`? — **start at `2`** (v1 was synthia's existing flat `Session`)
2. **Wire protocol serialization format** — JSON only or include `bincode` fallback? — **JSON only** (Simplicity First)
3. **CLI WebSocket client** — replace stdin/stdout or augment? — **augment** via opt-in `--wire` flag
4. **Provider v2 default migration path** — automatic or opt-in? — **opt-in** for 1 minor cycle
5. **Migration shim lifetime** — keep 1 minor or 2? — **1 minor** (0.3.0 deletes shim)
6. **Spec hygiene**: do all 71 specs or top 5? — **top 5 + 1 verification**

## Reference

- Parent design: [design.md](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- Proposal: [proposal.md](../proposal.md)
- Design: [design.md](../design.md)
- Tasks: [tasks.md](../tasks.md)
- Absorbed specs:
  - `openspec/specs/9-abstractions-toolification/spec.md`
  - `add-dynamic-tool-provider-system`
  - `adopt-explore-agent-recommendations`