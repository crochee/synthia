# Design: synthia-event-first (Change 2 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval
**Parent**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.2
**Depends on**: Change 1 (`AgentTool` + `ExtensionTool` + `AnyExtensionContext` types must exist)
**Absorbs**: `extension-points-phase-2` R2/R3/R4 (35 of 43 remaining points)

## Context

Synthia's main loop is **not thin**. `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` is 1037 LOC and contains:
- 11 `emit_turn_event(...)` call sites for JSONL logging
- 4+ hardcoded tool-name string comparisons (`SELF_REFLECT_TOOL_NAME`, `compact_context_tool`, `doom_loop_detected`, `sample_cascade_*`)
- Doom-loop handling with hardcoded `DefaultDoomLoopHandler::Cancel` (86 LOC)
- LLM-driven-vs-auto compact-context dispatch (lines 752-795)
- `format_background_task_notification` XML inline (lines 82-99)
- 5 separate session-end-reason mutation sites

Meanwhile, `synthia-permission/src/approval.rs` is 2098 LOC of hardcoded permission layer that the orchestrator partially bypasses (`synthia-tool-orchestrator/src/lib.rs:595-618` reimplements a 12-line match).

Three production agents have independently converged on **event-driven everything**:
- **opencode** (`packages/opencode/src/session/session.ts:355-375`, `permission/index.ts:23-187`) — `permission.asked`/`replied` event bus; 3 actions (`allow/deny/ask`) with "always" auto-resolve
- **codex** (`codex-rs/core/src/tools/orchestrator.rs:132-482`) — 350-line approval state machine + `AskForApproval` enum + `Granular` config + sandbox-denial escalation (lines 280-468)
- **pi-mono** (`packages/coding-agent/src/core/extensions/types.ts:950-972`) — 27 `ExtensionEvent` variants + `extension-first` design + `runner.ts:680-712` emit/bind/invalidate lifecycle

**Reusable assets (from in-flight or already-shipped)**:
- `crates/synthia-agent/src/tools/dynamic_provider/extension_points/` — 10 scope modules (60 declared extension points); **Zero call sites from main_loop**
- `crates/synthia-agent/src/tools/dynamic_provider/extension_context.rs` — three-state lifecycle (`Loading/Active/Stale`)
- `crates/synthia-agent/src/tools/dynamic_provider/extension_manager.rs` — `ExtensionManager` with cache invalidation
- `crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs` — `Action<T>` mutation pattern + wildcard matching
- `crates/synthia-permission/src/permission_future.rs` — `PermissionFuture` (from `production-grade-agent-architecture`)
- `crates/synthia-agent/src/doom_loop_handler.rs` — `DoomLoopDetector` (from `production-grade-agent-architecture`)

**Hard constraints (must not violate)**:
- P1 (KV-cache prefix consistency)
- P6 (Permission/DoomLoop fail-closed — `Ask`, not `Allow`)
- P9 (every `fire_*` and every state transition emits OTel span)
- No `unsafe`
- Type safety: every public surface `Send + Sync + 'static`

## Goals

1. Adopt **27 `ExtensionEvent` typed events** organized into 6 categories (Session/Agent/Tool/LLM/Permission/Plugin)
2. Adopt **`ExtensionRegistry` + `ExtensionCtx` three-state lifecycle** with `Action<T>` mutation
3. Shrink **Permission from 2098 → ≤ 500 LOC** via event-driven handler
4. Replace **hardcoded DoomLoop** with `DefaultDoomLoopExtension`
5. Shrink **`main_loop.rs` from 1037 → ≤ 400 LOC**
6. Simplify **`StreamBuilder` from 6 step types + 14 type params → `Step::Hook | Step::Tool` (2 variants)**
7. Wire **35 of 43 remaining extension points** (Round 2-4 of `extension-points-phase-2`)
8. **Zero behavioral regression** on the 5 historical e2e tests

## Non-Goals

