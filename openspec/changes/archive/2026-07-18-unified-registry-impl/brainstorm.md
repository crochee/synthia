# Unified Registry Implementation — Brainstorm

> Source: `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` (1823 lines)
> + `docs/superpowers/specs/2026-07-18-synthia-design-review.md` (121 findings, 35 critical, 40 high)
> + Existing OpenSpec change `unified-registry-design-review-fixes` (design-doc corrections applied)

---

## Background

Synthia has accumulated architectural debt across 6 dimensions:

1. **3 parallel tool abstractions**: `Tool` (legacy 11-method trait), `ExecutableTool` (orchestrator), `ToolProvider` (dynamic provider)
2. **2 parallel hook systems**: `AgentHook` (in-process) + `HookRunner` (external subprocess)
3. **3 parallel event channels**: `mpsc::UnboundedSender`, `broadcast::Sender(128)`, `broadcast::Sender(256)` with different capacities/ordering
4. **11+ discarded `AgentRunConfig` fields**: all `_xxx`-prefixed and dropped at `main_loop.rs:124-162`
5. **5 unfired hooks**: `on_before_tool`, `on_after_tool`, `on_error`, `on_iteration_end`, `on_complete` declared but never called
6. **No materialization/stale-detection**: LLM gets tool list at step T, plugin unloads at T+1, resolve panics

The design document proposes a **4-layer architecture** (core/loop/service/tool) with unified registries. The adversarial review found 121 issues; the `unified-registry-design-review-fixes` change applied 16 High-severity corrections to the design document itself. This brainstorm covers the **implementation** decisions.

---

## Decision Chain

### Q1: What is the minimum viable scope for a first implementation pass?

**Context**: The full design spans 8 phases over 24-30 months (realistic estimate per migration reviewer). We need a tractable first pass.

**Options**:
1. **Full Phase 0-2** (~8 months): Crate restructuring + unified Tool + unified Service registries. Leaves hooks, events, plugins for later.
2. **Phase 0 + Phase 1** (~3 months): Crate restructuring + Tool trait unification. Service layer deferred.
3. **Phase 1 only** (~2 months): Tool unification only. Minimal crate changes.

**Decision**: **Phase 0 + Phase 1 + Phase 2 core**. Rationale:
- Phase 0 (crate restructuring) is prerequisite for everything — must happen first
- Phase 1 (Tool unification) is the highest-value change (eliminates 3→1 tool abstractions)
- Phase 2 core (ServiceRegistry + Service trait + LoopServices) is needed because the loop currently drops 11 fields — without the service layer, the tool layer can't access dependencies properly
- Phases 3-8 (hooks, events, plugins, streaming, MCP, migration) are deferred to follow-up changes

### Q2: Should we create `synthia-service` as a new crate or merge into an existing crate?

**Context**: The design proposes a new `synthia-service` crate for the `Service` trait + `ServiceRegistry`. But `synthia-agent` already has some service-like types.

**Options**:
1. **New crate `synthia-service`** (design recommendation)
2. **Merge into `synthia-core`** — Service trait is foundational
3. **Merge into `synthia-agent`** — agent is the primary consumer

**Decision**: **New crate `synthia-service`**. Rationale:
- Layering: `synthia-core` (Layer 1) must not depend on service types. `synthia-service` (Layer 3) depends only on `synthia-core`
- Testability: Service implementations can be tested without pulling in the agent
- The `synthia-agent` crate already has too many responsibilities; adding Service registry increases coupling
- Matches the design's layering rule: `service → [core]`

### Q3: Should the `ServiceRegistry` use `TypeId`-keyed resolution or string-keyed only?

**Context**: B1/B3 findings show `Arc::downcast` doesn't work on `Arc<dyn Service>`. The fix proposes `TypeId::of::<Arc<dyn SubTrait>>()` keyed registry.

**Options**:
1. **TypeId-keyed only** — type-safe, no string lookups on hot path
2. **String-keyed only** — simpler, but loses type safety
3. **Dual index** (TypeId + string) — TypeId for hot path, string for diagnostics/introspection

