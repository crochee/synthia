//! Interactive mode for CLI
//!
//! Provides an interactive chat interface with the AI agent.

use std::{path::Path, sync::Arc};

use anyhow::Result;
use synthia_agent::{Agent, config::SessionConfig, tools::QuestionSenderImpl};
use tokio::{signal, sync::mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    agent::AgentSetup,
    color::print_banner,
    handler::MainLoopHandler,
    input::InputHandler,
    output::{self, print_tools},
    scheduler,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run the CLI in interactive mode
pub async fn run(
    _config: &crate::config::AppConfig,
    current_dir: &Path,
    agent: Agent,
    setup: &AgentSetup,
) -> Result<()> {
    print_banner("Synthia CLI ", VERSION);
    print_tools(&setup.tool_names());
    println!();
    output::print_help();

    // Session is created lazily on first user input to avoid empty sessions
    let session_config: Option<SessionConfig> = None;

    run_main_loop(agent, session_config, current_dir).await
}

/// Run the CLI in interactive mode with a specific session
pub async fn run_with_session(
    _config: &crate::config::AppConfig,
    current_dir: &Path,
    agent: Agent,
    setup: &AgentSetup,
    session_id: Option<&str>,
    last: bool,
    fork_from: Option<&str>,
) -> Result<()> {
    print_banner("Synthia CLI ", VERSION);
    print_tools(&setup.tool_names());
    println!();
    output::print_help();

    let session_config =
        crate::modes::resolve_session(&agent, session_id, last, fork_from)
            .await?;

    run_main_loop(agent, Some(session_config), current_dir).await
}

async fn run_main_loop(
    agent: Agent,
    session_config: Option<SessionConfig>,
    current_dir: &Path,
) -> Result<()> {
    let cancel_token = CancellationToken::new();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let question_sender = Arc::new(QuestionSenderImpl::new());
    let input_handler = InputHandler::new(event_tx, question_sender);
    let mut main_handler = MainLoopHandler::new(agent.clone(), session_config);

    // Start scheduler
    let _scheduler_handle =
        spawn_scheduler(&agent, current_dir, cancel_token.clone());

    // Start input handler
    let _input_handle =
        spawn_input_handler(input_handler, cancel_token.clone());

    // Start signal handler
    let _signal_handle = spawn_signal_handler(cancel_token.clone());

    // Run main loop
    main_handler.run(cancel_token.clone(), event_rx).await;

    // Cleanup
    cancel_token.cancel();
    Ok(())
}

fn spawn_scheduler(
    agent: &Agent,
    current_dir: &Path,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let agent = agent.clone();
    let current_dir = current_dir.to_path_buf();

    tokio::spawn(async move {
        if let Err(e) = scheduler::run(&agent, &current_dir, cancel_token).await
        {
            tracing::error!("Scheduler error: {}", e);
        }
    })
}

fn spawn_input_handler(
    mut input_handler: InputHandler,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        input_handler.run(cancel_token).await;
    })
}

fn spawn_signal_handler(
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        signal::ctrl_c().await.ok();
        cancel_token.cancel();
    })
}