- Wire Protocol (Submission/EventMsg/W3cTraceContext) — **Change 3**
- JSONL append-only session tree — **Change 3**
- Provider hot-swap with source_id isolation — **Change 3 R7**
- 9-abstractions external hook tool + plugin CLI as Tool — **Change 3 R8**
- jiti-style compile-time extension loading — explicitly rejected
- WASM tool provider — explicitly rejected

## Architecture

### Module Structure

```
crates/synthia-event/                       # NEW crate (primary landing spot)
├── lib.rs                                  # re-exports
├── event.rs                                # ExtensionEvent enum (27 variants)
├── registry.rs                             # ExtensionRegistry + emit() + wildcard
├── context.rs                              # ExtensionCtx { state, ... } three-state
├── action.rs                               # Action<T> mutation pattern
├── filter.rs                               # ExtensionFilter predicate
├── guard.rs                                # PermissionExtensibilityGuard
├── permission/                             # submodule: PermissionFuture::from_event, DefaultPermissionHandler
├── doom_loop/                              # submodule: DefaultDoomLoopExtension
├── output/                                 # submodule: OutputSink + ui.format + ui.metadata
├── tests/

crates/synthia-permission/                 # SHRUNK (1355 → ≤500 LOC)
├── approval.rs                             # DefaultPermissionHandler event-driven

crates/synthia-agent/                      # MODIFIED (main_loop slim)
├── stream_builder/builder/run/main_loop.rs # 1037 → ≤400 LOC
├── stream_builder/builder/step.rs          # StreamBuilder + Step enum
├── doom_loop_handler.rs                    # DELETED (replaced by DefaultDoomLoopExtension)
├── emit_*.rs                               # REMOVED (replaced by EventSink extension)
├── stream_builder/steps/                   # DELETED 6-step subgraph

crates/synthia-agent/src/tools/dynamic_provider/extension_points/   # EXTENDED
├── permission.rs                           # NEW (Round 2)
├── provider.rs                             # NEW (Round 2)
├── event_bus.rs                            # NEW (Round 3)
├── plugin_lifecycle.rs                     # NEW (Round 3)
├── session_tree.rs                         # NEW (Round 4)
├── output_ui.rs                            # NEW (Round 4)
```

### Core Data Structures

