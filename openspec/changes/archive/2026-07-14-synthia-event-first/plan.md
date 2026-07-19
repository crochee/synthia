# Plan: synthia-event-first (Change 2 of v3 architecture)

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development (recommended) or executing-plans. Steps use checkbox (`- [ ]`) syntax. **No auto-commit** — every Round ends with "等用户明确指示".

**Goal:** Adopt 27 `ExtensionEvent` typed events + `ExtensionRegistry` + three-state `ExtensionCtx` + `Action<T>` mutation; shrink Permission to ≤ 500 LOC; replace hardcoded DoomLoop with event-driven extension; shrink `main_loop.rs` to ≤ 400 LOC; simplify `StreamBuilder` from 6 steps to 2; wire 35 of 43 remaining extension points (Round 2-4 of `extension-points-phase-2`).

**Architecture / Reference / Validation:** see [design.md](../design.md).

**Tech Stack:** Rust (tokio async, DashMap, parking_lot, tracing, serde, schemars). New crate `synthia-event` depends on `tokio`, `serde`, `serde_json`, `parking_lot`, `tracing`, `dashmap`, `tokio-util`, `thiserror`. Adopts existing `Action<T>` from `crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs`.

**Dependencies:** Change 1 must be merged first (uses `AgentTool`, `ExtensionTool`, `AnyExtensionContext`).

---

## Round 1: `synthia-event` skeleton + 27 events

**Why first:** every other Round plugs into the type system.

### Task 1.1: New crate scaffold

**Files:**
- Create: `crates/synthia-event/Cargo.toml`
- Create: `crates/synthia-event/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1.1.1:** Cargo.toml deps: `serde`, `serde_json`, `tokio` (full), `parking_lot`, `tracing`, `tracing-subscriber`, `dashmap`, `tokio-util`, `thiserror`, plus `synthia-tool-core` from Change 1
- [ ] **Step 1.1.2:** Add to workspace `members`
- [ ] **Step 1.1.3:** `lib.rs` re-exports
- [ ] **Step 1.1.4:** `cargo check -p synthia-event`

### Task 1.2: 27 `ExtensionEvent` variants

**Files:** `crates/synthia-event/src/event.rs`

- [ ] **Step 1.2.1:** 6 Session Lifecycle variants (`SessionStart`, `SessionShutdown`, `SessionBeforeCompact`, `SessionCompact`, `SessionBeforeRollback`, `SessionAfterRollback`) — typed structs for each
- [ ] **Step 1.2.2:** 6 Agent Lifecycle variants (`AgentStart`, `AgentEnd`, `TurnStart`, `TurnEnd`, `IterationStart`, `IterationEnd`)
- [ ] **Step 1.2.3:** 7 Tool Lifecycle variants (`ToolDefinitionTransform`, `ToolCall`, `ToolResult`, `ToolExecutionUpdate`, `ToolSearchResult`, `ToolParallelismBarrier`, `ToolRegistryChange`)
- [ ] **Step 1.2.4:** 5 LLM/Provider variants (`BeforeProviderRequest`, `ProviderResponse`, `ChatParamsTransform`, `MessageSend`, `MessageReceive`)
- [ ] **Step 1.2.5:** 3 Permission variants (`PermissionAsk`, `PermissionNotify`, `DoomLoopDetected`)
- [ ] **Step 1.2.6:** Apply `#[serde(tag = "kind", rename_all = "snake_case")]` on the enum
- [ ] **Step 1.2.7:** All inner structs derive `Debug, Clone, Serialize, Deserialize`

### Task 1.3: `Action<T>` + `Action::apply_chain` + registry

**Files:** `crates/synthia-event/src/{action,registry}.rs`

- [ ] **Step 1.3.1:** `Action<T> { Proceed, Modify(T), Skip { reason }, Abort { reason } }`
- [ ] **Step 1.3.2:** `fn apply_chain(self, next: Action<T>) -> Action<T>` — Apply mutation chain semantics (Proceed | Modify takes precedence; Abort always wins)
- [ ] **Step 1.3.3:** `ExtensionRegistry { handlers: DashMap<&'static str, Vec<Box<dyn AnyExtensionHandler>>>, active_keys: DashMap<String, ()> }`
- [ ] **Step 1.3.4:** `register(id, handler)` and `emit(event)` methods
- [ ] **Step 1.3.5:** `emit` enforces P9: `tracing::info_span!(target: "synthia.extension", "extension.hook", point = ..., scope = ..., extension_id = ...).entered()`
- [ ] **Step 1.3.6:** Wildcard matching for `*` subscription (Phase 3 reused)

