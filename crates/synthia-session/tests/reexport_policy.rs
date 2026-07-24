//! Layer-2 defense for the `synthia_session` re-export policy.
//!
//! This integration test is the RUNTIME complement to the compile-fail
//! doc tests in `src/lib.rs`. The doc tests prove "the forbidden paths
//! don't compile". This test proves "the canonical paths DO compile and
//! resolve to the expected types". If either side breaks, the policy
//! has been violated and CI must fail.
//!
//! ## Why three layers?
//!
//! * **Layer 1 (doc tests)**: catches the FORBIDDEN patterns. Easy to
//!   bypass if someone deletes the doc tests.
//! * **Layer 2 (this test)**: catches CANONICAL paths drifting. A
//!   renaming of `types::Session` to `types::SessionModel` would break
//!   this test loudly, even if the forbidden doc tests were removed.
//! * **Layer 3 (CI script)**: catches structural violations of the
//!   `lib.rs` re-export layout (e.g. someone re-adds
//!   `pub use session::{Session, SessionError, SessionManagerTrait}`).
//!
//! ## 2026-06-15 update (Sub-task C of `p0-trait-review-remediation`)
//!
//! The historical `session::SessionManager` trait was REMOVED because it
//! had 0 trait bound usage, 0 dyn dispatch, and 1 real impl. After
//! removal, only the struct `synthia_session::manager::SessionManager`
//! remains. The old `test_session_manager_qualified_paths` test (which
//! distinguished the trait from the struct) has been replaced with
//! `test_session_manager_struct_canonical_path` below.
//!
//! If you change the public API, update ALL THREE layers in the same
//! change. See `src/lib.rs` for the policy block.

use std::path::PathBuf;

// Force `StateMachineError` (a type) to be re-evaluated at the type
// level. It is re-exported from `state_machine`; if it disappears from
// the crate root, the import below fails to compile.
//
// NOTE 2026-06-15: `PersistenceService` (formerly also a re-export
// from `service`) was REMOVED in change `2026-06-15-p2-trait-cleanup`
// because the trait had 0 trait-bound usage, 0 dyn dispatch (the
// `AgentDependencies` field already used the concrete `Store`), and
// exactly 1 real implementation. The crate root now re-exports the
// `load_session` / `metadata_to_session` helpers from `service`
// instead, and `Store` continues to expose the underlying methods
// directly.
#[allow(unused_imports)]
use synthia_session::StateMachineError as _StateMachineErrorAlias;
// Canonical (qualified) paths for the multi-ownership types:
use synthia_session::manager::SessionManager as ManagerStruct;
use synthia_session::{
    CheckpointData,
    Session,
    SessionConfig,
    SessionFilter,
    SessionInfo,
    SessionMetadata,
    SessionState,
    SessionStateMachine,
    StateEnterEffect,
    StateMachineError,
    Store,
    TokenBudget,
    TokenBudgetMonitor,
    TokenBudgetStatus,
    session::{Session as LegacySessionRecord, SessionError},
    types::Session as StateMachineRecord,
};

/// Verify that the `Session` at the crate root is the state-machine
/// model record (the one from `types::*`). If someone re-introduces the
/// historical offender (`pub use session::{Session, ...}`), the two
/// `Session` types will collide and this test will fail to compile.
#[test]
fn test_crate_root_session_is_state_machine_record() {
    // The `Session` accessible at the crate root MUST be the
    // state-machine record. If it's the legacy conversation record
    // instead, the function below won't accept it.
    fn assert_state_machine(_: Session) {}
    assert_state_machine(Session::new("test-id".to_string()));

    // A function that ONLY accepts the canonical `StateMachineRecord`
    // must also accept the crate-root `Session` (i.e. they are the
    // same type).
    fn assert_record(_: StateMachineRecord) {}
    assert_record(Session::new("test-id".to_string()));
}

/// Verify that the legacy conversation record is reachable via the
/// qualified path `synthia_session::session::Session` and that the two
/// `Session` types are DISTINCT (i.e. shadowing is NOT happening at
/// the crate root).
#[test]
fn test_legacy_session_record_is_qualified() {
    // Both names must be reachable.
    fn assert_state_machine(_: Session) {}
    fn assert_legacy(_: LegacySessionRecord) {}

    assert_state_machine(Session::new("sm".to_string()));
    assert_legacy(LegacySessionRecord::default());
}

