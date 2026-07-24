//! State-mutating actions — `enable_skill`, `disable_skill`,
//! `install_skill`, `uninstall_skill`.
//!
//! All four actions return `Result<CommandResult, Error>` so
//! they can be called from [`super::dispatch`]'s `match` arm;
//! failures are converted to user-facing messages (never
//! surfaced as errors — the command handler should not fail at
//! runtime for "skill not found" or "no installer configured").
//!
//! The enable/disable actions are safe to call without a
//! registry (degrade to a no-op confirmation). Install/uninstall
//! require an installer; without one they print a clear
//! "no installer configured" message.

use std::path::Path;

use synthia_core::Error;

use super::construct::SkillCommand;
use crate::types::CommandResult;

impl SkillCommand {
    pub(super) fn enable_skill(
        &self,
        skill_name: Option<&str>,
    ) -> Result<CommandResult, Error> {
        match skill_name {
            Some(name) => {
                if let Some(registry) = &self.registry {
                    if registry.enable(name) {
                        Ok(CommandResult::new(format!(
                            "Skill '{}' enabled.",
                            name
                        )))
                    } else {
                        Ok(CommandResult::new(format!(
                            "Skill '{}' not found or already enabled.",
                            name
                        )))
                    }
                } else {
                    Ok(CommandResult::new(format!("Skill '{}' enabled.", name)))
                }
            }
            None => Ok(CommandResult::new("Usage: /skill enable <skill_name>")),
        }
    }

    pub(super) fn disable_skill(
        &self,
        skill_name: Option<&str>,
    ) -> Result<CommandResult, Error> {
        match skill_name {
            Some(name) => {
                if let Some(registry) = &self.registry {
                    if registry.disable(name) {
                        Ok(CommandResult::new(format!(
                            "Skill '{}' disabled.",
                            name
                        )))
                    } else {
                        Ok(CommandResult::new(format!(
                            "Skill '{}' not found or already disabled.",
                            name
                        )))
                    }
                } else {
                    Ok(CommandResult::new(format!(
                        "Skill '{}' disabled.",
                        name
                    )))
                }
            }
            None => {
                Ok(CommandResult::new("Usage: /skill disable <skill_name>"))
            }
        }
    }

    pub(super) fn install_skill(
        &self,
        path: &str,
    ) -> Result<CommandResult, Error> {
        match &self.installer {
            Some(installer) => {
                let archive_path = Path::new(path);
                if !archive_path.exists() {
                    return Ok(CommandResult::new(format!(
                        "File not found: {}",
                        path
                    )));
                }

                match installer.install(archive_path, None) {
                    Ok(skill_name) => Ok(CommandResult::new(format!(
                        "Skill '{}' installed successfully from '{}'.",
                        skill_name, path
                    ))),
                    Err(e) => Ok(CommandResult::new(format!(
                        "Failed to install skill from '{}': {}",
                        path, e
                    ))),
                }
            }
            None => Ok(CommandResult::new(format!(
                "No installer configured. Cannot install from '{}'.",
                path
            ))),
        }
    }

    pub(super) fn uninstall_skill(
        &self,
        name: &str,
    ) -> Result<CommandResult, Error> {
        match &self.installer {
            Some(installer) => match installer.uninstall(name) {
                Ok(()) => Ok(CommandResult::new(format!(
                    "Skill '{}' uninstalled successfully.",
                    name
                ))),
                Err(e) => Ok(CommandResult::new(format!(
                    "Failed to uninstall skill '{}': {}",
                    name, e
                ))),
            },
            None => Ok(CommandResult::new(format!(
                "No installer configured. Cannot uninstall '{}'.",
                name
            ))),
        }
    }
}
