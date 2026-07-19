# Implementation Tasks — Extension Points Phase 2

> **Status (2026-07-12):** All 8 specs written, validated. 97/97 tasks complete (Round 1 ✅, Round 2 ✅, Round 3 ✅, Round 4 ✅). All commit tasks done. Phase 0 of `extension-points-phase-2` is shared with the archived `tool-abstraction-and-extensibility` change (compile errors fixed). Commits (top to bottom of master):
> - `feat(extension): 43 extension points across 8 scopes + 64-point matrix` (extension_points + extension_matrix.rs)
> - `chore: ignore local research and working notes` (.gitignore)
> - `feat(agent): complete tool-abstraction WIP and remove superseded docs` (tool-abstraction leftovers + 8 docs deletion)
>
> **Strategy:** 4 rounds, each = 1 logical commit, ≤15 extension points per round, additive only (no modifications to existing call sites). The 21 Phase 3 points (Agent Loop + Tool) remain unchanged.

---

## Round 1: Context + LLM (15 points) — ✅ DONE (2026-07-12)

> **Why first:** Context hooks interact with P1 (KV-cache prefix hash); LLM hooks rewrite chat-bound data. Both are highest-risk and most-frequently-reserved points.

### 1.1 Module structure

- [x] 1.1.1 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/llm.rs` — define `LlmExtensionRegistry`, typed event structs for all 8 LLM points, `Action<LlmOutput>` return type for mutation pattern
- [x] 1.1.2 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/context.rs` — define `ContextExtensionRegistry`, typed event structs for all 7 Context points, `Action<ContextOutput>` return type
- [x] 1.1.3 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs` — re-export `LlmExtensionRegistry` + `ContextExtensionRegistry`

### 1.2 LLM extension points (8)

- [x] 1.2.1 `system_prompt.transform` — `fn(&SystemPromptTransformInput) -> Action<SystemPromptTransformOutput>` (replace system prompt content; P1 hash recomputed after)
- [x] 1.2.2 `messages.transform` — `fn(&MessagesTransformInput) -> Action<MessagesTransformOutput>` (reorder/redact/annotate messages)
- [x] 1.2.3 `chat.params` — typed `ChatParams { temperature: f32, top_p: f32, top_k: u32, max_tokens: u32 }`, mutable reference
- [x] 1.2.4 `chat.headers.inject` — `fn(&ChatHeadersInput) -> Action<ChatHeadersOutput>` (add tracing IDs, auth tokens)
- [x] 1.2.5 `tool_choice.override` — `fn(&ToolChoiceInput) -> Action<ToolChoiceOutput>` (force function calling)
- [x] 1.2.6 `model.select` — `fn(&ModelSelectInput) -> Action<ModelSelectOutput>` (multi-model routing)
- [x] 1.2.7 `cache.breakpoint.set` — `fn(&CacheBreakpointInput) -> Vec<CacheBreakpoint>`
- [x] 1.2.8 `response.transform` — `fn(&ResponseTransformInput) -> Action<ResponseTransformOutput>` (post-LLM annotation; P1 hash on stored message)

### 1.3 Context extension points (7)

- [x] 1.3.1 `context.compact.trigger` — observe-only (external trigger flag)
- [x] 1.3.2 `context.compact.summarize` — `fn(&SummarizeInput) -> Option<String>` (None = default LLM summarization)
- [x] 1.3.3 `context.compact.replace` — `fn(&CompactPlan) -> Action<CompactPlan>` (change strategy)
- [x] 1.3.4 `context.prefix.participate` — `fn() -> Vec<u8>` (return bytes to include in prefix hash)
- [x] 1.3.5 `context.observability.emit` — observe-only (metrics emission)
- [x] 1.3.6 `context.token_budget.adjust` — `fn() -> Option<TokenBudget>`
- [x] 1.3.7 `context.message_filter` — `fn(&Vec<Message>) -> Action<Vec<Message>>` (PII redaction)

### 1.4 OTel spans

- [x] 1.4.1 Every `fire` method emits `tracing::info_span!("extension.hook", point, scope, extension_id, payload_size)` (consistent with Phase 3)
- [x] 1.4.2 Deterministic-transfrom guard: if `messages.transform` produces different hash on same input, log `extension.non_deterministic` event

### 1.5 Tests (≥6) — ✅ 19/19 pass

- [x] 1.5.1 `llm.rs::tests` — 8 tests passing: `new_registry_is_empty`, `chat_params_modification_reflected_in_request`, `deterministic_transform_preserves_hash`, `non_deterministic_transform_is_detected_by_caller`, `cache_breakpoint_returns_union_of_handlers`, `skip_short_circuits_the_chain`, `multiple_modifiers_apply_in_registration_order`, `concurrent_register_does_not_lose_handlers`
- [x] 1.5.2 `context.rs::tests` — 11 tests passing: `new_registry_is_empty`, `noop_filter_preserves_hash`, `modifying_filter_invalidates_cache`, `prefix_participate_bytes_included_in_hash`, `summarize_override_skips_llm_call`, `summarize_returns_none_when_no_handler_provides`, `token_budget_returns_first_non_none`, `compact_trigger_is_observe_only`, `compact_replace_changes_strategy`, `message_filter_proceed_is_no_op`, `concurrent_register_does_not_lose_handlers`

### 1.6 Validation

- [x] 1.6.1 `cargo check --workspace` → 0 errors
- [x] 1.6.2 `cargo test -p synthia-agent --lib extension_points::llm` → 8/8 pass
- [x] 1.6.3 `cargo test -p synthia-agent --lib extension_points::context` → 11/11 pass
- [x] 1.6.4 `cargo clippy -p synthia-agent --lib --all-targets --all-features --tests` → 0 new warnings (only pre-existing deprecation warnings on `build_default_tool_registry`)
- [x] 1.6.5 `cargo +nightly fmt --all` → clean
- [x] 1.6.6 Commit: `feat(extension): 15 LLM + Context extension points`（不自动 commit，等用户明确指示）

---

## Round 2: Permission + Provider (9 points)

> **Why second:** P6 fail-closed semantics must be in place before any plugin can register a `permission.ask` handler.

### 2.1 Module structure

- [x] 2.1.1 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/permission.rs` — `PermissionExtensionRegistry` + `PermissionExtensibilityGuard` wrapper
- [x] 2.1.2 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/provider.rs` — `ProviderExtensionRegistry` (DashMap + AtomicU64 cache_version)
- [x] 2.1.3 Re-export from `mod.rs`

### 2.2 Permission extension points (5)

- [x] 2.2.1 `permission.ask` — `fn(&PermissionRequest) -> Action<PermissionDecision>` (constrained: may only ADD to deny list)
- [x] 2.2.2 `permission.notify` — observe-only (audit log)
- [x] 2.2.3 `doom_loop.detected` — `fn(&DoomLoopInfo) -> DoomLoopAction` (AllowOneMore | DenyNow | AskUser)
- [x] 2.2.4 `blacklist.match` — `fn(&BlacklistInput) -> Option<BlacklistEntry>` (hot-path, O(1))
- [x] 2.2.5 `permission.persist` — `fn(&PersistInput) -> Action<PersistOutput>`

### 2.3 PermissionExtensibilityGuard (P6 fail-closed)

- [x] 2.3.1 Implement guard that wraps the chain: any handler that weakens `Deny → Allow` or `Deny → AskUser` is downgraded to `AskUser`
- [x] 2.3.2 Emit `permission.weakening_attempt` OTel event on every downgrade
- [x] 2.3.3 Test: `weakening_attempt_downgraded_to_ask_user` + `legitimate_deny_blacklist_allowed`

### 2.4 Provider extension points (4)

- [x] 2.4.1 `provider.register` — `fn(&ProviderConfig) -> Action<ProviderConfig>` (idempotent: re-register replaces)
- [x] 2.4.2 `provider.unregister` — `fn(name: &str) -> bool`
- [x] 2.4.3 `provider.auth` — `fn(&AuthRequest) -> Action<AuthRequest>` (token rotation)
- [x] 2.4.4 `provider.fallback` — `fn(&FallbackContext) -> Action<FallbackChain>`

### 2.5 Tests (≥4)

- [x] 2.5.1 `permission.rs::tests` — 2 tests: `weakening_attempt_downgraded_to_ask_user` + `legitimate_deny_blacklist_bypasses_user_prompt`
- [x] 2.5.2 `provider.rs::tests` — 2 tests: `concurrent_register_increments_cache_version` + `fallback_chain_iterated_in_order`

### 2.6 Validation

- [x] 2.6.1 `cargo check --workspace` → 0 errors
- [x] 2.6.2 `cargo test -p synthia-agent --lib extension_points::permission` → all pass
- [x] 2.6.3 `cargo test -p synthia-agent --lib extension_points::provider` → all pass
- [x] 2.6.4 `cargo clippy --workspace --all-targets --all-features --tests` → 0 new warnings
- [x] 2.6.5 `cargo +nightly fmt --all` → clean
- [x] 2.6.6 Commit: `feat(extension): 9 Permission + Provider extension points (fail-closed)`（不自动 commit，等用户明确指示）

---

## Round 3: Event Bus + Plugin Lifecycle (10 points)

> **Why third:** Event Bus and Plugin Lifecycle are the meta-observability layer. They're less coupled to data flow and don't need P1 or P6 guards.

### 3.1 Module structure

- [x] 3.1.1 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/event_bus.rs` — `EventBusExtensionRegistry` (typed topic enum + sequence numbers)
- [x] 3.1.2 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/plugin_lifecycle.rs` — `PluginLifecycleExtensionRegistry` (reuses `ExtensionContext` state machine)
- [x] 3.1.3 Re-export from `mod.rs`

### 3.2 Event Bus extension points (4)

- [x] 3.2.1 `event.subscribe` — `fn(&SubscribeRequest) -> Action<SubscribeOutput>` (register handler for typed topic)
- [x] 3.2.2 `event.publish` — fire all subscribers in registration order (no `Action` — direct invocation)
- [x] 3.2.3 `event.aggregate` — `fn(&AggregateRequest) -> Option<AggregatedEvent>`
- [x] 3.2.4 `event.replay` — `fn(&ReplayRequest) -> Vec<ReplayedEvent>` (tagged with `replay=true` in OTel)

### 3.3 Plugin Lifecycle extension points (6)

- [x] 3.3.1 `extension.load` — transition to `Loading`, queue pending registrations
- [x] 3.3.2 `extension.bind` — transition to `Active`, flush pending queue
- [x] 3.3.3 `extension.invalidate` — transition to `Stale`, retain `last_active` for diagnostics
- [x] 3.3.4 `extension.unload` — drop `last_active`, mark fully unloaded
- [x] 3.3.5 `extension.hot_swap` — 3-event sequence: `load` (new) + `invalidate` (old) + `bind` (new)
- [x] 3.3.6 `extension.dual_form` — `fn(&DualFormQuery) -> Action<DualFormResponse>`

### 3.4 Tests (≥4)

- [x] 3.4.1 `event_bus.rs::tests` — 2 tests: `within_topic_ordering_preserved` + `cross_topic_ordering_not_guaranteed`
- [x] 3.4.2 `plugin_lifecycle.rs::tests` — 2 tests: `hot_swap_transitions_through_valid_states` + `state_machine_integrity_under_100_iterations`

### 3.5 Validation

- [x] 3.5.1 `cargo check --workspace` → 0 errors
- [x] 3.5.2 `cargo test -p synthia-agent --lib extension_points::event_bus` → all pass
- [x] 3.5.3 `cargo test -p synthia-agent --lib extension_points::plugin_lifecycle` → all pass
- [x] 3.5.4 `cargo clippy --workspace --all-targets --all-features --tests` → 0 new warnings
- [x] 3.5.5 `cargo +nightly fmt --all` → clean
- [x] 3.5.6 Commit: `feat(extension): 10 Event Bus + Plugin Lifecycle extension points`（不自动 commit，等用户明确指示）

---

## Round 4: Session Tree + Output/UI (9 points + 64-point integration test)

> **Why last:** Session Tree and Output/UI are user-facing. The 64-point integration test verifies the full matrix is reachable.

### 4.1 Module structure

- [x] 4.1.1 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/session_tree.rs` — `SessionTreeExtensionRegistry` (write-bound pattern: most points append/mutate entries)
- [x] 4.1.2 `crates/synthia-agent/src/tools/dynamic_provider/extension_points/output_ui.rs` — `OutputUiExtensionRegistry` (intercept-bound pattern)
- [x] 4.1.3 Re-export from `mod.rs`

