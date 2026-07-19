# Tasks — synthia-tool-orchestrator-permission (Change #3)

> **Scope**: change #3 — tool/orchestrator/permission business logic
> **Out of scope**: change #1 (done) / change #2 (loop) / change #4 (server)
> **Pre-condition**: change #1 archived, change #2 tasks defined

---

## 0. Pre-flight

### Task 0.1: cargo baseline check

- **HOW**: `cargo +nightly fmt --all && cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`
- **EXPECTED**: exit code 0

---

## 1. tool-capability-integration (PR-1.1 ~ PR-1.2)

### Task 1.1: PR-1.1 — ToolCapabilities in ToolExecutionContext

- **WHERE**: `crates/synthia-tool/src/types.rs` + `crates/synthia-tool/src/unified_adapter.rs`
- **HOW**: add `capabilities: Option<ToolCapabilities>` to `ToolExecutionContext`; `ToolAdapter` populates from `synthia-core::ToolContext` when `unified-registry` feature enabled
- **EXPECTED**: construction test + feature-gate test pass

### Task 1.2: PR-1.2 — CapabilityBroker gate in DefaultToolOrchestrator

- **WHERE**: `crates/synthia-tool-orchestrator/src/lib.rs`
- **HOW**: add `capability_broker: Option<Arc<CapabilityBroker>>` to `DefaultToolOrchestrator`; before execution, check `allowed()` for declared capabilities; deny if not allowed
- **EXPECTED**: capability denied test + capability allowed test pass

---

## 2. category-based-permission (PR-2.1 ~ PR-2.3)

### Task 2.1: PR-2.1 — PermissionChecker category-based security_check

- **WHERE**: `crates/synthia-permission/src/checker/checker.rs`
- **HOW**: `security_check()` takes `Option<ToolCategory>` parameter; if category available, apply category-specific rules; otherwise fall back to name matching
- **EXPECTED**: Shell category test + Filesystem category test + fallback test pass

### Task 2.2: PR-2.2 — PermissionRule category pattern syntax

- **WHERE**: `crates/synthia-permission/src/rule.rs` + `merged_policy.rs`
- **HOW**: `PermissionRule.pattern` supports `category:Shell` prefix; `evaluate()` matches by category when prefix present
- **EXPECTED**: category pattern match test + mixed category+name test pass

### Task 2.3: PR-2.3 — ToolPermission sub-trait deprecation

- **WHERE**: `crates/synthia-tool/src/sub_traits/permission.rs`
- **HOW**: add `#[deprecated(note = "...")]` on `ToolPermission` trait; add migration guide in doc comment
- **EXPECTED**: deprecated trait compiles with warning; existing tests pass

---

## 3. tool-id-audit-trail (PR-3.1 ~ PR-3.2)

### Task 3.1: PR-3.1 — ToolId on ToolCallRequest + ToolCallResult

- **WHERE**: `crates/synthia-tool-orchestrator/src/types.rs`
- **HOW**: add `tool_id: Option<ToolId>` to both structs; add `synthia-tool-materialization` dependency
- **EXPECTED**: construction test + serde roundtrip test pass

### Task 3.2: PR-3.2 — Orchestrator populates ToolId from Materialization

- **WHERE**: `crates/synthia-tool-orchestrator/src/lib.rs`
- **HOW**: after `ToolResolver::resolve()`, if the tool has Materialization, set `request.tool_id`; echo in result; add to `ToolOrchestratorEvent`
- **EXPECTED**: tool_id populated test + event carries tool_id test pass

---

## 4. output-bound-integration (PR-4.1)

### Task 4.1: PR-4.1 — OutputBound::bind() in execute_and_emit Phase 4

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs`
- **HOW**: replace `truncate_output()` call with `OutputBound::bind()` from `LoopServices.output_bound`; add `output_bound: Option<Arc<dyn OutputBound>>` to `LoopServices`
- **EXPECTED**: within-bounds test + byte-cap truncation test + line-cap truncation test pass; existing truncation tests still pass

---

## 5. provenance-capability-permission (PR-5.1 ~ PR-5.2)

### Task 5.1: PR-5.1 — Provenance-based permission floor

- **WHERE**: `crates/synthia-tool-orchestrator/src/lib.rs` + `crates/synthia-permission/src/checker/checker.rs`
- **HOW**: add `fn apply_provenance_floor(provenance: &ToolProvenance, permission: Permission) -> Permission` function; Builtin→AutoApprove, Plugin→RequireConfirm, Ephemeral→RequireExplicit floor; orchestrator calls before approval
- **EXPECTED**: 3 provenance floor tests pass

### Task 5.2: PR-5.2 — Capability-based upgrade within provenance floor

- **WHERE**: `crates/synthia-tool-orchestrator/src/lib.rs`
- **HOW**: after provenance floor, check `CapabilityBroker::allowed()` for tool-declared capabilities; if denied, upgrade to `Deny`; orchestrator computes effective permission before approval phase
- **EXPECTED**: capability denied upgrade test + capability allowed stay test pass

---

## 6. wasm-sandbox-stub (PR-6.1)

### Task 6.1: PR-6.1 — SandboxAttempt::Wasm stub variant

- **WHERE**: `crates/synthia-tool/src/types.rs` (SandboxAttempt enum)
- **HOW**: add `Wasm { runtime: String }` variant; in `ToolAdapter::execute()`, match on Wasm → return error "WASM sandbox not yet implemented"; update all existing match arms
- **EXPECTED**: Wasm variant test + serde test + all existing sandbox tests pass

---

## 7. Quality gates

### Task 7.1: cargo fmt + clippy
### Task 7.2: cargo test split per-module
### Task 7.3: OpenSpec validation

---

## 8. Docs + retrospective

### Task 8.1: retrospective.md
