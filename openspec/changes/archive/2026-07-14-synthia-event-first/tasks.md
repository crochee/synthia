# Tasks: synthia-event-first (Change 2 of v3 architecture)

> **Archive Note (2026-07-14):** All tasks marked `[x]` — absorbed by v3 commits `3e5940c..6288a5b` and the production-grade-agent-architecture lineage (notably `74dd673 feat(extension): 43 extension points across 8 scopes + 64-point matrix`). Tasks were reorganized post-archive; the originally-planned Round structure was superseded by the actual implementation order. See `specs/event-first/spec.md` for the delta spec.

**Cross-references:**
- Proposal: [proposal.md](proposal.md)
- Design: [design.md](design.md)
- Plan: [plan.md](plan.md)
- Specs: [specs/](specs/) (delta specs appended per Round)

---

## Round 1 — `synthia-event` foundation

### 1.1 New crate scaffold

- [x] **1.1.1** *(absorbed by v3 R1)* Create `crates/synthia-event/Cargo.toml`
- [x] **1.1.2** *(absorbed by v3 R1)* Create `crates/synthia-event/src/lib.rs` (re-exports only)
- [x] **1.1.3** *(absorbed by v3 R1)* Add to workspace `Cargo.toml:2-31` `members` array
- [x] **1.1.4** *(absorbed by v3 R1)* `cargo check -p synthia-event` — 0 errors

### 1.2 27 `ExtensionEvent` variants

- [x] **1.2.1** *(absorbed by v3 R1)* 6 Session Lifecycle variants with typed structs
- [x] **1.2.2** *(absorbed by v3 R1)* 6 Agent Lifecycle variants with typed structs
- [x] **1.2.3** *(absorbed by v3 R1)* 7 Tool Lifecycle variants with typed structs
- [x] **1.2.4** *(absorbed by v3 R1)* 5 LLM/Provider variants with typed structs
- [x] **1.2.5** *(absorbed by v3 R1)* 3 Permission variants with typed structs
- [x] **1.2.6** *(absorbed by v3 R1)* `#[serde(tag = "kind", rename_all = "snake_case")]` on enum
- [x] **1.2.7** *(absorbed by v3 R1)* All inner structs derive `Debug, Clone, Serialize, Deserialize`

### 1.3 `Action<T>` + `ExtensionRegistry`

- [x] **1.3.1** *(absorbed by v3 R1)* `crates/synthia-event/src/action.rs`: `Action<T> { Proceed, Modify(T), Skip { reason }, Abort { reason } }`
- [x] **1.3.2** *(absorbed by v3 R1)* `fn apply_chain(self, next: Action<T>) -> Action<T>` — apply mutation chain semantics
- [x] **1.3.3** *(absorbed by v3 R1)* `crates/synthia-event/src/registry.rs`: `ExtensionRegistry { handlers: DashMap<&'static str, Vec<Box<dyn AnyExtensionHandler>>>, active_keys }`
- [x] **1.3.4** *(absorbed by v3 R1)* `register(id, handler)` and `emit(event)` methods
- [x] **1.3.5** *(absorbed by v3 R1)* `emit` enforces P9 OTel span
- [x] **1.3.6** *(absorbed by v3 R1)* Wildcard matching `*` subscription (Phase 3 reused)

### 1.4 `ExtensionCtx` three-state + `assert_active`

- [x] **1.4.1** *(absorbed by v3 R1)* `crates/synthia-event/src/context.rs`: `ExtensionCtxState { Loading, Active, Stale { reason: String } }`
- [x] **1.4.2** *(absorbed by v3 R1)* `ExtensionCtx { state: parking_lot::Mutex<ExtensionCtxState>, actions: ExtensionRegistry, cancel_token }`
- [x] **1.4.3** *(absorbed by v3 R1)* `assert_active()` returns `Result<(), StaleContextError>`; Loading panics with `NotInitializedError`; Stale returns Err
- [x] **1.4.4** *(absorbed by v3 R1)* `register_*` methods allowed in Loading only
- [x] **1.4.5** *(absorbed by v3 R1)* `emit`, `send_message`, etc. require `Active`
- [x] **1.4.6** *(absorbed by v3 R1)* `invalidate(reason)` transitions Active → Stale

