//! Chat service for handling conversation logic

use std::sync::Arc;

use synthia_agent::config::SessionConfig;

use crate::{AppState, error::ServerError, utils::create_user_message};

pub struct ChatService {
    state: Arc<AppState>,
}

impl ChatService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn get_or_create_session(
        &self,
        session_id: Option<String>,
    ) -> Result<String, ServerError> {
        match session_id {
            Some(id) => Ok(id),
            None => {
                let session =
                    self.state.agent.deps.session.create_session().await?;
                Ok(session.id)
            }
        }
    }

    pub fn create_user_message(
        &self,
        text: String,
    ) -> rmcp::model::SamplingMessage {
        create_user_message(text)
    }

    pub fn create_session_config(&self, session_id: String) -> SessionConfig {
        SessionConfig::new(session_id)
    }

    pub async fn add_message(
        &self,
        session_id: &str,
        message: &rmcp::model::SamplingMessage,
    ) -> Result<(), ServerError> {
        let config = SessionConfig::new(session_id.to_string());
        self.state
            .agent
            .deps
            .session
            .add_message(&config, message)
            .await?;
        Ok(())
    }
}
