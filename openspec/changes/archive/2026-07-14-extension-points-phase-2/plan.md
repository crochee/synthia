# Extension Points Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development (recommended) or executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the remaining 43 extension points across 8 scopes (LLM, Context, Permission, Provider, Plugin Lifecycle, Event Bus, Session Tree, Output/UI) to complete the 64-point extension matrix specified by `extension-point-matrix/spec.md`.

**Architecture:** Reuse the Phase 3 patterns: one module per scope, typed event structs (no `serde_json::Value` for inputs), DashMap-backed registries with `tracing::info_span!` OTel emission, and `Action<T>` mutation pattern (or observe-only `Fn(&Event)`). 4 implementation rounds, each = 1 logical commit, additive only — no modifications to existing call sites.

**Tech Stack:** Rust (tokio async, DashMap, tracing, serde, schemars). Same crates as Phase 3 (`synthia-agent::tools::dynamic_provider::extension_points`).

**Reference docs:**
- Tasks: [tasks.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/tasks.md)
- Design: [design.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/design.md)
- Specs: [specs/](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/specs/) (8 files)
- Existing Phase 3 patterns: [agent_loop.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/dynamic_provider/extension_points/agent_loop.rs), [tool.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs)

**Validation standard:** `cargo check --workspace` + `cargo clippy -p synthia-agent --lib --all-targets --all-features --tests` + `cargo +nightly fmt --all` after every task. Zero new warnings.

---

## Status — 2026-07-12

| Round | Scopes | Points | Status |
|-------|--------|--------|--------|
| 1 | Context + LLM | 15 | ✅ Done (8 + 11 tests pass) |
| 2 | Permission + Provider | 9 | Pending |
| 3 | Event Bus + Plugin Lifecycle | 10 | Pending |
| 4 | Session Tree + Output/UI | 9 + 64-point integration test | Pending |

---

## Round 1: Context + LLM (DONE) — Reference for Rounds 2-4

Round 1 is complete. The implementation pattern is established and serves as the template for Rounds 2-4. Code paths and types are documented below for the next implementer to mirror.

### File Structure (Round 1 deliverables)

- Created: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/llm.rs` (~770 lines, 8 typed points + tests)
- Created: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/context.rs` (~660 lines, 7 typed points + tests)
- Modified: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs` (re-exports)

### Round 1 Pattern (Apply to Rounds 2-4)

Each scope module follows this template:

```rust
// 1. Typed event structs (no serde_json::Value for inputs except for
//    message-list payloads, which pass through the Tool API's JSON contract)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XxxInput { ... }

// 2. Handler alias
pub type XxxHandler = Arc<dyn Fn(&XxxInput) -> Action<XxxInput> + Send + Sync>;

// 3. Registry
pub struct XxxExtensionRegistry {
    handlers: DashMap<String, Vec<XxxHandler>>,
    active_keys: DashMap<String, ()>,
}

impl XxxExtensionRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&self, id: impl Into<String>, handler: XxxHandler) { ... }
    pub fn has_handlers(&self, point: &str) -> bool { ... }
    pub fn fire(&self, event: XxxInput) -> Action<XxxInput> {
        // Emit OTel span per handler
        let _span = tracing::info_span!(
            target: "synthia.extension",
            "extension.hook",
            point = "...",
            scope = "...",
            extension_id = extension_id.as_str(),
        ).entered();
        // Dispatch chain (Proceed | Modify | Skip)
    }
}

#[cfg(test)] mod tests { ... }
```

### Round 1 Test Inventory (Reference)

8 LLM tests + 11 Context tests = 19 total. Includes:
- `new_registry_is_empty` for every registry
- `concurrent_register_does_not_lose_handlers` (DashMap thread-safety)
- `*_modification_reflected_in_request` (mutation pattern)
- `deterministic_transform_preserves_hash` (P1 verification)
- `non_deterministic_transform_is_detected_by_caller` (P1 warning)
- `skip_short_circuits_the_chain` (Action::Skip semantics)

### Round 1 Validation Commands

```bash
cargo check --workspace
cargo test -p synthia-agent --lib -- extension_points::llm extension_points::context
cargo clippy -p synthia-agent --lib --all-targets --all-features --tests
cargo +nightly fmt --all
```

All pass. 45/45 extension_points tests pass (26 original + 19 new).

---

## Round 2: Permission + Provider (9 points)

### Task 2.1: Module structure for Permission

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/permission.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs` (re-exports)

