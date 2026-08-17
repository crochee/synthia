# Verify — Synthia 仓库级架构重设 change #1 (架构基础设施)

> **Per-change verification report** (per OpenSpec `superpowers-bridge` schema)
> **Status**: PENDING (filled upon apply completion)
> **Scope**: verify 25 PRs + 4 quality gates + 7 preserved Synthia 独有设计

---

## Quality gates (binary, must be all green)

### Gate G1 — Cargo format baseline

- **Command**: `cargo +nightly fmt --all --check`
- **Expected**: exit code 0; no diff
- **Status**: ☐ PENDING

### Gate G2 — Cargo clippy

- **Command**: `cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`
- **Expected**: exit code 0
- **Notes**: per `.trae/rules/rust.md`, ALL warnings as errors; pre-existing warnings in unrelated crates are NOT this change's concern
- **Status**: ☐ PENDING

### Gate G3 — Cargo test (split, per-module — NEVER all-at-once)

> **Constraint (verbatim from `.trae/rules/rust.md`)**: "测试不能一次性执行所有测试用例，必须分批次执行，每个批次执行按模块进行"

- **Batches** (in dependency order):
  1. `cargo test -p synthia-event-v2` + `-p synthia-extension-hook`
  2. `cargo test -p synthia-goal-service` + `-p synthia-tool-materialization`
  3. `cargo test -p synthia-service` + `-p synthia-tool` (modified existing)
  4. `cargo test -p synthia-hook` (modified existing)
  5. `cargo test -p synthia-agent` (modified existing, 28-variant events)
  6. `cargo test -p synthia-session` (modified existing, OpRun::tool_id)
  7. `cargo test -p synthia-protocol` (modified existing, projection)
  8. `cargo test -p synthia-prefix-tracker` (smoke — 7 保留项之一)
  9. `cargo test -p synthia-cache-policy-applier` (smoke — 7 保留项之一)
  10. `cargo test -p synthia-compaction` (smoke — CompactionAnalyticsAttempt)
  11. `cargo test -p synthia-definition-drift` (smoke — DefinitionDrift)
  12. `cargo test -p synthia-grpc-message-proxy` (smoke — TURN_* 三态桥接)
- **Expected**: every batch green; no pre-existing failures introduced
- **Status**: ☐ PENDING

### Gate G4 — OpenSpec CLI schema validation

- **Command**: `openspec validate 2026-07-18-synthia-top5-borrow-integration --schema superpowers-bridge`
- **Expected**: exit code 0; no requirement `SHALL`/`MUST` violations; all scenarios at heading level 4 (`####`); no JSON/YAML parse errors
- **Status**: ☐ PENDING

### Gate G5 — Public API stability

- **Command**: `cargo public-api -p synthia-event-v2 -p synthia-extension-hook -p synthia-goal-service -p synthia-tool-materialization` (diff vs `2f0a9ad`)
- **Expected**:
  - new crates: zero public API change vs empty baseline (only additions)
  - modified crates (`synthia-tool`, `synthia-agent`, `synthia-session`, `synthia-protocol`, `synthia-hook`, `synthia-service`): only ADDITIONS, no breaking removals
- **Status**: ☐ PENDING

### Gate G6 — 7 preserved Synthia 独有设计 smoke tests

Each item below must have a targeted smoke test (existing or new) that passes:

| Preserved item | Smoke test | Status |
|----------------|------------|--------|
| PrefixTracker 三段 hash + rolling stability | `cargo test -p synthia-prefix-tracker` | ☐ PENDING |
| CachePolicyApplier `Arc::ptr_eq` short-circuit | `cargo test -p synthia-cache-policy-applier` + integration in tool-output-sanitizer | ☐ PENDING |
| JSONL 事件流 + TURN_* 三态 + `fail_interrupted_tools` | `cargo test -p synthia-event` + `cargo test -p synthia-grpc-message-proxy` | ☐ PENDING |
| CompactionAnalyticsAttempt trigger 区分 | `cargo test -p synthia-compaction` | ☐ PENDING |
| DefinitionDrift 检测 (subagent governance) | `cargo test -p synthia-definition-drift` | ☐ PENDING |
| gRPC message-proxy 跨进程事件推送 | `cargo test -p synthia-grpc-message-proxy` + PR-1.5 bridge integration | ☐ PENDING |
| LoopDetector 三件套 | `cargo test -p synthia-hook` (PR-4.3 integration) | ☐ PENDING |

