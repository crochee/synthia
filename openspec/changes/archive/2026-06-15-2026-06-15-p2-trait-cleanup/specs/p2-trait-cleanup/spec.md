# p2-trait-cleanup Specification

## Purpose

TBD - created by archiving change 2026-06-15-p2-trait-cleanup. Update Purpose after archive.

## ADDED Requirements

### Requirement: All 4 pure-YAGNI P2 traits MUST be removed entirely

The system MUST remove the following 4 `pub trait` definitions entirely,
because each has exactly 1 real implementation, 0 trait-bound usage,
0 dyn dispatch usage, and 0 `Arc<Trait>` / `Box<Trait>` wrapping
(verified by 4-0 REMOVE consensus on 2026-06-15):

1. `DoomLoopHandler` — `crates/synthia-agent/src/doom_loop_handler.rs:71`
2. `AuditWriter` — `crates/synthia-agent/src/audit.rs:17`
3. `EventStream` — `crates/synthia-server/src/event_stream.rs:64`
4. `SkillMatcher` — `crates/synthia-skill/src/matcher.rs:9`

The removal MUST:
- Delete each trait definition
- Delete the corresponding `impl Trait for Concrete` block
- Convert any unique method bodies to inherent methods on the
  concrete struct (so public method signatures are byte-identical
  except for the loss of trait dispatch)
- Update any `pub use` re-exports of the trait to remove the trait
- Update internal tests that referenced the trait to use the
  concrete type

#### Scenario: 4 pure-YAGNI traits are removed

- WHEN the workspace is searched for these 4 trait names in `.rs` files
- THEN `pub trait DoomLoopHandler` MUST NOT exist
- AND `pub trait AuditWriter` MUST NOT exist
- AND `pub trait EventStream` MUST NOT exist
- AND `pub trait SkillMatcher` MUST NOT exist
- AND the concrete struct types (`DefaultDoomLoopHandler`,
  `FileAuditWriter`, `SseEventStream`, `BM25Matcher`) MUST continue
  to expose all methods previously defined by the trait as inherent
  methods with identical signatures

### Requirement: Dead mcp_bridge module and ShellExecutor trait MUST be removed

The system MUST remove the entire `mcp_bridge` module from
`synthia-agent` (orphan module with no external `use` references) and
the `ShellExecutor` trait (mod.rs only, README duplicate addressed
separately).

The removal MUST:
- Delete `crates/synthia-agent/src/mcp_bridge.rs` entirely (contains
  `McpClient` trait + `McpTool` + `McpBridgeClient` + `McpBridge` +
  3 tests; module has 0 external imports)
- Remove `pub mod mcp_bridge;` from `crates/synthia-agent/src/lib.rs:26`
- Delete `pub trait ShellExecutor` from
  `crates/synthia-agent/src/shell/mod.rs:84`
- Convert `impl ShellExecutor for LocalShellExecutor` to inherent
  methods on `LocalShellExecutor`
- Keep `LocalShellExecutor` struct unchanged (still `pub use`d in
  `mod.rs:30`)

#### Scenario: Orphan mcp_bridge module is deleted

- WHEN the workspace is searched for `mcp_bridge` in `.rs` files
- THEN no `.rs` file outside the deleted module MUST reference it
- AND `pub mod mcp_bridge;` MUST NOT exist in `lib.rs`

#### Scenario: ShellExecutor trait is removed

- WHEN `crates/synthia-agent/src/shell/mod.rs` is inspected
- THEN `pub trait ShellExecutor` MUST NOT exist
- AND `LocalShellExecutor` MUST continue to expose its 2 methods
  as inherent methods

### Requirement: 4 dyn-dispatched P2 traits MUST be inlined to concrete types

The system MUST remove 4 dyn-dispatched P2 traits and replace
`dyn Trait` references with concrete types in their call sites:

1. `RiskEvaluator` — `crates/synthia-core/src/pbac/evaluation.rs:225`
2. `AuditLogger` — `crates/synthia-core/src/pbac/evaluation.rs:229`
3. `ContextService` — `crates/synthia-context/src/service.rs:85`
4. `SessionWriter` — `crates/synthia-context/src/session_writer.rs:6`

The replacement MUST:
- Delete each trait definition and impl
- Replace `Box<dyn Trait>` with `Box<ConcreteImpl>` in struct fields
- Replace `Arc<dyn Trait>` with `Arc<ConcreteImpl>` in struct fields
- Replace `&dyn Trait` with `&ConcreteImpl` in function parameters
- Rename generic builder methods that took `T: Trait + 'static` to
  concrete-typed methods (e.g.
  `with_risk_evaluator<R: RiskEvaluator>` →
  `with_standard_risk_evaluator(StandardRiskEvaluator)`)