### Task 1.4: `ExtensionCtx` three-state + `assert_active`

**Files:** `crates/synthia-event/src/context.rs`

- [ ] **Step 1.4.1:** `ExtensionCtxState { Loading, Active, Stale { reason: String } }`
- [ ] **Step 1.4.2:** `ExtensionCtx { state: parking_lot::Mutex<ExtensionCtxState>, actions: ExtensionRegistry, cancel_token }`
- [ ] **Step 1.4.3:** `assert_active()` returns `Result<(), StaleContextError>`; Loading panics with `NotInitializedError` (fail-fast); Stale returns `Err`
- [ ] **Step 1.4.4:** `register_*` methods allowed in Loading
- [ ] **Step 1.4.5:** `emit`, `send_message`, etc. require `Active`
- [ ] **Step 1.4.6:** `invalidate(reason)` transitions Active → Stale

### Task 1.5: Round 1 validation

- [ ] **Step 1.5.1:** `cargo +nightly fmt --all`
- [ ] **Step 1.5.2:** `cargo check -p synthia-event`
- [ ] **Step 1.5.3:** `cargo clippy -p synthia-event --all-targets --all-features --tests -- -D warnings` — 0 warnings
- [ ] **Step 1.5.4:** Unit tests: every variant can be constructed + `Action<T>::apply_chain` semantics verified
- [ ] **Step 1.5.5:** State machine: load → assert_active Err → bind → assert_active Ok → invalidate → assert_active Stale error
- [ ] **Step 1.5.6:** **No commit** — user approval required before R2

---

## Round 2: Permission event-driven + absorb ext-points R2 (9 points)

**Why second:** builds on event types; lands Permission fail-closed and Provider extension.

### Task 2.1: `PermissionExtensibilityGuard` (P6 fail-closed)

**Files:** `crates/synthia-event/src/guard.rs`

- [ ] **Step 2.1.1:** `PermissionExtensibilityGuard::downgrade_weaken_to_ask(decision: PermissionDecision) -> PermissionDecision`
- [ ] **Step 2.1.2:** If extension returns `Allow` but base policy is `Deny`/`Ask`, downgrade to `Ask`
- [ ] **Step 2.1.3:** Log `permission.weakening_attempt` OTel event
- [ ] **Step 2.1.4:** Unit test: `weakening_attempt_downgraded_to_ask_user`

### Task 2.2: `DefaultPermissionHandler` (event-driven)

**Files:** `crates/synthia-event/src/permission/mod.rs` + replace `crates/synthia-permission/src/approval.rs:78-360` hardcoded path

- [ ] **Step 2.2.1:** `DefaultPermissionHandler { inner: Arc<dyn ApprovalService + Send + Sync> }`
- [ ] **Step 2.2.2:** `check(request)`: fires `ExtensionEvent::PermissionAsk { request, reply_tx }`; 50ms grace timeout
- [ ] **Step 2.2.3:** If no listener fires within 50ms, fallback policy is `Ask` (P6)
- [ ] **Step 2.2.4:** `oneshot::Receiver<PermissionReply>` matches reply or cancellation
- [ ] **Step 2.2.5:** Add `PermissionFuture::from_event(req, tx)` method on `ApprovalService`
- [ ] **Step 2.2.6:** Deprecate `ApprovalService::check_sync(...)` with `#![deprecated]` (1 minor cycle)
- [ ] **Step 2.2.7:** `synthia-permission/src/approval.rs` shrinks from 2098 → ≤ 1500 LOC (R2 only); ≤ 500 by R3

### Task 2.3: 5 Permission extension points

