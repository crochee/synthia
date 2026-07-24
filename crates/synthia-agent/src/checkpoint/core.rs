//! `Checkpoint` struct: builder, save, rotate, and load mechanics.
//!
//! Checkpoints are written as pretty-printed JSON files named
//! `step_{N}.json` under `.agents/checkpoints/{session_id}/`. After each
//! [`Checkpoint::save`], old files are rotated so that at most
//! [`super::MAX_CHECKPOINTS`] files are retained per session.

use std::path::{Path, PathBuf};

use synthia_provider::types::Message;

use super::{
    AgentConfigSnapshot,
    CheckpointData,
    GuardianState,
    MAX_CHECKPOINTS,
    PendingToolCall,
};
use crate::types::TokenUsage;

/// Agent-level checkpoint that wraps the core checkpoint mechanics.
pub struct Checkpoint {
    session_id: String,
    checkpoint_dir: PathBuf,
    step: usize,
    messages: Vec<Message>,
    pending_tool_calls: Vec<PendingToolCall>,
    agent_config: AgentConfigSnapshot,
    guardian_state: GuardianState,
    token_usage: TokenUsage,
    iteration: usize,
}

impl Checkpoint {
    pub fn new(session_id: String, checkpoint_dir: PathBuf) -> Self {
        Self {
            session_id,
            checkpoint_dir,
            step: 0,
            messages: Vec::new(),
            pending_tool_calls: Vec::new(),
            agent_config: AgentConfigSnapshot::default(),
            guardian_state: GuardianState::default(),
            token_usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            iteration: 0,
        }
    }

    pub fn with_step(mut self, step: usize) -> Self {
        self.step = step;
        self
    }

    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    pub fn with_tool_calls(mut self, calls: Vec<PendingToolCall>) -> Self {
        self.pending_tool_calls = calls;
        self
    }

    pub fn with_config(mut self, config: AgentConfigSnapshot) -> Self {
        self.agent_config = config;
        self
    }

    pub fn with_guardian_state(mut self, state: GuardianState) -> Self {
        self.guardian_state = state;
        self
    }

    pub fn with_token_usage(mut self, usage: TokenUsage) -> Self {
        self.token_usage = usage;
        self
    }

    pub fn with_iteration(mut self, iteration: usize) -> Self {
        self.iteration = iteration;
        self
    }

    /// Save the checkpoint to `.agents/checkpoints/{session_id}/step_{N}.json`.
    ///
    /// Creates the checkpoint directory if it does not exist, serializes the
    /// checkpoint data as pretty-printed JSON, and rotates old checkpoints
    /// to enforce the maximum of 5.
    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.checkpoint_dir)?;
        let path = self.checkpoint_dir.join(format!("step_{}.json", self.step));
        let data = CheckpointData {
            session_id: self.session_id.clone(),
            step: self.step,
            timestamp: chrono::Utc::now().to_rfc3339(),
            messages: self.messages.clone(),
            pending_tool_calls: self.pending_tool_calls.clone(),
            agent_config: self.agent_config.clone(),
            guardian_state: self.guardian_state.clone(),
            token_usage: self.token_usage.clone(),
            iteration: self.iteration,
        };
        let json = serde_json::to_string_pretty(&data).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        std::fs::write(&path, json)?;

        // Rotate after each save
        Self::rotate(&self.checkpoint_dir, MAX_CHECKPOINTS)?;

        Ok(())
    }

    /// Delete the oldest checkpoint files to keep at most `max_count`.
    pub fn rotate(
        checkpoint_dir: &Path,
        max_count: usize,
    ) -> std::io::Result<()> {
        if !checkpoint_dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<_> = std::fs::read_dir(checkpoint_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    return None;
                }
                let step = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|stem| stem.strip_prefix("step_"))
                    .and_then(|num| num.parse::<usize>().ok())?;
                Some((step, path))
            })
            .collect();

        if entries.len() <= max_count {
            return Ok(());
        }

        entries.sort_by_key(|(step, _)| *step);
        let to_remove = entries.len() - max_count;

        for (_, path) in entries.iter().take(to_remove) {
            std::fs::remove_file(path)?;
        }

        Ok(())
    }

    /// Load the latest checkpoint for a given session.
    ///
    /// Reads all checkpoint files in the directory, finds the one with the
    /// highest step number, deserializes and returns it. Returns `None` if
    /// the directory does not exist or contains no checkpoints.
    pub fn load_latest(
        checkpoint_dir: &Path,
    ) -> std::io::Result<Option<CheckpointData>> {
        if !checkpoint_dir.exists() {
            return Ok(None);
        }

        let mut entries: Vec<_> = std::fs::read_dir(checkpoint_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    return None;
                }
                let step = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|stem| stem.strip_prefix("step_"))
                    .and_then(|num| num.parse::<usize>().ok())?;
                Some((step, path))
            })
            .collect();

        if entries.is_empty() {
            return Ok(None);
        }

        entries.sort_by_key(|(step, _)| *step);
        let (_, latest_path) = entries.last().unwrap();

        let json = std::fs::read_to_string(latest_path)?;
        let data: CheckpointData =
            serde_json::from_str(&json).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;

        Ok(Some(data))
    }

    /// Load the latest checkpoint for a specific session by scanning subdirectories.
    ///
    /// Scans the checkpoint root directory for a subdirectory matching the
    /// session ID, then loads the latest checkpoint from that subdirectory.
    pub fn load_latest_by_session(
        checkpoint_dir: &Path,
        session_id: &str,
    ) -> std::io::Result<Option<CheckpointData>> {
        let session_dir = checkpoint_dir.join(session_id);
        Self::load_latest(&session_dir)
    }
}
