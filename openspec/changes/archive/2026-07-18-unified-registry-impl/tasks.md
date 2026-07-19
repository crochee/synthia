## 0. Crate Restructuring (Phase 0)

- [x] 0.1 Create `synthia-service` crate with `Cargo.toml`, `lib.rs`, and module structure (`traits.rs`, `provider.rs`, `registry.rs`, `coordinator.rs`)
- [x] 0.2 Create `synthia-extension` crate skeleton: `Cargo.toml`, `lib.rs`, `manifest.rs` (type definitions only, no HookRunner migration)
- [x] 0.3 Create `synthia-event` crate skeleton: `Cargo.toml`, `lib.rs`, `bus.rs` (EventBus trait + AgentEvent types only, no channel replacement)
- [x] 0.4 Add all 3 new crates to workspace `Cargo.toml` and update dependency graph
- [x] 0.5 Add `unified-registry` feature flag to `synthia-core`, `synthia-tool`, `synthia-agent`, `synthia-permission`, `synthia-memory`, `synthia-hook`, `synthia-session-v2`, `synthia-mcp`
- [x] 0.6 Run `cargo check --workspace` and fix any dependency or compilation errors
- [x] 0.7 Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings

## 1. Unified Tool Trait (Phase 1)

- [x] 1.1 Define new `Tool` trait in `synthia-core/src/tool/mod.rs`: `name()`, `execute()`, `descriptor()` with `#[async_trait]` + `Send + Sync`
- [x] 1.2 Define `ToolDescriptor` struct with all fields: name, description, parameters, category, provenance, execution_mode, cancel_behavior, examples, permission_required, prompt_visible_provenance, is_hidden, is_user_invocable
- [x] 1.3 Define `ToolProvenance` enum: `Core`, `Plugin { id: PluginId }`, `Mcp { server, host_owned }`, `Context { source }`, `Dynamic` with `Clone, Debug, PartialEq, Eq, Hash` (NOT Copy)
- [x] 1.4 Define `ToolCategory`, `ExecutionMode`, `CancelBehavior` enums
- [x] 1.5 Define `ToolInput` struct (raw + parsed with `Send + Sync + 'static` bounds) and `ToolOutput` struct (content + structured + metadata + is_error)
- [x] 1.6 Define `ToolMetadata` struct (duration, tokens_in, tokens_out, truncated, managed_paths)
- [x] 1.7 Define `ToolError` enum with typed variants (CapabilityDenied, ExecutionFailed, Timeout, InvalidInput, etc.)
- [x] 1.8 Mark legacy `Tool` trait as `#[deprecated]` behind `#[cfg(not(feature = "unified-registry"))]`

## 2. Tool Provider + Registry (Phase 1)

- [x] 2.1 Define `ToolProvider` trait in `synthia-core/src/tool/provider.rs`: `id()`, `list_tools()`, `get_tool()`, `on_tool_event()`, `before_execute()`, `after_execute()` (NO pre_check)
- [x] 2.2 Define `ToolRegistry` struct with `inner: Arc<RwLock<HashMap<String, Vec<ToolEntry>>>>` and registration methods
- [x] 2.3 Define `ToolEntry` struct: `provider_id`, `tool: Arc<dyn Tool>`, `descriptor`, `identity: ToolIdentity`, `registration_token`
- [x] 2.4 Define `ToolIdentity` value type (`Clone, Debug, PartialEq, Eq`) with `name: String` + `generation: ToolGeneration(u64)`
- [x] 2.5 Implement `ToolRegistry::register_provider()` — atomic all-or-nothing, returns `RegistrationToken`
- [x] 2.6 Implement `ToolRegistry::unregister()` — removes all tools owned by token
- [x] 2.7 Implement `ToolRegistry::materialize(session_id, permissions)` — captures immutable snapshot with `ToolIdentity` clones
- [x] 2.8 Implement `ToolRegistry::resolve(mat, name)` — returns `Result<Arc<dyn Tool>, StaleOrUnknown>` with stale detection
- [x] 2.9 Implement `ToolRegistry::resolve_now(name)` — no-snapshot resolution for non-LLM callers
- [x] 2.10 Add `internal_consistency_check()` under `#[cfg(debug_assertions)]` — validates `list_tools`/`get_tool` bidirectional agreement

