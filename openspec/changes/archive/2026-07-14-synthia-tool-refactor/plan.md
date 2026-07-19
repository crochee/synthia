# Plan: synthia-tool-refactor (Change 1 of v3 architecture)

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development (recommended) or executing-plans. Steps use checkbox (`- [ ]`) syntax. **No auto-commit** — every Round ends with "等用户明确指示".

**Goal:** Adopt object-safe `ToolExecutor<Invocation>` + `ToolRouter`/`ToolRegistry` separation + `ToolExposure`/`ToolSearch` + dual `AgentTool`/`ExtensionTool` shape; collapse 3 parallel tool registries into 1; absorb in-flight `add-dynamic-tool-provider-system` + `adopt-explore-agent-recommendations`; migrate 9 non-Tool abstractions to `ExtensionTool`. Land in **7 Rounds**, each ≤ 1500 LOC, each independently verifiable, with **zero behavioral regression** on the 5 historical e2e tests.

**Architecture / Reference / Validation:** see [design.md](../design.md).

**Tech Stack:** Rust (tokio async, parking_lot, tracing, serde, schemars). New crate `synthia-tool-core` depends on `serde`, `serde_json`, `tokio`, `schemars`, `tracing` only.

---

## Round 1: `synthia-tool-core` skeleton (Foundation)

**Why first:** every later change builds on the type system; nothing else compiles until this lands.

### Task 1.1: New crate scaffold

**Files:**
- Create: `crates/synthia-tool-core/Cargo.toml`
- Create: `crates/synthia-tool-core/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1.1.1:** Create `Cargo.toml` with deps: `serde`, `serde_json`, `tokio` (sync feature), `parking_lot`, `tracing`, `tracing-subscriber`, `schemars`, `thiserror`. Mirror synthia-core dependency style.
- [ ] **Step 1.1.2:** Add to root `Cargo.toml` `members = [...]` array
- [ ] **Step 1.1.3:** `lib.rs` re-exports planned public items as TODO comments (no impl yet)
- [ ] **Step 1.1.4:** `cargo check -p synthia-tool-core` returns 0 errors

### Task 1.2: Exposure + Spec + Invocation types

**Files:** `crates/synthia-tool-core/src/{exposure,spec,invocation}.rs`

- [ ] **Step 1.2.1:** Define `ToolExposure { Direct, Deferred, DirectModelOnly, Hidden }` with `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`
- [ ] **Step 1.2.2:** Define `ToolName(pub Arc<str>)` newtype with `PartialEq, Eq, Hash, Display`
- [ ] **Step 1.2.3:** Define `ToolSpec { name, description, parameters: Arc<schemars::schema::Schema>, execution_mode, exposure, search_keywords }`
- [ ] **Step 1.2.4:** Define `LoadableToolSpec { name, summary, namespace, tokens_hint }`
- [ ] **Step 1.2.5:** Define `ToolSearchInfo { name, summary, keywords: Vec<String> }`
- [ ] **Step 1.2.6:** Define `ToolInvocation { Function { name, args, ctx }, ToolSearch { namespace, hint }, Other(Arc<dyn AnyToolInvocation>) }` — non_exhaustive
- [ ] **Step 1.2.7:** All structs derive `Debug, Clone, Serialize, Deserialize`

### Task 1.3: Round 1 validation

- [ ] **Step 1.3.1:** `cargo +nightly fmt --all` → clean
- [ ] **Step 1.3.2:** `cargo check -p synthia-tool-core` → 0 errors
- [ ] **Step 1.3.3:** `cargo clippy -p synthia-tool-core --all-targets --all-features --tests` → 0 new warnings
- [ ] **Step 1.3.4:** **No commit** — end of R1; user approval required before R2

---

## Round 2: `AgentTool` + `ExtensionTool` dual shape + legacy compat

**Why second:** the dual-shape change is breaking (trait rename) but blanket impl preserves compile for all existing impl blocks. Rounds 3-7 plug into this.

### Task 2.1: Define `AgentTool` (lean, 5 methods)

**Files:**
- Create: `crates/synthia-tool-core/src/agent_tool.rs`
- Create: `crates/synthia-tool-core/src/context.rs` (`ToolContext { cancellation_token, session_id, directory, ... }`)

- [ ] **Step 2.1.1:** `pub trait AgentTool: Send + Sync + 'static` with 5 methods: `name()`, `description()`, `execution_mode()`, `parameters()`, `call(args, ctx) -> Result<ToolOutputBox, FunctionCallError>`
- [ ] **Step 2.1.2:** Define `ToolOutputBox = Box<dyn DynToolOutput>` (object-safe wrapper carrying `Content` + `is_error` + `metadata` + `truncated_by`)
- [ ] **Step 2.1.3:** Define `FunctionCallError { Schema, Permission, Runtime, Cancelled, ... }` — non_exhaustive
- [ ] **Step 2.1.4:** Define `ToolContext` with `cancellation_token: tokio_util::sync::CancellationToken`, `session_id`, `directory: Option<PathBuf>`
- [ ] **Step 2.1.5:** Unit test: minimal impl `agent_tool::tests::trivial_tool` returns OK