- [ ] **Step 2.1.1:** Define typed event structs (`PermissionRequest`, `PermissionDecision`, `DoomLoopInfo`, `DoomLoopAction`, `BlacklistInput`, `BlacklistEntry`, `PersistInput`, `PersistOutput`)
- [ ] **Step 2.1.2:** Define handler aliases (`PermissionAskHandler`, `PermissionNotifyHandler`, `DoomLoopHandler`, `BlacklistHandler`, `PersistHandler`)
- [ ] **Step 2.1.3:** Implement `PermissionExtensibilityGuard` wrapper that downgrades any "weakening" attempt to `AskUser` (P6 fail-closed)
- [ ] **Step 2.1.4:** Implement `PermissionExtensionRegistry` with `register_*` + `fire_*` methods (5 points)
- [ ] **Step 2.1.5:** Every `fire` emits `extension.hook` OTel span + `permission.weakening_attempt` event on guard downgrade
- [ ] **Step 2.1.6:** Re-export from `mod.rs`

### Task 2.2: Permission tests

- [ ] **Step 2.2.1:** `weakening_attempt_downgraded_to_ask_user` — register handler that returns `Allow` for a `Deny` decision; assert final is `AskUser`
- [ ] **Step 2.2.2:** `legitimate_deny_blacklist_bypasses_user_prompt` — register `blacklist.match` handler returning `Some(BlacklistEntry { verdict: Deny, ... })`; assert no user prompt
- [ ] **Step 2.2.3:** `doom_loop_allow_one_more_propagates` — `DoomLoopAction::AllowOneMore` reaches caller
- [ ] **Step 2.2.4:** `notify_is_observe_only` — `permission.notify` handler runs but never mutates decision
- [ ] **Step 2.2.5:** `persist_returns_modified_state` — `permission.persist` handler can modify `PersistOutput`

### Task 2.3: Module structure for Provider

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/provider.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs`

- [ ] **Step 2.3.1:** Define typed structs (`ProviderConfig`, `AuthRequest`, `FallbackContext`, `FallbackChain`)
- [ ] **Step 2.3.2:** Define handler aliases (4 handlers)
- [ ] **Step 2.3.3:** Implement `ProviderExtensionRegistry` with `DashMap` + `AtomicU64 cache_version` (mirroring `ExtensionManager` pattern)
- [ ] **Step 2.3.4:** `register_provider` is idempotent — re-registering replaces; emit `provider.replaced` OTel event
- [ ] **Step 2.3.5:** Re-export from `mod.rs`

### Task 2.4: Provider tests

- [ ] **Step 2.4.1:** `concurrent_register_increments_cache_version` — 2 threads register; cache version increments atomically
- [ ] **Step 2.4.2:** `fallback_chain_iterated_in_order` — register `provider.fallback` returning `["primary", "secondary", "tertiary"]`; assert order preserved
- [ ] **Step 2.4.3:** `auth_token_rotation` — `provider.auth` handler rotates token; modified token is in actual request
- [ ] **Step 2.4.4:** `unregister_removes_provider` — `provider.unregister("foo")` makes subsequent `provider.resolve("foo")` return None

### Task 2.5: Round 2 validation

- [ ] **Step 2.5.1:** `cargo check --workspace` → 0 errors
- [ ] **Step 2.5.2:** `cargo test -p synthia-agent --lib -- extension_points::permission extension_points::provider` → all pass
- [ ] **Step 2.5.3:** `cargo clippy -p synthia-agent --lib --all-targets --all-features --tests` → 0 new warnings
- [ ] **Step 2.5.4:** `cargo +nightly fmt --all` → clean
- [ ] **Step 2.5.5:** Mark all Round 2 tasks complete in [tasks.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/tasks.md)
- [ ] **Step 2.5.6:** Commit: `feat(extension): 9 Permission + Provider extension points (fail-closed)`（不自动 commit，等用户明确指示）

---

## Round 3: Event Bus + Plugin Lifecycle (10 points)

### Task 3.1: Module structure for Event Bus

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/event_bus.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs`

