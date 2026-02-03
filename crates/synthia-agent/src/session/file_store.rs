use std::path::PathBuf;

use async_trait::async_trait;
use chrono::Utc;
use rmcp::model::SamplingMessage;
use tokio::fs;

use crate::{
    AgentError,
    Result,
    Session,
    config::SessionConfig,
    session::SessionManager,
};

fn default_sessions_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agent")
        .join("sessions")
}

pub struct SessionFileStore {
    base_path: PathBuf,
}

impl SessionFileStore {
    pub fn new() -> Self {
        Self {
            base_path: default_sessions_dir(),
        }
    }

    pub fn with_base_path(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.base_path.join(format!("{session_id}.json"))
    }

    fn list_path(&self) -> PathBuf {
        self.base_path.join("sessions.json")
    }
}

impl Default for SessionFileStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionManager for SessionFileStore {
    async fn get_session(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Option<Session>> {
        let path = self.session_path(&session_config.id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path).await?;
        let session: Session = serde_json::from_str(&content)?;
        Ok(Some(session))
    }

    async fn create_session(&self) -> Result<Session> {
        let session = Session::default();
        let path = self.session_path(&session.id);
        if let Some(parent) = &session.parent_id {
            let parent_path = self.session_path(parent);
            if !parent_path.exists() {
                return Err(AgentError::session(format!(
                    "Parent session {parent} does not exist"
                )));
            }
        }
        fs::create_dir_all(self.base_path.clone()).await?;
        let content = serde_json::to_string_pretty(&session)?;
        fs::write(path, content).await?;
        let list_path = self.list_path();
        let mut sessions: Vec<String> = if list_path.exists() {
            let content = fs::read_to_string(&list_path).await?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            vec![]
        };
        if !sessions.contains(&session.id) {
            sessions.push(session.id.clone());
            fs::write(&list_path, serde_json::to_string_pretty(&sessions)?)
                .await?;
        }
        Ok(session)
    }

    async fn update_session(&self, session: &Session) -> Result<()> {
        let path = self.session_path(&session.id);
        let content = serde_json::to_string_pretty(&session)?;
        fs::write(path, content).await?;
        Ok(())
    }

    async fn delete_session(
        &self,
        session_config: &SessionConfig,
    ) -> Result<()> {
        let path = self.session_path(&session_config.id);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        let list_path = self.list_path();
        if list_path.exists() {
            let content = fs::read_to_string(&list_path).await?;
            let mut sessions: Vec<String> =
                serde_json::from_str(&content).unwrap_or_default();
            sessions.retain(|id| id != &session_config.id);
            fs::write(&list_path, serde_json::to_string_pretty(&sessions)?)
                .await?;
        }
        Ok(())
    }

    async fn add_message(
        &self,
        session_config: &SessionConfig,
        message: &SamplingMessage,
    ) -> Result<()> {
        let mut session = self
            .get_session(session_config)
            .await?
            .ok_or_else(|| AgentError::session(session_config.id.clone()))?;
        session.conversation.push(message.clone());
        session.updated_at = Utc::now().timestamp();
        self.update_session(&session).await
    }

    async fn get_conversation(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Vec<SamplingMessage>> {
        let session = self
            .get_session(session_config)
            .await?
            .ok_or_else(|| AgentError::session(session_config.id.clone()))?;
        Ok(session.conversation)
    }

    async fn get_recent_conversations(
        &self,
        limit: usize,
        _mark: Option<&str>,
    ) -> Result<(Vec<Session>, Option<String>, bool)> {
        let list_path = self.list_path();
        if !list_path.exists() {
            return Ok((vec![], None, false));
        }
        let content = fs::read_to_string(&list_path).await?;
        let session_ids: Vec<String> =
            serde_json::from_str(&content).unwrap_or_default();
        let mut sessions = Vec::new();
        for id in session_ids.iter().rev().take(limit * 2) {
            if let Ok(Some(session)) =
                self.get_session(&SessionConfig::new(id.clone())).await
            {
                sessions.push(session);
            }
        }
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let has_more = sessions.len() > limit;
        sessions.truncate(limit);
        let next_mark = sessions.last().map(|s| s.id.clone());
        Ok((sessions, next_mark, has_more))
    }

    async fn get_conversation_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<SamplingMessage>> {
        self.get_conversation(&SessionConfig::new(session_id.to_string()))
            .await
    }

    async fn replace_conversation(
        &self,
        session_config: &SessionConfig,
        conversation: &[SamplingMessage],
    ) -> Result<()> {
        let mut session = self
            .get_session(session_config)
            .await?
            .ok_or_else(|| AgentError::session(session_config.id.clone()))?;
        session.conversation = conversation.to_vec();
        session.updated_at = Utc::now().timestamp();
        self.update_session(&session).await
    }

    async fn fix_conversation(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Vec<SamplingMessage>> {
        let mut session = self
            .get_session(session_config)
            .await?
            .ok_or_else(|| AgentError::session(session_config.id.clone()))?;
        let (fixed_messages, _) =
            crate::utils::fix_conversation(session.conversation);
        session.conversation = fixed_messages.clone();
        session.updated_at = Utc::now().timestamp();
        self.update_session(&session).await?;
        Ok(fixed_messages)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn test_session_file_store_new() {
        let store = SessionFileStore::new();
        // Should create with default path (not checked for specific value)
        let _ = store;
    }

    #[tokio::test]
    async fn test_session_file_store_with_base_path() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());
        assert!(
            store.base_path.exists()
                || !store.base_path.to_string_lossy().is_empty()
        );
    }

    #[tokio::test]
    async fn test_session_path() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());
        let path = store.session_path("test-id");
        assert!(path.to_string_lossy().contains("test-id"));
    }

