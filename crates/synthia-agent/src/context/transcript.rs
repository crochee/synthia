//! Transcript persistence for conversation history
//!
//! This module provides functionality to save full conversation transcripts
//! to disk before compaction, enabling conversation recovery.

use std::{io::Write, path::PathBuf};

use chrono::Utc;
use rmcp::model::SamplingMessage;

const TRANSCRIPT_DIR_NAME: &str = ".agents/transcripts";

/// Transcript persistence manager
#[derive(Debug, Clone)]
pub struct TranscriptManager {
    workspace_dir: PathBuf,
}

impl TranscriptManager {
    /// Create a new TranscriptManager
    pub(crate) fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }

    /// Get the transcript directory path
    pub(crate) fn transcript_dir(&self) -> PathBuf {
        self.workspace_dir.join(TRANSCRIPT_DIR_NAME)
    }

    /// Save transcript synchronously (for use in non-async contexts)
    pub(crate) fn save_transcript_sync(
        &self,
        session_id: &str,
        messages: &[SamplingMessage],
    ) -> std::io::Result<PathBuf> {
        let transcript_dir = self.transcript_dir();
        std::fs::create_dir_all(&transcript_dir)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("transcript_{session_id}_{timestamp}.jsonl");
        let transcript_path = transcript_dir.join(&filename);

        let mut file = std::fs::File::create(&transcript_path)?;

        for msg in messages {
            let serialized = serde_json::to_string(msg)?;
            writeln!(file, "{serialized}")?;
        }

        tracing::info!(
            "Saved transcript to {} ({} messages)",
            transcript_path.display(),
            messages.len()
        );

        Ok(transcript_path)
    }
}

impl Default for TranscriptManager {
    fn default() -> Self {
        Self {
            workspace_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transcript_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TranscriptManager::new(temp_dir.path().to_path_buf());

        let messages = vec![
            SamplingMessage::user_text("Hello"),
            SamplingMessage::assistant_text("Hi there!"),
        ];

        let path = manager
            .save_transcript_sync("test-session", &messages)
            .unwrap();

        assert!(path.exists());

        tokio::fs::remove_dir_all(temp_dir.path()).await.ok();
    }

    #[test]
    fn test_transcript_save_sync_only() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TranscriptManager::new(temp_dir.path().to_path_buf());

        let messages = vec![
            SamplingMessage::user_text("Hello"),
            SamplingMessage::assistant_text("Hi there!"),
        ];

        let path = manager
            .save_transcript_sync("test-session", &messages)
            .unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_transcript_dir_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TranscriptManager::new(temp_dir.path().to_path_buf());

        let transcript_dir = manager.transcript_dir();

        assert_eq!(transcript_dir, temp_dir.path().join(TRANSCRIPT_DIR_NAME));
    }

    #[test]
    fn test_transcript_filename_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TranscriptManager::new(temp_dir.path().to_path_buf());

        let messages = vec![SamplingMessage::user_text("Test")];

        let path = manager
            .save_transcript_sync("session-123", &messages)
            .unwrap();

        let filename = path.file_name().unwrap().to_str().unwrap();

        // Filename should match pattern: transcript_{session_id}_{timestamp}.jsonl
        assert!(filename.starts_with("transcript_session-123_"));
        assert!(filename.ends_with(".jsonl"));

        // Timestamp format: YYYYMMDD_HHMMSS (14 chars for timestamp + underscores)
        // transcript_session-123_YYYYMMDD_HHMMSS.jsonl
        let after_prefix =
            filename.strip_prefix("transcript_session-123_").unwrap();
        let before_ext = after_prefix.strip_suffix(".jsonl").unwrap();
        // Should be YYYYMMDD_HHMMSS (15 characters)
        assert_eq!(before_ext.len(), 15);
        assert!(before_ext.chars().all(|c| c.is_ascii_digit() || c == '_'));
    }

    #[test]
    fn test_transcript_empty_messages() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TranscriptManager::new(temp_dir.path().to_path_buf());

        let messages: Vec<SamplingMessage> = vec![];

        let path = manager
            .save_transcript_sync("empty-session", &messages)
            .unwrap();

        assert!(path.exists());
        // File should be empty (just newline from writeln! on empty vec)
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_transcript_multiple_messages() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TranscriptManager::new(temp_dir.path().to_path_buf());

        let messages = vec![
            SamplingMessage::user_text("First"),
            SamplingMessage::assistant_text("Second"),
            SamplingMessage::user_text("Third"),
        ];

        let path = manager
            .save_transcript_sync("multi-msg", &messages)
            .unwrap();

        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().count(), 3);
    }

    #[test]
    fn test_transcript_manager_default() {
        let manager = TranscriptManager::default();
        // Default should use current directory
        let transcript_dir = manager.transcript_dir();
        assert!(transcript_dir.ends_with(TRANSCRIPT_DIR_NAME));
    }
}