- [ ] **Step 3.1.1:** Define typed topic enum (replaces string topic in Phase 3) and `EventHandler` trait
- [ ] **Step 3.1.2:** Define handler aliases (`SubscribeHandler`, `PublishHandler`, `AggregateHandler`, `ReplayHandler`)
- [ ] **Step 3.1.3:** Implement `EventBusExtensionRegistry` with per-topic DashMap + sequence numbers
- [ ] **Step 3.1.4:** `event.publish` invokes handlers in registration order within a topic (NOT cross-topic)
- [ ] **Step 3.1.5:** `event.replay` returns events tagged with `replay=true` in OTel attributes
- [ ] **Step 3.1.6:** Re-export from `mod.rs`

### Task 3.2: Event Bus tests

- [ ] **Step 3.2.1:** `within_topic_ordering_preserved` — 2 handlers on topic T; `publish(T, payload)` invokes h1 before h2
- [ ] **Step 3.2.2:** `cross_topic_ordering_not_guaranteed` — handler subscribed to T1 + T2; concurrent `publish` may receive in any order

### Task 3.3: Module structure for Plugin Lifecycle

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/plugin_lifecycle.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs`

- [ ] **Step 3.3.1:** Define typed structs (`LoadRequest`, `BindRequest`, `InvalidateRequest`, `UnloadRequest`, `HotSwapRequest`, `DualFormQuery`, `DualFormResponse`)
- [ ] **Step 3.3.2:** Define handler aliases (6 handlers)
- [ ] **Step 3.3.3:** Reuse `ExtensionContext` state machine from `extension_context.rs` (Phase 3.1) — NO new states
- [ ] **Step 3.3.4:** `extension.hot_swap` is a 3-event sequence: `load` (new) + `invalidate` (old) + `bind` (new)
- [ ] **Step 3.3.5:** Re-export from `mod.rs`

### Task 3.4: Plugin Lifecycle tests

- [ ] **Step 3.4.1:** `hot_swap_transitions_through_valid_states` — fire 100 hot_swaps; assert ExtensionContext never enters invalid state
- [ ] **Step 3.4.2:** `state_machine_integrity_under_100_iterations` — load → bind → invalidate → unload cycle 100 times

### Task 3.5: Round 3 validation

- [ ] **Step 3.5.1:** `cargo check --workspace` → 0 errors
- [ ] **Step 3.5.2:** `cargo test -p synthia-agent --lib -- extension_points::event_bus extension_points::plugin_lifecycle` → all pass
- [ ] **Step 3.5.3:** `cargo clippy -p synthia-agent --lib --all-targets --all-features --tests` → 0 new warnings
- [ ] **Step 3.5.4:** `cargo +nightly fmt --all` → clean
- [ ] **Step 3.5.5:** Mark all Round 3 tasks complete in [tasks.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/tasks.md)
- [ ] **Step 3.5.6:** Commit: `feat(extension): 10 Event Bus + Plugin Lifecycle extension points`（不自动 commit，等用户明确指示）

---

## Round 4: Session Tree + Output/UI (9 points + 64-point integration)

### Task 4.1: Module structure for Session Tree

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/session_tree.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs`

- [ ] **Step 4.1.1:** Define typed structs (`EntryAppendInput`, `TreeWalkRequest`, `BranchNode`, `BranchCreateRequest`, `BranchCreateOutput`, `MigrateRequest`)
- [ ] **Step 4.1.2:** Define handler aliases (5 handlers)
- [ ] **Step 4.1.3:** Implement `SessionTreeExtensionRegistry` with write-bound pattern (most points append/mutate entries)
- [ ] **Step 4.1.4:** `session.branch.create` freezes parent (subsequent appends to parent return `BranchFrozenError`)
- [ ] **Step 4.1.5:** `session.compaction.preserve` is observe-only — preserve `from_hook=true` summaries per `pi-mono session-manager.ts:48-61`
- [ ] **Step 4.1.6:** Re-export from `mod.rs`

### Task 4.2: Session Tree tests

