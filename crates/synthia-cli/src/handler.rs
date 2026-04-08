//! Main loop handler module
//!
//! This module provides the main loop handler that processes input events.

use futures::StreamExt;
use rmcp::model::{Role, SamplingMessage, SamplingMessageContent};
use synthia_agent::{Agent, config::SessionConfig};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    color::colors,
    commands::{
        handle_clear,
        handle_compact,
        handle_help,
        handle_quit,
        handle_reasoning,
        handle_session_list,
        handle_session_new,
        handle_session_switch,
        handle_speed,
    },
    input::{InputCommand, InputEvent, MediaAttachment, media::MediaType},
    output::{OutputConfig, handle_agent_event, print_error, print_info},
};

/// Message shown when no session is active
const NO_SESSION_MSG: &str = "No active session. Type a message to create one.";

pub struct MainLoopHandler {
    agent: Agent,
    output_config: OutputConfig,
    session_config: Option<SessionConfig>,
}

impl MainLoopHandler {
    pub fn new(agent: Agent, session_config: Option<SessionConfig>) -> Self {
        Self {
            agent,
            output_config: OutputConfig::default(),
            session_config,
        }
    }

    /// Execute a handler that requires an immutable session reference
    async fn with_session(
        &self,
        handler: impl AsyncFnOnce(&Agent, &SessionConfig),
        action: &str,
    ) {
        match &self.session_config {
            Some(config) => handler(&self.agent, config).await,
            None => {
                eprintln!(
                    "{}Cannot {}: {}{}",
                    colors::YELLOW,
                    action,
                    NO_SESSION_MSG,
                    colors::RESET
                );
            }
        }
    }

    /// Execute a handler that requires a mutable session reference
    async fn with_session_mut(
        &mut self,
        handler: impl AsyncFnOnce(&mut Agent, &SessionConfig),
        action: &str,
    ) {
        match &self.session_config {
            Some(config) => handler(&mut self.agent, config).await,
            None => {
                eprintln!(
                    "{}Cannot {}: {}{}",
                    colors::YELLOW,
                    action,
                    NO_SESSION_MSG,
                    colors::RESET
                );
            }
        }
    }

    pub async fn run(
        &mut self,
        cancel_token: CancellationToken,
        mut event_rx: mpsc::UnboundedReceiver<InputEvent>,
    ) {
        while let Some(event) = event_rx.recv().await {
            match event {
                InputEvent::MultimodalInput { text, attachments } => {
                    self.process_user_input(
                        cancel_token.clone(),
                        text,
                        attachments,
                    )
                    .await;
                }
                InputEvent::Command(cmd) => {
                    if self.process_command(cmd).await {
                        break;
                    }
                }
            }
        }
    }

