# Spec: synthia-session-reexport-policy

## Purpose

The `synthia-session` crate historically had three modules that each
define a type called `Session`, `SessionManager`, or `SessionError`.
The crate root re-exported these by name from `pub use
session::{...}` while also glob-re-exporting `manager::*` and
`types::*`. The explicit re-exports shadowed the globs, producing
ambiguous name resolution and 40+ compile errors in downstream test
code.

This capability pins the canonical import paths for the
multi-ownership types, forbids re-introducing the conflicting
crate-root re-exports, and mandates a three-layer enforcement
mechanism (doc tests, integration test, CI script) to keep the
policy self-enforcing.

## Requirements

### Requirement: Multi-ownership types have canonical qualified paths

The system MUST expose the three multi-ownership types
(`Session`, `SessionManager`, `SessionError`) only via qualified
paths. The crate root MUST NOT re-export these by name from more
than one module.

#### Scenario: Session resolution

- WHEN a consumer writes `use synthia_session::Session;`
- THEN the name MUST resolve to the `types::Session` (state-machine
  model record) because the `types::*` glob is the unique owner of
  this name at the crate root.
- AND the name MUST NOT be re-exported from `session::Session`
  (legacy conversation record) at the crate root, because doing so
  would create an ambiguous name collision.

#### Scenario: SessionManager resolution

- WHEN a consumer writes `use synthia_session::SessionManager;`
- THEN the name MUST NOT resolve at the crate root.
- AND the consumer MUST write
  `use synthia_session::manager::SessionManager;` to get the
  concrete struct.
- AND the consumer MUST write
  `use synthia_session::session::SessionManager;` to get the trait.

#### Scenario: SessionError resolution

- WHEN a consumer writes `use synthia_session::SessionError;`
- THEN the name MUST NOT resolve at the crate root.
- AND the consumer MUST write
  `use synthia_session::session::SessionError;` to get the
  trait-layer error.

### Requirement: Crate root MUST re-export single-ownership types

The system MUST re-export types at the crate root that exist in only
one module, either by name or by glob. Examples that MUST remain
re-exported at the crate root:

- `synthia_session::Session` (from `types::*`)
- `synthia_session::SessionConfig` (from `types::*`)
- `synthia_session::SessionState` (from `types::*`)
- `synthia_session::SessionFilter` (from `manager::*`)
- `synthia_session::SessionInfo` (from `manager::*`)
- `synthia_session::Store` (from `store::*`)
- `synthia_session::SessionStateMachine` (from `state_machine::*`)
- `synthia_session::PersistenceService` (from `service::*`)

#### Scenario: Stable re-export list is preserved

- WHEN a consumer writes `use synthia_session::{Session, SessionConfig, ...};`
- THEN all 8 stable re-exports in the list above MUST resolve.
- IF any stable re-export is removed, the
  `test_stable_reexports_at_crate_root` integration test MUST
  fail.

### Requirement: Policy block documents the re-export restrictions

The `synthia_session::lib.rs` source file MUST contain a comment
block that:

- Lists the three known-conflict names
- Documents the canonical import paths for each
- States the rule against adding a new `pub use module::Foo` for
  any type that exists in more than one module

#### Scenario: Policy block is present and complete

- WHEN a contributor opens `crates/synthia-session/src/lib.rs`
- THEN they MUST see a comment block titled
  `Re-export policy (synthia-session)`.
- AND the block MUST mention `Session`, `SessionManager`, AND
  `SessionError` as the three known-conflict names.

### Requirement: Three-layer guard enforces the policy

The system MUST provide three independent layers of defense
against re-introducing the conflicting re-export.

#### Scenario: Layer 1 — compile_fail doc tests

- WHEN a contributor runs `cargo test -p synthia-session --doc`
- THEN the `compile_fail` doc tests for `SessionManager` and
  `SessionError` MUST each fail to compile (because the names do
  not resolve at the crate root).
