# Tasks — synthia-tool-orchestrator-permission (Change #3)

> **Scope**: change #3 — tool/orchestrator/permission business logic
> **Out of scope**: change #1 (done) / change #2 (loop) / change #4 (server)
> **Pre-condition**: change #1 archived, change #2 tasks defined

---

## 0. Pre-flight

### Task 0.1: cargo baseline check

- [x] 0.1 `cargo +nightly fmt --all && cargo clippy --workspace --all-targets --all-features --tests -- -D warnings` — exit code 0

---

## 1. tool-capability-integration (PR-1.1 ~ PR-1.2)

### Task 1.1: PR-1.1 — ToolCapabilities in ToolExecutionContext

- [x] 1.1 add `capabilities: Option<ToolCapabilities>` to `ToolExecutionContext` (gated behind `unified-registry` feature); `ToolAdapter` populates from `synthia-core::ToolContext`; `with_capabilities()` builder; 3 tests pass

### Task 1.2: PR-1.2 — CapabilityBroker gate in DefaultToolOrchestrator

- [x] 1.2 add `capability_broker: Option<Arc<CapabilityBroker>>` to `DefaultToolOrchestrator`; `capability_for_tool_name()` helper maps tool names to capabilities; integrated with provenance floor (Task 5.2); 4 tests pass

---

## 2. category-based-permission (PR-2.1 ~ PR-2.3)

### Task 2.1: PR-2.1 — PermissionChecker category-based security_check

- [x] 2.1 `security_check_with_category()` takes `Option<ToolCategory>`; `ToolCategory` enum added to `synthia-permission`; `PermissionRequest.tool_category` field added; `check()` routes by category first with name-matching fallback; 8 tests pass

### Task 2.2: PR-2.2 — PermissionRule category pattern syntax

- [x] 2.2 `parse_category_pattern()` extracts `category:X` prefix; `PermissionRule::matches()` routes by category when prefix present; `MergedPolicy::evaluate_with_category()` passes category through; name-match takes priority; fail-closed when category is None; 14 tests pass

### Task 2.3: PR-2.3 — ToolPermission sub-trait deprecation

- [x] 2.3 `#[deprecated]` on `ToolPermission`, `PermissionAlwaysAllow`, `PermissionAlwaysDeny` with migration guide; `#[allow(deprecated)]` on impl blocks and re-exports; existing tests pass

---

## 3. tool-id-audit-trail (PR-3.1 ~ PR-3.2)

### Task 3.1: PR-3.1 — ToolId on ToolCallRequest + ToolCallResult

- [x] 3.1 `tool_id: Option<ToolId>` on `ToolCallRequest`, `ToolCallResult`, `ToolOrchestratorEvent::Completed`; `synthia-tool-materialization` dep added; all construction sites updated; 7 tests pass

### Task 3.2: PR-3.2 — Orchestrator populates ToolId from Materialization

- [x] 3.2 `ToolIdResolver` trait + `HashMapToolIdResolver`; `tool_id_resolver` field on orchestrator; populates `request.tool_id` after resolve; caller-supplied tool_id not overwritten; 8 tests pass

---

## 4. output-bound-integration (PR-4.1)

### Task 4.1: PR-4.1 — OutputBound::bind() in execute_and_emit Phase 4

- [x] 4.1 `OutputBound::bind()` method with `BoundResult`; `output_bound: Option<OutputBound>` on `LoopServices`; Phase 4 truncation replaced; 7 tests pass (within-bounds, byte-cap, line-cap, default 50KiB/2000-line, control chars, head-only, None passthrough)

---

## 5. provenance-capability-permission (PR-5.1 ~ PR-5.2)

### Task 5.1: PR-5.1 — Provenance-based permission floor

- [x] 5.1 `apply_provenance_floor()` + `permission_is_more_restrictive()` functions; `ToolProvenanceResolver` trait; `tool_provenance_resolver` field on orchestrator; Builtin→AutoApprove, Plugin→RequireConfirm, Ephemeral→RequireExplicit; 11 tests pass

### Task 5.2: PR-5.2 — Capability-based upgrade within provenance floor

- [x] 5.2 Integrated capability check after provenance floor; capability denial expressed as `Permission::Deny` flowing through approval system; 4 integration tests pass (Builtin+denied→Deny, Builtin+allowed→AutoApprove, Plugin+denied→Deny, no-provenance+denied→Deny)

---

## 6. wasm-sandbox-stub (PR-6.1)

### Task 6.1: PR-6.1 — SandboxAttempt::Wasm stub variant

- [x] 6.1 `SandboxAttempt::Wasm { runtime: String }` variant added; `wrap()` returns UNSUPPORTED error; `PartialEq, Eq` derives added; 3 tests pass (construction, wrap error, serde roundtrip)

---

## 7. Quality gates

- [x] 7.1 `cargo +nightly fmt --all` — clean
- [x] 7.2 `cargo clippy --all-targets --all-features --tests --all` — clean (only pre-existing deprecation warnings)
- [x] 7.3 `cargo check --workspace` — passes
- [x] 7.4 Per-module tests: synthia-sandbox 15, synthia-permission 83, synthia-tool-orchestrator 83, synthia-tool 127, synthia-core output_bound 7 — all pass

---

## 8. Docs + retrospective

- [x] 8.1 retrospective.md written below