### Task 2.2: Define `ExtensionTool` (rich, +7 methods)

**Files:** `crates/synthia-tool-core/src/extension_tool.rs`

- [ ] **Step 2.2.1:** `pub trait ExtensionTool: AgentTool` with 7 default-implemented methods: `extension_api_version`, `prompt_snippet`, `prompt_guidelines`, `render_call`, `render_result`, `needs_extension_context`, `bind_extension`
- [ ] **Step 2.2.2:** Define `pub trait AnyExtensionContext: Send + Sync { fn on_event_filter(...); }` placeholder (full impl in Change 2 R1)
- [ ] **Step 2.2.3:** Marker: `impl dyn ExtensionTool` is the type witnessed in registries
- [ ] **Step 2.2.4:** Unit test: `extension_tool::tests::rich_tool` overrides `render_call` + `render_result`

### Task 2.3: Compat shim — preserve legacy `Tool` impl

**Files:**
- Modify: `crates/synthia-tool/src/traits.rs`
- Modify: `crates/synthia-tool/src/lib.rs` (re-export compatibility type alias)

- [ ] **Step 2.3.1:** Move existing 9-method `Tool` trait into `crates/synthia-tool-core/src/compat.rs` as `LegacyTool` internal alias
- [ ] **Step 2.3.2:** In `crates/synthia-tool/src/traits.rs`: `#[deprecated(since = "0.2.0", note = "use synthia_tool_core::AgentTool")] pub trait Tool: LegacyTool {}`
- [ ] **Step 2.3.3:** Provide blanket `impl<T: LegacyTool> AgentTool for T` in `synthia-tool-core/src/compat.rs`
- [ ] **Step 2.3.4:** Verify all existing `impl Tool for X` impls still compile (7 builtin tools)
- [ ] **Step 2.3.5:** Verify `clippy` doesn't flag legacy impls

### Task 2.4: Round 2 validation

- [ ] **Step 2.4.1:** `cargo check --workspace --all-features` → 0 errors
- [ ] **Step 2.4.2:** `cargo test -p synthia-agent -p synthia-tool -p synthia-tool-core` → all pass
- [ ] **Step 2.4.3:** **Behavioral regression check**: run `react_loop_test` + `e2e_llm_test` + `e2e_event_sequence_test` + `e2e_memory_correctness_test` — must all pass unchanged
- [ ] **Step 2.4.4:** **No commit** — user approval required before R3

---

## Round 3: Collapse 3 tool registries → 1

**Why third:** now that `AgentTool` exists, the 3 parallel registries can collapse without breaking callers (orphan trait methods fall through compat layer).

### Task 3.1: Adopt `ToolRegistry` v2 in `synthia-tool-core`

**Files:**
- Create: `crates/synthia-tool-core/src/registry.rs`
- Modify: `crates/synthia-tool/src/lib.rs` (deprecate old `ToolRegistry`)

- [ ] **Step 3.1.1:** Define `ToolRegistry { tools: RwLock<HashMap<ToolName, Arc<dyn AgentTool>>>, providers: RwLock<Vec<Arc<dyn ToolProvider>>>, cache_version: AtomicU64 }`
- [ ] **Step 3.1.2:** Methods: `new()`, `register<T: AgentTool>(&self, tool: T)`, `register_provider()`, `get(&ToolName)`, `list()`, `iter_provider()`
- [ ] **Step 3.1.3:** `register` increments `cache_version` atomically
- [ ] **Step 3.1.4:** Unit tests: `register_duplicate_replaces`, `register_increments_cache_version`, `get_unknown_returns_none`

### Task 3.2: Delete `LayeredToolRegistry` (only used by tests)

**Files:**
- Delete: `crates/synthia-tool/src/scoped_registry.rs:208-298` (the `LayeredToolRegistry` struct only — keep `ScopedToolRegistry` above)
- Modify: `crates/synthia-tool/src/scoped_registry.rs` (drop unused)