- [ ] **Step 4.2.1:** `append_preserves_submission_order` — submit 3 entries; assert persisted in order
- [ ] **Step 4.2.2:** `branch_create_freezes_parent` — after branch create, parent's `append` returns `BranchFrozenError`
- [ ] **Step 4.2.3:** `tree_walk_returns_pre_order` — depth-first traversal in insertion order

### Task 4.3: Module structure for Output/UI

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/output_ui.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs`

- [ ] **Step 4.3.1:** Define typed structs (`OutputFormatInput`, `MetadataPatch`, `MetadataValue`, `NotifyRequest`, `ConfirmRequest`, `RenderRequest`, `RenderOutput`, `ComponentKind`, `NotificationLevel`)
- [ ] **Step 4.3.2:** Define handler aliases (4 handler types — format, metadata, dialog, render)
- [ ] **Step 4.3.3:** Implement `OutputUiExtensionRegistry` with intercept-bound pattern
- [ ] **Step 4.3.4:** Host capability mapping: TUI native for Text/Diff, RPC JSON-only, Server HTML/SSR
- [ ] **Step 4.3.5:** Re-export from `mod.rs`

### Task 4.4: Output/UI tests

- [ ] **Step 4.4.1:** `tui_renders_text_and_diff` — `ComponentKind::Text` and `ComponentKind::Diff` render natively
- [ ] **Step 4.4.2:** `unsupported_kind_falls_back_to_string` — `ComponentKind::Chart` on TUI host → plain string
- [ ] **Step 4.4.3:** `dialog_confirm_blocks_for_user_response` — `ui.dialog.confirm` returns `bool` from user
- [ ] **Step 4.4.4:** `dialog_notify_is_non_blocking` — `ui.dialog.notify` does not block

### Task 4.5: 64-point integration test

**Files:**
- Create: `crates/synthia-agent/tests/extension_matrix.rs`

- [ ] **Step 4.5.1:** Build a list of all 64 extension point names (21 Phase 3 + 15 Round 1 + 9 Round 2 + 10 Round 3 + 9 Round 4)
- [ ] **Step 4.5.2:** For each point, register a no-op handler and call the corresponding `fire_*` method
- [ ] **Step 4.5.3:** Assert no panics, no errors, every `extension.hook` OTel span is emitted
- [ ] **Step 4.5.4:** Test passes when all 64 points are reachable

### Task 4.6: Round 4 validation

- [ ] **Step 4.6.1:** `cargo check --workspace` → 0 errors
- [ ] **Step 4.6.2:** `cargo test -p synthia-agent --lib -- extension_points::session_tree extension_points::output_ui` → all pass
- [ ] **Step 4.6.3:** `cargo test -p synthia-agent --test extension_matrix` → 64-point integration passes
- [ ] **Step 4.6.4:** `cargo clippy -p synthia-agent --lib --all-targets --all-features --tests` → 0 new warnings
- [ ] **Step 4.6.5:** `cargo +nightly fmt --all` → clean
- [ ] **Step 4.6.6:** Mark all Round 4 tasks complete in [tasks.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/tasks.md)
- [ ] **Step 4.6.7:** Commit: `feat(extension): 9 Session Tree + Output/UI extension points + 64-point integration test`（不自动 commit，等用户明确指示）

---

## Self-Review

- ✅ Spec coverage: all 8 spec files map to Round 1-4; 64-point matrix complete after Round 4
- ✅ No placeholders: each step has file paths, code patterns, and validation commands
- ✅ Type consistency: `Action<T>` defined in `tool.rs` and reused across all 8 scope modules
- ✅ Hard constraints: P1 (Context hooks fire before prefix snapshot), P6 (PermissionExtensibilityGuard), P9 (every fire emits OTel span)
- ✅ Project rule: no auto-commit — each round ends with "等用户明确指示"
- ✅ Backward compat: existing 21 Phase 3 points unchanged; only additive changes

---

## Summary

- **4 rounds × 1 commit each** = 4 commits
- **43 extension points** implemented (Round 1 ✅ done; Rounds 2-4 pending)
- **~24 new tests** + 1 cross-scope integration test (Round 4)
- **8 new files** in `extension_points/` (Round 1: 2 done; Rounds 2-4: 6 pending)
- **No breaking changes** to Phase 3 or to existing public API
- **Net new code (Rounds 1-4 combined)**: ~5,000 lines (impl + tests)
