//! Free-form print helpers used by the REPL loop.
//!
//! [`print_cli_error`] prefixes an error message with the
//! CLI-error style (red, stderr) and is called from
//! [`super::execute`], [`super::construct`], and
//! [`super::agent_message`]. [`print_help`] is the long
//! `/help` text shown by the [`super::types::CommandAction::Help`]
//! arm of the dispatch loop.

use crossterm::style::Stylize;

pub(crate) fn print_cli_error(message: String) {
    eprintln!("{}", message.red());
}

pub(crate) fn print_help() {
    println!(
        r#"
Available commands:
  /exit, /quit, /q  Exit the REPL
  /help, /h         Show this help message
  /clear            Clear the screen
  /mode [MODE]      View or switch agent mode (interactive/plan/execute/review)
  /status           Show session status (iterations, tools, tokens, memory)
  /compact          Trigger context compaction
  /model [NAME]     View or switch the active model
  /provider [NAME]  View or switch the active provider
  /session [new]    View session info or create a new session
  /session list     List all persisted sessions
  /session switch <id>  Switch to a different session
  /session delete <id>  Delete a persisted session
  /tools            List registered tools
  /memory           Memory management
  /memory list      List all hot memory entries
  /memory read <key> Read a specific hot memory key
  /memory set <key>=<value> Set a hot memory value
  /skills           List loaded skills
  /config show      Display current configuration as a formatted table
  /config reload    Trigger config hot-reload from disk
  /task list        Display tasks from task dispatcher
  /skill report     Diagnostic report of all skills with metadata
  /skill stats      Aggregate skill system statistics

Any other input is sent to the agent for processing.
"#
    );
}
