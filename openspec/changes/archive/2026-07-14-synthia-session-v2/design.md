# Design: synthia-session-v2 (Change 3 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval
**Parent**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.3
**Depends on**: Changes 1 + 2 must be merged first
**Absorbs**: 9-abstractions-toolification spec; add-dynamic-tool-provider-system; adopt-explore-agent-recommendations

## Context

Synthia has **two parallel session backends**, neither adequate:

1. `crates/synthia-session/src/types/session.rs` — flat `Session` struct (13 fields), no `Message`/`Part` abstraction. The hardcoded `ToolOutput` has `is_error: Option<bool>` only — no state machine; tool-call story is replay-as-string.
2. `crates/synthia-session/src/store/` — 21 files, 5059 LOC: synchronous file writes per append (no background task, no `Flush`, no `Shutdown`), checkpoint + event_log + metadata.json **3 ledgers**. No `branch()`/`fork()`/non-destructive rollback. No `W3cTraceContext`.

Three production agents have independently converged on **part-based + JSONL + wire protocol**:
- **opencode** (`packages/opencode/src/session/message-v2.ts:206-413`, `packages/core/src/v1/session.ts:253-385`): `WithParts { info, parts: Part[] }` 11-variant discriminated union; `ToolPart.state: ToolState` 4-machine with `time.compacted: Option<NonNegativeInt>`
- **codex** (`codex-rs/protocol/src/protocol.rs:155-1239+`, `codex-rs/rollout/src/recorder.rs:74-153`): `Submission { id, op, trace }` + `EventMsg` wire envelope + `W3cTraceContext` + `RolloutRecorder` background-task JSONL writer with `mpsc` + oneshot ack
- **pi-mono** (`packages/coding-agent/src/core/session-manager.ts:44-49, 669-1163`): append-only JSONL tree with `id`/`parentId` 8-char ULID; `branch(fromId)` non-destructive leaf-pointer move

**Reusable assets (from in-flight or already-shipped)**:
- All 27 `ExtensionEvent` from Change 2 R1 — wires directly into `Op::Compact` / `Op::ThreadRollback` etc.
- `PermissionRequest`, `PermissionDecision`, `DoomLoopSeverity` from Change 2 R2
- `AgentTool` + `ExtensionTool` dual shape from Change 1 R2
- `ToolRouter` model-visible + `ToolRegistry` runtime dispatch from Change 1 R4
- `DefaultDoomLoopExtension` from Change 2 R3
- 4 `ToolProvider`s (File/Bash/MCP/Search) from Change 1 R6
- Existing `extension_points/session_tree.rs` (Change 2 R6) with 5 Session Tree extension points

**Hard constraints (must not violate)**:
- P1 (KV-cache prefix consistency) — `W3cTraceContext::from_current_otel()` must read actual current context, not stub
- P8 (no information loss) — append-only JSONL; only index changes
- P9 (every fire emits OTel span) — wire protocol emits per-message OTel spans
- P10 (file as memory) — any registry state must be reproducible from disk
- No `unsafe`
- Type safety: every public surface `Send + Sync + 'static`
- Backward compat: legacy `synthia-session/store/` retains deprecation shim for one minor

## Goals

1. Adopt **opencode V2 message model**: `Message { info, parts[] }` + 11-variant `Part`
2. Adopt **opencode `ToolPart` 4-state machine** with type-safe `time.compacted: Option<u64>`
3. Adopt **pi-mono JSONL append-only tree** with `id`/`parentId` + `branch()`/`fork()`/`build_context()`
4. Adopt **codex `Submission`/`EventMsg`/`W3cTraceContext`** wire protocol for CLI/server/IDE clients
5. Adopt **codex `AskForApproval` + `Granular`** config
6. Adopt **codex `ProviderRegistry` v2** with `source_id` isolation hot-swap
7. Adopt **codex `RolloutRecorder`** pattern: background JSONL writer via `mpsc::Sender<TreeCmd>` + oneshot ack
8. **9-abstractions-toolification**: full closure including `ExternalHookTool` + `QuerySkillUsageTool` + plugin CLI
9. **Spec hygiene**: 5 highest-impact specs get proper `## Purpose`; 1 verification (`architecture-audit`)
10. **`main_loop.rs` ≤ 400 LOC** (per Change 2 R4) — kept green through Change 3
11. **`session/store/ -100%`** — replaced by deprecation shim
12. **Zero behavioral regression** on the 5 historical e2e tests