- [ ] **Step 3.2.1:** Audit all consumers: `cargo doc --document-private-items -p synthia-tool` + grep for `LayeredToolRegistry`
- [ ] **Step 3.2.2:** Confirm only `tests.rs` consumes it; delete the struct
- [ ] **Step 3.2.3:** `ScopedToolRegistry` keeps `ScopeGuard` RAII cleanup, gains thin wrapper around v2 `ToolRegistry`

### Task 3.3: Phase-out `synthia-tool::registry::ToolRegistry`

- [ ] **Step 3.3.1:** In `crates/synthia-tool/src/registry/registration/registry.rs`: `#[deprecated(...)]` on `ToolRegistry::register_defaults`
- [ ] **Step 3.3.2:** In `crates/synthia-agent/src/agent.rs` `register_defaults()` call sites: replace with `ExtensionManager::from_providers(default_providers())` (provider list TBD in R6)
- [ ] **Step 3.3.3:** `synthia-tool/src/registry` remains as a thin re-export of `synthia_tool_core::ToolRegistry`

### Task 3.4: Round 3 validation

- [ ] **Step 3.4.1:** `cargo check --workspace` → 0 errors
- [ ] **Step 3.4.2:** 5 historical e2e unchanged
- [ ] **Step 3.4.3:** `cargo clippy --workspace --all-targets -- -D warnings` → 0 new warnings
- [ ] **Step 3.4.4:** **No commit** — user approval required

---

## Round 4: `ToolRouter` (model-visible) + spec cache

### Task 4.1: Router

**Files:**
- Create: `crates/synthia-tool-core/src/router.rs`

- [ ] **Step 4.1.1:** `ToolRouter { registry: Arc<ToolRegistry>, model_spec_filter: fn(&ToolSpec) -> bool, spec_cache: RwLock<Option<(u64, Vec<ToolSpec>)>> }`
- [ ] **Step 4.1.2:** `model_visible_specs()`: pulls from cache if version matches, else re-builds
- [ ] **Step 4.1.3:** `search(query)`: iterates deferred tools (via `ToolSearchInfo`), returns `Vec<LoadableToolSpec>`
- [ ] **Step 4.1.4:** `dispatch(inv: ToolInvocation)`: routes `Function` to registry; routes `ToolSearch` to `search()`
- [ ] **Step 4.1.5:** Cache invalidation on `cache_version` change atomic CAS

### Task 4.2: Tests

- [ ] **Step 4.2.1:** `model_visible_specs_caches` — call twice, registry version unchanged → second call shares Vec
- [ ] **Step 4.2.2:** `cache_invalidates_on_register` — register after first call; second call returns updated specs
- [ ] **Step 4.2.3:** `search_returns_only_deferred` — Direct tools never appear; Deferred tools with matching keywords do
- [ ] **Step 4.2.4:** `dispatch_routes_function_to_registry` — pure unit

### Task 4.3: New crate `synthia-tool-router`

- [ ] **Step 4.3.1:** Wrap `ToolRouter` + thin `RouterHandle` for cross-crate access (allows `synthia-tool-orchestrator` to depend on this without pulling the whole `synthia-tool-core` test suite)
- [ ] **Step 4.3.2:** `lib.rs` re-exports only public types
- [ ] **Step 4.3.3:** `cargo check -p synthia-tool-router` clean

### Task 4.4: Round 4 validation

- [ ] **Step 4.4.1:** `cargo test -p synthia-tool-core` — router tests green
- [ ] **Step 4.4.2:** 5 historical e2e unchanged
- [ ] **Step 4.4.3:** **No commit**

---

## Round 5: `ToolExposure` + `ToolSearch` + bash truncated_by wired

### Task 5.1: Make all builtin tools exposure-aware

**Files:**
- Modify: `crates/synthia-tool/src/builtin/{read,write,multi_edit,apply_patch,glob,grep,web,monitor}.rs` (8 files)
- Modify: `crates/synthia-tool-orchestrator/src/lib.rs` (replace hardcoded `default_permission_for_tool` at line 254-261 with trait method)

- [ ] **Step 5.1.1:** Each builtin declares `fn exposure(&self) -> ToolExposure { ToolExposure::Direct }`; default = Direct
- [ ] **Step 5.1.2:** Add `Tool::required_permission()` to `AgentTool` trait; backend impls return `ApprovalRequirement`
- [ ] **Step 5.1.3:** Delete `default_permission_for_tool` match-arm in orchestrator; use `tool.required_permission()` instead
- [ ] **Step 5.1.4:** Add `Tool::deferred()` constructor helper for plugin tools (default = `Deferred`)

### Task 5.2: ToolSearch built-in tool

