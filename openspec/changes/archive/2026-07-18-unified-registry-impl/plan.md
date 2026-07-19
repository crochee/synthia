# Unified Registry Implementation — Plan

> Schema: superpowers-bridge | Change: unified-registry-impl
> Source: tasks.md (12 groups, 87 tasks) + design.md (8 decisions)

---

## Execution Strategy

**Principle**: Each task group maps to a git commit. Within each group, tasks are ordered by dependency. Feature flag `unified-registry` gates all new code — old path compiles without it.

**Verification checkpoint** after every group: `cargo check --workspace --all-features && cargo clippy --all-targets --all-features --tests --all && cargo +nightly fmt --all`

---

## Group 0: Crate Restructuring (Phase 0) — ~2 weeks

**Commit**: `feat(unified-registry): add synthia-service, synthia-extension, synthia-event crate skeletons`

### 0.1 Create synthia-service crate
```
crates/synthia-service/
  Cargo.toml          # [package] name = "synthia-service", edition = "2021"
  src/lib.rs          # pub mod traits; pub mod provider; pub mod registry; pub mod coordinator;
  src/traits.rs       # Service, StatefulService, ErasedStatefulService
  src/provider.rs     # ServiceProvider, ServiceKey, ServiceDescriptor
  src/registry.rs     # ServiceRegistry, ServiceEntry
  src/coordinator.rs  # SessionRunCoordinator (placeholder)
```
- Add `synthia-core` as dependency
- `cargo check -p synthia-service`

### 0.2 Create synthia-extension crate skeleton
```
crates/synthia-extension/
  Cargo.toml
  src/lib.rs          # pub mod manifest;
  src/manifest.rs     # PluginManifest, PluginId, PluginCapabilities (type defs only)
```
- Dependencies: `synthia-core` only
- `cargo check -p synthia-extension`

### 0.3 Create synthia-event crate skeleton
```
crates/synthia-event/
  Cargo.toml
  src/lib.rs          # pub mod bus;
  src/bus.rs          # EventBus trait (publish/subscribe signatures only)
```
- Dependencies: `synthia-core` only
- `cargo check -p synthia-event`

### 0.4 Workspace + feature flags
- Add all 3 crates to root `Cargo.toml` `[workspace]`
- Add `unified-registry` feature to affected crates: `synthia-core`, `synthia-tool`, `synthia-agent`, `synthia-permission`, `synthia-memory`, `synthia-hook`, `synthia-session-v2`, `synthia-mcp`
- Verify: `cargo check --workspace --all-features`

### 0.5 Format + lint
- `cargo +nightly fmt --all`
- `cargo clippy --all-targets --all-features --tests --all`

---

## Group 1: Unified Tool Trait (Phase 1) — ~1 week

**Commit**: `feat(unified-registry): define unified Tool trait + ToolDescriptor + ToolProvenance`

### 1.1 Tool trait in synthia-core
File: `crates/synthia-core/src/tool/mod.rs`
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
    fn descriptor(&self) -> &ToolDescriptor { self.cached_descriptor() }
    fn cached_descriptor(&self) -> &ToolDescriptor;
}
```
- Gate with `#[cfg(feature = "unified-registry")]`
- `cargo check -p synthia-core --features unified-registry`

### 1.2 ToolDescriptor + supporting types
File: `crates/synthia-core/src/tool/descriptor.rs`
- `ToolDescriptor` (all fields from design §5.2)
- `ToolCategory`, `ToolProvenance`, `ExecutionMode`, `CancelBehavior`
- `ToolExample`, `ContextSource`
- `ToolProvenance`: `Clone, Debug, PartialEq, Eq, Hash` (NOT Copy)

### 1.3 ToolInput / ToolOutput / ToolError
File: `crates/synthia-core/src/tool/types.rs`
- `ToolInput { raw: serde_json::Value, parsed: Box<dyn erased_serde::Serialize + Send + Sync + 'static> }`
- `ToolOutput { content, structured, metadata, is_error }`
- `ToolMetadata { duration, tokens_in, tokens_out, truncated, managed_paths }`
- `ToolError` enum with `CapabilityDenied`, `ExecutionFailed`, `Timeout`, `InvalidInput`