## Non-Goals

- SQLite-derived metadata mirror — separate Change
- Codex-style `code-mode` JS/WASM runtime — explicitly rejected
- Bazel build system — rejected
- Multi-agent namespaces — rejected
- jiti-style compile-time extension loading — rejected
- Migration to v4 message format (future, when needed)

## Architecture

### Module Structure

```
crates/synthia-protocol/                     # NEW crate
├── lib.rs
├── submission.rs                            # Submission + Op
├── event.rs                                 # EventMsg
├── trace.rs                                 # W3cTraceContext
├── approval.rs                              # AskForApproval enum + GranularApprovalConfig
├── error.rs                                 # wire-level errors
├── version.rs                               # PROTOCOL_VERSION = 2
└── tests/

crates/synthia-session-v2/                   # NEW crate (replaces synthia-session/store/)
├── lib.rs
├── entry.rs                                 # SessionEntry 14-variant enum
├── tree.rs                                  # SessionTree (HashMap+tree links)
├── manager.rs                               # SessionManager + mpsc write_tx + flush_handle
├── branch.rs                                # branch + branch_with_summary + fork
├── reload.rs                                # load + idempotent migrate_vN_to_vM
├── replay.rs                                # Replay service (for UI)
├── session_versions.rs                      # CURRENT_SESSION_VERSION = 2
├── message.rs                               # Message { info: MessageInfo, parts: Vec<Part> }
├── part.rs                                  # Part 11-variant enum
├── tool_part.rs                             # ToolPart + ToolState 4-state machine
├── writer_task.rs                           # session_writer_task (background mpsc -> JSONL)
└── tests/

crates/synthia-session/                      # SHRUNK to migration shim
├── store/                                   # thin re-export of synthia-session-v2
└── lib.rs                                   # re-export everything

crates/synthia-agent/                        # MODIFIED
├── agent.rs                                 # AgentRunConfig splits to 3 sub-contexts
└── ...

crates/synthia-server/                       # NEW routes
├── routes/submission.rs                     # POST /submission
└── routes/ws_event.rs                       # GET /ws

crates/synthia-cli/                          # MODIFIED to consume new protocol
└── ...

crates/synthia-provider/src/registry/         # v2 added
├── v1.rs                                    # legacy RwLock<HashMap> (deprecation path)
└── v2.rs                                    # source_id-isolated hot-swap
```

### Core Data Structures

