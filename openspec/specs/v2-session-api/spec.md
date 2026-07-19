## Purpose

Defines the `parent_id` field on `SessionSummary` and session metadata for V2 session APIs, enabling subagent session tracking and filtering by parent.
## Requirements
### Requirement: SessionSummary responses SHALL include the parent_id field

Every `SessionSummary` returned by V2 session endpoints SHALL include `parent_id`, which is `null` for top-level sessions and the parent session id for subagent sessions.

#### Scenario: List sessions includes subagents
- **WHEN** a client sends `GET /api/v2/sessions`
- **THEN** each returned `SessionSummary` contains a `parent_id` field

#### Scenario: Get session detail includes parent_id
- **WHEN** a client sends `GET /api/v2/sessions/{id}`
- **THEN** the returned session detail contains a `parent_id` field

---

### Requirement: SessionFilter SHALL support filtering by parent_id

`SessionManager` internal filtering APIs SHALL accept an optional `parent_id` and return only sessions whose metadata matches.

#### Scenario: Filter sessions by parent
- **WHEN** `SessionManager::list_sessions_for_user` is called with `parent_id = Some(parent_id)`
- **THEN** only sessions with that `parent_id` are returned

---

### Requirement: Session metadata persistence SHALL write parent_id

When a session with `parent_id` is saved, the metadata file SHALL contain the `parent_id` field, and loading it SHALL restore the value.

#### Scenario: Round-trip parent_id
- **WHEN** a child session is saved and then loaded
- **THEN** the loaded metadata has the same `parent_id`

### Requirement: synthia-protocol wire envelope (Submission / Op / EventMsg / W3cTraceContext)

The v3 architecture SHALL ship a wire-protocol crate `synthia-protocol` that defines `Submission { id, op, trace: Option<W3cTraceContext> }`, an 11-variant `Op` enum (`Interrupt`, `Compact`, `UserInput`, `ThreadRollback`, `ApprovalResponse`, `RefreshTools`, `Resubmit`, `UpdateModel`, `UpdateThinkingLevel`, `SwitchSession`, `ForkSession`), a 13-variant `EventMsg` enum (`SessionCreated`, `TurnStarted`, `TurnComplete`, `ToolCall`, `ToolCallOutput`, `ApprovalRequest`, `ApprovalResponded`, `CompactStarted`, `CompactCompleted`, `ThreadRolledBack`, `TokenCount`, `ModelRerouted`, `ToolSearched`, `Error`, `Warning`), and a `W3cTraceContext { traceparent, tracestate }` carrier with `from_current_otel()` / `attach_to_current_otel()` round-trip.

This requirement is **VERIFIED** as of 2026-07-14: the planning skeleton (`plan.md`, `tasks.md`, `proposal.md`, `design.md`) was authored 2026-07-12 but never committed, and the substantive work landed in commit `3e5940c` (R1) with `tracestate` preservation follow-up in `5538a06`.

#### Scenario: wire round-trip serialization (VERIFIED)
- **WHEN** running `cargo test -p synthia-protocol --test wire_roundtrip`
- **THEN** every variant of `Submission` / `Op` / `EventMsg` / `AskForApproval` / `ExecApprovalRequirement` SHALL round-trip through serde JSON with no field loss — covered by the wire_roundtrip tests in commit `3e5940c`; marked VERIFIED

#### Scenario: W3cTraceContext OTel attachment (VERIFIED)
- **WHEN** `W3cTraceContext::from_current_otel()` is called inside an active OTel span
- **THEN** the resulting `traceparent` / `tracestate` pair SHALL round-trip via `attach_to_current_otel()` and restore the same OTel context — covered by commit `5538a06`'s `tracestate` preservation test; marked VERIFIED

#### Scenario: AskForApproval Granular config
- **WHEN** a caller sends a `Submission { op: Op::ApprovalResponse { ... }, trace }` referencing a `GranularApprovalConfig`
- **THEN** the server SHALL resolve `ExecApprovalRequirement::NeedsApproval | Skip | Forbidden` per-tool deterministically — covered by the `approval.rs` types in commit `3e5940c`

---

### Requirement: synthia-session-v2 part-based message model (Message / Part / ToolPart / ToolState)