---

## Spec → Test traceability matrix

Each ADDED Requirement in each spec.md MUST have ≥ 1 matching test:

| Spec | Requirement | Test file | Status |
|------|-------------|-----------|--------|
| event-v2-system | EventV2 dual-layer bus | `crates/synthia-event-v2/tests/bus_dual_layer.rs` | ☐ PENDING |
| event-v2-system | EventEnvelope with prefix-version metadata | `crates/synthia-event-v2/tests/envelope_prefix.rs` | ☐ PENDING |
| event-v2-system | Projector + CommitGuard facade | `crates/synthia-event-v2/tests/projector_commit_guard.rs` | ☐ PENDING |
| event-v2-system | CleanupTask with 7-day retention | `crates/synthia-event-v2/tests/cleanup_retention.rs` | ☐ PENDING |
| event-v2-system | gRPC message-proxy bridge | `crates/synthia-event-v2/tests/grpc_bridge.rs` | ☐ PENDING |
| extension-system | Extension trait with 19 typed events | `crates/synthia-extension-hook/tests/trait_19_events.rs` | ☐ PENDING |
| extension-system | ExtensionManifest declarative registration | `crates/synthia-extension-hook/tests/manifest_parse.rs` | ☐ PENDING |
| extension-system | typed capability-scoped sandbox | `crates/synthia-extension-hook/tests/sandbox_capability.rs` | ☐ PENDING |
| extension-system | ExtensionRegistry double-registration | `crates/synthia-extension-hook/tests/registry_double.rs` | ☐ PENDING |
| extension-system | backward compat with 1-line stub | `crates/synthia-extension/tests/still_compiles.rs` | ☐ PENDING |
| service-registry-completion | OutputBound::Service trait | `crates/synthia-service/tests/output_bound.rs` | ☐ PENDING |
| service-registry-completion | Capability typed contract | `crates/synthia-service/tests/capability_contract.rs` | ☐ PENDING |
| service-registry-completion | peer-source identification | `crates/synthia-service/tests/peer_source.rs` | ☐ PENDING |
| service-registry-completion | reverse-dependency resolution | `crates/synthia-service/tests/reverse_dep.rs` | ☐ PENDING |
| goal-service-runtime | GoalService trait + CodeGoalService impl | `crates/synthia-goal-service/tests/code_default.rs` | ☐ PENDING |
| goal-service-runtime | semaphore-based admission | `crates/synthia-goal-service/tests/semaphore_admission.rs` | ☐ PENDING |
| goal-service-runtime | Weak runtime + idle eviction | `crates/synthia-goal-service/tests/runtime_drop.rs` | ☐ PENDING |
| goal-service-runtime | Keep/Set OCC retry | `crates/synthia-goal-service/tests/occ_retry.rs` | ☐ PENDING |
| hook-system-unification | HookOutcome 3-state | `crates/synthia-hook/tests/outcome_3state.rs` | ☐ PENDING |
| hook-system-unification | 10 typed hook events | `crates/synthia-hook/tests/events_10.rs` | ☐ PENDING |
| hook-system-unification | LoopDetector integration | `crates/synthia-hook/tests/loop_detector.rs` | ☐ PENDING |
| hook-system-unification | backward compat deprecation window | `crates/synthia-agent/tests/legacy_compiles.rs` | ☐ PENDING |
| tool-materialization-identity | ToolId + ProviderId newtype | `crates/synthia-tool-materialization/tests/id_newtype.rs` | ☐ PENDING |
| tool-materialization-identity | Materialization identity | `crates/synthia-tool-materialization/tests/materialization_identity.rs` | ☐ PENDING |
| tool-materialization-identity | whollyDisabled filter | `crates/synthia-tool-materialization/tests/wholly_disabled.rs` | ☐ PENDING |
| tool-materialization-identity | ToolProvenance enum | `crates/synthia-tool-materialization/tests/provenance.rs` | ☐ PENDING |
| tool-materialization-identity | Scope.fork + tool_id projection | `crates/synthia-tool-materialization/tests/scope_fork.rs` | ☐ PENDING |
| tool-output-sanitizer | OutputBound trait | `crates/synthia-tool-materialization/tests/output_bound.rs` | ☐ PENDING |
| tool-output-sanitizer | 7-day retention CleanupTask | `crates/synthia-event-v2/tests/cleanup_7d.rs` | ☐ PENDING |
| tool-output-sanitizer | ToolContext::take_output | `crates/synthia-tool/tests/take_output.rs` | ☐ PENDING |
| tool-output-sanitizer | CachePolicyApplier Arc::ptr_eq preserved | `crates/synthia-tool-materialization/tests/arc_ptr_eq.rs` | ☐ PENDING |
| custom-event-renderer | AgentEvent::Custom variant | `crates/synthia-agent/tests/event_custom.rs` | ☐ PENDING |
| custom-event-renderer | EventRenderer registry | `crates/synthia-extension-hook/tests/event_renderer.rs` | ☐ PENDING |
| custom-event-renderer | projection to AgentMessage | `crates/synthia-protocol/tests/custom_projection.rs` | ☐ PENDING |

