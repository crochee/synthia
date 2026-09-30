## Why

`synthia_session::lib.rs` historically re-exported `Session`, `SessionError`,
and `SessionManager` by name from `pub use session::{...}` while ALSO glob
re-exporting `manager::*` and `types::*`. The explicit re-export silently
shadowed the glob, so `synthia_session::SessionManager` resolved to the
**trait** in `session::SessionManager` instead of the **concrete struct**
in `manager::SessionManager` (and vice versa depending on the order of
`pub use` lines). This caused 40+ compile errors in
`synthia-session/tests/session_persistence.rs` and
`synthia-session/tests/session_manager_integration.rs` and was tracked as
a follow-up in `project_memory.md` ("synthia-session has pre-existing
dual Session AND dual SessionManager re-export shadowing").

The bug is structural: the `pub use` line itself is the offender, and
without an enforced policy any future contributor can re-introduce it
with a one-line edit. This change removes the offending re-exports AND
adds a three-layer guard (compile-fail doc tests, integration test, CI
shell script) to make the policy self-enforcing.

## What Changes

**Crate-root re-exports in `synthia_session::lib.rs`**
- From: `pub use session::{Session, SessionError, SessionManager};` plus
  `pub use manager::*;` (glob shadows the explicit; ambiguous resolution)
- To: explicit re-export of the 3 conflict names is REMOVED; consumers
  MUST use qualified paths (`manager::SessionManager`, `session::SessionError`,
  `session::SessionManager` (the trait), `types::Session`, `session::Session`
  (the legacy conversation record))
- Reason: name-shadowing trap is unfixable in place; the explicit re-export
  must be deleted and callers updated to use the qualified paths
- Impact: BREAKING for any code that imports `synthia_session::SessionManager`
  or `synthia_session::SessionError`. Two live consumer sites were
  updated in this same change (`synthia-agent/src/error/mod.rs` and
  the two `synthia-server/tests/e2e_server_*_test.rs` files)

**Policy documentation in `lib.rs`**
- From: no documentation of why the re-exports look the way they do
- To: a 30-line policy block at the top of `lib.rs` documents the three
  known-conflict names, the canonical paths, and the rule "don't add a
  new `pub use module::Foo` for any type that exists in more than one
  module"
- Reason: future contributors see WHY the re-exports are restricted
- Impact: documentation-only, no behavior change

**Layer-1 defense: `compile_fail` doc tests in `lib.rs`**
- From: no doc tests for the re-export policy
- To: 3 `compile_fail` doc tests that pin the FORBIDDEN patterns
  (`use synthia_session::SessionManager`, `use synthia_session::SessionError`,
  the historical offender `pub use session::{Session, SessionError,
  SessionManager}`), plus 3 positive doc tests pinning the CANONICAL
  paths and stable re-exports
- Reason: structural invariant must be checked at every `cargo test`
- Impact: doc tests run automatically; no extra setup needed

**Layer-2 defense: integration test `tests/reexport_policy.rs`**
- From: no integration test for the re-export policy
- To: 6 tests covering 6 invariants (crate-root `Session` is the
  state-machine record; legacy `Session` is qualified-only;
  `SessionManager` resolves to BOTH the struct and the trait via
  qualified paths; `SessionError` is qualified-only; stable re-exports
  are stable; `lib.rs` policy block is present and correct)
- Reason: Layer 1 can be bypassed by deleting the doc tests; Layer 2
  catches the same bug from a different angle, plus catches
  canonical-path drift (e.g. someone renaming `types::Session` would
  break Layer 2 loudly even if Layer 1 is deleted)
- Impact: runs on `cargo test --test reexport_policy`

**Layer-3 defense: CI shell script `scripts/check_reexports.sh`**
- From: no CI-level guard
- To: 5-check shell script that greps `lib.rs` for the historical
  offender, verifies all required module re-exports are present,
  verifies the policy block mentions the 3 conflict names, verifies
  the Layer 2 file exists, and greps the workspace for any unqualified
  `synthia_session::SessionManager` / `synthia_session::SessionError`
  usage
- Reason: cheap (< 100ms), portable (pure bash + awk + grep), catches
  the bug even before any Rust toolchain runs
- Impact: opt-in; CI can call this script as a pre-check or a post-check

**Dead code removal**
- From: `synthia-memory/src/memory_pipeline.rs` (279 lines) + 6
  submodules (~2342 lines), and `synthia-cli/src/scheduler/mod.rs`
  (124 lines), all referencing the shadowed `SessionManager` trait
- To: deleted (they were never declared in their respective `lib.rs`
  and have been dead code since the `Session` type-shadowing was
  introduced)
- Reason: dead code that won't compile when wired up; carrying it
  around means any future attempt to wire it up hits the same shadowing
  bug
- Impact: removes ~2745 lines of dead code

## Capabilities

### New Capabilities

- `synthia-session-reexport-policy`: documents the
  multi-ownership types (`Session`, `SessionManager`, `SessionError`),
  the canonical import paths, and the rule against crate-root
  re-exports for multi-ownership types. Establishes the three-layer
  enforcement (doc tests + integration test + CI script).

### Modified Capabilities

(none — no existing capability needs a delta)

## Impact

- **Code**:
  - `crates/synthia-session/src/lib.rs` — removed 1 line of explicit
    re-export; added ~50 lines of policy block + 6 doc tests
  - `crates/synthia-session/tests/reexport_policy.rs` — new file,
    6 tests, ~170 lines
  - `crates/synthia-agent/src/error/mod.rs` — 1 line updated
    (qualified `synthia_session::session::SessionError`)
  - `crates/synthia-server/tests/e2e_server_sse_test.rs` — 1 line
    updated (qualified `synthia_session::manager::SessionManager`)
  - `crates/synthia-server/tests/e2e_server_ws_test.rs` — 1 line
    updated (same)
  - `crates/synthia-memory/src/memory_pipeline.rs` + 6 submodules —
    deleted (8 files, ~2621 lines)
  - `crates/synthia-cli/src/scheduler/mod.rs` — deleted (124 lines)
  - `scripts/check_reexports.sh` — new file, ~140 lines
- **API**:
  - BREAKING: `synthia_session::SessionManager` and
    `synthia_session::SessionError` no longer resolve at the crate
    root. Two live consumer sites were updated; the two
    `e2e_server_*_test.rs` files were also updated.
  - All other re-exports preserved (single-ownership types are still
    at the root, e.g. `Session`, `SessionConfig`, `SessionFilter`).
- **Dependencies**: no new external dependencies
- **Spec validation**: 1 new capability spec
  (`synthia-session-reexport-policy`) is added; 0 existing specs are
  modified
- **Tests**: 0 new tests in the regular test suite (all 6 new tests
  live in `reexport_policy.rs`); 3 new doc tests; 5 new CI checks
  in the shell script
