//! Session controller and supporting types.

pub mod controller;
pub mod lane;
pub mod subagent_router;

pub use lane::{DefaultLane, LaneError, SessionLane};

/// Prefix of the session-id namespace the Messages endpoint mints for
/// requests that name no session.
///
/// `POST /v1/messages` gives every unbound request — a Cursor, Claude
/// Code or bare-SDK turn, none of which sends
/// `metadata.synthia_session_id` — a session of its own, so the
/// client's history is the only history the run sees. The transcript
/// that produces is real and addressable, but it is not one of the
/// user's conversations, so the management listing
/// (`routes::sessions::list_sessions`) leaves the namespace out while
/// a direct `GET /api/v1/sessions/{id}` still serves it.
///
/// The trailing separator is part of the constant, which makes the
/// membership test exact: a session that merely begins with the same
/// letters (`anthropicx-1`) is outside the namespace and stays
/// listed.
pub(crate) const EPHEMERAL_SESSION_PREFIX: &str = "anthropic-";