### 1.4 Deprecate legacy Tool trait
File: `crates/synthia-tool/src/traits.rs`
- Add `#[deprecated(since = "0.x.0", note = "use unified Tool trait with feature unified-registry")]`
- Wrap behind `#[cfg(not(feature = "unified-registry"))]`

---

## Group 2: ToolProvider + ToolRegistry (Phase 1) — ~1 week

**Commit**: `feat(unified-registry): implement ToolProvider + ToolRegistry + Materialization`

### 2.1 ToolProvider trait
File: `crates/synthia-core/src/tool/provider.rs`
```rust
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn list_tools(&self) -> Vec<ToolDescriptor>;
    async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>>;
    async fn on_tool_event(&self, _event: &ToolEvent) {}
    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> { Ok(()) }
    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
```

### 2.2 ToolRegistry implementation
File: `crates/synthia-core/src/tool/registry.rs`
- `ToolRegistry { inner: Arc<RwLock<HashMap<String, Vec<ToolEntry>>>> }`
- `ToolEntry { provider_id, tool, descriptor, identity, registration_token }`
- `register_provider()`, `unregister()`, `materialize()`, `resolve()`, `resolve_now()`
- `internal_consistency_check()` under `#[cfg(debug_assertions)]`

### 2.3 ToolIdentity + ToolGeneration
- `ToolIdentity { name: String, generation: ToolGeneration }` — value type, `Clone`
- `ToolGeneration(pub u64)` — monotonic counter

### 2.4 Materialization
- `Materialization { snapshot: HashMap<String, (Arc<dyn Tool>, ToolIdentity)>, snapshot_token }`
- `StaleOrUnknown` enum

---

## Group 3: Tool Provider Implementations (Phase 1) — ~1 week

**Commit**: `feat(unified-registry): implement BuiltinToolProvider + McpToolProvider + PluginToolProvider`

### 3.1 BuiltinToolProvider (dual map)
File: `crates/synthia-tool/src/builtin/registry.rs`
- `applications: HashMap<String, Arc<dyn Tool>>` + `local: HashMap<String, Arc<dyn Tool>>`
- `register_builtin()` — refuse `CoreNameTaken`
- `add_local()` / `remove_local()`

### 3.2 McpToolProvider + McpConnection trait
File: `crates/synthia-mcp/src/provider.rs`
- `McpConnection` trait: `connect()`, `close()`
- `McpTransportConfig` enum: `Stdio`, `StreamableHttp`, `WebSocket`
- `McpToolProvider { server_name, host_owned, connection: Arc<dyn McpConnection> }`

### 3.3 PluginToolProvider (namespaced)
File: `crates/synthia-extension/src/tool_provider.rs`
- `PluginToolProvider { plugin_id, tools, prompt_visible_provenance }`
- `namespaced_name()` → `plugin:<id>:<name>`

### 3.4 SkillToolProvider + SubagentToolProvider + DynamicToolProvider
- Thin delegations to existing registries

---

## Group 4: Output Bound + Sanitization (Phase 1) — ~3 days

**Commit**: `feat(unified-registry): implement OutputBound + SanitizationPolicy + async bound_output`

### 4.1 OutputBound struct
File: `crates/synthia-core/src/tool/output_bound.rs`
- All fields from design §5.2 (50 KiB, 2000 lines, 4 MiB session, 7d retention, 1h cleanup)
- `OverflowStrategy` and `SanitizationPolicy` enums

### 4.2 bound_output async implementation
- Truncation logic (head/tail for `TruncateHeadTail`)
- `tokio::fs` spill to managed dir
- Background cleanup task via `tokio::spawn` + `tokio::time::interval`

---

## Group 5: ToolCapabilities + CapabilityBroker (Phase 1) — ~2 days

**Commit**: `feat(unified-registry): implement ToolCapabilities + CapabilityBroker (security B5)`

### 5.1 ToolCapabilities + CapabilityBroker
File: `crates/synthia-core/src/tool/capability.rs`
- `ToolCapabilities` struct with 8 boolean flags
- `CapabilityBroker` — checks flag before returning service handle
- `ToolError::CapabilityDenied { service, need }`

