//! The [`BashTool`] struct + 2 constructors (`new` +
//! chainable `with_*` configurators) + the
//! [`BashTool::command_manager`] accessor.
//!
//! [`BashTool`] is the public type defined here; its
//! `Tool` trait impl is in [`super::trait_impl`] and its
//! low-level command executor is in
//! [`super::executor`].

use std::{path::PathBuf, sync::Arc};

use crate::{
    command_blacklist::CommandBlacklist,
    command_manager::CommandManager,
};

/// The bash execution tool. Holds a [`CommandBlacklist`]
/// (defense-in-depth gate), timeout bounds, output caps,
/// and a [`CommandManager`] for background processes.
pub struct BashTool {
    pub(super) sandbox: CommandBlacklist,
    pub(super) default_timeout_secs: u64,
    pub(super) max_timeout_secs: u64,
    pub(super) max_output_length: usize,
    pub(super) output_dir: PathBuf,
    pub(super) command_manager: Arc<CommandManager>,
}

impl BashTool {
    /// Construct a BashTool with the given blacklist and command
    /// manager. The blacklist is consulted as a defense-in-depth
    /// second gate after the registry's `PermissionChecker` has
    /// already allowed the call.
    pub fn new(
        command_manager: Arc<CommandManager>,
        sandbox: CommandBlacklist,
    ) -> Self {
        Self {
            sandbox,
            default_timeout_secs: 120,
            max_timeout_secs: 600,
            max_output_length: 30_000,
            output_dir: PathBuf::from("/tmp"),
            command_manager,
        }
    }

    pub fn with_default_timeout(mut self, secs: u64) -> Self {
        self.default_timeout_secs = secs;
        self
    }

    pub fn with_max_timeout(mut self, secs: u64) -> Self {
        self.max_timeout_secs = secs;
        self
    }

    pub fn with_max_output_length(mut self, len: usize) -> Self {
        self.max_output_length = len;
        self
    }

    pub fn with_output_dir(mut self, dir: PathBuf) -> Self {
        self.output_dir = dir;
        self
    }

    pub fn command_manager(&self) -> Arc<CommandManager> {
        self.command_manager.clone()
    }
}
