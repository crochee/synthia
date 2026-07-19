# Tasks — Synthia 仓库级架构重设 change #1 (架构基础设施)

> **Scope**: change #1 only (8 capability × 25 PR + 验收 + docs 更新)
> **Out of scope**: change #2 (loop) / #3 (tool business) / #4 (server CLI)
> **Format**: per-PR atomic task units (each PR < 500 LOC, independent review, revert safe)

---

## 0. Pre-flight (0 PRs, 工具基线) ✅

### Task 0.1: cargo baseline check ✅

- **WHERE**: repo root
- **HOW**: `cargo +nightly fmt --all && cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`
- **WHY**: make sure master HEAD `2f0a9ad` is green before any change starts
- **EXPECTED**: exit code 0, no warnings

---

## 1. event-v2-system (PR-1.1 ~ PR-1.5)

### Task 1.1: PR-1.1 — create `synthia-event-v2` crate skeleton ✅

- **WHERE**: `crates/synthia-event-v2/`
- **HOW**: scaffold new crate with `EventBus` trait + `EventSink` enum (InMemory | Sqlite) + Cargo.toml (`tokio`, `serde_json`, `async-trait`)
- **WHY**: PR-1.1 introduces the trait surface all later EventV2 PRs build on
- **EXPECTED**: `cargo check -p synthia-event-v2` exit code 0

### Task 1.2: PR-1.2 — EventEnvelope<T> + EventVersion + EventMeta ✅

- **WHERE**: `crates/synthia-event-v2/src/event.rs`
- **HOW**: add 3 struct definitions reusing existing `synthia-prefix-tracker` rolling hash (Synthia 保留)
- **WHY**: envelope is the wire format between emit and project
- **EXPECTED**: serde roundtrip test passes (deterministic sequence)

### Task 1.3: PR-1.3 — InMemory sink impl (default) ✅

- **WHERE**: `crates/synthia-event-v2/src/sink/in_memory.rs`
- **HOW**: bounded ring buffer (1024) + Drop cleanup; no external deps
- **WHY**: default impl ships without `rusqlite`
- **EXPECTED**: stress test 10k emit/consume exits clean

### Task 1.4: PR-1.4 — Sqlite sink impl (feature-gated) ✅

- **WHERE**: `crates/synthia-event-v2/src/sink/sqlite.rs`
- **HOW**: dual table (`events`, `projections`) + sqlx-migrations or `rusqlite` raw schema; gated by `event-v2,sqlite` features
- **WHY**: durable projection requires persistence (matches opencode `event.ts:680`)
- **EXPECTED**: dual-table CRUD test + process-restart recovery test pass

### Task 1.5: PR-1.5 — Projector + CommitGuard + aggregate_events facade ✅

- **WHERE**: `crates/synthia-event-v2/src/{projector,commit_guard,aggregate}.rs`
- **HOW**: `Projector` trait + `CommitGuard::validate` + `aggregate_events::<T>()` public facade; integrate gRPC message-proxy bridge to forward envelopes
- **WHY**: change #1 finalizes EventV2 surface; bridges TURN_* three-state to gRPC stream (Synthia 保留)
- **EXPECTED**: end-to-end test (emit → SQLite persist → restart → aggregate_events replays) passes

---

## 2. extension-system (PR-2.1 ~ PR-2.4)

### Task 2.1: PR-2.1 — create `synthia-extension-v2` skeleton ✅

- **WHERE**: `crates/synthia-extension-v2/`
- **HOW**: scaffold `Extension` trait + `ExtensionManifest` struct (replaces stub `crates/synthia-extension/src/lib.rs` in parallel)
- **WHY**: 19 typed events + sandbox need a real crate
- **EXPECTED**: `cargo check -p synthia-extension-v2` exit code 0

### Task 2.2: PR-2.2 — 19 typed event payloads ✅

- **WHERE**: `crates/synthia-extension-v2/src/events.rs`
- **HOW**: enum + 19 typed payload structs (no `serde_json::Value` in opt-in events)
- **WHY**: events listed in extension-system spec.md
- **EXPECTED**: exhaustive match compile passes for all 19

### Task 2.3: PR-2.3 — typed capability-scoped sandbox ✅