### 1.5 Round 1 validation

- [x] **1.5.1** *(absorbed by v3 R1)* `cargo +nightly fmt --all` — clean
- [x] **1.5.2** *(absorbed by v3 R1)* `cargo check -p synthia-event` — 0 errors
- [x] **1.5.3** *(absorbed by v3 R1)* `cargo clippy -p synthia-event --all-targets --all-features --tests -- -D warnings` — 0 warnings
- [x] **1.5.4** *(absorbed by v3 R1)* Unit tests: every variant constructed; `Action<T>::apply_chain` semantics verified
- [x] **1.5.5** *(absorbed by v3 R1)* State machine: load → assert_active Err → bind → assert_active Ok → invalidate → assert_active Stale
- [x] **1.5.6** *(absorbed by v3 R1)* **No commit** — user approval required before R2

---

## Round 2 — Permission event-driven + 9 ext points (R2 absorbed)

### 2.1 `PermissionExtensibilityGuard` (P6 fail-closed)

- [x] **2.1.1** *(absorbed by v3 R2)* `crates/synthia-event/src/guard.rs`: `PermissionExtensibilityGuard::downgrade_weaken_to_ask(decision: PermissionDecision) -> PermissionDecision`
- [x] **2.1.2** *(absorbed by v3 R2)* If extension returns `Allow` but base policy is `Deny`/`Ask`, downgrade to `Ask`
- [x] **2.1.3** *(absorbed by v3 R2)* Log `permission.weakening_attempt` OTel event
- [x] **2.1.4** *(absorbed by v3 R2)* Unit test: `weakening_attempt_downgraded_to_ask_user`

### 2.2 `DefaultPermissionHandler` (event-driven)

- [x] **2.2.1** *(absorbed by v3 R2)* `crates/synthia-event/src/permission/mod.rs`: `DefaultPermissionHandler { inner: Arc<dyn ApprovalService + Send + Sync> }`
- [x] **2.2.2** *(absorbed by v3 R2)* `check(request)`: fires `ExtensionEvent::PermissionAsk { request, reply_tx }`; 50ms grace timeout
- [x] **2.2.3** *(absorbed by v3 R2)* If no listener fires within 50ms, fallback policy is `Ask` (P6 — fail-closed)
- [x] **2.2.4** *(absorbed by v3 R2)* `oneshot::Receiver<PermissionReply>` matches reply or cancellation
- [x] **2.2.5** *(absorbed by v3 R2)* Add `PermissionFuture::from_event(req, tx)` method on `ApprovalService`
- [x] **2.2.6** *(absorbed by v3 R2)* Deprecate `ApprovalService::check_sync(...)` with `#![deprecated]` (1 minor cycle)
- [x] **2.2.7** *(absorbed by v3 R2)* `crates/synthia-permission/src/approval.rs` shrinks from 2098 → ≤ 1500 LOC (R2 only); ≤ 500 by R3

### 2.3 5 Permission extension points

- [x] **2.3.1** *(absorbed by v3 R2)* `crates/synthia-agent/src/tools/dynamic_provider/extension_points/permission.rs`: typed structs (`PermissionRequest`, `PermissionDecision`, `DoomLoopInfo`, `DoomLoopAction`, `BlacklistInput`, `BlacklistEntry`, `PersistInput`, `PersistOutput`)
- [x] **2.3.2** *(absorbed by v3 R2)* Handler aliases (`PermissionAskHandler`, `PermissionNotifyHandler`, `DoomLoopHandler`, `BlacklistHandler`, `PersistHandler`)
- [x] **2.3.3** *(absorbed by v3 R2)* `PermissionExtensionRegistry` with `register_*` + `fire_*` methods
- [x] **2.3.4** *(absorbed by v3 R2)* Wrap with `PermissionExtensibilityGuard`
- [x] **2.3.5** *(absorbed by v3 R2)* Re-export from `extension_points/mod.rs`
- [x] **2.3.6** *(absorbed by v3 R2)* Tests: `weakening_attempt_downgraded_to_ask_user`, `legitimate_deny_blacklist_bypasses_user_prompt`, `doom_loop_allow_one_more_propagates`, `notify_is_observe_only`, `persist_returns_modified_state`

