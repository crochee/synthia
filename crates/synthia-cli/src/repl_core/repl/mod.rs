//! REPL — interactive command-line interface.
//!
//! Submodule layout:
//!
//! - [`types`]: the data definitions
//!   ([`SessionState`], [`ReplConfig`], [`Repl`], [`CommandAction`],
//!   [`ReplContext`]) plus their trivial `Default` impls.
//! - [`state`]: the [`SessionState`] methods
//!   (`new`, `prompt`, `update`).
//! - [`construct`]: the [`Repl`] and [`ReplContext`] constructors
//!   ([`Repl::new`], [`ReplContext::new`], `load_skill_summaries`)
//!   and the free [`run`]/[`run_with_context`] entry points.
//!   [`current_user_id`] also lives here because it is a constructor
//!   for the per-machine identity.
//! - [`run_loop`]: the main REPL read-eval-print loop
//!   ([`Repl::run`], [`Repl::run_with_context`]) and
//!   [`Repl::print_status`].
//! - [`handle_command`]: the slash-command parser
//!   ([`Repl::handle_command`]).
//! - [`format_event`]: event formatting and stream rendering
//!   ([`Repl::format_event`], [`Repl::render_event_stream`]).
//! - [`execute`]: slash-command execution
//!   ([`Repl::execute_command`]).
//! - [`agent_message`]: non-slash input → agent dispatch
//!   ([`Repl::handle_agent_message`]).
//! - [`print`]: the [`print_cli_error`] and [`print_help`] free
//!   helpers used by the loop.
//! - [`syntax`]: code-fence detection and basic syntax highlighting
//!   (already extracted; re-exported from the module root).
//!
//! Unit tests live in [`tests`].

mod agent_message;
mod construct;
mod execute;
mod format_event;
mod handle_command;
mod print;
mod run_loop;
mod state;
mod syntax;
mod types;

#[cfg(test)]
mod tests;

pub use construct::{run, run_with_context};
pub use syntax::{format_with_syntax_highlighting, highlight_rust_code};
pub use types::{CommandAction, Repl, ReplConfig, ReplContext, SessionState};