- AND the positive doc tests for the canonical qualified paths
  and the stable re-exports MUST compile and run successfully.
- IF a future contributor re-introduces the offending
  `pub use session::{Session, SessionError, SessionManager};`,
  THEN the `compile_fail` doc test for the historical offender
  MUST start passing (because the name suddenly resolves), and CI
  MUST flag this as a regression.

#### Scenario: Layer 2 — integration test

- WHEN a contributor runs
  `cargo test -p synthia-session --test reexport_policy`
- THEN all 6 tests in `reexport_policy.rs` MUST pass.
- AND the test `test_lib_rs_documents_policy` MUST verify that
  the policy block is present and that no un-commented line in
  `lib.rs` matches the pattern `pub use session::`.
- IF a future contributor re-introduces the offending re-export,
  THEN the test `test_lib_rs_documents_policy` MUST fail to
  compile (because the crate-root `Session` becomes ambiguous and
  `Session::new(...)` stops resolving).

#### Scenario: Layer 3 — CI shell script

- WHEN CI runs `bash scripts/check_reexports.sh`
- THEN all 5 checks MUST pass:
  1. Historical offender absent
  2. Required module re-exports present
  3. Policy header documents the 3 conflict names
  4. Layer 2 integration test file exists
  5. Workspace uses qualified paths
- IF any check fails, the script MUST exit with a non-zero status
  and a human-readable error message identifying the violation.

### Requirement: Future API changes must update all three layers

A future contributor MUST update all three layers when changing the re-export structure of `synthia_session` (e.g. renaming `types::Session` to `types::SessionModel`, or adding a new multi-ownership type). The three layers are:

- Update the doc tests in `src/lib.rs` (Layer 1)
- Update the integration test in `tests/reexport_policy.rs` (Layer 2)
- Update the CI script `scripts/check_reexports.sh` (Layer 3)

The contributor SHALL revert the change or complete the missing layer before merge if any of the three layers is skipped.

#### Scenario: Renaming a canonical type

- WHEN a contributor renames `types::Session` to `types::SessionModel`
- THEN they MUST update the positive doc test
  `_doc_canonical_paths` in `src/lib.rs` to use the new name.
- AND they MUST update the integration test
  `test_crate_root_session_is_state_machine_record` in
  `tests/reexport_policy.rs` to use the new name.
- AND they MUST update the workspace grep in
  `scripts/check_reexports.sh` if the old name appears in any of
  its checks.
- IF any of the three layers is skipped, the contributor MUST
  revert the rename or complete the missing layer before merge.

#### Scenario: Adding a new multi-ownership type

- WHEN a contributor adds a new type that exists in more than one
  module (e.g. a new `Checkpoint` defined in both `types` and
  `store`)
- THEN they MUST NOT add a `pub use module::Checkpoint` to the
  crate root.
- AND they MUST add a new `compile_fail` doc test for the
  forbidden pattern.
- AND they MUST add a new positive doc test for the canonical
  path.
- AND they MUST add a new test case to
  `tests/reexport_policy.rs` covering the new type.
- AND they MUST update the workspace grep in
  `scripts/check_reexports.sh` to check for the new forbidden
  pattern.

## Rationale

The original bug was a structural one: a single line in `lib.rs`
(`pub use session::{Session, SessionError, SessionManager};`) caused
ambiguous name resolution that cascaded into 40+ test failures and
multiple rounds of "fix the imports" churn. The bug is easy to
re-introduce because the offending line looks innocuous. A
three-layer guard ensures that any re-introduction is caught
immediately, in three different parts of the dev cycle (compile
time, test time, CI time), by three different mechanisms (compiler
doc test, runtime test, shell script).

The three layers are intentionally redundant. A contributor who
deletes the doc tests to "tidy up" will still hit the integration
test. A contributor who renames `types::Session` will still hit
the doc tests. A contributor who somehow bypasses both will still
hit the CI script. The guard is "self-healing" in the sense that
no single layer is on the critical path.
