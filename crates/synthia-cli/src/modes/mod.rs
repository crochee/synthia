//! CLI execution modes
//!
//! This module provides different execution modes for the CLI:
//! - Interactive: Interactive chat mode with user input
//! - NonInteractive: Single query execution mode for automation

pub mod interactive;
pub mod non_interactive;

use std::path::Path;

use anyhow::{Context, Result};
use synthia_agent::{Agent, config::SessionConfig};

use crate::config::AppConfig;

/// Build agent and create session for non-interactive mode
pub async fn setup_agent_and_session(
    config: &AppConfig,
    current_dir: &Path,
    session_id: Option<&str>,
) -> Result<(Agent, SessionConfig)> {
    let (agent, _) =
        crate::agent::build_agent(config, current_dir, None).await?;
    let session_config = create_session(&agent, session_id).await?;
    Ok((agent, session_config))
}

/// Create a new session for the agent
async fn create_session(
    agent: &Agent,
    session_id: Option<&str>,
) -> Result<SessionConfig> {
    let session = match session_id {
        Some(id) => {
            let config = SessionConfig::new(id);
            match agent.deps.session.get_session(&config).await? {
                Some(session) => session,
                None => agent.deps.session.create_session().await?,
            }
        }
        None => agent.deps.session.create_session().await?,
    };
    Ok(SessionConfig::from(session))
}

/// Resolve session for Resume/Fork commands
pub async fn resolve_session(
    agent: &Agent,
    session_id: Option<&str>,
    last: bool,
    fork_from: Option<&str>,
) -> Result<SessionConfig> {
    // If forking, we need a source session
    if fork_from.is_some() {
        let source_id = resolve_session_id(agent, session_id, last)
            .await
            .context("Failed to resolve source session for fork")?;
        let source_config = SessionConfig::new(&source_id);
        let _source_session = agent
            .deps
            .session
            .get_session(&source_config)
            .await?
            .context("Source session not found")?;

        // Create new session with parent set to source
        let new_session = agent.deps.session.create_session().await?;
        let mut new_config = SessionConfig::from(new_session);
        new_config.parent_id = Some(source_id);
        return Ok(new_config);
    }

    // For resume, just get the session
    let resolved_id = resolve_session_id(agent, session_id, last).await?;
    let config = SessionConfig::new(&resolved_id);
    let session = agent
        .deps
        .session
        .get_session(&config)
        .await?
        .context("Session not found");
    Ok(SessionConfig::from(session?))
}

/// Resolve session ID from arguments or interactive picker
async fn resolve_session_id(
    agent: &Agent,
    session_id: Option<&str>,
    last: bool,
) -> Result<String> {
    if let Some(id) = session_id {
        return Ok(id.to_string());
    }

    if last {
        let (sessions, _, _) =
            agent.deps.session.get_recent_conversations(1, None).await?;
        if let Some(session) = sessions.first() {
            return Ok(session.id.clone());
        }
        anyhow::bail!("No sessions found");
    }

    // Show interactive picker
    let (sessions, _, _) = agent
        .deps
        .session
        .get_recent_conversations(10, None)
        .await?;
    if sessions.is_empty() {
        anyhow::bail!("No sessions found");
    }

    println!("Select a session:");
    for (i, session) in sessions.iter().enumerate() {
        let name = session.name.as_deref().unwrap_or("Unnamed");
        println!("  {}: {} (id: {})", i + 1, name, session.id);
    }
    println!();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let choice: usize = input.trim().parse().context("Invalid selection")?;
    if choice == 0 || choice > sessions.len() {
        anyhow::bail!("Invalid selection");
    }

    Ok(sessions[choice - 1].id.clone())
}