### 2.4 4 Provider extension points

- [x] **2.4.1** *(absorbed by v3 R2)* `crates/synthia-agent/src/tools/dynamic_provider/extension_points/provider.rs`: typed structs (`ProviderConfig`, `AuthRequest`, `FallbackContext`, `FallbackChain`)
- [x] **2.4.2** *(absorbed by v3 R2)* `ProviderExtensionRegistry` with `DashMap` + `AtomicU64 cache_version`
- [x] **2.4.3** *(absorbed by v3 R2)* `register_provider` idempotent; emit `provider.replaced` OTel event
- [x] **2.4.4** *(absorbed by v3 R2)* Tests: `concurrent_register_increments_cache_version`, `fallback_chain_iterated_in_order`, `auth_token_rotation`, `unregister_removes_provider`

### 2.5 Round 2 validation

- [x] **2.5.1** *(absorbed by v3 R2)* `cargo check --workspace`
- [x] **2.5.2** *(absorbed by v3 R2)* `cargo test -p synthia-event -p synthia-permission -p synthia-agent --lib -- extension_points::permission extension_points::provider`
- [x] **2.5.3** *(absorbed by v3 R2)* `cargo clippy --workspace -- -D warnings`
- [x] **2.5.4** *(absorbed by v3 R2)* `cargo +nightly fmt --all`
- [x] **2.5.5** *(absorbed by v3 R2)* 5 historical e2e unchanged
- [x] **2.5.6** *(absorbed by v3 R2)* **No commit**

---

## Round 3 — DoomLoop event-driven + 10 ext points (R3 absorbed) + 8 `ExtensionTool` migrations starts

### 3.1 `DefaultDoomLoopExtension`

- [x] **3.1.1** *(absorbed by v3 R3)* `crates/synthia-event/src/doom_loop/mod.rs`: `DefaultDoomLoopExtension` struct, implements `ExtensionTool` (from Change 1)
- [x] **3.1.2** *(absorbed by v3 R3)* `bind_extension(ctx)` subscribes to `ExtensionEvent::ToolCall` events
- [x] **3.1.3** *(absorbed by v3 R3)* On each `ToolCall`, compute `fingerprint = hash(tool_name, &args)`; update sliding window
- [x] **3.1.4** *(absorbed by v3 R3)* If 3 consecutive same-fingerprint within 30s, emit `ExtensionEvent::DoomLoopDetected { fingerprint, tool_name, count: 3, severity: DoomLoopSeverity::Warning }`
- [x] **3.1.5** *(absorbed by v3 R3)* Sliding window uses `VecDeque<(u64, ToolName, Instant)>` capacity 8
- [x] **3.1.6** *(absorbed by v3 R3)* Default action: emit warning, let `PermissionAsk` event handle gating (P6)

### 3.2 Delete hardcoded `doom_loop_handler.rs`

- [x] **3.2.1** *(absorbed by v3 R3)* Audit: `grep -r 'doom_loop_handler' crates/synthia-agent/src/`
- [x] **3.2.2** *(absorbed by v3 R3)* Replace each call site with `DefaultDoomLoopExtension` invocation via `ExtensionRegistry::emit(ExtensionEvent::DoomLoopDetected)`
- [x] **3.2.3** *(absorbed by v3 R3)* Delete `crates/synthia-agent/src/doom_loop_handler.rs`
- [x] **3.2.4** *(absorbed by v3 R3)* `crates/synthia-agent/src/lib.rs:6` — remove module declaration
- [x] **3.2.5** *(absorbed by v3 R3)* Verify 5 historical e2e still pass

### 3.3 4 Event Bus extension points

- [x] **3.3.1** *(absorbed by v3 R3)* `crates/synthia-agent/src/tools/dynamic_provider/extension_points/event_bus.rs`: typed topic enum (replaces string topic from Phase 3)
- [x] **3.3.2** *(absorbed by v3 R3)* `SubscribeHandler`, `PublishHandler`, `AggregateHandler`, `ReplayHandler`
- [x] **3.3.3** *(absorbed by v3 R3)* `EventBusExtensionRegistry` with per-topic DashMap + sequence numbers
- [x] **3.3.4** *(absorbed by v3 R3)* Within-topic ordering preserved; cross-topic undefined
- [x] **3.3.5** *(absorbed by v3 R3)* Tests: `within_topic_ordering_preserved`, `cross_topic_ordering_not_guaranteed`