**Decision**: **Dual index** (design recommendation). Rationale:
- TypeId index gives O(1) typed resolution without downcasting on hot path
- String index enables diagnostics (`services.resolve("memory")` for debugging)
- `parking_lot::RwLock` for both indices; reads are ~µs uncontended
- Consistent with the design (§6.2)

### Q4: Should `ToolContext` carry `Arc<ServiceRegistry>` or `CapabilityBroker`?

**Context**: B5 finding: full `ServiceRegistry` in ToolContext allows cross-service privilege escalation. The fix proposes per-tool `ToolCapabilities` + `CapabilityBroker`.

**Options**:
1. **Full `Arc<ServiceRegistry>`** — simple, but security risk
2. **`CapabilityBroker`** — least privilege, but more boilerplate per tool
3. **Hybrid: `Arc<ServiceRegistry>` with audit logging** — keeps simple API, adds observability

**Decision**: **`CapabilityBroker`** (design recommendation). Rationale:
- Security B5 is blocking — a compromised tool can dump memory, whitelist itself, fork sessions
- `ToolCapabilities` is a simple `bool`-flag struct; default is all-false (pure function tools)
- Only tools that need services (e.g., `GrepTool` needs `MemoryService`) declare capabilities
- The overhead is minimal: one struct per tool registration

### Q5: How to handle the 12 existing services during migration?

**Context**: Phase 2 wraps 12 services as `impl Service`. Each needs: Service trait impl, dyn X resolution sites, mock impl, tests, deprecation warnings, migration doc.

**Options**:
1. **All 12 at once** — maximum consistency, but 5-6 months
2. **3-4 hot-path services first** (Session, Hook, Tool, Permission) — 2-3 months, rest deferred
3. **1 template service first** — prove the pattern, then replicate

**Decision**: **3-4 hot-path services first** (Session, Hook, Permission, Memory). Rationale:
- These are on the hot loop path — every turn touches them
- Proves the `Service` trait + `ServiceRegistry` + `LoopServices::bootstrap` pattern
- Remaining 8 services (Guardian, Skill, Command, Task, Telemetry, Context, Extension, ModelRouter) follow the proven template
- Matches Migration reviewer's recommendation to split Phase 2 into 2a (foundation + template) and 2b (remaining)

### Q6: Should `LoopServices` cache resolved services or re-resolve each turn?

**Context**: F54 finding: repeated `services.get()` on hot path. `LoopServices` caches once per run.

**Options**:
1. **Cache once per `run_stream` call** — fastest, but stale if service re-registers mid-run
2. **Cache once per turn** — allows service hot-swap between turns
3. **Never cache, resolve each call** — always fresh, but ~µs overhead per resolution

**Decision**: **Cache once per `run_stream` call**. Rationale:
- Service re-registration mid-run is extremely rare (only during plugin hot-reload)
- Hot-reload already invalidates Materialization; `PolicyStale` detection handles the permission case
- The cache saves ~100 `get()` calls per turn × ~µs each = significant on long sessions
- If mid-run service swap is needed, add `LoopServices::invalidate()` method

### Q7: What about the GoalService / RunCoordinator / PendingMessageQueue (B7)?

**Context**: B7 flags 5 missing production-agent patterns. The design adds GoalService but defers CodeMode.

**Options**:
1. **Add all 5 now** — comprehensive, but scope explosion
2. **Add GoalService + RunCoordinator only** — highest value (loop correctness)
3. **Defer all 5** — minimal scope, but loses correctness improvements

**Decision**: **Add GoalService + RunCoordinator only, defer rest**. Rationale:
- `GoalService` is small (~50 lines trait + impl) and directly addresses loop abort semantics
- `RunCoordinator` prevents race conditions in parallel subagent runs — a correctness issue
- `PendingMessageQueue` is subsumed by `SteeringService` (already in design)
- `CodeMode` (V8 JS runtime) is a major new dependency — explicitly deferred
- DoomLoop detection already exists via `GuardianService`; B7's concern is wiring, which Phase 3 handles

### Q8: How to validate the migration doesn't break existing behavior?

**Context**: 30 crates, ~100K lines of Rust. Behavioral regression during migration is the top risk.

