//! `synthia-tool-materialization` — tool identity, provenance, and
//! materialization types.
//!
//! PR-5.1 introduces `ToolId`, `ProviderId`, and `ToolVisibility`.
//! PR-5.2 adds `Materialization` struct.
//! PR-5.3 adds `ToolProvenance` enum.
//! PR-6.1 adds `OutputBound` trait + `DefaultOutputBound`.

pub mod id;
pub mod materialization;
pub mod output_bound;
pub mod provenance;
pub mod visibility;

pub use id::{ProviderId, ToolId};
pub use materialization::Materialization;
pub use output_bound::{
    BoundOutput,
    DefaultOutputBound,
    OutputBound,
    OutputBoundConfig,
};
pub use provenance::ToolProvenance;
pub use visibility::ToolVisibility;