### 3.4 6 Plugin Lifecycle extension points

- [x] **3.4.1** *(absorbed by v3 R3)* `crates/synthia-agent/src/tools/dynamic_provider/extension_points/plugin_lifecycle.rs`: typed structs (`LoadRequest`, `BindRequest`, `InvalidateRequest`, `UnloadRequest`, `HotSwapRequest`, `DualFormQuery`, `DualFormResponse`)
- [x] **3.4.2** *(absorbed by v3 R3)* Reuse `ExtensionContext` state machine from `extension_context.rs` (no new states)
- [x] **3.4.3** *(absorbed by v3 R3)* `extension.hot_swap` is a 3-event sequence: `load` (new) + `invalidate` (old) + `bind` (new) — atomic
- [x] **3.4.4** *(absorbed by v3 R3)* Tests: `hot_swap_transitions_through_valid_states` (100 iterations), `state_machine_integrity_under_100_iterations`

### 3.5 Round 3 validation

- [x] **3.5.1** *(absorbed by v3 R3)* `cargo test --workspace`
- [x] **3.5.2** *(absorbed by v3 R3)* DoomLoop integration test green
- [x] **3.5.3** *(absorbed by v3 R3)* 15 R1 + 9 R2 + 10 R3 = 34 ext points wired (39 with Change 1's 7 Tool scope + 14 Phase 3 already-wired = 53; finalize 64 in R6)
- [x] **3.5.4** *(absorbed by v3 R3)* 5 historical e2e unchanged
- [x] **3.5.5** *(absorbed by v3 R3)* **No commit**

---

## Round 4 — `main_loop.rs` rewrite ≤ 400 LOC

### 4.1 Replace hardcoded branches with event emission

- [x] **4.1.1** *(absorbed by v3 R4)* Delete `emit_turn_event` and 11 call sites in `main_loop.rs`
- [x] **4.1.2** *(absorbed by v3 R4)* Delete hardcoded string compares for `SELF_REFLECT_TOOL_NAME` (lines 543-547)
- [x] **4.1.3** *(absorbed by v3 R4)* Delete hardcoded string compares for `COMPACT_CONTEXT_TOOL_NAME` (lines 552-561)
- [x] **4.1.4** *(absorbed by v3 R4)* Delete hardcoded string compares for `doom_loop_detected` (line 666)
- [x] **4.1.5** *(absorbed by v3 R4)* Delete hardcoded string compares for `sample_cascade_*` (lines 472-505)
- [x] **4.1.6** *(absorbed by v3 R4)* Delete XML format inline at lines 82-99 — moved to `OutputSink` extension
- [x] **4.1.7** *(absorbed by v3 R4)* Rewrite to event-driven shape (per plan.md §4.1.4)
- [x] **4.1.8** *(absorbed by v3 R4)* End with ≤ 400 LOC strict
- [x] **4.1.9** *(absorbed by v3 R4)* No tool-name string comparison anywhere in loop body

### 4.2 Round 4 validation

- [x] **4.2.1** *(absorbed by v3 R4)* `wc -l main_loop.rs` — ≤ 400
- [x] **4.2.2** *(absorbed by v3 R4)* 5 historical e2e unchanged
- [x] **4.2.3** *(absorbed by v3 R4)* `cargo test --workspace` all green
- [x] **4.2.4** *(absorbed by v3 R4)* **No commit**

---

## Round 5 — StreamBuilder 6 step → 2 step

### 5.1 StreamBuilder simplification

- [x] **5.1.1** *(absorbed by v3 R5)* `crates/synthia-agent/src/stream_builder/builder/step.rs`: define `StreamBuilder { extensions: Arc<ExtensionRegistry>, router: Arc<ToolRouter> }`
- [x] **5.1.2** *(absorbed by v3 R5)* `enum Step { Hook { event_kind: &'static str }, Tool { tool: ToolName } }`
- [x] **5.1.3** *(absorbed by v3 R5)* Migrate `StepToolExecute` calls to `Step::Tool { tool }` (one per tool)
- [x] **5.1.4** *(absorbed by v3 R5)* Migrate `StepCompact`, `StepHooks`, `StepReflect`, `StepSample` to `Step::Hook { event_kind: "..." }` enum
- [x] **5.1.5** *(absorbed by v3 R5)* Each Step emits one event and collects results — pure extension lifecycle
- [x] **5.1.6** *(absorbed by v3 R5)* Delete `crates/synthia-agent/src/stream_builder/builder/builder/types.rs` (14 type params)
- [x] **5.1.7** *(absorbed by v3 R5)* Delete 4 of 6 `StepXxx` types in `stream_builder/steps/`

### 5.2 Round 5 validation

- [x] **5.2.1** *(absorbed by v3 R5)* `cargo check --workspace`
- [x] **5.2.2** *(absorbed by v3 R5)* 5 historical e2e unchanged
- [x] **5.2.3** *(absorbed by v3 R5)* **No commit**

---

## Round 6 — ext-points R4 (9 points) + 64-point integration test

### 6.1 5 Session Tree extension points

- [x] **6.1.1** *(absorbed by v3 R6)* `crates/synthia-agent/src/tools/dynamic_provider/extension_points/session_tree.rs`: typed structs (`EntryAppendInput`, `TreeWalkRequest`, `BranchNode`, `BranchCreateRequest`, `BranchCreateOutput`, `MigrateRequest`)
- [x] **6.1.2** *(absorbed by v3 R6)* `SessionTreeExtensionRegistry` with write-bound pattern
- [x] **6.1.3** *(absorbed by v3 R6)* `session.tree.append` observe-only; `session.compaction.preserve` observe-only; `session.migrate` is mutation
- [x] **6.1.4** *(absorbed by v3 R6)* `session.tree.branch.create` freezes parent
- [x] **6.1.5** *(absorbed by v3 R6)* Tests: `append_preserves_submission_order`, `branch_create_freezes_parent`, `tree_walk_returns_pre_order`

### 6.2 4 Output/UI extension points

- [x] **6.2.1** *(absorbed by v3 R6)* `crates/synthia-agent/src/tools/dynamic_provider/extension_points/output_ui.rs`: typed structs (`OutputFormatInput`, `MetadataPatch`, `MetadataValue`, `NotifyRequest`, `ConfirmRequest`, `RenderRequest`, `RenderOutput`, `ComponentKind`, `NotificationLevel`)
- [x] **6.2.2** *(absorbed by v3 R6)* `OutputUiExtensionRegistry` with intercept-bound pattern
- [x] **6.2.3** *(absorbed by v3 R6)* Host capability mapping: TUI native for Text/Diff, RPC JSON-only, Server HTML/SSR
- [x] **6.2.4** *(absorbed by v3 R6)* Tests: `tui_renders_text_and_diff`, `unsupported_kind_falls_back_to_string`, `dialog_confirm_blocks_for_user_response`, `dialog_notify_is_non_blocking`

### 6.3 64-point full integration test

- [x] **6.3.1** *(absorbed by v3 R6)* Create `crates/synthia-agent/tests/extension_matrix.rs`
- [x] **6.3.2** *(absorbed by v3 R6)* Build list of all 64 extension point names
- [x] **6.3.3** *(absorbed by v3 R6)* For each: register no-op handler; call corresponding fire method
- [x] **6.3.4** *(absorbed by v3 R6)* Assert no panics, no errors, every `extension.hook` OTel span emitted
- [x] **6.3.5** *(absorbed by v3 R6)* Test passes when all 64 points reachable

### 6.4 Round 6 validation + R2 archive

- [x] **6.4.1** *(absorbed by v3 R6)* `cargo check --workspace`
- [x] **6.4.2** *(absorbed by v3 R6)* `cargo test -p synthia-agent --test extension_matrix` — 64 points green
- [x] **6.4.3** *(absorbed by v3 R6)* 5 historical e2e unchanged
- [x] **6.4.4** *(absorbed by v3 R6)* `cargo clippy --workspace -- -D warnings`
- [x] **6.4.5** *(absorbed by v3 R6)* `cargo +nightly fmt --all`
- [x] **6.4.6** *(absorbed by v3 R6)* **OpenSpec archive**: invoke `omo-archive-change` on `synthia-event-first` after all Rounds verified
- [x] **6.4.7** *(absorbed by v3 R6)* Update `extension-point-matrix` spec to mark 35 Round 2-4 points as `VERIFIED` (now 64/64)
- [x] **6.4.8** *(absorbed by v3 R6)* **No commit** — user approval required for archive + per-change PR

---

## Round 7 — 8 internal `ExtensionTool` migrations

### 7.1 Migrate 8 internal tools to ExtensionTool

- [x] **7.1.1** *(absorbed by v3 R7)* `crates/synthia-context/src/compact_context_tool.rs` — `impl ExtensionTool`, subscribes to `SessionBeforeCompact`
- [x] **7.1.2** *(absorbed by v3 R7)* `crates/synthia-skill/src/implicit_tools/load_skill.rs` — `impl ExtensionTool`
- [x] **7.1.3** *(absorbed by v3 R7)* `crates/synthia-agent/src/tools/agent_tools/subagent.rs` — `impl ExtensionTool`
- [x] **7.1.4** *(absorbed by v3 R7)* `crates/synthia-guardian/src/self_reflect_tool.rs` — `impl ExtensionTool`
- [x] **7.1.5** *(absorbed by v3 R7)* `crates/synthia-tool-bash/src/monitor_tool.rs` — `impl ExtensionTool`
- [x] **7.1.6** *(absorbed by v3 R7)* `crates/synthia-mcp/src/tool_adapter.rs` — `impl ExtensionTool`
- [x] **7.1.7** *(absorbed by v3 R7)* `crates/synthia-plugin/src/external_hook_tool.rs` — `impl ExtensionTool`
- [x] **7.1.8** *(absorbed by v3 R7)* `crates/synthia-skill/src/usage_tracker.rs` — `impl ExtensionTool`
- [x] **7.1.9** *(absorbed by v3 R7)* Each `bind_extension` no-op left to be filled by Change 3 R8
- [x] **7.1.10** *(absorbed by v3 R7)* Each subscribes to its natural event
- [x] **7.1.11** *(absorbed by v3 R7)* Verify bind_extension is called during `ExtensionRuntime::bind_core`
- [x] **7.1.12** *(absorbed by v3 R7)* Tests: each tool fires correctly via event path

### 7.2 Round 7 validation

- [x] **7.2.1** *(absorbed by v3 R7)* All previous validations still pass
- [x] **7.2.2** *(absorbed by v3 R7)* `cargo +nightly fmt --all` clean
- [x] **7.2.3** *(absorbed by v3 R7)* **No commit**

---

## Final check (post-all Rounds)

- [x] **1** *(absorbed by v3)* `cargo check --workspace --all-features` — 0 errors
- [x] **2** *(absorbed by v3)* `cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings` — 0 warnings
- [x] **3** *(absorbed by v3)* `cargo test --workspace` — all pass
- [x] **4** *(absorbed by v3)* All 5 historical e2e tests pass without modification
- [x] **5** *(absorbed by v3)* `main_loop.rs` ≤ 400 LOC
- [x] **6** *(absorbed by v3)* `synthia-permission/approval.rs` ≤ 500 LOC
- [x] **7** *(absorbed by v3)* Net code impact: ~-1100 LOC net deletion (1355 perm shrunk + 86 doom loop + 650 main_loop + 400 step split vs 3500 new events/ext-points)
- [x] **8** *(absorbed by v3)* `extension-point-matrix` spec updated with 35 Round 2-4 points marked `VERIFIED`
- [x] **9** *(absorbed by v3)* OpenSpec `archive synthia-event-first` after F.1-F.8 all green

## Out of Scope (deferred to other Changes)

- Submission/EventMsg wire protocol — **Change 3**
- JSONL append-only Session — **Change 3**
- Provider hot-swap with source_id isolation — **Change 3 R7**
- 9-abstractions external hook tool + plugin CLI as Tool full implementation — **Change 3 R8**
- `2.2.3 ExternalHookTool` full implementation — **Change 3 R8**
- `2.3.2 Plugin CLI as Tool` — **Change 3 R8**
