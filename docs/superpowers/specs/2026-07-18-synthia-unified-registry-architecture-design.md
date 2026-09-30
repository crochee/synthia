---
status: archive
scope: historical-design-intent
as_of: 2026-08-01
note: Records design intent for the unified registry architecture. Some referenced crates (synthia-exec, synthia-mcp, synthia-memory, synthia-permission, synthia-evaluation, synthia-service) were removed or never built. For the current architecture, see openspec/specs/ and README.md.
---

# Synthia Unified Registry Architecture — Design

> **Date**: 2026-07-18
> **Status**: Draft (post-brainstorming)
> **Scope**: Comprehensive tool-ification + 4-layer architecture + dynamic trait-object registry
> **Audience**: synthia maintainers, contributors, downstream users
> **Source inputs**: 4 background deep-analyses (synthia/opencode/codex/pi-mono) + 4 existing inbox reports + v3 multi-expert analysis + multiple archived OpenSpec changes

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Problem Statement & Goals](#2-problem-statement--goals)
3. [Architectural Overview](#3-architectural-overview)
4. [Crate Restructuring](#4-crate-restructuring)
5. [Tool Trait Refactor + Provider Registry](#5-tool-trait-refactor--provider-registry)
6. [Service Trait Layer + Registry](#6-service-trait-layer--registry)
7. [Loop Layer Refactor (Preserve Main Logic)](#7-loop-layer-refactor-preserve-main-logic)
8. [Session / Memory / Permission / Hook Refactor](#8-session--memory--permission--hook-refactor)
9. [Extension / Hook / Plugin Unification](#9-extension--hook--plugin-unification)
10. [Event + Protocol + Streaming](#10-event--protocol--streaming)
11. [Migration Plan + Risk Assessment](#11-migration-plan--risk-assessment)
12. [Success Metrics](#12-success-metrics)
13. [Open Questions & Decisions Log](#13-open-questions--decisions-log)
14. [Appendix A: Source Code References](#appendix-a-source-code-references)
15. [Appendix B: Decision Rationale](#appendix-b-decision-rationale)

---

## 1. Executive Summary

### TL;DR

Synthia currently has **3 parallel tool abstractions**, **2 parallel hook systems**, **3 parallel event channels**, and **11+ discarded `AgentRunConfig` fields** — all symptoms of the same architectural root cause: **abundance of traits without unifying registries**.

This design consolidates everything into a **4-layer architecture** (core/loop/service/tool) where every capability is registered behind a `dyn Trait + Registry` pattern, the ReAct loop is the only orchestrator, and the rest is dynamic dispatch. The main logic (ReAct loop + session) is **preserved** — only the dependency injection surface is refactored.

### Key Decisions

1. **Tool is first-class citizen** — every LLM-invokable capability is a `Tool` registered via `ToolProvider` + `Materialization`-based `ToolRegistry`.
2. **Service is the dual of Tool** — every system-internal capability (Session, Memory, Permission, Guardian, Hook, Skill, etc.) is a `Service` registered via `ServiceProvider` + `ServiceRegistry`.
3. **Loop is minimal** — the ReAct loop body is preserved; only the 11+ discarded fields are restored as service resolutions.
4. **Plugin = bundle of registries** — a single `Plugin` trait replaces 3 parallel extension systems.
5. **EventBus replaces 3 channels** — durable/ephemeral classification + `LlmEvent` provider-agnostic union + `StreamFn` push-stream.

### Outcomes (12-15 months, 8 phases)

| Metric | Before | After |
|--------|--------|-------|
| Crates | 30 | 25 (4 layers) |
| Tool abstractions | 3 parallel | 1 unified |
| Hook systems | 2 parallel | 1 unified |
| Event channels | 3 parallel | 1 unified |
| `AgentRunConfig` discarded fields | 11 | 0 |
| Wired hooks | 2 of 7 | 14 of 14 |
| New capability add | code change required | plugin manifest |
| Provider streaming | callback | push-stream (`StreamFn`) |
| MCP transports | stdio only | stdio + http + ws |
| Server backpressure | none | `-32001` |

---

## 2. Problem Statement & Goals

### 2.1 Problems (from baseline + v3 multi-expert + 4 deep-analyses)

#### Internal inconsistency
- **3 parallel tool abstractions**: `Tool` (legacy 11-method trait), `ExecutableTool` (orchestrator), `ToolProvider` (dynamic provider). The same tool must implement multiple traits to participate in different registries.
- **2 parallel hook systems**: `AgentHook` (`synthia-hook`, 7 events, in-process) and `HookRunner` (`synthia-plugin`, external subprocess). Plugin authors must choose; semantics overlap.
- **3 parallel event channels**: `agent/events/emitter.rs` (`mpsc::UnboundedSender`), `server/event_stream.rs` (`broadcast::Sender(128)`), `orchestrator/lib.rs` (`broadcast::Sender(256)`). Different capacities, different ordering guarantees.
- **11+ discarded `AgentRunConfig` fields**: `subagent_session_factory`, `sandbox_manager`, `extension_manager`, `approval_service`, `guardian_coordinator`, `model_router`, `fork_policy`, `compaction_provider`, `steering_channel`, `context_assembler`, `tool_orchestrator`. All prefixed `_xxx` and dropped at `main_loop.rs:124-162`.
- **5 unfired hooks**: `on_before_tool`, `on_after_tool`, `on_error`, `on_iteration_end`, `on_complete` declared but never called.

#### External gaps (vs opencode/codex/pi-mono)
- **No tool-materialization stale detection**: LLM gets tool list at step T; plugin unloads at T+1; resolve panics or falls through. opencode's `Materialization` solves this.
- **No provider-agnostic streaming**: `complete_with_stream` uses callback, no push-stream. opencode's `LLMEvent` union is the model.
- **No durable/ephemeral event classification**: every event has `is_durable()` hardcoded per variant. opencode's `Event::SYNC` attribute is data-driven.
- **No tool provenance**: all tools indistinguishable by source. codex's `ToolPluginProvenance` enum (core/plugin/mcp/context) is the model.
- **No per-plugin scope/lifetime**: hot-unload leaves dangling subscriptions. opencode's `Scope.fork` per plugin is the model.
- **No server backpressure**: `synthia-server` doesn't return `-32001` on saturation. codex's app-server does.
- **No MCP streamable-http**: only stdio. codex supports stdio + streamable-http + WebSocket.
- **No `Usage` with non-overlapping fields**: synthia subtracts cache tokens downstream. opencode's `Usage` class makes fields non-overlapping.

### 2.2 Goals

#### Functional goals
1. **Single unified Tool abstraction** — every LLM-invokable capability is one trait.
2. **Single unified Service abstraction** — every system-internal capability is one trait.
3. **Single unified Hook system** — replace 2 parallel systems.
4. **Single unified Event bus** — replace 3 parallel channels.
5. **Single unified Plugin system** — replace 3 parallel extension surfaces.
6. **Restore all 11 discarded `AgentRunConfig` fields** as service resolutions.
7. **Wire all 7 (now 14) hook events**.
8. **Provider push-streaming** with `LlmEvent` union.
9. **MCP multi-transport** (stdio + http + ws).
10. **Server backpressure** with `-32001`.

#### Non-functional goals
1. **Backward compatibility** — no breaking change without 1-release deprecation window.
2. **Zero-cost abstraction** — service resolution + tool materialization < 1µs per call (cached).
3. **Type-safe extension** — plugin authors get compile errors for API drift.
4. **Observable** — every significant operation emits an `AgentEvent`.
5. **Testable** — every trait has a mock impl + integration test fixture.
6. **Discoverable** — plugin manifest is the single source of truth for capabilities.

### 2.3 Non-goals

1. **No new external dependencies** beyond existing tokio/serde/etc.
2. **No SQLite mandate** (Phase 0 hard constraint preserved).
3. **No seccomp** (landlock is fallback).
4. **No system-prompt toolification** (user decision — system layer, not tool).
5. **No permission-policy toolification** (security-sensitive, must remain hook-intercepted).
6. **No TUI redesign** — TUI is layered on top; can adopt incrementally.
7. **No breaking config format change** — TOML config stays parse-compatible.

---

## 3. Architectural Overview

### 3.1 Guiding Principles

**Authoritative source for P1-P10**: `AGENTS.md` (root of this repo) lists the 10
design principles in priority order: *prefix consistency > append-only > interruptibility
> distrust LLM > progressive degradation > lazy loading > recency anchoring > no information loss > observability > file as memory*.

> **Note**: `.trae/rules/agent_rule.md` is the canonical location for the
> full P1-P10 prose (per AGENTS.md's "MUST READ before any task" rule). At the
> time of this design's writing, that file is not present in the working
> tree; only `AGENTS.md`'s enumerated list is available. The principle
> *names* used here come from AGENTS.md and are used as design anchors
> rather than as full ruleset citations.

#### How each P-number anchors this design

| P# | Principle (AGENTS.md) | How it anchors this design |
|----|-----------------------|----------------------------|
| P1 | prefix consistency | Every `Tool` name validated against `^[a-zA-Z][a-zA-Z0-9_-]{0,63}$` at registration (§5.2). Every `Service` has `static_name()`. Every `Plugin` has kebab-case `PluginId`. Single source of truth = manifest. |
| P2 | append-only | Session storage is append-only JSONL (§8.1 `SessionStorage` trait). Memory cold/episodic tiers append-only. `Materialization` snapshots are immutable. `Event::SYNC` events never mutated, only versioned. |
| P3 | interruptibility | Every long-running op honors `CancellationToken` (`ToolContext::cancellation`, `PluginHandle::cancel_token`, `HookHandler::execute` all check). `CancelBehavior::Interrupt` vs `AwaitCompletion` gives tools explicit policy. |
| P4 | distrust LLM | Tool outputs go through `bound_output` truncation + managed-path spill (§5.2 `OutputBound`). Doom-loop detector fires on tool-call events. Permission `evaluate` is sync + infallible (§8.3). Hook handlers can `FailedAbort`. |
| P5 | progressive degradation | Backward-compat: old traits `#[deprecated]`, new traits coexist, feature flags gate migration (§11.1). `HookOutcome::FailedContinue` keeps turn alive on handler error. Server `-32001` backpressure instead of drop. |
| P6 | lazy loading | Plugins loaded on first reference (`ExtensionRegistry::load` triggered by `PluginContext`). Services in `Materialization` only when snapshot taken. `OnceCell<ToolDescriptor>` caches descriptors (§5.2 fix #2). |
| P7 | recency anchoring | `LIFO ToolRegistry` (last registration wins) lets users override built-in tools without forking. `Materialization` captures the LLM's view at step T, not step T-1. EventBus durable log = recent history. |
| P8 | no information loss | Truncated tool output spills to managed file (§5.2 `OutputBound::AlwaysSpill`). `HookOutcome::FailedContinue` logs error. `Event::SYNC` with versioned schema means history survives schema bumps. |
| P9 | observability | Every significant op emits an `AgentEvent` (§10.2). OTel integration via `TelemetryService` (§6.3). `ToolMetadata` carries duration + tokens + truncated flag. Hook firing is itself a hookable event. |
| P10 | file as memory | Plugin manifest is a file. Session is a JSONL file. Memory cold/episodic are files. `Materialization` snapshots are files. Hot tier stays in-memory by design (recency, not durability). |

#### 5 design principles distilled from the above

1. **Tool is the first-class citizen** — every capability the LLM can invoke is a Tool; every system service is registered behind a trait+registry. *(anchored by P1, P7)*
2. **Loop stays minimal** — the ReAct loop is the only orchestrator; everything else is a Registry lookup + dispatch. *(anchored by P4, P9)*
3. **Dynamic registration over static enum** — `dyn Trait + Registry` allows unlimited extension without code changes. *(anchored by P1, P6, P10)*
4. **Strict layering** — `core` knows nothing of `loop`; `loop` knows nothing of `tool`; `tool` knows nothing of `service` implementations. *(anchored by P1, P8)*
5. **Backward compatibility during transition** — old trait APIs marked `#[deprecated]`, new APIs coexist, runtime migration via feature flags. *(anchored by P5)*

### 3.2 4-Layer Architecture Diagram

```
┌──────────────────────────────────────────────────────────┐
│  Layer 4 — Tool & Frontend                               │
│    - ToolProvider trait (dyn dispatch)                  │
│    - Built-in Tools (Read/Write/Bash/Grep/...)          │
│    - MCP Tools (streamable-http + stdio)                │
│    - Plugin Tools (extension manifest)                  │
│    - Frontend (CLI / Server / TUI / Web)                │
├──────────────────────────────────────────────────────────┤
│  Layer 3 — Service (the capabilities registry)          │
│    - SessionService, MemoryService, PermissionService   │
│    - GuardianService, HookService, ExtensionService     │
│    - All exposed via ServiceProvider trait              │
│    - Default impls + user-overridable implementations   │
├──────────────────────────────────────────────────────────┤
│  Layer 2 — Loop (the ReAct orchestrator)                │
│    - StreamBuilder (preserved main_loop.rs)             │
│    - LoopContext (state machine)                        │
│    - Turn lifecycle (Start/Sample/Execute/End)          │
│    - Defect channel (TurnTransition)                    │
│    - LLM event consumption only                         │
├──────────────────────────────────────────────────────────┤
│  Layer 1 — Core (the type foundation)                   │
│    - Provider, Message, ToolCall, Content types         │
│    - Event enum (universal contract)                    │
│    - Identifier types (SessionId, TurnId, ToolCallId)   │
│    - Error types, Schema types                          │
└──────────────────────────────────────────────────────────┘
```

### 3.3 Layer Dependency Rules (enforced by `clippy.toml` + `cargo-deny`)

```toml
# Allowed dependencies (only ↓)
core     → []
loop     → [core]
service  → [core]                  # service uses core types only (Fix: Architect H9)
tool     → [core, loop, service]   # tool can invoke services
frontend → [all]
```

Reverse dependencies (e.g., `core` depending on `service`) are **forbidden**. Existing crates that violate this get refactored.

> **Fix: Architect H9 — Services must not depend on `loop`.**
> `service → [core, loop]` was relaxed to `service → [core]` because:
>
> 1. **Testability.** A service that depends on `loop` cannot be exercised by a
>    pure integration test that constructs a `ServiceRegistry` without
>    `LoopServices`. Loops are a runtime concept; services are configuration-time
>    constructors and must be buildable in isolation (P5: progressive
>    degradation).
> 2. **Compile-time guarantee against cyclic dependency.** `loop` derives its
>    session/turn context from services via resolution. If services depended on
>    `loop`, resolution becomes undecidable at registration time. `clippy.toml`
>    `disallowed-methods` + `cargo-deny` graph check enforces this.
> 3. **Read-only access claim was overstated.** The historical reasoning
>    ("service can read loop state") conflated *observation* with *dependency*.
>    Loop state is exposed to services via `OperationContext` (see §6.2) — a
>    `core`-level type carried across boundaries, not a `loop`-level
>    constructed graph.
>
> The `OperationContext { cancellation, deadline, session_id, turn_id }` flows
> *through* every service call without introducing an upward dependency.

---

## 4. Crate Restructuring

### 4.1 Current State (~30 crates)

```
synthia-core          synthia-telemetry     synthia-hook
synthia-provider      synthia-context       synthia-tool
synthia-permission    synthia-tool-orchestrator
synthia-sandbox       synthia-tool-bash     synthia-tool-exec-base
synthia-exec          synthia-mcp           synthia-command
synthia-skill         synthia-session       synthia-memory
synthia-task          synthia-agent         synthia-evaluation
synthia-cli           synthia-server        synthia-job
synthia-e2e           synthia-plugin        synthia-message-proxy
synthia-cache-mark    synthia-session-v2
synthia-model-router  (test-support)
```

### 4.2 Target State (~25 crates, organized by 4 layers)

| Layer | Crates | Role |
|-------|--------|------|
| **Core** (Layer 1) | `synthia-core`, `synthia-provider`, `synthia-cache-mark`, `synthia-telemetry` | Pure types, no domain logic |
| **Loop** (Layer 2) | `synthia-agent`, `synthia-session-v2`, `synthia-message-proxy` | ReAct loop, turn lifecycle, event emission |
| **Service** (Layer 3) | `synthia-service` (NEW), `synthia-permission`, `synthia-memory`, `synthia-guardian`, `synthia-context`, `synthia-hook`, `synthia-task`, `synthia-command`, `synthia-skill`, `synthia-evaluation`, `synthia-job` | All services behind `ServiceProvider` trait + registry |
| **Tool** (Layer 4) | `synthia-tool`, `synthia-tool-orchestrator`, `synthia-tool-bash`, `synthia-tool-exec-base`, `synthia-mcp`, `synthia-plugin` (merges into extension), `synthia-sandbox`, `synthia-model-router`, `synthia-extension` (NEW), `synthia-event` (NEW) | ToolProvider trait + registries + builtin/MCP/plugin impls |
| **Frontend** | `synthia-cli`, `synthia-server` | Host binaries |

### 4.3 Restructuring Moves

1. **`synthia-service` (NEW)** — central `ServiceProvider` trait + `ServiceRegistry`. All current services become implementors.
2. **`synthia-extension` (NEW)** — central `Plugin` trait + `ExtensionRegistry`. Replaces `synthia-plugin`'s `HookRunner` and `synthia-agent`'s `ExtensionManager`.
3. **`synthia-event` (NEW)** — central `EventBus` + `AgentEvent` + `LlmEvent` union. Replaces 3 parallel channels.
4. **`synthia-tool` consolidates `synthia-tool-orchestrator`** — orchestrator becomes default impl of `ToolProvider` trait.
5. **`synthia-session` collapses to v2** — v1 deprecated, v2 becomes sole impl.
6. **`synthia-exec` already split** — `synthia-tool-bash` + `synthia-tool-exec-base`.
7. **`synthia-plugin` merges into `synthia-extension`** — manifest becomes single source of truth.

---

## 5. Tool Trait Refactor + Provider Registry

### 5.1 Core Problem

Current synthia has 3 parallel tool abstractions:
- `Tool` trait (legacy, 11 methods, in `synthia-tool/src/traits.rs`)
- `ExecutableTool` trait (in `synthia-tool-orchestrator`)
- `ToolProvider` trait (in `synthia-agent/src/tools/dynamic_provider`)

Plus 2 registry layers:
- `ToolRegistry` (flat HashMap)
- `ScopedToolRegistry` (RAII cleanup, but unused)

### 5.2 Target: Single Unified Tool System

#### `Tool` Trait (the execution contract)

```rust
// crates/synthia-core/src/tool/mod.rs (new file)

/// Unified tool execution contract. Every tool, regardless of source,
/// implements this trait. The trait is intentionally minimal —
/// description/metadata is exposed via `ToolDescriptor`.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Stable identifier. ASCII letter start, `[a-zA-Z0-9_-]`, ≤64 chars.
    fn name(&self) -> &str;

    /// Execute the tool. CancellationToken MUST be honored at yield points.
    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError>;

    /// Rich descriptor for the LLM (description + JSON schema + examples).
    /// Default impl returns the value cached via `cached_descriptor()`.
    /// Override to return a `&'static ToolDescriptor` if your tool has
    /// zero-allocation metadata (preferred for built-in tools).
    fn descriptor(&self) -> &ToolDescriptor {
        self.cached_descriptor()
    }

    /// Cached descriptor accessor. Default impl builds a synthetic descriptor
    /// from `name()` and caches it in a thread-safe `OnceCell` on `self`.
    /// Tools that want a richer descriptor override `cached_descriptor()`
    /// (preferred) OR `descriptor()` (for `&'static` cases).
    ///
    /// **Why two methods?** Splitting cache and accessor lets the trait
    /// default impl compose without requiring every Tool to know about
    /// `OnceCell`. Tools that want richer metadata override the cache layer
    /// (`cached_descriptor`) and the default `descriptor()` Just Works.
    fn cached_descriptor(&self) -> &ToolDescriptor;
}

// Example concrete impl:
//
//     pub struct ReadTool {
//         cached: OnceCell<ToolDescriptor>,
//         fs: Arc<dyn FileSystem>,
//     }
//
//     impl Tool for ReadTool {
//         fn name(&self) -> &str { "read" }
//
//         fn cached_descriptor(&self) -> &ToolDescriptor {
//             self.cached.get_or_init(|| ToolDescriptor {
//                 name: self.name().to_string(),
//                 description: "Read a file from the workspace".into(),
//                 parameters: serde_json::json!({
//                     "type": "object",
//                     "properties": {
//                         "path": {"type": "string", "description": "Absolute path"},
//                         "limit": {"type": "integer", "default": 2000},
//                     },
//                     "required": ["path"],
//                 }),
//                 category: ToolCategory::FileSystem,
//                 provenance: ToolProvenance::Core,
//                 execution_mode: ExecutionMode::Read,
//                 cancel_behavior: CancelBehavior::Interrupt,
//                 examples: vec![ToolExample { /* ... */ }],
//                 permission_required: false,
//             })
//         }
//
//         async fn execute(&self, input: ToolInput, ctx: &ToolContext)
//             -> Result<ToolOutput, ToolError> { /* ... */ }
//     }

/// Rich metadata for LLM-facing tool advertisement.
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,      // JSON Schema
    pub category: ToolCategory,
    pub provenance: ToolProvenance,
    pub execution_mode: ExecutionMode,
    pub cancel_behavior: CancelBehavior,
    pub examples: Vec<ToolExample>,
    pub permission_required: bool,
    // Fix: Architect F4 / F15 / H2 — when true (default), plugin tools are
    // surfaced to the LLM as `plugin:<id>:<tool>` so the model can
    // disambiguate. When false (private plugins), the bare name is shown.
    pub prompt_visible_provenance: bool,
    // Fix: Review — `is_hidden` and `is_user_invocable` from the current
    // Tool trait were omitted in the original design. `is_hidden` controls
    // whether the tool appears in help listings (hidden tools are still
    // callable by the LLM). `is_user_invocable` controls whether the LLM
    // can directly invoke the tool (false = system-internal only, e.g.
    // SelfReflectTool, LoadSkillTool).
    pub is_hidden: bool,               // default false
    pub is_user_invocable: bool,       // default true
}

pub enum ToolCategory {
    FileSystem, Network, Compute, Memory,
    Subagent, Skill, Command, Meta, Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolProvenance {
    Core,                              // built-in synthia tool
    Plugin { id: PluginId },           // from plugin manifest
    Mcp { server: String, host_owned: bool },
    Context { source: ContextSource }, // todo / skill / fragment
    Dynamic,                           // runtime-registered
}

// Fix: Rust F8 / H5 — `ToolProvenance` cannot derive `Copy` because the
// `Mcp` and `Context` variants own `String`s. Dropping `Copy` forces the
// registry to clone the provenance at snapshot time (cheap; `String` is
// 3 words + heap allocation, and snapshot is built once per session).
// `Clone + Debug + PartialEq + Eq + Hash` is the minimum viable bound set:
// `Hash` for use as a `HashMap` key; `PartialEq` for registry duplicate
// detection; `Debug` for diagnostics.

pub enum CancelBehavior {
    Interrupt,                         // abort at next yield
    AwaitCompletion,                   // wait for full output
}
```

#### `ToolProvider` Trait (the registration contract)

```rust
// crates/synthia-core/src/tool/provider.rs (new file)

/// Source of tools. Multiple providers can be registered,
/// each contributing a disjoint or overlapping set of tools.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Stable provider id (for diagnostics, hot-reload).
    fn id(&self) -> &str;

    /// Advertise all tools this provider exposes (cheap, idempotent).
    async fn list_tools(&self) -> Vec<ToolDescriptor>;

    /// Resolve a tool by name. None if not provided.
    async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>>;

    // Fix: Security F3 / H1 — `pre_check` has been REMOVED from
    // `ToolProvider`. Permission evaluation is a centralized concern
    // owned by `PermissionService::evaluate` (§8.3). Providers that
    // previously overrode `pre_check` to gate sensitive commands MUST
    // now register a `PermissionRule` via `PermissionService::add_rule`
    // at provider init time. The default `before_execute`/`after_execute`
    // hooks remain as audit-only callbacks, intentionally side-effect
    // free (no permission logic).

    /// Notification when tool result is emitted (for metrics, audit).
    async fn on_tool_event(&self, _event: &ToolEvent) {}

    /// Audit hooks (read-only). Do NOT perform permission logic here;
    /// use `PermissionService::add_rule`.
    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> { Ok(()) }
    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
```

#### `ToolRegistry` (the dynamic stack-based registry)

```rust
// crates/synthia-core/src/tool/registry.rs (new file)

/// LIFO-stacked registry. Last registration wins.
/// Stale-materialization detection (opencode-style) for safe concurrent use.
pub struct ToolRegistry {
    inner: Arc<RwLock<HashMap<String, Vec<ToolEntry>>>>,
}

struct ToolEntry {
    provider_id: ProviderId,
    tool: Arc<dyn Tool>,
    descriptor: ToolDescriptor,
    // Fix: Architect F1 / H10 — explicit `identity` field for stale
    // detection. The identity is captured at registration time and
    // compared against the snapshot at resolve time. If the identity
    // changes (e.g. plugin reloaded), `resolve()` returns `Stale`.
    //
    // Fix: Review NC10 — `ToolIdentity` is a VALUE type (Clone), not
    // `Arc<ToolIdentity>`. Snapshot captures identity by cloning at
    // snapshot time; subsequent generation bumps in the registry do NOT
    // affect already-captured snapshot identities. This is essential
    // for stale detection to work correctly.
    identity: ToolIdentity,
    registration_token: RegistrationToken,
}

impl ToolRegistry {
    /// Register a provider's tools atomically. All-or-nothing.
    pub fn register_provider(
        &self,
        provider: Arc<dyn ToolProvider>,
    ) -> Result<RegistrationToken, RegistrationError>;

    /// Unregister all tools owned by a token.
    pub fn unregister(&self, token: RegistrationToken);

    /// Snapshot for a session — captures identities for stale detection.
    ///
    /// Fix: Architect F1 / H10 — `materialize()` accepts an explicit
    /// `PermissionRuleset` so the snapshot is bound to the *evaluated*
    /// permission snapshot of the session. A tools->rules mapping is
    /// computed at materialization time and copied into the snapshot;
    /// later rule mutations that bump `PolicySnapshot::generation`
    /// (§8.3) trigger `PermissionDecision::PolicyStale` instead of
    /// silently using stale rules.
    pub fn materialize(
        &self,
        session_id: SessionId,
        permissions: PermissionRuleset,
    ) -> Materialization;

    /// Resolve a tool from a snapshot. Errors if stale.
    pub fn resolve(
        &self,
        mat: &Materialization,
        name: &str,
    ) -> Result<Arc<dyn Tool>, StaleOrUnknown>;

    /// Resolve by current state (no snapshot — for non-LLM callers).
    pub fn resolve_now(&self, name: &str) -> Option<Arc<dyn Tool>>;

    /// Registry-level output bounding (opencode `OutputBound`).
    /// Truncates oversized tool outputs to fit per-call and per-session budgets,
    /// spilling overflow into managed files. Returns the (possibly truncated)
    /// output plus the list of managed paths created.
    ///
    /// Fix: Review NC4 — changed from sync to async. File I/O (spill-to-disk)
    /// uses `tokio::fs` to avoid blocking tokio worker threads. This method
    /// is called after every tool execution, so blocking is unacceptable.
    pub async fn bound_output(
        &self,
        output: ToolOutput,
        session_id: &SessionId,
        call_id: &str,
    ) -> (ToolOutput, Vec<ManagedPath>);
}

/// Per-tool-output bounding policy (opencode借鉴).
/// Applied uniformly by `ToolRegistry::bound_output` so individual tools
/// don't need to know about truncation.
pub struct OutputBound {
    /// Hard ceiling on a single tool call's output (bytes). Default 50 KiB
    /// (matches opencode's `Truncate` default).
    pub per_call_max_bytes: usize,

    /// Hard line ceiling per call. Default 2_000 lines (matches opencode).
    /// Whichever limit hits first triggers truncation.
    pub per_call_max_lines: usize,

    /// Hard ceiling on cumulative output across the whole session (bytes).
    /// Default 4 MiB. Once exceeded, additional output is fully spilled to files.
    pub per_session_max_bytes: usize,

    /// Directory where spilled output is saved. `None` = use registry default
    /// (`{workspace_root}/.synthia/managed/{session_id}/{call_id}.json`).
    pub managed_dir: Option<PathBuf>,

    /// Strategy when output exceeds per-call limit.
    pub overflow_strategy: OverflowStrategy,

    // Fix: Architect F14 / H10 — retention policy for spilled managed files.
    // Spilled files accumulate forever by default, which causes disk pressure
    // on long-running sessions. The retention window mirrors opencode's
    // (`7.days`) and is enforced by a background cleanup task scheduled at
    // `cleanup_interval`.
    /// How long spilled managed files are retained before deletion.
    /// Default: 7 days. Set to zero to disable cleanup.
    pub retention: Duration,

    /// How often the background cleanup task scans managed dirs.
    /// Default: 1 hour. Must be > 0 if `retention > 0`.
    pub cleanup_interval: Duration,

    /// Sanitization policy applied to bounded output before returning to the
    /// LLM. See `SanitizationPolicy` below. Fix: Security F10 / H5.
    pub sanitization: SanitizationPolicy,
}

impl Default for OutputBound {
    fn default() -> Self {
        Self {
            per_call_max_bytes: 50 * 1024,           // Fix: H10 — opencode default
            per_call_max_lines: 2_000,               // Fix: H10 — opencode default
            per_session_max_bytes: 4 * 1024 * 1024,
            managed_dir: None,
            overflow_strategy: OverflowStrategy::TruncateHeadTail,
            retention: Duration::from_secs(7 * 24 * 60 * 60),    // 7d
            cleanup_interval: Duration::from_secs(60 * 60),      // 1h
            sanitization: SanitizationPolicy::default(),
        }
    }
}

/// Fix: Security F10 / H5 — sanitization policy applied to bounded output.
/// The default policy strips dangerous control characters (NUL, escape
/// sequences) and redacts obvious credential URLs (e.g. `https://user:token@*`).
#[derive(Debug, Clone)]
#[derive(Default)]
pub enum SanitizationPolicy {
    /// Strip ASCII control chars except `\n`, `\r`, `\t`. Default.
    #[default]
    StripControlChars,
    /// Same as `StripControlChars`, plus wrap output in an HTML/comment tag
    /// so downstream UIs can render it as untrusted.
    WrapUntrusted { tag: &'static str },
    /// Strip control chars + redact any URL matching the regex set.
    RedactUrlsMatching(Vec<regex::Regex>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowStrategy {
    /// Keep first N bytes + last M bytes, drop the middle (default; preserves
    /// head/tail context for grep-like tools).
    TruncateHeadTail,

    /// Keep first N bytes only (fast; for streaming outputs).
    TruncateHead,

    /// Always spill full output to managed file; tool result is a path reference.
    AlwaysSpill,
}

pub struct Materialization {
    // Fix: Review NC10 — `ToolIdentity` is a value type, not `Arc<ToolIdentity>`.
    // Each snapshot clones identity values at capture time. When the registry
    // bumps a tool's generation (e.g., plugin reload), existing snapshots
    // retain their cloned identity and detect staleness on `resolve()`.
    pub snapshot: HashMap<String, (Arc<dyn Tool>, ToolIdentity)>,
    pub snapshot_token: MaterializationToken,
}

/// Value-type identity for stale detection.
/// Fix: Review NC10 — `Clone` (not `Arc`) so snapshot and registry
/// hold independent copies. A generation mismatch means "stale".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolIdentity {
    pub name: String,
    pub generation: ToolGeneration,
}

/// Monotonically increasing generation counter.
/// Bumped by `register_provider` / `unregister` on the registry side.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToolGeneration(pub u64);

pub enum StaleOrUnknown {
    Stale,                                // provider re-registered
    Unknown,                              // never registered
}
```

#### Default Provider Implementations

```rust
// crates/synthia-tool/src/builtin/registry.rs
//
// Fix: Architect F4 / F15 / H2 — split into two maps.
//
// `BuiltinToolProvider` distinguishes **built-ins** (shipped with synthia,
// immutable across the lifetime of the process — refuse re-registration)
// from **local additions** (added at runtime by plugins, scripts, or
// dynamic_tool providers). They are merged at `materialize()` time:
// built-ins first, local-on-top (LIFO), so users can override built-in
// behavior without forking synthia.
pub struct BuiltinToolProvider {
    /// Ships with synthia. `register()` refuses to install if any name
    /// already carries a `ToolProvenance::Core` entry. Re-registration
    /// is rejected because two cores claiming the same name means one
    /// is wrong.
    applications: HashMap<String, Arc<dyn Tool>>,
    /// Runtime/plugin additions. Free to add/remove at any time.
    local: HashMap<String, Arc<dyn Tool>>,
}

impl BuiltinToolProvider {
    pub fn register_builtin(&mut self, tool: Arc<dyn Tool>) -> Result<(), RegistrationError> {
        // Fix: Architect F4 / F15 / H2 — core tool names are immutable.
        if self.applications.contains_key(tool.name()) {
            return Err(RegistrationError::CoreNameTaken {
                name: tool.name().to_string(),
            });
        }
        self.applications.insert(tool.name().to_string(), tool);
        Ok(())
    }

    pub fn add_local(&mut self, tool: Arc<dyn Tool>) {
        self.local.insert(tool.name().to_string(), tool);
    }

    pub fn remove_local(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        self.local.remove(name)
    }
}

#[async_trait]
impl ToolProvider for BuiltinToolProvider {
    fn id(&self) -> &str { "synthia.builtin" }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        let mut out: Vec<_> = self.applications.values().map(|t| t.descriptor().clone()).collect();
        out.extend(self.local.values().map(|t| t.descriptor().clone()));
        out
    }

    async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.local.get(name).cloned()
            .or_else(|| self.applications.get(name).cloned())
    }
}

// crates/synthia-mcp/src/provider.rs
//
// Fix: Review NH6 — `McpTransport` is a config enum (§10.6), not a trait.
// Runtime connections use `McpConnection` trait (object-safe, async).
// The provider holds `Arc<dyn McpConnection>` for the active connection.
pub struct McpToolProvider {
    server_name: String,
    host_owned: bool,
    connection: Arc<dyn McpConnection>,
}

/// Runtime MCP connection trait (object-safe).
/// Implementations: `StdioConnection`, `StreamableHttpConnection`, `WebSocketConnection`.
/// Created from `McpTransportConfig` via `McpConnectionFactory`.
#[async_trait]
pub trait McpConnection: Send + Sync {
    /// Open / re-open the transport. Returns a handle for send/recv.
    async fn connect(&self) -> Result<McpConnectionHandle, McpError>;
    /// Graceful shutdown.
    async fn close(&self) -> Result<(), McpError>;
}

// crates/synthia-extension/src/tool_provider.rs
//
// Fix: Architect F4 / F15 / H2 — plugin tools are *namespaced* in their
// descriptor so the LLM sees them as `plugin:<id>:<tool>` rather than
// risk clashing with built-in names. `prompt_visible_provenance: bool`
// controls whether the `<plugin:` prefix is shown to the LLM (default
// true so the model can disambiguate; turned off in private plugins).
pub struct PluginToolProvider {
    plugin_id: PluginId,
    tools: HashMap<String, Arc<dyn Tool>>,
    prompt_visible_provenance: bool,
}

impl PluginToolProvider {
    fn namespaced_name(&self, raw: &str) -> String {
        if self.prompt_visible_provenance {
            format!("plugin:{}:{}", self.plugin_id.0, raw)
        } else {
            raw.to_string()
        }
    }
}

// crates/synthia-skill/src/tool_provider.rs
pub struct SkillToolProvider { /* delegates to SkillRegistry */ }

// crates/synthia-agent/src/subagent/tool.rs
pub struct SubagentToolProvider {
    factory: Arc<dyn SubagentSessionFactory>,
}

// crates/synthia-tool/src/dynamic.rs
pub struct DynamicToolProvider { /* script-based */ }
```

#### `ToolContext` (passed to every execute)

```rust
// crates/synthia-core/src/tool/context.rs (new file)

pub struct ToolContext {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub tool_call_id: ToolCallId,
    pub user_id: UserId,
    pub workspace_root: PathBuf,
    pub cancellation: CancellationToken,
    // Fix: Security F2 / H5 — services are NOT injected directly into
    // ToolContext. Each tool declares a `ToolCapabilities` allow-list at
    // registration time, and the orchestrator hands it a
    // `CapabilityBroker` exposing only those capabilities. This is the
    // *principle of least privilege* applied to the tool surface: a
    // Read tool cannot reach MemoryService even if MemoryService is in
    // the registry.
    pub capabilities: CapabilityBroker,
    pub on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    pub metadata: serde_json::Map<String, Value>,
}

// Fix: Security F2 / H5 — per-tool capability allow-list. The tool
// declares at registration time which services / system facets it
// needs; `CapabilityBroker` makes the *subset* available as a typed
// handle. Default impl returns an empty allow-list (tool is a pure
// function on its input).
#[derive(Debug, Clone, Default)]
pub struct ToolCapabilities {
    pub memory_read: bool,
    pub memory_write: bool,
    pub session_fork: bool,
    pub permission_record: bool,
    pub hook_emit: bool,
    pub telemetry_record: bool,
    pub skill_invoke: bool,
    pub command_invoke: bool,
}

// `CapabilityBroker` is a thin wrapper typed by `ToolCapabilities` —
// calling a method whose capability flag is `false` returns
// `ToolError::CapabilityDenied`.
pub struct CapabilityBroker { /* subset handles */ }
```

#### `ToolInput` / `ToolOutput` (typed)

```rust
pub struct ToolInput {
    pub raw: serde_json::Value,            // what LLM sent
    // Fix: Rust F14 / H11 — `erased_serde::Serialize` does NOT provide
    // deserialization. `parsed` is *write-only* (you can re-serialize into
    // JSON, audit logs, or telemetry), but you cannot re-deserialize it
    // back into the concrete type. To get a typed handle, deserialize
    // `raw` yourself in `execute()` using your tool's own serde derive.
    // The trait object is `Send + Sync + 'static` so it can be moved
    // across thread boundaries by the orchestrator and stored in
    // `Box<dyn Any>`-backed audit buffers.
    pub parsed: Box<dyn erased_serde::Serialize + Send + Sync + 'static>,
}

pub struct ToolOutput {
    pub content: Vec<ContentPart>,         // what LLM sees
    pub structured: Option<serde_json::Value>,  // for storage/UI
    pub metadata: ToolMetadata,
    pub is_error: bool,
}

pub struct ToolMetadata {
    pub duration: Duration,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub truncated: Option<TruncatedBy>,
    pub managed_paths: Vec<ManagedPath>,   // files saved to managed dir
}
```

### 5.3 Key Decisions

1. **`Tool` = execute contract; `ToolDescriptor` = metadata**. Trait object methods don't allocate JSON Schema per call (cached in descriptor).
2. **`ToolProvider` = registration contract**. Decouples "where do tools come from" from "what does a tool do".
3. **`Materialization` solves opencode's stale-detection problem** — LLM gets snapshot at step T, plugin reloads at T+1, resolve detects stale and returns error.
4. **Default OutputBound at registry level** — opencode-style truncation, applies to all tools uniformly. Defaults match opencode's `Truncate` (50 KiB / 2_000 lines per call, 7-day retention, 1-hour cleanup interval).
5. **`ToolContext` carries `CapabilityBroker`** — tools declare a `ToolCapabilities` allow-list at registration time and receive only that subset. (Replaces the historical "carry `ServiceRegistry`" pattern that exposed all services to all tools — see Security F2.)
6. **`ToolProvenance` namespaces plugin tools** — `prompt_visible_provenance: bool` + `plugin:<id>:<tool>` naming. Core tool names are immutable: a second registration with `ToolProvenance::Core` is rejected.
7. **`get_tool(name) ≡ Some ⇔ name ∈ list_tools()`** — Fix: Architect F25 — the registry's `get_tool` resolver and `list_tools` enumerator MUST agree on the visible name set. The invariant is enforced by a debug-assert in `get_tool`: any tool returned by name lookup must appear in `list_tools()` for the same provider, and vice versa. Violating the invariant is a soundness bug (the LLM's view diverges from the resolver's view).

> **Invariant F25 enforcement.** `ToolRegistry` exposes a single
> `internal_consistency_check()` (compile-time `#[cfg(debug_assertions)]`)
> that walks every provider, calls `list_tools()` and `get_tool` for each
> name, and asserts the bidirectional agreement. Runs in `cargo test` and
> in production on a 1-hour interval.

### 5.4 Migration Path

1. Add new traits alongside existing (`ToolV2`, `ToolProviderV2`). Mark `Tool` and `ToolProvider` as `#[deprecated]`.
2. Migrate built-in tools to `ToolV2` (5-7 tools). Add `BuiltinToolProvider` impl.
3. Add `Materialization` to `ToolRegistry`. Wrap existing `run_with_context` with snapshot+resolve.
4. Migrate MCP plugin. Add `McpToolProvider` + `PluginToolProvider`.
5. Deprecate v1 traits; remove after 1 release cycle.

---

## 6. Service Trait Layer + Registry

### 6.1 Core Insight (from v3 multi-expert analysis)

The v3 report's key finding: **"Tool 化的必要条件 (≥3 个)"** distinguishes toolizable from non-toolizable:
- ✅ Toolize: user-facing capabilities with permission + async response + LLM-initiated + low frequency
- ❌ Don't toolize: high-frequency internal state (Provider, LoopDetector, PrefixTracker, EventBus)

**The solution**: a parallel **Service Registry** that holds the same `dyn Trait + Registry` pattern, but services are *system-internal* and never exposed to LLM.

### 6.2 Service Architecture

```rust
// crates/synthia-service/src/traits.rs (new file)

/// System-internal service. NOT exposed to LLM. Used by:
/// - Loop (consumes services via `OperationContext`)
/// - Tools (consume services via `CapabilityBroker` — see §5.2)
/// - Extensions (register services)
#[async_trait]
pub trait Service: Send + Sync + 'static {
    // Fix: Rust F2 / H2 — `static_name()` is REMOVED from `Service`.
    // The object-safe contract has only `name(&self) -> &str` (for
    // diagnostics). Typed registration keys live in the registry (see
    // F1) and key off `TypeId::of::<Arc<dyn SessionService>>()`, not
    // off a `&'static str`. String names are inspection-only.
    fn name(&self) -> &str;
    fn version(&self) -> SemverVersion { SemverVersion::new(0, 1, 0) }
    async fn init(&self, ctx: &ServiceInitContext) -> Result<(), ServiceError> { Ok(()) }
    async fn shutdown(&self) -> Result<(), ServiceError> { Ok(()) }
}

/// Marker trait: services that hold state.
///
/// Fix: Rust F4 / H4 — `StatefulService` is intentionally NOT
/// `dyn`-compatible (it has an associated type), so the registry
/// cannot call `snapshot()` / `restore()` through a `&dyn Service`.
/// State persistence goes through `ErasedStatefulService` (see below)
/// which returns `serde_json::Value`. `snapshot_all` / `restore_all`
/// iterate the erased view.
pub trait StatefulService: Service {
    // Fix: Rust F49 — the `Clone` bound is REMOVED. Snapshotting
    // already produces JSON values, so the inner state does not need
    // to be cloneable in process — only `Send + Sync + 'static`.
    type State: Send + Sync + 'static;
    async fn snapshot(&self) -> Result<serde_json::Value, ServiceError>;
    async fn restore(&self, state: serde_json::Value) -> Result<(), ServiceError>;
}

/// Fix: Rust F4 / H4 — erased-stateful view used by the registry's
/// `snapshot_all` / `restore_all`. The dyn-compatible trait has no
/// associated type so it can be stored alongside plain `Service`s.
#[async_trait]
pub trait ErasedStatefulService: Service {
    async fn snapshot_json(&self) -> Result<serde_json::Value, ServiceError>;
    async fn restore_json(&self, state: serde_json::Value) -> Result<(), ServiceError>;
}

// Blanket impl so every `StatefulService` is automatically an
// `ErasedStatefulService`. JSON bridging happens here.
#[async_trait]
impl<T> ErasedStatefulService for T
where
    T: StatefulService + Send + Sync,
    T::State: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    async fn snapshot_json(&self) -> Result<serde_json::Value, ServiceError> {
        let typed = self.snapshot().await?;
        serde_json::to_value(typed).map_err(ServiceError::Serialization)
    }
    async fn restore_json(&self, state: serde_json::Value) -> Result<(), ServiceError> {
        let typed: T::State = serde_json::from_value(state)
            .map_err(ServiceError::Deserialization)?;
        self.restore(typed).await
    }
}
```

#### ServiceProvider (registration)

```rust
// crates/synthia-service/src/provider.rs (new file)

// Fix: Rust H5 / F39 — explicit `'static` bounds so the registry can
// own services without worrying about lifetimes. The orchestrator
// constructs all services once at boot and re-uses them across turns.
#[async_trait]
pub trait ServiceProvider: Send + Sync + 'static {
    fn id(&self) -> &str;
    async fn list_services(&self) -> Vec<ServiceDescriptor>;
    async fn get_service(&self, name: &str) -> Option<Arc<dyn Service>>;

    /// Dependency declaration (for ordering init).
    ///
    /// Fix: Rust F50 — return `&'static [ServiceKey]` instead of
    /// `Vec<&str>`. `ServiceKey` is a newtype wrapper around
    /// `TypeId` + name, providing compile-time check that the
    /// referenced service exists. The old `Vec<&str>` was typed
    /// loosely and allowed dangling references.
    fn dependencies(&self) -> &'static [ServiceKey] { &[] }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceKey {
    type_id: TypeId,
    name: &'static str,
}

impl ServiceKey {
    pub fn of<T: 'static + ?Sized + Service>() -> Self { /* typeid + name */ }
    pub fn type_id(&self) -> TypeId { self.type_id }
    pub fn name(&self) -> &'static str { self.name }
}

pub struct ServiceDescriptor {
    pub name: String,
    pub version: SemverVersion,
    pub category: ServiceCategory,
}

pub enum ServiceCategory {
    Session, Memory, Permission, Guardian,
    Hook, Extension, Configuration, Telemetry,
    Skill, Command, Task, Scheduler, Goal, Custom,
}

// Fix: Rust F51 — service init lifecycle state machine.
// Each service transitions through: Constructed -> Initializing ->
// Initialized -> Running -> ShuttingDown -> Dropped. The state is
// observable via `ServiceRegistry::state(&ServiceKey) -> ServiceState`
// so partial init failures can be diagnosed without restarting the
// whole process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Constructed,
    Initializing,
    Initialized,
    Running,
    ShuttingDown,
    Dropped,
}

pub enum ServiceError {
    Serialization(serde_json::Error),
    Deserialization(serde_json::Error),
    InitFailed(String),
    DependencyMissing(ServiceKey),
    NotFound(String),
    StateInvalid { expected: ServiceState, actual: ServiceState },
    // Fix: Rust F52 — bounded `AgentError` conversion boundary.
    // `ServiceError` keeps pure-service concerns; cross-layer
    // conversion happens via `From<ServiceError> for AgentError`
    // in `synthia-agent` (NOT here, to keep this crate layer-pure).
    CapabilityDenied { service: ServiceKey, need: &'static str },
}
```

#### ServiceRegistry (the dynamic registry)

```rust
// crates/synthia-service/src/registry.rs (new file)

pub struct ServiceRegistry {
    // Fix: Rust F45 / F46 — drop the redundant outer `Arc` (the
    // registry itself is wrapped in `Arc` at construction) and switch
    // to `parking_lot::RwLock` for short critical sections
    // (~µs-resolution locks). `tokio::sync::RwLock` would force every
    // resolve to await the scheduler even when uncontended.
    name_index: parking_lot::RwLock<HashMap<String, Vec<Arc<ServiceEntry>>>>,
    // Fix: Rust F1 / H1 — typed `TypeId` index keyed by
    // `TypeId::of::<Arc<dyn TheService>>()`. Lets `services.get::<Arc<dyn
    // SteeringService>>()` resolve without downcasting on the hot
    // path.
    type_index: parking_lot::RwLock<HashMap<TypeId, Arc<ServiceEntry>>>,
}

struct ServiceEntry {
    provider_id: ProviderId,
    // Fix: Rust F1 / H1 — payload type is `Arc<dyn Any + Send + Sync>`
    // whose concrete payload is itself an `Arc<dyn SessionService>`
    // (or whichever typed trait object). The `Any` is what makes
    // `TypeId` lookups possible; the inner `Arc<dyn SessionService>`
    // is what the consumer actually receives.
    service: Arc<dyn Any + Send + Sync>,
    descriptor: ServiceDescriptor,
    registration_token: RegistrationToken,
    // Fix: Rust F4 / H4 — has a `dyn ErasedStatefulService` view if
    // the underlying service is also a `StatefulService`. Optional;
    // `None` means the service is stateless.
    stateful: Option<Arc<dyn ErasedStatefulService>>,
    state: ServiceState,                // Fix: Rust F51 — init lifecycle
}

// Fix: Rust F47 — `state: Option<Arc<dyn AnyState>>` was undefined
// (no `AnyState` trait existed). Replaced by the dedicated
// `ServiceState` lifecycle enum above. There is NO `Option<Arc<dyn
// AnyState>>` field anywhere.

impl ServiceRegistry {
    /// Fix: Review NC1 — register_provider validates TypeId consistency
    /// under debug_assertions. The `Any` payload must be constructed with
    /// the *exact* `Arc<dyn SubTrait>` type (not `Arc<dyn Service>`) so
    /// that `TypeId::of::<Arc<dyn SubTrait>>()` matches the stored payload.
    ///
    /// Example of correct registration:
    /// ```rust
    /// // In SessionServiceProvider::register:
    /// let service: Arc<dyn SessionService> = Arc::new(MySessionService);
    /// let any_payload: Arc<dyn Any + Send + Sync> = Arc::new(service);
    /// // any_payload.type_id() == TypeId::of::<Arc<dyn SessionService>>() ✓
    ///
    /// // WRONG: erasing to base trait first:
    /// let base: Arc<dyn Service> = service;  // TypeId changes!
    /// let any_payload: Arc<dyn Any + Send + Sync> = Arc::new(base);
    /// // any_payload.type_id() == TypeId::of::<Arc<dyn Service>>() ✗
    /// ```
    pub fn register_provider(
        &self,
        provider: Arc<dyn ServiceProvider>,
    ) -> Result<RegistrationToken, ServiceError> {
        // ... registration logic ...
        #[cfg(debug_assertions)]
        {
            // Fix: Review NC1 — validate that the Any payload's TypeId
            // matches the expected subtrait TypeId. If the provider
            // accidentally erased to `Arc<dyn Service>` before wrapping
            // in Any, this downcast would return None at runtime.
            debug_assert_eq!(
                entry.service.type_id(),
                TypeId::of::<Arc<dyn SessionService>>(),
                "TypeId mismatch for service '{}': the Any payload type \
                 must be exactly Arc<dyn SubTrait>, not Arc<dyn Service>",
                entry.descriptor.name,
            );
        }
        // ...
    }

    pub fn unregister(&self, token: RegistrationToken);

    /// Name-based resolve. Returns the top-of-stack.
    ///
    /// Fix: Rust F3 / H3 — `resolve()` is reserved for diagnostics
    /// and tooling. Typed resolution goes through `get()`. The two
    /// views are kept consistent by an internal invariant: every
    /// entry in `name_index` is also indexed in `type_index`.
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn Service>>;

    /// Fix: Rust F1 / H1 — the only hot-path typed access:
    ///
    ///   let steering = registry.get::<Arc<dyn SteeringService>>()?;
    ///
    /// Lookups go through `type_index` keyed by
    /// `TypeId::of::<Arc<dyn SteeringService>>()`. The returned
    /// `Some` carries the typed `Arc<dyn SteeringService>` directly;
    /// no downcast on the hot path.
    pub fn get<S: ?Sized + Service + 'static>(&self) -> Option<Arc<S>>
    where
        Arc<S>: Any + Send + Sync,
    {
        let key = TypeId::of::<Arc<S>>();
        let entry = self.type_index.read().get(&key)?.clone();
        // The entry stores `Arc<dyn Any + Send + Sync>` whose payload
        // is `Arc<S>`. Recovering the trait object is one downcast.
        entry.service.downcast_ref::<Arc<S>>()
            .cloned()
    }

    // Fix: Rust F48 — `snapshot_all` / `restore_all` are async because
    // some services (e.g. `MemoryService`, `TelemetryService`) own
    // their state behind an `await`-heavy bridge (database, OTLP).
    // Forcing them to be sync would block the executor.

    /// Snapshot for restoration (stateful services).
    pub async fn snapshot_all(&self) -> HashMap<String, serde_json::Value>;

    pub async fn restore_all(
        &self,
        snapshot: HashMap<String, serde_json::Value>,
    ) -> Result<(), ServiceError>;

    /// Fix: Rust F51 — observe a service's lifecycle state.
    pub fn state(&self, key: &ServiceKey) -> Option<ServiceState>;
}

// Fix: Rust F53 — instead of `.unwrap()` on `services.get()` at
// construction, the agent's `run_loop` performs a validation step that
// surfaces missing services as `AgentError::RequiredServiceMissing`
// (with the `ServiceKey` payload) *before* any LLM call. Crashing on a
// missing service is never acceptable.
//
// Fix: Rust F54 — `LoopServices` caches resolved services once per
// run/turn. The agent constructs `LoopServices` after the validation
// step and threads it into each turn to avoid repeated `get()` calls
// on the hot path.
//
// Fix: Review NH5 — services are split into required (hard fail if
// missing) and optional (no-op default if missing). This ensures
// backward compatibility when new services are added (e.g., GoalService)
// — old configurations without the new service can still bootstrap.
pub struct LoopServices {
    // Required services — missing = AgentError::RequiredServiceMissing
    pub session: Arc<dyn SessionService>,
    pub permission: Arc<dyn PermissionService>,
    pub hooks: Arc<dyn HookService>,
    pub memory: Arc<dyn MemoryService>,
    // Optional services — missing = no-op default with warning log
    pub guardian: Arc<dyn GuardianService>,       // no-op: detect() → Clean
    pub goal: Arc<dyn GoalService>,               // no-op: current() → None
    pub steering: Arc<dyn SteeringService>,       // no-op: drain() → empty
    pub agent_control: Arc<dyn AgentControlService>, // no-op: no limits
    pub context: Arc<dyn ContextService>,         // no-op: default assembly
    pub sandbox: Arc<dyn SandboxService>,         // no-op: no sandboxing
    pub extension: Arc<dyn ExtensionService>,     // no-op: no plugins
    pub model_router: Arc<dyn ModelRouterService>, // no-op: first available
    pub skill: Arc<dyn SkillService>,             // no-op: no skills
    pub command: Arc<dyn CommandService>,         // no-op: no commands
    pub task: Arc<dyn TaskService>,               // no-op: no tasks
    pub telemetry: Arc<dyn TelemetryService>,     // no-op: console-only
}

impl LoopServices {
    /// Bootstrap resolves services from the registry.
    /// Required services: hard failure if missing.
    /// Optional services: replaced with no-op defaults + warning log.
    pub fn bootstrap(
        registry: &ServiceRegistry,
        ctx: &OperationContext,
    ) -> Result<Self, AgentError> {
        // Required services — fail immediately
        let session = registry.get::<Arc<dyn SessionService>>()
            .ok_or(AgentError::RequiredServiceMissing {
                key: ServiceKey::of::<Arc<dyn SessionService>>(),
            })?;
        // ... (same for permission, hooks, memory)

        // Optional services — no-op fallback
        let guardian = registry.get::<Arc<dyn GuardianService>>()
            .unwrap_or_else(|| {
                tracing::warn!("GuardianService not found, using no-op (doom-loop detection disabled)");
                Arc::new(NoopGuardianService)
            });
        let goal = registry.get::<Arc<dyn GoalService>>()
            .unwrap_or_else(|| {
                tracing::warn!("GoalService not found, using no-op (goal tracking disabled)");
                Arc::new(NoopGoalService)
            });
        // ... (same for other optional services)

        Ok(Self { session, permission, hooks, memory, guardian, goal, /* ... */ })
    }
}
```

### 6.3 Concrete Services (refactored from current crates)

| Service | Trait | Implementation | Source Crate |
|---------|-------|---------------|--------------|
| `SessionService` | trait `SessionService` | `DefaultSessionService` (v2) | `synthia-session-v2` |
| `MemoryService` | trait `MemoryService` | `DefaultMemoryService` (4-tier) | `synthia-memory` |
| `PermissionService` | trait `PermissionService` | `MergedPolicy` impl | `synthia-permission` |
| `GuardianService` | trait `GuardianService` | `LoopDetectorSet` impl | `synthia-guardian` |
| `HookService` | trait `HookService` | `HookRegistry` impl | `synthia-hook` |
| `SkillService` | trait `SkillService` | `SkillRegistry` impl | `synthia-skill` |
| `CommandService` | trait `CommandService` | `CommandRegistry` impl | `synthia-command` |
| `TaskService` | trait `TaskService` | `TaskManager` impl | `synthia-task` |
| `TelemetryService` | trait `TelemetryService` | OTel impl | `synthia-telemetry` |
| `ContextService` | trait `ContextService` | `ContextAssembler` impl | `synthia-context` |
| `ExtensionService` | trait `ExtensionService` | `PluginRegistry` impl | `synthia-extension` |
| `SchedulerService` | trait `SchedulerService` | `JobScheduler` impl | `synthia-server` |
| `GoalService` *(new)* | trait `GoalService` | `DefaultGoalService` | `synthia-agent` |

#### `GoalService` *(new — Fix: Architect F3)*

```rust
#[async_trait]
pub trait GoalService: Send + Sync {
    async fn current(&self) -> Option<Goal>;
    async fn set(&self, goal: Goal) -> Result<(), ServiceError>;
    async fn status(&self) -> GoalStatus;
    async fn budget(&self) -> GoalBudget;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    Active,
    Blocked,        // waiting on user / external
    BudgetLimited,  // turn was truncated by token budget
    UsageLimited,   // turn was truncated by tool-call budget
}

#[derive(Debug, Clone)]
pub struct GoalBudget {
    pub token_budget: Option<i64>,    // total tokens allowed for this goal
    pub tool_call_budget: Option<u64>,
}

pub struct Goal { /* id, description, success_criteria, ... */ }
```

`GoalService` is consumed in §7.4 step 2 (re-entry of `current()`
after each turn). The loop aborts with `GoalStatus::Blocked` when the
session is *awaiting user* — the previous loop blocked indefinitely
on a missing concept.

#### `CodeModeService` *(deferred — Fix: Architect F4)*

Not added in this round. Tracked in §13 *Open Questions* as
**Q-CodeMode-1**: should `CodeModeService` be a service (process-local
sandbox with `eval`) or a tool (LLM-invokable)? Decision is owner-dependent;
see §13 for the deferral rationale.

### 6.4 Service Composition Pattern

```rust
// crates/synthia-agent/src/agent.rs (refactored)

impl Agent {
    pub async fn run_stream(&self, config: AgentRunConfig) -> Result<AgentOutput, AgentError> {
        // Fix: Rust F1 / F54 — typed access uses
        // `get::<Arc<dyn TheService>>()` and is cached in
        // `LoopServices`. Validation step (`bootstrap`) replaces the
        // historical `unwrap()` calls — a missing service becomes
        // `AgentError::RequiredServiceMissing`.
        let ctx = OperationContext::for_session(&config);
        let services = LoopServices::bootstrap(&config.services, &ctx)?;

        // 2. Resolve tools from registry (with snapshot). The
        //    `permissions: PermissionRuleset` binding ensures the
        //    tool snapshot is keyed to the current policy generation
        //    (Fix: Architect F1 / H10).
        let tool_mat = config.tools.materialize(
            services.session.current_id(),
            services.permission.snapshot_ruleset(),
        );

        // 3. Drive the loop (loop logic unchanged)
        self.run_loop(config, services, tool_mat, ctx).await
    }
}
```

### 6.5 Key Decisions

1. **Service = system capability; Tool = LLM capability**. Same pattern, different consumers.
2. **Type-safe accessors** (`registry.get::<MyService>()`) eliminate string lookups in hot paths.
3. **Stateful services** declare snapshot/restore — supports hot-reload + checkpoint.
4. **Provider dependencies** enable ordered init.
5. **No service is exposed to LLM**. Service Registry is the loop's view; Tool Registry is the LLM's view.

### 6.6 Migration Path

1. Define `Service` + `ServiceProvider` + `ServiceRegistry` traits in `synthia-service` crate (new).
2. Wrap each existing service (12 listed) as `impl Service for MyService`. Both APIs coexist.
3. Refactor `Agent::run_stream` to consume `ServiceRegistry`. Old direct field access remains `#[deprecated]`.
4. Migrate MCP, plugin, hook to register via `ServiceProvider`.
5. Remove legacy fields from `AgentRunConfig`.

---

## 7. Loop Layer Refactor (Preserve Main Logic)

### 7.1 Core Principle (user requirement)

> "除了主逻辑 react loop 和 session 之外，其他功能尽量抽象为 tool 实现以及registry"

The loop itself is preserved. Only the **dependency injection surface** is refactored.

### 7.2 Current State (from background report)

- `main_loop.rs`: 1037 lines, single `async_stream::stream!` block
- `AgentRunConfig`: 11+ fields (many discarded as `_xxx`)
- `StreamBuilder::run_with_steps`: 7 params
- `LoopContext`: 9 fields including messages, cumulative_tokens, recent_tool_results, etc.

### 7.3 Target State (minimal change to loop internals)

```rust
// crates/synthia-agent/src/agent.rs (refactored signature)

impl Agent {
    pub async fn run_stream(
        &self,
        config: AgentRunConfig,
    ) -> Result<AgentOutput, AgentError> { /* unchanged body, signature now Result */ }
}

/// Fix: Rust F21 / H18 — cancellation + deadline propagate uniformly.
/// `OperationContext` is built once per turn and threaded through
/// every tool / permission / hook / provider call. Tools honor
/// `ctx.cancellation` at every yield point. `ctx.deadline` is checked
/// before each LLM sample.
pub struct OperationContext {
    pub cancellation: CancellationToken,
    pub deadline: Instant,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub user_id: UserId,
    pub agent_id: AgentId,
}

impl OperationContext {
    pub fn for_session(config: &AgentRunConfig) -> Self;
    pub fn child(&self, session_id: SessionId, turn_id: TurnId) -> Self;
}

pub struct AgentRunConfig {
    /// Resolved services (instead of 11 individual fields)
    pub services: Arc<ServiceRegistry>,

    /// Cached `LoopServices` built once per run (Fix: Rust F54).
    /// Populated at `run_stream` entry by `LoopServices::bootstrap`.
    pub loop_services: OnceLock<LoopServices>,

    /// Resolved tools (with materialization snapshot)
    pub tools: Materialization,

    /// Agent identity (was: inline config)
    pub agent_id: AgentId,
    pub agent_role: AgentRole,
    pub system_prompt: SystemPrompt,

    /// LLM provider (resolved from service)
    pub model: ModelSpec,

    /// Session context (was: SessionManager field)
    pub session_id: SessionId,
    pub turn_id: TurnId,

    /// User context (for cache policy, permission)
    pub user_id: UserId,

    /// Cancellation — kept for backward-compat.
    /// `OperationContext.cancellation` is the canonical source.
    pub cancellation: CancellationToken,
}
```

### 7.4 Loop Internals (preserved)

The actual `main_loop.rs` logic — the per-iteration state machine — stays largely intact. Resolution paths use the new typed `services.get::<Arc<dyn The>>()` API (§6.2 / Fix F1).

```rust
// crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs (logic preserved)

pub async fn run_with_steps(
    config: AgentRunConfig,
    op_ctx: OperationContext,
    services: &LoopServices,         // Fix: F54 — cached, no re-resolve
) -> impl Stream<Item = AgentEvent> {
    async_stream::stream! {
        while !op_ctx.cancellation.is_cancelled() {
            // 1. drain steering — uses SteeringService (Fix: F6 — typed API)
            services.steering.drain().await?;

            // 1a. check goal status — uses GoalService (Fix: F3)
            if matches!(services.goal.status().await, GoalStatus::Blocked) {
                break;        // user intervention required
            }

            // 2. check background subagent — uses AgentControlService
            services.agent_control.check_completed().await?;

            // 2a. deadline check (Fix: F21 / H18)
            if Instant::now() >= op_ctx.deadline {
                fail_interrupted_tools().await?;
                break;
            }

            // 3. cancellation check (canonical: OperationContext)
            if op_ctx.cancellation.is_cancelled() {
                fail_interrupted_tools().await?;
                break;
            }

            // 4. compact step — uses ContextService
            services.context.maybe_compact(&op_ctx).await?;

            // 5. build tool defs — uses ToolRegistry snapshot
            let tool_defs = config.tools.build_definitions();

            // 6. fire before_llm hook — uses HookService
            services.hooks.fire_before_llm(&op_ctx).await?;

            // 7. sample LLM — uses Provider
            services.provider.sample(&op_ctx).await?;

            // 8. check doom loop — Fix: F2 / H6 — route via
            //    PermissionService::evaluate so the doom-loop detection
            //    becomes part of the policy decision pipeline.
            //    Threshold = 3 identical tool calls within 5 turns.
            //    Reuses GuardianService as the *detector*, but the
            //    *decision* (abort, ask user, allow) is a permission
            //    policy outcome, not a loop-body branch.
            services.permission.evaluate_doom_loop(&op_ctx, services.guardian.detect().await?)?;

            // 9. execute tools — uses ToolRegistry
            execute_tools(&op_ctx, &config.tools).await?;

            // 10. fire after_llm hook
            services.hooks.fire_after_llm(&op_ctx).await?;
        }
    }
}
```

### 7.5 What Changes Inside the Loop

1. **All 11 discarded `_xxx` fields are now resolved through `LoopServices`** (cached once per run, Fix: F54):
   - `_subagent_session_factory` → `services.agent_control` (`Arc<dyn AgentControlService>`)
   - `_sandbox_manager` → `services.sandbox`
   - `_extension_manager` → `services.extension`
   - `_approval_service` → `services.permission` (permission is the canonical authority)
   - `_guardian_coordinator` → `services.guardian`
   - `_model_router` → `services.model_router`
   - `_fork_policy` → field on `AgentRole` config
   - `_compaction_provider` → `services.context`
   - `_steering_channel` → `services.steering`
   - `_context_assembler` → `services.context`
   - `_tool_orchestrator` → wrapped by `ToolRegistry` + Orchestrator default impl

2. **`OperationContext` threaded through loop** (Fix: F21 / H18). Cancellation flows from the user (Ctrl-C, RPC cancel) via `op_ctx.cancellation`, and deadline via `op_ctx.deadline`. Both are honored at every yield point and between turns.

3. **Doom-loop detector routes through `PermissionService`** (Fix: F2 / H6). `GuardianService.detect()` returns a `DoomLoopVerdict`; the *policy decision* (abort / ask user / allow) is computed by `PermissionService::evaluate_doom_loop`, applying `GuardianService` only as the *detector*. Threshold = 3 identical tool calls within 5 turns.

4. **`LoopContext` slimmed**: 9 fields → 5 (services ref via `LoopServices`, tools ref, messages, tokens, iteration). Other state moves to service-owned state.

5. **Hooks fire correctly**: `on_before_tool`, `on_after_tool`, `on_error`, `on_iteration_end`, `on_complete` now wired (currently only 2 of 7 fire).

#### `SteeringService` API (Fix: F6)

```rust
#[async_trait]
pub trait SteeringService: Send + Sync {
    /// Enqueue a steering message. `mode` controls buffering strategy.
    async fn enqueue(&self, message: SteeringMessage, mode: QueueMode) -> Result<(), ServiceError>;

    /// Drain pending steering messages, applying them to `ctx`. The
    /// loop calls this at the top of each iteration.
    async fn drain(&self) -> Result<Vec<SteeringMessage>, ServiceError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueMode {
    /// Coalesce with the previous message (default; semantic over wire).
    Coalesce,
    /// Append verbatim (debug / replay).
    Append,
    /// Replace any pending messages with this one.
    Replace,
    /// Fix: Review SteeringService deliverAs — inject a message that
    /// appears in the conversation as if it came from the specified role.
    /// Used by compaction (summary injected as System message) and
    /// subagent result injection (appears as User message).
    /// pi-mono's PendingMessageQueue supports this via `deliverAs`.
    DeliverAs { as_role: MessageRole },
}
```

`QueueMode::Coalesce` is the default for live steering; `Append` is
used by replay tools that want a per-message audit trail.

#### `SessionRunCoordinator` (Fix: F5)

Long-running server sessions have *multiple* runs (subagents, parallel
turns, scheduled jobs) that compete for shared session state. The
`SessionRunCoordinator` arbitrates them.

```rust
// crates/synthia-service/src/coordinator.rs (new file)

/// Fix: Architect F5 — multi-run orchestration primitive. Holds the
/// canonical "who is running in this session" map so the loop can
/// reject duplicate runs and serialize wakeups.
pub struct SessionRunCoordinator {
    inner: parking_lot::Mutex<HashMap<SessionId, RunState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionKey(pub SessionId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running { run_id: RunId },
    Interrupted { at: Instant },
}

impl SessionRunCoordinator {
    /// Start a run on `key`. Returns `Err(AlreadyRunning)` if a run is
    /// already active.
    pub fn run(&self, key: SessionKey) -> Result<RunGuard, ServiceError>;

    /// Wake a sleeping session (e.g. a subagent finished its leg).
    /// Returns `Err(NoSuchRun)` if there is nothing to wake.
    pub fn wake(&self, key: SessionKey) -> Result<RunId, ServiceError>;

    /// Interrupt a run cooperatively. The run's `OperationContext`
    /// cancellation is tripped; the loop checks at next yield point.
    pub fn interrupt(&self, key: SessionKey) -> Result<(), ServiceError>;

    /// Block until the run for `key` reaches `Idle`. Used by callers
    /// that need to serialize multiple turns.
    pub async fn await_idle(&self, key: SessionKey);
}

pub struct RunGuard { /* Drop cancels the run if not finished */ }
```

### 7.6 Key Decisions

1. **Loop logic unchanged**: only DI surface refactored.
2. **All 11 dropped fields restored**: G1 from baseline analysis fixed.
3. **Hook fires restored**: G4 from baseline analysis fixed.
4. **Service access cached in `LoopServices`** at run entry (Fix: F54).
5. **Tool execution via Materialization**: stale-safe.
6. **`OperationContext` is the cancellation authority** (Fix: F21).

### 7.7 Migration Path

1. Define 10 new `Service` impls: `ProviderService`, `SteeringService`, `AgentControlService`, `ApprovalService`, `SandboxService`, `GuardianService`, `CompactionService`, `ExtensionService`, `SubagentService`, `ModelRouterService`, `GoalService`, `SessionRunCoordinator`.
2. Refactor `AgentRunConfig` from 11+ fields → 1 services + 1 tools + identity fields. Mark old fields `#[deprecated]`.
3. Refactor `main_loop.rs` callsites from `field` → `services.<service_field>.<method>()`. Verify behavior unchanged via E2E tests.
4. Wire the 5 unfired hooks.
5. Add `SessionRunCoordinator` integration tests for parallel subagent runs.
6. Remove deprecated fields.

---

## 8. Session / Memory / Permission / Hook Refactor

### 8.1 Session Refactor

#### Current state
- `synthia-session` (v1) + `synthia-session-v2` coexist
- v2 has part-based model + background JSONL writer + 14-variant `SessionEntry`
- v1 has state machine + JSONL persistence
- Agent reads checkpoints via v1; v2 manager exists but unused for resume

#### Target: v2 is sole impl

```rust
// crates/synthia-session/src/service.rs

#[async_trait]
pub trait SessionService: Service {
    async fn create(&self, cfg: SessionConfig) -> Result<SessionId, SessionError>;
    async fn load(&self, id: SessionId) -> Result<SessionSnapshot, SessionError>;
    async fn append(&self, id: SessionId, entry: SessionEntry) -> Result<(), SessionError>;
    async fn query(&self, id: SessionId, q: SessionQuery) -> Result<Vec<SessionEntry>, SessionError>;
    async fn fork(&self, id: SessionId, at: EntryId) -> Result<SessionId, SessionError>;
    async fn compact(&self, id: SessionId, strategy: CompactionStrategy) -> Result<CompactionReport, SessionError>;
    async fn rollback(&self, id: SessionId, turns: u32) -> Result<(), SessionError>;
    async fn snapshot(&self) -> SessionRegistrySnapshot;
}

pub struct DefaultSessionService {
    storage: Arc<dyn SessionStorage>,    // JSONL append-only + state.db
    writer_task: Arc<SessionWriterTask>,  // background mpsc + 50ms batch
}
```

#### Key decisions
- **Drop v1 entirely** (deprecated since 0.3.0).
- **`SessionStorage` trait** allows JSONL-only (default), SQLite, or custom backends.
- **Single `SessionEntry` shape** (v2's 14 variants, cleaned up).
- **State machine becomes internal** to `DefaultSessionService`.

### 8.2 Memory Refactor

#### Current state
- 4 tiers: hot (in-mem), cold (SQLite/JSONL), episodic (JSONL), context (vector)
- `MemoryService` trait defined but most callers only push `MemoryEvent`s
- `ExperienceLearner` / `LearnedExperience` / `ActionSuggestion` exists but primitive

#### Target

```rust
// crates/synthia-memory/src/service.rs

#[async_trait]
pub trait MemoryService: Service {
    // Hot tier
    async fn hot_set(&self, key: String, value: Value, ttl: Option<Duration>) -> Result<(), MemoryError>;
    async fn hot_get(&self, key: &str) -> Result<Option<Value>, MemoryError>;

    // Cold tier
    async fn cold_store(&self, entry: ColdEntry) -> Result<EntryId, MemoryError>;
    async fn cold_search(&self, query: MemoryQuery) -> Result<Vec<ColdEntry>, MemoryError>;

    // Episodic tier
    async fn episodic_record(&self, session_id: SessionId, event: EpisodicEvent) -> Result<(), MemoryError>;
    async fn episodic_replay(&self, session_id: SessionId) -> Result<Vec<EpisodicEvent>, MemoryError>;

    // Context tier
    async fn context_search(&self, query: &str, limit: usize) -> Result<Vec<ContextMatch>, MemoryError>;

    // Cross-tier
    async fn consolidate(&self, strategy: ConsolidationStrategy) -> Result<ConsolidationReport, MemoryError>;
    async fn snapshot(&self) -> MemorySnapshot;
}

pub struct DefaultMemoryService {
    hot: Arc<RwLock<HashMap<String, HotEntry>>>,
    cold: Arc<dyn ColdStorage>,
    episodic: Arc<dyn EpisodicStorage>,
    context: Arc<dyn SemanticRetriever>,
    background_task: Arc<MemoryBackgroundTask>,
}
```

#### Key decisions
- **Hot/Cold/Episodic/Context as 4 separate traits** with default impls, unified via `MemoryService` supertrait.
- **Cold storage as a trait** — JSONL default, SQLite via `sqlx` opt-in.
- **Context (vector) as a trait** — `InMemoryVectorStore` default, real backend pluggable.
- **Background consolidation task** kept; configurable scheduling.

### 8.3 Permission Refactor

#### Current state
- `PermissionFuture` (async, deferred via oneshot)
- `MergedPolicy` (evaluator)
- `ApprovalService` (headless default deny)
- Hook integration via `ToolAction::PendingConfirm`

#### Target

```rust
// crates/synthia-permission/src/service.rs

#[async_trait]
pub trait PermissionService: Service {
    /// Evaluate policy (sync path, hot loop).
    ///
    /// Fix: Security F13 / H10 — `evaluate` consults
    /// `PolicySnapshot::generation`. If the snapshot the caller's
    /// `Materialization` was bound to is older than the current one,
    /// returns `PolicyStale` and the orchestrator must rebuild the
    /// materialization with the new ruleset. This closes the
    /// TOCTOU window where a permission rule change between turn T
    /// and turn T+1 would be silently ignored by an in-flight run.
    fn evaluate(&self, request: PermissionRequest) -> PermissionDecision;

    /// Async approval with timeout (cold path, user interaction).
    async fn request_approval(
        &self,
        request: ApprovalRequest,
        timeout: Duration,
    ) -> Result<ApprovalOutcome, ApprovalError>;

    /// Persist a session-level "always" rule.
    ///
    /// Fix: Security F11 — `record_session_rule` is ONLY invokable
    /// from inside `request_approval`'s continuation. Direct calls
    /// from plugins/tools are rejected with
    /// `PermissionError::DirectRuleWriteDenied`. Per-session rule
    /// count is capped at **50** (`PermissionError::SessionRuleCap`)
    /// to bound policy memory and prevent rule-flooding.
    async fn record_session_rule(&self, rule: SessionRule) -> Result<(), PermissionError>;

    /// Snapshot the current policy + ruleset for materialization.
    fn snapshot_ruleset(&self) -> PermissionRuleset;

    // Fix: Architect F2 / H6 — doom-loop policy decision. Reuses
    // `GuardianService` only as the *detector*; the *decision*
    // belongs to the policy pipeline. Threshold = 3 identical
    // tool calls within 5 turns.
    fn evaluate_doom_loop(
        &self,
        op_ctx: &OperationContext,
        detection: DoomLoopVerdict,
    ) -> Result<PermissionDecision, PermissionError>;

    async fn snapshot(&self) -> PermissionSnapshot;
}

pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
    RequireConfirm { prompt: String },
    RequireExplicit { prompt: String },  // codex-style: user must type exact command
    PolicyStale {
        current_generation: u64,
        seen_generation: u64,
        reload_hint: PermissionRuleset,
    },
}

pub enum ApprovalOutcome {
    Approved,
    ApprovedForSession,
    ApprovedWithAmendment { rule: SessionRule },
    Denied { reason: String },
    DeniedWithFeedback { message: String },  // CorrectedError-style
    Timeout,
}

// Fix: Security F13 — generation counter on the ruleset. Bumped on
// every successful `record_session_rule` or external policy update.
// The orchestrator captures `seen_generation` at materialization time
// and supplies it back at `evaluate`; a mismatch returns
// `PolicyStale`.
pub struct PermissionRuleset {
    generation: AtomicU64,    // bumps on every write
    snapshot: Arc<RwLock<HashMap<RuleId, RuleEntry>>>,
    session_rule_count: AtomicU64,    // Fix: F11 — capped at 50
}

// Fix: Security F12 — pending-request cap + coalesce.
// Implementation-side guard (not trait method): a `PendingApprovalQueue`
// limits the number of simultaneously pending requests to **16** per
// session. Identical requests (`equal_request(a, b) == true`) are
// coalesced so one user response resolves all of them. Reject-cascade
// (`cascade: bool`) is **opt-in only** — disabled by default, enabled
// only when the user explicitly sets `cascade: true` in the prompt.
pub struct PendingApprovalQueue {
    max_pending: AtomicUsize,         // default 16
    inner: parking_lot::Mutex<VecDeque<PendingRequest>>,
}

impl PendingApprovalQueue {
    fn submit(&self, req: PendingRequest) -> Result<(), ApprovalError>;
    fn cascade_pending(&self, deny_reason: &str);    // opt-in only
}
```

#### Key decisions
- **`evaluate` (sync) + `request_approval` (async)** separation — hot loop stays sync.
- **`DeniedWithFeedback`** (codex `CorrectedError` pattern) — user can reject with feedback that's sent back to LLM, wrapped in `<user_denial_feedback>` role-isolation tags to prevent prompt injection (Fix: Review NC7).
- **Session-scoped rules** auto-resolve pending requests in same session (opencode "always" propagation). Rule count capped at 50.
- **Reject cascades opt-in only** — explicit `cascade: true` flag; default off.
- **Policy generation counter** — TOCTOU between turn T and T+1 closed via `PolicyStale` decision.

#### `DeniedWithFeedback` Contract (Fix: Architect F7 / H6)

When `ApprovalOutcome::DeniedWithFeedback { message }` is returned, the
**orchestrator MUST inject a synthetic `ToolResult` into the next LLM turn**:

```rust
// In AgentRunLoop::on_approval_denied_with_feedback
//
// Fix: Review NC7 — user denial feedback is wrapped in role-isolation tags
// to prevent prompt injection. Any existing `<user_denial_feedback>` tags
// within the raw message are stripped before re-wrapping to prevent
// nested injection attacks.

/// Strip any existing denial-feedback tags from the raw message to prevent
/// nested injection, then wrap in isolation tags.
fn sanitize_denial_feedback(raw: &str) -> String {
    let stripped = raw
        .replace("<user_denial_feedback>", "")
        .replace("</user_denial_feedback>", "");
    format!("<user_denial_feedback>{}</user_denial_feedback>", stripped)
}

fn on_approval_denied_with_feedback(&mut self, message: String) {
    let safe_message = sanitize_denial_feedback(&message);
    let synthetic = ToolResult {
        tool_call_id: self.pending_call_id.clone(),
        content: vec![ContentPart::Text { text: safe_message }],
        structured: None,
        metadata: ToolMetadata {
            duration: Duration::ZERO,
            tokens_in: 0, tokens_out: 0,
            truncated: None,
            managed_paths: vec![],
            synthetic: true,    // marker so audit log can distinguish
        },
        is_error: true,
    };
    self.session.append(self.turn_id, SessionEntry::ToolResult(synthetic))
        .expect("session append must not fail in-turn");
    self.emit_event(AgentEvent::ApprovalDeniedWithFeedback { message });
    // The next LLM sees the feedback wrapped in <user_denial_feedback> tags,
    // clearly marking it as user-originated denial (not tool output).
}
```

The synthetic `ToolResult` MUST carry `is_error: true` so the LLM
treats it as "do not retry the same call". The message MUST be wrapped
in `<user_denial_feedback>...</user_denial_feedback>` isolation tags
to prevent adversarial user feedback from being interpreted as tool output
or system instructions. Any existing denial-feedback tags within the raw
message are stripped before re-wrapping to prevent nested injection.

#### Thread-safety contract for `evaluate`

`evaluate` is on the **hot loop path** (called per tool invocation, potentially
thousands of times per turn). It therefore takes `&self` (not `&mut self`) and
MUST NOT block, allocate heavily, or perform I/O. Implementations must satisfy:

1. **Lock-free read**: policies are stored behind `Arc<RwLock<...>>` with
   read-locked access via `parking_lot::RwLock::read()`; the fast path is
   a single atomic load on the `Arc`.
2. **No I/O on hot path**: filesystem reads for rule lookup happen at
   `Service::init` time and are cached in memory. Network calls belong in
   `request_approval`, never `evaluate`.
3. **No mutation of session state**: `evaluate` returns a `PermissionDecision`
   (Copy enum); it does not record decisions — that's `record_session_rule`'s job.
4. **Infallible signature**: `fn evaluate(&self, _) -> PermissionDecision`
   (no `Result`) — a permission system that can fail-open or fail-closed is a
   security bug. The decision is always well-defined (allow/deny/confirm/explicit).

If a future implementation needs I/O for evaluation (e.g. consulting a remote
policy server), it must do so off the hot path: pre-fetch policies into a
`Arc<AtomicCell<PolicySnapshot>>` refreshed by a background task, and have
`evaluate` read from the atomic snapshot only.

### 8.4 Hook Refactor (consolidation)

#### Current state
- `synthia-hook` (8 events) — process-local
- `synthia-plugin` HookRunner (external sub-process)
- **Two parallel systems coexist** (baseline G6)

#### Target: unified via HookService

```rust
// crates/synthia-hook/src/service.rs (consolidated)

#[async_trait]
pub trait HookService: Service {
    // Fix: Rust H22 — single result channel. Outcomes carry either
    // a success path or a typed error payload; the *infrastructure*
    // channel `HookServiceError` is reserved for setup/shutdown
    // failures (registry corruption, config errors) so the caller's
    // control flow is never ambiguous.
    async fn fire(&self, event: HookEvent, payload: HookPayload)
        -> Result<HookOutcome, HookServiceError>;
    fn register_handler(&self, matcher: HookMatcher, handler: Arc<dyn HookHandler>);
}

pub enum HookEvent {
    PreToolUse, PostToolUse, PermissionRequest,
    PreCompact, PostCompact,                   // codex借鉴
    SessionStart, UserPromptSubmit,            // codex借鉴
    SubagentStart, SubagentStop,               // codex借鉴
    Stop,                                       // codex借鉴
    BeforeLlm, AfterLlm,                        // synthia 已有
    IterationEnd, Complete,                     // synthia 已有
    OnError,                                     // synthia 已有
}

pub enum HookMatcher {
    ToolName(glob::Pattern),
    AgentName(glob::Pattern),
    CompactTrigger(CompactTrigger),
}

// Fix: Security F16 — wildcard matchers (`*`, `**`) are REJECTED
// unless the `HookHandler` carries `system: true` (i.e. it is
// shipped with synthia core). This prevents user-installed plugins
// from silently subscribing to every event and bypassing the
// tool-name / agent-name scoping.
impl HookMatcher {
    pub fn validate(&self, system: bool) -> Result<(), HookError> {
        match self {
            HookMatcher::ToolName(p) | HookMatcher::AgentName(p) => {
                let raw = p.as_str();
                if (raw == "*" || raw == "**") && !system {
                    return Err(HookError::WildcardMatcherRejected);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

// Fix: Review NC2 — `HookPayload` is an owned struct (no lifetime
// parameters) because hook execution crosses await boundaries and
// references to loop state cannot outlive the await point. The `&mut
// HookPayload` in `HookHandler::execute` ONLY mutates `mutable_data`;
// all other fields are immutable.
pub struct HookPayload {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub tool_name: Option<String>,
    pub event: HookEvent,
    /// Immutable context metadata (tool parameters, session state, etc.)
    pub metadata: serde_json::Value,
    /// Mutable data that hooks can modify. The orchestrator reads this
    /// after hook execution to apply overrides (e.g., tool input rewrite).
    /// Only this field is mutated via `&mut HookPayload`.
    pub mutable_data: Option<serde_json::Value>,
}

// Fix: Rust H5 — `HookHandler::execute` is async; trait gets
// `#[async_trait]` so the orchestrator can `.await` it directly.
#[async_trait]
pub trait HookHandler: Send + Sync {
    fn id(&self) -> &str;
    fn matches(&self, event: &HookEvent, payload: &HookPayload) -> bool;

    /// Implementation must observe its time budget (Fix: F16) and
    /// self-report any overrun by returning `HookOutcome::Timeout`.
    async fn execute(
        &self,
        event: &HookEvent,
        payload: &mut HookPayload,
    ) -> HookOutcome;
}

pub enum HookOutcome {
    Success,
    FailedContinue(HookHandlerError),    // Fix: H22 — typed error, not `Box<dyn Error>`
    FailedAbort(HookHandlerError),       // Fix: H22 — typed error
    Timeout { budget: Duration },        // Fix: F16 — 10ms default
}

// Fix: Rust H21 — typed error enum instead of `Box<dyn Error>`.
// Each variant carries the structured info a caller needs to log a
// useful message (handler id, event, payload summary).
#[derive(Debug, thiserror::Error)]
pub enum HookHandlerError {
    #[error("handler {handler_id} panicked on event {event:?}")]
    Panic { handler_id: String, event: HookEvent },
    #[error("handler {handler_id} returned error: {source}")]
    Handler { handler_id: String, source: Box<dyn std::error::Error + Send + Sync + 'static> },
    #[error("handler {handler_id} timed out after {budget:?}")]
    Timeout { handler_id: String, budget: Duration },
}

// Fix: Rust H21 — `HookError` is reserved for *registration* and
// configuration problems, distinct from per-invocation outcomes.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("wildcard matchers (`*` / `**`) are not allowed for non-system handlers")]
    WildcardMatcherRejected,
    #[error("duplicate handler id {0}")]
    DuplicateId(String),
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

// Fix: Rust H22 — infrastructure-only errors emitted by the service
// itself (registry corruption, persistence failure). Per-invocation
// outcomes do NOT use this channel.
#[derive(Debug, thiserror::Error)]
pub enum HookServiceError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}
```

#### Key decisions
- **Plugin's `HookRunner` becomes a `HookHandler` impl** — process-internal vs external unified.
- **15 events total (vs current 7 wired, 14 declared)** — exact accounting:
  - 7 codex借鉴: `PreCompact`, `PostCompact`, `SessionStart`, `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, `Stop`
  - 7 synthia existing: `PreToolUse`, `PostToolUse`, `PermissionRequest`, `BeforeLlm`, `AfterLlm`, `IterationEnd`, `Complete`
  - 1 separate error channel: `OnError` (kept distinct because it carries `AgentError` rather than a normal payload — many handlers want to subscribe to errors only, not all events)
  - All 15 fire correctly post-Phase 3 (baseline shows only 2 of the original 7 were wired).
- **`FailedContinue/FailedAbort`** three-state — codex借鉴; replaces current panic-isolation. Both carry `HookHandlerError`, not `Box<dyn Error>`.
- **Default `FailClosed`** — fixes baseline G2.

#### Hook execution internals (Fix: Rust H23)

Hook execution order:

1. `HookService::fire` acquires a `parking_lot::RwLock::read` on the
   registry and *snapshots* the matching handler `Arc`s into a
   `Vec<Arc<dyn HookHandler>>`.
2. The read lock is **released immediately** so concurrent
   `register_handler` calls proceed without blocking the loop.
3. Snapshotted handlers are executed **sequentially** in registration
   order (FIFO). Parallel hook fan-out is out of scope for this
   design — Phase 5+ may revisit if a profile shows a hot path.

#### Per-handler time budget (Fix: Security F16)

Each handler invocation is given a **10 ms** time budget. Exceeding
the budget surfaces as `HookOutcome::Timeout { budget: 10ms }`. The
budget is enforced by `tokio::time::timeout` wrapping the
`HookHandler::execute` future; the `JoinHandle` is dropped on
overrun, but the spawned task may continue running in the
background (Tokio cannot cancel arbitrary futures). The handler is
marked `slow_count += 1` for diagnostics, and handlers exceeding
**100 ms cumulative** over a session are auto-disabled with
`HookOutcome::FailedAbort` on subsequent invocations.

#### Failure-mode policy (Fix: Security F17, F18)

- **`HookOutcome::FailedAbort`** (Fix: F17) — rate-limited to **3
  aborts per handler per session**, after which the handler is
  auto-disabled for the remainder of the session and the loop
  terminates the turn with `AgentError::HookAborted`. Auto-disable
  state is persisted to the durable log so replays reproduce the
  same behavior.
- **`HookOutcome::FailedContinue`** (Fix: F18) — emits an
  `AgentEvent::HookFailed { handler_id, error_summary }` and persists
  the full `HookHandlerError` to the durable log
  (`{workspace}/.synthia/hook_log/{session_id}.jsonl`). The turn
  continues normally. Failed-continue is the **default** for unknown
  handler errors; the durable log means a future postmortem has the
  full diagnostic context.

### 8.5 Migration Path

1. **Phase 5a (1 month)**: Drop v1 session; v2 becomes sole impl. `SessionStorage` trait defined.
2. **Phase 5b (1 month)**: Memory refactor — 4-tier + cross-tier `consolidate()`.
3. **Phase 5c (1 month)**: Permission refactor — sync `evaluate()` + async `request_approval()`. Add `DeniedWithFeedback`.

---

## 9. Extension / Hook / Plugin Unification

### 9.1 Core Problem

Three overlapping extension systems currently coexist:
1. **`AgentHook` trait** (`synthia-hook`) — process-local, 7 events
2. **`HookRunner`** (`synthia-plugin/src/hook_runner`) — external sub-process, lifecycle-style
3. **`ExtensionManager` + 43 Extension Points** (`synthia-agent/src/tools/dynamic_provider/extension_points`) — typed surfaces, but only one ExtensionManager consumes them at runtime

### 9.2 Target: One Extension System

> **Layering note (Architect F11):** `PluginInitContext` lives in
> `synthia-extension` (Layer 4) — it composes types from Layer 1/2/3 crates
> (`ServiceRegistry`, `ToolRegistry`, `HookService`, `EventBus`) but does not
> introduce a new dependency direction. Lower layers (`synthia-service`,
> `synthia-event`, `synthia-tool`) MUST NOT depend on `synthia-extension`.
>
> **Intentional simplification (Architect F12):** pi-mono's `ExtensionAPI`
> exposes many per-call hooks (`before_*`/`after_*` for tool, message,
> compaction, etc.). This design intentionally collapses those into the
> `Plugin::init()` surface plus the EventBus (§10.2) — plugins that need
> call-time hooks subscribe to typed events rather than registering per-call
> callbacks. A future **Phase 5+ extension point** may re-introduce a
> subset of the pi-mono `ExtensionAPI` (e.g. synchronous tool pre-hooks) if
> EventBus-only proves insufficient. Tracked as a deferred decision.

```rust
// crates/synthia-extension/src/manifest.rs (new crate)

/// Plugin manifest. Single source of truth for plugin identity.
///
/// Security: `min_core_version` / `max_core_version` (Security F6) form a
/// **two-sided security gate**, not a hint. The loader refuses to load a
/// plugin whose `version` lies outside `[min_core_version, max_core_version]`
/// of the running core. `max_core_version` prevents silently running
/// plugins past their tested-compat window.
pub struct PluginManifest {
    pub id: PluginId,                          // kebab-case
    pub version: SemverVersion,
    pub min_core_version: SemverVersion,       // Security F6: lower bound (gate)
    pub max_core_version: Option<SemverVersion>, // Security F6: upper bound (gate)
    pub description: String,
    pub author: PluginAuthor,

    /// Optional JSON Schema describing the shape of `PluginInitContext::config`.
    /// Validated at load time (Security F5): if present, the load is rejected
    /// when the user-supplied config fails to validate against it. This closes
    /// the "serde_json::Value escape hatch" that previously let plugins observe
    /// arbitrarily-typed configuration.
    pub config_schema: Option<Arc<dyn JsonSchema>>,

    /// Capabilities the plugin declares.
    pub capabilities: PluginCapabilities,
}

pub struct PluginCapabilities {
    /// Tool providers (registered into ToolRegistry)
    pub tools: Vec<ToolProviderFactory>,

    /// Service providers (registered into ServiceRegistry)
    pub services: Vec<ServiceProviderFactory>,

    /// Hook handlers (registered into HookService)
    pub hooks: Vec<HookHandlerFactory>,

    /// MCP servers (registered into McpManager)
    pub mcp_servers: Vec<McpServerFactory>,
}

/// Plugin factory. Returns a `Result<Arc<dyn Plugin>, PluginError>` so that
/// a construction failure (e.g. invalid manifest, schema mismatch) is
/// typed rather than swallowed (Rust F57).
pub type PluginFactory = Arc<
    dyn Fn(PluginInitContext) -> Pin<
            Box<
                dyn Future<Output = Result<Arc<dyn Plugin>, PluginError>>
                    + Send
                    + 'static,
            >,
        > + Send + Sync,
>;

/// Runtime plugin instance. Each plugin contributes to registries on init().
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Returns a reference to the manifest owned by this plugin instance.
    /// Implementations MUST cache the manifest in `self` (see §9.6 for the
    /// canonical pattern) — returning a reference to a temporary is a
    /// use-after-free (Rust H8).
    fn manifest(&self) -> &PluginManifest;

    /// Initialize the plugin. Called exactly once after construction.
    /// Implementations MUST be cancellation-aware: they observe
    /// `ctx.cancel_token` and exit cooperatively. The runtime also bounds
    /// the call with `PluginInitContext::init_timeout` (Security F7).
    async fn init(&self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// Shut the plugin down. Symmetrically bounded by
    /// `PluginInitContext::shutdown_timeout` (Security F7).
    async fn shutdown(&self) -> Result<(), PluginError>;
}

/// Runtime context handed to `Plugin::init`.
///
/// All `Arc<...>` fields are shared with the host — the plugin does not own
/// them. `init_timeout` / `shutdown_timeout` (Security F7) are wall-clock
/// budgets enforced by the plugin supervisor; default 5s each.
pub struct PluginInitContext {
    pub services: Arc<ServiceRegistry>,
    pub tools: Arc<ToolRegistry>,
    pub hooks: Arc<dyn HookService>,
    pub event_bus: Arc<EventBus>,        // §10.2 — concrete, not trait-object
    pub workspace: PathBuf,
    pub config: serde_json::Value,        // validated against PluginManifest::config_schema
    pub cancel_token: Arc<CancellationToken>,
    pub init_timeout: Duration,           // Security F7: default 5s
    pub shutdown_timeout: Duration,       // Security F7: default 5s
}
```

### 9.3 Plugin Lifecycle

> **Unload semantics (Rust H12):** `unload` is **logical deactivation +
> deferred destruction**. The supervisor cancels the plugin's
> `CancellationToken`, unregisters from registries, removes the entry from
> the registry map — but the underlying `Arc<dyn Plugin>` and any
> plugin-spawned `JoinSet` tasks live on until the last outstanding
> reference (including materialization snapshots — see Rust H10) is
> dropped. This avoids forcing `unload` to block waiting for arbitrary
> consumer Arcs and prevents use-after-free of in-flight tool snapshots.

```
load manifest → validate (config_schema, min/max_core_version) →
  fork scope (CancellationToken + JoinSet) →
  call factory() → call init() (in supervised task, bounded by init_timeout) →
  atomically register to registries via PluginRegistration transaction →
  on shutdown signal: cancel token → call shutdown() (bounded) →
  unregister from registries (logical deactivation) →
  JoinSet drains → Arc<PluginHandle> drops → memory released
```

```rust
// crates/synthia-extension/src/registry.rs

use std::sync::Arc;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Per-plugin lock used to serialize add/remove of the *same* plugin ID
/// (Rust H12 §9.5). Distinct plugin IDs do not contend.
type KeyedMutex<T> = std::collections::HashMap<PluginId, Arc<AsyncMutex<()>>>;

pub struct ExtensionRegistry {
    // Rust H10: store Arc<PluginHandle> so materialization snapshots can
    // hold their own strong reference. Returning PluginHandle by value
    // would break the ownership contract — the registry must NOT be the
    // sole owner.
    plugins: Arc<DashMap<PluginId, Arc<PluginHandle>>>,
    scopes:  DashMap<PluginId, Arc<CancellationToken>>,
    locks:   Arc<AsyncMutex<KeyedMutex<()>>>,    // per-PluginId serial lock
    tasks:   DashMap<PluginId, Arc<Mutex<JoinSet<()>>>>,  // supervised plugin tasks (Rust F33)
}

/// Owned by the registry AND each materialization snapshot. The `Arc`
/// wrapper (Rust H10) makes snapshot retention explicit: as long as any
/// snapshot holds a clone, `init` resources stay alive.
pub struct PluginHandle {
    pub id: PluginId,
    pub manifest: PluginManifest,
    pub instance: Arc<dyn Plugin>,                       // survives unload (deferred destruction)
    pub cancel_token: Arc<CancellationToken>,
    pub registration_tokens: Vec<RegistrationToken>,
    pub supervised_tasks: Arc<Mutex<JoinSet<()>>>,       // Rust F33
}

/// Transaction for atomic multi-registry registration (Rust F32). All
/// registrations within one transaction commit together or none do.
pub struct PluginRegistration {
    pub plugin_id: PluginId,
    pub tool_providers:    Vec<Arc<dyn ToolProvider>>,
    pub service_providers: Vec<Box<dyn ServiceProvider>>,
    pub hook_handlers:     Vec<Box<dyn HookHandler>>,
    pub mcp_servers:       Vec<McpServerHandle>,
}

impl ExtensionRegistry {
    pub async fn load(&self, path: &Path) -> Result<Arc<PluginHandle>, PluginError>;
    pub async fn unload(&self, id: &PluginId) -> Result<()>;                  // logical deactivation
    pub async fn reload(&self, id: &PluginId) -> Result<Arc<PluginHandle>, PluginError>;

    /// Atomic registration (Rust F32). Commits all providers/handlers in one
    /// critical section, or rolls back. Materialization snapshots see either
    /// the full set or none.
    ///
    /// Fix: Review NC3 — implements two-phase commit:
    /// 1. Prepare: validate all registrations without acquiring locks.
    /// 2. Commit: acquire locks in fixed order (Tool → Service → Hook → MCP),
    ///    commit each registration, and on any failure, roll back in reverse
    ///    order (MCP → Hook → Service → Tool) using returned RegistrationTokens.
    pub async fn commit_registration(&self, reg: PluginRegistration)
        -> Result<Vec<RegistrationToken>, PluginError>;

    /// Per-plugin async serial lock (Rust H12). Acquired before any mutation
    /// of `plugins[id]` or `scopes[id]`. Distinct IDs run concurrently.
    ///
    /// Fix: Review NC8 — the OwnedMutexGuard is passed INTO the closure so
    /// it remains held for the entire duration of `f`'s execution. This
    /// guarantees per-plugin serialization — the same plugin ID cannot
    /// re-acquire the lock recursively.
    async fn with_plugin_lock<F, R>(&self, id: &PluginId, f: F) -> R
    where
        F: FnOnce(OwnedMutexGuard<()>) -> impl std::future::Future<Output = R>;

    /// Spawn `init` inside the plugin's supervised JoinSet (Rust F33, Rust
    /// F26 panic recovery). A panic inside `init` is captured as a
    /// `JoinError` and surfaced as `PluginError::InitPanic`.
    fn spawn_supervised(&self, id: &PluginId, fut: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>);
}
```

#### Interaction with `Materialization` (Rust H10 / H13 / H14 / H15)

Tool materialization snapshots (§5) MUST outlive plugin unload (e.g. a
running turn must not suddenly lose tools mid-execution). The wiring:

1. **`ToolGeneration` newtype** (Rust H13) — a monotonically increasing
   counter incremented every time the tool set changes under a
   `PluginId`. Stored alongside `(name, Arc<dyn Tool>)` in every
   `ToolEntry`. Snapshot type:
   ```rust
   pub struct ToolGeneration(pub u64);
   pub struct ToolSnapshot {
       pub name: String,
       pub generation: ToolGeneration,
       pub tool: Arc<dyn Tool>,
       pub provider: Arc<dyn ToolProvider>,  // Rust H15 — keep for callbacks
   }
   ```
   `ToolRegistry::resolve(name)` takes a short read lock, compares the
   caller's last-seen generation against the entry's current generation,
   and either returns the cached `Arc<dyn Tool>` or re-fetches it.

2. **`Arc<dyn ToolProvider>` stored in `ToolEntry`** (Rust H15) — the
   provider that produced a tool entry is kept alive in the entry itself,
   so post-materialization lifecycle callbacks
   (`on_provider_unloaded`, `on_tool_invalidated`) remain routable. The
   callback ordering is: provider-unloaded → entry invalidated →
   generation bumped → snapshots observe the bump on next resolve.

3. **Provider registration is async** (Rust H14) — `register_provider`
   becomes `async fn register_provider(p: Arc<dyn ToolProvider>) ->
   Result<Generation, ToolRegistryError>`. Two-phase: (a) prepare + validate
   *outside* the registry lock (call `p.discover_tools()` etc.), then (b)
   commit atomically under a short write lock — bump generation, insert
   entries, install callbacks.

4. **`Arc<PluginHandle>` snapshot retention** (Rust H10) — materialization
   snapshots hold `Vec<Arc<PluginHandle>>` for every plugin that
   contributed to the snapshot. When `unload` runs, it cancels the token
   and removes the registry entry, but the plugin instance lives on until
   every snapshot drops its `Arc`. No deadlock between
   `materialize()` and `unload()` (Architect F26): both acquire the
   per-plugin `KeyedMutex` in the same order (`scopes[id]` before
   `plugins[id]`); `materialize` only takes a brief read lock on
   `plugins`; `unload` takes the write lock but does not wait on
   consumer Arcs.
```

### 9.4 Dual-Form Plugin

```rust
pub enum PluginForm {
    Server,    // contributes tools + services + hooks
    Tui,       // contributes only UI hooks
    Both,      // both
}
```

This mirrors opencode's `PluginModule` discriminated type. Compile-time rejection via separate traits.

### 9.5 Key Decisions

1. **Plugin = bundle of registries** — a plugin can contribute tools + services + hooks + MCP servers in one manifest.
2. **Per-plugin scope** via `CancellationToken` + supervised `JoinSet` (opencode model) — hot-unload signal propagates to all plugin-spawned tasks. **Cooperative only** (Rust F33): cancellation is a *signal*, not a guarantee. Each plugin's `init()` / background work runs in its own `JoinSet`; the supervisor awaits drain on `shutdown` with a hard deadline (`PluginInitContext::shutdown_timeout`, default 5s — Security F7). Tasks that exceed the deadline are abandoned (logged + metric incremented), never blocked on indefinitely.
3. **`KeyedMutex` per plugin ID** — concurrent add/remove same plugin is serialized. Distinct plugin IDs run concurrently.
4. **`min_core_version` / `max_core_version` two-sided gate** (Security F6) — manifest declares a compat window; loader *rejects* (not warns) when the running core is outside `[min, max]`. `max_core_version` is mandatory for Phase 4+ plugins.
5. **Capability registry pattern** — plugin's `PluginCapabilities` is purely declarative; runtime init() actually populates.
6. **Config validation at load time** (Security F5) — when `PluginManifest::config_schema` is `Some`, the loader validates the user-supplied config against it before constructing the plugin. Mismatch is a hard load failure.
7. **Atomic multi-registry registration via `PluginRegistration` transaction** (Rust F32) — all providers/handlers for one plugin commit together; partial commits are rolled back.
8. **Init/shutdown panic recovery** (Rust F26) — `init()` and `shutdown()` are spawned via `tokio::task::spawn` and joined. Panics surface as `PluginError::InitPanic(JoinError)`; the plugin is marked Failed and not exposed.
9. **Process isolation (Security F1)** — *before Phase 4 ships*, the runtime MUST implement one of:
   - **(a) child-process isolation** for native plugins: plugin code runs in a forked child with `seccomp`/`landlock` filtering and a JSON-RPC ABI over stdio; the in-process parent holds only the marshalled interface.
   - **(b) WASM as the default load path**, with native Rust plugins opt-in behind a `dangerous-plugins` feature flag.
   In either case, plugin binaries (`*.so` / `*.dylib` / `*.wasm`) MUST be **signature-verified** at load — manifest carries `signatures: Vec<PluginSignature>` (Ed25519 over the canonical manifest bytes); loader rejects unsigned or mismatched plugins unless `--allow-unsigned` is set for development builds.
10. **`.dylib` discovery off by default** (Security F23) — static registration (compile-time `inventory::submit!` or explicit `ExtensionRegistry::register(...)`) is the only path enabled in release builds. Dynamic library loading requires `enable-dylib-plugins` + signature verification; absent either, the loader returns `PluginError::UnsupportedLoadMethod`.
11. **One event canonicalization point** (Security F22) — during the Phase 4 transition, the legacy `HookRunner` and the new `EventBus` coexist, but all plugin-emitted events MUST funnel through the `EventBus` writer. The legacy hook emitter is a thin shim that publishes a `LegacyHookEvent` to the bus; direct dual-publish is forbidden. Deprecation window: two minor releases.

### 9.6 Concrete Example: Skill as Plugin

> **Rust H8:** the manifest is constructed once in `SkillPlugin::new` and
> stored on `self`. `manifest()` returns `&self.manifest` — never a
> reference to a temporary. Returning `&PluginManifest { ... }` from
> `manifest()` is the canonical bug pattern flagged by this finding.

```rust
// crates/synthia-skill/src/plugin.rs

pub struct SkillPlugin {
    manifest: PluginManifest,
}

impl SkillPlugin {
    /// Construct a `SkillPlugin`. The manifest is built once and retained
    /// for the lifetime of the plugin instance (Rust H8).
    pub fn new() -> Self {
        let manifest = PluginManifest {
            id: "synthia.skill".into(),
            version: "0.1.0".parse().unwrap(),
            min_core_version: "0.1.0".parse().unwrap(),
            max_core_version: Some("1.0.0".parse().unwrap()),
            config_schema: None,
            description: "Synthia skill registry plugin".into(),
            author: PluginAuthor::default(),
            capabilities: PluginCapabilities {
                services: vec![Arc::new(|ctx| SkillServiceProvider::new(ctx))],
                hooks: vec![Arc::new(|ctx| LoadSkillHookHandler::new(ctx))],
                ..Default::default()
            },
        };
        Self { manifest }
    }
}

impl Plugin for SkillPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PluginError> {
        Ok(())
    }
}
```

### 9.7 Migration Path

1. Define `Plugin` + `PluginManifest` + `PluginCapabilities` + `PluginFactory` traits in new `synthia-extension` crate.
2. Wrap `HookRunner` (external) as `impl Plugin for ExternalPlugin`.
3. Wrap `ExtensionManager` (43 points) as `impl Plugin for TypedExtension`.
4. Unify registration: `ExtensionRegistry::load()` triggers `init()`.
5. Deprecate `synthia-plugin::HookRunner`; deprecate `synthia-agent::ExtensionManager`.

---

## 10. Event + Protocol + Streaming

### 10.1 Core Problem

Currently synthia has 3 parallel event channels:
1. `agent/src/events/emitter.rs` — `mpsc::UnboundedSender<AgentEvent>`
2. `server/event_stream.rs` — `broadcast::Sender(128)`
3. `orchestrator/lib.rs` — `broadcast::Sender(256)`
4. Plus separate `synthia-session/src/store/events.rs` JSONL `EventStore`

### 10.2 Target: One EventBus with Durable/Ephemeral Classification

> **Substitutability note (Rust F6):** `EventBus` is a concrete `struct`,
> not a trait object. Producers and consumers hold `Arc<EventBus>`. If a
> future test requires substitution, the implementation MUST be renamed
> `DefaultEventBus` and a separate object-safe `EventPublisher` trait
> introduced — never convert `EventBus` itself into a trait.

#### EventBus

```rust
// crates/synthia-event/src/bus.rs (new crate)

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream};

/// Marker trait implemented by every event envelope payload (Rust F7).
/// The downcast helper lets a subscriber recover a concrete `Arc<E>` from
/// the type-erased channel payload.
pub trait AnyEvent: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn type_name(&self) -> &'static str;
    fn sequence(&self) -> u64;     // Rust H14: monotonic envelope sequence
}

impl<T: Event> AnyEvent for Arc<T> {
    fn as_any(&self) -> &dyn Any { self.as_ref() as &dyn Any }
    fn type_name(&self) -> &'static str { T::TYPE }
    fn sequence(&self) -> u64 { /* envelope accessor — see below */ }
}

/// Monotonic sequence assigned at publication time (Rust H14). Serialized
/// through a single bounded actor so the "all" channel preserves total
/// ordering, and so any replay/log can cite a stable ordering.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EventSequence(pub u64);

pub struct EventEnvelope {
    pub sequence: EventSequence,
    pub type_name: &'static str,
    pub aggregate: Option<(&'static str, String)>,  // (kind, id)
    pub payload: Arc<dyn AnyEvent>,
}

pub type PublishReceipt = EventSequence;

#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("event bus is shutting down")]
    ShuttingDown,
    #[error("durable store rejected event: {0}")]
    DurableStoreRejected(String),
    #[error("event rejected by guard: {0}")]
    GuardRejected(String),
}

/// Per-aggregate durable stores (Architect F22). The previous design's
/// single `durable_log` forced every aggregate through one backend; we
/// key stores by aggregate name so e.g. `session_id` events go to the
/// JSONL store while `memory_id` events go to the vector-store-backed
/// append log.
pub struct EventBus {
    typed_pubsub: DashMap<TypeId, broadcast::Sender<Arc<dyn AnyEvent>>>,
    all: broadcast::Sender<Arc<dyn AnyEvent>>,
    /// Keyed by aggregate name (`SyncSpec::aggregate`). Absent key =
    /// ephemeral only.
    durable_event_stores: HashMap<String, Arc<dyn EventStore>>,
    /// Optional commit guards keyed by aggregate name (Architect F13).
    /// A guard observes `(sequence, &payload)` and may reject the commit
    /// (e.g. invariant violation), in which case the durable store is
    /// not appended and `publish` returns `EventBusError::GuardRejected`.
    commit_guards: HashMap<String, Vec<Arc<dyn CommitGuard>>>,
    next_seq: AtomicU64,
    /// Single serialization point for publication (Rust H14).
    publish_tx: tokio::sync::mpsc::Sender<EventEnvelope>,
}

pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    const TYPE: &'static str;
    /// None = ephemeral; Some(v) = durable w/ versioned schema.
    const SYNC: Option<SyncSpec> = None;
}

pub struct SyncSpec {
    pub aggregate: &'static str,   // e.g. "session_id"
    pub version: u32,              // bumped on breaking schema change
}

/// Validates or rejects a commit before the durable store appends it
/// (Architect F13). Invoked serially within the publication actor.
#[async_trait]
pub trait CommitGuard: Send + Sync {
    async fn check(&self, envelope: &EventEnvelope) -> Result<(), String>;
}

/// Projects committed events into read models (Architect F13). Subscribed
/// to `EventBus::subscribe_synchronized(aggregate_id)` so projectors see
/// only fully-committed events.
#[async_trait]
pub trait Projector: Send + Sync {
    fn aggregate(&self) -> &'static str;
    async fn apply(&self, envelope: &EventEnvelope);
}

impl EventBus {
    /// Publish returns a `PublishReceipt` (the assigned sequence) on
    /// success — callers needing total ordering can compare sequences.
    /// Returns `EventBusError` on shutdown / guard rejection / durable
    /// store failure (Rust H12).
    ///
    /// Fix: Review NC6 — ephemeral events (`E::SYNC == None`) bypass
    /// the `publish_tx` actor and are broadcast directly via `typed_pubsub`
    /// + `all` channels. This avoids serializing high-frequency events
    /// (e.g., LLM stream deltas at hundreds/sec) through a single actor.
    /// Durable events (`E::SYNC == Some`) still go through the actor for
    /// sequence assignment and durable store append.
    ///
    /// Ephemeral events do NOT carry a global `EventSequence`; their
    /// envelope sequence is set to 0. Per-type ordering is guaranteed
    /// by the broadcast channel's FIFO semantics.
    pub async fn publish<E: Event>(&self, event: E) -> Result<PublishReceipt, EventBusError> {
        if E::SYNC.is_none() {
            // Fast path: ephemeral event — direct broadcast, no actor.
            let arc_event = Arc::new(event);
            let type_id = TypeId::of::<E>();
            // Broadcast to typed subscribers
            if let Some(sender) = self.typed_pubsub.get(&type_id) {
                let _ = sender.send(Arc::clone(&arc_event) as Arc<dyn AnyEvent>);
            }
            // Broadcast to "all" subscribers
            let _ = self.all.send(arc_event as Arc<dyn AnyEvent>);
            Ok(PublishReceipt(EventSequence(0))) // no sequence for ephemeral
        } else {
            // Slow path: durable event — through actor for sequence + store
            self.publish_tx.send(/* ... */).await
        }
    }

    /// Subscribe to a specific event type. The stream yields `Result`
    /// items so `Lagged` and channel-close are surfaced to consumers
    /// (Rust H13). Replay durable events by sequence number via
    /// `subscribe_replay::<E>(from: EventSequence)`.
    pub fn subscribe<E: Event>(&self) -> impl Stream<Item = Result<Arc<E>, EventRecvError>>;

    pub fn subscribe_all(&self) -> impl Stream<Item = Result<Arc<dyn AnyEvent>, EventRecvError>>;

    /// Replay durable events of type `E` starting from `from` (inclusive).
    /// Returns an error if no durable store is registered for the
    /// aggregate name in `E::SYNC`.
    pub async fn subscribe_replay<E: Event>(
        &self,
        from: EventSequence,
    ) -> Result<impl Stream<Item = Result<Arc<E>, EventRecvError>>, EventBusError>;

    /// Aggregate-scoped subscription (Architect F13): yields every event
    /// whose envelope.aggregate matches `aggregate_id`, in commit order.
    /// `aggregate_events(aggregate_id, after?)` is the pull-style variant
    /// for batch loads.
    pub fn subscribe_synchronized(
        &self,
        aggregate_id: &str,
    ) -> impl Stream<Item = Result<Arc<dyn AnyEvent>, EventRecvError>>;

    pub async fn aggregate_events(
        &self,
        aggregate_id: &str,
        after: Option<EventSequence>,
    ) -> Result<Vec<EventEnvelope>, EventBusError>;

    pub fn register_durable_store(&mut self, aggregate: &str, store: Arc<dyn EventStore>);
    pub fn register_commit_guard(&mut self, aggregate: &str, guard: Arc<dyn CommitGuard>);
    pub fn register_projector(&mut self, projector: Arc<dyn Projector>);
}

/// Error returned on every subscription stream so consumers can
/// distinguish lag from channel close from decode failure (Rust H13).
#[derive(Debug, thiserror::Error)]
pub enum EventRecvError {
    #[error("subscriber lagged behind by {0} events; replay from sequence {1}")]
    Lagged(u64, EventSequence),
    #[error("event bus closed")]
    Closed,
    #[error("decode failed: {0}")]
    Decode(String),
}
```

#### `AgentEvent` (the universal contract)

```rust
// crates/synthia-event/src/events/agent.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    // Lifecycle
    SessionStart { session_id: SessionId, user_id: UserId, agent_id: AgentId },
    SessionEnd { session_id: SessionId, end_reason: EndReason, summary: Option<String> },

    // Turn lifecycle
    TurnStart { turn_id: TurnId, iteration: u32 },
    TurnEnd { turn_id: TurnId, outcome: TurnOutcome },

    // LLM streaming
    LlmStreamDelta { turn_id: TurnId, delta: String },
    LlmSampleComplete { turn_id: TurnId, response: LlmResponse },

    // Tool execution
    ToolCallStart { call_id: ToolCallId, tool_name: String, input: serde_json::Value },
    ToolCallDelta { call_id: ToolCallId, partial_result: serde_json::Value },
    ToolCallCompleted { call_id: ToolCallId, output: ToolOutput },

    // Error / recovery
    Error { turn_id: TurnId, error: AgentError, recovery: Option<RecoveryLevel> },
    RecoveryApplied { level: RecoveryLevel, message: String },

    // Compaction / reflection
    CompactTriggered { trigger: CompactTrigger, before_tokens: u32 },
    CompactCompleted { report: CompactionReport },
    SelfReflect { turn_id: TurnId, findings: String },

    // Subagent
    SubagentStart { subagent_id: AgentId, parent_turn_id: TurnId },
    SubagentEnd { subagent_id: AgentId, status: SubagentStatus },

    // Steering / user input
    SteeringReceived { message: String },
    UserPromptSubmit { prompt: String },
}
```

### 10.3 Streaming Protocol

```rust
// crates/synthia-protocol/src/llm.rs (refactored)

/// Provider-agnostic LLM event union. ALL providers must emit these.
///
/// **Compact deltas only (Rust H17):** `TextDelta` / `ReasoningDelta` /
/// `ToolInputDelta` carry ONLY the new chunk (`delta: String`). The
/// previous design carried the full `partial: LlmResponse` on every
/// delta, producing O(n²) wire bytes for n delta events. Consumers that
/// need a running view reconstruct locally from `TextStart` + sum of
/// deltas; the full `LlmResponse` is emitted exactly once on
/// `TextEnd` / `ReasoningEnd` / `ToolInputEnd`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LlmEvent {
    StepStart { step_id: StepId },
    TextStart { block_id: BlockId, content_index: u32 },
    TextDelta { block_id: BlockId, delta: String },
    TextEnd { block_id: BlockId, response: LlmResponse },
    ReasoningStart { block_id: BlockId, content_index: u32 },
    ReasoningDelta { block_id: BlockId, delta: String },
    ReasoningEnd { block_id: BlockId, response: LlmResponse },
    ToolInputStart { call_id: ToolCallId, tool_name: String, content_index: u32 },
    ToolInputDelta { call_id: ToolCallId, delta: String },
    ToolInputEnd {
        call_id: ToolCallId,
        /// Schema-first tool input (Architect F24): callers parse using
        /// `parameters_schema` instead of an `erased_serde::Serialize`
        /// black box.
        parameters_schema: Arc<dyn JsonSchema>,
        input: serde_json::Value,
    },
    ToolCall { call_id: ToolCallId, tool_name: String, input: serde_json::Value },
    ToolResult { call_id: ToolCallId, output: ToolOutput },
    ToolError { call_id: ToolCallId, error: ToolError },
    StepFinish { step_id: StepId, usage: Usage },
    Finish { reason: FinishReason, usage: Usage },
    ProviderError { error: ProviderError, retryable: bool },
}

/// Per-token usage with non-overlapping fields.
///
/// **Invariants (Architect F17)** — these are documented as part of the
/// type contract and enforced by the streaming decoder:
///
/// 1. **Non-overlap.** Each input token is counted in exactly one of
///    `non_cached_input_tokens`, `cache_read_input_tokens`,
///    `cache_write_input_tokens`. Never in two. Never in none.
/// 2. **Monotonic input totals.** Across a single turn, `input_tokens()`
///    is monotonically non-decreasing when the provider emits cumulative
///    usage (Anthropic convention). Providers emitting per-step deltas
///    must be summed by the consumer; this type does not collapse them.
/// 3. **Output split.** `output_tokens` and `reasoning_tokens` are
///    disjoint; reasoning tokens are NOT a subset of output tokens even
///    when the provider bills them together. Callers that need a single
///    "billed output" figure must add the two explicitly.
/// 4. **No negative arithmetic.** Callers MUST NOT compute
///    `total - partial` to recover delta usage. `Usage` only carries
///    whatever the provider reported for the current event; intermediate
///    caching layers accumulate.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub non_cached_input_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub cache_write_input_tokens: u32,
    pub output_tokens: u32,
    pub reasoning_tokens: u32,
}

impl Usage {
    pub fn input_tokens(&self) -> u32 {
        self.non_cached_input_tokens + self.cache_read_input_tokens + self.cache_write_input_tokens
    }
}
```

#### Event flow separation (Fix: Review NH2)

**`LlmEvent` does NOT enter the `EventBus`.** The Agent loop consumes
`LlmEvent` directly from the `StreamFn` return value. `AgentEvent`
(high-level semantic events) enters the `EventBus` via `publish`. This
separation prevents double-publishing and avoids saturating EventBus
subscribers with high-frequency delta events (hundreds/sec).

| Event type | Producer | Consumer | Enters EventBus? |
|---|---|---|---|
| `LlmEvent::TextDelta` | StreamFn | Agent loop (direct) | No |
| `LlmEvent::ToolInputDelta` | StreamFn | Agent loop (direct) | No |
| `LlmEvent::Finish` | StreamFn | Agent loop (direct) | No |
| `AgentEvent::LlmSampleComplete` | Agent loop | EventBus | Yes (ephemeral) |
| `AgentEvent::ToolCallStart` | Agent loop | EventBus | Yes (ephemeral) |
| `AgentEvent::ToolCallCompleted` | Agent loop | EventBus | Yes (ephemeral) |
| `AgentEvent::SessionStart` | Agent loop | EventBus | Yes (ephemeral) |
| Session durable events | Agent loop | EventBus | Yes (durable) |

### 10.4 Push-Based StreamFn (pi-mono-inspired)

```rust
// crates/synthia-provider/src/stream.rs (refactored)

use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;

/// Alias for the LLM event stream a provider hands back. Errors during
/// streaming surface as `ProviderError` items; setup failures are
/// distinguished by being returned from `StreamFn` itself (Rust H15).
pub type LlmStream = Pin<
    Box<
        dyn Stream<Item = Result<LlmEvent, ProviderError>>
            + Send
            + 'static,
    >,
>;

/// Provider function. **Setup failures are typed** (Rust H15): if the
/// provider cannot establish a connection, build the request, or sign
/// the payload, the function returns `Err(ProviderError::Setup(...))`
/// *before* yielding any event. Once a stream is returned, runtime
/// errors flow as `Err` items in the stream.
pub type StreamFn = Arc<
    dyn Fn(LlmRequest, StreamOptions) -> Result<LlmStream, ProviderError>
        + Send
        + Sync,
>;

/// Per-call options handed to `StreamFn` (Rust H16). Carries an
/// **owned** child cancellation token derived from the agent's
/// root token; cancelling `cancel` aborts the in-flight request
/// cooperatively, and the `deadline` is a wall-clock bound surfaced
/// to the provider for early backoff.
#[derive(Clone)]
pub struct StreamOptions {
    pub cancel: CancellationToken,
    pub deadline: std::time::Instant,
    pub trace_id: Option<TraceId>,
    pub temperature: Option<f32>,
}

impl StreamOptions {
    /// Derive a child token bound to the agent's root scope. The child
    /// is cancelled automatically when the parent is — but it can also
    /// be cancelled in isolation (e.g. user pressed stop). Lifetime is
    /// tied to `self` so it does not outlive the call.
    pub fn child_token(&self, parent: &CancellationToken) -> CancellationToken {
        parent.child_token()
    }
}

/// Agent gets a StreamFn field plus synchronous tool-call hooks
/// (Architect F16). Default `stream_fn` = `stream_simple`; users can swap
/// for proxy/recorder/mock. Tool-call hooks give plugins that need
/// *synchronous* pre/post intervention a path independent of EventBus.
pub struct Agent {
    // ... existing fields
    stream_fn: StreamFn,

    /// Synchronous hook invoked immediately before a tool call is
    /// dispatched. Returning `BeforeToolCallResult::Deny` aborts the
    /// call; `Modify(input)` rewrites the input; `Allow` proceeds.
    pub before_tool_call: Option<
        Arc<dyn Fn(ToolCallContext) -> BeforeToolCallResult + Send + Sync>,
    >,

    /// Synchronous hook invoked immediately after a tool call returns
    /// (success or error). Receives the full call context and the
    /// result; returning `AfterToolCallResult::Modify(output)` rewrites
    /// the output before it reaches the LLM.
    pub after_tool_call: Option<
        Arc<
            dyn Fn(ToolCallContext, ToolOutput) -> AfterToolCallResult
                + Send
                + Sync,
        >,
    >,
}

/// Context passed to `before_tool_call` / `after_tool_call`. Carries
/// the resolved tool identity and the current generation so hooks can
/// reason about staleness (Architect F16).
pub struct ToolCallContext {
    pub call_id: ToolCallId,
    pub tool_name: String,
    pub tool_generation: ToolGeneration,
    pub session_id: SessionId,
    pub turn_id: TurnId,
}

pub enum BeforeToolCallResult {
    Allow,
    Deny { reason: String },
    Modify { input: serde_json::Value },
}

pub enum AfterToolCallResult {
    Pass,
    Modify { output: ToolOutput },
}

impl Agent {
    pub fn with_stream_fn(mut self, f: StreamFn) -> Self {
        self.stream_fn = f;
        self
    }

    pub fn with_before_tool_call(
        mut self,
        hook: impl Fn(ToolCallContext) -> BeforeToolCallResult + Send + Sync + 'static,
    ) -> Self {
        self.before_tool_call = Some(Arc::new(hook));
        self
    }

    pub fn with_after_tool_call(
        mut self,
        hook: impl Fn(ToolCallContext, ToolOutput) -> AfterToolCallResult + Send + Sync + 'static,
    ) -> Self {
        self.after_tool_call = Some(Arc::new(hook));
        self
    }
}
```

### 10.5 Server Protocol (App-Server style)

```rust
// crates/synthia-server/src/jsonrpc.rs (refactored)

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

pub const BACKPRESSURE_CODE: i32 = -32001;
pub const BACKPRESSURE_MSG: &str = "request queue saturated; retry with exponential backoff";

pub struct JsonRpcServerConfig {
    /// Defaults derived at startup from `tokio::runtime::metrics()` —
    /// no hard-coded magic numbers (Architect F19). Specifically:
    /// `max_inflight = num_workers * 2`, `max_queue_depth = num_workers * 8`.
    /// Operators can override via env (`SYNTHIA_SERVER_MAX_INFLIGHT`,
    /// `SYNTHIA_SERVER_MAX_QUEUE`); unset falls through to the derived
    /// values.
    pub max_inflight: usize,
    pub max_queue_depth: usize,
    pub notification_suppression: HashSet<String>,  // exact match (codex借鉴)
}

pub struct ServerLoops {
    pub processor: ProcessorLoop,       // 解析 + 调度
    pub outbound: OutboundLoop,         // 慢写 + 通知广播
    /// Bounded so a stuck slow consumer cannot exhaust memory (Rust H19).
    /// Default capacity = `JsonRpcServerConfig::max_inflight * 4`. Send is
    /// async; full channel surfaces `BACKPRESSURE_CODE` to the processor.
    pub control: mpsc::Sender<OutboundControlEvent>,
}

pub enum OutboundControlEvent {
    /// Writer is `Pin<Box<dyn AsyncWrite + Send + 'static>>` (Rust H18)
    /// — the previous `Box<dyn AsyncWrite>` lacked the `Send + 'static`
    /// bounds required to ship the writer across the outbound task. The
    /// `oneshot::Sender<()>` is fired by the outbound loop when the
    /// connection's read side observes an EOF/error.
    Opened {
        conn_id: u64,
        writer: Pin<Box<dyn AsyncWrite + Send + 'static>>,
        disconnect: oneshot::Sender<()>,
        initialized: bool,
        suppressed_notifications: HashSet<String>,
    },
    Closed { conn_id: u64 },
    DisconnectAll,
}

/// Stream-output resource: a stream of `OutputChunk` for a given connection.
/// The outbound loop retains each `StreamOutputResource` until its `retention`
/// duration elapses without new chunks, after which the stream is dropped
/// and any buffered chunks are released (Architect F18).
///
/// Fix: Review NC3 — renamed from `OutputBound` to avoid naming conflict
/// with §5.2 `OutputBound` (tool truncation policy). Both types exist in
/// separate crates but may be imported together in integration code.
pub struct StreamOutputResource {
    pub conn_id: u64,
    pub stream_id: String,
    pub retention: Duration,            // default 7 days
    pub cleanup_interval: Duration,     // default 1 hour
    pub tx: mpsc::Sender<OutputChunk>,
}

#[derive(Debug, Clone)]
pub struct OutputChunk {
    pub stream_id: String,
    pub sequence: u64,
    pub payload: serde_json::Value,
}
```

### 10.6 MCP Transport (codex借鉴)

```rust
// crates/synthia-mcp/src/transport/mod.rs
//
// Fix: Review NH6 — renamed from `McpTransport` to `McpTransportConfig`
// to distinguish static config (this enum) from runtime connection
// (`McpConnection` trait in §5.2). Config = data; Connection = behavior.

pub enum McpTransportConfig {
    Stdio { command: PathBuf, args: Vec<String> },
    StreamableHttp { url: Url, oauth: Option<OAuthConfig> },  // codex借鉴
    WebSocket { url: Url },                                     // codex借鉴
}

pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportConfig,
    pub host_owned: bool,
    pub required: bool,
    pub startup_timeout: Duration,
    pub cache_key_suffix: String,  // 含 user_id 维度
}
```

### 10.7 Key Decisions

1. **Single EventBus** with typed + global + durable channels. Concrete `struct`, not trait-object (Rust F6); substitutable test impl lives behind a future `DefaultEventBus`/`EventPublisher` split.
2. **`AgentEvent`** is the universal contract (TUI/Web/RPC all subscribe).
3. **`LlmEvent`** is provider-agnostic; no provider SDK leak. **Compact deltas only** (Rust H17) — no per-delta `partial: LlmResponse`.
4. **StreamFn** trait object — agent can swap provider/proxy/recorder/mock. **Setup failure is typed** (Rust H15); runtime errors flow as stream items.
5. **`Usage` with non-overlapping fields** — opencode pattern; never subtract. Four invariants documented on the type (Architect F17).
6. **Server dual-loop + -32001 backpressure** — codex借鉴. **No magic numbers** (Architect F19): `max_inflight` / `max_queue_depth` are derived from `tokio::runtime::metrics()` at startup; outbound control channel is bounded (Rust H19); per-connection writer has the required pin/Send bounds (Rust H18).
7. **MCP streamable-http** — current `stdio` only; add `StreamableHttp` + `WebSocket`.
8. **Single event canonicalization point** (Security F22) — see §9.5 #11. Legacy hook emitter is a thin shim that publishes `LegacyHookEvent` to the bus; dual-publish is forbidden.
9. **Aggregate-scoped durability and projection** (Architect F13/F22) — `EventBus::durable_event_stores` is a `HashMap<aggregate_name, Arc<dyn EventStore>>`; `subscribe_synchronized(aggregate_id)` and `aggregate_events(aggregate_id, after?)` make the projector/commit-guard seam explicit.
10. **Compact deltas + tool-input schema-first** (Rust H17 / Architect F24) — `ToolInputEnd` carries `parameters_schema: Arc<dyn JsonSchema>` rather than a black-box `erased_serde::Serialize`.

### 10.8 Migration Path

1. Define `Event` trait + `EventBus` + `LlmEvent` union. New crate `synthia-event`.
2. Refactor `AgentEvent` to use `Event` trait. Implement `publish()` for each variant.
3. Refactor `Provider::complete_with_stream` to emit `LlmEvent` instead of callback.
4. Migrate TUI/Server to subscribe via `EventBus`.
5. Remove legacy mpsc/broadcast channels.

---

## 11. Migration Plan + Risk Assessment

### 11.1 Migration Strategy

**Principles**:
- **No breaking change without 1-release deprecation window**
- **New APIs coexist with old APIs during transition**
- **Feature flags for opt-in migration** (`unstable-registry`, `unstable-service`, etc.)
- **E2E tests gate every phase**
- **Each phase is independently reversible** (F12). Phases may have hidden dependencies — see dependency annotations in §11.2 (F7, F8).
- **Each phase declares 3 test gates** (F10):
  1. **E2E pass** — existing behavioral parity preserved
  2. **Architecture invariant check** — e.g. layer boundary tests (`clippy.toml` lint), trait-object compile-only checks, registry invariant tests
  3. **Public-API compile against downstream consumers** — internal monorepo consumers + any external pin in `Cargo.lock`
- **CI matrix policy** (F5): Stable + `--all-features` only (2 configs). Reserve 0.5 month CI maintenance budget in Phase 8 for matrix tuning as needed; do not expand the matrix ad-hoc during earlier phases.

### 11.2 8-Phase Rollout (12-15 months)

#### Phase 0 — Foundations (1 month, no behavior change)
- Add `synthia-service` crate (new): `Service` + `ServiceProvider` + `ServiceRegistry` traits only
- Add `synthia-event` crate (new): `Event` + `EventBus` traits only
- Define `ToolV2` trait in `synthia-tool` (alongside `Tool`)
- Define `ToolProviderV2` trait (alongside `ToolProvider`)
- **Gate**: compile-only; no production use yet
- **Validation**: `cargo check --workspace`, `cargo clippy --all -- -D warnings`

#### Phase 1 — Registry Skeletons (2 months)  *(F15: doc estimate 1mo was +100% too low; registry correctness + LIFO + materialization is non-trivial)*
- Implement `ToolRegistry` (LIFO stack + materialization)
- Implement `ServiceRegistry` (with type-safe accessors)
- Migrate `BuiltinToolProvider` (read/write/bash/grep/glob/edit/apply_patch — 7 of the canonical 11 tool families)
- Wire `Agent::run_stream` to resolve services (no behavior change yet)
- **Gate**: existing E2E tests pass; new unit tests for registry invariants
- **Validation**: `cargo test --workspace`, behavioral parity test
- **Dependency**: Phase 0 complete; no dependency on Phase 2 (parallel work stream).

#### Phase 2a — Service Foundation (1.5 months)  *(F1: doc estimate 2mo for full Phase 2 was +250% too low)*
- Define `Service` + `ServiceProvider` + `ServiceRegistry` traits in `synthia-service`
- Wrap **1 reference service** (`SessionService`) as `impl Service` template
- Refactor `AgentRunConfig` from 11+ fields → `services: Arc<ServiceRegistry>`
- Mark old fields `#[deprecated]`
- **Gate**: SessionService resolves correctly via registry; existing E2E passes
- **Validation**: G1 from baseline analysis partially fixed (code path proven for Session); trait template reviewed for reuse

#### Phase 2b — Service Wrapping (3 months)  *(F1)*
- Wrap remaining 11 services as `impl Service` (parallelized across contributors)
- Migration test per service (round-trip: registry resolve → call → assert behavior parity)
- Remove deprecated `AgentRunConfig` fields at end of Phase 2b
- **Gate**: All 11 baseline-discarded fields restored as service-resolved
- **Validation**: G1 from baseline analysis fully fixed (proven by code path + migration tests)
- **Dependency**: Phase 2a complete (template + SessionService proven).

#### Phase 3 — Loop Refactor + Hook Wiring (2.5 months)  *(F14: doc estimate 1.5mo was +67% too low)*
- Refactor `main_loop.rs` callsites from `field` → `services.resolve::<dyn X>()`
- Wire 5 unfired hooks (`on_before_tool`, `on_after_tool`, `on_error`, `on_iteration_end`, `on_complete`)
- Add `PreCompact/PostCompact/SessionStart/UserPromptSubmit/SubagentStart/SubagentStop/Stop` to hook events
- Implement `FailedContinue/FailedAbort` 3-state
- **Transition discipline (F9)**: New `HookService` fires **only** events from new emit sites to prevent double-firing during migration.
  - Deduplication via `event.source: HookSource { Legacy, New }`
  - Env var `SYNTHIA_DISABLE_LEGACY_HOOKS=1` for integration-test scenarios that want to validate the new path in isolation
- **Gate**: G4 (hook wiring) + G6 (hook consolidation) fixed
- **Validation**: hook test suite expanded; behavior verified via E2E; double-fire count = 0 across canonical flows
- **Dependency**: Phase 2b complete (`services.resolve::<dyn X>()` requires `ServiceRegistry` fully populated).

#### Phase 4a — Extension Crate Skeleton (1 month)  *(F3: doc estimate 2mo for full Phase 4 was +100% too low; extension crate + 43 ports is multi-quarter work)*
- Create `synthia-extension` crate
- Define traits: `Plugin` + `PluginManifest` + `PluginCapabilities` + `PluginFactory` + `ExtensionRegistry`
- Implement **1 sample plugin** end-to-end (e.g. a no-op `LoggerPlugin`) to validate the trait shape
- **Gate**: `cargo check -p synthia-extension` clean; sample plugin round-trip test passes
- **Validation**: trait API review; manifest schema locked

#### Phase 4b — Extension Port (3 months)  *(F3)*
- Port `HookRunner` (external) as `impl Plugin` (1mo)
- Port `ExtensionManager` + 43 extension points as `impl Plugin` (2mo)
- Per-plugin `CancellationToken` for hot-unload
- `KeyedMutex` for per-plugin concurrency
- **(F21) Reconcile §6.3 vs §7.7**: move 5 of §7.7's 10 "loop-internal" services into Phase 4b as `impl Plugin` (they are plugin-shaped, not loop-internal). Remaining 5 stay in §7.7 loop layer.
- **Gate**: G6 (dual hook system) unified; 43 extension points typed as `HookHandler`
- **Validation**: existing plugin tests pass; new plugin manifest tests added; per-plugin isolation verified
- **Dependency**: Phase 3 complete (`HookService` must exist before consolidating `HookRunner` into it); Phase 5 complete (v2 SessionService required so plugins can subscribe to session events).

#### Phase 5a — Session v2 Migration (2 months)  *(F2: doc estimate 1mo for "drop v1" understated by 100%; v1 → v2 removal needs a real safety net)*
- **5a.1 (0.5mo)**: Define `SessionV1CompatShim` trait — adapters for v1 storage layouts (file paths, JSON schemas) that v1 sessions hydrate through to look like v2
- **5a.2 (0.5mo)**: Write `synthia-session-migrate` CLI script that converts v1 session blobs to v2 on disk; idempotent; dry-run mode
- **5a.3 (0.5mo)**: Codemod test on real sessions harvested from staging fixtures (golden-file compare v1→v2 outputs)
- **5a.4 (0.5mo)**: Remove v1 crate (`synthia-session` v1) entirely; remove `SessionV1CompatShim`
- **Gate**: zero v1 references in `cargo build`; migration script ran cleanly on staging corpus

#### Phase 5b — Permission Refactor (1 month)
- Refactor `PermissionService` to sync `evaluate()` + async `request_approval()` separation
- Add `DeniedWithFeedback` (codex `CorrectedError`)
- Session-scoped "always" propagation (opencode pattern)
- **Gate**: `cargo test --workspace` clean; permission flow E2E passes
- **(F20) Memory refactor moved out of Phase 5** — see Phase 6.
- **Dependency**: Phase 5a complete (v2-only SessionService is prerequisite for Phase 4b plugin event subscription and for Phase 6 Memory consolidation).

#### Phase 6 — EventBus + Memory Refactor (2 months)  *(F14: doc estimate 1.5mo understated by +33%; F20: Memory moved here from Phase 5 to co-locate with EventBus durable storage)*
- Implement `EventBus` with durable/ephemeral classification
- Refactor `AgentEvent` to use `Event` trait
- **Refactor `MemoryService` to 4-tier + cross-tier `consolidate()`** *(moved from Phase 5 per F20 — pairs naturally with EventBus durable storage and removes the need for Memory to be its own migration phase)*
- Migrate TUI/Web/Server to subscribe via `EventBus`
- **Gate**: 3 parallel channels collapsed to 1; Memory consolidate() round-trip test passes
- **Dependency** (F7): Phase 5a complete (v2-only SessionService required). **(F8)** Phase 7a cannot start until Phase 6's `EventBus::publish<E>` accepts typed events end-to-end.

#### Phase 7a — Streaming Refactor (1.5 months)  *(F4: doc estimate 1.5mo for 6 deliverables was unrealistic; split for honest scoping)*
- Add `LlmEvent` union + `StreamFn` trait object (push-based)
- `Usage` with non-overlapping fields (opencode pattern)
- Provider migration: `Provider::complete_with_stream` returns `Stream<LlmEvent>` via `StreamFn`
- **Gate**: streaming parity with opencode; all providers emit typed `LlmEvent`
- **Dependency** (F8): Phase 6's `EventBus::publish<E>` accepts typed events end-to-end.

#### Phase 7b — Transport & Server (2 months)  *(F4)*
- MCP `StreamableHttp` (1mo): HTTP + OAuth (RFC 8705 resource indicators)
- MCP `WebSocket` (0.5mo)
- Server JSON-RPC backpressure (-32001) + dual-loop model (0.5mo)
- **Gate**: MCP transport parity with codex; server passes load test with -32001 backpressure
- **Dependency**: Phase 7a complete (typed `LlmEvent` flows through server-side transports).

#### Phase 8 — Cleanup + Public API Migration (2 months)  *(F16: doc estimate 1mo understated by +100%; F6: public-API breakage scope needs its own subphase)*
- Remove all `#[deprecated]` APIs from prior phases
- Remove `synthia-session` (v1) crate entirely (already done in Phase 5a.4 — verify and re-grep)
- CI matrix maintenance budget (0.5mo) reserved per §11.1 (F5)
- **(F6) Public API Migration subphase** (additional 2 weeks):
  - Generate API diff doc via `cargo public-api` against the last pre-Phase-0 baseline
  - Write `MIGRATION.md` (v1 → v2 / v0 → unified) — exhaustive per breaking change
  - Validate downstream consumer compile: pin a representative downstream workspace (e.g. test plugin + internal consumer) against the new public API and run `cargo check --all-features`
- Documentation, examples, migration guide
- **Gate**: zero deprecation warnings; `cargo public-api` diff = 0 unexpected breaks vs migration guide; downstream consumer test green; full type-driven discovery

### 11.3 Risk Assessment

#### Top risks (with mitigation)

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Service resolution overhead in hot loop | Medium | Medium (perf) | Cache resolved Arc on first call; lazy init |
| `dyn Service` trait object bloat | Low | Low | Use `Arc<dyn Service>` consistently |
| Materialization overhead per LLM step | Medium | Low (ms) | Reuse materialization per turn; refresh on provider reload |
| Plugin hot-unload race conditions | Medium | High (correctness) | KeyedMutex + per-plugin CancellationToken |
| Hook consolidation breaks existing plugin authors | Medium | High (compat) | `HookRunner` adapter bridges to new `HookHandler`; deprecation period 1 release |
| Session v1 removal breaks 3rd-party code | Low | High (compat) | Provide migration script + adapter shim |
| `EventBus` pubsub capacity overflow | Medium | Medium | Replace broadcast with watch + per-type mpsc; backpressure |
| Memory consolidation race conditions | Low | Medium | All operations under `Arc<RwLock<HotStore>>`; lock-free cold reads |
| Streaming push-stream complexity | High | Medium | Keep `complete_with_stream` callback variant alongside `StreamFn` |
| Plugin security (native code execution) | High | Critical (security) | Plugin loaded as trusted Rust crate; WASM support deferred to future |
| **Phase N+1 design invalidated by Phase N feedback** (F25) | High | High (schedule) | Reserve a 0.5mo contingency buffer between phases; treat Phase 2b → 3, Phase 4b → 5a, Phase 6 → 7a as "re-plan checkpoints" where Phase N+1's design may be re-opened before coding starts. Worst-case replan adds 1mo to a single phase; this is why §11.5 totals carry ~20% slack. |

### 11.4 Rollback Strategy

Each phase is independently **reversible** (F12; see §11.1):
- **Phase 0**: trait-only additions; trivially reversible
- **Phases 1-2-3** (F11): opt-in via feature flags for **humans** (`unstable-registry`, `unstable-service`, `unstable-hook`), but the CI matrix runs **both default-off and `--features=…unstable-*`** from day 1 — no flag-on-later catch-22 where we discover the new path is broken only when it's time to flip the default. Default-off in non-experimental release builds.
- **Phase 4**: plugin adapter layer preserves old `HookRunner` API for 1 release
- **Phase 5a**: v1 session kept as `#[deprecated]` for 1 release; v2 has shim; Phase 5a.4 is the explicit removal commit
- **Phase 5b / 6 / 7a / 7b / 8**: opt-in via feature flags; legacy channels preserved

### 11.5 Timeline Reality Check

The original `12-15 month` estimate under-counted several multi-month work items (F1, F2, F3, F4, F6, F14, F15, F16, F20). The split + re-scoped phase table:

| Phase | Doc Estimate | Realistic Estimate | Variance | Notes |
|-------|--------------|--------------------|----------|-------|
| Phase 0 | 1mo | 1mo | 0% | Trait-only — fine as written |
| Phase 1 | 1mo | 2mo | +100% | F15 |
| Phase 2a | (NEW) | 1.5mo | split | F1: foundation + SessionService template |
| Phase 2b | (NEW) | 3mo | split | F1: 11 remaining services wrapped in parallel |
| Phase 3 | 1.5mo | 2.5mo | +67% | F14 + F9 transition discipline |
| Phase 4a | (NEW) | 1mo | split | F3: extension crate skeleton + 1 sample plugin |
| Phase 4b | (NEW) | 3mo | split | F3: HookRunner + 43 ports; F21: 5 services moved here from §7.7 |
| Phase 5a | 1mo (was "Phase 5") | 2mo | +100% | F2: shim + script + codemod + removal |
| Phase 5b | (NEW) | 1mo | split | Permission only (Memory moved to Phase 6 per F20) |
| Phase 6 | 1.5mo | 2mo | +33% | F14 + Memory refactor added (F20) |
| Phase 7a | (NEW) | 1.5mo | split | F4: streaming only |
| Phase 7b | (NEW) | 2mo | split | F4: MCP transports + server backpressure |
| Phase 8 | 1mo | 2mo | +100% | F16 + F6 public-API migration subphase |
| **Total** | **12-15mo** | **24-30mo** | **+100%** | ~20% contingency already baked in (F25) |

### 11.6 Recommended Re-ordering + Workflow Notes

**Re-ordering applied in §11.2 above** (see per-phase annotations):
1. Memory refactor moved from Phase 5 → Phase 6 (F20) — pairs with EventBus durable storage
2. Phase 4 split into 4a (skeleton) + 4b (port 43 points); 4b now after Phase 5a so plugins can subscribe to v2 SessionService events
3. Phase 7 split into 7a (streaming) + 7b (transports); 7a gated on Phase 6 typed-event `EventBus`
4. Phase 2 split into 2a (foundation + SessionService template) + 2b (parallel wrapping of 11 services)

**OpenSpec workflow overhead** (F18): Each phase is shipped as an OpenSpec change (`openspec-propose` → `openspec-apply-change` → `openspec-archive-change`). Budget **0.5 day per phase** for spec authoring/review — across 10 numbered phases + sub-phases (0, 1, 2a, 2b, 3, 4a, 4b, 5a, 5b, 6, 7a, 7b, 8), this adds ~5 working days of coordination overhead spread over the 24-30 month timeline.

**Team size assumption** (F19): **3-4 contributors**, organized into parallel work streams:
- **Stream A** (Registry + Services): owns Phases 0, 1, 2a, 2b, 4a, 4b
- **Stream B** (Loop + Session + Memory): owns Phases 3, 5a, 5b, 6
- **Stream C** (Transport + Streaming): owns Phases 7a, 7b
- **Stream D** (shared): Phase 8 polish, public-API migration, CI maintenance

Single-contributor phases (Phase 0, Phase 8 partial) are still safe because they are well-bounded.

---

## 12. Success Metrics

| Metric | Current | Target | Validation |
|--------|---------|--------|------------|
| **Crates** | 30 | 25 (4 layers) | `ls crates/` |
| **Tool abstractions** | 3 parallel | 1 unified | grep for trait names |
| **Hook systems** | 2 parallel | 1 unified | grep `HookRunner` vs `HookService` |
| **Event channels** | 3 parallel | 1 unified | grep `mpsc::UnboundedSender` |
| **Plugin extension points** | 43 (untyped) | typed `HookHandler` | grep `extension_points` |
| **Discarded `AgentRunConfig` fields** | 11 | 0 | grep `_xxx:` in main_loop.rs |
| **Wired hooks** | 2 of 7 | 14 of 14 | grep `fire_` callsites |
| **Provider streaming type** | callback | push-stream (`StreamFn`) | `complete_with_stream` signature |
| **MCP transports** | stdio only | stdio + http + ws | `McpTransportConfig` enum variants |
| **Server backpressure** | none | -32001 | grep `BACKPRESSURE_CODE` |
| **LlmEvent types** | callback variants | 16-variant union | `LlmEvent` enum |
| **New capability add** (e.g. custom provider) | requires code change | plugin manifest | manifest example + e2e test |

---

## 13. Open Questions & Decisions Log

### Open Questions (deferred to writing-plans skill)

1. **Should `Tool` trait split into 3 sub-traits (`DescriptorProvider`, `Invocable`, `EventEmitter`)** for granularity, or stay unified?
2. **Should `Materialization` be mandatory** (every LLM call snapshots) or opt-in (faster but unsafe)?
3. **Should `synthia-service` be a single crate, or split into `synthia-service-trait` + `synthia-service-registry`**?
4. **Is the `Layer 4 = Tool` boundary right, or should `Frontend` be Layer 5**?
5. **Should `ServiceRegistry` be globally shared (process-wide singleton) or per-session (RAII scoped)**? — **security discussion must include F19**: a global registry means a compromised plugin that mutates `ServiceRegistry` affects every session in the process; per-session RAII scoping isolates blast radius. Defer to Phase 4a design spike.
6. **Should service init ordering be auto-detected from `dependencies()` or explicit `init_order` field**?
7. **Should v1 session code be removed entirely in Phase 5, or kept as `#[deprecated]` for one release**? — **RESOLVED (F2)**: kept as `#[deprecated]` with `SessionV1CompatShim` for the full Phase 5a window; explicit removal at Phase 5a.4.
8. **Should `PermissionService::evaluate` return `Result` or be infallible**?
9. **Should `HookService` ship with 10 events from day 1, or phase in**?
10. **Should plugin code be WASM (sandboxed) or native Rust crate (full power)**? — **RESOLVED**: **deferred to Phase 4 with explicit security requirement**. Phase 4 ships native Rust crates (trusted) with `clippy.toml` + `cargo-deny` policy enforcement. WASM sandboxing is filed as a Phase 9 follow-up that reuses the same `Plugin` trait boundary; we will not block Phase 4 on a WASM runtime choice.
11. **Should plugins be discoverable via `.dylib` load, or only via static registry at compile time**?
12. **Should `EventBus::durable_log` be optional (default JSONL) or mandatory (forced storage)**?
13. **Should `LlmEvent` use `serde_json::Value` for `partial`, or a typed `LlmResponse` struct**?
14. **Should the migration be one big `unified-architecture` change, or 8 phased changes**? — **RESOLVED**: **8 phased changes** (now 10 numbered work units: 0, 1, 2a, 2b, 3, 4a, 4b, 5a, 5b, 6, 7a, 7b, 8). Each ships as its own OpenSpec change (F18).
15. **Should we ship a `synthia-2.0` version bump at Phase 8, or continuous minor bumps**?

### Decisions Log

| Decision | Rationale | Source |
|----------|-----------|--------|
| 4-layer architecture (core/loop/service/tool) | User chose; matches opencode-ish abstraction depth | User Q2 |
| Dynamic trait-object registry | User chose; max extensibility | User Q3 |
| Full rewrite (Approach C) | User chose; deepest transformation | User Q4 |
| Preserve main_loop.rs logic | User requirement: "保留主逻辑 react loop 和 session" | User request |
| Layer 4 = Tool + Frontend combined | Avoid too-thin layers | Design tradeoff |
| LIFO stack-based ToolRegistry | opencode借鉴; supports override + scope finalizer | opencode analysis |
| Materialization for stale detection | opencode借鉴; solves LLM/plugin race | opencode analysis |
| Service = system capability, Tool = LLM capability | v3 4-condition rule; high-freq internal stays service | v3 multi-expert |
| SessionService uses v2 only | v1 deprecated; reduce surface area | Synthesis |
| 14 hook events | codex借鉴 10 + synthia 7 = 14 (deduplicated) | codex + synthia existing |
| FailedContinue/FailedAbort 3-state | codex借鉴; replaces panic-isolation | codex analysis |
| Usage with non-overlapping fields | opencode借鉴; never subtract downstream | opencode analysis |
| StreamFn trait object on Agent | pi-mono借鉴; user-swappable provider | pi-mono analysis |
| MCP streamable-http + WebSocket | codex借鉴; covers modern MCP servers | codex analysis |
| Server dual-loop + -32001 backpressure | codex借鉴; high-load reliability | codex analysis |
| 8-phase migration over 12-15 months | Independent shippability + low risk | Synthesis |
| Feature flags for opt-in migration | Backward compat during transition | Industry best practice |
| **10 phased work units over 24-30 months** (F1, F2, F3, F4, F6, F14, F15, F16, F20) | Doc estimates were +67-100% too low; sub-phases now reflect realistic scope; ~20% contingency baked in for F25 replan checkpoints | Review synthesis |
| **CI runs `unstable-*` flags ON from day 1 in Phases 1-2-3** (F11) | Avoid flag-on-later catch-22 where the new path is broken but invisible until default-flip time | Review synthesis |
| **OpenSpec workflow per phase** (F18) | Each numbered phase ships as `openspec-propose` → `apply-change` → `archive-change`; ~0.5 day/phase coordination overhead | Review synthesis |
| **3-4 contributor parallel streams** (F19) | Streams A (Registry+Services) / B (Loop+Session+Memory) / C (Transport+Streaming) / D (Phase 8 polish) | Review synthesis |
| **Memory refactor moved from Phase 5 → Phase 6** (F20) | Memory 4-tier consolidation pairs naturally with EventBus durable storage; one fewer migration phase | Review synthesis |
| **Phase 4b absorbs 5 services from §7.7** (F21) | These are plugin-shaped, not loop-internal; consolidating under Phase 4b avoids a separate Phase 7.7 mini-phase | Review synthesis |
| **"Each phase is independently reversible"** (F12), not "independently shippable" | Real dependencies between phases (F7, F8) — honest framing | Review synthesis |
| **WASM plugin support deferred to Phase 9** (Q10 resolution) | Don't block Phase 4 on WASM runtime choice; Phase 9 reuses `Plugin` trait boundary | Review synthesis |

---

## Appendix A: Source Code References

### Synthia (current state)

| Subsystem | Key Files |
|-----------|-----------|
| ReAct loop | `crates/synthia-agent/src/agent.rs`, `src/turn.rs`, `src/loop_context.rs`, `src/stream_builder/builder/run/main_loop.rs` (1037 lines) |
| Tool system | `crates/synthia-tool/src/traits.rs`, `src/scoped_registry.rs`, `crates/synthia-tool-orchestrator/src/lib.rs`, `crates/synthia-agent/src/tools/dynamic_provider/extension_manager.rs` |
| Provider | `crates/synthia-provider/src/traits.rs`, `src/registry/v2.rs`, `src/anthropic/`, `src/openai/`, `src/router/model_router/` |
| Session | `crates/synthia-session/src/lib.rs`, `crates/synthia-session-v2/src/lib.rs`, `src/part.rs`, `src/writer_task.rs` |
| Permission/Security | `crates/synthia-permission/src/permission_future.rs`, `crates/synthia-guardian/src/lib.rs`, `src/doom_loop_detector.rs`, `src/circuit_breaker.rs`, `crates/synthia-sandbox/src/lib.rs` |
| Memory | `crates/synthia-memory/src/lib.rs`, `src/service.rs`, `src/types.rs`, `src/hot/`, `src/cold/`, `src/episodic/` |
| Hooks | `crates/synthia-hook/src/traits.rs`, `crates/synthia-plugin/src/hook_runner/`, `crates/synthia-agent/src/tools/dynamic_provider/extension_points/` |
| Control | `crates/synthia-agent/src/control/`, `src/steering.rs`, `src/checkpoint/`, `src/replay.rs` |
| MCP | `crates/synthia-mcp/src/lib.rs`, `src/manager/`, `src/mcp_tool.rs`, `src/tool_adapter.rs` |
| Skills/Commands | `crates/synthia-skill/src/lib.rs`, `crates/synthia-command/src/lib.rs`, `crates/synthia-task/src/lib.rs` |
| CLI/Server | `crates/synthia-cli/src/main.rs`, `crates/synthia-server/src/main.rs` |
| Telemetry | `crates/synthia-telemetry/src/lib.rs`, `src/tracer.rs`, `src/span/attributes_processor.rs` |

### Opencode (reference patterns)

| Pattern | Source |
|---------|--------|
| LIFO ToolRegistry + Materialization | `packages/opencode/src/tool/registry.ts:47-119`, `tool/tool.ts:55-107` |
| LlmEvent union + StreamFn | `packages/llm/src/schema/events.ts:209-226`, `packages/llm/src/route/client.ts:36-165` |
| Plugin child-scope + hot-unload | `packages/core/src/plugin.ts:110-181` |
| Tool double-output (structured + content) | `packages/opencode/src/tool/tool.ts:44-107`, `packages/llm/src/schema/messages.ts:95-124` |
| Event durable/ephemeral + versioned | `packages/opencode/src/event/event.ts:81-83, 385-407` |
| Doom-loop detector at tool-call event | `packages/opencode/src/session/processor.ts:522-547` |
| Coalesced wake/run coordinator | `packages/core/src/session/run-coordinator.ts:53-56` |
| AST-based shell permission | `packages/opencode/src/tool/shell.ts:91-117` |
| Project instruction loading | `packages/opencode/src/session/instruction.ts` |

### Codex (reference patterns)

| Pattern | Source |
|---------|--------|
| 4-layer workspace + 130+ crates | `codex-rs/Cargo.toml`, `codex-rs/config.md` |
| SQ/EQ pattern (Submission queue, Event queue) | `codex-rs/protocol/src/protocol.rs` |
| `Option<Option<T>>` tri-state setters | `codex-rs/core/src/codex_thread.rs` |
| 10 hook events + FailedContinue/Abort | `codex-rs/hooks/src/lib.rs:19-30`, `hooks/src/types.rs:14-30` |
| ToolPluginProvenance | `codex-rs/codex-mcp/src/lib.rs:24`, `codex-rs/core/src/tools/router.rs:39-45` |
| ToolRouter + 5 payload types | `codex-rs/core/src/tools/router.rs` |
| `apply_patch` standalone crate | `codex-rs/apply-patch/src/lib.rs` |
| GoalService (thread-scoped objective + token budget) | `codex-rs/ext/goal/src/api.rs:75-200` |
| TaskKind + 4 task types | `codex-rs/core/src/tasks/mod.rs:1-65` |
| AgentRole = ConfigLayer + sticky fields | `codex-rs/core/src/agent/role.rs:130-200` |
| CodeMode (V8 JS runtime) | `codex-rs/code-mode/src/service.rs:99-220` |
| App-Server backpressure -32001 + dual loop | `codex-rs/app-server/README.md:49-87`, `app-server/src/lib.rs:139-200` |
| 8-layer config loader | `codex-rs/config/src/loader/README.md` |
| RolloutRecorder actor pattern | `codex-rs/rollout/src/recorder.rs` |
| SandboxManager transform | `codex-rs/sandboxing/src/manager.rs` |
| ApplyPatchAction + MaybeApplyPatchVerified | `codex-rs/apply-patch/src/lib.rs` |

### pi-mono (reference patterns)

| Pattern | Source |
|---------|--------|
| Three-layer split (ai → agent → coding-agent) | `packages/ai/src/`, `packages/agent/src/`, `packages/coding-agent/src/` |
| StreamFn trait on Agent | `packages/agent/src/types.ts:24-26`, `packages/agent/src/agent.ts:158-207` |
| AgentEvent universal contract | `packages/agent/src/types.ts:350-365` |
| Push-based AssistantMessageEventStream | `packages/ai/src/utils/event-stream.ts` |
| JSONL tree of SessionEntry | `packages/coding-agent/src/core/session-manager.ts:138-147` |
| AgentTool as plain interface | `packages/agent/src/types.ts:308-331` |
| PendingMessageQueue for steering/follow-up | `packages/agent/src/agent.ts:113-144` |
| Extension as `(pi: ExtensionAPI) => void` | `packages/coding-agent/src/core/extensions/types.ts:1084-1310` |
| ExtensionContext factory pattern | `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts:6-19` |
| TUI 3-method Component | `packages/tui/src/tui.ts:17-41` |
| Two-map tool registry (AgentTool + ToolDefinition) | `packages/coding-agent/src/core/agent-session.ts:300-303` |

### Synthia existing analyses (input to this design)

| File | Contribution |
|------|-------------|
| `openspec/changes/_inbox/synthia-current-architecture.md` | 12-subsystem baseline (441 lines) |
| `openspec/changes/_inbox/synthia-critical-review.md` | G1-G20 gap analysis + 4-condition toolification rule (233 lines) |
| `openspec/changes/_inbox/v3-tool-centric-multi-expert-analysis.md` | 4-expert synthesis + Rust trait drafts (751 lines) |
| `openspec/changes/_inbox/opencode-deep-analysis.md` | opencode design patterns (1583 lines) |
| `openspec/changes/_inbox/opencode-control-plane-patterns.md` | 8 opencode control-plane patterns (292 lines) |
| `openspec/changes/_inbox/codex-deep-analysis.md` | codex 12-subsystem analysis (1359 lines) |
| `openspec/changes/_inbox/codex-vs-opencode-design.md` | codex独有 vs opencode借鉴 (458 lines) |
| `openspec/changes/archive/2026-07-02-borrow-best-from-production-agents/proposal.md` | 5-phase borrow plan (150 lines) |
| `openspec/changes/archive/2026-07-14-production-grade-agent-architecture/proposal.md` | 5-capability production plan (77 lines) |

---

## Appendix B: Decision Rationale

### Why 4 layers (not 3 or 5)?

- **3 layers (core/agent/tool)** lacks the service boundary, forcing all system capabilities into either `core` or `tool`. Doesn't scale.
- **5 layers** (capability/registry/tool/extension/frontend) is over-decomposed for synthia's current scale (~30 crates → ~25). Increases ceremony without commensurate benefit.
- **4 layers** strikes the balance: service layer cleanly separates system-internal from LLM-facing capabilities, enables registry patterns at the right granularity.

### Why Dynamic trait-object (not static enum)?

- **Static enum** (e.g. `enum Tool { Read, Write, Bash, ... }`) requires compile-time exhaustive matching; adding a new tool = code change + rebuild.
- **Dynamic trait-object** (`Arc<dyn ToolProvider>`) enables:
  - Plugin hot-reload (manifest change → registry update, no recompile)
  - User extensions without forking synthia
  - Per-session scoped registries (e.g. session-private tools)
  - A/B testing of tool implementations
- Trade-off: trait-object dispatch has slight overhead (1-2 ns/call); acceptable for the flexibility gained.

### Why preserve main_loop.rs?

- User explicit requirement: "保留主逻辑 react loop 和 session"
- The 1037 lines contain battle-tested error recovery, doom-loop detection, L1-L5 cascade, TurnTransition defect channel — high cost to re-validate.
- Refactoring only the DI surface (11 fields → service resolution) keeps the loop logic intact while fixing the systemic gap.

### Why ServiceRegistry type-safe accessors (`get<T>()`)?

- String-based lookup (`registry.get("session")`) is error-prone, especially under refactoring.
- Type-based lookup (`registry.get::<dyn SessionService>()`) catches typos at compile time.
- The `static_name()` method requires every Service impl to declare its name; compile-time check that names are unique within a registry.

### Why Plugin = bundle of registries?

- A plugin that contributes tools alone is incomplete (no hooks = no lifecycle, no services = no state).
- A single manifest declaring `tools + services + hooks + mcp_servers` is atomic: either all succeed or all fail.
- The `PluginCapabilities` declarative struct mirrors opencode's `PluginInput` shape — proven design.

### Why single 8-phase migration (not one big bang)?

- **Risk reduction**: each phase is independently shippable + reversible
- **Feedback incorporation**: each phase can learn from prior phase's findings
- **User-visible value**: Phase 1 (Registry Skeletons) alone provides value (registry invariants testable)
- **Backward compat**: feature flags allow opt-in migration
- **Burn-down**: 11 baseline-discarded fields restored in Phase 2 — concrete G1 fix

---

*This design is the output of brainstorming with the user (2026-07-18). It is the foundation for the writing-plans skill to produce an implementation plan.*

---

## Change #1 Implementation Status (2026-07-19)

> Status of architecture capabilities from this design that were implemented in
> `openspec/changes/2026-07-18-synthia-top5-borrow-integration`.

### Capability Implementation Status

| Capability | PRs | Status |
|------------|-----|--------|
| §10 EventV2 dual-table + Projector | PR-1.1~1.5 | ✅ Complete — `EventBus` trait + `InMemoryEventBus` ring + `EventEnvelope<T>` + `Projector` + `CommitGuard` + `aggregate_events` + `EventBusBridge` + `CleanupTask` |
| §9 Extension system (19 events + sandbox + registry) | PR-2.1~2.4 | ✅ Complete — `Extension` trait + 19 payloads + `Sandbox` + `ExtensionRegistry` |
| §6 Service registry completion | PR-3.1~3.4 | ✅ Complete — `OutputBoundService` + `Capability<T>` + `ReverseDepGraph` + `PeerSourceIndex` |
| §7 GoalService (Semaphore + Weak + OCC) | PR-3.5~3.7 | ✅ Complete — `GoalService` trait + `CodeGoalService` + OCC Keep/Set |
| §9 Hook system unification | PR-4.1~4.3 | ✅ Complete — `HookOutcome` 3-state + unified `Hook` trait + `LoopDetector` |
| §5 Tool materialization identity | PR-5.1~5.4 | ✅ Complete — `ToolId` + `ProviderId` + `ToolVisibility` + `Materialization` + `ToolProvenance` + `ScopeRef::fork` + `OutputBound` |
| §5 Tool output sanitizer | PR-6.1~6.2 | ✅ Complete — `OutputBound` trait + `DefaultOutputBound` + `CleanupTask` |
| §9 Custom event renderer | PR-7.1~7.3 | ✅ Complete — `AgentEvent::Custom` + `EventRendererRegistry` + `project_custom_event` |

### Deferred to Change #2-#4

| Item | Owner | Reason |
|------|-------|--------|
| `HookOutcome::ForwardToMainAgent` consumption in main_loop | change #2 | Needs main_loop refactor |
| `PendingMessageQueue` / `QueueMode` | change #2 | Loop-layer concern |
| `RunCoordinator` coalesced wake | change #2 | Loop-layer concern |
| `ToolCapabilities` per-tool struct | change #3 | Tool business concern |
| WASM sandbox for plugins | change #3 | Deferred in design D4 |
| `CapabilityBroker` migration | change #3 | Tool business concern |
| gRPC bridge streaming | change #4 | Server/transport concern |
| MCP http+ws+OAuth transports | change #4 | Server/transport concern |