```rust
// crates/synthia-protocol/src/trace.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct W3cTraceContext {
    pub traceparent: String,
    pub tracestate: Option<String>,
}

impl W3cTraceContext {
    pub fn from_current_otel() -> Option<Self> { ... }
    pub fn attach_to_current_otel(&self) -> OpenTelemetryContext { ... }
    pub fn invalid() -> Self {
        Self {
            traceparent: "00-00000000000000000000000000000000-0000000000000000-00".to_string(),
            tracestate: None,
        }
    }
}

// crates/synthia-protocol/src/submission.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Interrupt        { reason: String },
    Compact          { manual: bool, summary_hint: Option<String> },
    UserInput        { items: Vec<InputItem>, final_output_json_schema: Option<Value>, additional_context: Option<String> },
    ThreadRollback   { num_turns: u32 },
    ApprovalResponse { id: ApprovalId, decision: PermissionDecision },
    RefreshTools,
    Resubmit         { message_ids: Vec<MessageId> },
    UpdateModel      { model: String },
    UpdateThinkingLevel { level: ThinkingLevel },
    SwitchSession    { session_id: SessionId },
    ForkSession      { at_message_id: MessageId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: SubmissionId,
    pub op: Op,
    pub client_user_message_id: Option<String>,
    pub trace: Option<W3cTraceContext>,
}

// crates/synthia-protocol/src/event.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum EventMsg {
    SessionCreated       { session_id, parent_session_id, cli_version },
    TurnStarted          { session_id, turn_id, model },
    TurnComplete         { session_id, turn_id, status },
    ToolCall             { session_id, turn_id, call_id, tool_name, args },
    ToolCallOutput       { session_id, turn_id, call_id, output },
    ApprovalRequest      { session_id, request },
    ApprovalResponded    { session_id, request_id, decision },
    CompactStarted       { session_id, reason, current_tokens, threshold, can_cancel },
    CompactCompleted     { session_id, summary, dropped_message_ids, new_leaf },
    ThreadRolledBack     { session_id, target_message_id, num_turns },
    TokenCount           { session_id, info },
    ModelRerouted        { session_id, from, to, reason },
    ToolSearched         { session_id, query, results },
    Error                { session_id, kind, payload, recoverable },
    Warning              { session_id, message },
}

// crates/synthia-session-v2/src/message.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub info: MessageInfo,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: MessageId,
    pub parent_message_id: Option<MessageId>,
    pub role: Role,
    pub time: MessageTime,
    pub agent_name: Option<String>,
    pub model_id: Option<String>,
    pub trace: Option<W3cTraceContext>,
    pub summary: bool,
    pub error: Option<MessageError>,
}

// crates/synthia-session-v2/src/part.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(ToolPart),
    File(FilePart),
    StepStart(StepStartPart),
    StepFinish(StepFinishPart),
    Patch(PatchPart),
    Snapshot(SnapshotPart),
    Compaction(CompactionPart),
    Subtask(SubtaskPart),
    Agent(AgentPart),
    Custom(CustomPart),
}

// crates/synthia-session-v2/src/tool_part.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPart {
    pub call_id: CallId,
    pub tool_name: ToolName,
    pub args: serde_json::Value,
    pub state: ToolState,
    pub metadata: HashMap<String, Value>,
    pub attachments: Vec<AttachmentRef>,
    pub time: ToolTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolState {
    Pending   { queued_at: DateTime<Utc> },
    Running   { started_at: DateTime<Utc>, partial_output: Option<String> },
    Completed { output: serde_json::Value, ended_at: DateTime<Utc>, duration_ms: u64 },
    Error     { message: String, interrupted: bool, ended_at: DateTime<Utc> },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolTime {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub compacted: Option<DateTime<Utc>>,    // type-safe Compaction marker
}

// crates/synthia-session-v2/src/entry.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEntry {
    Header       { id, parent_id, created_at, cli_version, rust_version, model_provider, agent_name, agent_role, sandbox_policy, approval_policy },
    Message      { id, parent_message_id, role, parts, time, agent_name, model_id, trace },
    Compaction   { id, parent_message_id, first_kept_message_id, tokens_before, from_hook, summary, dropped_message_ids },
    BranchSummary{ id, parent_message_id, from_message_id, summary, from_hook },
    ModelChange  { id, parent_message_id, from_model, to_model, reason },
    ThinkingLevelChange { id, parent_message_id, from, to },
    Label        { id, target_id, label, sticky },
    SessionInfo  { id, parent_session_id, name, labels },
    CustomMessageEntry { id, parent_message_id, payload, display, source },
    CustomEntry  { id, parent_message_id, payload, source },
    Fork         { id, parent_session_id, forked_at_message_id },
    Rollback     { id, target_message_id, num_turns },
    ErrorEvent   { id, parent_message_id, error_kind, recoverable, payload },
}

// crates/synthia-session-v2/src/tree.rs
pub struct SessionTree {
    pub entries: BTreeMap<MessageId, SessionEntry>,
    pub children: HashMap<MessageId, Vec<MessageId>>,
    pub root: SessionId,
    pub leaf: MessageId,
    pub paths_from_root: Vec<MessageId>,
}

// crates/synthia-session-v2/src/manager.rs
pub struct SessionManager {
    tree: Arc<RwLock<SessionTree>>,
    path: Arc<RwLock<PathBuf>>,
    write_tx: mpsc::Sender<TreeCmd>,
    flush_handle: Mutex<Option<JoinHandle<()>>>,
}

pub enum TreeCmd {
    Append  { entry: SessionEntry, ack: oneshot::Sender<Result<MessageId, SessionError>> },
    Flush   { ack: oneshot::Sender<Result<(), SessionError>> },
    Shutdown{ ack: oneshot::Sender<()> },
}

impl SessionManager {
    pub async fn append(&self, e: SessionEntry) -> Result<MessageId, SessionError> { ... }
    pub async fn branch(&self, target: MessageId) -> Result<(), SessionError> { ... }
    pub async fn branch_with_summary(&self, target: MessageId, summary: String) -> Result<MessageId, SessionError> { ... }
    pub async fn fork(&self, at: MessageId) -> Result<SessionId, SessionError> { ... }
    pub async fn build_context(&self) -> Result<Vec<Message>, SessionError> { ... }
    pub async fn open(path: &Path) -> Result<Self, SessionError> { ... }
}

// crates/synthia-protocol/src/approval.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AskForApproval {
    UnlessTrusted,
    OnFailure,
    OnRequest,
    Granular(GranularApprovalConfig),
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularApprovalConfig {
    pub sandbox_approval: bool,
    pub rules: bool,
    pub skill_approval: bool,
    pub request_permissions: bool,
    pub mcp_elicitations: bool,
}

// crates/synthia-provider/src/registry/v2.rs
pub struct ProviderRegistry {
    providers: tokio::sync::RwLock<HashMap<String, RegisteredProvider>>,
}

struct RegisteredProvider {
    provider: Arc<dyn ModelProvider>,
    source_id: String,
}

impl ProviderRegistry {
    pub fn register(&self, name: impl Into<String>, provider: Arc<dyn ModelProvider>, source_id: impl Into<String>) { ... }
    pub fn unregister(&self, name: &str, source_id: &str) -> Result<(), ProviderError> { ... }
    pub async fn replace_source(&self, source_id: &str, new_set: Vec<(String, Arc<dyn ModelProvider>)>) -> Result<usize> { ... }
}
```