/// Verify that `SessionManager` is reachable only via the qualified
/// path `synthia_session::manager::SessionManager` (the struct).
///
/// Historical note: a sibling `pub trait SessionManager` used to live
/// in `synthia_session::session::SessionManager`. It was REMOVED on
/// 2026-06-15 in change `2026-06-15-p0-trait-review-remediation`
/// Sub-task C because it had 0 trait bound usage, 0 dyn dispatch, and
/// 1 real impl. After removal, the `manager::SessionManager` struct is
/// the sole `SessionManager` in the crate. The crate-root path
/// `synthia_session::SessionManager` is still FORBIDDEN (see
/// `_doc_session_manager_forbidden` in `src/lib.rs`).
#[test]
fn test_session_manager_struct_canonical_path() {
    // The struct (concrete impl) lives in `manager`. If someone
    // re-introduces a `pub use session::SessionManagerTrait` line that
    // shadows this struct, the constructor signature would still
    // match (the trait would not have a `new` method), so we
    // additionally check that the constructor exists on the struct.
    let _: fn(PathBuf) -> ManagerStruct = |p| ManagerStruct::new(p);
}

/// Verify that `SessionError` is reachable only via the qualified path
/// `synthia_session::session::SessionError`.
#[test]
fn test_session_error_qualified_path() {
    let e: SessionError = SessionError::session("test".to_string());
    let _: String = format!("{:?}", e);
}

/// Smoke test for the stable crate-root re-exports. If any of these
/// fail to resolve, the re-export list in `src/lib.rs` has drifted
/// from this contract.
#[test]
fn test_stable_reexports_at_crate_root() {
    // Each line forces a name resolution at the crate root. If the
    // re-export is removed, this test stops compiling.
    let _: Option<fn() -> CheckpointData> = None;
    let _: Option<fn() -> SessionConfig> = None;
    let _: Option<fn() -> SessionFilter> = None;
    let _: Option<fn() -> SessionInfo> = None;
    let _: Option<fn() -> SessionMetadata> = None;
    let _: Option<fn() -> SessionState> = None;
    let _: Option<fn() -> SessionStateMachine> = None;
    let _: Option<fn() -> StateEnterEffect> = None;
    let _: Option<fn() -> StateMachineError> = None;
    let _: Option<fn() -> Store> = None;
    let _: Option<fn() -> TokenBudget> = None;
    let _: Option<fn() -> TokenBudgetMonitor> = None;
    let _: Option<fn() -> TokenBudgetStatus> = None;
}

/// Verify that the re-export policy is documented in `src/lib.rs` so
/// future contributors see it. This is a sanity test on the source
/// text, not the public API, but it's cheap to run.
#[test]
fn test_lib_rs_documents_policy() {
    let lib_rs = include_str!("../src/lib.rs");
    // The policy header MUST be present.
    assert!(
        lib_rs.contains("Re-export policy (synthia-session)"),
        "src/lib.rs is missing the 'Re-export policy' section header"
    );
    // The three known-conflict names MUST be called out in the doc.
    assert!(
        lib_rs.contains("SessionManager")
            && lib_rs.contains("SessionError")
            && lib_rs.contains("Session"),
        "src/lib.rs policy section must mention Session, SessionManager, and SessionError"
    );
    // The historical offender line MUST NOT be present in active code
    // (it's only present as a commented-out fixture inside a doc test).
    let active_pub_use_session_block: Vec<&str> = lib_rs
        .lines()
        .filter(|l| {
            let trimmed = l.trim_start();
            !trimmed.starts_with("//")
                && !trimmed.starts_with("///")
                && !trimmed.starts_with("//!")
                && trimmed.contains("pub use session::")
        })
        .collect();
    assert!(
        active_pub_use_session_block.is_empty(),
        "Found active `pub use session::` line(s) in src/lib.rs: {:?}. \
         These create the historical name-shadowing trap. See the \
         re-export policy at the top of src/lib.rs for why this is forbidden.",
        active_pub_use_session_block
    );
}