The v3 architecture SHALL ship a `synthia-session-v2` crate that replaces the flat `Session` struct with `Message { info: MessageInfo, parts: Vec<Part> }` (opencode `WithParts`-style), an 11-variant `Part` enum (`Text` / `Reasoning` / `Tool` / `File` / `StepStart` / `StepFinish` / `Patch` / `Snapshot` / `Compaction` / `Subtask` / `Agent` / `Custom`), and a `ToolPart { call_id, tool_name, args, state: ToolState, metadata, attachments, time }` with a 4-state machine (`Pending` / `Running` / `Completed` / `Error`) and a type-safe `ToolTime { start, end, compacted: Option<DateTime<Utc>> }`.

This requirement is **VERIFIED** as of 2026-07-14: the planning skeleton was authored 2026-07-12 but never committed, and the substantive work landed in commit `50277c4` (R2). See the Archive Note at the top of [tasks.md](../tasks.md) for the full commit-by-commit mapping.

#### Scenario: planning skeleton never committed (VERIFIED)
- **WHEN** inspecting `git log -- openspec/changes/synthia-session-v2/` and the parent history
- **THEN** no commit SHALL introduce `crates/synthia-session-v2/` or `crates/synthia-protocol/` from this change folder — confirmed; the planning markdown files exist on disk and have never been committed

#### Scenario: part-based message model shipped (VERIFIED)
- **WHEN** running `git log --oneline -- crates/synthia-session-v2/src/`
- **THEN** the command SHALL show commit `50277c4` introducing `message.rs` / `part.rs` / `tool_part.rs` / `entry.rs` / `tree.rs` / `session_versions.rs` — confirmed; marked VERIFIED

#### Scenario: serializer round-trip (VERIFIED)
- **WHEN** running `cargo test -p synthia-session-v2 --test serializer_roundtrip`
- **THEN** every `Message` / `Part` / `ToolPart` / `SessionEntry` SHALL round-trip through serde JSON with `ToolState` transitions preserved — covered by commit `50277c4`'s test suite; marked VERIFIED

#### Scenario: ToolTime.compacted type-safety guard (VERIFIED)
- **WHEN** deserializing a `ToolTime` whose `compacted` field is the string `"not-a-date"`
- **THEN** the deserializer SHALL reject the value rather than coerce — covered by `tests/type_safety.rs` in commit `50277c4`; marked VERIFIED

---

### Requirement: SessionTree + SessionManager + background JSONL writer

`synthia-session-v2` SHALL expose `SessionTree { entries, children, root, leaf, paths_from_root }` (BTreeMap+HashMap layout with `paths_from_root` cache invalidated on leaf change), `SessionManager { tree: Arc<RwLock<SessionTree>>, path: Arc<RwLock<PathBuf>>, write_tx: mpsc::Sender<TreeCmd>, flush_handle: Mutex<Option<JoinHandle<()>>> }` with `append` (oneshot ack) / `flush` / `shutdown`, and a `session_writer_task` background task (mpsc::Receiver<TreeCmd>, 50ms batch tick, bounded 10_000 capacity, fsync on flush, JoinHandle on shutdown).

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `92bef17` (R4).

#### Scenario: 1000-op append stress (VERIFIED)
- **WHEN** running `cargo test -p synthia-session-v2 --test stress -- --nocapture`
- **THEN** 1000 `TreeCmd` appends SHALL complete without blocking the caller thread — covered by commit `92bef17`'s `tests/stress.rs`; marked VERIFIED

#### Scenario: paths_from_root cache invariant
- **WHEN** the `leaf` pointer changes via `branch(target)`
- **THEN** `paths_from_root` SHALL be invalidated and recomputed on next access — covered by commit `92bef17`'s tree cache test

#### Scenario: drain + join on shutdown
- **WHEN** `SessionManager::shutdown()` is called with pending commands in the channel
- **THEN** the writer task SHALL drain the channel, fsync, then return — covered by commit `92bef17`'s flush test

---

### Requirement: branch / branch_with_summary / fork non-destructive operations

`synthia-session-v2` SHALL expose three non-destructive tree operations: `branch(target)` (updates the `leaf` pointer to `target` without rewriting history), `branch_with_summary(target, summary)` (appends a `BranchSummary` entry then moves the leaf), and `fork(at_message_id)` (clones the subtree rooted at `at_message_id` to a fresh `SessionId`).

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `92bef17` (R4).

#### Scenario: branch updates leaf only (VERIFIED)
- **WHEN** `branch(target)` is called with a valid entry id
- **THEN** `SessionTree.leaf` SHALL point at `target` while `entries` and `children` are unchanged — covered by commit `92bef17`'s branch test; marked VERIFIED