## 3. Tool Provider Implementations (Phase 1)

- [x] 3.1 Implement `BuiltinToolProvider` with `applications` + `local` dual map
- [x] 3.2 Implement `BuiltinToolProvider::register_builtin()` — refuse `CoreNameTaken` on duplicate
- [x] 3.3 Implement `BuiltinToolProvider::add_local()` / `remove_local()` — mutable local additions
- [x] 3.4 Implement `McpToolProvider` with `Arc<dyn McpConnection>` (NOT `Arc<dyn McpTransport>`)
- [x] 3.5 Define `McpConnection` trait (object-safe): `connect()`, `close()` with `#[async_trait]`
- [x] 3.6 Define `McpTransportConfig` enum (Stdio, StreamableHttp, WebSocket) — config, NOT trait
- [x] 3.7 Implement `PluginToolProvider` with `plugin_id` + `prompt_visible_provenance` + `namespaced_name()`
- [x] 3.8 Implement `SkillToolProvider` delegating to `SkillRegistry`
- [x] 3.9 Implement `SubagentToolProvider` delegating to `SubagentSessionFactory`
- [x] 3.10 Implement `DynamicToolProvider` for script-based tools

## 4. Output Bound + Sanitization (Phase 1)

- [x] 4.1 Define `OutputBound` struct with `per_call_max_bytes` (50 KiB), `per_call_max_lines` (2000), `per_session_max_bytes` (4 MiB), `managed_dir`, `overflow_strategy`, `retention` (7d), `cleanup_interval` (1h), `sanitization`
- [x] 4.2 Implement `OutputBound::default()` matching opencode values
- [x] 4.3 Define `OverflowStrategy` enum: `TruncateHeadTail`, `TruncateHead`, `AlwaysSpill`
- [x] 4.4 Define `SanitizationPolicy` enum: `StripControlChars`, `WrapUntrusted`, `RedactUrlsMatching`
- [x] 4.5 Implement `ToolRegistry::bound_output()` as `async fn` — truncation + tokio::fs spill
- [x] 4.6 Implement managed file retention cleanup task (7d default, 1h interval)

## 5. ToolCapabilities + CapabilityBroker (Phase 1)

- [x] 5.1 Define `ToolCapabilities` struct with boolean flags: `memory_read`, `memory_write`, `session_fork`, `permission_record`, `hook_emit`, `telemetry_record`, `skill_invoke`, `command_invoke`
- [x] 5.2 Implement `Default` for `ToolCapabilities` (all false)
- [x] 5.3 Define `CapabilityBroker` struct — thin wrapper typed by `ToolCapabilities`
- [x] 5.4 Implement `CapabilityBroker` methods — each checks capability flag, returns `ToolError::CapabilityDenied` if false
- [x] 5.5 Update `ToolContext` to carry `capabilities: CapabilityBroker` (NOT `Arc<ServiceRegistry>`)

## 6. Migrate Built-in Tools (Phase 1)

- [x] 6.1 Migrate `ReadTool` to new `Tool` trait (name, execute, descriptor with `cached_descriptor`)
- [x] 6.2 Migrate `WriteTool` to new `Tool` trait
- [x] 6.3 Migrate `BashTool` to new `Tool` trait
- [x] 6.4 Migrate `GrepTool` to new `Tool` trait with `memory_read: true` capability
- [x] 6.5 Migrate `EditTool` to new `Tool` trait
- [x] 6.6 Migrate `ShellTool` to new `Tool` trait
- [x] 6.7 Migrate `ListTool` / `TreeTool` to new `Tool` trait
- [x] 6.8 Register all migrated tools via `BuiltinToolProvider::register_builtin()`
- [x] 6.9 Run `cargo test -p synthia-tool --all-features` and fix failures

