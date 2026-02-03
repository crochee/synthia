use synthia_agent::{Agent, config::SessionConfig};
use tracing::{info, instrument};

use crate::output::{print_error, print_info};

#[instrument(skip(agent, session_config))]
pub async fn handle_clear(agent: &Agent, session_config: &SessionConfig) {
    if let Err(e) = agent
        .deps
        .session
        .replace_conversation(session_config, &[])
        .await
    {
        print_error(&e);
    } else {
        print_info("Conversation history cleared.");
    }
}

#[instrument(skip(agent, session_config))]
pub async fn handle_compact(agent: &mut Agent, session_config: &SessionConfig) {
    match agent.deps.session.fix_conversation(session_config).await {
        Ok(conversation) => {
            match agent.deps.context.compact(&conversation).await {
                Ok(Some(result)) => {
                    info!("Compaction reason: {}", result.reason);
                    if let Err(e) = agent
                        .deps
                        .session
                        .replace_conversation(session_config, &result.messages)
                        .await
                    {
                        print_error(&e);
                    } else {
                        print_info("Conversation history compacted.");
                    }
                }
                Ok(None) => {
                    print_info(
                        "Conversation history does not need compaction.",
                    );
                }
                Err(e) => {
                    print_error(&e);
                }
            }
        }
        Err(e) => {
            print_error(&e);
        }
    }
}

#[instrument(skip(agent, session_config))]
pub async fn handle_session_new(
    agent: &Agent,
    session_config: &mut Option<SessionConfig>,
) {
    match agent.deps.session.create_session().await {
        Ok(session) => {
            let config = SessionConfig::from(session);
            print_info(&format!(
                "Created and switched to new session: {}",
                config.id
            ));
            *session_config = Some(config);
        }
        Err(e) => {
            print_error(&e);
        }
    }
}

#[instrument(skip(agent, session_config))]
pub async fn handle_session_switch(
    agent: &Agent,
    session_config: &mut Option<SessionConfig>,
    target_id: &str,
) {
    let target_config = SessionConfig::new(target_id);
    match agent.deps.session.get_session(&target_config).await {
        Ok(Some(session)) => {
            let config = SessionConfig::from(session);
            print_info(&format!("Switched to session: {}", config.id));
            *session_config = Some(config);
        }
        Ok(None) => {
            print_error(&format!("Session '{}' not found", target_id));
        }
        Err(e) => {
            print_error(&e);
        }
    }
}

#[instrument(skip(agent))]
pub async fn handle_session_list(agent: &Agent) {
    match agent.deps.session.get_recent_conversations(100, None).await {
        Ok((sessions, _, _)) => {
            if sessions.is_empty() {
                println!("No sessions found.");
                return;
            }
            println!("Available sessions:");
            for session in sessions {
                let name_display = session
                    .name
                    .as_ref()
                    .map(|n| format!(" ({})", n))
                    .unwrap_or_default();
                println!(
                    "  - {}{} [{} messages]",
                    session.id,
                    name_display,
                    session.conversation.len()
                );
            }
        }
        Err(e) => {
            print_error(&e);
        }
    }
}

pub fn handle_help() {
    crate::output::print_help();
}

pub async fn handle_quit() -> bool {
    println!("Goodbye!");
    true
}

pub fn handle_reasoning(output_config: &mut crate::output::OutputConfig) {
    output_config.show_reasoning = !output_config.show_reasoning;
    print_info(&format!(
        "Reasoning display: {}",
        if output_config.show_reasoning {
            "on"
        } else {
            "off"
        }
    ));
}

pub fn handle_speed(
    speed: Option<u64>,
    output_config: &mut crate::output::OutputConfig,
) {
    if let Some(ms) = speed {
        output_config.typing_delay_ms = ms;
        print_info(&format!("Typing speed set to {}ms", ms));
    } else {
        print_info(&format!(
            "Current typing speed: {}ms",
            output_config.typing_delay_ms
        ));
    }
}