- **WHERE**: `crates/synthia-extension-v2/src/sandbox.rs`
- **HOW**: capability table per extension manifest + executor that rejects missing capabilities before invoking callback; integrated `HookOutcome::Deny` (see hook-system-unification)
- **WHY**: design-review B4 partial fix (typed Rust trait sandbox; WASM deferred to change #3)
- **EXPECTED**: rejection unit test + prometheus counter test pass

### Task 2.4: PR-2.4 — ExtensionRegistry + double-registration ✅

- **WHERE**: `crates/synthia-extension-v2/src/registry.rs`
- **HOW**: `ExtensionRegistry::register` / `deregister` with atomic `ServiceRegistry` registration; reject duplicate ids
- **WHY**: enables bidirectional find (extension consumers <-> service consumers)
- **EXPECTED**: double-register integration test passes; existing `synthia-extension` 1-line stub still compiles with `#[deprecated]`

---

## 3. service-registry-completion (PR-3.1 ~ PR-3.4)

### Task 3.1: PR-3.1 — OutputBound::Service trait + bound_service method ✅

- **WHERE**: `crates/synthia-service/src/output_bound.rs`
- **HOW**: define `OutputBoundService` trait + 5 new methods on `ServiceRegistry::bound_service::<T>()` (replace 1st TODO at `registry.rs:142`)
- **WHY**: design-review H9 partial fix (cut reverse dep cycle)
- **EXPECTED**: type-system test (no runtime panic for non-Send types) passes

### Task 3.2: PR-3.2 — typed Capability<T> contract ✅

- **WHERE**: `crates/synthia-service/src/capability.rs`
- **HOW**: `Capability<T>` marker + `register_with_capability` + `capabilities_provided::<T>()` query (replace 2nd TODO at `registry.rs:158`)
- **WHY**: explicit capability makes audits easier
- **EXPECTED**: contract-mismatch integration test passes

### Task 3.3: PR-3.3 — reverse-dependency tracking (no broker) ✅

- **WHERE**: `crates/synthia-service/src/reverse_dep.rs`
- **HOW**: `DashMap<ServiceId, BTreeSet<ServiceId>>` for edges; cycle detection on bind (replace 3rd TODO at `registry.rs:201`); `reverse_dependents_of` exposed to tooling only (NOT introduced into ToolContext — change #3 deferred)
- **WHY**: H9 closure at the introspection level; runtime broker pushed
- **EXPECTED**: cycle-detection unit test passes

### Task 3.4: PR-3.4 — peer-source (CapsuleId/StreamId) registration ✅

- **WHERE**: `crates/synthia-service/src/peer_source.rs`
- **HOW**: `Source::{Capsule, Stream}` enum + `register_with_source` + `get_by_capsule::<T>(id)` (replace 4th TODO at `registry.rs:267`)
- **WHY**: completeness — fully resolve `synthia-service::registry` TODOs
- **EXPECTED**: source-not-found integration test passes; existing 286-line registry test suite unchanged

---

## 4. goal-service-runtime (PR-3.5 ~ PR-3.7)

### Task 4.1: PR-3.5 — create `synthia-goal-service` skeleton ✅

- **WHERE**: `crates/synthia-goal-service/`
- **HOW**: scaffold `GoalService` trait + `TaskGoal` 7-state struct
- **WHY**: independent crate enforces `synthia-agent` → `synthia-goal-service` single-direction
- **EXPECTED**: trait object safety compile test passes

### Task 4.2: PR-3.6 — CodeGoalService via Arc<Semaphore> + Weak runtime ✅

- **WHERE**: `crates/synthia-goal-service/src/code.rs`
- **HOW**: `Arc<tokio::sync::Semaphore>` admission + `Weak<tokio::runtime::Handle>` runtime; default permits = num_cpus * 2
- **WHY**: matches codex GoalService model
- **EXPECTED**: admission + runtime-drop unit tests pass

### Task 4.3: PR-3.7 — Keep/Set OCC retry + eviction ✅

- **WHERE**: `crates/synthia-goal-service/src/occ.rs`
- **HOW**: `Keep` (writer) + `Set` (state setter) with version check + retry up to 3; `GoalError::MaxRetriesExceeded` after 3rd failure
- **WHY**: optimistic concurrency control for high-concurrency goals
- **EXPECTED**: OCC retry integration test passes (10ms conflict window)

---

## 5. hook-system-unification (PR-4.1 ~ PR-4.3)

### Task 5.1: PR-4.1 — HookOutcome 3-state + 10 events ✅

- **WHERE**: `crates/synthia-hook/src/outcome.rs` + `events.rs`
- **HOW**: `HookOutcome { Allow | Deny { reason } | ForwardToMainAgent { hint } }` + 10 typed events (含 Synthia 独有 `PreMessageDrop`)
- **WHY**: unify `synthia-agent::Hook` + `synthia-plugin::HookRunner`
- **EXPECTED**: exhaustive match compile test for 10 events + 3 outcomes

### Task 5.2: PR-4.2 — new unified Hook trait + deprecation marker ✅

- **WHERE**: `crates/synthia-hook/src/trait.rs`
- **HOW**: define `Hook` trait operating on 10 events; mark `synthia-agent::Hook` + `synthia-plugin::HookRunner` with `#[deprecated(note = "...")]`
- **WHY**: 3-month deprecation window per design D3
- **EXPECTED**: adapter pattern wires old → new automatically

### Task 5.3: PR-4.3 — LoopDetector integration ✅

- **WHERE**: `crates/synthia-hook/src/loop_detector.rs`
- **HOW**: integrate `detect_repeat` / `similarity_threshold` / `recovery_action`; emit `HookOutcome::Deny { reason: "loop_detected" }` after 3rd > 90% similar `PostToolUse`
- **WHY**: Synthia 保留 LoopDetector 三件套
- **EXPECTED**: loop-detected integration test passes (3-similar-tool-call scenario)

---

## 6. tool-materialization-identity (PR-5.1 ~ PR-5.4)

### Task 6.1: PR-5.1 — ToolId + ProviderId + ToolVisibility newtypes ✅

- **WHERE**: `crates/synthia-tool-materialization/src/{id,visibility}.rs`
- **HOW**: `ToolId(Uuid)` + `ProviderId(&'static str)` interned via `once_cell` + `ToolVisibility` enum
- **WHY**: identity-bearing types precede Materialization
- **EXPECTED**: serde + Display tests pass; empty-string `const_assert` rejects at compile time

### Task 6.2: PR-5.2 — Materialization struct + identity in scoped_registry.materialize() ✅

- **WHERE**: `crates/synthia-tool/src/scoped_registry.rs` (existing) + `crates/synthia-tool-materialization/src/materialization.rs` (new)
- **HOW**: add `Materialization { id, provider_id, visibility, wholly_disabled, provenance, scope_fork }` returned by `materialize()`; preserve existing LIFO + RAII semantics
- **WHY**: PR-5.2 introduces identity WITHOUT breaking 618-line existing tests
- **EXPECTED**: existing scoped_registry tests pass untouched; new materialization struct test passes

### Task 6.3: PR-5.3 — ToolProvenance enum ✅

- **WHERE**: `crates/synthia-tool-materialization/src/provenance.rs`
- **HOW**: `enum { Builtin, Plugin { extension_id }, Ephemeral { source_id } }` + integration with builtin registration path
- **WHY**: distinguishes origin for audits
- **EXPECTED**: provenance test for each variant passes

### Task 6.4: PR-5.4 — Scope.fork + whollyDisabled filter + tool_id session projection ✅

- **WHERE**: `crates/synthia-tool/src/scope.rs` + `crates/synthia-session/src/op_run.rs`
- **HOW**: `Scope::fork(name) -> Arc<Scope>` with `Weak<Scope>` parent; `ScopedToolRegistry::resolve` skip when latest materialization is `wholly_disabled`; `OpRun` record gains `tool_id: ToolId`
- **EXPECTED**: fork parent-drop test + wholly-disabled filter test + `OpRun::tool_id` roundtrip test pass

---

## 7. tool-output-sanitizer (PR-6.1 ~ PR-6.2)

### Task 7.1: PR-6.1 — OutputBound trait (60 行) ✅

- **WHERE**: `crates/synthia-tool-materialization/src/output_bound.rs`
- **HOW**: `bind(&self, output: Vec<u8>)` + `content_len()` + `cleanup()`; default 50KiB / 2000 lines cap; configurable via `OutputBoundConfig`
- **WHY**: opencode `outputBound.ts` mirror
- **EXPECTED**: truncation + cap-bypass tests pass

### Task 7.2: PR-6.2 — CleanupTask 7d retention + ToolContext::take_output ✅

- **WHERE**: `crates/synthia-event-v2/src/cleanup.rs` + `crates/synthia-tool/src/context.rs`
- **HOW**: `CleanupTask::spawn(interval, retention_days)` runs every 3600s, deletes outputs older than 7d; `ToolContext::take_output -> Option<Vec<u8>>` drains buffer (50 行)
- **WHY**: persistence + bounded buffer work together
- **EXPECTED**: 7d eviction test + `take_output` consumption test pass; `CachePolicyApplier::Arc::ptr_eq` short-circuit preserved (verified via `tool_output_arc_ptr_eq_hit_total` metric)

---

## 8. custom-event-renderer (PR-7.1 ~ PR-7.3)

### Task 8.1: PR-7.1 — AgentEvent::Custom variant ✅

- **WHERE**: `crates/synthia-agent/src/events/event_enum.rs`
- **HOW**: append `Custom { event_type: String, data: serde_json::Value }` (preserving variant order — 28 existing stay in current positions)
- **WHY**: pi-mono `extensions/types.ts` Custom support
- **EXPECTED**: serde roundtrip + exhaustive match compile test pass

### Task 8.2: PR-7.2 — EventRenderer registry + builtin JSON renderer ✅

- **WHERE**: `crates/synthia-extension-v2/src/event_renderer.rs`
- **HOW**: `EventRendererRegistry` keyed by `event_type`; builtin `JsonEventRenderer` as wildcard `*`
- **EXPECTED**: wildcard match test + custom-shadow test pass

### Task 8.3: PR-7.3 — Custom event projection to AgentMessage ✅

- **WHERE**: `crates/synthia-protocol/src/projection.rs`
- **HOW**: project Custom → `EventMsg::CustomEvent` with `rendered` field; fallback to builtin JSON on render failure
- **EXPECTED**: render-failure fallback test + protocol-level roundtrip test pass

---

## 9. Docs + retrospective (post-implementation)

### Task 9.1: docs/unified-registry-architecture-design.md checked items ✅

- **WHERE**: `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md`
- **HOW**: tick boxes for each PR that landed; remove ✗
- **EXPECTED**: 100% items checked or explicitly deferred to change #2-#4

### Task 9.2: docs/design-review.md remediation table ✅

- **WHERE**: `docs/superpowers/specs/2026-07-18-synthia-design-review.md`
- **HOW**: add "Closed by" column with PR link for each finding; leave open findings as change #2-#4 owners
- **EXPECTED**: 35 critical + 40 high findings each annotated

### Task 9.3: retrospective.md ✅

- **WHERE**: `openspec/changes/2026-07-18-synthia-top5-borrow-integration/retrospective.md`
- **HOW**: capture what worked / what didn't / surprises / lessons for change #2-#4
- **EXPECTED**: file exists with ≥ 4 sections

---

## 10. Quality gates (final verification)

### Task 10.1: cargo fmt + clippy ✅

- **HOW**: `cargo +nightly fmt --all && cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`
- **EXPECTED**: exit code 0

### Task 10.2: cargo test split (per Rust project rules — never run all at once) ✅

- **HOW**: per-module, e.g. `cargo test -p synthia-event-v2`, `cargo test -p synthia-extension-v2`, ..., continuing through each new + modified crate, in dependency order; never `cargo test --workspace` in one shot
- **EXPECTED**: every batch green; no pre-existing failures

### Task 10.3: OpenSpec CLI schema validation ✅

- **HOW**: `openspec validate 2026-07-18-synthia-top5-borrow-integration --type change --json`
- **EXPECTED**: exit code 0; no requirement `SHALL`/`MUST` violations; all scenarios at heading level 4 (####)

### Task 10.4: public API stability check ✅

- **HOW**: `cargo check --workspace --all-features` verifies no existing public API breakage; all new types are additions only
- **EXPECTED**: no surface change to existing public APIs; only additions

---

## 11. Total task count

| Group | Count | PRs |
|-------|-------|-----|
| 0. Pre-flight | 1 | 0 |
| 1. event-v2-system | 5 | PR-1.1 ~ 1.5 |
| 2. extension-system | 4 | PR-2.1 ~ 2.4 |
| 3. service-registry-completion | 4 | PR-3.1 ~ 3.4 |
| 4. goal-service-runtime | 3 | PR-3.5 ~ 3.7 |
| 5. hook-system-unification | 3 | PR-4.1 ~ 4.3 |
| 6. tool-materialization-identity | 4 | PR-5.1 ~ 5.4 |
| 7. tool-output-sanitizer | 2 | PR-6.1 ~ 6.2 |
| 8. custom-event-renderer | 3 | PR-7.1 ~ 7.3 |
| 9. Docs + retrospective | 3 | (chore) |
| 10. Quality gates | 4 | (verify) |
| **Total** | **34** | **25 PRs + 9 chore** |
