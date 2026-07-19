# Design: synthia-session re-export cleanup

## Root cause analysis

`synthia_session::lib.rs` had three modules that each define a type
called `Session`, `SessionManager`, or `SessionError`:

| Module    | Type             | Kind                | Role                          |
|-----------|------------------|---------------------|-------------------------------|
| `types`   | `Session`        | struct              | state-machine model record    |
| `session` | `Session`        | struct              | legacy conversation record    |
| `manager` | `SessionManager` | struct              | concrete implementation       |
| `session` | `SessionManager` | trait               | abstraction                   |
| `session` | `SessionError`   | enum                | trait-layer error             |

The original `lib.rs` re-export block was:

```rust
pub mod manager;
pub mod session;
pub mod types;

pub use manager::*;                                      // line 9
pub use service::PersistenceService;
pub use session::{Session, SessionError, SessionManager}; // line 11
pub use state_machine::{...};
pub use store::{...};
pub use token_budget::TokenBudgetMonitor;
pub use types::*;                                        // line 19
```

The problem: **explicit re-exports shadow glob re-exports in Rust**, so:

- `synthia_session::SessionManager` resolves to `session::SessionManager`
  (the trait) because the explicit `pub use session::SessionManager`
  on line 11 overrides the `manager::*` glob on line 9
- `synthia_session::Session` resolves to `types::Session` (the
  state-machine record) because the explicit `pub use session::Session`
  on line 11 SHOULD override the `types::*` glob on line 19... except
  that `types` is declared AFTER `session` in the file, and the glob
  resolution order in Rust is undefined
- `synthia_session::SessionError` resolves to `session::SessionError`
  (the trait-layer error) via the explicit re-export

In practice, this produced 40+ compile errors in the test suite
because `tests/session_persistence.rs` used
`use synthia_session::Session;` expecting the state-machine record,
but depending on Rust's resolution order sometimes got the legacy
conversation record (which has a different `new` signature and
fields).

The bug was tracked as a follow-up in `project_memory.md` and the
"shadowing" pattern is documented in
`.trae/rules/agent_rule.md` (P6: Distrust by Default) as a class of
trap to avoid.

## Fix design

### Change 1: Remove the explicit re-export

Delete the line:

```rust
pub use session::{Session, SessionError, SessionManager};
```

This is the only structural fix needed. The remaining glob re-exports
are unambiguous because each remaining name has a single owner
(`types::*` for `Session`, `SessionConfig`, etc.; `manager::*` for
`SessionFilter`, `SessionInfo`; etc.).

### Change 2: Document the policy in a comment block

Add a 30-line policy block at the top of `lib.rs` that:

- Lists the three known-conflict names
- Documents the canonical import paths
- States the rule: "do NOT add a new `pub use module::Foo` for any
  type that exists in more than one module"
- Lists the 5 typical violations to avoid

### Change 3: Update live consumers

Two live consumers used the now-unresolved paths:

- `crates/synthia-agent/src/error/mod.rs:404` used
  `From<synthia_session::SessionError>` — updated to
  `From<synthia_session::session::SessionError>`
- `crates/synthia-server/tests/e2e_server_sse_test.rs` and
  `e2e_server_ws_test.rs` used `use synthia_session::SessionManager;`
  — updated to `use synthia_session::manager::SessionManager;`

A grep across the workspace confirmed these were the only live
consumers of the affected short paths.

### Change 4: Delete dead code

Two modules were never declared in their `lib.rs` and have been
dead code since at least 2026-06-13:

- `crates/synthia-memory/src/memory_pipeline.rs` (279 lines) + 6
  submodules in `crates/synthia-memory/src/memory_pipeline/`
  (~2342 lines), totaling ~2621 lines. Both the file and directory
  contents reference `synthia_session::SessionManager` (the trait)
  and would not compile if wired up.
- `crates/synthia-cli/src/scheduler/mod.rs` (124 lines) — also
  references the trait, would not compile if wired up.

Deleting these prevents future wiring attempts from hitting the
same shadowing bug.

## Three-layer guard design

Each layer defends the policy from a different angle. The intent is
that any single layer can be bypassed but bypassing all three
simultaneously requires deliberate effort.

### Layer 1: `compile_fail` doc tests

Doc tests in `lib.rs` itself. The Rust compiler runs these on
`cargo test --doc` and fails the build if a `compile_fail` test
suddenly starts compiling, or a positive test stops compiling.

**Forbidden patterns pinned by `compile_fail`:**

