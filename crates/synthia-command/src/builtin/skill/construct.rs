//! [`SkillCommand`] struct + builders.
//!
//! The fields are `pub(super)` so the action submodules
//! ([`super::view`], [`super::validate`], [`super::stats`],
//! [`super::report`], [`super::lifecycle`]) can read them
//! directly without expanding the public surface.

use std::sync::Arc;

use synthia_skill::{
    SkillRegistry,
    installer::SkillInstaller,
    usage::SkillUsageTracker,
};

/// `/skill` command handler.
///
/// Holds optional references to the skill registry, installer,
/// and usage tracker. Every action gracefully degrades when its
/// dependency is `None` (e.g. `list` with no registry prints
/// "no skills loaded"; `install` without an installer prints
/// "No installer configured").
pub struct SkillCommand {
    pub(super) registry: Option<Arc<SkillRegistry>>,
    pub(super) installer: Option<Arc<SkillInstaller>>,
    pub(super) usage_tracker: Option<SkillUsageTracker>,
}

impl SkillCommand {
    pub fn new() -> Self {
        Self {
            registry: None,
            installer: None,
            usage_tracker: None,
        }
    }

    pub fn with_registry(registry: Arc<SkillRegistry>) -> Self {
        Self {
            registry: Some(registry),
            installer: None,
            usage_tracker: None,
        }
    }

    pub fn with_usage_tracker(mut self, tracker: SkillUsageTracker) -> Self {
        self.usage_tracker = Some(tracker);
        self
    }

    pub fn with_installer(mut self, installer: Arc<SkillInstaller>) -> Self {
        self.installer = Some(installer);
        self
    }
}

impl Default for SkillCommand {
    fn default() -> Self {
        Self::new()
    }
}