### 8 Implementation Rounds

| Round | Scope | LOC | Files | Verification |
|-------|-------|-----|-------|--------------|
| **R1** | `synthia-protocol` skeleton (Submission/Op/EventMsg/W3cTraceContext/AskForApproval) | +1500 | 1 new crate | wire round-trip JSON tests |
| **R2** | `synthia-session-v2` skeleton (Message/Part/ToolPart + 4-state + SessionEntry 14-variant + SessionTree) | +1500 | 1 new crate | serializer round-trip |
| **R3** | Collapse `synthia-session/store/ 21 files → thin migration shim` | -3500 | 21 files deleted, 1 re-export | store tests pass via shim |
| **R4** | background writer (mpsc + 50ms batch + oneshot ack) | -50 | 1 file replaced | append under 1000 ops no blocking |
| **R5** | split `AgentRunConfig` (17 → 3 sub-contexts) | -400 | 1 file modified | agent.rs ≤ 700 LOC |
| **R6** | server/CLI wire protocol (axum POST /submission, GET /ws) | +600 | 3 new files | wiremock 5 HTTP behaviors |
| **R7** | `ProviderRegistry` v2 + 3 events (Register/Unregister/HotSwap) | +200 | 1 new file | source_id isolation test |
| **R8** | 9-abstractions-toolification full closure (ExternalHookTool full, QuerySkillUsageTool full, plugin CLI as Tool) | +800 | 6 modified | 9-abstractions test green |

