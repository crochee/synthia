//! The [`ApplyPatchInput`] deserializer.
//!
//! The tool takes a single `patch: String` field that must start
//! with `*** Begin Patch` (V4A format). The actual parsing of the
//! V4A body into [`super::tool::ApplyPatchTool::call`] is done by
//! `v4a::parse_v4a` — this struct only validates the JSON
//! envelope.

use serde::Deserialize;

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ApplyPatchInput {
    /// The V4A patch text (must start with `*** Begin Patch`).
    pub patch: String,
}