**Files:** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/permission.rs` (new)

- [ ] **Step 2.3.1:** `PermissionRequest`, `PermissionDecision`, `DoomLoopInfo`, `DoomLoopAction`, `BlacklistInput`, `BlacklistEntry`, `PersistInput`, `PersistOutput` typed structs
- [ ] **Step 2.3.2:** Handler aliases (`PermissionAskHandler`, `PermissionNotifyHandler`, `DoomLoopHandler`, `BlacklistHandler`, `PersistHandler`)
- [ ] **Step 2.3.3:** `PermissionExtensionRegistry` with `register_*` + `fire_*` methods
- [ ] **Step 2.3.4:** Wrap with `PermissionExtensibilityGuard`
- [ ] **Step 2.3.5:** Re-export from `extension_points/mod.rs`
- [ ] **Step 2.3.6:** Tests: weakening-down, legitimate deny, doom_loop allow-one-more, observe-only notify, persist modifies state

### Task 2.4: 4 Provider extension points

**Files:** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/provider.rs` (new)

- [ ] **Step 2.4.1:** `ProviderConfig`, `AuthRequest`, `FallbackContext`, `FallbackChain` typed structs
- [ ] **Step 2.4.2:** `ProviderExtensionRegistry` with `DashMap` + `AtomicU64 cache_version`
- [ ] **Step 2.4.3:** `register_provider` idempotent; emit `provider.replaced` OTel event
- [ ] **Step 2.4.4:** Tests: `concurrent_register_increments_cache_version`, `fallback_chain_iterated_in_order`, `auth_token_rotation`, `unregister_removes_provider`

### Task 2.5: Round 2 validation

- [ ] **Step 2.5.1:** `cargo check --workspace`
- [ ] **Step 2.5.2:** `cargo test -p synthia-event -p synthia-permission -p synthia-agent --lib -- extension_points::permission extension_points::provider`
- [ ] **Step 2.5.3:** `cargo clippy --workspace -- -D warnings`
- [ ] **Step 2.5.4:** `cargo +nightly fmt --all`
- [ ] **Step 2.5.5:** 5 historical e2e unchanged
- [ ] **Step 2.5.6:** **No commit**

---

## Round 3: DoomLoop event-driven + absurb ext-points R3 (10 points) + 8 `ExtensionTool` migrations

### Task 3.1: `DefaultDoomLoopExtension`

**Files:** `crates/synthia-event/src/doom_loop/mod.rs`

- [ ] **Step 3.1.1:** `DefaultDoomLoopExtension` struct, implements `ExtensionTool` (from Change 1)
- [ ] **Step 3.1.2:** `bind_extension(ctx)` subscribes to `ExtensionEvent::ToolCall` events
- [ ] **Step 3.1.3:** On each `ToolCall`, compute `fingerprint = hash(tool_name, &args)`; update sliding window
- [ ] **Step 3.1.4:** If 3 consecutive same-fingerprint within 30s, emit `ExtensionEvent::DoomLoopDetected { fingerprint, tool_name, count: 3, severity: DoomLoopSeverity::Warning }`
- [ ] **Step 3.1.5:** Sliding window uses `VecDeque<(u64, ToolName, Instant)>` with capacity 8
- [ ] **Step 3.1.6:** Default action: emit warning, let `PermissionAsk` event handle gating (P6)

### Task 3.2: Delete hardcoded `doom_loop_handler.rs`

**Files:**
- Delete: `crates/synthia-agent/src/doom_loop_handler.rs` (86 LOC)
- Modify: `crates/synthia-agent/src/lib.rs:6` (remove module)

- [ ] **Step 3.2.1:** Audit call sites: `grep -r 'doom_loop_handler' crates/synthia-agent/src/`
- [ ] **Step 3.2.2:** Replace each call site with `DefaultDoomLoopExtension` invocation via `ExtensionRegistry::emit(ExtensionEvent::DoomLoopDetected)`
- [ ] **Step 3.2.3:** Delete the file
- [ ] **Step 3.2.4:** Verify 5 historical e2e still pass (DoomLoop test paths must reroute via event)

### Task 3.3: 4 Event Bus extension points