#### Scenario: fork produces independent subtree (VERIFIED)
- **WHEN** `fork(at_message_id)` is called
- **THEN** a new `SessionId` SHALL own a clone of the subtree starting at `at_message_id`, with the original `SessionTree` unchanged — covered by commit `92bef17`'s fork test; marked VERIFIED

---

### Requirement: synthia-session/store/ collapsed to thin re-export shim

The legacy `crates/synthia-session/src/store/` module (originally 21 files / 5059 LOC covering `checkpoint`, `event_log`, `metadata`, `persistence`, `index`, `query`, `backup`, `recovery`, `compaction`, `cache`, `config`, `error`, `fsutil`, `io`, `lock`, `path_util`, `schema`, `search`, `store_lock`, `transaction`, `version`) SHALL be replaced by a thin re-export shim that delegates to `synthia-session-v2` types via `pub use synthia_session_v2::*;` and `#[deprecated]` aliases for backward compatibility.

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `facd3a9` (R3), which deleted 9 of the 21 store files and converted the remainder into a re-export shim ≤ 200 LOC.

#### Scenario: store module path still resolves (VERIFIED)
- **WHEN** downstream code imports `synthia_session::store::*`
- **THEN** the import SHALL resolve via the shim re-exports without breaking the build — covered by commit `facd3a9`; marked VERIFIED

#### Scenario: deprecation warnings emitted
- **WHEN** `cargo build` runs on a consumer of the legacy store API
- **THEN** deprecation warnings SHALL be emitted on every moved type — covered by `crates/synthia-session/src/deprecation.rs` in commit `facd3a9`

#### Scenario: idempotent migration v1→v2→v3
- **WHEN** `migrate_v1_to_v2` / `migrate_v2_to_v3` are run twice on the same input
- **THEN** the second run SHALL be a no-op (gated by `version: u32` field) — covered by `crates/synthia-session/src/migration.rs` idempotency test in commit `facd3a9`

---

### Requirement: AgentRunConfig split into RunContext / ToolContext / SessionContext

The monolithic `AgentRunConfig` (17 fields) SHALL be split into three composable sub-contexts: `RunContext { cancel_token, user_id, session_id, input }`, `ToolContext { tool_orchestrator, tool_router, approval_handler }`, and `SessionContext { session_manager }`. Zero-copy `SubContext` views SHALL be provided for ergonomic field access from the monolithic config.

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `38ab080` (R5).

#### Scenario: 3 sub-contexts composable (VERIFIED)
- **WHEN** `crates/synthia-agent/tests/context_split.rs` runs
- **THEN** `RunContext` / `ToolContext` / `SessionContext` SHALL be independently constructible and passable to the agent loop — covered by commit `38ab080`; marked VERIFIED

#### Scenario: SubContext zero-copy view
- **WHEN** a caller holds an `&AgentRunConfig` and reads `ctx.session_id()` via `SubContext`
- **THEN** the read SHALL NOT clone the underlying data — covered by commit `38ab080`'s SubContext impl

#### Scenario: main_loop.rs ≤ 400 LOC (VERIFIED)
- **WHEN** `wc -l crates/synthia-agent/src/main_loop.rs` is run after commit `38ab080`
- **THEN** the line count SHALL be ≤ 400 — confirmed; marked VERIFIED

---

### Requirement: Server/CLI wire protocol over HTTP/WS

`synthia-server` SHALL expose `POST /submission` (axum::Json<Submission>, dispatches via `SessionManager::append`) and `GET /ws` (WebSocket upgrade streaming `EventMsg`, with `extension.hook` OTel span per event). The legacy `POST /run_stream` SHALL be retained as `#[deprecated]` for one minor version. `synthia-cli` SHALL consume the same protocol via an opt-in `--wire` flag.

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `07e657e` (R6).

#### Scenario: 5 HTTP behaviors round-trip (VERIFIED)
- **WHEN** `cargo test -p synthia-server --test wiremock_submission` runs
- **THEN** `SubmitUserInput`, `Interrupt`, `Compact`, `ThreadRollback`, and `ApprovalResponse` submissions SHALL all reach `SessionManager::append` — covered by commit `07e657e`; marked VERIFIED

#### Scenario: WebSocket event stream
- **WHEN** `cargo test -p synthia-server --test wiremock_ws` runs
- **THEN** the WebSocket SHALL receive ≥ 5 `EventMsg` frames from the agent loop — covered by commit `07e657e`

#### Scenario: OTel span per WS event
- **WHEN** the server emits an `EventMsg` over `/ws`
- **THEN** an `extension.hook` OTel span SHALL be emitted with the event id as a span attribute — covered by commit `07e657e`'s span emission test