```rust
// crates/synthia-event/src/event.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionEvent {
    // Session Lifecycle (6)
    SessionStart            { session_id: SessionId, agent_name: String, ctx: SystemContextSnapshot },
    SessionShutdown         { session_id: SessionId, reason: ShutdownReason },
    SessionBeforeCompact    { session_id: SessionId, current_tokens: u64, threshold: u64, can_cancel: bool, prior_summary: Option<String> },
    SessionCompact          { session_id: SessionId, summary: CompactionPart, dropped_message_ids: Vec<MessageId> },
    SessionBeforeRollback   { session_id: SessionId, to_message_id: MessageId, snapshot: Option<FileSnapshot> },
    SessionAfterRollback    { session_id: SessionId, to_message_id: MessageId },

    // Agent Lifecycle (6)
    AgentStart              { agent_name: String, parent_agent: Option<String> },
    AgentEnd                { agent_name: String, status: AgentStatus },
    TurnStart               { session_id: SessionId, user_text: Option<String> },
    TurnEnd                 { session_id: SessionId, status: TurnStatus, duration_ms: u64 },
    IterationStart          { turn_id: TurnId, n: u32 },
    IterationEnd            { turn_id: TurnId, n: u32, tools_called: Vec<ToolName> },

    // Tool Lifecycle (7)
    ToolDefinitionTransform { tool: ToolSpec, ctx: TransformCtx },
    ToolCall                { call_id: CallId, tool_name: ToolName, args: Value, parent_turn: TurnId, decision: ToolDecision },
    ToolResult              { call_id: CallId, output: ToolOutputBox, is_error: bool, duration_ms: u64 },
    ToolExecutionUpdate     { call_id: CallId, partial: PartialToolOutput },
    ToolSearchResult        { query: ToolSearchQuery, returned: Vec<LoadableToolSpec> },
    ToolParallelismBarrier  { tool_name: ToolName, barrier_id: BarrierId, waiters: usize },
    ToolRegistryChange      { added: Vec<ToolName>, removed: Vec<ToolName>, source_id: String },

    // LLM / Provider (5)
    BeforeProviderRequest   { payload: ProviderPayload, trace: Option<W3cTraceContext> },
    ProviderResponse        { response: ProviderResponseView, trace: Option<W3cTraceContext> },
    ChatParamsTransform     { params: ChatParams, ctx: TransformCtx },
    MessageSend             { role: Role, parts: Vec<Part>, ctx: TransformCtx },
    MessageReceive          { message: Message, ctx: TransformCtx },

    // Permission (3)
    PermissionAsk           { request: PermissionRequest, reply_tx: tokio::sync::oneshot::Sender<PermissionReply> },
    PermissionNotify        { request: PermissionRequest, decision: PermissionDecision },
    DoomLoopDetected        { fingerprint: u64, tool_name: ToolName, count: u32, severity: DoomLoopSeverity },
}

// crates/synthia-event/src/registry.rs
pub struct ExtensionRegistry {
    handlers: DashMap<&'static str, Vec<Box<dyn AnyExtensionHandler>>>,
    active_keys: DashMap<String, ()>,
}

impl ExtensionRegistry {
    pub fn new() -> Self { ... }
    pub fn on_event(&self, id: impl Into<String>, handler: Arc<dyn AnyExtensionHandler>) { ... }
    pub async fn emit(&self, event: ExtensionEvent) -> Result<Option<Action<ExtensionEvent>>, ExtensionError> {
        // 1. OTel span (P9)
        let _span = tracing::info_span!(target: "synthia.extension", "extension.hook", ...);
        // 2. Walk handlers in order; each may return Action<T> or error
        // 3. Apply last-Action<T> mutation
    }
}

// crates/synthia-event/src/context.rs
pub enum ExtensionCtxState { Loading, Active, Stale { reason: String } }

pub struct ExtensionCtx {
    state: parking_lot::Mutex<ExtensionCtxState>,
    actions: ExtensionRegistry,
    cancel_token: tokio_util::sync::CancellationToken,
}

impl ExtensionCtx {
    pub fn assert_active(&self) -> Result<(), StaleContextError> { ... }
    pub async fn emit(&self, event: ExtensionEvent) -> Result<...> { ... }
}

// crates/synthia-event/src/action.rs
pub enum Action<T> {
    Proceed,
    Modify(T),
    Skip { reason: String },
    Abort { reason: String },
}

// crates/synthia-event/src/permission/mod.rs
pub struct DefaultPermissionHandler {
    inner: Arc<dyn ApprovalService + Send + Sync>,
}

impl DefaultPermissionHandler {
    pub async fn check(&self, request: PermissionRequest) -> Result<PermissionDecision, PermissionError> {
        let (tx, rx) = oneshot::channel();
        tokio::select! {
            // 50ms grace period: if no listener fires, fall back to policy
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                self.inner.fallback(&request).await
            }
            reply = rx => match reply {
                Ok(reply) => self.inner.evaluate(reply).await,
                Err(_) => self.inner.fallback(&request).await, // cancelled
            }
        }
    }
}

// crates/synthia-event/src/doom_loop/mod.rs
pub struct DefaultDoomLoopExtension;

impl ExtensionTool for DefaultDoomLoopExtension {
    async fn bind_extension(&self, ctx: Arc<dyn AnyExtensionContext>) {
        ctx.on_event_filter(|e| matches!(e, ExtensionEvent::ToolCall { .. }),
                            |e| Box::pin(self.maybe_emit_doom_loop(e))).await;
    }
}
```

### 7 Implementation Rounds

