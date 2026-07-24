//! Data carriers for the agent-file loader family.
//!
//! Two things live here:
//!
//! - [`AgentChangeEvent`]: the 3-variant enum
//!   (`Added` / `Modified` / `Removed`) the loader
//!   pushes onto its event queue, and that the
//!   `notify`-backed [`super::loader::AgentFileLoader::watch`]
//!   watcher sends back over an mpsc channel.
//!   Kept public at the top level so downstream
//!   consumers can pattern-match on the variants
//!   without going through a private wrapper.
//! - [`MAX_EXTENDS_DEPTH`]: the `extends` chain
//!   depth cap (4) consumed by
//!   [`super::extends::resolve_extends`]. Lives
//!   here (not in `extends.rs`) because it's the
//!   loader family's only global constant and
//!   keeping it with the data types avoids a
//!   2-file constant ping-pong.

/// Event emitted when an agent file changes.
///
/// Surfaced both by the explicit
/// [`super::loader::AgentFileLoader::reload`] /
/// [`super::loader::AgentFileLoader::detect_removals`]
/// methods (where the events are queued and
/// drained via
/// [`super::loader::AgentFileLoader::take_change_events`])
/// and by the background
/// [`super::loader::AgentFileLoader::watch`]
/// mpsc channel.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentChangeEvent {
    /// A previously-unseen id was first loaded.
    Added(String),
    /// A cached id's on-disk content changed.
    Modified(String),
    /// A cached id's file disappeared from disk
    /// (detected by `detect_removals`).
    Removed(String),
}

/// Max depth the `extends` chain resolution is
/// allowed to walk. See
/// [`super::extends::resolve_extends`] for the
/// cycle-detection semantics.
pub const MAX_EXTENDS_DEPTH: usize = 4;
