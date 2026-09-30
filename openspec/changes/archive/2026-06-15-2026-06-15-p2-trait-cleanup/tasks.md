# Tasks: p2-trait-cleanup

## Phase 1: Pre-flight audit (✅ DONE)

- [x] 1.1 — Re-grep all 12 P2 trait current usage (impl/bound/dyn/call_site) via shell loop
- [x] 1.2 — Identify mcp_bridge module as fully orphaned (no external `use` of `mcp_bridge::*`)
- [x] 1.3 — 4-party review of all 12 (12/12 ≥ 3-1 consensus, 11/12 = 4-0)
- [x] 1.4 — Categorize into 4 sub-tasks by complexity tier

## Phase 2: Sub-task A — 4 pure YAGNI removes

- [ ] 2.1 — Remove `DoomLoopHandler` trait + impl from [crates/synthia-agent/src/doom_loop_handler.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/doom_loop_handler.rs)
  - Keep: `DoomLoopConfig`, `doom_loop_detected` function
  - Verify: `cargo check -p synthia-agent`
- [ ] 2.2 — Remove `AuditWriter` trait + impl from [crates/synthia-agent/src/audit.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/audit.rs)
  - Keep: `FileAuditWriter` (inherent)
  - Update test: `test_audit_writer_trait_impl` → `test_audit_writer`
  - Verify: `cargo check -p synthia-agent`