**Options**:
1. **Feature-flag all new code** — old and new coexist, toggle at runtime
2. **Compile-time `cfg` flag** — new code only compiled with feature, but can't coexist in same binary
3. **Progressive replacement with E2E tests** — replace one service at a time, run E2E suite each step

**Decision**: **Feature-flag + E2E tests per service**. Rationale:
- `#[cfg(feature = "unified-registry")]` gates new trait impls; existing code compiles without the feature
- Each service migration has a corresponding E2E test that compares old vs new behavior
- `cargo test --all-features` runs both paths in CI
- Matches P5 (progressive degradation) — old path is always available as fallback

---

## Design Trade-offs

### Centralization vs Flexibility
- **Centralized registries** (ToolRegistry, ServiceRegistry) enable observability, stale detection, and capability control
- **Cost**: every capability must go through registration; no more ad-hoc `Arc<X>` field injection
- **Verdict**: net positive — the current ad-hoc injection is the root cause of 11 discarded fields

### Type Safety vs Dyn-compatibility
- **Type-safe `get::<Arc<dyn SubTrait>>()`** eliminates string lookups and downcasting
- **Cost**: each service subtrait must be registered with exact `TypeId`; accidental `Arc<dyn Service>` erasure is a runtime bug
- **Mitigation**: `debug_assert!` in `register_provider` validates TypeId consistency (§6.2)
- **Verdict**: type safety wins — the debug_assert catches misregistration at test time

### Sync vs Async Service Access
- **`PermissionService::evaluate` is sync** — hot path, no I/O, must not block
- **`PermissionService::request_approval` is async** — cold path, user interaction
- **`ServiceRegistry::get` is sync** — TypeId lookup under `parking_lot::RwLock::read` is ~µs
- **`Service::init` / `Service::shutdown` are async** — may do I/O
- **Verdict**: split sync/async by path temperature, not by uniform async

### Immutability vs LIFO Override
- **Core tool names are immutable** (refuse re-registration) — prevents shadowing (B6)
- **Plugin/local tools are LIFO** (last registration wins) — allows user overrides
- **Verdict**: core immutability + local LIFO balances security and flexibility

---

## Scope Boundaries

### In scope (this change)
- Phase 0: Crate restructuring (`synthia-service` new, `synthia-extension` new, `synthia-event` new)
- Phase 1: Unified Tool trait + ToolProvider + ToolRegistry + Materialization + BuiltinToolProvider + McpToolProvider
- Phase 2a: Service trait + ServiceRegistry + LoopServices + 4 hot-path services (Session, Hook, Permission, Memory)
- Phase 2b: GoalService + RunCoordinator (new)
- `ToolCapabilities` + `CapabilityBroker` (security fix B5)
- `ToolProvenance` namespacing + core immutability (security fix B6)
- Feature flag `unified-registry` for coexistence

### Deferred (follow-up changes)
- Phase 2c: Remaining 8 services (Guardian, Skill, Command, Task, Telemetry, Context, Extension, ModelRouter)
- Phase 3: Hook unification (15 events, HookService, HookHandler)
- Phase 4: Plugin unification (Plugin trait, ExtensionRegistry, PluginManifest)
- Phase 5: EventBus unification (replace 3 channels)
- Phase 6: Session v1 drop + Memory 4-tier refactor
- Phase 7: Streaming + MCP multi-transport
- Phase 8: Public API migration + docs
- CodeMode (V8 JS runtime) — indefinitely deferred
- Plugin sandboxing (WASM/seccomp) — security prerequisite but large scope

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| TypeId mismatch at registration | Medium | High (runtime None) | debug_assert + registration doc examples |
| Feature flag combinatorial explosion | Low | Medium | Pin CI to `--all-features` (2 configs) |
| Migration breaks E2E tests | Medium | High | Per-service E2E comparison; rollback via feature flag |
| `parking_lot::RwLock` contention | Low | Low | Read locks are ~µs; write locks only at registration |
| ToolMaterialization stale detection false positives | Low | Medium | `ToolGeneration` monotonic; only bumps on actual registry change |
| `LoopServices` cache staleness during plugin hot-reload | Low | Medium | `LoopServices::invalidate()` + Materialization stale detection |
| Scope creep into deferred phases | Medium | High | Strict scope boundaries; each deferred phase = separate OpenSpec change |
