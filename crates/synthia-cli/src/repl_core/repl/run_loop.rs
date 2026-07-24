//! Main REPL read-eval-print loop.
//!
//! [`Repl::run`] is a thin wrapper that builds a fresh
//! [`super::types::ReplContext`]; [`Repl::run_with_context`]
//! is the real loop. [`Repl::print_status`] lives here
//! because it is only ever called from the dispatch match
//! in `run_with_context`.

use std::io::{self, Write};

use rustyline::DefaultEditor;

use super::types::{CommandAction, Repl, ReplContext};
use crate::commands::AgentMode;

impl Repl {
    /// Run the main REPL loop: read input, parse, dispatch commands, format agent events.
    pub async fn run(
        &mut self,
        workspace: &crate::workspace::WorkspaceInfo,
    ) -> anyhow::Result<()> {
        let mut ctx = ReplContext::new(self.workspace_root.clone()).await;
        self.run_with_context(workspace, &mut ctx).await?;
        Ok(())
    }

    /// Run the REPL loop with an existing context (for testing or advanced use).
    pub async fn run_with_context(
        &mut self,
        workspace: &crate::workspace::WorkspaceInfo,
        ctx: &mut ReplContext,
    ) -> anyhow::Result<()> {
        let mut rl = DefaultEditor::new()?;

        let history_path = workspace.root.join(".agents/.cli_history");
        if history_path.exists() {
            let _ = rl.load_history(&history_path);
        }

        if ctx.provider.is_some() {
            println!(
                "Provider: {} | Model: {}",
                ctx.current_provider_name, ctx.current_model
            );
        }

        loop {
            let prompt = self.state.read().prompt();
            let readline = rl.readline(&prompt);

            let input = match readline {
                Ok(line) => line,
                Err(_) => {
                    println!();
                    break;
                }
            };

            if !input.trim().is_empty() {
                rl.add_history_entry(&input)?;
            }

            // Route slash commands through handle_command
            match self.handle_command(&input) {
                CommandAction::Quit => {
                    println!("Goodbye!");
                    break;
                }
                CommandAction::Clear => {
                    print!("\x1B[2J\x1B[1;1H");
                    io::stdout().flush()?;
                }
                CommandAction::Help => {
                    super::print::print_help();
                }
                CommandAction::Execute(cmd) => {
                    self.execute_command(cmd, ctx).await;
                }
                CommandAction::AgentMessage(msg) => {
                    self.handle_agent_message(&msg, ctx).await;
                }
                CommandAction::Empty => {
                    continue;
                }
                CommandAction::Status => {
                    self.print_status(ctx);
                }
                CommandAction::Compact => {
                    // Trigger context compaction (Task 10.10)
                    println!(
                        "Context compaction triggered. Session will be compacted on next LLM call."
                    );
                }
                CommandAction::Mode(mode) => {
                    // Handle mode command (Task 10.8)
                    match mode {
                        Some(m) => {
                            if let Ok(agent_mode) = m.parse::<AgentMode>() {
                                let mut state = self.state.write();
                                state.mode = agent_mode;
                                println!(
                                    "{}",
                                    state.theme.format_success(&format!(
                                        "Mode switched to: {}",
                                        agent_mode
                                    ))
                                );
                            } else {
                                println!(
                                    "{}",
                                    self.state.read().theme.format_error(&format!(
                                        "Unknown mode: '{}'. Valid modes: interactive, plan, execute, review",
                                        m
                                    ))
                                );
                            }
                        }
                        None => {
                            let state = self.state.read();
                            println!(
                                "Current mode: {}",
                                state
                                    .theme
                                    .format_prompt(&state.mode.to_string())
                            );
                        }
                    }
                }
                CommandAction::MemoryDisplay(value) => {
                    // Handle memory get/set (Task 10.12)
                    if let Some(ref hot) = ctx.hot_memory {
                        match &value {
                            Some(val) => {
                                if let Err(e) =
                                    hot.write("user_value", val).await
                                {
                                    println!(
                                        "{}",
                                        self.state.read().theme.format_error(
                                            &format!(
                                                "Error setting memory: {}",
                                                e
                                            )
                                        )
                                    );
                                } else {
                                    println!(
                                        "{}",
                                        self.state
                                            .read()
                                            .theme
                                            .format_success("Memory updated.")
                                    );
                                }
                            }
                            None => match hot.read("user_value").await {
                                Ok(Some(content)) => {
                                    println!("=== user_value ===");
                                    println!("{}", content);
                                }
                                Ok(None) => {
                                    println!("No value stored for user_value.")
                                }
                                Err(e) => {
                                    println!("Error reading memory: {}", e)
                                }
                            },
                        }
                    } else {
                        println!("HotMemory not initialized.");
                    }
                }
            }
        }

        let history_path = workspace.root.join(".agents/.cli_history");
        let _ = rl.save_history(&history_path);

        Ok(())
    }

    /// Print extended status information (Task 10.9).
    pub(super) fn print_status(&self, ctx: &ReplContext) {
        let state = self.state.read();
        let theme = &state.theme;
        println!("{}", theme.format_prompt("=== Session Status ==="));
        println!("Session ID: {}", ctx.session_id);
        println!("Mode: {}", state.mode);
        println!(
            "Iterations: {}",
            theme.format_text(&state.iteration_count.to_string())
        );
        println!(
            "Tool calls: {}",
            theme.format_text(&state.tool_call_count.to_string())
        );
        println!(
            "Token usage: {} prompt + {} completion = {} total",
            theme.format_text(&state.token_usage.prompt_tokens.to_string()),
            theme.format_text(&state.token_usage.completion_tokens.to_string()),
            theme.format_text(&state.token_usage.total_tokens.to_string()),
        );

        // Memory entries count
        if ctx.hot_memory.is_some() {
            println!("Memory: HotMemory initialized");
        } else {
            println!("Memory: Not initialized");
        }
    }
}