#### Scenario: legacy /run_stream deprecation
- **WHEN** a client calls `POST /run_stream`
- **THEN** the server SHALL respond with a deprecation header `Sunset: ...` and the response body — covered by `crates/synthia-server/src/routes/legacy_run_stream.rs` in commit `07e657e`

---

### Requirement: ProviderRegistry v2 with source_id hot-swap

The canonical runtime provider registry in v3 SHALL be `ProviderRegistry v2` (commit `6f48d76`) with `tokio::sync::RwLock<HashMap<String, RegisteredProvider>>` storage, `RegisteredProvider { provider, source_id }` records, and three operations: `register(name, provider, source_id)` (re-register with same `source_id` REJECTS), `unregister(name, source_id)` (silently ignores missing), and `replace_source(source_id, new_set)` (atomic single-writer swap).

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `6f48d76` (R7).

#### Scenario: source_id isolation (VERIFIED)
- **WHEN** two providers register under the same `name` but different `source_id`
- **THEN** `ProviderRegistry::get(name)` SHALL return both, distinguishable by `source_id` — covered by `crates/synthia-provider/tests/source_id_isolation.rs` in commit `6f48d76`; marked VERIFIED

#### Scenario: atomic hot-swap (VERIFIED)
- **WHEN** `ProviderRegistry::replace_source(source_id, new_set)` is called while readers iterate
- **THEN** the swap SHALL be atomic with respect to readers (no torn reads) — covered by `crates/synthia-provider/tests/hot_swap.rs` in commit `6f48d76`; marked VERIFIED

#### Scenario: re-register under same source REJECTS
- **WHEN** a caller attempts `register(name, provider, source_id)` for a `(name, source_id)` pair that is already registered
- **THEN** the call SHALL return an error rather than silently replacing — covered by commit `6f48d76`'s rejection test

#### Scenario: 3 extension events emitted
- **WHEN** any of `register` / `unregister` / `replace_source` runs
- **THEN** the corresponding `ProviderRegister` / `ProviderUnregister` / `ExtensionHotSwap` event SHALL be emitted via `synthia-provider/src/extension.rs` — covered by commit `6f48d76`'s event emission test

---

### Requirement: 9-abstractions toolification verified on the build path

Per the `9-abstractions-toolification/spec.md` spec, all 9 non-Tool abstractions SHALL be reachable through the standard `ToolRegistry` registration path. The build-path proof (commit `7393a7a`) SHALL verify: (a) `ExternalHookTool` actually subscribes to `ExtensionEvent`, (b) `QuerySkillUsageTool` has a full search+filter+format implementation, (c) MCP integration binds to `ExtensionTool`, (d) plugin CLI entries with `kind: Tool` in their manifest register via `ExtensionTool`, and (e) all 9 abstractions pass the `crates/synthia-agent/tests/9_abstractions.rs` integration test.

This requirement is **VERIFIED** as of 2026-07-14: covered by commit `7393a7a` (R8).

#### Scenario: 9-abstractions integration test passes (VERIFIED)
- **WHEN** running `cargo test -p synthia-agent --test 9_abstractions`
- **THEN** all 9 abstraction integration tests SHALL pass (`spec_names_list_has_nine_entries`, `spec_names_are_all_distinct`, `query_skill_usage_tool_impl_exists`, `compact_context_tool_impl_exists`, `empty_registry_reports_empty`, plus 4 more) — confirmed; marked VERIFIED

#### Scenario: ExternalHookTool subscription assertion (VERIFIED)
- **WHEN** `ExternalHookTool::new()` is called and an `ExtensionEvent` is dispatched
- **THEN** the tool SHALL receive the event via its `bind_extension` subscription — covered by commit `7393a7a`'s subscription assertion test; marked VERIFIED

#### Scenario: 5 spec hygiene updates + 1 verification (VERIFIED)
- **WHEN** inspecting `## Purpose` sections in `architecture-audit`, `agent-bus`, `context-compaction`, `agent-react-loop`, and `convergent-prompt-assembly` specs
- **THEN** none SHALL carry `TBD Purpose` boilerplate — covered by commit `7393a7a`; marked VERIFIED

#### Scenario: architecture-audit spec VERIFIED scenario
- **WHEN** the `architecture-audit` spec's `### Requirement: ...` sections are inspected
- **THEN** each SHALL have a matching `#### Scenario: VERIFIED` mechanically completed — covered by commit `7393a7a`

---