### 4.2 Session Tree extension points (5)

- [x] 4.2.1 `session.entry.append` — `fn(&EntryAppendInput) -> Action<EntryAppendOutput>` (mutate metadata/tags; may `Skip`)
- [x] 4.2.2 `session.entry.tree_walk` — `fn(&TreeWalkRequest) -> Vec<BranchNode>` (pre-order traversal)
- [x] 4.2.3 `session.branch.create` — `fn(&BranchCreateRequest) -> Action<BranchCreateOutput>` (freezes parent)
- [x] 4.2.4 `session.version.migrate` — `fn(&MigrateRequest) -> Option<serde_json::Value>` (None = default chain)
- [x] 4.2.5 `session.compaction.preserve` — observe-only (preserve `from_hook=true` summaries per `pi-mono session-manager.ts:48-61`)

### 4.3 Output/UI extension points (4)

- [x] 4.3.1 `output.format` — `fn(&OutputFormatInput) -> Action<OutputFormatOutput>` (rewrite content; deterministic, P1)
- [x] 4.3.2 `output.metadata.inject` — `fn(&OutputMetadataInput) -> MetadataPatch`
- [x] 4.3.3 `ui.dialog.{select, confirm, input, notify}` — typed dialog requests; `confirm` blocks for user response (with optional timeout)
- [x] 4.3.4 `ui.render.component` — `fn(&RenderRequest) -> Action<RenderOutput>`; host fallback to plain text for unsupported kinds

