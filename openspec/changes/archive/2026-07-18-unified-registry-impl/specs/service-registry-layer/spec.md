## ADDED Requirements

### Requirement: Service Trait Contract
Every system-internal capability SHALL implement `Service` trait with `name(&self) -> &str`, `version()`, `init()`, and `shutdown()`. The trait SHALL NOT include `static_name()` — typed registration keys use `TypeId::of::<Arc<dyn SubTrait>>()`. String names are diagnostics-only.

#### Scenario: Service implements trait
- **WHEN** a service implements the `Service` trait
- **THEN** it SHALL provide `name()`, `version()`, `init()`, and `shutdown()` methods

#### Scenario: No static_name on Service
- **WHEN** code attempts to call `T::static_name()` on a Service type
- **THEN** compilation SHALL fail — the method does not exist on the trait

---

### Requirement: Service Registry Dual Index
`ServiceRegistry` SHALL maintain a `type_index: HashMap<TypeId, Arc<ServiceEntry>>` for typed O(1) resolution and a `name_index: HashMap<String, Vec<Arc<ServiceEntry>>>` for string-based diagnostics. Both SHALL use `parking_lot::RwLock` for concurrent read access.

#### Scenario: Typed service resolution
- **WHEN** `registry.get::<Arc<dyn SessionService>>()` is called
- **THEN** the TypeId index SHALL return the typed `Arc<dyn SessionService>` without downcasting

#### Scenario: String-based diagnostics
- **WHEN** `registry.resolve("memory")` is called
- **THEN** the name index SHALL return `Arc<dyn Service>` for introspection

---

### Requirement: TypeId Registration Validation
`ServiceRegistry::register_provider` SHALL validate under `debug_assertions` that the `Any` payload's `TypeId` matches the expected subtrait `TypeId`. Registration with an incorrectly-erased `Arc<dyn Service>` (instead of `Arc<dyn SubTrait>`) SHALL trigger a debug assertion failure.

#### Scenario: Correct TypeId registration
- **WHEN** a service is registered as `Arc<dyn SessionService>` wrapped in `Arc<dyn Any + Send + Sync>`
- **THEN** `debug_assert!` SHALL pass: payload TypeId equals `TypeId::of::<Arc<dyn SessionService>>()`

#### Scenario: Incorrect TypeId registration detected
- **WHEN** a service is incorrectly erased to `Arc<dyn Service>` before wrapping in Any
- **THEN** `debug_assert!` SHALL fail in debug builds, indicating TypeId mismatch

---

### Requirement: LoopServices Bootstrap with Required/Optional Split
`LoopServices::bootstrap` SHALL resolve required services (Session, Permission, Hook, Memory) with hard failure on missing, and optional services with no-op fallback + warning log. The result SHALL be cached in `OnceLock<LoopServices>` per `run_stream` call.

#### Scenario: Required service missing
- **WHEN** a required service (e.g., SessionService) is not in the registry
- **THEN** bootstrap SHALL return `AgentError::RequiredServiceMissing` with the `ServiceKey`

#### Scenario: Optional service missing
- **WHEN** an optional service (e.g., GoalService) is not in the registry
- **THEN** bootstrap SHALL substitute a no-op default and emit a `tracing::warn!`

#### Scenario: LoopServices cached per run
- **WHEN** `run_stream` is called
- **THEN** `LoopServices` SHALL be computed once and reused across all turns in the run

---

### Requirement: OperationContext Cancellation Propagation
`OperationContext` SHALL carry `cancellation: CancellationToken`, `deadline: Instant`, `session_id`, `turn_id`, `user_id`, and `agent_id`. It SHALL be threaded through every tool, permission, hook, and provider call. Tools SHALL honor `cancellation` at every yield point.

#### Scenario: Cancellation propagates to tools
- **WHEN** a cancellation token is triggered during tool execution
- **THEN** the tool SHALL check `ctx.cancellation.is_cancelled()` at yield points and abort

#### Scenario: Deadline enforced between turns
- **WHEN** `Instant::now() >= op_ctx.deadline` between turns
- **THEN** the loop SHALL break after failing interrupted tools

---

### Requirement: Service Lifecycle State Machine
Each service SHALL transition through `Constructed → Initializing → Initialized → Running → ShuttingDown → Dropped`. `ServiceRegistry::state(&ServiceKey)` SHALL return the current state for diagnostics.

#### Scenario: Service init failure observable
- **WHEN** a service's `init()` fails
- **THEN** its state SHALL remain `Initializing` and `ServiceRegistry::state()` SHALL return that state

---

### Requirement: StatefulService Erased View
`StatefulService` with associated type `State` SHALL NOT be dyn-compatible. An `ErasedStatefulService` trait with `snapshot_json()`/`restore_json()` SHALL provide the dyn-compatible view. A blanket impl SHALL bridge `StatefulService` → `ErasedStatefulService` via serde.

#### Scenario: Snapshot via erased view
- **WHEN** `snapshot_all()` is called on the registry
- **THEN** stateful services SHALL be snapshotted through `ErasedStatefulService::snapshot_json()` returning `serde_json::Value`