| Round | Scope | LOC | Files | Verification |
|-------|-------|-----|-------|--------------|
| **R1** | `synthia-event` skeleton + 27 events + registry/ctx/action | +1500 | 1 new crate | All event variant unit tests |
| **R2** | Permission event-driven; absorb extension-points R2 (Permission × 5 + Provider × 4 = 9 points) | -850 | shrink + 2 new ext points | PermissionExtensibilityGuard test |
| **R3** | DoomLoop event-driven; absorb extension-points R3 (EventBus × 4 + PluginLifecycle × 6 = 10 points); `ExtensionTool` ×8 internal migration starts | -86 + 800 | 2 new ext points + 8 modified | DoomLoop integration test |
| **R4** | `main_loop.rs` rewrite ≤ 400 LOC; remove 11 emit sites + 4+ tool-name string compares | -650 | 1 file replaced | 5 historical e2e pass |
| **R5** | StreamBuilder 6 step → 2 step | -400 | 1 file replaced | `Step::Hook | Step::Tool` enum covers all behaviors |
| **R6** | extension-points R4 (Session Tree × 5 + Output/UI × 4 = 9 points) + 64-point integration | +800 | 2 new ext points + 1 new test | 64-point integration test pass |
| **R7** | 8 internal `ExtensionTool` migrations (compact_context, subagent, guardian, monitor, MCP, hook, usage, plugin_cli) | +400 | 8 modified | bind_extension no-op tests |

### Hard rules per Round

1. **Every `emit` must produce an OTel span** (P9)
2. **Permission fail-closed**: 50ms timeout → `Ask`, never `Allow` (P6)
3. **`ExtensionCtx::Stale` panics on action methods** — `assert_active()` enforced RAII
4. **No `unsafe`**
5. **No `as any` / `#[allow(async_fn_in_trait)]`**
6. **Backward compat**: legacy `ApprovalService::check` continues to compile throughout 0.2.x

## Migration / Rollback

**On deprecation** (R2):
```rust
// crates/synthia-permission/src/approval.rs
#[deprecated(since = "0.2.0", note = "use PermissionFuture::from_event")]
impl ApprovalService { pub async fn check_sync(...) }
```

**On removal** (next major 0.3.0):
- `check_sync` is removed
- `synthia-permission` becomes a thin re-export of `synthia-event::permission`

**Rollback path**: revert commits in reverse order; `synthia-event` is additive the entire 0.2.x cycle.

## Validation Standard

After every Round:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test -p synthia-agent -p synthia-event -p synthia-permission
cargo test -p synthia-agent --test react_loop_test --test e2e_llm_test --test e2e_event_sequence_test --test e2e_memory_correctness_test
```

Specific:
- `cargo test -p synthia-event` — all 27 event variant tests + state machine + Action<T> tests green
- `cargo test -p synthia-agent --test doom_loop_via_event` (new) — DoomLoop integration green
- `cargo test -p synthia-agent --test extension_matrix` (new) — 64-point full integration green
- 5 historical e2e tests unchanged

## Open Questions

1. **W3cTraceContext on `BeforeProviderRequest`** — defined here or Change 3? — **Change 3** (depends on `Submission` definition)
2. **Compact-context dispatch — LLM-vs-auto inside `SessionBeforeCompact`** — handle here or Change 3? — **Change 2** (per event scope definition)
3. **`format_background_task_notification` migration to `OutputSink`** — write `OutputSink` here or Change 3? — **Change 2** (UI events are extension lifecycle scope)
4. **DoomLoop severity enum** — 3 levels (Warning/Critical/Fatal) or 2? — **3 levels** (warning can route to permission, critical aborts, fatal terminates)
5. **PermissionFuture timeout value** — 50 ms or configurable? — **configurable**, default 50 ms

## Reference

- Parent design: [design.md](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- Proposal: [proposal.md](../proposal.md)
- Plan: [plan.md](../plan.md)
- Tasks: [tasks.md](../tasks.md)
- pi-mono: `packages/coding-agent/src/core/extensions/types.ts:950-972`, `runner.ts:680-712`, `loader.ts:134-180`
- opencode: `permission/index.ts:23-187`, `session/session.ts:355-375`
- codex: `codex-rs/core/src/tools/orchestrator.rs:132-482`, `protocol/src/protocol.rs:807-855`
- Existing in-flight: [`extension-points-phase-2`](../extension-points-phase-2/) (43 points)