```rust
/// ```compile_fail
/// use synthia_session::SessionManager;
/// fn _shadowing_trap() -> Box<dyn synthia_session::SessionManager> { ... }
/// ```
```

```rust
/// ```compile_fail
/// use synthia_session::SessionError;
/// fn _shadowing_trap(e: synthia_session::SessionError) -> String { ... }
/// ```
```

```rust
/// ```compile_fail
/// // The historical offender: pub use session::{Session, SessionError, SessionManager};
/// use synthia_session::SessionManager as _Alias;
/// ```
```

**Canonical paths pinned by positive doc tests:**

```rust
/// ```
/// use synthia_session::manager::SessionManager;
/// use synthia_session::session::{Session as LegacySessionRecord, SessionError};
/// use synthia_session::types::{Session, SessionConfig, SessionState};
/// use synthia_session::store::Store;
/// use synthia_session::state_machine::SessionStateMachine;
/// ```
```

**Stable re-exports pinned by positive doc tests:**

```rust
/// ```
/// use synthia_session::{
///     CheckpointData, PersistenceService, SessionFilter, SessionInfo,
///     SessionMetadata, SessionStateMachine, StateEnterEffect,
///     StateMachineError, Store, TokenBudget, TokenBudgetMonitor,
///     TokenBudgetStatus,
/// };
/// ```
```

### Layer 2: Integration test `tests/reexport_policy.rs`

A separate test binary in `tests/`. Catches:

1. **Drift in canonical paths**: if `types::Session` is renamed to
   `types::SessionModel`, the import in the test file fails loudly
2. **Drift in stable re-exports**: if `pub use state_machine::*` is
   removed, the `use synthia_session::SessionStateMachine` line in
   the test fails
3. **Policy block drift**: a test that does
   `include_str!("../src/lib.rs")` and asserts on the policy block
   content
4. **Re-introduction of the historical offender**: a grep over the
   active code in `lib.rs` that fails if any un-commented line
   matches `pub use session::`
5. **Type collision**: positive tests that confirm `Session` at
   the crate root is the state-machine record, not the legacy
   conversation record, by attempting to call `Session::new(...)` —
   which is only defined on the state-machine record, not the
   legacy one

### Layer 3: CI shell script `scripts/check_reexports.sh`

Pure bash + awk + grep. Runs in < 100 ms. Five checks:

1. **Historical offender absent**: greps active (un-commented) code
   in `lib.rs` for `pub use ... session::... SessionManager` — fails
   if found
2. **Required module re-exports present**: greps `lib.rs` for the
   `manager::`, `service::PersistenceService`, `state_machine::`,
   `store::`, `token_budget::`, `types::*` re-exports
3. **Policy header present**: greps `lib.rs` for the
   `Re-export policy (synthia-session)` header and the 3 conflict
   names
4. **Layer 2 file exists**: checks
   `crates/synthia-session/tests/reexport_policy.rs` exists and
   mentions all 3 layers
5. **Workspace uses qualified paths**: greps all `*.rs` files in
   the workspace (excluding `target/`, `.git/`, `lib.rs`, and the
   policy test itself) for unqualified
   `synthia_session::SessionManager` or
   `synthia_session::SessionError` — fails if found

CI integration is left to the user's CI system; the script is
self-contained and can be called from any workflow file.

## Why not a custom clippy lint?

Considered and rejected. A custom clippy lint would require:

- A separate `clippy_lints` crate
- Implementing the lint logic (declaration scanning, resolution
  analysis)
- Compiling on nightly (clippy lints are nightly-only)
- Configuring the workspace to load the custom lint

The three-layer approach achieves the same goal (catch the bug at
multiple points in the dev cycle) with:

- 0 external dependencies
- 0 nightly-only features
- 1 file in `tests/` (auto-discovered by `cargo test`)
- 1 file in `scripts/` (callable from any CI)
- 6 inline doc tests (auto-discovered by `cargo test --doc`)

The three-layer approach is also MORE robust because each layer
catches a different class of failure (compile failure, type
mismatch, structural drift).

## Why delete the dead code?

Two reasons:

1. **Carrying dead code is a future trap**: the moment someone
   wires `pub mod memory_pipeline;` into
   `crates/synthia-memory/src/lib.rs`, the build breaks with the
   same shadowing error. The dead code is a landmine.
2. **YAGNI**: 2745 lines of unused code is a maintenance burden
   even if it compiles. Deleting now is cheaper than deferring.

## Verification plan

1. Run `cargo check --workspace --tests` — must be clean
2. Run `cargo test -p synthia-session` — all 6 reexport_policy
   tests pass, all existing tests continue to pass
3. Run `cargo test -p synthia-session --doc` — all 6 doc tests
   pass (3 compile_fail + 3 positive)
4. Run `bash scripts/check_reexports.sh` — all 5 checks pass
5. Mutate `lib.rs` to add back `pub use session::{Session,
   SessionError, SessionManager};` — confirm that the
   `test_lib_rs_documents_policy` integration test fails to
   compile
6. Run `cargo test -p synthia-agent` — the 8
   `explicit_recovery_paths` tests continue to pass; the
   3 pre-existing failures (master baseline) remain
