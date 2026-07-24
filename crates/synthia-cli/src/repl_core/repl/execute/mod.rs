//! Slash-command execution.
//!
//! [`Repl::execute_command`] is the single dispatch table for
//! [`super::types::CommandAction::Execute`]. Each arm prints
//! directly to stdout (the REPL is line-buffered) and mutates
//! the [`super::types::ReplContext`] in place (model/provider
//! switches, session id, config reload, etc.).
//!
//! The dispatch arms are split into focused submodules:
//! - [`model`]: `/model` command
//! - [`provider`]: `/provider` command
//! - [`session`]: `/session` commands
//! - [`tools`]: `/tools` command
//! - [`memory`]: `/memory` commands
//! - [`skills`]: `/skills`, `/skill-report`, `/skill-stats` commands
//! - [`config`]: `/config`, `/config-reload`, `/task-list` commands

mod config;
mod memory;
mod model;
mod provider;
mod session;
mod skills;
mod tools;

use synthia_core::generate_session_id;

use super::types::{Repl, ReplContext};
use crate::commands::CliCommand;

impl Repl {
    /// Execute a parsed CLI command against the REPL context.
    pub(super) async fn execute_command(
        &self,
        cmd: CliCommand,
        ctx: &mut ReplContext,
    ) {
        match cmd {
            CliCommand::Model(model) => model::handle_model(model, ctx),
            CliCommand::Provider(name) => provider::handle_provider(name, ctx),
            CliCommand::Session(None) => {
                println!("Session: {} (messages: 0)", ctx.session_id);
            }
            CliCommand::Session(Some(ref arg)) if arg == "new" => {
                ctx.session_id = generate_session_id();
                println!("New session: {}", ctx.session_id);
            }
            CliCommand::SessionList => session::handle_session_list(ctx),
            CliCommand::SessionSwitch(id) => {
                session::handle_session_switch(ctx, id)
            }
            CliCommand::SessionDelete(id) => {
                session::handle_session_delete(ctx, id)
            }
            CliCommand::Session(Some(arg)) => {
                println!(
                    "Unknown session command: {}. Use 'new' to create a new session, 'list' to view all, 'switch <id>' to switch, 'delete <id>' to remove.",
                    arg
                );
            }
            CliCommand::Tools => tools::handle_tools().await,
            CliCommand::Memory(None) => memory::handle_memory_show(ctx).await,
            CliCommand::Memory(Some(ref sub)) if sub == "list" => {
                memory::handle_memory_list(ctx).await;
            }
            CliCommand::Memory(Some(ref sub)) if sub.starts_with("read ") => {
                let key = sub[5..].trim();
                memory::handle_memory_read(ctx, key).await;
            }
            CliCommand::Memory(Some(ref sub)) if sub.starts_with("set ") => {
                memory::handle_memory_set(ctx, &sub[4..]).await;
            }
            CliCommand::Memory(Some(sub)) => {
                println!(
                    "Unknown memory subcommand: {}. Use '/memory', '/memory list', '/memory read <key>', or '/memory set <key>=<value>'.",
                    sub
                );
            }
            CliCommand::Skills => skills::handle_skills(),
            CliCommand::ConfigShow => config::handle_config_show(ctx),
            CliCommand::ConfigReload => config::handle_config_reload(ctx),
            CliCommand::TaskList => config::handle_task_list(),
            CliCommand::SkillReport => skills::handle_skill_report(ctx),
            CliCommand::SkillStats => skills::handle_skill_stats(ctx),
            _ => {}
        }
    }
}