- [ ] 2.3 — Remove `EventStream` trait + impl from [crates/synthia-server/src/event_stream.rs](file:///home/crochee/workspace/synthia/crates/synthia-server/src/event_stream.rs)
  - Keep: `SseEventStream` (inherent), `EventBroadcaster`
  - Update `synthia-server/src/lib.rs` re-export
  - Verify: `cargo check -p synthia-server`
- [ ] 2.4 — Remove `SkillMatcher` trait + impl from [crates/synthia-skill/src/matcher.rs](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/matcher.rs)
  - Keep: `BM25Matcher` (inherent)
  - Update `synthia-skill/src/lib.rs` re-export
  - Verify: `cargo check -p synthia-skill`

## Phase 3: Sub-task B — Dead module + ShellExecutor

- [ ] 3.1 — Delete entire `mcp_bridge.rs` module from synthia-agent (orphan module)
  - Files: delete `crates/synthia-agent/src/mcp_bridge.rs`
  - Edit `crates/synthia-agent/src/lib.rs`: remove `pub mod mcp_bridge;`
  - Verify: `cargo check -p synthia-agent`, `grep -rn 'mcp_bridge' crates/ --include='*.rs'` → 0
- [ ] 3.2 — Remove `ShellExecutor` trait (mod.rs only) from [crates/synthia-agent/src/shell/mod.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/shell/mod.rs)
  - Keep: `LocalShellExecutor` (inherent)
  - README duplicate handled in Sub-task D
  - Verify: `cargo check -p synthia-agent`

## Phase 4: Sub-task C — 4 dyn → concrete type

- [ ] 4.1 — Remove `RiskEvaluator` trait from [crates/synthia-core/src/pbac/evaluation.rs](file:///home/crochee/workspace/synthia/crates/synthia-core/src/pbac/evaluation.rs)
  - Field: `Option<Box<dyn RiskEvaluator>>` → `Option<Box<StandardRiskEvaluator>>`
  - Method: `with_risk_evaluator<R: RiskEvaluator>` → `with_standard_risk_evaluator(StandardRiskEvaluator)`
  - Verify: `cargo check --workspace`
- [ ] 4.2 — Remove `AuditLogger` trait from [crates/synthia-core/src/pbac/evaluation.rs](file:///home/crochee/workspace/synthia/crates/synthia-core/src/pbac/evaluation.rs)
  - Field: `Option<Box<dyn AuditLogger>>` → `Option<Box<ConsoleAuditLogger>>`
  - Method: `with_audit_logger<L: AuditLogger>` → `with_console_audit_logger(ConsoleAuditLogger)`
  - Verify: `cargo check --workspace`
- [ ] 4.3 — Remove `ContextService` trait from [crates/synthia-context/src/service.rs](file:///home/crochee/workspace/synthia/crates/synthia-context/src/service.rs)
  - Field in `AgentDependencies`: `Option<Arc<dyn ContextService>>` → `Option<Arc<DefaultContextService>>`
  - Method: `with_context_service(Arc<dyn ContextService>)` → `with_default_context_service(Arc<DefaultContextService>)`
  - Verify: `cargo check --workspace`
- [ ] 4.4 — Remove `SessionWriter` trait from [crates/synthia-context/src/session_writer.rs](file:///home/crochee/workspace/synthia/crates/synthia-context/src/session_writer.rs)
  - Parameter: `&dyn SessionWriter` → `&NoOpSessionWriter` in call sites
  - Verify: `cargo check --workspace`

## Phase 5: Sub-task D — PersistenceService + README

- [ ] 5.1 — Remove `PersistenceService` trait from [crates/synthia-session/src/service.rs](file:///home/crochee/workspace/synthia/crates/synthia-session/src/service.rs)
  - Convert 7 trait methods to `Store` inherent methods
  - Update 13 internal UFCS call sites in same file's tests
  - Update `crates/synthia-session/src/lib.rs` re-export
  - Update `crates/synthia-session/tests/reexport_policy.rs` (remove `use synthia_session::PersistenceService`)
  - Verify: `cargo check --workspace`, `cargo test -p synthia-session`
- [ ] 5.2 — Clean up `ShellExecutor` README duplicate from [crates/synthia-agent/src/shell/README.md](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/shell/README.md)
  - Delete lines defining `pub trait ShellExecutor: Send + Sync { ... }` (line 37 area)
  - Keep documentation referencing the trait (modify wording if needed)
  - Verify: `grep -rn 'pub trait ShellExecutor' crates/` → 0

## Phase 6: Quality gates (full workspace)

- [ ] 6.1 — `cargo check --workspace --all-targets` → 0 errors
- [ ] 6.2 — `cargo test --workspace` → 0 failures
- [ ] 6.3 — `cargo clippy --all-targets --all-features --tests --all` → 0 warnings
- [ ] 6.4 — `cargo +nightly fmt --all` → formatted
- [ ] 6.5 — `bash scripts/check_synced_spec_format.sh` → OK
- [ ] 6.6 — `openspec validate 2026-06-15-p2-trait-cleanup --strict` → valid
- [ ] 6.7 — Final `grep` audit: 12/12 traits not in `crates/` source (except allowed: doc comments, historical references)

## Phase 7: Verify + archive

- [ ] 7.1 — Fill [verify.md](file:///home/crochee/workspace/synthia/openspec/changes/2026-06-15-p2-trait-cleanup/verify.md) with execution evidence
- [ ] 7.2 — 12 commits (1 per trait/concern) following P0/P1 pattern
- [ ] 7.3 — `yes | openspec archive 2026-06-15-p2-trait-cleanup`

## 总计: 22 tasks

- Phase 1 (pre-flight): 4 (✅ done)
- Phase 2 (Sub-task A): 4
- Phase 3 (Sub-task B): 2
- Phase 4 (Sub-task C): 4
- Phase 5 (Sub-task D): 2
- Phase 6 (gates): 7
- Phase 7 (verify+archive): 3
- (includes 1 spec write task outside)

## 依赖关系

- Phase 2 完全独立,可顺序或并行做
- Phase 3 独立
- Phase 4 顺序(同文件 evaluation.rs 中 Risk + Audit 可 1 commit)
- Phase 5 独立
- Phase 6 依赖所有 Phase 2-5
- Phase 7 依赖 Phase 6

## 与 P0/P1 决策对齐

- 同样的"1 trait per commit"模式 (12 commits = 12 traits)
- 同样的 4-party 共识 (3-1 minimum, 4-0 preferred)
- 同样的"trait → inherent"路径
- 同样的"公共 API 破坏透明记录"原则
- 同样的 spec.md → tasks.md → verify.md 流程