## 7. Service Trait + Registry (Phase 2a)

- [x] 7.1 Define `Service` trait in `synthia-service/src/traits.rs`: `name()`, `version()`, `init()`, `shutdown()` with `#[async_trait]`
- [x] 7.2 Define `ServiceProvider` trait: `id()`, `list_services()`, `get_service()`, `dependencies()`
- [x] 7.3 Define `ServiceDescriptor`, `ServiceCategory`, `ServiceKey`, `ServiceState` enums/types
- [x] 7.4 Define `ServiceError` enum with typed variants (Serialization, InitFailed, DependencyMissing, NotFound, StateInvalid, CapabilityDenied)
- [x] 7.5 Define `StatefulService` trait with associated type `State` (NOT dyn-compatible)
- [x] 7.6 Define `ErasedStatefulService` trait with `snapshot_json()`/`restore_json()` (dyn-compatible)
- [x] 7.7 Implement blanket `ErasedStatefulService` for `StatefulService` via serde bridge
- [x] 7.8 Implement `ServiceRegistry` with `type_index` (TypeId → ServiceEntry) + `name_index` (String → Vec<ServiceEntry>) using `parking_lot::RwLock`
- [x] 7.9 Implement `ServiceRegistry::register_provider()` with `debug_assert!` TypeId validation
- [x] 7.10 Implement `ServiceRegistry::get::<Arc<dyn SubTrait>>()` — TypeId lookup + downcast
- [x] 7.11 Implement `ServiceRegistry::resolve(&str)` — string-based diagnostics
- [x] 7.12 Implement `ServiceRegistry::snapshot_all()` / `restore_all()` — async stateful service persistence
- [x] 7.13 Implement `ServiceRegistry::state(&ServiceKey)` — lifecycle state observation

## 8. LoopServices + OperationContext (Phase 2a)

- [x] 8.1 Define `OperationContext` struct: `cancellation`, `deadline`, `session_id`, `turn_id`, `user_id`, `agent_id`
- [x] 8.2 Implement `OperationContext::for_session()` and `OperationContext::child()`
- [x] 8.3 Define `LoopServices` struct with required (session, permission, hooks, memory) + optional (guardian, goal, steering, agent_control, context, sandbox, extension, model_router, skill, command, task, telemetry) service fields
- [x] 8.4 Implement `LoopServices::bootstrap()` — required: hard fail; optional: no-op fallback + warning
- [x] 8.5 Implement no-op service stubs for all 10 optional services
- [x] 8.6 Add `loop_services: OnceLock<LoopServices>` to `AgentRunConfig`
- [x] 8.7 Refactor `Agent::run_stream` to call `LoopServices::bootstrap()` at entry and cache in `OnceLock`

## 9. Migrate Hot-Path Services (Phase 2a)

- [x] 9.1 Define `SessionService` subtrait extending `Service`: `create()`, `load()`, `append()`, `query()`, `fork()`, `compact()`, `rollback()`, `snapshot()`
- [x] 9.2 Implement `impl Service for DefaultSessionService` (v2) with `#[cfg(feature = "unified-registry")]`
- [x] 9.3 Register `SessionService` in `ServiceRegistry` at agent init
- [x] 9.4 Define `HookService` subtrait: `fire()`, `register_handler()`
- [x] 9.5 Implement `impl Service for HookRegistry` with `#[cfg(feature = "unified-registry")]`
- [x] 9.6 Register `HookService` in `ServiceRegistry` at agent init
- [x] 9.7 Define `PermissionService` subtrait: `evaluate()`, `request_approval()`, `record_session_rule()`, `snapshot_ruleset()`, `evaluate_doom_loop()`
- [x] 9.8 Implement `impl Service for MergedPolicy` with `#[cfg(feature = "unified-registry")]`
- [x] 9.9 Add `PermissionDecision::PolicyStale` variant with generation fields
- [x] 9.10 Add `PermissionRuleset::generation: AtomicU64` counter + 50-rule cap
- [x] 9.11 Implement `evaluate_doom_loop()` routing `GuardianService::detect()` through policy pipeline
- [x] 9.12 Register `PermissionService` in `ServiceRegistry` at agent init
- [x] 9.13 Define `MemoryService` subtrait: `hot_set()`, `hot_get()`, `cold_store()`, `cold_search()`, `episodic_record()`, `episodic_replay()`, `context_search()`, `consolidate()`, `snapshot()`
- [x] 9.14 Implement `impl Service for DefaultMemoryService` with `#[cfg(feature = "unified-registry")]`
- [x] 9.15 Register `MemoryService` in `ServiceRegistry` at agent init

