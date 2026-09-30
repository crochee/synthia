## ADDED Requirements

### Requirement: Retryable trait MUST be removed

The system MUST remove the `pub trait Retryable` definition and its single
`impl Retryable for Error` from `crates/synthia-provider/src/retry.rs`.
The trait is a no-op wrapper that delegates to the inherent method
`Error::is_retryable()` defined in `crates/synthia-core/src/error.rs:218`.
Removal MUST NOT change runtime behavior because all call sites in
`retry.rs` already use Rust's method resolution, which prefers the
inherent method over the trait method.

#### Scenario: Retryable trait is deleted from the source tree

- WHEN the source file `crates/synthia-provider/src/retry.rs` is
  inspected
- THEN the file MUST NOT contain `pub trait Retryable` or
  `impl Retryable for Error`
- AND `Error::is_retryable()` MUST still be callable as an inherent
  method without any `use` import

#### Scenario: No regression in retry execution

- WHEN `cargo test -p synthia-provider` is executed after removal
- THEN all existing tests (including retry executor tests) MUST pass
  with 0 failures
- AND the test for `is_retryable_error(status)` MUST still cover
  HTTP status codes 429, 500, 502, 503, 504 as retryable

### Requirement: McpClientFacade duplicate definitions MUST be removed

The system MUST remove both `pub trait McpClientFacade` definitions
located at `crates/synthia-mcp/src/types.rs:95` and
`crates/synthia-mcp/src/traits.rs:16`. Both definitions have zero
implementations, zero call sites, and zero dynamic-dispatch usage.
The traits exist as a semantic duplicate in sibling modules allowed by
Rust's module-path-based name resolution but contribute confusion and
maintenance overhead. Removal MUST NOT change runtime behavior.

#### Scenario: No McpClientFacade trait remains in synthia-mcp

- WHEN the entire `crates/synthia-mcp/src/` directory is scanned for
  `McpClientFacade` references
- THEN the result MUST be zero matches in `.rs` files
- AND `crates/synthia-mcp/src/traits.rs` MUST be either deleted or
  contain no `pub trait` definition named `McpClientFacade`

#### Scenario: No regression in synthia-mcp compilation and tests

- WHEN `cargo check -p synthia-mcp` and `cargo test -p synthia-mcp`
  are executed after removal
- THEN both commands MUST succeed with 0 errors and 0 test failures
- AND the `McpClient` struct in `client.rs` MUST remain unchanged

### Requirement: SessionManager trait MUST be removed entirely

The system MUST remove the `pub trait SessionManager` (12 methods, defined
at `crates/synthia-session/src/session.rs:110`) entirely. The trait has
zero trait-bound usage, zero dyn dispatch usage, and exactly one real
implementation (`SessionFileStore` at
`crates/synthia-session/src/file_store.rs`).

**Decision (Sub-task C re-decision, 2026-06-15)**: The original plan
was to split `SessionManager` into `SessionReader` and `SessionWriter`
per ISP. After the 4-party review surfaced the 0-bound / 0-dyn / 1-impl
profile, the consensus shifted to **REMOVE the trait entirely** rather
than introduce two speculative traits. The concrete
`SessionFileStore` is the only consumer; the trait provided no
extensibility, no abstraction, and no test value (the in-tree mock
`MockSessionManager` was the second impl but only used to test the
trait's own default methods).

The removal MUST:
- Keep `SessionFileStore`'s public method signatures byte-identical to
  the previous `impl SessionManagerTrait for SessionFileStore` (now
  converted to inherent methods)
- Update `lib.rs` re-export policy documentation to record the removal
- Update `tests/reexport_policy.rs` to test only the struct path
  (the trait-dichotomy test is replaced with
  `test_session_manager_struct_canonical_path`)
- Preserve the `compile_fail` doctests in `lib.rs` that forbid
  `use synthia_session::SessionManager;` at the crate root (these
  remain valid because the struct `manager::SessionManager` is not
  re-exported at the root)

#### Scenario: SessionManager trait is removed from the source tree

- WHEN the trait definitions in `crates/synthia-session/src/session.rs`
  are inspected
- THEN `pub trait SessionManager` MUST NOT exist
- AND no replacement trait (`SessionReader`, `SessionWriter`, or other)
  MUST be introduced
- AND `SessionFileStore` MUST continue to expose all 12 methods as
  inherent methods with identical signatures

#### Scenario: No regression in synthia-session and dependent crates

- WHEN `cargo test --workspace` is executed after the removal
- THEN all tests in `synthia-session` and all downstream consumers
  MUST pass with 0 failures
- AND `cargo clippy --all-targets --all-features --tests --all` MUST
  report 0 warnings
- AND `cargo +nightly fmt --all` MUST produce no diff

#### Scenario: Call sites remain consistent (no orphan trait references)

- WHEN the workspace is searched for `SessionManager` references in
  `.rs` files after the removal
- THEN the result MUST be zero matches **in non-comment, non-test-fixture
  positions** (i.e. the only remaining matches are the legitimate
  `manager::SessionManager` struct, historical policy comments in
  `lib.rs`, and the `compile_fail` doctest fixtures)