    #[instrument(skip(self, text, attachments, cancel_token))]
    async fn process_user_input(
        &mut self,
        cancel_token: CancellationToken,
        text: String,
        attachments: Vec<MediaAttachment>,
    ) {
        // Ensure session exists before processing user input
        if self.session_config.is_none() {
            match self.agent.deps.session.create_session().await {
                Ok(session) => {
                    self.session_config = Some(SessionConfig::from(session));
                    print_info("Created new session");
                }
                Err(e) => {
                    print_error(&e);
                    return;
                }
            }
        }

        let session_config = self.session_config.as_ref().unwrap();
        let user_msg = self.build_user_message(&text, attachments);

        let event_stream = match self
            .agent
            .reply(user_msg, session_config, cancel_token)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                print_error(&e);
                tracing::error!("Error during chat: {}", e);
                return;
            }
        };

        print!("{}Assistant{} > ", colors::CYAN, colors::RESET);

        let config_clone = self.output_config.clone();
        tokio::pin!(event_stream);
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    handle_agent_event(
                        &synthia_agent::config::AgentName::Custom(
                            "main".to_string(),
                        ),
                        &event,
                        &config_clone,
                    )
                    .await
                }
                Err(e) => {
                    print_error(&e);
                    tracing::error!("Error during chat: {}", e);
                }
            }
        }
        println!();
    }

    fn build_user_message(
        &self,
        text: &str,
        attachments: Vec<MediaAttachment>,
    ) -> SamplingMessage {
        let mut contents = Vec::new();

        if !text.is_empty() {
            contents.push(SamplingMessageContent::Text(
                rmcp::model::RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            ));
        }

        for attachment in attachments {
            let content = Self::convert_attachment(attachment);
            contents.push(content);
        }

        SamplingMessage {
            role: Role::User,
            content: rmcp::model::SamplingContent::Multiple(contents),
            meta: None,
        }
    }

    fn convert_attachment(
        attachment: MediaAttachment,
    ) -> SamplingMessageContent {
        match attachment.media_type {
            MediaType::Image => {
                SamplingMessageContent::Image(rmcp::model::RawImageContent {
                    mime_type: attachment.mime_type,
                    data: attachment.data_url,
                    meta: None,
                })
            }
            MediaType::Audio => {
                SamplingMessageContent::Audio(rmcp::model::RawAudioContent {
                    mime_type: attachment.mime_type,
                    data: attachment.data_url,
                })
            }
            MediaType::Video | MediaType::Pdf | MediaType::Unknown => {
                SamplingMessageContent::Text(rmcp::model::RawTextContent {
                    text: format!(
                        "[Attachment: {} - {}]",
                        attachment
                            .source_path
                            .as_ref()
                            .unwrap_or(&"unknown".to_string()),
                        attachment.mime_type
                    ),
                    meta: None,
                })
            }
        }
    }

    async fn process_command(&mut self, cmd: InputCommand) -> bool {
        match cmd {
            InputCommand::Quit => handle_quit().await,
            InputCommand::Help => {
                handle_help();
                false
            }
            InputCommand::Clear => {
                self.with_session(handle_clear, "clear conversation").await;
                false
            }
            InputCommand::Compact => {
                self.with_session_mut(handle_compact, "compact conversation")
                    .await;
                false
            }
            InputCommand::SessionNew => {
                handle_session_new(&self.agent, &mut self.session_config).await;
                false
            }
            InputCommand::SessionSwitch(id) => {
                handle_session_switch(
                    &self.agent,
                    &mut self.session_config,
                    &id,
                )
                .await;
                false
            }
            InputCommand::SessionList => {
                handle_session_list(&self.agent).await;
                false
            }
            InputCommand::Reasoning => {
                handle_reasoning(&mut self.output_config);
                false
            }
            InputCommand::Speed(speed) => {
                handle_speed(Some(speed), &mut self.output_config);
                false
            }
            InputCommand::Attach(_) => {
                println!(
                    "{}Use /attach <path> in input mode{}",
                    colors::YELLOW,
                    colors::RESET
                );
                false
            }
            InputCommand::Attachments => {
                println!(
                    "{}Use /attachments in input mode{}",
                    colors::YELLOW,
                    colors::RESET
                );
                false
            }
            InputCommand::ClearAttachments => {
                println!(
                    "{}Use /clear-attachments in input mode{}",
                    colors::YELLOW,
                    colors::RESET
                );
                false
            }
            InputCommand::Export(format) => {
                println!(
                    "{}Export to {} format not yet implemented{}",
                    colors::YELLOW,
                    format,
                    colors::RESET
                );
                false
            }
            InputCommand::Token => {
                println!(
                    "{}Token info not yet implemented{}",
                    colors::YELLOW,
                    colors::RESET
                );
                false
            }
            InputCommand::History(count) => {
                let history_count = count.unwrap_or(10);
                println!(
                    "{}Last {} messages:{}",
                    colors::CYAN,
                    history_count,
                    colors::RESET
                );
                false
            }
        }
    }
}