**Files:** `crates/synthia-tool/src/builtin/search_tools.rs` (new)

- [ ] **Step 5.2.1:** Implement `ToolSearchTool` as a builtin that calls `ToolRouter::search`
- [ ] **Step 5.2.2:** Wire into default exposure = `Direct`
- [ ] **Step 5.2.3:** Test: 3 deferred tools registered; `tool_search({"hint": "git"})` returns matching subset

### Task 5.3: Wire bash `truncated_by`

**Files:**
- Modify: `crates/synthia-tool-bash/src/bash_tool/executor.rs:48-68`
- Modify: `crates/synthia-tool/src/types.rs:50-146` (`ToolOutput.truncated_by` is already declared; now used)

- [ ] **Step 5.3.1:** Replace `cap_to_char_boundary(&mut s, self.max_output_length)` with `truncate_tail_to_bytes(&mut s, max_bytes)` using UTF-8 char-boundary (`(b & 0xc0) != 0x80`)
- [ ] **Step 5.3.2:** Populate `ToolOutput { truncated_by: Some(TruncatedBy::Bytes { shown, total }), ... }`
- [ ] **Step 5.3.3:** Test: bash output over 50 KB → `truncated_by: Bytes { shown: 50*1024, total: ... }` present

### Task 5.4: Round 5 validation

- [ ] **Step 5.4.1:** `cargo test -p synthia-tool-bash` — bash suite green incl. truncation
- [ ] **Step 5.4.2:** `cargo test -p synthia-tool-core` — exposure + search tests
- [ ] **Step 5.4.3:** 5 historical e2e unchanged
- [ ] **Step 5.4.4:** **No commit**

---

## Round 6: `ExtensionTool` ×9 + 4 `ToolProvider`s (absorb in-flight)

### Task 6.1: Provider finalization (absorb `add-dynamic-tool-provider-system`)

**Files:**
- Create: `crates/synthia-agent/src/tools/providers/{file,search,bash,mcp}_tools_provider.rs`
- Create: `crates/synthia-agent/src/tools/extension_manager.rs` (or move from `dynamic_provider/extension_manager.rs`)
- Modify: `crates/synthia-agent/src/agent.rs:806-823` (use `ExtensionManager` as the source for default tools; `register_defaults` deprecated path removed)

- [ ] **Step 6.1.1:** `FileToolsProvider` — already implemented (commit `ec74cff`); audit and lock down
- [ ] **Step 6.1.2:** `SearchToolsProvider` — wraps `read`, `glob`, `grep`, `ls` (file-mutating vs read tools split by capability)
- [ ] **Step 6.1.3:** `BashToolsProvider` — wraps `bash`, `monitor`, `command_blacklist`
- [ ] **Step 6.1.4:** `MCPToolsProvider` — wraps `synthia-mcp::Client::list_tools()` for each connected MCP server
- [ ] **Step 6.1.5:** Unit tests: each provider's `list_tools()` returns expected set

### Task 6.2: 9 abstractions → `ExtensionTool`

**Files:**
- Modify: `crates/synthia-context/src/compact_context_tool.rs` (drop facade)
- Modify: `crates/synthia-skill/src/implicit_tools/load_skill.rs`
- Modify: `crates/synthia-agent/src/tools/agent_tools/subagent.rs` (split off from `team.rs`)
- Modify: `crates/synthia-guardian/src/self_reflect_tool.rs` (rename const path → ExtensionTool)
- Modify: `crates/synthia-tool-bash/src/monitor_tool.rs`
- Modify: `crates/synthia-mcp/src/tool_adapter.rs`
- Modify: `crates/synthia-plugin/src/external_hook_tool.rs`
- Modify: `crates/synthia-skill/src/usage_tracker.rs`
- Modify: `crates/synthia-plugin/src/cli_tool.rs`

- [ ] **Step 6.2.1:** Each becomes `impl ExtensionTool`. Verify `bind_extension()` no-op (left for Change 2 R7)
- [ ] **Step 6.2.2:** Remove the `main_loop.rs:543-547` hardcoded string comparison for `SELF_REFLECT_TOOL_NAME`
- [ ] **Step 6.2.3:** Remove the `compact_context_tool` facade trick in `main_loop.rs:552-561`
- [ ] **Step 6.2.4:** Register each via the appropriate `ToolProvider`'s `list_tools()`
- [ ] **Step 6.2.5:** Integration test: each of the 9 abstractions is discoverable via the standard `ToolRouter::model_visible_specs`

### Task 6.3: `register_defaults` removed

