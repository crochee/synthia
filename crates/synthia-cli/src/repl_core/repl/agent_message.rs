//! Non-slash input → agent dispatch.
//!
//! [`Repl::handle_agent_message`] is the production call path
//! for ordinary user input. It wires the agent together with
//! the REPL context, installs a SIGINT handler that cancels the
//! running agent, and drains the event stream through
//! [`super::format_event::Repl::render_event_stream`].

use std::{sync::Arc, time::Duration};

use indicatif::ProgressBar;
#[allow(deprecated)]
use synthia_agent::{
    Agent,
    AgentConfig,
    AgentInput,
    AgentRunConfig,
    build_default_tool_registry,
    tools::orchestrator::build_default_tool_orchestrator,
};
use synthia_context::assembler::ContextAssembler;
use synthia_permission::{ApprovalStore, TerminalApprovalService};
use synthia_sandbox::composite::CompositeSandboxManager;
use synthia_session::store::Store as SessionStore;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::{
    construct::current_user_id,
    print::print_cli_error,
    types::{Repl, ReplContext},
};

impl Repl {
    /// Handle a message to be sent to the agent, including SIGINT cancellation support.
    pub(super) async fn handle_agent_message(
        &self,
        msg: &str,
        ctx: &ReplContext,
    ) {
        let agent_input = AgentInput::text(msg);
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let cancel_tx = Arc::new(std::sync::Mutex::new(Some(cancel_tx)));

        // Spawn SIGINT handler: on Ctrl+C, cancel the running agent
        let ctrlc_cancel = cancel_tx.clone();
        let ctrlc_handler = tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            if let Ok(mut guard) = ctrlc_cancel.lock()
                && let Some(tx) = guard.take()
            {
                let _ = tx.send(());
            }
        });

        if let Some(provider) = &ctx.provider {
            let config = AgentConfig {
                model: ctx.current_model.clone(),
                workspace_root: ctx.workspace_root.clone(),
                ..Default::default()
            };

            #[allow(deprecated)]
            let tool_registry = build_default_tool_registry(
                ctx.workspace_root.clone(),
                None,
                None,
            );
            let hook_registry = Arc::new(synthia_hook::HookRegistry::new());

            // Create model router
            let model_router =
                Arc::new(synthia_provider::router::ModelRouter::new());

            // Create context assembler
            let assembler = Arc::new(ContextAssembler::new(config.max_tokens));

            // Create session store
            let session_store_dir =
                ctx.workspace_root.join(".synthia").join("sessions");
            let session_store = SessionStore::new(session_store_dir);

            // Create cancellation token
            let cancel_token = CancellationToken::new();

            // Spawn task to cancel when cancel_rx fires
            let cancel_token_clone = cancel_token.clone();
            tokio::spawn(async move {
                let _ = cancel_rx.await;
                cancel_token_clone.cancel();
            });

            // Construct the tool orchestrator for this REPL session.
            let approval_service =
                Arc::new(TerminalApprovalService::new(ApprovalStore::new()));
            let sandbox_manager =
                Arc::new(CompositeSandboxManager::default_linux(
                    ctx.workspace_root.clone(),
                ));
            let (tool_orchestrator, _tool_resolver) =
                build_default_tool_orchestrator(
                    ctx.workspace_root.clone(),
                    approval_service.clone(),
                    sandbox_manager.clone(),
                );

            // Task 10.2: Show progress spinner for LLM API calls
            let provider_name = ctx.current_provider_name.clone();
            let spinner = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    indicatif::ProgressStyle::default_spinner()
                        .tick_strings(&[
                            "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
                        ])
                        .template("{spinner} {msg}")
                        .unwrap(),
                );
                pb.set_message(format!("Waiting for {}...", provider_name));
                pb.enable_steady_tick(Duration::from_millis(80));
                pb
            });

            let mut event_stream = Agent::run_stream(AgentRunConfig {
                provider: provider.clone(),
                tool_registry,
                hook_registry,
                model_router,
                user_id: current_user_id().unwrap_or_else(|e| {
                    print_cli_error(format!("[identity_error] {}", e));
                    // §1 invariant: `user_id` MUST be non-empty.
                    // Identity load failure is fatal at the REPL
                    // boundary; the agent will get a fatal error
                    // when it tries to persist.
                    String::new()
                }),
                session_id: ctx.session_id.clone(),
                input: agent_input,
                config,
                context_assembler: Some(assembler),
                session_store,
                steering_channel: None,
                cancel_token,
                memory_event_sender: None,
                agent_control: None,
                fork_policy: Default::default(),
                // No runtime L4 CompactionProvider wired in the REPL
                // yet; cascade falls through to L5 reset.
                compaction_provider: None,
                session_input_queue: None,
                subagent_session_factory: None,
                approval_service: Some(approval_service),
                sandbox_manager: Some(sandbox_manager),
                tool_orchestrator: Some(tool_orchestrator),
                guardian_coordinator: None,
                extension_manager: None,
            });

            // Task 10.3: Progress bar for file search operations
            let search_progress: Option<ProgressBar> = None;

            self.render_event_stream(&mut event_stream).await;

            // Stop spinner and search progress
            if let Ok(pb) = spinner.await {
                pb.finish_and_clear();
            }
            if let Some(pb) = search_progress {
                pb.finish_and_clear();
            }
        } else {
            println!(
                "[info] No LLM provider configured. Input received: \"{}\"",
                msg
            );
            println!(
                "[info] Configure a provider in .agents/config.toml or set OPENAI_API_KEY/ANTHROPIC_API_KEY."
            );
        }

        ctrlc_handler.abort();
    }
}