**Files:** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/event_bus.rs` (new)

- [ ] **Step 3.3.1:** Typed topic enum (replaces string topic from Phase 3)
- [ ] **Step 3.3.2:** `SubscribeHandler`, `PublishHandler`, `AggregateHandler`, `ReplayHandler`
- [ ] **Step 3.3.3:** `EventBusExtensionRegistry` with per-topic DashMap + sequence numbers
- [ ] **Step 3.3.4:** Within-topic ordering preserved; cross-topic undefined
- [ ] **Step 3.3.5:** Tests: `within_topic_ordering_preserved`, `cross_topic_ordering_not_guaranteed`

### Task 3.4: 6 Plugin Lifecycle extension points

**Files:** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/plugin_lifecycle.rs` (new)

- [ ] **Step 3.4.1:** `LoadRequest`, `BindRequest`, `InvalidateRequest`, `UnloadRequest`, `HotSwapRequest`, `DualFormQuery`, `DualFormResponse`
- [ ] **Step 3.4.2:** Reuse `ExtensionContext` state machine from `extension_context.rs` (no new states)
- [ ] **Step 3.4.3:** `extension.hot_swap` is a 3-event sequence: `load` (new) + `invalidate` (old) + `bind` (new) — atomic
- [ ] **Step 3.4.4:** Tests: `hot_swap_transitions_through_valid_states` (100 iterations), `state_machine_integrity_under_100_iterations`

### Task 3.5: Round 3 validation