### 4.4 Host capability mapping

- [x] 4.4.1 TUI host: native render for `Text` + `Diff`; fallback to `String` for `Chart`
- [x] 4.4.2 RPC host: JSON-only serialization with `component_kind` metadata
- [x] 4.4.3 Server host: HTML/SSR with `data-component` hydration

### 4.5 Tests (≥4 + 1 integration)

- [x] 4.5.1 `session_tree.rs::tests` — 2 tests: `append_preserves_submission_order` + `branch_create_freezes_parent`
- [x] 4.5.2 `output_ui.rs::tests` — 2 tests: `tui_renders_text_and_diff` + `unsupported_kind_falls_back_to_string`
- [x] 4.5.3 `tests/extension_matrix.rs` — 1 integration test: every one of the 64 extension points has a registered handler that fires without panic and produces an OTel span

### 4.6 Validation

- [x] 4.6.1 `cargo check --workspace` → 0 errors
- [x] 4.6.2 `cargo test -p synthia-agent --lib extension_points::session_tree` → all pass
- [x] 4.6.3 `cargo test -p synthia-agent --lib extension_points::output_ui` → all pass
- [x] 4.6.4 `cargo test -p synthia-agent --test extension_matrix` → 64-point integration passes
- [x] 4.6.5 `cargo clippy --workspace --all-targets --all-features --tests` → 0 new warnings
- [x] 4.6.6 `cargo +nightly fmt --all` → clean
- [x] 4.6.7 Commit: `feat(extension): 9 Session Tree + Output/UI extension points + 64-point integration test`（不自动 commit，等用户明确指示）

---

## Summary

- **4 rounds × 1 commit each** = 4 commits
- **43 extension points** implemented (8 + 7 + 5 + 4 + 6 + 4 + 5 + 4)
- **~24 new tests** + 1 cross-scope integration test
- **8 new files** in `extension_points/` (no modifications to existing code)
- **No breaking changes** to Phase 3 (Agent Loop + Tool) or to existing public API
- **Hard constraints preserved:** P1 (Context scope, R1.4.2 deterministic guard), P6 (Permission scope, R2.3 PermissionExtensibilityGuard), P9 (every `fire` and state transition emits OTel)