## 10. Refactor Main Loop (Phase 2a)

- [x] 10.1 Replace `_subagent_session_factory` with `services.agent_control` in `main_loop.rs`
- [x] 10.2 Replace `_sandbox_manager` with `services.sandbox`
- [x] 10.3 Replace `_extension_manager` with `services.extension`
- [x] 10.4 Replace `_approval_service` with `services.permission`
- [x] 10.5 Replace `_guardian_coordinator` with `services.guardian`
- [x] 10.6 Replace `_model_router` with `services.model_router`
- [x] 10.7 Replace `_fork_policy` with `AgentRole` config field
- [x] 10.8 Replace `_compaction_provider` with `services.context`
- [x] 10.9 Replace `_steering_channel` with `services.steering`
- [x] 10.10 Replace `_context_assembler` with `services.context`
- [x] 10.11 Replace `_tool_orchestrator` with `ToolRegistry` + Orchestrator default impl
- [x] 10.12 Add `OperationContext` threading through loop steps: cancellation + deadline checks
- [x] 10.13 Add deadline check between turns (`Instant::now() >= op_ctx.deadline`)
- [x] 10.14 Add goal status check at step 1a (`services.goal.status().await`)
- [x] 10.15 Run E2E tests comparing old vs new path behavior

## 11. GoalService + RunCoordinator (Phase 2b)

- [x] 11.1 Define `GoalService` trait: `current()`, `set()`, `status()`, `budget()`
- [x] 11.2 Define `Goal`, `GoalStatus`, `GoalBudget` types
- [x] 11.3 Implement `DefaultGoalService` (in-memory, per-session)
- [x] 11.4 Implement `NoopGoalService` (always Active, never blocks)
- [x] 11.5 Define `SessionRunCoordinator` with `inner: parking_lot::Mutex<HashMap<SessionId, RunState>>`
- [x] 11.6 Define `RunState` enum: `Idle`, `Running { run_id }`, `Interrupted { at }`
- [x] 11.7 Implement `SessionRunCoordinator::run()` — returns `RunGuard` or `AlreadyRunning`
- [x] 11.8 Implement `SessionRunCoordinator::wake()` — returns `RunId` or `NoSuchRun`
- [x] 11.9 Implement `SessionRunCoordinator::interrupt()` — trips cancellation token
- [x] 11.10 Implement `SessionRunCoordinator::await_idle()` — blocks until Idle
- [x] 11.11 Implement `RunGuard` Drop (transitions to Idle)
- [x] 11.12 Add `SessionRunCoordinator` integration test for parallel subagent runs

## 12. Validation + Cleanup

- [x] 12.1 Run `cargo check --workspace --all-features` and fix all errors
- [x] 12.2 Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings
- [x] 12.3 Run `cargo +nightly fmt --all`
- [x] 12.4 Run `cargo test --workspace --all-features` and fix all failures
- [x] 12.5 Verify feature flag toggle: `cargo test --workspace` (without unified-registry) passes with deprecation warnings only
- [x] 12.6 Verify E2E test parity between old and new paths
- [x] 12.7 Add `debug_assert!` TypeId validation tests for ServiceRegistry
- [x] 12.8 Add Materialization stale detection integration test
