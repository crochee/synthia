//! Deprecated re-exports of types moved to `synthia-session-v2`.
//!
//! Retained until 0.3.0 for backward compatibility.

#[deprecated(
    since = "0.2.0",
    note = "use synthia_session_v2::CURRENT_SESSION_VERSION"
)]
pub use synthia_session_v2::CURRENT_SESSION_VERSION as LEGACY_SESSION_VERSION;
#[deprecated(since = "0.2.0", note = "use synthia_session_v2::Message")]
pub use synthia_session_v2::Message as LegacyMessage;
#[deprecated(since = "0.2.0", note = "use synthia_session_v2::Part")]
pub use synthia_session_v2::Part as LegacyPart;
#[deprecated(
    since = "0.2.0",
    note = "use synthia_session_v2::SessionEntry"
)]
pub use synthia_session_v2::SessionEntry as LegacySessionEntry;
#[deprecated(
    since = "0.2.0",
    note = "use synthia_session_v2::SessionTree"
)]
pub use synthia_session_v2::SessionTree as LegacySessionTree;
#[deprecated(since = "0.2.0", note = "use synthia_session_v2::ToolPart")]
pub use synthia_session_v2::ToolPart as LegacyToolPart;