---

## Docs remediation status

| Doc | Annotation target | Method | Status |
|-----|------------------|--------|--------|
| `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` | 1823 行 4 层架构 checked items | task 9.1 | ☐ PENDING |
| `docs/superpowers/specs/2026-07-18-synthia-design-review.md` | 121 findings (35 critical + 40 high + 38 medium + 8 low) | task 9.2 | ☐ PENDING |
| `openspec/changes/_inbox/v3-tool-centric-multi-expert-analysis.md` | Top-5 ROI alignment check | manual review | ☐ PENDING |
| `openspec/changes/_inbox/synthia-critical-review.md` | 11 AgentRunConfig fields + 20 G/N gap annotation | manual review | ☐ PENDING |

---

## Out-of-scope confirmation (must NOT be in change #1)

For verify auditor: the following MUST remain UNTOUCHED by change #1 PRs (otherwise escalate):

- [ ] `crates/synthia-agent/src/main_loop.rs` — unchanged content
- [ ] `crates/synthia-agent/src/turn_state.rs` — no new fields
- [ ] `crates/synthia-tool/src/<business_tools>/*` (per-tool business logic) — unchanged
- [ ] `crates/synthia-permission/*` — unchanged (tree-sitter scope is change #3)
- [ ] `crates/synthia-server/*` — unchanged
- [ ] `crates/synthia-cli/*` — unchanged
- [ ] MCP / OAuth / 背压 (-32001) — unchanged
- [ ] `crates/synthia-protocol/src/<existing_messages>/*` — only ADDITIONS (no removals)

---

## Sign-off

- [ ] All 6 quality gates green (G1-G6)
- [ ] All 34 spec → test traceability rows ✅
- [ ] All 4 docs remediation rows ✅
- [ ] All 8 out-of-scope confirmations ✅
- [ ] retrospective.md completed (cross-check at retrospectives)
- [ ] 25 PRs each merged with squash merge + green CI per PR

---

## Forward-looking (for change #2-#4 retrospective feed)

When change #1 archives, feed forward to change #2:

- boundary ownership between `synthia-service` (change #1) and `synthia-agent::main_loop` (change #2) — locked at PR-3.3 reverse-dep closure
- `HookOutcome::ForwardToMainAgent` semantic — change #2 main_loop needs to consume
- `AgentEvent::Custom` projection to `AgentMessage` — change #2 convertToLlm + transformContext must NOT collapse Custom
- `ToolContext::tool_id` field — change #2 main_loop needs to read for subagent governance
- `OutputBound` 60 行 trait — change #3 add tree-sitter AST 透传
- 4 capability broker migration boundary (change #3)