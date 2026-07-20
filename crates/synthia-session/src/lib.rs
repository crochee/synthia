pub mod error;
pub mod manager;
pub mod service;
pub mod session;
pub mod state_machine;
pub mod store;
pub mod token_budget;
pub mod types;

// =============================================================================
// Re-export policy (synthia-session)
// =============================================================================
//
// The crate has historically exported several types that exist in more than
// one module with the SAME name. The most painful collisions are:
//
//   * `Session`        -- `types::Session`  (state-machine model record)
//                         `session::Session` (legacy conversation record)
//   * `SessionManager` -- `manager::SessionManager`  (concrete impl, struct)
//                         ~~`session::SessionManager`  (abstraction, trait)~~
//                          REMOVED 2026-06-15 (Sub-task C of
//                          `2026-06-15-p0-trait-review-remediation`): 0 trait
//                          bound usage, 0 dyn dispatch, 1 real impl.
//   * `SessionError`   -- `session::SessionError`    (trait-layer error)
//                         (any future re-export from `types` would collide)
//
// Re-exporting these by name at the crate root is fundamentally
// name-shadowing fragile: an explicit `pub use session::Session`
// (or, historically, `pub use session::SessionManager`) silently overrides
// the earlier `pub use manager::*` / `pub use types::*` glob, so a consumer
// that does `use synthia_session::Session` may get either the
// struct or the trait depending on the order of `pub use` lines in this
// file. This has caused 40+ compile errors in the past (see 2026-06-13
// follow-up: "synthia-session has pre-existing dual Session re-export
// shadowing").
//
// To prevent future traps:
//   1. Glob re-exports from single-ownership modules are still allowed
//      (`pub use manager::*`, `pub use types::*`).
//   2. Multi-ownership types are NEVER re-exported by name. Consumers
//      must write the canonical path explicitly:
//          `synthia_session::types::Session`
//          `synthia_session::manager::SessionManager`
//          `synthia_session::session::SessionError`
//   3. The legacy `session::Session` record is intentionally NOT
//      promoted to the crate root -- if you need it, the qualified path
//      `synthia_session::session::Session` is unambiguous.
//   4. (Historical) `session::SessionManager` trait was REMOVED 2026-06-15.
//      Previously required the qualified path
//      `synthia_session::session::SessionManager`. After removal, only
//      the struct `synthia_session::manager::SessionManager` remains.
//
// If you ever need to add a new top-level re-export, first grep for the
// name across all `pub mod` blocks of this crate to confirm there is no
// collision. If there is, do NOT add the re-export; document the
// canonical path here instead.
//
// =============================================================================
// Policy enforcement (compile-fail doc tests below)
// =============================================================================
//
// The doc tests in this module are the FIRST line of defense against
// re-introducing a name-shadowing trap. If any of the `compile_fail` tests
// ever start passing, or the positive tests ever start failing, the
// re-export policy has been violated and the workspace is exposed to
// the same class of bug fixed on 2026-06-13.
//
// Layered defense:
//   1. compile_fail doc tests in this module  (runs on `cargo test --doc`)
//   2. integration test `tests/reexport_policy.rs` (runs on `cargo test`)
//   3. CI script `scripts/check_reexports.sh`  (runs in any CI environment)
//
// If you intentionally change the public API, update ALL THREE layers.

// ---- Forbidden: ambiguous crate-root re-exports (must NOT compile) ----

/// ```compile_fail
/// // FORBIDDEN: `SessionManager` is intentionally NOT re-exported at the
/// // crate root because it is both a trait (in `session`) and a struct
/// // (in `manager`). Use the qualified path instead.
/// use synthia_session::SessionManager;
/// fn _shadowing_trap() -> Box<dyn synthia_session::SessionManager> {
///     unreachable!()
/// }
/// ```
#[allow(dead_code)]
fn _doc_session_manager_forbidden() {}

/// ```compile_fail
/// // FORBIDDEN: `SessionError` is intentionally NOT re-exported at the
/// // crate root. Use `synthia_session::session::SessionError` instead.
/// use synthia_session::SessionError;
/// fn _shadowing_trap(e: synthia_session::SessionError) -> String {
///     format!("{:?}", e)
/// }
/// ```
#[allow(dead_code)]
fn _doc_session_error_forbidden() {}