- [ ] **Step 6.3.1:** Add `default_providers()` factory in `synthia-agent/src/agent.rs`
- [ ] **Step 6.3.2:** `register_defaults` in `crates/synthia-tool/src/registry/registration/registry.rs` is `#[deprecated]` + emits warning on use

### Task 6.4: Round 6 validation

- [ ] **Step 6.4.1:** `cargo test --workspace` all pass
- [ ] **Step 6.4.2:** New test: `cargo test -p synthia-agent --test 9_abstractions` validates all 9 abstractions come through the standard `ToolRegistry::run_with_context` path
- [ ] **Step 6.4.3:** 5 historical e2e unchanged
- [ ] **Step 6.4.4:** **No commit**

---

## Round 7: Wire 7 Tool-scope extension points; finalize 64-point partial matrix

### Task 7.1: 7 extension-point events

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool_v2.rs` (new — wire 7 new points)
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/mod.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs` (existing 9 points preserved)

- [ ] **Step 7.1.1:** `tool.registry.register` (observe-only) — fires after `ToolRegistry::register`
- [ ] **Step 7.1.2:** `tool.registry.unregister` (observe-only) — fires before removal
- [ ] **Step 7.1.3:** `tool.definition.transform` (`Action<ToolSpec>`) — modify LLM-visible spec at registration
- [ ] **Step 7.1.4:** `tool.execution_mode.override` (`Action<ExecutionMode>`) — change `Parallel` ↔ `Sequential`
- [ ] **Step 7.1.5:** `tool.parallelism.barrier` (`Action<BarrierId>`) — hold tool calls of same kind serial
- [ ] **Step 7.1.6:** `tool.output.format` (`Action<ToolOutputBox>`) — post-process output content
- [ ] **Step 7.1.7:** `tool.output.metadata.inject` (`Action<ToolOutput>`) — add metadata before persisting

### Task 7.2: Tests per point

- [ ] **Step 7.2.1:** `register_observer_fires`
- [ ] **Step 7.2.2:** `definition_transform_modifies_spec`
- [ ] **Step 7.2.3:** `execution_mode_override_toggles_parallelism`
- [ ] **Step 7.2.4:** `parallelism_barrier_serializes_same_tool`
- [ ] **Step 7.2.5:** `output_format_replaces_content`
- [ ] **Step 7.2.6:** `output_metadata_inject_persists`

### Task 7.3: Partial 64-point integration test

**Files:** `crates/synthia-agent/tests/extension_matrix_r1_to_r7.rs` (new)

- [ ] **Step 7.3.1:** Build list of all wired extension points (Round 1's 15 + Round 7's 7 = 22)
- [ ] **Step 7.3.2:** For each, register a no-op handler; call corresponding fire; assert OTel span emitted
- [ ] **Step 7.3.3:** Test passes when all 22 are reachable

### Task 7.4: Round 7 validation + R1 archive

- [ ] **Step 7.4.1:** All previous-round validations still pass
- [ ] **Step 7.4.2:** `cargo +nightly fmt --all` clean
- [ ] **Step 7.4.3:** **OpenSpec archive** (per `omo-archive-change` skill): `openspec archive synthia-tool-refactor` after all 7 Rounds pass
- [ ] **Step 7.4.4:** Update `extension-point-matrix` spec to mark 7 Tool-scope points as `VERIFIED`
- [ ] **Step 7.4.5:** **No commit** — user approval required for archive + per-change PR

---

## Self-Review

- ✅ Every Round has file paths, code patterns, validation commands
- ✅ No placeholders: implementation steps concrete
- ✅ Project rule: no auto-commit — every Round ends with "user approval required"
- ✅ Backward compat: legacy `Tool` impls continue to compile throughout 0.2.x
- ✅ Hard constraints: P1 (spec transforms are deterministic), P6 (permission is trait method, not orchestrator switch), P9 (every fire emits OTel)
- ✅ In-flight works absorbed: `add-dynamic-tool-provider-system` Phase 2 + `adopt-explore-agent-recommendations` R1-R3 fully consumed
- ✅ Out of scope clearly listed (defer to Change 2 / Change 3)
- ✅ Net new code: ~5,000 LOC across 7 Rounds; net deletions ~3,000 LOC

## Summary

- 7 Rounds × 1 commit each (with user approval) = 7 commits
- ~5,000 LOC new + ~3,000 LOC deleted = net ~+2,000
- ~24 new tests + 1 integration test (Round 7)
- New crates: `synthia-tool-core`, `synthia-tool-router`, optionally `synthia-tool-runtime` (depends on R5 split decision)
- No breaking changes to existing public API during 0.2.x; `Tool` removed at 0.3.0
