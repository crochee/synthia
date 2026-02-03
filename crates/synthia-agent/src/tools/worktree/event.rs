//! Worktree event bus for lifecycle tracking

use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorktreeEvent {
    pub event: String,
    pub ts: i64,
    pub task: Option<serde_json::Value>,
    pub worktree: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub(crate) struct WorktreeEventBus {
    path: PathBuf,
}

impl WorktreeEventBus {
    pub(crate) fn new(worktrees_dir: PathBuf) -> Self {
        let path = worktrees_dir.join("events.jsonl");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        Self { path }
    }

    pub(crate) fn emit(
        &self,
        event: &str,
        task: Option<serde_json::Value>,
        worktree: Option<serde_json::Value>,
        error: Option<String>,
    ) -> std::io::Result<()> {
        let event = WorktreeEvent {
            event: event.to_string(),
            ts: Utc::now().timestamp(),
            task,
            worktree,
            error,
        };

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        use std::io::Write;
        writeln!(file, "{}", serde_json::to_string(&event)?)?;
        Ok(())
    }

    pub(crate) fn list_recent(&self, limit: usize) -> std::io::Result<String> {
        if !self.path.exists() {
            return Ok("[]".to_string());
        }

        let content = std::fs::read_to_string(&self.path)?;
        let lines: Vec<&str> = content.lines().rev().take(limit).collect();

        let mut events = Vec::new();
        for line in lines.iter().rev() {
            if let Ok(event) = serde_json::from_str::<WorktreeEvent>(line) {
                events.push(event);
            }
        }

        Ok(serde_json::to_string_pretty(&events)
            .unwrap_or_else(|_| "[]".to_string()))
    }
}