/// NOTE: `use synthia_session::Session;` is NOT marked `compile_fail`
/// because `Session` IS legitimately accessible at the crate root via
/// the `pub use types::*` glob. The collision concern is between the
/// legacy `session::Session` (conversation record) and the canonical
/// `types::Session` (state-machine model). The current policy keeps
/// `types::Session` at the root and leaves the legacy one qualified
/// under `synthia_session::session::Session`. See
/// `_doc_legacy_session_canonical` below for the qualified-path
/// contract.
/// ---- compile_fail: historical offender MUST NOT compile ----
/// ```compile_fail
/// // FORBIDDEN: the re-export `pub use session::{Session, SessionError,
/// // SessionManager}` was the original source of the bug. Do NOT add it
/// // back. The line below is the historical offender, kept here as a
/// // negative test fixture.
/// //
/// // pub use session::{Session, SessionError, SessionManager};
/// use synthia_session::SessionManager as _Alias;
/// ```
#[allow(dead_code)]
fn _doc_historical_offender() {}

/// ```
/// // CANONICAL (legacy): the legacy conversation record is reachable via
/// // the qualified `synthia_session::session::Session` path. It is NOT
/// // promoted to the crate root because it would shadow
/// // `synthia_session::types::Session` (re-exported via `types::*`).
/// use synthia_session::session::Session as LegacySessionRecord;
/// let _: fn() -> LegacySessionRecord = || unreachable!();
/// ```
#[allow(dead_code)]
fn _doc_legacy_session_canonical() {}

// ---- Positive: canonical paths MUST compile and resolve ----

/// ```
/// // CANONICAL: the qualified paths are the only stable entry points.
/// use synthia_session::{
///     manager::SessionManager,
///     session::{Session as LegacySession, SessionError},
///     state_machine::SessionStateMachine,
///     store::Store,
///     types::{Session, SessionConfig, SessionState},
/// };
///
/// fn _canonical_paths() {
///     let _: Option<SessionManager> = None;
///     let _: Option<LegacySession> = None;
///     let _: Option<Session> = None;
///     let _: Option<SessionConfig> = None;
///     let _: Option<SessionState> = None;
///     let _: Option<Store> = None;
///     let _: Option<SessionStateMachine> = None;
///     let _: Option<SessionError> = None;
/// }
/// ```
#[allow(dead_code)]
fn _doc_canonical_paths() {}

// ---- Positive: stable re-exports at the crate root MUST still work ----

/// ```
/// // Stable re-exports (single-ownership modules only).
/// use synthia_session::{
///     CheckpointData,          // from store
///     SessionFilter,           // from manager
///     SessionInfo,             // from manager
///     SessionMetadata,         // from store
///     SessionStateMachine,     // from state_machine
///     StateEnterEffect,        // from state_machine
///     StateMachineError,       // from state_machine
///     Store,                   // from store
///     TokenBudget,             // from types (via glob)
///     TokenBudgetMonitor,      // from token_budget
///     TokenBudgetStatus,       // from types (via glob)
///     SessionSummary,          // from manager
///     // NOTE: Session, SessionConfig, SessionState are also re-exported
///     // from `types::*` and are valid at the crate root. There is no
///     // shadowing risk because `types` is the unique owner of those names.
///     //
///     // NOTE 2026-06-15: `PersistenceService` trait was REMOVED in change
///     // `2026-06-15-p2-trait-cleanup`. The concrete `Store` remains at
///     // the crate root, and helpers `metadata_to_session` / `load_session`
///     // are reachable via the qualified `synthia_session::service::*`
///     // path.
/// };
/// ```
#[allow(dead_code)]
fn _doc_stable_reexports() {}

pub use manager::{SessionFilter, SessionInfo, SessionSummary};
pub use service::{load_session, metadata_to_session};
pub use state_machine::{
    SessionStateMachine,
    StateEnterEffect,
    StateMachineError,
};
pub use store::{CheckpointData, SessionMetadata, Store};
pub use token_budget::TokenBudgetMonitor;
pub use types::*;

// R3 (synthia-session-v2) re-exports — additive on top of the legacy surface.
// New code should reach for these directly; legacy re-exports stay
// until 0.3.0 so external crates that still import `synthia_session::Store`
// etc. continue to compile.
pub mod migration;
pub use migration::*;
pub use synthia_session_v2::*;
