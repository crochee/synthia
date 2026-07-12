//! Wire protocol version.
//!
//! Bump when a `Submission` or `EventMsg` variant is added/removed/renamed.
//! Bumped to `2` for synthia v3 architecture (Message+Part+ToolState).

/// Current wire protocol version.
///
/// v1: legacy `Session` flat struct (no longer supported by `synthia-protocol`).
/// v2: `Submission` + `Op` + `EventMsg` + `W3cTraceContext` + `AskForApproval`.
pub const PROTOCOL_VERSION: u32 = 2;