This is a public-API breaking change: 4 builder methods lose their
generic parameter. Documented in the commit message and `verify.md`.

#### Scenario: PolicyEvaluator no longer uses dyn dispatch

- WHEN `crates/synthia-core/src/pbac/evaluation.rs` is inspected
- THEN `pub trait RiskEvaluator` MUST NOT exist
- AND `pub trait AuditLogger` MUST NOT exist
- AND `PolicyEvaluator` MUST contain fields typed
  `Option<Box<StandardRiskEvaluator>>` and
  `Option<Box<ConsoleAuditLogger>>` (concrete, not `dyn`)
- AND `PolicyEvaluator::with_standard_risk_evaluator` and
  `with_console_audit_logger` MUST exist as concrete-typed
  builder methods

#### Scenario: ContextService and SessionWriter are inlined

- WHEN `crates/synthia-context/src/service.rs` and
  `session_writer.rs` are inspected
- THEN `pub trait ContextService` MUST NOT exist
- AND `pub trait SessionWriter` MUST NOT exist
- AND `AgentDependencies::with_default_context_service(Arc<DefaultContextService>)`
  MUST exist as the only builder method
- AND any `&dyn SessionWriter` parameter MUST be `&NoOpSessionWriter`

### Requirement: PersistenceService MUST be inlined to Store inherent methods

The system MUST remove the `PersistenceService` trait (7 methods
including 2 generic methods) and move all methods to inherent
methods on `Store`.

The removal MUST:
- Delete `pub trait PersistenceService` from
  `crates/synthia-session/src/service.rs:20`
- Convert all 7 trait methods to inherent methods on `Store`
- Update 13 internal UFCS call sites in the same file's tests
  (e.g. `PersistenceService::save_session(&store, &session)` →
  `store.save_session(&session)`)
- Remove `pub use service::PersistenceService;` from
  `crates/synthia-session/src/lib.rs:184`
- Update `crates/synthia-session/tests/reexport_policy.rs` to
  remove `use synthia_session::PersistenceService;`

#### Scenario: PersistenceService trait is removed

- WHEN `crates/synthia-session/src/service.rs` is inspected
- THEN `pub trait PersistenceService` MUST NOT exist
- AND `Store` MUST expose all 7 methods (`save_session`,
  `load_session`, `append_message`, `load_messages_recent`,
  `load_messages_all`, `save_checkpoint`, `load_checkpoint`)
  as inherent methods with identical signatures
- AND no `PersistenceService::` UFCS prefix MUST appear in any
  test file under `crates/synthia-session/`

### Requirement: ShellExecutor README duplicate MUST be cleaned up

The system MUST remove the duplicate `pub trait ShellExecutor`
definition from `crates/synthia-agent/src/shell/README.md` to
eliminate grep pollution. The deletion MUST:
- Remove the `pub trait ShellExecutor: Send + Sync { ... }` block
  (line 37 area)
- Keep all other README content referencing the (now inherent)
  methods on `LocalShellExecutor`

#### Scenario: README duplicate is removed

- WHEN the workspace is searched for `pub trait ShellExecutor` in
  both `.rs` and `.md` files
- THEN no matches MUST exist (since the mod.rs definition was
  also removed in the prior sub-task)
- AND `crates/synthia-agent/src/shell/README.md` MUST remain as
  a documentation file

### Requirement: No regression after 12 P2 trait removals

The system MUST pass all quality gates after the 12 P2 trait
removals (4 pure YAGNI + 1 dead module + 1 ShellExecutor mod.rs
+ 4 dyn-Replace + 1 PersistenceService + 1 README cleanup).

#### Scenario: All quality gates pass

- WHEN `cargo check --workspace --all-targets` is executed
- THEN it MUST report 0 errors
- WHEN `cargo test --workspace` is executed
- THEN it MUST report 0 failures (baseline maintained or improved)
- WHEN `cargo clippy --all-targets --all-features --tests --all` is executed
- THEN it MUST report 0 warnings
- WHEN `cargo +nightly fmt --all` is executed
- THEN it MUST produce no diff
- WHEN `bash scripts/check_synced_spec_format.sh` is executed
- THEN it MUST report OK
- WHEN `openspec validate 2026-06-15-p2-trait-cleanup --strict` is executed
- THEN it MUST report the change as valid

#### Scenario: Public API breakage is intentional and documented

- WHEN the breaking changes (14 exported items + 4 builder method
  signatures) are summarised
- THEN the summary MUST be recorded in `verify.md`
- AND the 12 commits MUST each have a self-documenting message
  following the pattern `p2-cleanup: remove <TraitName> trait
  (rationale + scope)`
