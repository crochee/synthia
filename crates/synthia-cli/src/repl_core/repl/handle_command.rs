//! Slash-command parser.
//!
//! [`Repl::handle_command`] is the only function in this module.
//! It maps a raw input line to a [`CommandAction`] that the
//! `run_with_context` dispatch loop can switch on. Slash commands
//! (starting with `/`) are routed to the [`crate::commands::CliCommand`]
//! parser; everything else is forwarded to the agent.

use super::types::{CommandAction, Repl};
use crate::commands::CliCommand;

impl Repl {
    /// Handle a slash command by detecting if input starts with `/` and routing appropriately.
    ///
    /// Returns `CommandAction::AgentMessage` for non-slash input (to be sent to the agent),
    /// or a specific command action for slash commands.
    pub fn handle_command(&self, input: &str) -> CommandAction {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return CommandAction::Empty;
        }

        // If input does NOT start with `/`, it should be sent to the agent
        if !trimmed.starts_with('/') {
            return CommandAction::AgentMessage(trimmed.to_string());
        }

        // Parse the slash command and route it
        match CliCommand::parse(input) {
            CliCommand::Exit | CliCommand::Quit => CommandAction::Quit,
            CliCommand::Help => CommandAction::Help,
            CliCommand::Clear => CommandAction::Clear,
            CliCommand::Mode(arg) => CommandAction::Mode(arg),
            CliCommand::Status => CommandAction::Status,
            CliCommand::Compact => CommandAction::Compact,
            CliCommand::Model(arg) => {
                CommandAction::Execute(CliCommand::Model(arg))
            }
            CliCommand::Provider(arg) => {
                CommandAction::Execute(CliCommand::Provider(arg))
            }
            CliCommand::Session(arg) => {
                CommandAction::Execute(CliCommand::Session(arg))
            }
            CliCommand::SessionList => {
                CommandAction::Execute(CliCommand::SessionList)
            }
            CliCommand::SessionSwitch(id) => {
                CommandAction::Execute(CliCommand::SessionSwitch(id))
            }
            CliCommand::SessionDelete(id) => {
                CommandAction::Execute(CliCommand::SessionDelete(id))
            }
            CliCommand::Tools => CommandAction::Execute(CliCommand::Tools),
            CliCommand::Memory(arg) => {
                CommandAction::Execute(CliCommand::Memory(arg))
            }
            CliCommand::Skills => CommandAction::Execute(CliCommand::Skills),
            CliCommand::ConfigShow => {
                CommandAction::Execute(CliCommand::ConfigShow)
            }
            CliCommand::ConfigReload => {
                CommandAction::Execute(CliCommand::ConfigReload)
            }
            CliCommand::TaskList => {
                CommandAction::Execute(CliCommand::TaskList)
            }
            CliCommand::SkillReport => {
                CommandAction::Execute(CliCommand::SkillReport)
            }
            CliCommand::SkillStats => {
                CommandAction::Execute(CliCommand::SkillStats)
            }
            CliCommand::Message(msg) => {
                if msg.is_empty() {
                    CommandAction::Empty
                } else {
                    CommandAction::AgentMessage(msg)
                }
            }
            CliCommand::Unknown(cmd) => {
                println!(
                    "Unknown command: /{}. Type /help for available commands.",
                    cmd
                );
                CommandAction::Empty
            }
        }
    }
}
