//! `CommandHandler` impl — args parsing and action routing.
//!
//! This is the entry point. It splits the raw `args` string on
//! whitespace, picks the action verb (the first token), and
//! forwards the rest to the matching `super::view` /
//! `super::validate` / `super::stats` / `super::report` /
//! `super::lifecycle` method. Empty input prints the usage
//! banner; unknown verbs print the available-verb list.

use async_trait::async_trait;
use synthia_core::Error;

use super::construct::SkillCommand;
use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

#[async_trait]
impl CommandHandler for SkillCommand {
    fn name(&self) -> &str {
        "skill"
    }

    async fn execute(
        &self,
        args: &str,
        _ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        let args = args.trim();
        if args.is_empty() {
            return Ok(CommandResult::new(
                "Usage: /skill <list|info|validate|stats|report|enable|disable|install|uninstall> [options]\n\
                 /skill list [--json]              - List all available skills\n\
                 /skill info <name> [--json]       - Show skill metadata and scripts\n\
                 /skill validate <path>            - Validate a SKILL.md file\n\
                 /skill stats [--json]             - Show global skill statistics\n\
                 /skill report <name> [--json]     - Show skill usage report\n\
                 /skill enable <name>              - Enable a skill\n\
                 /skill disable <name>             - Disable a skill\n\
                 /skill install <path>             - Install a skill from .skill ZIP package\n\
                 /skill uninstall <name>           - Uninstall a skill",
            ));
        }

        let parts: Vec<&str> = args.split_whitespace().collect();
        let action = parts[0];

        match action {
            "list" => {
                let json_mode = parts.contains(&"--json");
                self.list_skills(json_mode)
            }
            "info" => {
                let json_mode = parts.contains(&"--json");
                let name = parts
                    .iter()
                    .skip(1)
                    .find(|p| *p != &"--json")
                    .map(|s| s.to_string());
                match name {
                    Some(n) => self.info_skill(&n, json_mode),
                    None => Ok(CommandResult::new(
                        "Usage: /skill info <name> [--json]",
                    )),
                }
            }
            "validate" => {
                let path_parts: Vec<&&str> = parts.iter().skip(1).collect();
                if path_parts.is_empty() {
                    return Ok(CommandResult::new(
                        "Usage: /skill validate <path>",
                    ));
                }
                let path = path_parts
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                self.validate_skill(&path)
            }
            "stats" => {
                let json_mode = parts.contains(&"--json");
                self.global_stats(json_mode)
            }
            "report" => {
                let json_mode = parts.contains(&"--json");
                let name = parts
                    .iter()
                    .skip(1)
                    .find(|p| *p != &"--json")
                    .map(|s| s.to_string());
                match name {
                    Some(n) => self.skill_report(&n, json_mode),
                    None => Ok(CommandResult::new(
                        "Usage: /skill report <name> [--json]",
                    )),
                }
            }
            "enable" => self.enable_skill(parts.get(1).copied()),
            "disable" => self.disable_skill(parts.get(1).copied()),
            "install" => {
                let path = parts
                    .iter()
                    .skip(1)
                    .find(|p| *p != &"--json")
                    .map(|s| s.to_string());
                match path {
                    Some(p) => self.install_skill(&p),
                    None => Ok(CommandResult::new(
                        "Usage: /skill install <path-to-zip>",
                    )),
                }
            }
            "uninstall" => {
                let name = parts
                    .iter()
                    .skip(1)
                    .find(|p| *p != &"--json")
                    .map(|s| s.to_string());
                match name {
                    Some(n) => self.uninstall_skill(&n),
                    None => {
                        Ok(CommandResult::new("Usage: /skill uninstall <name>"))
                    }
                }
            }
            other => Ok(CommandResult::new(format!(
                "Unknown skill action: {}. Available: list, info, validate, stats, report, enable, disable, install, uninstall",
                other
            ))),
        }
    }
}