### Hard rules per Round

1. **Every `W3cTraceContext::from_current_otel()` asserts non-empty current context** in debug builds
2. **Every append to JSONL is durable** (fsync per 50ms batch) — P10 file-as-memory
3. **Every wire event emits `extension.hook` OTel span** (P9)
4. **Migration shim remains compatible with old test code** throughout 0.2.x
5. **No `unsafe`**
6. **No `as any` / `#[allow(async_fn_in_trait)]`**
7. **Backward compat**: `synthia-server` keeps `run_stream` HTTP `/run_stream` POST endpoint as deprecated shim

## Migration / Rollback

**On deprecation** (R3):
```rust
// crates/synthia-session/store/mod.rs (shim)
// delegates to synthia_session_v2::* for everything except legacy types
```

**On removal** (next major 0.3.0):
- `synthia-session/store/` directory deleted entirely
- `synthia-session` crate becomes `pub use synthia_session_v2::*`

**Rollback path**: revert commits in reverse order; new crates are additive the entire 0.2.x cycle.

## Validation Standard

After every Round:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test -p synthia-protocol -p synthia-session-v2 -p synthia-session -p synthia-server -p synthia-cli -p synthia-agent -p synthia-provider
cargo test -p synthia-agent --test react_loop_test --test e2e_llm_test --test e2e_event_sequence_test --test e2e_memory_correctness_test
```

Specific:
- `cargo test -p synthia-protocol` — wire round-trip tests
- `cargo test -p synthia-session-v2` — migration idempotent tests
- `cargo test -p synthia-server` — wiremock HTTP/WS tests
- `cargo test -p synthia-agent --test 9_abstractions` — all 9 abstractions come through new path
- 5 historical e2e unchanged

## Open Questions

1. **`Session` header `version` field** — start at `2` or `3` (skip `1`)? — **start at `2`** (v1 was synthia's existing flat `Session`)
2. **`SYNTHETIC_ATTACHMENT_PROMPT` migration** — keep verbatim from opencode or rewrite? — **rewrite** in `serde::Deserialize` form
3. **Wire protocol serialization format** — JSON only or include `bincode` fallback? — **JSON only** (Simplicity First)
4. **CLI WebSocket client** — replace stdin/stdout or augment? — **augment** via opt-in `--wire` flag (default stays stdin)
5. **Provider v2 default migration path** — automatic or opt-in? — **opt-in** via `[experimental] provider_v2: true` in `config.toml` (1 minor cycle, default to v2)
6. **Migration shim lifetime** — keep 1 minor or 2? — **1 minor** (0.3.0 deletes shim)

## Reference

- Parent design: [design.md](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- Proposal: [proposal.md](../proposal.md)
- Plan: [plan.md](../plan.md)
- Tasks: [tasks.md](../tasks.md)
- opencode patterns: `packages/opencode/src/session/message-v2.ts:206-413`, `packages/core/src/v1/session.ts:253-385`
- codex patterns: `codex-rs/protocol/src/protocol.rs:155-1239+`, `codex-rs/rollout/src/recorder.rs:74-153`, `codex-rs/execpolicy/src/policy.rs:28-251`, `codex-rs/core/src/tools/orchestrator.rs:132-482`, `codex-rs/core/src/state/src/model/thread_metadata.rs`
- pi-mono patterns: `packages/coding-agent/src/core/session-manager.ts:44-49, 669-1163`
- 9-abstractions spec: [`openspec/specs/9-abstractions-toolification/spec.md`](../../specs/9-abstractions-toolification/spec.md)
- Absorbed: `add-dynamic-tool-provider-system`, `adopt-explore-agent-recommendations`
