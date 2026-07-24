use std::path::PathBuf;

use super::types::{CHECKPOINT_MAX_COUNT, Checkpoint, CheckpointData};
use crate::types::ContextError;

impl Checkpoint {
    pub fn new(session_id: String, checkpoint_dir: PathBuf) -> Self {
        Self {
            session_id,
            checkpoint_dir,
            step: 0,
            guardian_state: super::types::GuardianState::default(),
            messages: Vec::new(),
            pending_tool_calls: Vec::new(),
            config_snapshot: super::types::AgentConfigSnapshot::default(),
            token_usage: synthia_provider::types::TokenUsage::default(),
            iteration_count: 0,
        }
    }

    pub fn with_step(mut self, step: usize) -> Self {
        self.step = step;
        self
    }

    pub fn with_messages(
        mut self,
        messages: Vec<synthia_provider::types::Message>,
    ) -> Self {
        self.messages = messages;
        self
    }

    pub fn with_iteration(mut self, iteration: usize) -> Self {
        self.iteration_count = iteration;
        self
    }

    pub fn with_guardian_state(
        mut self,
        state: super::types::GuardianState,
    ) -> Self {
        self.guardian_state = state;
        self
    }

    pub fn with_tool_calls(
        mut self,
        calls: Vec<super::types::PendingToolCall>,
    ) -> Self {
        self.pending_tool_calls = calls;
        self
    }

    pub fn save(&self) -> Result<(), ContextError> {
        std::fs::create_dir_all(&self.checkpoint_dir)?;
        let path = self.checkpoint_dir.join(format!("step_{}.json", self.step));
        let data = CheckpointData {
            session_id: self.session_id.clone(),
            step: self.step,
            timestamp: chrono::Utc::now().to_rfc3339(),
            messages: self.messages.clone(),
            pending_tool_calls: self.pending_tool_calls.clone(),
            config_snapshot: self.config_snapshot.clone(),
            guardian_state: self.guardian_state.clone(),
            token_usage: self.token_usage.clone(),
            iteration_count: self.iteration_count,
        };
        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(&path, json)?;
        self.rotate(CHECKPOINT_MAX_COUNT)?;
        Ok(())
    }

    pub fn load_latest_by_session(
        checkpoint_dir: PathBuf,
        session_id: &str,
    ) -> Result<Option<Self>, ContextError> {
        if !checkpoint_dir.exists() {
            return Ok(None);
        }
        let entries: Vec<_> = std::fs::read_dir(&checkpoint_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("json")
            })
            .collect();

        if entries.is_empty() {
            return Ok(None);
        }

        let latest = entries
            .into_iter()
            .filter_map(|e| {
                let path = e.path();
                let step = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|stem| stem.strip_prefix("step_"))
                    .and_then(|num| num.parse::<usize>().ok())?;
                Some((step, e))
            })
            .max_by_key(|(step, _)| *step);

        if let Some((step, _entry)) = latest {
            let cp = Checkpoint::load(checkpoint_dir, step)?;
            if cp.session_id == session_id || session_id.is_empty() {
                Ok(Some(cp))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn advance_step(&mut self) {
        self.step += 1;
    }

    pub fn update_messages(
        &mut self,
        messages: Vec<synthia_provider::types::Message>,
    ) {
        self.messages = messages;
    }

    pub fn update_iteration(&mut self, iteration: usize) {
        self.iteration_count = iteration;
    }

    pub fn load(
        checkpoint_dir: PathBuf,
        step: usize,
    ) -> Result<Self, ContextError> {
        let path = checkpoint_dir.join(format!("step_{}.json", step));
        let data = std::fs::read_to_string(&path)?;
        let checkpoint_data: CheckpointData = serde_json::from_str(&data)?;
        Ok(Checkpoint {
            session_id: checkpoint_data.session_id,
            checkpoint_dir,
            step: checkpoint_data.step,
            guardian_state: checkpoint_data.guardian_state,
            messages: checkpoint_data.messages,
            pending_tool_calls: checkpoint_data.pending_tool_calls,
            config_snapshot: checkpoint_data.config_snapshot,
            token_usage: checkpoint_data.token_usage,
            iteration_count: checkpoint_data.iteration_count,
        })
    }

    pub fn load_latest(
        checkpoint_dir: PathBuf,
    ) -> Result<Option<Self>, ContextError> {
        if !checkpoint_dir.exists() {
            return Ok(None);
        }
        let entries: Vec<_> = std::fs::read_dir(&checkpoint_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("json")
            })
            .collect();

        if entries.is_empty() {
            return Ok(None);
        }

        let latest = entries.into_iter().max_by_key(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.strip_prefix("step_"))
                .and_then(|num| num.parse::<usize>().ok())
                .unwrap_or(0)
        });

        if let Some(entry) = latest {
            let step = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.strip_prefix("step_"))
                .and_then(|num| num.parse::<usize>().ok())
                .unwrap_or(0);
            let checkpoint = Checkpoint::load(checkpoint_dir, step)?;
            Ok(Some(checkpoint))
        } else {
            Ok(None)
        }
    }

    pub fn rotate(&self, max_checkpoints: usize) -> Result<(), ContextError> {
        let entries: Vec<_> = std::fs::read_dir(&self.checkpoint_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("json")
            })
            .collect();

        let to_remove = entries.len().saturating_sub(max_checkpoints);
        if to_remove > 0 {
            let mut sorted: Vec<_> = entries
                .into_iter()
                .filter_map(|e| {
                    let step = e
                        .path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|stem| stem.strip_prefix("step_"))
                        .and_then(|num| num.parse::<usize>().ok())?;
                    Some((step, e))
                })
                .collect();
            sorted.sort_by_key(|(step, _)| *step);

            for i in 0..to_remove {
                if let Some((_, entry)) = sorted.get(i) {
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }
}
