//! Tests for `ToolRegistry` and its `Registry`-trait integration.
//!
//! Split into focused sub-modules by concern:
//! - `support` — shared fixtures (`TestEntryTool`, `ShadowTool`,
//!   `NamedTool`) and the `collect_results` stream helper used
//!   by every dispatching sub-module.
//! - `exposure` — `ToolExposure` plumbing and the
//!   descriptor / snapshot projection.
//! - `session_scope` — `create_session_scope` token allocation,
//!   drop semantics, and registry-drop ordering.
//! - `registration` — register / unregister, descriptor and
//!   snapshot caches, the `Registry` trait surface, hidden
//!   gating, and dispatch through `run_stream`.
//! - `snapshot` — dual-index `snapshot()` ordering and
//!   materialisation.
//! - `dispatch` — streaming dispatch, truncation, contract
//!   violations, and `snapshot_with_provenance`.
//! - `mutations` — post-registration `set_exposure` /
//!   `set_hidden` mutations.
//! - `argument_validation` — the R74 JSON-Schema argument
//!   validation switch.
//!
//! The imports below mirror what the original monolithic
//! `mod tests` block carried: a `Tool7` alias for the
//! crate-public `Tool` trait (so dispatch tests can exercise
//! both `Tool` and `Tool7` impls side by side), the shared
//! `Context` / `ToolOutput` types. `support` is also
//! re-exported so every test sub-module can pick up
//! `TestEntryTool` / `ShadowTool` / `NamedTool` /
//! `collect_results` through a single `use super::*;`.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use synthia_core::registry::{Registry, RegistryItem};

use super::{
    super::{traits::Tool as Tool7, types::Context},
    *,
};
use crate::{traits::Tool, types::ToolOutput};

mod support;
pub(crate) use support::{
    NamedTool,
    ShadowTool,
    TestEntryTool,
    collect_results,
};

mod argument_validation;
mod dispatch;
mod exposure;
mod mutations;
mod registration;
mod session_scope;
mod snapshot;