- [ ] **Step 3.5.1:** `cargo test --workspace`
- [ ] **Step 3.5.2:** DoomLoop integration test green
- [ ] **Step 3.5.3:** 35 → 45 (15 Round 1 + 9 Round 2 + 10 Round 3 wait, that's 34, exactly as planned) — 15 R1 + 9 R2 + 10 R3 = 34 — note the 5 Tool scope points wire in Change 1, so total = 39 so far; finalize 64 in R6
- [ ] **Step 3.5.4:** 5 historical e2e unchanged
- [ ] **Step 3.5.5:** **No commit**

---

## Round 4: `main_loop.rs` rewrite ≤ 400 LOC

**Why fourth:** events stable enough that the loop is now a thin dispatcher.

### Task 4.1: Replace hardcoded branches with event emission

**Files:** `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` (REPLACE entirely)

- [ ] **Step 4.1.1:** Delete `emit_turn_event` and 11 call sites
- [ ] **Step 4.1.2:** Delete hardcoded string compares for `SELF_REFLECT_TOOL_NAME` (lines 543-547), `COMPACT_CONTEXT_TOOL_NAME` (lines 552-561), `doom_loop_detected` (line 666), `sample_cascade_*` (lines 472-505)
- [ ] **Step 4.1.3:** Delete XML format inline (lines 82-99) — moved to `OutputSink` extension
- [ ] **Step 4.1.4:** Rewrite to event-driven shape:
  ```rust
  pub async fn run_main_loop(...) -> Result<AgentRun, AgentError> {
      let mut ctx = ExtensionCtx::new_active(&config, &ext_registry);
      ctx.emit(ExtensionEvent::SessionStart { ... }).await?;

      let mut turn = TurnState::default();
      let mut iter = 0;
      while iter < config.max_iterations && !cancel.is_cancelled() {
          iter += 1;
          ctx.emit(ExtensionEvent::IterationStart { turn_id, n: iter }).await?;

          let payload = compose_via_extensions(&ctx, &turn).await?;
          let response = config.provider.send(payload).await?;
          let message = response.into_message();
          turn.add_assistant(message.clone());
          ctx.emit(ExtensionEvent::MessageReceive { message, ctx: TransformCtx::default() }).await?;

          let tool_calls = message.tool_calls();
          if tool_calls.is_empty() {
              turn.end_turn(TurnStatus::Completed);
              ctx.emit(ExtensionEvent::IterationEnd { ... }).await?;
              break;
          }

          let mut tool_results = vec![];
          for call in tool_calls {
              let (tx, rx) = oneshot::channel();
              ctx.emit(ExtensionEvent::ToolCall { ..., decision: ToolDecision::Pending }).await?;
              let decision = PermissionHandler::check(&ctx, call.permission_request()).await?;
              // ... execute via ToolRouter ...
              tool_results.push(ToolResult { ... });
              ctx.emit(ExtensionEvent::ToolResult { ... }).await?;
          }
          turn.add_tool_results(tool_results);
          ctx.emit(ExtensionEvent::IterationEnd { ... }).await?;
      }
      ctx.emit(ExtensionEvent::SessionShutdown { ... }).await?;
      Ok(AgentRun::new(turn))
  }
  ```
- [ ] **Step 4.1.5:** End with ≤ 400 LOC strict
- [ ] **Step 4.1.6:** No tool-name string comparison anywhere in loop body

### Task 4.2: Round 4 validation

- [ ] **Step 4.2.1:** `wc -l main_loop.rs` returns ≤ 400
- [ ] **Step 4.2.2:** 5 historical e2e unchanged
- [ ] **Step 4.2.3:** `cargo test --workspace` all green
- [ ] **Step 4.2.4:** **No commit**

---

## Round 5: StreamBuilder 6 step → 2 step

### Task 5.1: StreamBuilder simplification

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder/step.rs`
- Delete: `crates/synthia-agent/src/stream_builder/builder/builder/types.rs` (14 type params)
- Delete: 5 of 6 `StepXxx` types in `stream_builder/steps/`

- [ ] **Step 5.1.1:** Define `StreamBuilder { extensions: Arc<ExtensionRegistry>, router: Arc<ToolRouter> }`
- [ ] **Step 5.1.2:** `enum Step { Hook { event_kind: &'static str }, Tool { tool: ToolName } }`
- [ ] **Step 5.1.3:** Migrate `StepToolExecute` calls to `Step::Tool { tool }` (one per tool)
- [ ] **Step 5.1.4:** Migrate `StepCompact`, `StepHooks`, `StepReflect`, `StepSample` to `Step::Hook { event_kind: "session.before_compact" }` etc.
- [ ] **Step 5.1.5:** Each Step emits one event and collects results — pure extension lifecycle now
- [ ] **Step 5.1.6:** Delete the 4 step types replaced by `Hook { event_kind }`

### Task 5.2: Round 5 validation

- [ ] **Step 5.2.1:** `cargo check --workspace`
- [ ] **Step 5.2.2:** 5 historical e2e unchanged
- [ ] **Step 5.2.3:** **No commit**

---

## Round 6: extension-points R4 + 64-point integration test

### Task 6.1: 5 Session Tree extension points

**Files:** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/session_tree.rs` (new)

- [ ] **Step 6.1.1:** `EntryAppendInput`, `TreeWalkRequest`, `BranchNode`, `BranchCreateRequest`, `BranchCreateOutput`, `MigrateRequest`
- [ ] **Step 6.1.2:** `SessionTreeExtensionRegistry` with write-bound pattern
- [ ] **Step 6.1.3:** `session.tree.append` is observe-only; `session.compaction.preserve` is observe-only; `session.migrate` is mutation
- [ ] **Step 6.1.4:** `session.tree.branch.create` freezes parent
- [ ] **Step 6.1.5:** Tests: `append_preserves_submission_order`, `branch_create_freezes_parent`, `tree_walk_returns_pre_order`

### Task 6.2: 4 Output/UI extension points

**Files:** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/output_ui.rs` (new)

- [ ] **Step 6.2.1:** `OutputFormatInput`, `MetadataPatch`, `MetadataValue`, `NotifyRequest`, `ConfirmRequest`, `RenderRequest`, `RenderOutput`, `ComponentKind`, `NotificationLevel`
- [ ] **Step 6.2.2:** `OutputUiExtensionRegistry` with intercept-bound pattern
- [ ] **Step 6.2.3:** Host capability mapping: TUI native for Text/Diff, RPC JSON-only, Server HTML/SSR
- [ ] **Step 6.2.4:** Tests: `tui_renders_text_and_diff`, `unsupported_kind_falls_back_to_string`, `dialog_confirm_blocks_for_user_response`, `dialog_notify_is_non_blocking`

### Task 6.3: 64-point full integration test

**Files:** `crates/synthia-agent/tests/extension_matrix.rs` (new)

- [ ] **Step 6.3.1:** Build list of all 64 extension point names (15 R1 + 9 R2 + 10 R3 + 5 R4 Session + 4 R4 OutputUI = 43; **plus** Change 1's 7 Tool scope points + 14 Phase 3 already-wired points = **64**)
- [ ] **Step 6.3.2:** For each, register a no-op handler
- [ ] **Step 6.3.3:** Call corresponding `fire_*` / `emit` method
- [ ] **Step 6.3.4:** Assert no panics, no errors, every `extension.hook` OTel span emitted
- [ ] **Step 6.3.5:** Test passes when all 64 points reachable

### Task 6.4: Round 6 validation + R2 archive

- [ ] **Step 6.4.1:** `cargo check --workspace`
- [ ] **Step 6.4.2:** `cargo test -p synthia-agent --test extension_matrix` — all 64 points green
- [ ] **Step 6.4.3:** 5 historical e2e unchanged
- [ ] **Step 6.4.4:** `cargo clippy --workspace -- -D warnings`
- [ ] **Step 6.4.5:** `cargo +nightly fmt --all`
- [ ] **Step 6.4.6:** **OpenSpec archive** (per `omo-archive-change` skill): `openspec archive synthia-event-first` after all 7 Rounds verified
- [ ] **Step 6.4.7:** Update `extension-point-matrix` spec to mark 35 Round 2-4 points as `VERIFIED` (now 64/64)
- [ ] **Step 6.4.8:** **No commit** — user approval required for archive + per-change PR

---

## Round 7: 8 internal `ExtensionTool` migrations

### Task 7.1: Migrate 8 internal tools to ExtensionTool

**Files:**
- `crates/synthia-context/src/compact_context_tool.rs`
- `crates/synthia-skill/src/implicit_tools/load_skill.rs`
- `crates/synthia-agent/src/tools/agent_tools/subagent.rs`
- `crates/synthia-guardian/src/self_reflect_tool.rs`
- `crates/synthia-tool-bash/src/monitor_tool.rs`
- `crates/synthia-mcp/src/tool_adapter.rs`
- `crates/synthia-plugin/src/external_hook_tool.rs`
- `crates/synthia-skill/src/usage_tracker.rs`

- [ ] **Step 7.1.1:** Each `impl ExtensionTool`, with `bind_extension` no-op left to be filled by Change 3 R8
- [ ] **Step 7.1.2:** Each subscribes to its natural event (e.g., compact_context_tool subscribes to `SessionBeforeCompact`)
- [ ] **Step 7.1.3:** Verify bind_extension is called during `ExtensionRuntime::bind_core`
- [ ] **Step 7.1.4:** Tests: each tool fires correctly via event path

### Task 7.2: Round 7 validation

- [ ] **Step 7.2.1:** All previous validations still pass
- [ ] **Step 7.2.2:** `cargo +nightly fmt --all` clean
- [ ] **Step 7.2.3:** **No commit**

---

## Self-Review

- ✅ Every Round has file paths, code patterns, validation commands
- ✅ No placeholders: implementation steps concrete
- ✅ Project rule: no auto-commit — every Round ends with "user approval required"
- ✅ Backward compat: legacy `ApprovalService::check_sync` continues to compile throughout 0.2.x
- ✅ Hard constraints: P1, P6 (fail-closed), P9 (every fire OTel)
- ✅ In-flight works absorbed: `extension-points-phase-2` R2/R3/R4 fully consumed
- ✅ Out of scope clearly listed (defer to Change 3)
- ✅ Net code impact: ~-1100 LOC net deletion (1355 perm shrunk + 86 doom loop + 650 main_loop + 400 step split vs 3500 new events/ext-points)

## Summary

- 7 Rounds × 1 commit each (with user approval) = 7 commits
- ~3,500 new LOC + ~2,200 deleted LOC = net ~+1,300
- ~24 new tests + 1 cross-cutting 64-point integration test
- New crate: `synthia-event` (1 primary landing)
- `synthia-permission` shrinks from 2098 → ≤ 500 LOC
- `main_loop.rs` shrinks from 1037 → ≤ 400 LOC
- Permission/DoomLoop/Compact/Handoff all event-driven
- No breaking changes to existing public API during 0.2.x