### 5.2 ToolContext update
File: `crates/synthia-core/src/tool/context.rs`
- Replace `services: Arc<ServiceRegistry>` with `capabilities: CapabilityBroker`

---

## Group 6: Migrate Built-in Tools (Phase 1) — ~1 week

**Commit**: `feat(unified-registry): migrate built-in tools to unified Tool trait`

### 6.1-6.7 Per-tool migration
For each tool (Read, Write, Bash, Grep, Edit, Shell, List/Tree):
- Implement new `Tool` trait (name, execute, cached_descriptor)
- Add `ToolCapabilities` declaration (e.g., GrepTool: `memory_read: true`)
- Add `ToolProvenance::Core`
- Gate with `#[cfg(feature = "unified-registry")]`
- Keep old impl behind `#[cfg(not(feature = "unified-registry"))]`

### 6.8 Register via BuiltinToolProvider
- `provider.register_builtin(Arc::new(ReadTool::new()))?` for each

### 6.9 Test
- `cargo test -p synthia-tool --all-features`
- Verify deprecation warnings only (no errors)

---

## Group 7: Service Trait + Registry (Phase 2a) — ~2 weeks

**Commit**: `feat(unified-registry): implement Service trait + ServiceRegistry + LoopServices`

### 7.1-7.4 Service trait hierarchy
File: `crates/synthia-service/src/traits.rs`
- `Service` trait: `name()`, `version()`, `init()`, `shutdown()`
- `StatefulService` with assoc type `State` (NOT dyn-compatible)
- `ErasedStatefulService` with `snapshot_json()`/`restore_json()` (dyn-compatible)
- Blanket impl bridging them via serde

File: `crates/synthia-service/src/provider.rs`
- `ServiceProvider` trait: `id()`, `list_services()`, `get_service()`, `dependencies()`
- `ServiceKey`, `ServiceDescriptor`, `ServiceCategory`, `ServiceState`, `ServiceError`

### 7.5-7.8 ServiceRegistry implementation
File: `crates/synthia-service/src/registry.rs`
- Dual index: `type_index` + `name_index` with `parking_lot::RwLock`
- `register_provider()` with `debug_assert!` TypeId validation
- `get::<Arc<dyn SubTrait>>()` — TypeId lookup + downcast
- `resolve(&str)` — string diagnostics
- `snapshot_all()` / `restore_all()` — async
- `state(&ServiceKey)` — lifecycle observation

### 7.9 OperationContext
File: `crates/synthia-service/src/context.rs`
- `OperationContext { cancellation, deadline, session_id, turn_id, user_id, agent_id }`
- `for_session()`, `child()` constructors

### 7.10 LoopServices
File: `crates/synthia-service/src/loop_services.rs`
- `LoopServices` struct with required + optional service fields
- `bootstrap()` — required: hard fail, optional: no-op + warning
- No-op stubs for all 10 optional services

---

## Group 8: Migrate Hot-Path Services (Phase 2a) — ~2 weeks

**Commit**: `feat(unified-registry): migrate Session, Hook, Permission, Memory to Service trait`

### 8.1 SessionService
File: `crates/synthia-session-v2/src/service.rs`
- `SessionService` subtrait: `create()`, `load()`, `append()`, `query()`, `fork()`, `compact()`, `rollback()`, `snapshot()`
- `impl Service for DefaultSessionService`
- Register in ServiceRegistry at agent init

### 8.2 HookService
File: `crates/synthia-hook/src/service.rs`
- `HookService` subtrait: `fire()`, `register_handler()`
- `impl Service for HookRegistry`

### 8.3 PermissionService
File: `crates/synthia-permission/src/service.rs`
- `PermissionService` subtrait: `evaluate()`, `request_approval()`, `record_session_rule()`, `snapshot_ruleset()`, `evaluate_doom_loop()`
- `PermissionDecision::PolicyStale` variant
- `PermissionRuleset::generation: AtomicU64` + 50-rule cap
- `evaluate_doom_loop()` routing `GuardianService::detect()` through policy
- `impl Service for MergedPolicy`

