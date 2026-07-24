//! Session service for session management logic

use std::sync::Arc;

use synthia_agent::config::SessionConfig;

use super::types::{CompactionResult, FormattedMessage, SessionInfo};
use crate::{
    AppState,
    error::ServerError,
    utils::{extract_text, format_role},
};

pub struct SessionService {
    state: Arc<AppState>,
}

impl SessionService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn create(&self) -> Result<SessionInfo, ServerError> {
        let session = self.state.agent.deps.session.create_session().await?;
        Ok(SessionInfo {
            id: session.id,
            name: session.name,
            created_at: session.created_at,
            updated_at: Some(session.updated_at),
            message_count: Some(session.conversation.len()),
        })
    }

    pub async fn get(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionInfo>, ServerError> {
        let config = SessionConfig::new(session_id.to_string());
        let session =
            self.state.agent.deps.session.get_session(&config).await?;

        Ok(session.map(|s| SessionInfo {
            id: s.id,
            name: s.name,
            created_at: s.created_at,
            updated_at: Some(s.updated_at),
            message_count: Some(s.conversation.len()),
        }))
    }

    pub async fn delete(&self, session_id: &str) -> Result<bool, ServerError> {
        let config = SessionConfig::new(session_id.to_string());
        self.state
            .agent
            .deps
            .session
            .delete_session(&config)
            .await?;
        Ok(true)
    }

    pub async fn list(
        &self,
        limit: usize,
        mark: Option<&str>,
    ) -> Result<(Vec<SessionInfo>, Option<String>, bool), ServerError> {
        let (sessions, next_mark, has_more) = self
            .state
            .agent
            .deps
            .session
            .get_recent_conversations(limit, mark)
            .await?;

        let session_infos: Vec<SessionInfo> = sessions
            .into_iter()
            .map(|s| SessionInfo {
                id: s.id,
                name: s.name,
                created_at: s.created_at,
                updated_at: Some(s.updated_at),
                message_count: Some(s.conversation.len()),
            })
            .collect();

        Ok((session_infos, next_mark, has_more))
    }

    pub async fn get_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<FormattedMessage>, ServerError> {
        let config = SessionConfig::new(session_id.to_string());
        let messages = self
            .state
            .agent
            .deps
            .session
            .get_conversation(&config)
            .await?;

        Ok(messages
            .iter()
            .map(|msg| FormattedMessage {
                role: format_role(&msg.role).to_string(),
                content: extract_text(msg).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn compact(
        &self,
        session_id: &str,
    ) -> Result<CompactionResult, ServerError> {
        let config = SessionConfig::new(session_id.to_string());
        let messages = self
            .state
            .agent
            .deps
            .session
            .get_conversation(&config)
            .await?;
        let before_count = messages.len();

        let result = self.state.agent.deps.context.compact(&messages).await?;

        match result {
            Some(compaction) => {
                self.state
                    .agent
                    .deps
                    .session
                    .replace_conversation(&config, &compaction.messages)
                    .await?;

                Ok(CompactionResult {
                    before_count,
                    after_count: compaction.messages.len(),
                    strategy: format!("{:?}", compaction.metadata.strategy),
                    token_ratio_before: compaction.metadata.usage_ratio_before,
                    token_ratio_after: compaction.metadata.usage_ratio_after,
                })
            }
            None => Ok(CompactionResult {
                before_count,
                after_count: before_count,
                strategy: "None".to_string(),
                token_ratio_before: 0.0,
                token_ratio_after: 0.0,
            }),
        }
    }
}