    #[tokio::test]
    async fn test_list_path() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());
        let path = store.list_path();
        assert!(path.to_string_lossy().contains("sessions.json"));
    }

    #[tokio::test]
    async fn test_create_and_get_session() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        assert!(!session.id.is_empty());

        let config = SessionConfig::new(session.id.clone());
        let retrieved = store.get_session(&config).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, session.id);
    }

    #[tokio::test]
    async fn test_get_nonexistent_session() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let config = SessionConfig::new("nonexistent-id".to_string());
        let result = store.get_session(&config).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_session() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let mut session = store.create_session().await.unwrap();
        session.name = Some("Updated Name".to_string());

        store.update_session(&session).await.unwrap();

        let config = SessionConfig::new(session.id.clone());
        let retrieved = store.get_session(&config).await.unwrap().unwrap();
        assert_eq!(retrieved.name, Some("Updated Name".to_string()));
    }

    #[tokio::test]
    async fn test_delete_session() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        let config = SessionConfig::new(session.id.clone());

        store.delete_session(&config).await.unwrap();

        let result = store.get_session(&config).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_add_message() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        let config = SessionConfig::new(session.id.clone());

        let message = rmcp::model::SamplingMessage::user_text("Hello");
        store.add_message(&config, &message).await.unwrap();

        let retrieved = store.get_session(&config).await.unwrap().unwrap();
        assert_eq!(retrieved.conversation.len(), 1);
    }

    #[tokio::test]
    async fn test_get_conversation() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        let config = SessionConfig::new(session.id.clone());

        let msg1 = rmcp::model::SamplingMessage::user_text("Hello");
        let msg2 = rmcp::model::SamplingMessage::user_text("World");
        store.add_message(&config, &msg1).await.unwrap();
        store.add_message(&config, &msg2).await.unwrap();

        let conversation = store.get_conversation(&config).await.unwrap();
        assert_eq!(conversation.len(), 2);
    }

    #[tokio::test]
    async fn test_replace_conversation() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        let config = SessionConfig::new(session.id.clone());

        let new_conversation = vec![
            rmcp::model::SamplingMessage::user_text("New"),
            rmcp::model::SamplingMessage::user_text("Conversation"),
        ];

        store
            .replace_conversation(&config, &new_conversation)
            .await
            .unwrap();

        let conversation = store.get_conversation(&config).await.unwrap();
        assert_eq!(conversation.len(), 2);
    }

    #[tokio::test]
    async fn test_get_recent_conversations() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        // Create multiple sessions
        let _s1 = store.create_session().await.unwrap();
        let _s2 = store.create_session().await.unwrap();

        let (sessions, next_mark, has_more) =
            store.get_recent_conversations(10, None).await.unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(!has_more);
        assert!(next_mark.is_some());
    }

    #[tokio::test]
    async fn test_get_conversation_messages() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        let config = SessionConfig::new(session.id.clone());

        let msg = rmcp::model::SamplingMessage::user_text("Test");
        store.add_message(&config, &msg).await.unwrap();

        let messages =
            store.get_conversation_messages(&session.id).await.unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_fix_conversation() {
        let temp = tempdir().unwrap();
        let store = SessionFileStore::with_base_path(temp.path().to_path_buf());

        let session = store.create_session().await.unwrap();
        let config = SessionConfig::new(session.id.clone());

        let msg = rmcp::model::SamplingMessage::user_text("Test");
        store.add_message(&config, &msg).await.unwrap();

        let fixed = store.fix_conversation(&config).await.unwrap();
        assert_eq!(fixed.len(), 1);
    }
}