### 8.4 MemoryService
File: `crates/synthia-memory/src/service.rs`
- `MemoryService` subtrait: 4-tier methods + `consolidate()` + `snapshot()`
- `impl Service for DefaultMemoryService`

---

## Group 9: Refactor Main Loop (Phase 2a) — ~2 weeks

**Commit**: `feat(unified-registry): refactor AgentRunConfig + main_loop to use ServiceRegistry`

### 9.1-9.11 Replace discarded fields
For each of the 11 `_xxx` fields in `main_loop.rs`:
- Replace `_field` with `services.<service_field>.<method>()`
- Mark old field `#[deprecated]` in `AgentRunConfig`
- Add `#[allow(deprecated)]` on old field access temporarily

### 9.2 OperationContext threading
- Thread `op_ctx` through every tool, permission, hook, provider call
- Add cancellation check at yield points
- Add deadline check between turns

### 9.3 Goal status check
- Add `services.goal.status().await` at step 1a of loop
- Break on `GoalStatus::Blocked`

### 9.4 E2E comparison
- Run existing E2E suite with `unified-registry` enabled
- Compare behavior against baseline (without feature flag)

---

## Group 10: GoalService + RunCoordinator (Phase 2b) — ~1 week

**Commit**: `feat(unified-registry): add GoalService + SessionRunCoordinator`

### 10.1 GoalService
File: `crates/synthia-service/src/goal.rs`
- `GoalService` trait: `current()`, `set()`, `status()`, `budget()`
- `Goal`, `GoalStatus`, `GoalBudget` types
- `DefaultGoalService` (in-memory)
- `NoopGoalService` (always Active)

### 10.2 SessionRunCoordinator
File: `crates/synthia-service/src/coordinator.rs`
- `SessionRunCoordinator { inner: parking_lot::Mutex<HashMap<SessionId, RunState>> }`
- `run()`, `wake()`, `interrupt()`, `await_idle()`
- `RunGuard` with Drop → Idle transition
- Integration test for parallel subagent runs

---

## Group 11: Validation + Cleanup — ~3 days

**Commit**: `chore(unified-registry): format, lint, test, validate feature flag parity`

### 11.1 Full workspace check
- `cargo check --workspace --all-features`
- `cargo clippy --all-targets --all-features --tests --all`
- `cargo +nightly fmt --all`
- `cargo test --workspace --all-features`

### 11.2 Feature flag parity
- `cargo test --workspace` (without unified-registry) — must pass with deprecation warnings only

### 11.3 TypeId validation tests
- Register service with correct TypeId → passes
- Register service with incorrect TypeId → debug_assert fails

### 11.4 Materialization stale detection tests
- Take snapshot, reload provider, resolve → Stale
- Take snapshot, no changes, resolve → Ok

---

## Rollback Strategy

Any group can be reverted by:
1. `git revert <commit>` for that group
2. Disabling `unified-registry` feature flag in `Cargo.toml`
3. Old code path compiles and runs without the feature

---

## Estimated Timeline

| Group | Duration | Cumulative |
|-------|----------|------------|
| 0: Crate restructuring | 2 weeks | 2 weeks |
| 1: Tool trait | 1 week | 3 weeks |
| 2: ToolProvider + ToolRegistry | 1 week | 4 weeks |
| 3: Provider impls | 1 week | 5 weeks |
| 4: OutputBound | 3 days | 6 weeks |
| 5: CapabilityBroker | 2 days | 6.5 weeks |
| 6: Migrate tools | 1 week | 7.5 weeks |
| 7: Service trait + Registry | 2 weeks | 9.5 weeks |
| 8: Migrate services | 2 weeks | 11.5 weeks |
| 9: Refactor main loop | 2 weeks | 13.5 weeks |
| 10: GoalService + RunCoordinator | 1 week | 14.5 weeks |
| 11: Validation | 3 days | ~15 weeks |

**Total: ~15 weeks (~3.5 months)**

This matches the design's Phase 0 + Phase 1 + Phase 2a + Phase 2b estimate and is within the migration reviewer's adjusted budget for these phases.
